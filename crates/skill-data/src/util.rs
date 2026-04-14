// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Shared utilities — MutexExt, UTC date/time formatters, date-directory
//! scanning, read-only SQLite opener.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// ── MutexExt ──────────────────────────────────────────────────────────────────

// Re-export the canonical definition from `skill-constants`.
pub use skill_constants::MutexExt;

// ── Read-only SQLite opener ───────────────────────────────────────────────────

/// Open a SQLite database in **read-only** mode with `SQLITE_OPEN_NO_MUTEX`.
///
/// Consolidates the repeated `Connection::open_with_flags(…, READ_ONLY | NO_MUTEX)`
/// pattern used across many crates.
pub fn open_readonly(path: &Path) -> Result<rusqlite::Connection, rusqlite::Error> {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
}

// ── JSON config load / save ────────────────────────────────────────────────────

/// Load a JSON config file, returning `T::default()` if the file is missing or
/// malformed.
///
/// This replaces the repeated `read_to_string(&path).ok().and_then(|s|
/// serde_json::from_str(&s).ok()).unwrap_or_default()` three-liner used by
/// `load_model_config`, `load_umap_config`, `load_log_config`, etc.
pub fn load_json_or_default<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Pretty-print `val` as JSON and write it to `path`.
///
/// Parent directories are **not** created — call `fs::create_dir_all` first if
/// needed.  Errors are silently ignored (matches existing behaviour across the
/// codebase).
pub fn save_json<T: serde::Serialize>(path: &Path, val: &T) {
    if let Ok(json) = serde_json::to_string_pretty(val) {
        let _ = std::fs::write(path, json);
    }
}

// ── SQLite WAL pragmas ────────────────────────────────────────────────────────

/// Apply the standard WAL + NORMAL-sync pragmas to a SQLite connection.
///
/// This consolidates the `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;`
/// one-liner duplicated in `activity_store`, `screenshot_store`, `hooks_log`,
/// and `eeg_embeddings`.
pub fn init_wal_pragmas(conn: &rusqlite::Connection) {
    let _ = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;");
}

// ── Blob ↔ f32 conversion ─────────────────────────────────────────────────────

/// Deserialise a SQLite `BLOB` (little-endian packed `f32` values) into a `Vec<f32>`.
///
/// This replaces the `chunks_exact(4).map(…)` pattern that was duplicated in
/// skill-commands, skill-label-index, skill-router, and skill-data/screenshot_store.
#[inline]
pub fn blob_to_f32(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Serialise a `&[f32]` slice into a `Vec<u8>` (little-endian) for SQLite storage.
#[inline]
pub fn f32_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

// ── HuggingFace Hub cache helpers ──────────────────────────────────────────────

/// Return the HuggingFace Hub cache root directory.
///
/// Resolution order (mirrors `hf_hub::Cache::from_env()`):
/// 1. `$HUGGINGFACE_HUB_CACHE`
/// 2. `$HF_HOME/hub`
/// 3. `~/.cache/huggingface/hub`
///
/// This avoids pulling the `hf-hub` crate into `skill-data`.
pub fn hf_cache_root() -> PathBuf {
    if let Ok(p) = std::env::var("HUGGINGFACE_HUB_CACHE") {
        return PathBuf::from(p);
    }
    if let Ok(hf_home) = std::env::var("HF_HOME") {
        return PathBuf::from(hf_home).join("hub");
    }
    home_dir_or_tmp().join(".cache/huggingface/hub")
}

/// Best-effort home directory, falling back to the system temp dir.
fn home_dir_or_tmp() -> PathBuf {
    #[cfg(unix)]
    {
        std::env::var("HOME")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
    }
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
    }
}

/// Return the model-specific directory under the HF cache for `repo_id`.
///
/// E.g. `hf_model_dir("Zyphra/ZUNA")` → `<cache>/models--Zyphra--ZUNA`.
pub fn hf_model_dir(repo_id: &str) -> PathBuf {
    let folder = format!("models--{}", repo_id.replace('/', "--"));
    hf_cache_root().join(folder)
}

/// Ensure the standard `blobs/` and `refs/` directories exist under
/// the model dir for `repo_id`.  Returns `(model_dir, blobs_dir, refs_dir)`.
pub fn hf_ensure_dirs(repo_id: &str) -> std::io::Result<(PathBuf, PathBuf, PathBuf)> {
    let model_dir = hf_model_dir(repo_id);
    let blobs_dir = model_dir.join("blobs");
    let refs_dir = model_dir.join("refs");
    std::fs::create_dir_all(&blobs_dir)?;
    std::fs::create_dir_all(&refs_dir)?;
    Ok((model_dir, blobs_dir, refs_dir))
}

// ── Date-directory scanning ───────────────────────────────────────────────────

/// Scan `skill_dir` for `YYYYMMDD` sub-directories and return them sorted.
///
/// This is the single implementation replacing `list_date_dirs` (skill-commands)
/// and `date_dirs` (skill-label-index).
pub fn date_dirs(skill_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(skill_dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.len() == 8 && name.bytes().all(|b| b.is_ascii_digit()) && entry.path().is_dir() {
            out.push((name.to_string(), entry.path()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

// ── UTC date/time helpers ─────────────────────────────────────────────────────

/// Whether `y` is a leap year (proleptic Gregorian).
#[inline]
pub fn is_leap(y: u32) -> bool {
    y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400))
}

/// Break a Unix-second timestamp into `(year, month, day, hour, min, sec)` (UTC).
pub fn civil_from_unix(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let rem = secs % 86400;
    let h = (rem / 3600) as u32;
    let m = ((rem % 3600) / 60) as u32;
    let s = (rem % 60) as u32;

    let mut days = secs / 86400;
    let mut y = 1970u32;
    loop {
        let in_yr = if is_leap(y) { 366u64 } else { 365 };
        if days < in_yr {
            break;
        }
        days -= in_yr;
        y += 1;
    }

    let ml: [u64; 12] = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut mo = 1u32;
    for &l in &ml {
        if days < l {
            break;
        }
        days -= l;
        mo += 1;
    }

    (y, mo, days as u32 + 1, h, m, s)
}

/// Current UTC date as `"YYYYMMDD"`.
pub fn yyyymmdd_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (y, mo, d, _, _, _) = civil_from_unix(secs);
    format!("{y:04}{mo:02}{d:02}")
}

/// Current UTC time as the integer `YYYYMMDDHHmmss`.
pub fn yyyymmddhhmmss_utc() -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (y, mo, d, h, m, s) = civil_from_unix(secs);
    (y as i64) * 10_000_000_000
        + (mo as i64) * 100_000_000
        + (d as i64) * 1_000_000
        + (h as i64) * 10_000
        + (m as i64) * 100
        + s as i64
}

/// Current UTC time as `("YYYYMMDDHHmmss", unix_secs)` — string variant for
/// file naming (used by the screenshot pipeline).
pub fn yyyymmddhhmmss_utc_str() -> (String, u64) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (y, mo, d, h, m, s) = civil_from_unix(secs);
    (format!("{y:04}{mo:02}{d:02}{h:02}{m:02}{s:02}"), secs)
}

/// Convert Unix seconds → `YYYYMMDDHHmmss` integer.
pub fn unix_to_ts(secs: u64) -> i64 {
    let (y, mo, d, h, m, s) = civil_from_unix(secs);
    (y as i64) * 10_000_000_000
        + (mo as i64) * 100_000_000
        + (d as i64) * 1_000_000
        + (h as i64) * 10_000
        + (m as i64) * 100
        + s as i64
}

/// Convert `YYYYMMDDHHmmss` integer → Unix seconds (UTC).
pub fn ts_to_unix(ts: i64) -> u64 {
    let s = (ts % 100) as u64;
    let m = (ts / 100 % 100) as u64;
    let h = (ts / 10_000 % 100) as u64;
    let d = (ts / 1_000_000 % 100) as u64;
    let mo = (ts / 100_000_000 % 100) as u64;
    let y = (ts / 10_000_000_000) as u32;

    let mut days = 0u64;
    for yr in 1970..y {
        days += if is_leap(yr) { 366 } else { 365 };
    }

    let month_days: [u64; 12] = [
        31,
        if is_leap(y) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for &md in month_days.iter().take(mo as usize - 1) {
        days += md;
    }

    days += d - 1;
    days * 86400 + h * 3600 + m * 60 + s
}

/// Format a Unix-second timestamp as `"YYYY-MM-DD HH:MM"` (UTC, no external crate).
pub fn fmt_unix_utc(ts: u64) -> String {
    let (yr, mo, d, h, m, _) = civil_from_unix(ts);
    format!("{yr:04}-{mo:02}-{d:02} {h:02}:{m:02}")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::approx_constant)]
mod tests {
    use super::*;

    #[test]
    fn hf_model_dir_formats_correctly() {
        let dir = hf_model_dir("Zyphra/ZUNA");
        let name = dir.file_name().unwrap().to_string_lossy();
        assert_eq!(name, "models--Zyphra--ZUNA");
    }

    #[test]
    fn hf_cache_root_returns_nonempty() {
        let root = hf_cache_root();
        assert!(!root.as_os_str().is_empty());
    }

    #[test]
    fn blob_f32_roundtrip() {
        let v = vec![1.0f32, -2.5, 3.14159, 0.0];
        let blob = f32_to_blob(&v);
        let v2 = blob_to_f32(&blob);
        assert_eq!(v, v2);
    }

    #[test]
    fn blob_to_f32_empty() {
        assert!(blob_to_f32(&[]).is_empty());
    }

    #[test]
    fn civil_epoch() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn civil_known_date() {
        // 2026-03-15 12:00:00 UTC = 1773576000
        let (y, mo, d, h, _, _) = civil_from_unix(1773576000);
        assert_eq!((y, mo, d, h), (2026, 3, 15, 12));
    }

    #[test]
    fn unix_to_ts_roundtrip() {
        let unix = 1773576000u64;
        let ts = unix_to_ts(unix);
        assert_eq!(ts_to_unix(ts), unix);
    }

    #[test]
    fn fmt_unix_utc_format() {
        let s = fmt_unix_utc(1773576000);
        assert_eq!(s, "2026-03-15 12:00");
    }

    #[test]
    fn leap_year_checks() {
        assert!(is_leap(2000));
        assert!(!is_leap(1900));
        assert!(is_leap(2024));
        assert!(!is_leap(2023));
    }

    #[test]
    fn ts_to_unix_known_date() {
        // 20260315120000 → 2026-03-15 12:00:00 UTC = 1773576000
        assert_eq!(ts_to_unix(20260315120000), 1773576000);
    }

    #[test]
    fn ts_to_unix_epoch() {
        assert_eq!(ts_to_unix(19700101000000), 0);
    }

    #[test]
    fn unix_to_ts_known_date() {
        assert_eq!(unix_to_ts(1773576000), 20260315120000);
    }

    #[test]
    fn civil_from_unix_leap_day() {
        // 2024-02-29 00:00:00 UTC = 1709164800
        let (y, mo, d, _, _, _) = civil_from_unix(1709164800);
        assert_eq!((y, mo, d), (2024, 2, 29));
    }

    #[test]
    fn civil_from_unix_end_of_year() {
        // 2025-12-31 23:59:59 UTC = 1767225599
        let (y, mo, d, h, m, s) = civil_from_unix(1767225599);
        assert_eq!((y, mo, d, h, m, s), (2025, 12, 31, 23, 59, 59));
    }

    #[test]
    fn f32_to_blob_length() {
        let v = vec![1.0f32, 2.0, 3.0];
        let blob = f32_to_blob(&v);
        assert_eq!(blob.len(), 12); // 3 floats × 4 bytes
    }

    #[test]
    fn blob_to_f32_truncates_partial() {
        // 5 bytes → only 1 float (4 bytes), last byte ignored
        let blob = vec![0u8; 5];
        let v = blob_to_f32(&blob);
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn fmt_unix_utc_epoch() {
        assert_eq!(fmt_unix_utc(0), "1970-01-01 00:00");
    }

    #[test]
    fn yyyymmdd_utc_returns_8_chars() {
        let s = yyyymmdd_utc();
        assert_eq!(s.len(), 8);
        assert!(s.starts_with("20"), "expected current century: {s}");
    }

    #[test]
    fn yyyymmddhhmmss_utc_returns_14_digits() {
        let ts = yyyymmddhhmmss_utc();
        assert!(ts > 20200101000000, "expected recent timestamp: {ts}");
        assert!(ts < 21000101000000, "expected before 2100: {ts}");
    }
}
