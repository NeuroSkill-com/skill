// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Shared utilities — MutexExt, UTC date/time formatters, date-directory
//! scanning, read-only SQLite opener.

use std::path::{Path, PathBuf};
use std::sync::MutexGuard;
use std::time::{SystemTime, UNIX_EPOCH};

// ── MutexExt ──────────────────────────────────────────────────────────────────

/// Extension trait for `std::sync::Mutex` that recovers from poison.
pub trait MutexExt<T> {
    /// Acquire the lock, recovering the guard even if the mutex is poisoned.
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for std::sync::Mutex<T> {
    #[inline]
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poison| {
            eprintln!("[skill-data] Mutex was poisoned — recovering");
            poison.into_inner()
        })
    }
}

// ── Read-only SQLite opener ───────────────────────────────────────────────────

/// Open a SQLite database in **read-only** mode with `SQLITE_OPEN_NO_MUTEX`.
///
/// Consolidates the repeated `Connection::open_with_flags(…, READ_ONLY | NO_MUTEX)`
/// pattern used across many crates.
pub fn open_readonly(path: &Path) -> Result<rusqlite::Connection, rusqlite::Error> {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
}

// ── Date-directory scanning ───────────────────────────────────────────────────

/// Scan `skill_dir` for `YYYYMMDD` sub-directories and return them sorted.
///
/// This is the single implementation replacing `list_date_dirs` (skill-commands)
/// and `date_dirs` (skill-label-index).
pub fn date_dirs(skill_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(skill_dir) else { return out };
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
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Break a Unix-second timestamp into `(year, month, day, hour, min, sec)` (UTC).
pub fn civil_from_unix(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let rem = secs % 86400;
    let h = (rem / 3600) as u32;
    let m = ((rem % 3600) / 60) as u32;
    let s = (rem % 60) as u32;

    let mut days = (secs / 86400) as u64;
    let mut y = 1970u32;
    loop {
        let in_yr = if is_leap(y) { 366u64 } else { 365 };
        if days < in_yr { break; }
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
        if days < l { break; }
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
    let s  = (ts                   % 100) as u64;
    let m  = (ts /         100     % 100) as u64;
    let h  = (ts /      10_000     % 100) as u64;
    let d  = (ts /   1_000_000     % 100) as u64;
    let mo = (ts / 100_000_000     % 100) as u64;
    let y  = (ts / 10_000_000_000)        as u32;

    let mut days = 0u64;
    for yr in 1970..y {
        days += if is_leap(yr) { 366 } else { 365 };
    }

    let month_days: [u64; 12] = [
        31, if is_leap(y) { 29 } else { 28 }, 31, 30, 31, 30,
        31, 31, 30, 31, 30, 31,
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
mod tests {
    use super::*;

    #[test]
    fn civil_epoch() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn civil_known_date() {
        // 2026-03-15 12:00:00 UTC = 1773748800
        let (y, mo, d, h, _, _) = civil_from_unix(1773748800);
        assert_eq!((y, mo, d, h), (2026, 3, 15, 12));
    }

    #[test]
    fn unix_to_ts_roundtrip() {
        let unix = 1773748800u64;
        let ts = unix_to_ts(unix);
        assert_eq!(ts_to_unix(ts), unix);
    }

    #[test]
    fn fmt_unix_utc_format() {
        let s = fmt_unix_utc(1773748800);
        assert_eq!(s, "2026-03-15 12:00");
    }

    #[test]
    fn leap_year_checks() {
        assert!(is_leap(2000));
        assert!(!is_leap(1900));
        assert!(is_leap(2024));
        assert!(!is_leap(2023));
    }
}
