// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

use std::fs;
use std::path::PathBuf;

use skill_history::{find_metrics_path, find_ppg_path};

fn tmp_case_dir(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let unique = format!(
        "skill-history-{name}-{}-{}",
        std::process::id(),
        chrono_like_now_nanos()
    );
    p.push(unique);
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).expect("create temp dir");
    p
}

fn chrono_like_now_nanos() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[test]
fn finds_metrics_csv_for_exg_session() {
    let dir = tmp_case_dir("metrics-csv");
    let eeg = dir.join("exg_1700000000.csv");
    let metrics = dir.join("exg_1700000000_metrics.csv");

    fs::write(&eeg, "").expect("write eeg");
    fs::write(&metrics, "").expect("write metrics");

    let found = find_metrics_path(&eeg).expect("metrics path");
    assert_eq!(found, metrics);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn finds_ppg_csv_for_legacy_muse_session() {
    let dir = tmp_case_dir("ppg-muse");
    let eeg = dir.join("muse_1700000001.csv");
    let ppg = dir.join("muse_1700000001_ppg.csv");

    fs::write(&eeg, "").expect("write eeg");
    fs::write(&ppg, "").expect("write ppg");

    let found = find_ppg_path(&eeg).expect("ppg path");
    assert_eq!(found, ppg);

    let _ = fs::remove_dir_all(dir);
}
