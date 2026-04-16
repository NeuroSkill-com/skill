// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for a remote device streaming over iroh.
//!
//! Receives [`RemoteDeviceEvent`]s from the iroh device proxy channel and
//! translates them into the standard [`DeviceEvent`] vocabulary.  The session
//! runner's full pipeline (DSP → CSV → embedding → broadcast → DND → hooks)
//! processes the remote stream identically to a locally connected headset.
//!
//! ## Design
//!
//! * `RemoteDeviceEvent::DeviceConnected` → `DeviceEvent::Connected` + descriptor update
//! * `RemoteDeviceEvent::SensorChunk` → many `DeviceEvent::Eeg` + `Ppg` + `Imu` frames
//! * `RemoteDeviceEvent::Battery` → `DeviceEvent::Battery`
//! * `RemoteDeviceEvent::DeviceDisconnected` → `DeviceEvent::Disconnected`
//! * `RemoteDeviceEvent::Location` → `DeviceEvent::Meta` (stored as JSON)
//! * `RemoteDeviceEvent::PhoneImu` → `DeviceEvent::Meta` (phone sensors, separate from headset IMU)
//! * `RemoteDeviceEvent::PhoneInfo` → `DeviceEvent::Meta` (phone descriptor)
//! * `RemoteDeviceEvent::Meta` → `DeviceEvent::Meta`

use std::collections::VecDeque;

use super::{
    BatteryFrame, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, EegFrame, ImuFrame, PpgFrame,
};

use skill_iroh::{RemoteDeviceEvent, SharedDeviceEventTx};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

// ── IrohRemoteAdapter ─────────────────────────────────────────────────────────

/// No data watchdog: if the remote sends nothing for this duration the session
/// ends cleanly.  Covers the case where the QUIC connection dropped without an
/// explicit MSG_DEVICE_DISCONNECTED (e.g. phone out of range, app killed).
const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(30);

pub struct IrohRemoteAdapter {
    rx: mpsc::Receiver<RemoteDeviceEvent>,
    desc: DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,
    peer_id: String,
    /// Whether we've received a DeviceConnected event from the remote.
    got_connected: bool,
    /// Whether a real remote descriptor (MSG_DEVICE_CONNECTED) was received.
    /// If false, we treat early sensor chunks as authoritative for channel count.
    has_remote_descriptor: bool,
    /// Highest timestamp (compact YYYYMMDDHHmmss) we've emitted.
    /// Used to enforce monotonic ordering — backlog data that arrives
    /// with timestamps older than what we've already processed is
    /// re-stamped to `last_ts + epsilon` so the pipeline never sees
    /// out-of-order data.
    last_emitted_ts: i64,
    /// Shared tx slot — cleared in Drop so subsequent iroh messages get
    /// "no active session" instead of "event channel closed".
    cleanup_slot: SharedDeviceEventTx,
}

impl IrohRemoteAdapter {
    /// Create a new adapter that reads events from the given receiver.
    ///
    /// `cleanup_slot` is the shared tx slot in the daemon state.  When this
    /// adapter is dropped (session ends), the slot is cleared to `None` so
    /// any subsequent iroh messages log "no active session" rather than
    /// "event channel closed".
    pub fn new(rx: mpsc::Receiver<RemoteDeviceEvent>, peer_id: String, cleanup_slot: SharedDeviceEventTx) -> Self {
        Self {
            rx,
            desc: DeviceDescriptor {
                kind: "iroh-remote",
                caps: DeviceCaps::EEG | DeviceCaps::PPG | DeviceCaps::IMU | DeviceCaps::BATTERY,
                eeg_channels: 4,
                eeg_sample_rate: 256.0,
                channel_names: vec!["TP9".into(), "AF7".into(), "AF8".into(), "TP10".into()],
                pipeline_channels: 4,
                ppg_channel_names: vec!["Ambient".into(), "Infrared".into(), "Red".into()],
                imu_channel_names: vec![
                    "AccelX".into(),
                    "AccelY".into(),
                    "AccelZ".into(),
                    "GyroX".into(),
                    "GyroY".into(),
                    "GyroZ".into(),
                ],
                fnirs_channel_names: Vec::new(),
            },
            pending: VecDeque::new(),
            peer_id,
            got_connected: false,
            has_remote_descriptor: false,
            last_emitted_ts: 0,
            cleanup_slot,
        }
    }

    /// Expand a SensorChunk into individual DeviceEvents.
    ///
    /// Enforces monotonic timestamps: if this chunk's timestamp is older
    /// than the last emitted timestamp (backlog data arriving after live),
    /// we adjust it forward so the pipeline always sees increasing times.
    /// The adjustment preserves intra-chunk sample spacing (1/sample_rate).
    fn expand_sensor_chunk(&mut self, timestamp: i64, chunk: skill_iroh::device_proto::SensorChunk) {
        let ch = chunk.eeg_data.len();
        if ch == 0 {
            return;
        }
        let spc = chunk.eeg_data[0].len();
        if spc == 0 {
            return;
        }

        // Update descriptor if parameters changed.
        self.desc.eeg_sample_rate = chunk.sample_rate as f64;

        if !self.has_remote_descriptor {
            // No remote descriptor yet: trust live chunk shape to avoid
            // sticky fallback defaults (e.g. default 4ch while device streams 3ch).
            self.desc.eeg_channels = ch;
            self.desc.pipeline_channels = ch.min(skill_constants::EEG_CHANNELS);
            if self.desc.channel_names.len() != ch {
                self.desc.channel_names = (0..ch).map(|i| format!("ExG{i}")).collect();
            }
        } else if ch > self.desc.eeg_channels {
            // Descriptor exists; only expand when upstream adds channels.
            self.desc.eeg_channels = ch;
            self.desc.pipeline_channels = ch.min(skill_constants::EEG_CHANNELS);
            if self.desc.channel_names.len() < ch {
                for i in self.desc.channel_names.len()..ch {
                    self.desc.channel_names.push(format!("ExG{i}"));
                }
            }
        }

        let out_ch = self.desc.eeg_channels.max(ch);
        if ch < out_ch {
            eprintln!("[iroh-remote] short EEG chunk: got {ch} channels, expected {out_ch}; padding missing channels");
        }

        // Enforce monotonic ordering.  If the chunk is from the backlog
        // (timestamp ≤ last emitted), shift it just past the last emitted
        // time.  This keeps all downstream models/storage/logic happy —
        // they rely on strictly non-decreasing timestamps.
        let effective_ts = if timestamp <= self.last_emitted_ts {
            self.last_emitted_ts + 1 // +1 second in compact format
        } else {
            timestamp
        };
        self.last_emitted_ts = effective_ts;

        let base_ts = ts_to_unix_approx(effective_ts);
        let dt = 1.0 / chunk.sample_rate as f64;

        // PPG interleaving
        let ppg_ch = chunk.ppg_data.len();
        let ppg_spc = if ppg_ch > 0 { chunk.ppg_data[0].len() } else { 0 };
        let ppg_stride = spc.checked_div(ppg_spc).unwrap_or(0);
        let mut ppg_idx = 0usize;

        // IMU interleaving
        let imu_n = chunk.imu_data.len();
        let imu_stride = spc.checked_div(imu_n).unwrap_or(0);
        let mut imu_idx = 0usize;

        for i in 0..spc {
            let ts = base_ts + i as f64 * dt;

            let channels: Vec<f64> = (0..out_ch)
                .map(|c| if c < ch { chunk.eeg_data[c][i] as f64 } else { 0.0 })
                .collect();
            self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                channels,
                timestamp_s: ts,
            }));

            // PPG
            if ppg_stride > 0 && ppg_idx < ppg_spc && (i % ppg_stride == 0) {
                for ppg_c in 0..ppg_ch {
                    if ppg_idx < chunk.ppg_data[ppg_c].len() {
                        self.pending.push_back(DeviceEvent::Ppg(PpgFrame {
                            channel: ppg_c,
                            samples: vec![chunk.ppg_data[ppg_c][ppg_idx]],
                            timestamp_s: ts,
                        }));
                    }
                }
                ppg_idx += 1;
            }

            // IMU
            if imu_stride > 0 && imu_idx < imu_n && (i % imu_stride == 0) {
                let (ax, ay, az, gx, gy, gz) = chunk.imu_data[imu_idx];
                self.pending.push_back(DeviceEvent::Imu(ImuFrame {
                    accel: [ax, ay, az],
                    gyro: Some([gx, gy, gz]),
                    mag: None,
                }));
                imu_idx += 1;
            }
        }
    }

    /// Parse a DeviceConnected JSON descriptor and update our descriptor + emit Connected.
    fn handle_device_connected(&mut self, json: &str) -> DeviceEvent {
        // Parse the JSON descriptor from the iOS client
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
            if let Some(kind) = v["kind"].as_str() {
                // We can't change &'static str kind at runtime, but we log it
                eprintln!("[iroh-remote] remote device kind: {kind}");
            }
            if let Some(sr) = v["sample_rate"].as_f64() {
                self.desc.eeg_sample_rate = sr;
            }
            if let Some(chs) = v["eeg_channels"].as_array() {
                let names: Vec<String> = chs.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect();
                if !names.is_empty() {
                    self.desc.eeg_channels = names.len();
                    self.desc.pipeline_channels = names.len().min(skill_constants::EEG_CHANNELS);
                    self.desc.channel_names = names;
                }
            }
            if let Some(ppg) = v["ppg_channels"].as_array() {
                self.desc.ppg_channel_names = ppg.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect();
                if self.desc.ppg_channel_names.is_empty() {
                    self.desc.caps.remove(DeviceCaps::PPG);
                }
            }
            if let Some(imu) = v["imu_channels"].as_array() {
                self.desc.imu_channel_names = imu.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect();
                if self.desc.imu_channel_names.is_empty() {
                    self.desc.caps.remove(DeviceCaps::IMU);
                }
            }
            // Update caps from explicit array
            if let Some(caps) = v["caps"].as_array() {
                let mut c = DeviceCaps::empty();
                for cap in caps {
                    match cap.as_str() {
                        Some("eeg") => c |= DeviceCaps::EEG,
                        Some("ppg") => c |= DeviceCaps::PPG,
                        Some("imu") => c |= DeviceCaps::IMU,
                        Some("battery") => c |= DeviceCaps::BATTERY,
                        Some("meta") => c |= DeviceCaps::META,
                        _ => {}
                    }
                }
                if !c.is_empty() {
                    self.desc.caps = c;
                }
            }

            let name = v["name"].as_str().unwrap_or("Remote Device").to_owned();
            let id = v["id"].as_str().unwrap_or(&self.peer_id).to_owned();

            DeviceEvent::Connected(DeviceInfo {
                name,
                id,
                serial_number: v["serial"].as_str().map(str::to_owned),
                firmware_version: v["firmware"].as_str().map(str::to_owned),
                hardware_version: v["hardware"].as_str().map(str::to_owned),
                bootloader_version: None,
                mac_address: None,
                headset_preset: None,
            })
        } else {
            // Fallback if JSON parsing fails
            DeviceEvent::Connected(DeviceInfo {
                name: "Remote Device (iroh)".into(),
                id: self.peer_id.clone(),
                ..Default::default()
            })
        }
    }
}

impl Drop for IrohRemoteAdapter {
    fn drop(&mut self) {
        // Clear the shared tx slot so subsequent iroh messages log
        // "no active session" instead of "event channel closed".
        if let Ok(mut g) = self.cleanup_slot.lock() {
            *g = None;
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for IrohRemoteAdapter {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }

    async fn next_event(&mut self) -> Option<DeviceEvent> {
        loop {
            // Yield pending events first
            if let Some(ev) = self.pending.pop_front() {
                return Some(ev);
            }

            // Wait for next remote event — bounded by a watchdog timeout.
            // If no data arrives within WATCHDOG_TIMEOUT, the QUIC tunnel
            // is assumed dead and we return None to end the session cleanly.
            let remote_ev = match timeout(WATCHDOG_TIMEOUT, self.rx.recv()).await {
                Ok(Some(ev)) => ev,
                Ok(None) => return None, // channel closed (tx dropped)
                Err(_elapsed) => {
                    eprintln!(
                        "[iroh-remote] no data for {}s — ending session (peer: {})",
                        WATCHDOG_TIMEOUT.as_secs(),
                        self.peer_id
                    );
                    return None;
                }
            };

            match remote_ev {
                RemoteDeviceEvent::DeviceConnected { descriptor_json, .. } => {
                    self.got_connected = true;
                    self.has_remote_descriptor = true;
                    return Some(self.handle_device_connected(&descriptor_json));
                }
                RemoteDeviceEvent::DeviceDisconnected { .. } => {
                    return Some(DeviceEvent::Disconnected);
                }
                RemoteDeviceEvent::SensorChunk { timestamp, chunk, .. } => {
                    // If we haven't gotten a Connected event yet, synthesize one
                    if !self.got_connected {
                        self.got_connected = true;
                        self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                            name: "Remote Device (iroh)".into(),
                            id: self.peer_id.clone(),
                            ..Default::default()
                        }));
                    }
                    self.expand_sensor_chunk(timestamp, chunk);
                    // Loop back to yield from pending
                }
                RemoteDeviceEvent::Battery { level_pct, .. } => {
                    return Some(DeviceEvent::Battery(BatteryFrame {
                        level_pct,
                        voltage_mv: None,
                        temperature_raw: None,
                    }));
                }
                RemoteDeviceEvent::Location {
                    location, timestamp, ..
                } => {
                    // Store GPS as Meta event so it's accessible to the pipeline
                    let json = serde_json::json!({
                        "type": "location",
                        "timestamp": timestamp,
                        "latitude": location.latitude,
                        "longitude": location.longitude,
                        "altitude": location.altitude,
                        "accuracy": location.accuracy,
                        "speed": location.speed,
                        "heading": location.heading,
                    });
                    return Some(DeviceEvent::Meta(json));
                }
                RemoteDeviceEvent::PhoneImu { timestamp, samples, .. } => {
                    // Serialize phone sensor data as a JSON Meta event.
                    // This keeps it completely separate from the headset's IMU
                    // which flows through DeviceEvent::Imu → head_pose / CSV.
                    let phone_samples: Vec<serde_json::Value> = samples
                        .iter()
                        .map(|s| {
                            serde_json::json!({
                                "dt": s.dt,
                                "raw_accel": s.raw_accel,
                                "user_accel": s.user_accel,
                                "gravity": s.gravity,
                                "gyro": s.gyro,
                                "mag": s.mag,
                                "attitude": s.attitude,
                                "pressure": s.pressure,
                            })
                        })
                        .collect();
                    let json = serde_json::json!({
                        "type": "phone_imu",
                        "timestamp": timestamp,
                        "count": samples.len(),
                        "samples": phone_samples,
                    });
                    return Some(DeviceEvent::Meta(json));
                }
                RemoteDeviceEvent::PhoneInfo {
                    info_json, timestamp, ..
                } => {
                    // Store phone descriptor as Meta so it's persisted alongside session data.
                    // The desktop can use this to identify which phone sent which data
                    // when multiple phones are connected simultaneously.
                    if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&info_json) {
                        v["type"] = serde_json::json!("phone_info");
                        v["timestamp"] = serde_json::json!(timestamp);
                        return Some(DeviceEvent::Meta(v));
                    }
                }
                RemoteDeviceEvent::Meta { json, .. } => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                        return Some(DeviceEvent::Meta(v));
                    }
                }
            }
        }
    }

    async fn disconnect(&mut self) {
        self.rx.close();
    }
}

/// Approximate conversion of `YYYYMMDDHHmmss` integer → Unix seconds.
fn ts_to_unix_approx(ts: i64) -> f64 {
    let s = (ts % 100) as u64;
    let m = (ts / 100 % 100) as u64;
    let h = (ts / 10_000 % 100) as u64;
    let d = (ts / 1_000_000 % 100) as u64;
    let mo = (ts / 100_000_000 % 100) as u64;
    let y = (ts / 10_000_000_000) as u32;

    let mut days = 0u64;
    for yr in 1970..y {
        let leap = yr.is_multiple_of(4) && (!yr.is_multiple_of(100) || yr.is_multiple_of(400));
        days += if leap { 366 } else { 365 };
    }
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let month_days: [u64; 12] = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for &md in month_days.iter().take((mo as usize).saturating_sub(1)) {
        days += md;
    }
    days += d.saturating_sub(1);
    (days * 86400 + h * 3600 + m * 60 + s) as f64
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn ts_to_unix_known_date() {
        let unix = ts_to_unix_approx(20260315120000);
        assert!((unix - 1773576000.0).abs() < 1.0);
    }

    #[tokio::test]
    async fn adapter_handles_sensor_chunk() {
        let (tx, rx) = mpsc::channel(4);
        let mut adapter =
            IrohRemoteAdapter::new(rx, "test-peer".into(), std::sync::Arc::new(std::sync::Mutex::new(None)));

        // Send a sensor chunk
        let chunk = skill_iroh::device_proto::SensorChunk {
            sample_rate: 256.0,
            eeg_data: vec![vec![1.0; 10]; 4],
            ppg_data: vec![],
            imu_data: vec![],
        };
        tx.send(RemoteDeviceEvent::SensorChunk {
            seq: 1,
            timestamp: 20260315120000,
            chunk,
        })
        .await
        .unwrap();

        // First: auto-synthesized Connected
        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Connected(_)));

        // Then 10 EEG frames
        for _ in 0..10 {
            let ev = adapter.next_event().await.unwrap();
            assert!(matches!(ev, DeviceEvent::Eeg(_)));
        }
    }

    #[tokio::test]
    async fn adapter_handles_device_connected_json() {
        let (tx, rx) = mpsc::channel(4);
        let mut adapter =
            IrohRemoteAdapter::new(rx, "test-peer".into(), std::sync::Arc::new(std::sync::Mutex::new(None)));

        let json = serde_json::json!({
            "kind": "muse",
            "name": "Muse-ABCD",
            "id": "AA:BB:CC:DD",
            "sample_rate": 256.0,
            "eeg_channels": ["TP9", "AF7", "AF8", "TP10"],
            "ppg_channels": ["Ambient", "Infrared", "Red"],
            "caps": ["eeg", "ppg", "imu", "battery"]
        })
        .to_string();

        tx.send(RemoteDeviceEvent::DeviceConnected {
            seq: 1,
            timestamp: 20260315120000,
            descriptor_json: json,
        })
        .await
        .unwrap();

        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Connected(info) => {
                assert_eq!(info.name, "Muse-ABCD");
                assert_eq!(info.id, "AA:BB:CC:DD");
            }
            _ => panic!("expected Connected"),
        }
        assert_eq!(adapter.descriptor().eeg_channels, 4);
        assert_eq!(adapter.descriptor().eeg_sample_rate, 256.0);
    }

    #[tokio::test]
    async fn adapter_handles_location() {
        let (tx, rx) = mpsc::channel(4);
        let mut adapter =
            IrohRemoteAdapter::new(rx, "test-peer".into(), std::sync::Arc::new(std::sync::Mutex::new(None)));

        tx.send(RemoteDeviceEvent::Location {
            seq: 1,
            timestamp: 20260315120000,
            location: skill_iroh::IrohLocation {
                latitude: 37.7749,
                longitude: -122.4194,
                altitude: 10.0,
                accuracy: 5.0,
                speed: 0.0,
                heading: 0.0,
            },
        })
        .await
        .unwrap();

        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Meta(json) => {
                assert_eq!(json["type"], "location");
                assert!((json["latitude"].as_f64().unwrap() - 37.7749).abs() < 1e-4);
            }
            _ => panic!("expected Meta"),
        }
    }

    #[tokio::test]
    async fn adapter_pads_short_chunks_to_descriptor_channels() {
        let (tx, rx) = mpsc::channel(8);
        let mut adapter = IrohRemoteAdapter::new(rx, "test".into(), std::sync::Arc::new(std::sync::Mutex::new(None)));

        // Descriptor declares 6 EEG channels.
        let json = serde_json::json!({
            "kind": "attentivu",
            "name": "AttentivU-053",
            "id": "attn:test",
            "sample_rate": 250.0,
            "eeg_channels": ["ExG0", "ExG1", "ExG2", "ExG3", "ExG4", "ExG5"],
            "caps": ["eeg", "imu", "battery"]
        })
        .to_string();
        tx.send(RemoteDeviceEvent::DeviceConnected {
            seq: 1,
            timestamp: 20260315120000,
            descriptor_json: json,
        })
        .await
        .unwrap();
        let _ = adapter.next_event().await.unwrap(); // Connected

        // Chunk carries only 3 channels (upstream omitted inactive channels).
        let chunk = skill_iroh::device_proto::SensorChunk {
            sample_rate: 250.0,
            eeg_data: vec![vec![1.0; 4], vec![2.0; 4], vec![3.0; 4]],
            ppg_data: vec![],
            imu_data: vec![],
        };
        tx.send(RemoteDeviceEvent::SensorChunk {
            seq: 2,
            timestamp: 20260315120001,
            chunk,
        })
        .await
        .unwrap();

        // First EEG frame should be padded back to 6 channels.
        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Eeg(f) => assert_eq!(f.channels.len(), 6),
            _ => panic!("expected Eeg"),
        }
    }

    #[tokio::test]
    async fn adapter_interleaves_imu() {
        let (tx, rx) = mpsc::channel(4);
        let mut adapter = IrohRemoteAdapter::new(rx, "test".into(), std::sync::Arc::new(std::sync::Mutex::new(None)));

        let chunk = skill_iroh::device_proto::SensorChunk {
            sample_rate: 256.0,
            eeg_data: vec![vec![0.0; 8]; 4],
            ppg_data: vec![],
            imu_data: vec![(1.0, 2.0, 9.8, 0.1, 0.2, 0.3), (1.1, 2.1, 9.9, 0.0, 0.0, 0.0)],
        };
        tx.send(RemoteDeviceEvent::SensorChunk {
            seq: 1,
            timestamp: 20260315120000,
            chunk,
        })
        .await
        .unwrap();

        // Connected + 8 EEG + 2 IMU = 11 events
        let mut eeg_count = 0;
        let mut imu_count = 0;
        let mut other = 0;
        for _ in 0..11 {
            match adapter.next_event().await.unwrap() {
                DeviceEvent::Eeg(_) => eeg_count += 1,
                DeviceEvent::Imu(f) => {
                    imu_count += 1;
                    assert!(f.gyro.is_some());
                }
                DeviceEvent::Connected(_) => other += 1,
                _ => {}
            }
        }
        assert_eq!(eeg_count, 8);
        assert_eq!(imu_count, 2);
        assert_eq!(other, 1); // Connected
    }

    /// Backlog data arriving with older timestamps must be shifted forward
    /// so the pipeline always sees non-decreasing timestamps.
    #[tokio::test]
    async fn adapter_monotonic_timestamps_backlog() {
        let (tx, rx) = mpsc::channel(8);
        let mut adapter = IrohRemoteAdapter::new(rx, "test".into(), std::sync::Arc::new(std::sync::Mutex::new(None)));

        // Send a "live" chunk at timestamp T=120500
        let live_chunk = skill_iroh::device_proto::SensorChunk {
            sample_rate: 256.0,
            eeg_data: vec![vec![1.0; 4]; 4],
            ppg_data: vec![],
            imu_data: vec![],
        };
        tx.send(RemoteDeviceEvent::SensorChunk {
            seq: 1,
            timestamp: 20260315120500, // 12:05:00
            chunk: live_chunk,
        })
        .await
        .unwrap();

        // Drain: Connected + 4 EEG
        for _ in 0..5 {
            adapter.next_event().await.unwrap();
        }

        // Now send "backlog" chunk with OLDER timestamp T=120000 (before live)
        let backlog_chunk = skill_iroh::device_proto::SensorChunk {
            sample_rate: 256.0,
            eeg_data: vec![vec![2.0; 4]; 4],
            ppg_data: vec![],
            imu_data: vec![],
        };
        tx.send(RemoteDeviceEvent::SensorChunk {
            seq: 2,
            timestamp: 20260315120000, // 12:00:00 — BEFORE the live chunk
            chunk: backlog_chunk,
        })
        .await
        .unwrap();

        // Drain 4 EEG frames from backlog
        let mut backlog_timestamps = Vec::new();
        for _ in 0..4 {
            match adapter.next_event().await.unwrap() {
                DeviceEvent::Eeg(f) => backlog_timestamps.push(f.timestamp_s),
                other => panic!("expected Eeg, got {:?}", std::mem::discriminant(&other)),
            }
        }

        // The backlog timestamps should be AFTER the live chunk's timestamps
        // (shifted forward because the adapter enforces monotonic ordering)
        let live_base = ts_to_unix_approx(20260315120500);
        for ts in &backlog_timestamps {
            assert!(
                *ts >= live_base,
                "backlog timestamp {ts} should be >= live base {live_base}"
            );
        }
    }

    /// Two chunks with the same timestamp should not produce duplicate times.
    #[tokio::test]
    async fn adapter_monotonic_same_timestamp() {
        let (tx, rx) = mpsc::channel(8);
        let mut adapter = IrohRemoteAdapter::new(rx, "test".into(), std::sync::Arc::new(std::sync::Mutex::new(None)));

        let ts = 20260315120000i64;
        for seq in 1..=2 {
            let chunk = skill_iroh::device_proto::SensorChunk {
                sample_rate: 256.0,
                eeg_data: vec![vec![0.0; 2]; 4],
                ppg_data: vec![],
                imu_data: vec![],
            };
            tx.send(RemoteDeviceEvent::SensorChunk {
                seq,
                timestamp: ts,
                chunk,
            })
            .await
            .unwrap();
        }

        // Drain Connected + 2 EEG from chunk 1
        let mut all_ts = Vec::new();
        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Connected(_)));
        for _ in 0..2 {
            if let DeviceEvent::Eeg(f) = adapter.next_event().await.unwrap() {
                all_ts.push(f.timestamp_s);
            }
        }
        // Drain 2 EEG from chunk 2
        for _ in 0..2 {
            if let DeviceEvent::Eeg(f) = adapter.next_event().await.unwrap() {
                all_ts.push(f.timestamp_s);
            }
        }

        // All timestamps should be strictly non-decreasing
        for i in 1..all_ts.len() {
            assert!(
                all_ts[i] >= all_ts[i - 1],
                "timestamps must be non-decreasing: {} < {} at index {}",
                all_ts[i],
                all_ts[i - 1],
                i
            );
        }
        // Chunk 2 timestamps should be > chunk 1 timestamps (shifted forward)
        assert!(all_ts[2] > all_ts[1], "second chunk must be shifted forward");
    }

    /// DeviceDisconnected event returns None (session ends).
    #[tokio::test]
    async fn adapter_disconnect_returns_none() {
        let (tx, rx) = mpsc::channel(4);
        let mut adapter = IrohRemoteAdapter::new(rx, "test".into(), std::sync::Arc::new(std::sync::Mutex::new(None)));

        tx.send(RemoteDeviceEvent::DeviceDisconnected {
            seq: 1,
            timestamp: 20260315120000,
        })
        .await
        .unwrap();

        let ev = adapter.next_event().await.unwrap();
        assert!(matches!(ev, DeviceEvent::Disconnected));
    }

    /// Channel close (tunnel drop) returns None.
    #[tokio::test]
    async fn adapter_channel_close_returns_none() {
        let (tx, rx) = mpsc::channel(4);
        let mut adapter = IrohRemoteAdapter::new(rx, "test".into(), std::sync::Arc::new(std::sync::Mutex::new(None)));

        drop(tx); // simulate tunnel drop

        let ev = adapter.next_event().await;
        assert!(ev.is_none(), "channel close should return None");
    }

    /// AttentivU device descriptor updates caps correctly.
    #[tokio::test]
    async fn adapter_attentivu_descriptor() {
        let (tx, rx) = mpsc::channel(4);
        let mut adapter = IrohRemoteAdapter::new(rx, "test".into(), std::sync::Arc::new(std::sync::Mutex::new(None)));

        let json = serde_json::json!({
            "kind": "attentivu",
            "name": "AtU-1234",
            "id": "XX:YY:ZZ",
            "sample_rate": 250.0,
            "eeg_channels": ["EXG0_CH0", "EXG0_CH1", "EXG1_CH0", "EXG1_CH1"],
            "ppg_channels": [],
            "imu_channels": ["AccelX", "AccelY", "AccelZ", "GyroX", "GyroY", "GyroZ"],
            "caps": ["eeg", "imu", "battery"]
        })
        .to_string();

        tx.send(RemoteDeviceEvent::DeviceConnected {
            seq: 1,
            timestamp: 20260315120000,
            descriptor_json: json,
        })
        .await
        .unwrap();

        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Connected(info) => {
                assert_eq!(info.name, "AtU-1234");
            }
            _ => panic!("expected Connected"),
        }
        assert_eq!(adapter.descriptor().eeg_channels, 4);
        assert_eq!(adapter.descriptor().eeg_sample_rate, 250.0);
        assert!(adapter.descriptor().ppg_channel_names.is_empty());
        assert!(!adapter.descriptor().caps.contains(DeviceCaps::PPG));
        assert!(adapter.descriptor().caps.contains(DeviceCaps::EEG));
        assert!(adapter.descriptor().caps.contains(DeviceCaps::IMU));
    }

    /// PhoneInfo event is forwarded as Meta.
    #[tokio::test]
    async fn adapter_phone_info() {
        let (tx, rx) = mpsc::channel(4);
        let mut adapter = IrohRemoteAdapter::new(rx, "test".into(), std::sync::Arc::new(std::sync::Mutex::new(None)));

        let info_json = serde_json::json!({
            "phone_model": "iPhone16,1",
            "os": "iOS",
            "os_version": "18.2",
        })
        .to_string();

        tx.send(RemoteDeviceEvent::PhoneInfo {
            seq: 1,
            timestamp: 20260315120000,
            info_json,
        })
        .await
        .unwrap();

        let ev = adapter.next_event().await.unwrap();
        match ev {
            DeviceEvent::Meta(json) => {
                assert_eq!(json["type"], "phone_info");
                assert_eq!(json["phone_model"], "iPhone16,1");
            }
            _ => panic!("expected Meta"),
        }
    }
}
