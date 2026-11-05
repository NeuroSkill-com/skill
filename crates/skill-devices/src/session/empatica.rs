// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for the Empatica EmbracePlus wristband.
//!
//! Built on the [`empatica`] crate, which speaks the band's BLE protocol over
//! the Web Bluetooth API — the same `webbluetooth` session every other BLE
//! device in this app uses.
//!
//! The band is not an EEG device, so this declares no EEG channels. What it
//! reports live:
//!
//! * **Skin temperature** → [`DeviceEvent::Vitals`].
//! * **EDA, steps, ambient light, magnetometer, tags** → [`DeviceEvent::Meta`]
//!   for broadcast/logging.
//! * **Seizure-detection events** → [`DeviceEvent::Meta`].
//!
//! ## What is deliberately not decoded
//!
//! Accelerometer, gyroscope and PPG arrive delta-compressed, and decoding them
//! needs the band's sensor profile (range, channel count, compression mode),
//! which is stored in a session buffer rather than sent over the live link.
//! Without it the bytes cannot be scaled correctly, and guessing a scale would
//! put wrong numbers into a recording. These packets are therefore counted and
//! dropped; the adapter does not declare `PPG` or `IMU`.
//!
//! [`empatica`]: https://docs.rs/empatica

use std::collections::VecDeque;

use empatica::prelude::*;
use tokio::sync::mpsc;
use tracing::info;

use super::{now_secs, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, VitalsFrame};

/// How to find and open a band.
#[derive(Clone, Debug)]
pub struct EmpaticaConfig {
    /// Advertised-name prefix to match. Empty accepts any band advertising the
    /// Embrace service — the firmware-dependent name is not a reliable key.
    pub name_prefix: String,
    /// How long to scan before giving up.
    pub scan_timeout_secs: u64,
}

impl Default for EmpaticaConfig {
    fn default() -> Self {
        Self {
            name_prefix: String::new(),
            scan_timeout_secs: 15,
        }
    }
}

impl EmpaticaConfig {
    /// The `empatica` client configuration this asks for.
    pub fn to_client_config(&self) -> EmbraceClientConfig {
        EmbraceClientConfig {
            scan_timeout_secs: self.scan_timeout_secs,
            name_prefix: self.name_prefix.clone(),
            ..Default::default()
        }
    }
}

pub struct EmpaticaAdapter {
    rx: mpsc::Receiver<EmbraceEvent>,
    handle: Option<EmbraceHandle>,
    desc: DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,
    /// Compressed packets seen but not decodable; logged once, not per packet.
    undecoded: u64,
}

fn descriptor() -> DeviceDescriptor {
    DeviceDescriptor {
        kind: "empatica",
        caps: DeviceCaps::VITALS | DeviceCaps::META,
        eeg_channels: 0,
        eeg_sample_rate: 0.0,
        channel_names: Vec::new(),
        pipeline_channels: 0,
        ppg_channel_names: Vec::new(),
        imu_channel_names: Vec::new(),
        fnirs_channel_names: Vec::new(),
    }
}

impl EmpaticaAdapter {
    pub fn new(rx: mpsc::Receiver<EmbraceEvent>, handle: EmbraceHandle) -> Self {
        Self {
            rx,
            handle: Some(handle),
            desc: descriptor(),
            pending: VecDeque::new(),
            undecoded: 0,
        }
    }

    fn translate(&mut self, ev: EmbraceEvent) {
        match ev {
            EmbraceEvent::Connected(name) => {
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name,
                    ..Default::default()
                }));
            }
            EmbraceEvent::Disconnected => self.pending.push_back(DeviceEvent::Disconnected),
            EmbraceEvent::DeviceInfo(i) => {
                // `fw` is the key the session runner reads a firmware version from.
                self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                    "source": "empatica_device_info",
                    "fw": i.firmware_revision,
                    "hw": i.hardware_revision,
                    "serial": i.serial_number,
                    "model": i.model_number,
                    "manufacturer": i.manufacturer,
                })));
            }
            EmbraceEvent::Decoded { data, received_at, .. } => {
                let ts = if received_at > 0.0 { received_at } else { now_secs() };
                self.translate_decoded(data, ts);
            }
            EmbraceEvent::Seizure(s) => {
                self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                    "source": "empatica_seizure",
                    "timestamp": s.timestamp,
                    "sensitivity": format!("{:?}", s.sensitivity),
                    "classification": format!("{:?}", s.classification),
                    "epoch_probability": s.epoch_probability,
                    "cumulative_probability": s.cumulative_probability,
                })));
            }
            // Delta-compressed Acc/Gyro/PPG: see the module docs.
            EmbraceEvent::Sensor(_) => {
                if self.undecoded == 0 {
                    info!("Empatica: compressed sensor streams are not decoded (no live sensor profile)");
                }
                self.undecoded = self.undecoded.saturating_add(1);
            }
        }
    }

    fn translate_decoded(&mut self, data: Decoded, ts: f64) {
        match data {
            Decoded::Temperature(samples) => {
                for s in samples {
                    // A zero reading is an unpopulated sensor, not 0 °C skin.
                    if s.body_raw == 0 {
                        continue;
                    }
                    self.pending.push_back(DeviceEvent::Vitals(VitalsFrame {
                        timestamp_s: ts,
                        temperature_c: Some(s.body_celsius()),
                        ..Default::default()
                    }));
                }
            }
            Decoded::Eda(samples) => {
                let raw: Vec<[u16; 2]> = samples.iter().map(|s| [s.raw0, s.raw1]).collect();
                self.meta("empatica_eda", ts, serde_json::json!({ "raw": raw }));
            }
            Decoded::Steps(n) => self.meta("empatica_steps", ts, serde_json::json!({ "steps": n })),
            Decoded::Tag(t) => self.meta("empatica_tag", ts, serde_json::json!({ "tag": t })),
            Decoded::AmbientLight(v) => self.meta("empatica_light", ts, serde_json::json!({ "raw": v })),
            Decoded::AmbientLightMilliLux(v) => {
                self.meta("empatica_light", ts, serde_json::json!({ "milli_lux": v }));
            }
            // Raw magnetometer / diagnostics / system housekeeping: not session data.
            Decoded::Magnetometer(_)
            | Decoded::DebugMagnetometer(_)
            | Decoded::UnfilteredAccelerometer(_)
            | Decoded::System { .. } => {}
        }
    }

    fn meta(&mut self, source: &str, ts: f64, mut body: serde_json::Value) {
        if let Some(o) = body.as_object_mut() {
            o.insert("source".into(), source.into());
            o.insert("timestamp_s".into(), ts.into());
        }
        self.pending.push_back(DeviceEvent::Meta(body));
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for EmpaticaAdapter {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }

    async fn next_event(&mut self) -> Option<DeviceEvent> {
        loop {
            if let Some(ev) = self.pending.pop_front() {
                return Some(ev);
            }
            let vendor_ev = self.rx.recv().await?;
            self.translate(vendor_ev);
        }
    }

    async fn disconnect(&mut self) {
        if let Some(ref h) = self.handle {
            if let Err(e) = h.disconnect().await {
                info!(error = %e, "Empatica disconnect reported an error");
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn adapter() -> EmpaticaAdapter {
        let (_tx, rx) = mpsc::channel(8);
        EmpaticaAdapter {
            rx,
            handle: None,
            desc: descriptor(),
            pending: VecDeque::new(),
            undecoded: 0,
        }
    }

    fn decoded(data: Decoded) -> EmbraceEvent {
        EmbraceEvent::Decoded {
            count: 1,
            received_at: 1_700_000_000.0,
            data,
        }
    }

    #[test]
    fn descriptor_declares_vitals_and_no_eeg() {
        let a = adapter();
        let d = a.descriptor();
        assert_eq!(d.kind, "empatica");
        assert!(d.caps.contains(DeviceCaps::VITALS));
        assert!(!d.caps.contains(DeviceCaps::EEG));
        assert!(!d.caps.contains(DeviceCaps::PPG), "PPG is not decoded live");
        assert!(!d.caps.contains(DeviceCaps::IMU), "IMU is not decoded live");
        assert_eq!(d.eeg_channels, 0);
    }

    #[test]
    fn temperature_becomes_skin_temperature_vitals() {
        let mut a = adapter();
        // 4480 * 0.0078125 = 35.0 °C
        a.translate(decoded(Decoded::Temperature(vec![TemperatureSample {
            body_raw: 4480,
            device_raw: 0,
        }])));
        match a.pending.front().unwrap() {
            DeviceEvent::Vitals(v) => {
                assert_eq!(v.temperature_c, Some(35.0));
                assert_eq!(v.timestamp_s, 1_700_000_000.0);
                assert_eq!(v.hr_bpm, None);
            }
            other => panic!("expected Vitals, got {other:?}"),
        }
    }

    #[test]
    fn a_zero_temperature_is_not_a_measurement() {
        let mut a = adapter();
        a.translate(decoded(Decoded::Temperature(vec![TemperatureSample {
            body_raw: 0,
            device_raw: 0,
        }])));
        assert!(a.pending.is_empty());
    }

    #[test]
    fn eda_and_steps_go_to_meta_with_source() {
        let mut a = adapter();
        a.translate(decoded(Decoded::Eda(vec![EdaSample { raw0: 10, raw1: 20 }])));
        a.translate(decoded(Decoded::Steps(3)));
        let srcs: Vec<_> = a
            .pending
            .iter()
            .map(|e| match e {
                DeviceEvent::Meta(v) => v["source"].as_str().unwrap().to_string(),
                other => panic!("expected Meta, got {other:?}"),
            })
            .collect();
        assert_eq!(srcs, ["empatica_eda", "empatica_steps"]);
    }

    #[test]
    fn device_info_reports_firmware_under_the_runner_key() {
        let mut a = adapter();
        a.translate(EmbraceEvent::DeviceInfo(empatica::types::DeviceInfo {
            firmware_revision: Some("3.1.0".into()),
            ..Default::default()
        }));
        match a.pending.front().unwrap() {
            DeviceEvent::Meta(v) => assert_eq!(v["fw"].as_str(), Some("3.1.0")),
            other => panic!("expected Meta, got {other:?}"),
        }
    }

    #[test]
    fn compressed_sensor_packets_are_dropped_not_guessed() {
        let mut a = adapter();
        a.translate(EmbraceEvent::Sensor(SensorPacket {
            packet_type: PacketType::PpgGreen,
            tag: 0,
            count: 1,
            declared_len: 0,
            received_at: 0.0,
            payload: vec![1, 2, 3],
        }));
        assert!(a.pending.is_empty());
        assert_eq!(a.undecoded, 1);
    }

    #[test]
    fn disconnect_without_a_handle_is_safe() {
        let mut a = adapter();
        futures::executor::block_on(a.disconnect());
    }
}
