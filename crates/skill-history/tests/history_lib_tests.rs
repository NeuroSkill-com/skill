// SPDX-License-Identifier: GPL-3.0-only
//! Tests for skill-history session listing, helpers, and CSV timestamp parsing.

use skill_history::{delete_session, get_history_stats, list_session_days, list_sessions_for_day};
use std::fs;
use tempfile::tempdir;

// ── Helper: write a session JSON sidecar ─────────────────────────────────────

fn write_session_json(dir: &std::path::Path, ts: u64, duration: u64) {
    let csv_name = format!("exg_{ts}.csv");
    let json_name = format!("exg_{ts}.json");
    let meta = serde_json::json!({
        "csv_file": csv_name,
        "session_start_utc": ts,
        "session_end_utc": ts + duration,
        "session_duration_s": duration,
        "device": {
            "name": "Muse-2",
            "id": "00:55:DA:B0:00:01",
            "serial_number": "TEST-001"
        },
        "total_samples": duration * 256,
        "sample_rate_hz": 256
    });
    fs::write(dir.join(&json_name), serde_json::to_string(&meta).unwrap()).unwrap();
    // Create a dummy CSV file so it counts as a real session.
    fs::write(dir.join(&csv_name), "t,ch0\n1700000000,0.1\n").unwrap();
}

fn write_legacy_session(dir: &std::path::Path, ts: u64) {
    let csv_name = format!("muse_{ts}.csv");
    let json_name = format!("muse_{ts}.json");
    let meta = serde_json::json!({
        "csv_file": csv_name,
        "session_start_utc": ts,
        "session_end_utc": ts + 600,
        "device_name": "Muse-S",
        "battery_pct": 85.0
    });
    fs::write(dir.join(&json_name), serde_json::to_string(&meta).unwrap()).unwrap();
    fs::write(dir.join(&csv_name), "t,ch0\n").unwrap();
}

// ── list_session_days ────────────────────────────────────────────────────────

#[test]
fn list_session_days_empty_dir() {
    let dir = tempdir().unwrap();
    let days = list_session_days(dir.path());
    assert!(days.is_empty());
}

#[test]
fn list_session_days_finds_valid_days() {
    let dir = tempdir().unwrap();
    let d1 = dir.path().join("20260101");
    let d2 = dir.path().join("20260315");
    fs::create_dir_all(&d1).unwrap();
    fs::create_dir_all(&d2).unwrap();
    write_session_json(&d1, 1735689600, 3600);
    write_session_json(&d2, 1742025600, 1800);

    let days = list_session_days(dir.path());
    assert_eq!(days.len(), 2);
    // Newest first
    assert_eq!(days[0], "20260315");
    assert_eq!(days[1], "20260101");
}

#[test]
fn list_session_days_ignores_non_date_dirs() {
    let dir = tempdir().unwrap();
    // Invalid directory names
    fs::create_dir_all(dir.path().join("notes")).unwrap();
    fs::create_dir_all(dir.path().join("1234")).unwrap();
    fs::create_dir_all(dir.path().join("abcdefgh")).unwrap();
    // Valid but empty (no session files)
    fs::create_dir_all(dir.path().join("20260101")).unwrap();

    let days = list_session_days(dir.path());
    assert!(days.is_empty());
}

#[test]
fn list_session_days_orphan_csv_counts() {
    let dir = tempdir().unwrap();
    let d = dir.path().join("20260201");
    fs::create_dir_all(&d).unwrap();
    // Orphan CSV without JSON sidecar
    fs::write(d.join("exg_1738368000.csv"), "t,ch0\n").unwrap();

    let days = list_session_days(dir.path());
    assert_eq!(days.len(), 1);
    assert_eq!(days[0], "20260201");
}

// ── list_sessions_for_day ────────────────────────────────────────────────────

#[test]
fn list_sessions_for_day_empty() {
    let dir = tempdir().unwrap();
    let sessions = list_sessions_for_day("20260101", dir.path(), None);
    assert!(sessions.is_empty());
}

#[test]
fn list_sessions_for_day_reads_json_metadata() {
    let dir = tempdir().unwrap();
    let d = dir.path().join("20260315");
    fs::create_dir_all(&d).unwrap();
    write_session_json(&d, 1742025600, 1800);

    let sessions = list_sessions_for_day("20260315", dir.path(), None);
    assert_eq!(sessions.len(), 1);

    let s = &sessions[0];
    assert_eq!(s.session_start_utc, Some(1742025600));
    assert_eq!(s.session_end_utc, Some(1742025600 + 1800));
    assert_eq!(s.session_duration_s, Some(1800));
    assert_eq!(s.device_name.as_deref(), Some("Muse-2"));
    assert_eq!(s.serial_number.as_deref(), Some("TEST-001"));
    assert_eq!(s.sample_rate_hz, Some(256));
}

#[test]
fn list_sessions_for_day_legacy_format() {
    let dir = tempdir().unwrap();
    let d = dir.path().join("20260101");
    fs::create_dir_all(&d).unwrap();
    write_legacy_session(&d, 1735689600);

    let sessions = list_sessions_for_day("20260101", dir.path(), None);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].device_name.as_deref(), Some("Muse-S"));
    assert_eq!(sessions[0].battery_pct, Some(85.0));
}

#[test]
fn list_sessions_for_day_multiple_sorted_newest_first() {
    let dir = tempdir().unwrap();
    let d = dir.path().join("20260315");
    fs::create_dir_all(&d).unwrap();
    write_session_json(&d, 1742025600, 1800); // earlier
    write_session_json(&d, 1742040000, 900); // later

    let sessions = list_sessions_for_day("20260315", dir.path(), None);
    assert_eq!(sessions.len(), 2);
    assert!(sessions[0].session_start_utc > sessions[1].session_start_utc);
}

#[test]
fn list_sessions_orphan_csv_without_json() {
    let dir = tempdir().unwrap();
    let d = dir.path().join("20260201");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("exg_1738368000.csv"), "t,ch0\n1738368000,0.1\n").unwrap();

    let sessions = list_sessions_for_day("20260201", dir.path(), None);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].csv_file, "exg_1738368000.csv");
    assert_eq!(sessions[0].session_start_utc, Some(1738368000));
    // No JSON metadata → device fields are None
    assert!(sessions[0].device_name.is_none());
}

#[test]
fn list_sessions_skips_corrupt_json() {
    let dir = tempdir().unwrap();
    let d = dir.path().join("20260315");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("exg_1742025600.json"), "not valid json!!!").unwrap();
    fs::write(d.join("exg_1742025600.csv"), "t,ch0\n").unwrap();

    // Corrupt JSON is skipped; CSV with .json present is NOT orphan → 0 sessions
    let sessions = list_sessions_for_day("20260315", dir.path(), None);
    assert_eq!(sessions.len(), 0);
}

#[test]
fn list_sessions_corrupt_json_without_csv_ignored() {
    let dir = tempdir().unwrap();
    let d = dir.path().join("20260315");
    fs::create_dir_all(&d).unwrap();
    // Corrupt JSON with no corresponding CSV
    fs::write(d.join("exg_1742025600.json"), "not json").unwrap();
    // Valid session alongside
    write_session_json(&d, 1742030000, 600);

    let sessions = list_sessions_for_day("20260315", dir.path(), None);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_start_utc, Some(1742030000));
}

// ── delete_session ───────────────────────────────────────────────────────────

#[test]
fn delete_session_removes_files() {
    let dir = tempdir().unwrap();
    let csv = dir.path().join("exg_1700000000.csv");
    let json = dir.path().join("exg_1700000000.json");
    let metrics = dir.path().join("exg_1700000000_metrics.csv");
    fs::write(&csv, "data").unwrap();
    fs::write(&json, "{}").unwrap();
    fs::write(&metrics, "data").unwrap();

    delete_session(csv.to_str().unwrap()).unwrap();

    assert!(!csv.exists());
    assert!(!json.exists());
    assert!(!metrics.exists());
}

#[test]
fn delete_session_missing_files_ok() {
    let dir = tempdir().unwrap();
    let csv = dir.path().join("exg_1700000000.csv");
    // Only create the CSV, not the sidecar files
    fs::write(&csv, "data").unwrap();

    let result = delete_session(csv.to_str().unwrap());
    assert!(result.is_ok());
    assert!(!csv.exists());
}

// ── get_history_stats ────────────────────────────────────────────────────────

#[test]
fn get_history_stats_empty() {
    let dir = tempdir().unwrap();
    let stats = get_history_stats(dir.path());
    assert_eq!(stats.total_sessions, 0);
    assert_eq!(stats.total_secs, 0);
}

#[test]
fn get_history_stats_counts_sessions() {
    let dir = tempdir().unwrap();
    let d1 = dir.path().join("20260101");
    let d2 = dir.path().join("20260102");
    fs::create_dir_all(&d1).unwrap();
    fs::create_dir_all(&d2).unwrap();
    write_session_json(&d1, 1735689600, 3600);
    write_session_json(&d2, 1735776000, 1800);

    let stats = get_history_stats(dir.path());
    assert_eq!(stats.total_sessions, 2);
    assert_eq!(stats.total_secs, 5400);
}
