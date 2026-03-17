// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for the IDUN Guardian in-ear EEG earbud.
//!
//! The Guardian is a single-earbud BLE device with a bipolar electrode montage
//! (in-ear-canal signal + outer-ear reference), producing **one EEG channel at
//! 250 Hz**.  It also has a 6-DOF IMU (accelerometer + gyroscope at ~52 Hz).
//!
//! EEG, accelerometer, and gyroscope data are multiplexed onto a single BLE
//! GATT characteristic.  Each EEG notification contains up to 20 samples.

use std::collections::VecDeque;

use tokio::sync::mpsc;

use idun::prelude::*;
use skill_constants::{EEG_CHANNELS, IDUN_CHANNEL_NAMES, IDUN_EEG_CHANNELS, IDUN_SAMPLE_RATE};

use super::{
    BatteryFrame, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo,
    EegFrame, ImuFrame, now_secs,
};

// ── IdunAdapter ───────────────────────────────────────────────────────────────

pub struct IdunAdapter {
    rx:      mpsc::Receiver<GuardianEvent>,
    handle:  Option<GuardianHandle>,
    desc:    DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,

    /// Last known accelerometer values so gyro events can pair with them.
    last_accel: [f32; 3],
}

impl IdunAdapter {
    pub fn new(rx: mpsc::Receiver<GuardianEvent>, handle: GuardianHandle) -> Self {
        let channel_names: Vec<String> =
            IDUN_CHANNEL_NAMES.iter().map(|s| (*s).to_owned()).collect();

        Self {
            rx,
            handle: Some(handle),
            desc: DeviceDescriptor {
                kind: "idun",
                caps: DeviceCaps::EEG | DeviceCaps::IMU | DeviceCaps::BATTERY,
                eeg_channels: IDUN_EEG_CHANNELS,
                eeg_sample_rate: IDUN_SAMPLE_RATE,
                channel_names,
                pipeline_channels: IDUN_EEG_CHANNELS.min(EEG_CHANNELS),
            },
            pending: VecDeque::new(),
            last_accel: [0.0; 3],
        }
    }

    /// Test-only constructor without a real BLE handle.
    #[cfg(test)]
    pub(crate) fn new_for_test(rx: mpsc::Receiver<GuardianEvent>) -> Self {
        let channel_names: Vec<String> =
            IDUN_CHANNEL_NAMES.iter().map(|s| (*s).to_owned()).collect();

        Self {
            rx,
            handle: None,
            desc: DeviceDescriptor {
                kind: "idun",
                caps: DeviceCaps::EEG | DeviceCaps::IMU | DeviceCaps::BATTERY,
                eeg_channels: IDUN_EEG_CHANNELS,
                eeg_sample_rate: IDUN_SAMPLE_RATE,
                channel_names,
                pipeline_channels: IDUN_EEG_CHANNELS.min(EEG_CHANNELS),
            },
            pending: VecDeque::new(),
            last_accel: [0.0; 3],
        }
    }

    fn translate(&mut self, ev: GuardianEvent) {
        match ev {
            GuardianEvent::Connected(name) => {
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name: name.clone(),
                    id: name,
                    ..Default::default()
                }));
            }

            GuardianEvent::Disconnected => {
                self.pending.push_back(DeviceEvent::Disconnected);
            }

            GuardianEvent::DeviceInfo(info) => {
                // Emit as Meta so session runner can log device details.
                let val = serde_json::json!({
                    "mac_address": info.mac_address,
                    "firmware_version": info.firmware_version,
                    "hardware_version": info.hardware_version,
                });
                self.pending.push_back(DeviceEvent::Meta(val));
            }

            GuardianEvent::Eeg(reading) => {
                let ts = if reading.timestamp > 0.0 {
                    reading.timestamp / 1000.0 // ms -> s
                } else {
                    now_secs()
                };

                // Each packet may contain multiple samples; emit one EegFrame per sample.
                if let Some(ref samples) = reading.samples {
                    let sample_dt = 1.0 / IDUN_SAMPLE_RATE;
                    for (i, &uv) in samples.iter().enumerate() {
                        self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                            channels: vec![uv],
                            timestamp_s: ts + (i as f64) * sample_dt,
                        }));
                    }
                }
                // If samples is None (raw-only), we skip — no decoded data available.
            }

            GuardianEvent::Accelerometer(reading) => {
                self.last_accel = [reading.sample.x, reading.sample.y, reading.sample.z];
                self.pending.push_back(DeviceEvent::Imu(ImuFrame {
                    accel: self.last_accel,
                    gyro: None,
                    mag: None,
                }));
            }

            GuardianEvent::Gyroscope(reading) => {
                self.pending.push_back(DeviceEvent::Imu(ImuFrame {
                    accel: self.last_accel,
                    gyro: Some([reading.sample.x, reading.sample.y, reading.sample.z]),
                    mag: None,
                }));
            }

            GuardianEvent::Battery(bat) => {
                self.pending.push_back(DeviceEvent::Battery(BatteryFrame {
                    level_pct: bat.level as f32,
                    voltage_mv: None,
                    temperature_raw: None,
                }));
            }

            GuardianEvent::Impedance(_) => {
                // Impedance monitoring is diagnostic only; not forwarded.
            }
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for IdunAdapter {
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
            let _ = h.stop_recording().await;
            let _ = h.disconnect().await;
        }
    }
}
