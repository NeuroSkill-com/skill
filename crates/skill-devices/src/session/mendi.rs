// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for the Mendi fNIRS headband.
//!
//! Mendi streams optical fNIRS frame data (IR/Red/Ambient), IMU readings,
//! temperature, and battery status over BLE.
//!
//! Each frame emits:
//!   - [`DeviceEvent::Fnirs`] — 9 raw optical channels in the order defined by
//!     `fnirs_channel_names`: IR-left, IR-right, IR-pulse, Red-left, Red-right,
//!     Red-pulse, Ambient-left, Ambient-right, Ambient-pulse.
//!   - [`DeviceEvent::Imu`] — accelerometer + gyroscope from the same frame.
//!   - [`DeviceEvent::Meta`] — temperature and other diagnostics.

use std::collections::VecDeque;

use mendi::prelude::*;
use tokio::sync::mpsc;

use super::{
    now_secs, BatteryFrame, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, FnirsFrame, ImuFrame,
};

pub struct MendiAdapter {
    rx: mpsc::Receiver<MendiEvent>,
    handle: Option<MendiHandle>,
    desc: DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,
}

impl MendiAdapter {
    pub fn new(rx: mpsc::Receiver<MendiEvent>, handle: MendiHandle) -> Self {
        Self {
            rx,
            handle: Some(handle),
            desc: DeviceDescriptor {
                kind: "mendi",
                caps: DeviceCaps::FNIRS | DeviceCaps::IMU | DeviceCaps::BATTERY | DeviceCaps::META,
                eeg_channels: 0,
                eeg_sample_rate: 0.0,
                channel_names: Vec::new(),
                pipeline_channels: 0,
                ppg_channel_names: Vec::new(),
                imu_channel_names: vec![
                    "AccelX".into(),
                    "AccelY".into(),
                    "AccelZ".into(),
                    "GyroX".into(),
                    "GyroY".into(),
                    "GyroZ".into(),
                ],
                fnirs_channel_names: vec![
                    "IR Left".into(),
                    "IR Right".into(),
                    "IR Pulse".into(),
                    "Red Left".into(),
                    "Red Right".into(),
                    "Red Pulse".into(),
                    "Ambient Left".into(),
                    "Ambient Right".into(),
                    "Ambient Pulse".into(),
                ],
            },
            pending: VecDeque::new(),
        }
    }

    fn translate(&mut self, ev: MendiEvent) {
        match ev {
            MendiEvent::Connected(info) => {
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name: info.name,
                    id: info.id,
                    firmware_version: info.firmware_version,
                    hardware_version: info.hardware_version,
                    ..Default::default()
                }));
            }
            MendiEvent::Disconnected => {
                self.pending.push_back(DeviceEvent::Disconnected);
            }
            MendiEvent::Frame(frame) => {
                let ts = if frame.timestamp > 0.0 {
                    frame.timestamp / 1000.0
                } else {
                    now_secs()
                };

                // fNIRS optical channels in the canonical order declared by
                // fnirs_channel_names: IR L/R/Pulse, Red L/R/Pulse, Amb L/R/Pulse.
                self.pending.push_back(DeviceEvent::Fnirs(FnirsFrame {
                    channels: vec![
                        frame.ir_left as f64,
                        frame.ir_right as f64,
                        frame.ir_pulse as f64,
                        frame.red_left as f64,
                        frame.red_right as f64,
                        frame.red_pulse as f64,
                        frame.amb_left as f64,
                        frame.amb_right as f64,
                        frame.amb_pulse as f64,
                    ],
                    timestamp_s: ts,
                }));

                self.pending.push_back(DeviceEvent::Imu(ImuFrame {
                    accel: [frame.accel_x_g(), frame.accel_y_g(), frame.accel_z_g()],
                    gyro: Some([frame.gyro_x_dps(), frame.gyro_y_dps(), frame.gyro_z_dps()]),
                    mag: None,
                }));

                // Temperature and other diagnostics go to Meta for WS broadcast.
                self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                    "source": "mendi_frame",
                    "timestamp_s": ts,
                    "temperature_c": frame.temperature,
                })));
            }
            MendiEvent::Battery(b) => {
                self.pending.push_back(DeviceEvent::Battery(BatteryFrame {
                    level_pct: b.percentage() as f32,
                    voltage_mv: Some(b.voltage_mv as f32),
                    temperature_raw: None,
                }));
            }
            MendiEvent::Calibration(c) => {
                self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                    "source": "mendi_calibration",
                    "timestamp_s": c.timestamp / 1000.0,
                    "offset_left": c.offset_left,
                    "offset_right": c.offset_right,
                    "offset_pulse": c.offset_pulse,
                    "auto_calibration": c.auto_calibration,
                    "low_power_mode": c.low_power_mode,
                })));
            }
            MendiEvent::Diagnostics(d) => {
                self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                    "source": "mendi_diagnostics",
                    "timestamp_s": d.timestamp / 1000.0,
                    "imu_ok": d.imu_ok,
                    "sensor_ok": d.sensor_ok,
                    "adc": d.adc.as_ref().map(|a| serde_json::json!({
                        "voltage_mv": a.voltage_mv,
                        "charging": a.charging,
                        "usb_connected": a.usb_connected,
                    })),
                })));
            }
            MendiEvent::SensorRead(s) => {
                self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                    "source": "mendi_sensor_read",
                    "timestamp_s": s.timestamp / 1000.0,
                    "address": s.address,
                    "data": s.data,
                })));
            }
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for MendiAdapter {
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
            let _ = h.disable_sensor().await;
            let _ = h.disconnect().await;
        }
    }
}
