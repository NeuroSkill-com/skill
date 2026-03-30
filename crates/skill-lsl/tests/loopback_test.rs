// SPDX-License-Identifier: GPL-3.0-only
//! LSL loopback test: create outlet → adapter → pull samples via DeviceAdapter.
#![allow(clippy::unwrap_used, clippy::panic)]

use rlsl::prelude::*;
use rlsl::types::ChannelFormat;
use skill_devices::session::{DeviceAdapter, DeviceEvent};
use std::time::Duration;

/// Create an adapter and verify the Connected event arrives immediately.
#[tokio::test]
async fn adapter_sends_connected_event() {
    let mut adapter = tokio::task::spawn_blocking(|| {
        let info = StreamInfo::new(
            "ConnectTest",
            "EEG",
            4,
            256.0,
            ChannelFormat::Float32,
            "test-connect-001",
        );
        skill_lsl::LslAdapter::new(&info)
    })
    .await
    .unwrap();

    // The Connected event is sent by the inlet thread immediately
    let evt = tokio::time::timeout(Duration::from_secs(2), adapter.next_event())
        .await
        .expect("should receive event within 2s");

    match evt {
        Some(DeviceEvent::Connected(info)) => {
            assert!(info.name.contains("ConnectTest"));
            assert_eq!(info.hardware_version.as_deref(), Some("EEG"));
        }
        other => panic!("expected Connected, got {other:?}"),
    }
}

/// Verify read_channel_labels via the adapter's descriptor.
#[test]
fn loopback_partial_labels_padded() {
    std::thread::spawn(|| {
        let info = StreamInfo::new("PartialLabels", "EEG", 6, 500.0, ChannelFormat::Float32, "test-partial");
        let desc = info.desc();
        let channels = desc.append_child("channels");
        // Only label 3 of 6 channels
        for label in &["Cz", "Pz", "Oz"] {
            let ch = channels.append_child("channel");
            ch.append_child_value("label", label);
        }

        let adapter = skill_lsl::LslAdapter::new(&info);
        let d = adapter.descriptor();
        assert_eq!(d.channel_names.len(), 6);
        assert_eq!(d.channel_names[0], "Cz");
        assert_eq!(d.channel_names[1], "Pz");
        assert_eq!(d.channel_names[2], "Oz");
        // Remaining should be auto-generated
        assert_eq!(d.channel_names[3], "Ch4");
        assert_eq!(d.channel_names[4], "Ch5");
        assert_eq!(d.channel_names[5], "Ch6");
    })
    .join()
    .unwrap();
}
