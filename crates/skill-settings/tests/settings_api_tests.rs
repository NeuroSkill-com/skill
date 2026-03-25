// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

use std::path::Path;

use skill_settings::{default_storage_format, default_ws_host, default_ws_port, settings_path, OpenBciBoard};

#[test]
fn settings_path_uses_settings_file_name() {
    let p = settings_path(Path::new("/tmp/neuroskill-test"));
    assert!(p.ends_with("settings.json"));
}

#[test]
fn websocket_defaults_are_stable() {
    assert_eq!(default_ws_host(), "127.0.0.1");
    assert_eq!(default_ws_port(), 8375);
}

#[test]
fn storage_default_is_csv() {
    assert_eq!(default_storage_format(), "csv");
}

#[test]
fn openbci_board_sample_rates_match_contract() {
    assert_eq!(OpenBciBoard::Ganglion.sample_rate(), 200.0);
    assert_eq!(OpenBciBoard::Cyton.sample_rate(), 250.0);
    assert_eq!(OpenBciBoard::CytonWifi.sample_rate(), 1000.0);
}
