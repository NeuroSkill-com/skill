// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for the Hermes V1 EEG headset.
//!
//! Hermes delivers 8-channel EEG at 250 Hz and 9-DOF IMU data over BLE GATT.
//! Multiple EEG samples per notification are emitted as individual
//! [`EegFrame`]s.

use std::collections::VecDeque;

use tokio::sync::mpsc;

use hermes_ble::prelude::*;
use skill_constants::{
    EEG_CHANNELS, HERMES_CHANNEL_NAMES, HERMES_EEG_CHANNELS, HERMES_SAMPLE_RATE,
};

use super::{
    DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, EegFrame, ImuFrame,
};

// ── HermesAdapter ─────────────────────────────────────────────────────────────

pub struct HermesAdapter {
    rx: mpsc::Receiver<HermesEvent>,
    /// `None` only in tests (no BLE hardware available).
    handle: Option<HermesHandle>,
    desc: DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,
}

impl HermesAdapter {
    pub fn new(rx: mpsc::Receiver<HermesEvent>, handle: HermesHandle) -> Self {
        let channel_names: Vec<String> = HERMES_CHANNEL_NAMES
            .iter()
            .map(|s| (*s).to_owned())
            .collect();

        Self {
            rx,
            handle: Some(handle),
            desc: DeviceDescriptor {
                kind: "hermes",
                caps: DeviceCaps::EEG | DeviceCaps::IMU,
                eeg_channels: HERMES_EEG_CHANNELS,
                eeg_sample_rate: HERMES_SAMPLE_RATE,
                channel_names,
                pipeline_channels: HERMES_EEG_CHANNELS.min(EEG_CHANNELS),
            },
            pending: VecDeque::new(),
        }
    }

    /// Test-only constructor without a real BLE handle.
    #[cfg(test)]
    pub(crate) fn new_for_test(rx: mpsc::Receiver<HermesEvent>) -> Self {
        let channel_names: Vec<String> = HERMES_CHANNEL_NAMES
            .iter()
            .map(|s| (*s).to_owned())
            .collect();

        Self {
            rx,
            handle: None,
            desc: super::DeviceDescriptor {
                kind: "hermes",
                caps: super::DeviceCaps::EEG | super::DeviceCaps::IMU,
                eeg_channels: HERMES_EEG_CHANNELS,
                eeg_sample_rate: HERMES_SAMPLE_RATE,
                channel_names,
                pipeline_channels: HERMES_EEG_CHANNELS.min(skill_constants::EEG_CHANNELS),
            },
            pending: std::collections::VecDeque::new(),
        }
    }

    fn translate(&mut self, ev: HermesEvent) {
        match ev {
            HermesEvent::Connected(name) => {
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name: name.clone(),
                    id: name,
                    ..Default::default()
                }));
            }

            HermesEvent::Disconnected => {
                self.pending.push_back(DeviceEvent::Disconnected);
            }

            HermesEvent::Eeg(sample) => {
                let ts = sample.timestamp / 1000.0; // ms → s
                self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                    channels: sample.channels.to_vec(),
                    timestamp_s: ts,
                }));
            }

            HermesEvent::Motion(m) => {
                self.pending.push_back(DeviceEvent::Imu(ImuFrame {
                    accel: [m.accel.x, m.accel.y, m.accel.z],
                    gyro: Some([m.gyro.x, m.gyro.y, m.gyro.z]),
                    mag: Some([m.mag.x, m.mag.y, m.mag.z]),
                }));
            }

            HermesEvent::PacketsDropped(_) | HermesEvent::Event(_) | HermesEvent::Config(_) => {
                // Not forwarded — diagnostic only.
            }
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for HermesAdapter {
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
            let _ = h.disconnect().await;
        }
    }
}
