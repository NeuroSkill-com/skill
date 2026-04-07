// SPDX-License-Identifier: GPL-3.0-only
//! Tests for StatusResponse lifecycle (connect, disconnect, clear).
#![allow(clippy::unwrap_used)]

use skill_daemon_common::StatusResponse;

#[test]
fn default_is_disconnected() {
    let s = StatusResponse::default();
    assert_eq!(s.state, "");
    assert!(s.device_name.is_none());
    assert!(s.device_kind.is_empty());
    assert!(s.device_id.is_none());
    assert_eq!(s.sample_count, 0);
    assert_eq!(s.eeg_channel_count, 0);
    assert_eq!(s.eeg_sample_rate_hz, 0.0);
    assert!(s.channel_names.is_empty());
    assert!(!s.has_ppg);
    assert!(!s.has_imu);
}

#[test]
fn clear_device_resets_all_fields() {
    let mut s = StatusResponse {
        state: "connected".into(),
        device_name: Some("TestDevice".into()),
        device_kind: "lsl".into(),
        device_id: Some("test-id".into()),
        sample_count: 10000,
        battery: 75.0,
        eeg_channel_count: 32,
        eeg_sample_rate_hz: 256.0,
        channel_names: (1..=32).map(|i| format!("Ch{i}")).collect(),
        channel_quality: vec!["good".into(); 32],
        serial_number: Some("SN-123".into()),
        mac_address: Some("AA:BB:CC".into()),
        hardware_version: Some("v2".into()),
        has_ppg: true,
        has_imu: true,
        ppg_sample_count: 500,
        csv_path: Some("/tmp/test.csv".into()),
        ..Default::default()
    };

    s.clear_device();

    assert_eq!(s.state, "disconnected");
    assert!(s.device_name.is_none());
    assert!(s.device_kind.is_empty());
    assert!(s.device_id.is_none());
    assert_eq!(s.sample_count, 0);
    assert_eq!(s.battery, 0.0);
    assert_eq!(s.eeg_channel_count, 0);
    assert_eq!(s.eeg_sample_rate_hz, 0.0);
    assert!(s.channel_names.is_empty());
    assert!(s.channel_quality.is_empty());
    assert!(s.serial_number.is_none());
    assert!(s.mac_address.is_none());
    assert!(s.hardware_version.is_none());
    assert!(!s.has_ppg);
    assert!(!s.has_imu);
    assert_eq!(s.ppg_sample_count, 0);
    assert!(s.csv_path.is_none());
}

#[test]
fn clear_device_preserves_paired_devices() {
    let mut s = StatusResponse {
        state: "connected".into(),
        paired_devices: vec![skill_daemon_common::PairedDeviceResponse {
            id: "dev-1".into(),
            name: "Muse".into(),
            last_seen: 1000,
        }],
        ..Default::default()
    };

    s.clear_device();

    // paired_devices should NOT be cleared — they persist across sessions
    assert_eq!(s.paired_devices.len(), 1);
    assert_eq!(s.paired_devices[0].name, "Muse");
}

#[test]
fn json_roundtrip_preserves_all_fields() {
    let s = StatusResponse {
        state: "connected".into(),
        device_name: Some("SkillVirtualEEG".into()),
        device_kind: "lsl".into(),
        device_id: Some("virtual-001".into()),
        sample_count: 256000,
        battery: 0.0,
        eeg_channel_count: 32,
        eeg_sample_rate_hz: 256.0,
        channel_names: (1..=32).map(|i| format!("Ch{i}")).collect(),
        channel_quality: vec!["good".into(); 32],
        has_ppg: false,
        has_imu: false,
        csv_path: Some("/path/to/session.csv".into()),
        ..Default::default()
    };

    let json = serde_json::to_string(&s).unwrap();
    let parsed: StatusResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.state, "connected");
    assert_eq!(parsed.device_name.as_deref(), Some("SkillVirtualEEG"));
    assert_eq!(parsed.device_kind, "lsl");
    assert_eq!(parsed.eeg_channel_count, 32);
    assert_eq!(parsed.eeg_sample_rate_hz, 256.0);
    assert_eq!(parsed.channel_names.len(), 32);
    assert_eq!(parsed.channel_quality.len(), 32);
    assert_eq!(parsed.csv_path.as_deref(), Some("/path/to/session.csv"));
}

#[test]
fn json_deserialization_defaults_missing_fields() {
    // Simulate an older daemon that only sends core fields
    let json = r#"{
        "state": "connected",
        "device_name": "OldDevice",
        "sample_count": 100,
        "battery": 50.0,
        "retry_attempt": 0,
        "retry_countdown_secs": 0,
        "paired_devices": []
    }"#;

    let parsed: StatusResponse = serde_json::from_str(json).unwrap();

    assert_eq!(parsed.state, "connected");
    assert_eq!(parsed.device_name.as_deref(), Some("OldDevice"));
    // New fields should default gracefully
    assert_eq!(parsed.eeg_channel_count, 0);
    assert_eq!(parsed.eeg_sample_rate_hz, 0.0);
    assert!(parsed.channel_names.is_empty());
    assert!(!parsed.has_ppg);
    assert!(parsed.device_kind.is_empty());
}
