// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Timezone-aware local-day computation for session history.
//
// **Single source of truth** — the TypeScript frontend passes its
// `getTimezoneOffset()` and this module handles all UTC↔local
// conversions, day-boundary computation, and session filtering.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{label_store, list_session_days, list_sessions_for_day, SessionEntry};

// ── Date arithmetic (no external crate needed) ───────────────────────────────
//
// Uses Howard Hinnant's public-domain algorithms:
//   http://howardhinnant.github.io/date_algorithms.html

/// Convert days-since-Unix-epoch to (year, month, day).
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u32; // day-of-era  [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

/// Convert (year, month, day) to days-since-Unix-epoch.
fn ymd_to_days(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

// ── Public helpers ───────────────────────────────────────────────────────────

/// Convert a UTC unix timestamp to a local `YYYY-MM-DD` key.
///
/// `tz_offset_secs` is seconds east of UTC (e.g. −25 200 for UTC−7).
/// The frontend computes this as `new Date().getTimezoneOffset() * -60`.
pub fn utc_to_local_date_key(utc_secs: u64, tz_offset_secs: i64) -> String {
    let local_secs = utc_secs as i64 + tz_offset_secs;
    let days = local_secs.div_euclid(86400);
    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Convert a UTC unix timestamp to a `YYYYMMDD` directory name.
pub fn utc_secs_to_dir(utc_secs: u64) -> String {
    let days = utc_secs as i64 / 86400;
    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}{m:02}{d:02}")
}

/// Parse a `YYYY-MM-DD` local key and return the UTC-second boundaries
/// `[start, end)` of that local calendar day.
///
/// Returns `None` for malformed keys.
pub fn local_day_bounds_utc(local_key: &str, tz_offset_secs: i64) -> Option<(u64, u64)> {
    let (y, m, d) = parse_local_key(local_key)?;
    let midnight_utc_secs = ymd_to_days(y, m, d) * 86400;
    // Local midnight expressed in UTC = midnight_utc − tz_offset
    let start = (midnight_utc_secs - tz_offset_secs).try_into().ok()?;
    let end = start + 86400;
    Some((start, end))
}

/// Compact result returned to the frontend for each local day.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocalDayInfo {
    /// `YYYY-MM-DD` in the user's local timezone.
    pub key: String,
    /// UTC unix-seconds of local midnight (start of this day).
    pub start_utc: u64,
    /// UTC unix-seconds of next local midnight (exclusive end).
    pub end_utc: u64,
}

// ── Core API ─────────────────────────────────────────────────────────────────

/// Return local day keys that have sessions, newest first.
///
/// This is the **single source of truth** replacing the three divergent
/// `buildLocalDays` implementations that lived in TypeScript.
pub fn list_local_session_days(skill_dir: &Path, tz_offset_secs: i64) -> Vec<LocalDayInfo> {
    let utc_dirs = list_session_days(skill_dir);
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let today = utc_to_local_date_key(now_secs, tz_offset_secs);

    let mut seen = std::collections::BTreeSet::new();
    for dir in &utc_dirs {
        if dir.len() != 8 {
            continue;
        }
        let Ok(y) = dir[..4].parse::<i32>() else {
            continue;
        };
        let Ok(m) = dir[4..6].parse::<u32>() else {
            continue;
        };
        let Ok(d) = dir[6..8].parse::<u32>() else {
            continue;
        };

        // A UTC day [00:00, 23:59:59] can straddle two local calendar days.
        let dir_start_utc = ymd_to_days(y, m, d) * 86400;
        let dir_end_utc = dir_start_utc + 86400 - 1;

        for &utc in &[dir_start_utc as u64, dir_end_utc as u64] {
            let lk = utc_to_local_date_key(utc, tz_offset_secs);
            if lk <= today {
                seen.insert(lk);
            }
        }
    }

    // Build result with pre-computed bounds (saves the frontend from
    // re-deriving them and eliminates another class of mismatch bugs).
    let mut result: Vec<LocalDayInfo> = seen
        .into_iter()
        .filter_map(|key| {
            let (start_utc, end_utc) = local_day_bounds_utc(&key, tz_offset_secs)?;
            Some(LocalDayInfo {
                key,
                start_utc,
                end_utc,
            })
        })
        .collect();
    result.sort_by(|a, b| b.key.cmp(&a.key)); // newest first
    result
}

/// Load sessions for a single local calendar day.
///
/// Handles the UTC directory fan-out, de-duplication, timestamp filtering,
/// and sorting — all in one place so the frontend never touches timestamps.
pub fn list_sessions_for_local_day(
    local_key: &str,
    tz_offset_secs: i64,
    skill_dir: &Path,
    label_store: Option<&label_store::LabelStore>,
) -> Vec<SessionEntry> {
    let Some((start_utc, end_utc)) = local_day_bounds_utc(local_key, tz_offset_secs) else {
        return vec![];
    };

    // Which UTC directories could contain sessions for this local day?
    let dir1 = utc_secs_to_dir(start_utc);
    let dir2 = utc_secs_to_dir(end_utc.saturating_sub(1));

    let mut dirs = vec![dir1.clone()];
    if dir2 != dir1 {
        dirs.push(dir2);
    }

    // Fetch + de-duplicate across UTC dirs.
    let mut seen = std::collections::HashSet::new();
    let mut merged = Vec::new();
    for dir in &dirs {
        for s in list_sessions_for_day(dir, skill_dir, label_store) {
            if seen.insert(s.csv_path.clone()) {
                merged.push(s);
            }
        }
    }

    // Keep only sessions whose start (or end) falls within this local day.
    merged.retain(|s| {
        let t = s.session_start_utc.or(s.session_end_utc);
        matches!(t, Some(t) if t >= start_utc && t < end_utc)
    });

    // Newest first.
    merged.sort_by(|a, b| {
        let ta = a.session_start_utc.or(a.session_end_utc).unwrap_or(0);
        let tb = b.session_start_utc.or(b.session_end_utc).unwrap_or(0);
        tb.cmp(&ta)
    });

    merged
}

// ── Internal ─────────────────────────────────────────────────────────────────

fn parse_local_key(key: &str) -> Option<(i32, u32, u32)> {
    let mut parts = key.splitn(3, '-');
    let y: i32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    Some((y, m, d))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // UTC−7 (PDT): getTimezoneOffset() = 420 → offset = −25200
    const TZ_PDT: i64 = -25200;
    // UTC+0
    const TZ_UTC: i64 = 0;
    // UTC+5:30 (IST)
    const TZ_IST: i64 = 19800;
    // UTC+14 (Line Islands)
    const TZ_LINT: i64 = 50400;
    // UTC−12 (Baker Island)
    const TZ_BIT: i64 = -43200;

    // March 1, 2026 00:00:00 UTC = 1,772,323,200
    const MAR01_MIDNIGHT_UTC: u64 = 1_772_323_200;

    // ── days_to_ymd / ymd_to_days round-trip ─────────────────────────────

    #[test]
    fn epoch_is_1970_01_01() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn ymd_round_trip() {
        for &(y, m, d) in &[
            (1970, 1, 1),
            (2000, 2, 29),
            (2026, 3, 29),
            (2026, 12, 31),
            (1969, 12, 31),
        ] {
            let days = ymd_to_days(y, m, d);
            assert_eq!(days_to_ymd(days), (y, m, d), "round-trip {y}-{m}-{d}");
        }
    }

    #[test]
    fn known_epoch_value() {
        // 2026-03-01 00:00 UTC = day 20513 from epoch
        let days = ymd_to_days(2026, 3, 1);
        assert_eq!(days * 86400, MAR01_MIDNIGHT_UTC as i64);
    }

    // ── utc_to_local_date_key ────────────────────────────────────────────

    #[test]
    fn utc_midnight_in_utc() {
        assert_eq!(utc_to_local_date_key(MAR01_MIDNIGHT_UTC, TZ_UTC), "2026-03-01");
    }

    #[test]
    fn utc_midnight_in_pdt() {
        // March 1 00:00 UTC → Feb 28 17:00 PDT
        assert_eq!(utc_to_local_date_key(MAR01_MIDNIGHT_UTC, TZ_PDT), "2026-02-28");
    }

    #[test]
    fn utc_noon_in_pdt() {
        // March 1 12:00 UTC → March 1 05:00 PDT
        assert_eq!(utc_to_local_date_key(MAR01_MIDNIGHT_UTC + 43200, TZ_PDT), "2026-03-01");
    }

    #[test]
    fn utc_in_ist() {
        // March 1 00:00 UTC → March 1 05:30 IST
        assert_eq!(utc_to_local_date_key(MAR01_MIDNIGHT_UTC, TZ_IST), "2026-03-01");
    }

    #[test]
    fn utc_in_lint() {
        // March 1 00:00 UTC → March 1 14:00 LINT (UTC+14)
        assert_eq!(utc_to_local_date_key(MAR01_MIDNIGHT_UTC, TZ_LINT), "2026-03-01");
        // March 1 12:00 UTC → March 2 02:00 LINT
        assert_eq!(utc_to_local_date_key(MAR01_MIDNIGHT_UTC + 43200, TZ_LINT), "2026-03-02");
    }

    #[test]
    fn utc_in_bit() {
        // March 1 00:00 UTC → Feb 28 12:00 BIT (UTC−12)
        assert_eq!(utc_to_local_date_key(MAR01_MIDNIGHT_UTC, TZ_BIT), "2026-02-28");
    }

    // ── utc_secs_to_dir ──────────────────────────────────────────────────

    #[test]
    fn dir_from_epoch() {
        assert_eq!(utc_secs_to_dir(0), "19700101");
    }

    #[test]
    fn dir_from_known_ts() {
        assert_eq!(utc_secs_to_dir(MAR01_MIDNIGHT_UTC), "20260301");
        assert_eq!(utc_secs_to_dir(MAR01_MIDNIGHT_UTC + 86399), "20260301");
        assert_eq!(utc_secs_to_dir(MAR01_MIDNIGHT_UTC + 86400), "20260302");
    }

    // ── local_day_bounds_utc ─────────────────────────────────────────────

    #[test]
    fn bounds_utc_zone() {
        let (s, e) = local_day_bounds_utc("2026-03-01", TZ_UTC).unwrap();
        assert_eq!(s, MAR01_MIDNIGHT_UTC);
        assert_eq!(e - s, 86400);
    }

    #[test]
    fn bounds_pdt() {
        // Local March 1 midnight PDT = March 1 07:00 UTC
        let (s, e) = local_day_bounds_utc("2026-03-01", TZ_PDT).unwrap();
        assert_eq!(s, MAR01_MIDNIGHT_UTC + 25200);
        assert_eq!(e - s, 86400);
    }

    #[test]
    fn bounds_ist() {
        // Local March 1 midnight IST = Feb 28 18:30 UTC
        let (s, e) = local_day_bounds_utc("2026-03-01", TZ_IST).unwrap();
        assert_eq!(s, MAR01_MIDNIGHT_UTC - 19800);
        assert_eq!(e - s, 86400);
    }

    #[test]
    fn bounds_malformed_key() {
        assert!(local_day_bounds_utc("bad", TZ_UTC).is_none());
        assert!(local_day_bounds_utc("2026-13-01", TZ_UTC).is_some()); // we don't validate ranges
    }

    // ── list_local_session_days (unit-level, no I/O) ─────────────────────

    #[test]
    fn utc_dir_fans_out_to_two_local_days_in_pdt() {
        // Simulate: UTC dir "20260301" should produce local days
        // "2026-02-28" (from 00:00 UTC → 17:00 prev-day local) and
        // "2026-03-01" (from 23:59 UTC → 16:59 local).
        let start = MAR01_MIDNIGHT_UTC;
        let end = start + 86399;
        let k1 = utc_to_local_date_key(start, TZ_PDT);
        let k2 = utc_to_local_date_key(end, TZ_PDT);
        assert_eq!(k1, "2026-02-28");
        assert_eq!(k2, "2026-03-01");
    }

    // ── Session filtering ────────────────────────────────────────────────

    #[test]
    fn session_in_local_day_pdt() {
        // Session at March 1 08:00 UTC → March 1 01:00 PDT → local March 1
        let ts = MAR01_MIDNIGHT_UTC + 8 * 3600;
        let (s, e) = local_day_bounds_utc("2026-03-01", TZ_PDT).unwrap();
        assert!(ts >= s && ts < e, "session should be in local March 1");
    }

    #[test]
    fn session_before_local_day_pdt() {
        // Session at March 1 06:00 UTC → Feb 28 23:00 PDT → local Feb 28
        let ts = MAR01_MIDNIGHT_UTC + 6 * 3600;
        let (s, e) = local_day_bounds_utc("2026-03-01", TZ_PDT).unwrap();
        assert!(ts < s, "session should be before local March 1");
        // But should be in local Feb 28
        let (s2, e2) = local_day_bounds_utc("2026-02-28", TZ_PDT).unwrap();
        assert!(ts >= s2 && ts < e2, "session should be in local Feb 28");
    }

    #[test]
    fn session_at_exact_midnight_boundary() {
        // Session at exactly local midnight → belongs to the new day
        let (s, e) = local_day_bounds_utc("2026-03-01", TZ_PDT).unwrap();
        assert!(s < e && e - s == 86400);
    }

    // ── End-to-end with temp dir ─────────────────────────────────────────

    #[test]
    fn list_local_days_with_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let skill = tmp.path();

        // Create a day directory with a session JSON
        let day_dir = skill.join("20260301");
        std::fs::create_dir_all(&day_dir).unwrap();
        std::fs::write(
            day_dir.join("exg_1772348400.json"),
            r#"{"csv_file":"exg_1772348400.csv","session_start_utc":1772348400,"session_end_utc":1772352000}"#,
        )
        .unwrap();
        // Also create the CSV so csv_path resolves
        std::fs::write(day_dir.join("exg_1772348400.csv"), "t,v\n1772348400.0,0.1\n").unwrap();

        // UTC: March 1 07:00 UTC → PDT: March 1 00:00 local
        let days = list_local_session_days(skill, TZ_PDT);
        let keys: Vec<&str> = days.iter().map(|d| d.key.as_str()).collect();
        assert!(keys.contains(&"2026-03-01"), "expected 2026-03-01 in {:?}", keys);

        // Now load sessions for that local day
        let sessions = list_sessions_for_local_day("2026-03-01", TZ_PDT, skill, None);
        assert_eq!(sessions.len(), 1, "expected 1 session, got {}", sessions.len());
        assert_eq!(sessions[0].session_start_utc, Some(1_772_348_400));
    }

    #[test]
    fn session_in_wrong_local_day_filtered_out() {
        let tmp = tempfile::tempdir().unwrap();
        let skill = tmp.path();

        let day_dir = skill.join("20260301");
        std::fs::create_dir_all(&day_dir).unwrap();
        // Session at March 1 00:00 UTC → Feb 28 17:00 PDT → local Feb 28
        std::fs::write(
            day_dir.join("exg_1772323200.json"),
            r#"{"csv_file":"exg_1772323200.csv","session_start_utc":1772323200,"session_end_utc":1772326800}"#,
        )
        .unwrap();
        std::fs::write(day_dir.join("exg_1772323200.csv"), "t,v\n").unwrap();

        // Should NOT appear in local March 1
        let sessions_mar1 = list_sessions_for_local_day("2026-03-01", TZ_PDT, skill, None);
        assert_eq!(
            sessions_mar1.len(),
            0,
            "session at UTC midnight should not be in local March 1 (PDT)"
        );

        // Should appear in local Feb 28
        let sessions_feb28 = list_sessions_for_local_day("2026-02-28", TZ_PDT, skill, None);
        assert_eq!(
            sessions_feb28.len(),
            1,
            "session at UTC midnight should be in local Feb 28 (PDT)"
        );
    }

    #[test]
    fn multiple_utc_dirs_merge_into_one_local_day() {
        let tmp = tempfile::tempdir().unwrap();
        let skill = tmp.path();

        // Session A: in UTC dir 20260228, at Feb 28 23:00 UTC → Feb 28 16:00 PDT
        let dir28 = skill.join("20260228");
        std::fs::create_dir_all(&dir28).unwrap();
        std::fs::write(
            dir28.join("exg_1772262000.json"),
            r#"{"csv_file":"exg_1772262000.csv","session_start_utc":1772262000,"session_end_utc":1772265600}"#,
        )
        .unwrap();
        std::fs::write(dir28.join("exg_1772262000.csv"), "t,v\n").unwrap();

        // Session B: in UTC dir 20260301, at March 1 08:00 UTC → March 1 01:00 PDT
        let dir01 = skill.join("20260301");
        std::fs::create_dir_all(&dir01).unwrap();
        std::fs::write(
            dir01.join("exg_1772352000.json"),
            r#"{"csv_file":"exg_1772352000.csv","session_start_utc":1772352000,"session_end_utc":1772355600}"#,
        )
        .unwrap();
        std::fs::write(dir01.join("exg_1772352000.csv"), "t,v\n").unwrap();

        // Local March 1 PDT: spans March 1 07:00 UTC → March 2 07:00 UTC
        // Session A (Feb 28 23:00 UTC) is before → NOT in local March 1
        // Session B (March 1 08:00 UTC) is inside → in local March 1
        let sessions = list_sessions_for_local_day("2026-03-01", TZ_PDT, skill, None);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_start_utc, Some(1_772_352_000));

        // Local Feb 28 PDT: spans Feb 28 07:00 UTC → March 1 07:00 UTC
        // Session A is inside; Session B is after
        let sessions = list_sessions_for_local_day("2026-02-28", TZ_PDT, skill, None);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_start_utc, Some(1_772_262_000));
    }

    #[test]
    fn utc_timezone_no_shift() {
        let tmp = tempfile::tempdir().unwrap();
        let skill = tmp.path();

        let day_dir = skill.join("20260301");
        std::fs::create_dir_all(&day_dir).unwrap();
        std::fs::write(
            day_dir.join("exg_1772323200.json"),
            r#"{"csv_file":"exg_1772323200.csv","session_start_utc":1772323200,"session_end_utc":1772326800}"#,
        )
        .unwrap();
        std::fs::write(day_dir.join("exg_1772323200.csv"), "t,v\n").unwrap();

        let sessions = list_sessions_for_local_day("2026-03-01", TZ_UTC, skill, None);
        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn extreme_positive_offset_lint() {
        // UTC+14: March 1 00:00 UTC = March 1 14:00 LINT (same day)
        assert_eq!(utc_to_local_date_key(MAR01_MIDNIGHT_UTC, TZ_LINT), "2026-03-01");
        // But March 1 12:00 UTC = March 2 02:00 LINT (next day)
        assert_eq!(utc_to_local_date_key(MAR01_MIDNIGHT_UTC + 43200, TZ_LINT), "2026-03-02");
    }

    #[test]
    fn local_days_info_has_correct_bounds() {
        let tmp = tempfile::tempdir().unwrap();
        let skill = tmp.path();
        let day_dir = skill.join("20260301");
        std::fs::create_dir_all(&day_dir).unwrap();
        std::fs::write(
            day_dir.join("exg_1772352000.json"),
            r#"{"csv_file":"exg_1772352000.csv","session_start_utc":1772352000,"session_end_utc":1772355600}"#,
        )
        .unwrap();
        std::fs::write(day_dir.join("exg_1772352000.csv"), "t,v\n").unwrap();

        let days = list_local_session_days(skill, TZ_PDT);
        let mar1 = days.iter().find(|d| d.key == "2026-03-01");
        assert!(mar1.is_some(), "missing 2026-03-01 in days: {:?}", days);
        let info = mar1.unwrap();
        assert_eq!(info.end_utc - info.start_utc, 86400);
        // March 1 00:00 PDT = March 1 07:00 UTC
        assert_eq!(info.start_utc, MAR01_MIDNIGHT_UTC + 25200);
    }
}
