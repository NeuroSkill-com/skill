// SPDX-License-Identifier: GPL-3.0-only
//! Session finalizer for the PTY shim.
//!
//! Periodically scans `~/.skill/terminal-logs/` for `.log` files belonging to
//! shims that have already exited, then for each one:
//!
//! 1. Loads the `.idx` timing sidecar (16 bytes per PTY-write batch).
//! 2. Joins overlapping rows from `terminal_commands` (by time range).
//! 3. For every command, binary-searches the index to find the byte slice
//!    of the log that belongs to it, ANSI-strips it, zstd-compresses the
//!    raw bytes, and writes one row to `terminal_outputs`.
//! 4. After all commands in the file are processed, deletes the `.log` and
//!    `.idx` (the SQLite row is now the canonical record).
//!
//! This runs as a tokio background task. Heavy work (file I/O,
//! compression, ANSI strip) lives on `spawn_blocking` so it doesn't stall
//! the runtime.

use std::path::{Path, PathBuf};
use std::time::Duration;

use skill_daemon_state::AppState;
use tracing::{debug, warn};

const SCAN_INTERVAL: Duration = Duration::from_secs(60);
/// Max raw output to store per command; longer outputs get truncated. 256 KB
/// covers virtually every interactive use; pathological cases (`cat huge.log`)
/// are deliberately capped so a single command can't bloat the DB.
const MAX_RAW_BYTES_PER_COMMAND: u64 = 256 * 1024;
/// Cap stripped text similarly so FTS rows stay small.
const MAX_STRIPPED_CHARS_PER_COMMAND: usize = 64 * 1024;

/// Spawn the finalizer loop. Runs once a minute; cheap if there's nothing to do.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(SCAN_INTERVAL).await;
            let state = state.clone();
            let _ = tokio::task::spawn_blocking(move || run_once(&state)).await;
        }
    });
}

fn run_once(state: &AppState) -> anyhow::Result<()> {
    let Some(home) = dirs::home_dir() else { return Ok(()) };
    let dir = home.join(".skill").join("terminal-logs");
    if !dir.is_dir() {
        return Ok(());
    }

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let log_path = entry.path();
        if log_path.extension().is_none_or(|e| e != "log") {
            continue;
        }
        if pid_alive_for_log(&log_path) {
            continue; // shim still writing
        }
        if let Err(e) = finalize_one(state, &log_path) {
            warn!(path = %log_path.display(), error = %e, "finalize failed");
        }
    }
    Ok(())
}

fn finalize_one(state: &AppState, log_path: &Path) -> anyhow::Result<()> {
    let idx_path = log_path.with_extension("idx");
    let idx = read_index(&idx_path)?;
    if idx.is_empty() {
        // No timing data — drop the empty session.
        let _ = std::fs::remove_file(log_path);
        let _ = std::fs::remove_file(&idx_path);
        return Ok(());
    }

    let log_bytes = std::fs::read(log_path)?;
    let session_start_us = idx.first().map(|e| e.micros).unwrap_or(0);
    let session_end_us = idx.last().map(|e| e.micros).unwrap_or(u64::MAX);
    // `terminal_commands.started_at`/`ended_at` are in seconds; convert to
    // microseconds for comparison with our index timestamps.
    let session_start_s = session_start_us / 1_000_000;
    let session_end_s = session_end_us.saturating_add(999_999) / 1_000_000;

    let store = match get_store(state) {
        Some(s) => s,
        None => return Ok(()),
    };

    let cmds = store.commands_in_range(session_start_s, session_end_s);
    if cmds.is_empty() {
        // No matching commands recorded — likely an interactive session with
        // no preexec hook (or running before the daemon started). Drop the
        // raw files; nothing to attach them to.
        let _ = std::fs::remove_file(log_path);
        let _ = std::fs::remove_file(&idx_path);
        return Ok(());
    }

    let mut written = 0;
    for (cmd_id, started_at_s, ended_at_s) in cmds {
        let start_us = started_at_s.saturating_mul(1_000_000);
        // ended_at is the moment the prompt redrew; pad by 100 ms so the
        // tail of the command's output (which may arrive slightly after
        // precmd fires) isn't truncated.
        let end_us = ended_at_s.saturating_mul(1_000_000).saturating_add(100_000);

        let start_off = byte_offset_at_or_after(&idx, start_us);
        let end_off = byte_offset_at_or_after(&idx, end_us);
        if end_off <= start_off {
            continue;
        }
        let end_off = end_off.min(log_bytes.len() as u64);
        let start_off = start_off.min(end_off);
        let raw_size = end_off - start_off;
        let raw_capped = raw_size.min(MAX_RAW_BYTES_PER_COMMAND);
        let raw_slice = &log_bytes[start_off as usize..(start_off + raw_capped) as usize];

        // Compute stripped text from the (possibly truncated) raw slice.
        let mut stripped = skill_data::ansi::strip_ansi(raw_slice);
        if stripped.len() > MAX_STRIPPED_CHARS_PER_COMMAND {
            // Truncate at a UTF-8 char boundary.
            let mut end = MAX_STRIPPED_CHARS_PER_COMMAND;
            while !stripped.is_char_boundary(end) {
                end -= 1;
            }
            stripped.truncate(end);
        }

        // Compress the raw slice. Level 3 is the zstd default — fast and good
        // ratio on highly-repetitive ANSI streams.
        let raw_zstd = match zstd::encode_all(raw_slice, 3) {
            Ok(v) => Some(v),
            Err(e) => {
                warn!(error = %e, "zstd encode failed; storing stripped text only");
                None
            }
        };

        store.insert_terminal_output(cmd_id, start_us, end_us, raw_zstd.as_deref(), raw_size, &stripped);
        written += 1;
    }

    // Close the matching `terminal_sessions` row. The session id is the log
    // filename's stem (the shim sets it both as $NEUROSKILL_SESSION and as
    // the log filename when it starts).
    if let Some(stem) = log_path.file_stem().and_then(|s| s.to_str()) {
        store.close_terminal_session(stem, session_end_us / 1_000_000);
    }

    debug!(
        path = %log_path.display(),
        commands = written,
        bytes = log_bytes.len(),
        "finalized session"
    );

    // Source of truth is now in SQLite; drop the scratch files.
    let _ = std::fs::remove_file(log_path);
    let _ = std::fs::remove_file(&idx_path);
    Ok(())
}

#[derive(Clone, Copy)]
struct IdxEntry {
    /// Byte offset *after* this PTY-write batch.
    offset: u64,
    /// Wall-clock micros since UNIX epoch.
    micros: u64,
}

fn read_index(path: &Path) -> anyhow::Result<Vec<IdxEntry>> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return Ok(vec![]),
    };
    if bytes.len() % 16 != 0 {
        // Truncated tail — drop the partial record.
        let trunc = bytes.len() - (bytes.len() % 16);
        return parse_index(&bytes[..trunc]);
    }
    parse_index(&bytes)
}

fn parse_index(bytes: &[u8]) -> anyhow::Result<Vec<IdxEntry>> {
    let mut out = Vec::with_capacity(bytes.len() / 16);
    for chunk in bytes.chunks_exact(16) {
        let offset = u64::from_le_bytes(chunk[..8].try_into().unwrap());
        let micros = u64::from_le_bytes(chunk[8..].try_into().unwrap());
        out.push(IdxEntry { offset, micros });
    }
    Ok(out)
}

/// Find the byte offset immediately following the first index entry whose
/// timestamp is `>= target_us`. If `target_us` is past the end, returns the
/// final offset (i.e. the full log size at finalization time).
fn byte_offset_at_or_after(idx: &[IdxEntry], target_us: u64) -> u64 {
    if idx.is_empty() {
        return 0;
    }
    if target_us <= idx[0].micros {
        // Anything at or before the first entry maps to the start of the file.
        return 0;
    }
    if target_us > idx.last().unwrap().micros {
        return idx.last().unwrap().offset;
    }
    // Binary-search by timestamp.
    let pos = idx.binary_search_by(|e| e.micros.cmp(&target_us)).unwrap_or_else(|i| i);
    // The offset *before* this entry is where this batch starts; we want the
    // start, so look up the previous entry's offset (or 0 for index 0).
    if pos == 0 {
        0
    } else {
        idx[pos - 1].offset
    }
}

fn pid_alive_for_log(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    let Some(pid_str) = stem.rsplit('-').next() else {
        return false;
    };
    let Ok(pid) = pid_str.parse::<libc::pid_t>() else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    // SAFETY: signal 0 is the standard POSIX "is this PID alive" probe; libc::kill
    // is async-signal-safe and never dereferences user memory for the signal=0 case.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Trait alias so we can swap in a mock store under test.
trait Store {
    fn commands_in_range(&self, start_s: u64, end_s: u64) -> Vec<(i64, u64, u64)>;
    fn insert_terminal_output(
        &self,
        command_id: i64,
        time_start_us: u64,
        time_end_us: u64,
        raw_zstd: Option<&[u8]>,
        raw_size: u64,
        stripped: &str,
    );
    fn close_terminal_session(&self, id: &str, ended_at: u64);
}

struct ActivityStoreAdapter(skill_data::activity_store::ActivityStore);

impl Store for ActivityStoreAdapter {
    fn commands_in_range(&self, start_s: u64, end_s: u64) -> Vec<(i64, u64, u64)> {
        // Pull pending finalizations from the store. The activity store's own
        // helper already filters on (ended_at NOT NULL AND no terminal_outputs
        // row), so we just narrow by time range here.
        self.0
            .pending_terminal_outputs(10_000)
            .into_iter()
            .filter(|(_, started_s, ended_s)| {
                // Either endpoint inside the session window, or the command
                // span fully contains it (long-running command).
                let cmd_in_session = *started_s >= start_s.saturating_sub(2) && *ended_s <= end_s.saturating_add(2);
                let session_in_cmd = *started_s <= start_s && *ended_s >= end_s;
                cmd_in_session || session_in_cmd
            })
            .collect()
    }

    fn insert_terminal_output(
        &self,
        command_id: i64,
        time_start_us: u64,
        time_end_us: u64,
        raw_zstd: Option<&[u8]>,
        raw_size: u64,
        stripped: &str,
    ) {
        self.0
            .insert_terminal_output(command_id, time_start_us, time_end_us, raw_zstd, raw_size, stripped);
    }

    fn close_terminal_session(&self, id: &str, ended_at: u64) {
        self.0.close_terminal_session(id, ended_at);
    }
}

fn get_store(state: &AppState) -> Option<ActivityStoreAdapter> {
    let skill_dir = state.skill_dir.lock().ok()?.clone();
    // open() (read/write) — finalizer inserts rows.
    skill_data::activity_store::ActivityStore::open(&skill_dir).map(ActivityStoreAdapter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(offset: u64, micros: u64) -> IdxEntry {
        IdxEntry { offset, micros }
    }

    #[test]
    fn binary_search_basic() {
        let idx = vec![entry(10, 1000), entry(20, 2000), entry(30, 3000)];
        assert_eq!(byte_offset_at_or_after(&idx, 0), 0);
        assert_eq!(byte_offset_at_or_after(&idx, 1000), 0);
        assert_eq!(byte_offset_at_or_after(&idx, 1500), 10);
        assert_eq!(byte_offset_at_or_after(&idx, 2000), 10);
        assert_eq!(byte_offset_at_or_after(&idx, 4000), 30);
    }

    #[test]
    fn parse_index_round_trip() {
        let mut buf = Vec::new();
        for (off, mic) in [(10u64, 1000u64), (20, 2000), (30, 3000)] {
            buf.extend_from_slice(&off.to_le_bytes());
            buf.extend_from_slice(&mic.to_le_bytes());
        }
        let parsed = parse_index(&buf).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].offset, 10);
        assert_eq!(parsed[2].micros, 3000);
    }
}

// Force the function to be reachable when finalizer is wired up; remove
// when commands_in_range is hoisted to the public API.
#[allow(dead_code)]
fn _unused_paths(_p: PathBuf) {}
