// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

use std::fs;
use std::path::PathBuf;

use skill_label_index::{mean_eeg_for_window, LabelIndexState};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("skill-label-index-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&p).expect("create temp dir");
    p
}

#[test]
fn mean_eeg_for_window_returns_none_when_no_data_present() {
    let dir = tmp_dir("empty");
    let got = mean_eeg_for_window(&dir, 1_700_000_000, 1_700_000_100);
    assert!(got.is_none());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn label_index_state_load_initializes_three_indices() {
    let dir = tmp_dir("state-load");
    let state = LabelIndexState::new();

    state.load(&dir);

    assert!(state.text.lock().expect("text lock").is_some());
    assert!(state.context.lock().expect("context lock").is_some());
    assert!(state.eeg.lock().expect("eeg lock").is_some());

    let _ = fs::remove_dir_all(dir);
}
