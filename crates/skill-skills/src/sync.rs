// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Periodic download of the latest community skills from GitHub.
//!
//! Downloads the `main` branch tarball from the public `NeuroSkill-com/skills`
//! repository, extracts it into `<skill_dir>/skills/`, and writes a small
//! `.last_sync` JSON sidecar so the caller can skip re-downloads within the
//! configured refresh interval.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Default GitHub tarball URL (public, no auth required).
const SKILLS_TARBALL_URL: &str = "https://github.com/NeuroSkill-com/skills/archive/refs/heads/main.tar.gz";

/// Sidecar written next to the extracted skills directory.
const LAST_SYNC_FILE: &str = ".skills_last_sync";

/// Default refresh interval: 24 hours.
pub const DEFAULT_SKILLS_REFRESH_SECS: u64 = 86_400;

// ── Types ─────────────────────────────────────────────────────────────────────

/// Persisted sync metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SyncMeta {
    /// Unix timestamp (seconds) of the last successful sync.
    last_sync_ts: u64,
    /// URL that was fetched (so we detect config changes).
    #[serde(default)]
    url: String,
}

/// Result of a sync attempt.
#[derive(Debug, Clone)]
pub enum SyncOutcome {
    /// Skills were downloaded and extracted successfully.
    Updated { skills_dir: PathBuf, elapsed_ms: u64 },
    /// Skipped because the last sync is still fresh.
    Fresh { next_sync_in_secs: u64 },
    /// An error occurred.
    Failed(String),
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Synchronise the community skills directory if the refresh interval has
/// elapsed since the last successful sync.
///
/// - `skill_dir` — the user data directory (e.g. `~/.skill`).
/// - `interval_secs` — minimum seconds between syncs.  Pass `0` to force.
/// - `url` — optional override for the tarball URL.
///
/// This function is **blocking** (network I/O via `ureq`) — call it from a
/// background thread or `tokio::task::spawn_blocking`.
pub fn sync_skills(skill_dir: &Path, interval_secs: u64, url: Option<&str>) -> SyncOutcome {
    let target_dir = skill_dir.join(skill_constants::SKILLS_SUBDIR);
    let meta_path = skill_dir.join(LAST_SYNC_FILE);
    let tarball_url = url.unwrap_or(SKILLS_TARBALL_URL);

    // Check freshness.
    if interval_secs > 0 {
        if let Some(meta) = load_meta(&meta_path) {
            let now = now_secs();
            let age = now.saturating_sub(meta.last_sync_ts);
            if age < interval_secs && meta.url == tarball_url {
                return SyncOutcome::Fresh {
                    next_sync_in_secs: interval_secs - age,
                };
            }
        }
    }

    let start = std::time::Instant::now();

    // Download.
    let tarball = match download(tarball_url) {
        Ok(bytes) => bytes,
        Err(e) => return SyncOutcome::Failed(format!("download: {e}")),
    };

    // Extract to a temp dir first, then swap.
    let tmp_dir = skill_dir.join(".skills_tmp");
    let _ = fs::remove_dir_all(&tmp_dir);
    if let Err(e) = fs::create_dir_all(&tmp_dir) {
        return SyncOutcome::Failed(format!("create tmp dir: {e}"));
    }

    if let Err(e) = extract_tarball(&tarball, &tmp_dir) {
        let _ = fs::remove_dir_all(&tmp_dir);
        return SyncOutcome::Failed(format!("extract: {e}"));
    }

    // The tarball contains a single top-level directory like `skills-main/`.
    // Move its contents into the target.
    let Some(inner) = find_single_child_dir(&tmp_dir) else {
        let _ = fs::remove_dir_all(&tmp_dir);
        return SyncOutcome::Failed("tarball does not contain a single top-level directory".into());
    };

    // Atomic-ish swap: remove old, rename new.
    let _ = fs::remove_dir_all(&target_dir);
    if let Err(e) = fs::rename(&inner, &target_dir) {
        // Cross-device rename fallback: copy tree.
        if let Err(e2) = copy_dir_recursive(&inner, &target_dir) {
            let _ = fs::remove_dir_all(&tmp_dir);
            return SyncOutcome::Failed(format!("move/copy: rename={e}, copy={e2}"));
        }
    }
    let _ = fs::remove_dir_all(&tmp_dir);

    // Write sync metadata.
    let meta = SyncMeta {
        last_sync_ts: now_secs(),
        url: tarball_url.to_owned(),
    };
    save_meta(&meta_path, &meta);

    let elapsed_ms = start.elapsed().as_millis() as u64;
    SyncOutcome::Updated {
        skills_dir: target_dir,
        elapsed_ms,
    }
}

/// Return the Unix timestamp of the last successful sync, or `None`.
pub fn last_sync_ts(skill_dir: &Path) -> Option<u64> {
    let meta = load_meta(&skill_dir.join(LAST_SYNC_FILE))?;
    Some(meta.last_sync_ts)
}

// ── Internals ─────────────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn load_meta(path: &Path) -> Option<SyncMeta> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_meta(path: &Path, meta: &SyncMeta) {
    if let Ok(json) = serde_json::to_string_pretty(meta) {
        let _ = fs::write(path, json);
    }
}

fn download(url: &str) -> anyhow::Result<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build().into();
    let resp = agent.get(url).call()?;

    let mut buf = Vec::new();
    resp.into_body().into_reader().read_to_end(&mut buf)?;
    Ok(buf)
}

fn extract_tarball(data: &[u8], dest: &Path) -> anyhow::Result<()> {
    let gz = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(gz);
    archive.unpack(dest)?;
    Ok(())
}

fn find_single_child_dir(parent: &Path) -> Option<PathBuf> {
    let entries: Vec<_> = fs::read_dir(parent)
        .ok()?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .collect();
    if entries.len() == 1 {
        Some(entries[0].path())
    } else {
        None
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_roundtrip() {
        let dir = std::env::temp_dir().join("skill-skills-sync-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(LAST_SYNC_FILE);

        assert!(load_meta(&path).is_none());

        let meta = SyncMeta {
            last_sync_ts: 1234567890,
            url: "https://example.com".into(),
        };
        save_meta(&path, &meta);

        let loaded = load_meta(&path).unwrap();
        assert_eq!(loaded.last_sync_ts, 1234567890);
        assert_eq!(loaded.url, "https://example.com");

        let _ = fs::remove_dir_all(&dir);
    }
}
