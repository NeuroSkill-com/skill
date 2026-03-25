// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

use std::fs;
use std::path::PathBuf;

use skill_history::{list_session_days, list_sessions_for_day};

fn temp_root(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("skill-history-prefix-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&p).expect("create temp root");
    p
}

#[test]
fn list_session_days_accepts_exg_and_legacy_muse_prefixes() {
    let root = temp_root("days");
    let d1 = root.join("20260101");
    let d2 = root.join("20260102");
    fs::create_dir_all(&d1).expect("d1");
    fs::create_dir_all(&d2).expect("d2");

    fs::write(d1.join("exg_1700000000.json"), "{}").expect("write exg json");
    fs::write(d2.join("muse_1700000001.json"), "{}").expect("write muse json");

    let days = list_session_days(&root);
    assert_eq!(days, vec!["20260102".to_string(), "20260101".to_string()]);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn list_sessions_for_day_reads_both_exg_and_muse_sidecars() {
    let root = temp_root("sessions-sidecar");
    let day = "20260103";
    let day_dir = root.join(day);
    fs::create_dir_all(&day_dir).expect("day dir");

    fs::write(day_dir.join("exg_1700000100.csv"), "").expect("exg csv");
    fs::write(day_dir.join("muse_1700000200.csv"), "").expect("muse csv");

    fs::write(
        day_dir.join("exg_1700000100.json"),
        r#"{"csv_file":"exg_1700000100.csv","session_start_utc":1700000100,"session_end_utc":1700000199}"#,
    )
    .expect("exg sidecar");
    fs::write(
        day_dir.join("muse_1700000200.json"),
        r#"{"csv_file":"muse_1700000200.csv","session_start_utc":1700000200,"session_end_utc":1700000299}"#,
    )
    .expect("muse sidecar");

    let sessions = list_sessions_for_day(day, &root, None);
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().any(|s| s.csv_file.starts_with("exg_")));
    assert!(sessions.iter().any(|s| s.csv_file.starts_with("muse_")));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn list_sessions_for_day_accepts_orphan_exg_and_muse_csv() {
    let root = temp_root("sessions-orphan");
    let day = "20260104";
    let day_dir = root.join(day);
    fs::create_dir_all(&day_dir).expect("day dir");

    fs::write(day_dir.join("exg_1700000300.csv"), "").expect("exg csv");
    fs::write(day_dir.join("muse_1700000400.csv"), "").expect("muse csv");

    let sessions = list_sessions_for_day(day, &root, None);
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().any(|s| s.csv_file == "exg_1700000300.csv"));
    assert!(sessions.iter().any(|s| s.csv_file == "muse_1700000400.csv"));

    let _ = fs::remove_dir_all(root);
}
