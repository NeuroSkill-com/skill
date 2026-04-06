// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
#![allow(clippy::all)]
//! Tests for the session module: adapters, event translation, channel
//! accumulation, capability flags, descriptors.

use super::{now_secs, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo};

// ── DeviceCaps ────────────────────────────────────────────────────────────────

#[test]
fn caps_bitflags_combine() {
    let caps = DeviceCaps::EEG | DeviceCaps::PPG | DeviceCaps::IMU;
    assert!(caps.contains(DeviceCaps::EEG));
    assert!(caps.contains(DeviceCaps::PPG));
    assert!(caps.contains(DeviceCaps::IMU));
    assert!(!caps.contains(DeviceCaps::BATTERY));
    assert!(!caps.contains(DeviceCaps::META));
}

#[test]
fn caps_empty_contains_nothing() {
    let caps = DeviceCaps::empty();
    assert!(!caps.contains(DeviceCaps::EEG));
}

// ── DeviceDescriptor ──────────────────────────────────────────────────────────

#[test]
fn descriptor_pipeline_channels_capped() {
    let n = skill_constants::EEG_CHANNELS + 8; // more than the DSP limit
    let desc = DeviceDescriptor {
        kind: "test",
        caps: DeviceCaps::EEG,
        eeg_channels: n,
        eeg_sample_rate: 250.0,
        channel_names: (0..n).map(|i| format!("Ch{i}")).collect(),
        pipeline_channels: n.min(skill_constants::EEG_CHANNELS),
        ppg_channel_names: Vec::new(),
        imu_channel_names: Vec::new(),
        fnirs_channel_names: Vec::new(),
    };
    assert_eq!(desc.pipeline_channels, skill_constants::EEG_CHANNELS);
}

#[test]
fn descriptor_small_channel_count_not_capped() {
    let desc = DeviceDescriptor {
        kind: "test",
        caps: DeviceCaps::EEG,
        eeg_channels: 4,
        eeg_sample_rate: 256.0,
        channel_names: vec!["A".into(), "B".into(), "C".into(), "D".into()],
        pipeline_channels: 4_usize.min(skill_constants::EEG_CHANNELS),
        ppg_channel_names: Vec::new(),
        imu_channel_names: Vec::new(),
        fnirs_channel_names: Vec::new(),
    };
    assert_eq!(desc.pipeline_channels, 4);
}

// ── DeviceInfo default ────────────────────────────────────────────────────────

#[test]
fn device_info_default_has_empty_fields() {
    let info = DeviceInfo::default();
    assert!(info.name.is_empty());
    assert!(info.id.is_empty());
    assert!(info.serial_number.is_none());
    assert!(info.firmware_version.is_none());
}

// ── now_secs ──────────────────────────────────────────────────────────────────

#[test]
fn now_secs_returns_plausible_timestamp() {
    let t = now_secs();
    // Should be after 2024-01-01 and before 2100-01-01.
    assert!(t > 1_704_067_200.0);
    assert!(t < 4_102_444_800.0);
}

// ── Muse adapter ──────────────────────────────────────────────────────────────

mod muse_tests {
    #[allow(unused_imports)]
    use super::*;

    /// Test the channel accumulation logic directly by simulating what
    /// `translate` does for EEG events.
    #[test]
    fn channel_accumulator_aligns_frames() {
        use std::collections::VecDeque;

        // Simulate the accumulator logic from MuseAdapter.
        const MUSE_EEG_CHANNELS: usize = 4;
        let mut ch_bufs: [VecDeque<f64>; MUSE_EEG_CHANNELS] = Default::default();
        let mut pending: Vec<Vec<f64>> = Vec::new();

        // Push 3 samples for each of the 4 channels.
        let samples_per_ch: [Vec<f64>; 4] = [
            vec![1.0, 2.0, 3.0],
            vec![10.0, 20.0, 30.0],
            vec![100.0, 200.0, 300.0],
            vec![1000.0, 2000.0, 3000.0],
        ];

        for (ch, samples) in samples_per_ch.iter().enumerate() {
            ch_bufs[ch].extend(samples.iter().copied());
        }

        // Drain aligned frames.
        loop {
            let min_len = ch_bufs.iter().map(|b| b.len()).min().unwrap_or(0);
            if min_len == 0 {
                break;
            }
            for _ in 0..min_len {
                let frame: Vec<f64> = (0..MUSE_EEG_CHANNELS)
                    .map(|c| ch_bufs[c].pop_front().unwrap())
                    .collect();
                pending.push(frame);
            }
        }

        assert_eq!(pending.len(), 3);
        assert_eq!(pending[0], vec![1.0, 10.0, 100.0, 1000.0]);
        assert_eq!(pending[1], vec![2.0, 20.0, 200.0, 2000.0]);
        assert_eq!(pending[2], vec![3.0, 30.0, 300.0, 3000.0]);
    }

    /// Channel accumulator does not emit frames when channels are uneven.
    #[test]
    fn channel_accumulator_waits_for_all_channels() {
        use std::collections::VecDeque;

        const MUSE_EEG_CHANNELS: usize = 4;
        let mut ch_bufs: [VecDeque<f64>; MUSE_EEG_CHANNELS] = Default::default();

        // Only push to channels 0 and 1.
        ch_bufs[0].extend([1.0, 2.0].iter());
        ch_bufs[1].extend([10.0, 20.0].iter());

        let min_len = ch_bufs.iter().map(|b| b.len()).min().unwrap_or(0);
        assert_eq!(min_len, 0); // Channels 2 and 3 are empty → no frames.
    }

    /// Channel accumulator handles partial overlap correctly.
    #[test]
    fn channel_accumulator_partial_then_complete() {
        use std::collections::VecDeque;

        const N: usize = 4;
        let mut ch_bufs: [VecDeque<f64>; N] = Default::default();
        let mut frames: Vec<Vec<f64>> = Vec::new();

        // First batch: only channels 0-2 get data.
        ch_bufs[0].extend([1.0, 2.0].iter());
        ch_bufs[1].extend([10.0, 20.0].iter());
        ch_bufs[2].extend([100.0, 200.0].iter());

        let min_len = ch_bufs.iter().map(|b| b.len()).min().unwrap_or(0);
        assert_eq!(min_len, 0); // Can't drain yet.

        // Second batch: channel 3 arrives.
        ch_bufs[3].extend([1000.0, 2000.0].iter());

        // Now drain.
        loop {
            let min_len = ch_bufs.iter().map(|b| b.len()).min().unwrap_or(0);
            if min_len == 0 {
                break;
            }
            for _ in 0..min_len {
                let frame: Vec<f64> = (0..N).map(|c| ch_bufs[c].pop_front().unwrap()).collect();
                frames.push(frame);
            }
        }

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], vec![1.0, 10.0, 100.0, 1000.0]);
        assert_eq!(frames[1], vec![2.0, 20.0, 200.0, 2000.0]);
    }

    /// Out-of-range electrode index is ignored.
    #[test]
    fn channel_accumulator_ignores_out_of_range_electrode() {
        use std::collections::VecDeque;

        const N: usize = 4;
        let mut ch_bufs: [VecDeque<f64>; N] = Default::default();

        // Electrode 5 is out of range for 4-channel Muse.
        let electrode = 5;
        if electrode < N {
            ch_bufs[electrode].extend([99.0].iter());
        }
        // Nothing should be buffered.
        assert!(ch_bufs.iter().all(|b| b.is_empty()));
    }
}

// ── MW75 adapter ──────────────────────────────────────────────────────────────

mod mw75_tests {
    use super::*;
    use crate::session::mw75::Mw75Adapter;
    use mw75::prelude::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn mw75_translates_eeg_event() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(Mw75Event::Eeg(mw75::types::EegPacket {
            timestamp: 1700000000.0,
            event_id: 239,
            counter: 1,
            ref_value: 0.0,
            drl: 0.0,
            channels: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0],
            checksum_valid: true,
            feature_status: 0,
        }))
        .await
        .unwrap();
        drop(tx);

        let mut adapter = Mw75Adapter::new_for_test(rx, None);

        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Eeg(frame) => {
                assert_eq!(frame.channels.len(), 12);
                assert_eq!(frame.timestamp_s, 1700000000.0);
                assert!((frame.channels[0] - 1.0).abs() < f64::EPSILON);
                assert!((frame.channels[11] - 12.0).abs() < f64::EPSILON);
            }
            other => panic!("expected Eeg, got {other:?}"),
        }

        // Channel closed → None.
        assert!(adapter.next_event().await.is_none());
    }

    #[tokio::test]
    async fn mw75_translates_battery_event() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(Mw75Event::Battery(mw75::types::BatteryInfo { level: 75 }))
            .await
            .unwrap();
        drop(tx);

        let mut adapter = Mw75Adapter::new_for_test(rx, None);

        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Battery(b) => {
                assert!((b.level_pct - 75.0).abs() < f32::EPSILON);
                assert!(b.voltage_mv.is_none());
            }
            other => panic!("expected Battery, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn mw75_connected_event() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(Mw75Event::Connected("MW75 Neuro".to_string())).await.unwrap();
        drop(tx);

        let mut adapter = Mw75Adapter::new_for_test(rx, None);

        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Connected(info) => {
                assert_eq!(info.name, "MW75 Neuro");
            }
            other => panic!("expected Connected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn mw75_disconnected_event() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(Mw75Event::Disconnected).await.unwrap();
        drop(tx);

        let mut adapter = Mw75Adapter::new_for_test(rx, None);

        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[tokio::test]
    async fn mw75_initial_connected_info() {
        let (tx, rx) = mpsc::channel(16);
        drop(tx);

        let info = DeviceInfo {
            name: "MW75 Test".into(),
            id: "AA:BB:CC".into(),
            ..Default::default()
        };
        let mut adapter = Mw75Adapter::new_for_test(rx, Some(info));

        // Should get the synthetic Connected event first.
        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Connected(info) => {
                assert_eq!(info.name, "MW75 Test");
                assert_eq!(info.id, "AA:BB:CC");
            }
            other => panic!("expected Connected, got {other:?}"),
        }

        // Then None (channel closed).
        assert!(adapter.next_event().await.is_none());
    }

    #[tokio::test]
    async fn mw75_activation_event_skipped() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(Mw75Event::Activated(mw75::types::ActivationStatus {
            eeg_enabled: true,
            raw_mode_enabled: true,
        }))
        .await
        .unwrap();
        tx.send(Mw75Event::Disconnected).await.unwrap();
        drop(tx);

        let mut adapter = Mw75Adapter::new_for_test(rx, None);

        // Activation is skipped, next should be Disconnected.
        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[test]
    fn mw75_descriptor_correct() {
        let (_, rx) = mpsc::channel(16);
        let adapter = Mw75Adapter::new_for_test(rx, None);

        let desc = adapter.descriptor();
        assert_eq!(desc.kind, "mw75");
        assert_eq!(desc.eeg_channels, 12);
        assert!((desc.eeg_sample_rate - 500.0).abs() < f64::EPSILON);
        assert!(desc.caps.contains(DeviceCaps::EEG));
        assert!(desc.caps.contains(DeviceCaps::BATTERY));
        assert!(!desc.caps.contains(DeviceCaps::PPG));
        assert!(!desc.caps.contains(DeviceCaps::IMU));
    }
}

// ── Hermes adapter ────────────────────────────────────────────────────────────

mod hermes_tests {
    use super::*;
    use crate::session::hermes::HermesAdapter;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn hermes_translates_eeg_event() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(hermes_ble::prelude::HermesEvent::Eeg(hermes_ble::types::EegSample {
            packet_index: 0,
            sample_index: 0,
            timestamp: 5000.0, // ms
            channels: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
        }))
        .await
        .unwrap();
        drop(tx);

        let mut adapter = HermesAdapter::new_for_test(rx);

        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Eeg(frame) => {
                assert_eq!(frame.channels.len(), 8);
                assert!((frame.timestamp_s - 5.0).abs() < f64::EPSILON); // 5000ms → 5s
                assert!((frame.channels[0] - 1.0).abs() < f64::EPSILON);
                assert!((frame.channels[7] - 8.0).abs() < f64::EPSILON);
            }
            other => panic!("expected Eeg, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn hermes_translates_motion_event() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(hermes_ble::prelude::HermesEvent::Motion(
            hermes_ble::types::MotionData {
                timestamp: 1000.0,
                accel: hermes_ble::types::XyzSample { x: 0.1, y: 0.2, z: 9.8 },
                gyro: hermes_ble::types::XyzSample { x: 1.0, y: 2.0, z: 3.0 },
                mag: hermes_ble::types::XyzSample { x: 0.5, y: 0.5, z: 0.5 },
            },
        ))
        .await
        .unwrap();
        drop(tx);

        let mut adapter = HermesAdapter::new_for_test(rx);

        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Imu(imu) => {
                assert!((imu.accel[2] - 9.8).abs() < 0.01);
                assert!(imu.gyro.is_some());
                assert!((imu.gyro.unwrap()[0] - 1.0).abs() < f32::EPSILON);
                assert!(imu.mag.is_some());
            }
            other => panic!("expected Imu, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn hermes_packets_dropped_skipped() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(hermes_ble::prelude::HermesEvent::PacketsDropped(5))
            .await
            .unwrap();
        tx.send(hermes_ble::prelude::HermesEvent::Disconnected).await.unwrap();
        drop(tx);

        let mut adapter = HermesAdapter::new_for_test(rx);

        // PacketsDropped is not forwarded, should get Disconnected.
        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[test]
    fn hermes_descriptor_correct() {
        let (_, rx) = mpsc::channel(16);
        let adapter = HermesAdapter::new_for_test(rx);

        let desc = adapter.descriptor();
        assert_eq!(desc.kind, "hermes");
        assert_eq!(desc.eeg_channels, 8);
        assert!((desc.eeg_sample_rate - 250.0).abs() < f64::EPSILON);
        assert!(desc.caps.contains(DeviceCaps::EEG));
        assert!(desc.caps.contains(DeviceCaps::IMU));
        assert!(!desc.caps.contains(DeviceCaps::PPG));
        assert!(!desc.caps.contains(DeviceCaps::BATTERY));
    }
}

// ── Emotiv adapter ────────────────────────────────────────────────────────────

mod emotiv_tests {
    use super::*;
    use crate::session::emotiv::EmotivAdapter;
    use emotiv::prelude::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn emotiv_translates_eeg_event() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(CortexEvent::SessionCreated("ses-1".into())).await.unwrap();
        tx.send(CortexEvent::Eeg(EegData {
            samples: vec![1.0, 2.0, 3.0, 4.0, 5.0],
            time: 1700000000.0,
        }))
        .await
        .unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        // First: Connected from SessionCreated
        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Connected(_)));

        // Second: EEG
        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Eeg(frame) => {
                assert_eq!(frame.channels.len(), 5);
                assert!((frame.timestamp_s - 1700000000.0).abs() < f64::EPSILON);
            }
            other => panic!("expected Eeg, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn emotiv_stop_all_streams_triggers_disconnect() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(CortexEvent::Warning {
            code: emotiv::protocol::CORTEX_STOP_ALL_STREAMS,
            message: serde_json::Value::Null,
        })
        .await
        .unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[tokio::test]
    async fn emotiv_close_session_triggers_disconnect() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(CortexEvent::Warning {
            code: emotiv::protocol::CORTEX_CLOSE_SESSION,
            message: serde_json::Value::Null,
        })
        .await
        .unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[tokio::test]
    async fn emotiv_error_triggers_disconnect() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(CortexEvent::Error("connection lost".into())).await.unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[tokio::test]
    async fn emotiv_other_warnings_ignored() {
        let (tx, rx) = mpsc::channel(16);

        // HEADSET_CONNECTED = 104 — informational, not a disconnect
        tx.send(CortexEvent::Warning {
            code: emotiv::protocol::HEADSET_CONNECTED,
            message: serde_json::Value::Null,
        })
        .await
        .unwrap();
        // Follow with a real disconnect to terminate
        tx.send(CortexEvent::Disconnected).await.unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        // The warning should be skipped; first real event should be Disconnected
        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[tokio::test]
    async fn emotiv_cortex_disconnected_triggers_disconnect() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(CortexEvent::Disconnected).await.unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[tokio::test]
    async fn emotiv_headset_disconnected_warning_triggers_disconnect() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(CortexEvent::Warning {
            code: emotiv::protocol::HEADSET_DISCONNECTED,
            message: serde_json::Value::String("INSIGHT-ABC".into()),
        })
        .await
        .unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[tokio::test]
    async fn emotiv_headset_connection_failed_triggers_disconnect() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(CortexEvent::Warning {
            code: emotiv::protocol::HEADSET_CONNECTION_FAILED,
            message: serde_json::Value::String("EPOCX-123".into()),
        })
        .await
        .unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[tokio::test]
    async fn emotiv_subscribe_error_does_not_disconnect() {
        let (tx, rx) = mpsc::channel(16);

        // Subscribe failure should NOT trigger disconnect.
        tx.send(CortexEvent::Error("Subscribe 'eeg' failed: code=123 no license".into()))
            .await
            .unwrap();
        // Follow with a real disconnect to verify the subscribe error was skipped.
        tx.send(CortexEvent::Disconnected).await.unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        // Should get Disconnected from the explicit CortexEvent::Disconnected,
        // NOT from the subscribe error.
        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[tokio::test]
    async fn emotiv_channel_closed_returns_none() {
        let (tx, rx) = mpsc::channel(16);
        drop(tx); // Close channel immediately.

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        // Closed channel → None.
        assert!(adapter.next_event().await.is_none());
    }

    #[tokio::test]
    async fn emotiv_non_data_events_skipped_gracefully() {
        let (tx, rx) = mpsc::channel(16);

        // Send a bunch of non-data events that produce no DeviceEvent.
        tx.send(CortexEvent::Connected).await.unwrap();
        tx.send(CortexEvent::Authorized).await.unwrap();
        tx.send(CortexEvent::Metrics(emotiv::types::MetricsData {
            values: vec![0.5; 13],
            time: 100.0,
        }))
        .await
        .unwrap();
        tx.send(CortexEvent::BandPower(emotiv::types::BandPowerData {
            powers: vec![1.0; 5],
            time: 100.0,
        }))
        .await
        .unwrap();
        // End with an actual data event.
        tx.send(CortexEvent::Disconnected).await.unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        // All non-data events should be skipped; first visible event is Disconnected.
        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[tokio::test]
    async fn emotiv_motion_translates_to_imu() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(CortexEvent::Motion(emotiv::types::MotionData {
            samples: vec![0.0, 0.0, 0.5, 0.3, 0.2, 0.1, 0.01, 0.02, -1.0, 50.0, 30.0, 20.0],
            time: 100.0,
        }))
        .await
        .unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Imu(imu) => {
                assert!((imu.accel[0] - 0.01).abs() < f32::EPSILON);
                assert!((imu.accel[1] - 0.02).abs() < f32::EPSILON);
                assert!((imu.accel[2] - (-1.0)).abs() < f32::EPSILON);
                assert!(imu.gyro.is_none()); // Cortex has quaternions, not raw gyro
                assert!(imu.mag.is_some());
                let mag = imu.mag.unwrap();
                assert!((mag[0] - 50.0).abs() < f32::EPSILON);
            }
            other => panic!("expected Imu, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn emotiv_dev_translates_to_battery() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(CortexEvent::Dev(emotiv::types::DevData {
            signal: 2.0,
            contact_quality: vec![4.0, 4.0, 3.0, 4.0, 4.0],
            battery_percent: 72.0,
            time: 100.0,
        }))
        .await
        .unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Battery(b) => {
                assert!((b.level_pct - 72.0).abs() < f32::EPSILON);
                assert!(b.voltage_mv.is_none());
            }
            other => panic!("expected Battery, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn emotiv_data_labels_updates_descriptor() {
        let (tx, rx) = mpsc::channel(16);

        tx.send(CortexEvent::DataLabels(emotiv::types::DataLabels {
            stream_name: "eeg".into(),
            labels: vec![
                "COUNTER".into(),
                "INTERPOLATED".into(),
                "AF3".into(),
                "AF4".into(),
                "T7".into(),
                "T8".into(),
                "Pz".into(),
                "RAW_CQ".into(),
                "MARKERS".into(),
            ],
        }))
        .await
        .unwrap();
        // Follow with an EEG frame to verify electrode_indices are used.
        tx.send(CortexEvent::Eeg(EegData {
            samples: vec![99.0, 88.0, 1.0, 2.0, 3.0, 4.0, 5.0, 77.0, 66.0],
            time: 100.0,
        }))
        .await
        .unwrap();
        drop(tx);

        // Start with 14 channels, DataLabels should correct to 5.
        let names: Vec<String> = (0..14).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 14, names);

        // DataLabels produces no DeviceEvent, skip straight to EEG.
        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Eeg(frame) => {
                // Should have 5 electrode values (AF3, AF4, T7, T8, Pz),
                // NOT the full 9-element raw array.
                assert_eq!(frame.channels.len(), 5);
                assert!((frame.channels[0] - 1.0).abs() < f64::EPSILON); // AF3
                assert!((frame.channels[4] - 5.0).abs() < f64::EPSILON); // Pz
            }
            other => panic!("expected Eeg, got {other:?}"),
        }

        // Descriptor should be updated.
        let desc = adapter.descriptor();
        assert_eq!(desc.eeg_channels, 5);
        assert_eq!(desc.channel_names, vec!["AF3", "AF4", "T7", "T8", "Pz"]);
    }

    #[tokio::test]
    async fn emotiv_replay_queues_events() {
        let (tx, rx) = mpsc::channel(16);
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        // Replay a DataLabels + EEG combo as would happen during connect.
        adapter.replay(vec![
            CortexEvent::DataLabels(emotiv::types::DataLabels {
                stream_name: "eeg".into(),
                labels: vec!["AF3".into(), "AF4".into(), "Pz".into()],
            }),
            CortexEvent::Eeg(EegData {
                samples: vec![10.0, 20.0, 30.0],
                time: 200.0,
            }),
        ]);

        // EEG from replay should be available.
        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Eeg(frame) => {
                assert_eq!(frame.channels.len(), 3);
            }
            other => panic!("expected Eeg, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn emotiv_double_disconnect_is_harmless() {
        let (tx, rx) = mpsc::channel(16);

        // Simulate the double-disconnect from Warning(HEADSET_DISCONNECTED) +
        // CortexEvent::Disconnected that the Cortex client sends.
        tx.send(CortexEvent::Warning {
            code: emotiv::protocol::HEADSET_DISCONNECTED,
            message: serde_json::Value::Null,
        })
        .await
        .unwrap();
        tx.send(CortexEvent::Disconnected).await.unwrap();
        drop(tx);

        let names: Vec<String> = (0..5).map(|i| format!("Ch{i}")).collect();
        let mut adapter = EmotivAdapter::new_for_test(rx, 5, names);

        // First call gets Disconnected (from the Warning).
        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));

        // Second call also returns Disconnected (from CortexEvent::Disconnected).
        // This is fine — session runner breaks on the first one.
        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    #[test]
    fn emotiv_descriptor_correct() {
        let (_, rx) = mpsc::channel(16);
        let names: Vec<String> = (0..14).map(|i| format!("Ch{i}")).collect();
        let adapter = EmotivAdapter::new_for_test(rx, 14, names);

        let desc = adapter.descriptor();
        assert_eq!(desc.kind, "emotiv");
        assert_eq!(desc.eeg_channels, 14);
        assert!((desc.eeg_sample_rate - 128.0).abs() < f64::EPSILON);
        assert!(desc.caps.contains(DeviceCaps::EEG));
        assert!(desc.caps.contains(DeviceCaps::IMU));
        assert!(desc.caps.contains(DeviceCaps::BATTERY));
        assert!(!desc.caps.contains(DeviceCaps::PPG));
    }
}

// ── OpenBCI adapter ───────────────────────────────────────────────────────────

mod openbci_tests {
    use super::*;
    use crate::session::openbci::OpenBciAdapter;

    #[test]
    fn make_descriptor_caps_include_eeg_and_imu() {
        let desc = OpenBciAdapter::make_descriptor(
            "ganglion",
            4,
            200.0,
            vec!["Ch1".into(), "Ch2".into(), "Ch3".into(), "Ch4".into()],
        );
        assert_eq!(desc.kind, "ganglion");
        assert_eq!(desc.eeg_channels, 4);
        assert!(desc.caps.contains(DeviceCaps::EEG));
        assert!(desc.caps.contains(DeviceCaps::IMU));
        assert!(!desc.caps.contains(DeviceCaps::PPG));
        assert_eq!(desc.pipeline_channels, 4);
    }

    #[test]
    fn make_descriptor_large_channel_count_capped() {
        let n = skill_constants::EEG_CHANNELS + 8;
        let desc = OpenBciAdapter::make_descriptor("galea", n, 250.0, (0..n).map(|i| format!("Ch{i}")).collect());
        assert_eq!(desc.eeg_channels, n);
        assert_eq!(desc.pipeline_channels, skill_constants::EEG_CHANNELS);
    }

    #[tokio::test]
    async fn openbci_emits_synthetic_connected_first() {
        // Create a stream with no samples (immediate close).
        let (sample_tx, sample_rx) = tokio::sync::mpsc::channel(1);
        drop(sample_tx); // Close immediately.

        let desc = OpenBciAdapter::make_descriptor(
            "ganglion",
            4,
            200.0,
            vec!["Ch1".into(), "Ch2".into(), "Ch3".into(), "Ch4".into()],
        );
        let info = DeviceInfo {
            name: "Ganglion".into(),
            id: "test-id".into(),
            ..Default::default()
        };

        let mut adapter = OpenBciAdapter::from_receiver(sample_rx, desc, info);

        // First event should be synthetic Connected.
        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Connected(info) => {
                assert_eq!(info.name, "Ganglion");
                assert_eq!(info.id, "test-id");
            }
            other => panic!("expected Connected, got {other:?}"),
        }

        // Stream is closed → None.
        assert!(adapter.next_event().await.is_none());
    }

    #[tokio::test]
    async fn openbci_translates_sample_to_eeg_and_imu() {
        let (sample_tx, sample_rx) = tokio::sync::mpsc::channel(4);

        // Send a sample with EEG + accel.
        sample_tx
            .send(openbci::sample::Sample {
                sample_num: 0,
                eeg: vec![10.0, 20.0, 30.0, 40.0],
                accel: Some([0.1, 0.2, 9.8]),
                analog: None,
                resistance: None,
                timestamp: 1700000000.0,
                end_byte: 0xC0,
                aux_bytes: [0; 6],
            })
            .await
            .unwrap();

        // Send a sample without accel.
        sample_tx
            .send(openbci::sample::Sample {
                sample_num: 1,
                eeg: vec![11.0, 21.0, 31.0, 41.0],
                accel: None,
                analog: None,
                resistance: None,
                timestamp: 1700000001.0,
                end_byte: 0xC1,
                aux_bytes: [0; 6],
            })
            .await
            .unwrap();

        drop(sample_tx);

        let desc = OpenBciAdapter::make_descriptor(
            "ganglion",
            4,
            200.0,
            vec!["Ch1".into(), "Ch2".into(), "Ch3".into(), "Ch4".into()],
        );
        let info = DeviceInfo {
            name: "G".into(),
            id: "id".into(),
            ..Default::default()
        };

        let mut adapter = OpenBciAdapter::from_receiver(sample_rx, desc, info);

        // 1. Synthetic Connected.
        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Connected(_)));

        // 2. EEG from first sample.
        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Eeg(frame) => {
                assert_eq!(frame.channels, vec![10.0, 20.0, 30.0, 40.0]);
                assert!((frame.timestamp_s - 1700000000.0).abs() < f64::EPSILON);
            }
            other => panic!("expected Eeg, got {other:?}"),
        }

        // 3. IMU from first sample (has accel).
        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Imu(imu) => {
                assert!((imu.accel[2] - 9.8).abs() < 0.01);
                assert!(imu.gyro.is_none());
            }
            other => panic!("expected Imu, got {other:?}"),
        }

        // 4. EEG from second sample.
        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Eeg(frame) => {
                assert_eq!(frame.channels, vec![11.0, 21.0, 31.0, 41.0]);
            }
            other => panic!("expected Eeg, got {other:?}"),
        }

        // 5. No IMU from second sample (accel was None).
        // Should get None (channel closed).
        assert!(adapter.next_event().await.is_none());
    }
}
