// SPDX-License-Identifier: GPL-3.0-only
//! LSL loopback test: create outlet → adapter → pull samples via DeviceAdapter.
#![allow(clippy::unwrap_used, clippy::panic)]

use rlsl::prelude::*;
use rlsl::types::ChannelFormat;
use skill_devices::session::{DeviceAdapter, DeviceEvent};
use std::time::Duration;

/// Create an outlet then an adapter and verify the Connected event arrives.
///
/// Runs on a dedicated OS thread (not a tokio spawn_blocking) so that:
///  - rlsl's internal runtime doesn't conflict with the test runtime, and
///  - the outlet stays alive through the entire connection handshake.
#[tokio::test]
async fn adapter_sends_connected_event() {
    let (tx, rx) = tokio::sync::oneshot::channel();

    std::thread::Builder::new()
        .name("lsl-loopback-test".into())
        .spawn(move || {
            let info = StreamInfo::new(
                "ConnectTest",
                "EEG",
                4,
                256.0,
                ChannelFormat::Float32,
                "test-connect-001",
            );
            // Outlet must exist before the adapter so the inlet can resolve it.
            // Keep it alive until after the Connected event is received.
            let _outlet = rlsl::outlet::StreamOutlet::new(&info, 0, 360);
            let mut adapter = skill_lsl::LslAdapter::new(&info);

            // Drive next_event() synchronously — we're on a plain OS thread.
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build runtime");
            let evt = rt.block_on(async { tokio::time::timeout(Duration::from_secs(15), adapter.next_event()).await });
            let _ = tx.send(evt);
            // Keep _outlet alive until the event is sent.
        })
        .expect("spawn thread");

    let result = tokio::time::timeout(Duration::from_secs(20), rx)
        .await
        .expect("thread did not complete within 20s")
        .expect("channel closed");

    let evt = result.expect("should receive event within 15s");
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
