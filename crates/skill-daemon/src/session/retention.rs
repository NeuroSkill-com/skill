// SPDX-License-Identifier: GPL-3.0-only
//! Session-file retention.
//!
//! Day directories (`<skill_dir>/YYYYMMDD/`) hold all of a day's session
//! artifacts: EEG/PPG/IMU/fNIRS CSV+Parquet, sidecar JSONs, metrics caches,
//! per-day SQLite + HNSW indices. After `file_retention_days` they are
//! removed wholesale.
//!
//! Wholesale day-dir deletion is correct because every session-related
//! artifact lives inside the day dir, and the dir name itself encodes the
//! date — no per-file timestamp parsing required.

use std::path::Path;

/// Convert Unix seconds (UTC) to a packed `YYYYMMDD` integer using the same
/// civil-from-days arithmetic as [`crate::session::shared::utc_date_dir`].
pub(crate) fn unix_to_yyyymmdd(secs: u64) -> u32 {
    let days = (secs / 86400) as i64;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32) * 10000 + (m as u32) * 100 + (d as u32)
}

/// Remove every day directory in `skill_dir` whose name is older than the
/// retention window. Returns `(removed, errors)`.
///
/// `retention_days == 0` disables retention (matches the convention used
/// elsewhere in settings).
pub(crate) fn prune_session_dirs(skill_dir: &Path, retention_days: u32, now_secs: u64) -> (usize, usize) {
    if retention_days == 0 {
        return (0, 0);
    }
    let cutoff_secs = now_secs.saturating_sub(u64::from(retention_days) * 86400);
    let cutoff_yyyymmdd = unix_to_yyyymmdd(cutoff_secs);

    let Ok(entries) = std::fs::read_dir(skill_dir) else {
        return (0, 0);
    };

    let mut removed = 0;
    let mut errors = 0;
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else { continue };
        if name_str.len() != 8 || !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(dir_yyyymmdd) = name_str.parse::<u32>() else {
            continue;
        };
        if dir_yyyymmdd >= cutoff_yyyymmdd {
            continue;
        }
        match std::fs::remove_dir_all(entry.path()) {
            Ok(()) => removed += 1,
            Err(e) => {
                errors += 1;
                tracing::warn!(dir = %entry.path().display(), %e, "failed to prune session day dir");
            }
        }
    }
    (removed, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch_dir(root: &Path, name: &str) {
        let d = root.join(name);
        std::fs::create_dir_all(&d).unwrap();
        // Drop a placeholder file so we exercise recursive removal.
        std::fs::write(d.join("exg_1.csv"), "timestamp_s,Ch1\n0.0,0.0\n").unwrap();
        std::fs::write(d.join("exg_1.json"), r#"{"device_name":"x"}"#).unwrap();
    }

    #[test]
    fn unix_to_yyyymmdd_known_dates() {
        // 2024-01-01 00:00:00 UTC = 1704067200
        assert_eq!(unix_to_yyyymmdd(1_704_067_200), 20240101);
        // 2025-06-15 12:00:00 UTC = 1750_000_800 — verify packing format.
        let v = unix_to_yyyymmdd(1_750_000_800);
        assert!((20250101..=20251231).contains(&v));
        // Epoch.
        assert_eq!(unix_to_yyyymmdd(0), 19700101);
    }

    #[test]
    fn prune_removes_old_dirs_only() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        // now = 2024-06-15
        let now: u64 = 1_718_409_600;

        // Old: 2024-01-01 (≈ 166 days old)
        touch_dir(root, "20240101");
        // Borderline: 2024-06-14 (1 day old — keep)
        touch_dir(root, "20240614");
        // Today.
        touch_dir(root, "20240615");
        // Non-day dir: must NOT be touched.
        touch_dir(root, "logs");
        // 8-digit non-numeric: must NOT be touched.
        std::fs::create_dir_all(root.join("notadate")).unwrap();
        std::fs::write(root.join("notadate/marker"), "").unwrap();

        let (removed, errors) = prune_session_dirs(root, 30, now);
        assert_eq!(errors, 0);
        assert_eq!(removed, 1, "only the 2024-01-01 dir should be pruned");

        assert!(!root.join("20240101").exists());
        assert!(root.join("20240614").exists());
        assert!(root.join("20240615").exists());
        assert!(root.join("logs").exists());
        assert!(root.join("notadate").exists());
    }

    #[test]
    fn prune_disabled_when_retention_zero() {
        let td = tempfile::tempdir().unwrap();
        touch_dir(td.path(), "20200101");
        let now: u64 = 1_718_409_600;

        let (removed, errors) = prune_session_dirs(td.path(), 0, now);
        assert_eq!(removed, 0);
        assert_eq!(errors, 0);
        assert!(td.path().join("20200101").exists());
    }

    #[test]
    fn prune_handles_missing_dir() {
        let td = tempfile::tempdir().unwrap();
        let missing = td.path().join("does_not_exist");
        let (removed, errors) = prune_session_dirs(&missing, 30, 1_718_409_600);
        assert_eq!(removed, 0);
        assert_eq!(errors, 0);
    }
}
