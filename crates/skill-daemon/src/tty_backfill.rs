// SPDX-License-Identifier: GPL-3.0-only
//! One-shot legacy backfill for the terminal pipeline.
//!
//! When the new shim+finalizer landed, every pre-existing terminal log on
//! disk was an opaque `<ts>-<pid>.log.zst` (compressed by the old rotation
//! code) with no `.idx` sidecar — so the finalizer skips them. This module
//! sweeps those once at daemon startup, decompresses each, ANSI-strips,
//! creates a `terminal_sessions` row with the whole stripped text, and
//! retroactively links any `terminal_commands` rows whose `started_at`
//! falls inside the file's time range.
//!
//! The session id matches the filename (e.g. `20260425-184113-5711`) so a
//! re-run is a no-op once we've migrated a file.
//!
//! This runs in a `spawn_blocking` task off the tokio runtime so it never
//! holds up daemon startup, and processes one file at a time to keep
//! peak memory low. Compression budget per session: 64 KB stripped text
//! cap (matches `MAX_STRIPPED_CHARS_PER_COMMAND` for consistency).

#![cfg(unix)]

use skill_daemon_state::AppState;
use std::path::Path;
use tracing::{debug, info, warn};

/// Cap on stripped text written per session (post-ANSI-strip). Many legacy
/// sessions are TUI-heavy with megabytes of redraw noise; truncating keeps
/// the DB small while preserving whatever readable content is there.
const MAX_STRIPPED_BYTES: usize = 64 * 1024;

pub fn spawn(state: AppState) {
    tokio::task::spawn_blocking(move || run_once(&state));
}

fn run_once(state: &AppState) {
    let Some(home) = dirs::home_dir() else { return };
    let dir = home.join(".skill").join("terminal-logs");
    if !dir.is_dir() {
        return;
    }
    let skill_dir = match state.skill_dir.lock() {
        Ok(g) => g.clone(),
        Err(_) => return,
    };
    let Some(store) = skill_data::activity_store::ActivityStore::open(&skill_dir) else {
        return;
    };

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut migrated = 0u32;
    let mut linked = 0u64;

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        // Only legacy compressed logs (.log.zst). Live `.log` files are owned
        // by the new shim+finalizer; we don't touch those.
        if !path.to_string_lossy().ends_with(".log.zst") {
            continue;
        }
        match migrate_one(&store, &path) {
            Ok(Some(n)) => {
                migrated += 1;
                linked += n;
            }
            Ok(None) => {} // already migrated
            Err(e) => warn!(path = %path.display(), error = %e, "backfill failed"),
        }
    }

    if migrated > 0 {
        info!(
            sessions = migrated,
            commands_linked = linked,
            "terminal-log backfill complete"
        );
    }
}

fn migrate_one(store: &skill_data::activity_store::ActivityStore, path: &Path) -> anyhow::Result<Option<u64>> {
    let session_id =
        parse_session_id(path).ok_or_else(|| anyhow::anyhow!("filename doesn't parse as <ts>-<pid>.log.zst"))?;

    // Skip if we've already created a session row for this file. Idempotent
    // re-runs let us re-trigger the backfill without duplicating work.
    if session_already_migrated(store, &session_id) {
        return Ok(None);
    }

    let (started_at_s, pid) =
        parse_filename_meta(path).ok_or_else(|| anyhow::anyhow!("filename time/pid parse failed"))?;
    let ended_at_s = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(started_at_s);

    let compressed = std::fs::read(path)?;
    let raw = zstd::decode_all(&compressed[..]).map_err(|e| anyhow::anyhow!("zstd decode: {e}"))?;

    // ANSI-strip and truncate at a UTF-8 char boundary.
    let mut stripped = skill_data::ansi::strip_ansi(&raw);
    if stripped.len() > MAX_STRIPPED_BYTES {
        let mut end = MAX_STRIPPED_BYTES;
        while !stripped.is_char_boundary(end) {
            end -= 1;
        }
        stripped.truncate(end);
    }

    // Compress the stripped (not raw) text — 5–10× smaller than the original
    // .log.zst and far more useful for search/embedding later.
    let session_text_zstd = zstd::encode_all(stripped.as_bytes(), 3).unwrap_or_default();
    let original_stripped_size = stripped.len() as u64;

    // Create the session row. `initial_cwd` is unknowable for legacy logs —
    // leave blank; clients render '—'.
    store.upsert_terminal_session(
        &session_id,
        started_at_s,
        "", // shell unknown for legacy
        "", // terminal_name unknown
        Some(pid),
        "",
    );
    store.close_terminal_session(&session_id, ended_at_s);
    store.set_terminal_session_text(&session_id, &session_text_zstd, original_stripped_size);

    // Bulk-link any orphan commands that ran during this session window.
    let linked = store.link_commands_to_session(&session_id, started_at_s, ended_at_s);
    debug!(
        session = %session_id,
        commands_linked = linked,
        stripped_bytes = original_stripped_size,
        "backfilled legacy session"
    );

    Ok(Some(linked))
}

fn parse_session_id(path: &Path) -> Option<String> {
    // Strip both `.log.zst` extensions to get the bare stem.
    let name = path.file_name().and_then(|s| s.to_str())?;
    name.strip_suffix(".log.zst").map(String::from)
}

fn parse_filename_meta(path: &Path) -> Option<(u64, i64)> {
    // Filename format: YYYYMMDD-HHMMSS-<pid>
    let name = path.file_name()?.to_str()?.strip_suffix(".log.zst")?;
    let (date, rest) = name.split_once('-')?; // "20260425", "184113-5711"
    let (clock, pid_str) = rest.split_once('-')?; // "184113", "5711"
    let pid: i64 = pid_str.parse().ok()?;

    if date.len() != 8 || clock.len() != 6 {
        return None;
    }
    let yyyy: i32 = date[..4].parse().ok()?;
    let mm: u32 = date[4..6].parse().ok()?;
    let dd: u32 = date[6..8].parse().ok()?;
    let hh: u32 = clock[..2].parse().ok()?;
    let min: u32 = clock[2..4].parse().ok()?;
    let ss: u32 = clock[4..6].parse().ok()?;

    let dt = chrono::NaiveDate::from_ymd_opt(yyyy, mm, dd)?
        .and_hms_opt(hh, min, ss)?
        .and_local_timezone(chrono::Local)
        .single()?;
    Some((dt.timestamp() as u64, pid))
}

fn session_already_migrated(store: &skill_data::activity_store::ActivityStore, session_id: &str) -> bool {
    // The cheapest check: does the session row already exist with non-empty
    // text? `get_terminal_session_text` returns `Some(...)` only when both
    // blob and size are present and non-empty.
    store.get_terminal_session_text(session_id).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_session_id() {
        let p = PathBuf::from("/some/dir/20260425-184113-5711.log.zst");
        assert_eq!(parse_session_id(&p).as_deref(), Some("20260425-184113-5711"));
    }

    #[test]
    fn parses_filename_meta() {
        let p = PathBuf::from("/some/dir/20260425-184113-5711.log.zst");
        let (ts, pid) = parse_filename_meta(&p).unwrap();
        assert!(ts > 1_700_000_000); // sanity: post-2023
        assert_eq!(pid, 5711);
    }

    #[test]
    fn rejects_bad_filename() {
        let p = PathBuf::from("/some/dir/garbage.log.zst");
        assert!(parse_filename_meta(&p).is_none());
    }
}
