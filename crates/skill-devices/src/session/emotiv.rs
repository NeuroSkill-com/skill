// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for Emotiv EEG headsets (EPOC X, EPOC+, Insight, Flex).
//!
//! Emotiv headsets communicate through the Cortex WebSocket API (JSON-RPC 2.0)
//! running on the local EMOTIV Launcher service.  The adapter translates
//! [`CortexEvent`]s into [`DeviceEvent`]s for the generic session runner.
//!
//! ## Supported models
//!
//! | Model      | EEG ch | Sample rate |
//! |------------|--------|-------------|
//! | EPOC X     | 14     | 128 Hz      |
//! | EPOC+      | 14     | 128 Hz      |
//! | Insight    | 5      | 128 Hz      |
//! | EPOC Flex  | 32     | 128 Hz      |

use std::collections::VecDeque;

use tokio::sync::mpsc;

use emotiv::prelude::*;
use skill_constants::{
    EEG_CHANNELS,
    EMOTIV_EPOC_EEG_CHANNELS, EMOTIV_EPOC_CHANNEL_NAMES,
    EMOTIV_SAMPLE_RATE,
};

use super::{
    BatteryFrame, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo,
    EegFrame, ImuFrame,
};

// ── EmotivAdapter ─────────────────────────────────────────────────────────────

pub struct EmotivAdapter {
    rx:      mpsc::Receiver<CortexEvent>,
    handle:  Option<CortexHandle>,
    desc:    DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,
}

impl EmotivAdapter {
    /// Create a new adapter from an active Cortex event channel and handle.
    ///
    /// `eeg_channels` and `channel_names` should match the connected headset
    /// model (14 for EPOC, 5 for Insight, etc.).
    pub fn new(
        rx: mpsc::Receiver<CortexEvent>,
        handle: CortexHandle,
        eeg_channels: usize,
        channel_names: Vec<String>,
    ) -> Self {
        Self {
            rx,
            handle: Some(handle),
            desc: DeviceDescriptor {
                kind: "emotiv",
                caps: DeviceCaps::EEG | DeviceCaps::IMU | DeviceCaps::BATTERY,
                eeg_channels,
                eeg_sample_rate: EMOTIV_SAMPLE_RATE,
                channel_names,
                pipeline_channels: eeg_channels.min(EEG_CHANNELS),
            },
            pending: VecDeque::new(),
        }
    }

    /// Convenience constructor for the common EPOC X / EPOC+ (14-channel) case.
    pub fn new_epoc(rx: mpsc::Receiver<CortexEvent>, handle: CortexHandle) -> Self {
        let channel_names: Vec<String> =
            EMOTIV_EPOC_CHANNEL_NAMES.iter().map(|s| (*s).to_owned()).collect();
        Self::new(rx, handle, EMOTIV_EPOC_EEG_CHANNELS, channel_names)
    }

    /// Test-only constructor without a real Cortex handle.
    #[cfg(test)]
    pub(crate) fn new_for_test(
        rx: mpsc::Receiver<CortexEvent>,
        eeg_channels: usize,
        channel_names: Vec<String>,
    ) -> Self {
        Self {
            rx,
            handle: None,
            desc: DeviceDescriptor {
                kind: "emotiv",
                caps: DeviceCaps::EEG | DeviceCaps::IMU | DeviceCaps::BATTERY,
                eeg_channels,
                eeg_sample_rate: EMOTIV_SAMPLE_RATE,
                channel_names,
                pipeline_channels: eeg_channels.min(EEG_CHANNELS),
            },
            pending: VecDeque::new(),
        }
    }

    fn translate(&mut self, ev: CortexEvent) {
        match ev {
            CortexEvent::SessionCreated(session_id) => {
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name: "Emotiv".into(),
                    id: session_id,
                    ..Default::default()
                }));
            }

            CortexEvent::Disconnected => {
                self.pending.push_back(DeviceEvent::Disconnected);
            }

            CortexEvent::Eeg(data) => {
                let channels: Vec<f64> = data.samples.iter()
                    .take(self.desc.eeg_channels)
                    .copied()
                    .collect();
                self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                    channels,
                    timestamp_s: data.time,
                }));
            }

            CortexEvent::Motion(data) => {
                // Cortex motion stream: [COUNTER, INTERP, Q0, Q1, Q2, Q3, ACCX, ACCY, ACCZ, MAGX, MAGY, MAGZ]
                let accel = if data.samples.len() >= 9 {
                    [data.samples[6] as f32, data.samples[7] as f32, data.samples[8] as f32]
                } else {
                    [0.0; 3]
                };
                let mag = if data.samples.len() >= 12 {
                    Some([data.samples[9] as f32, data.samples[10] as f32, data.samples[11] as f32])
                } else {
                    None
                };
                self.pending.push_back(DeviceEvent::Imu(ImuFrame {
                    accel,
                    gyro: None, // Cortex motion stream has quaternions, not raw gyro
                    mag,
                }));
            }

            CortexEvent::Dev(data) => {
                self.pending.push_back(DeviceEvent::Battery(BatteryFrame {
                    level_pct: data.battery_percent as f32,
                    voltage_mv: None,
                    temperature_raw: None,
                }));
            }

            // Performance metrics, band power, mental commands, facial expressions,
            // system events, records, markers, profiles — not forwarded to session runner.
            _ => {}
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for EmotivAdapter {
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
            let _ = h.close_session().await;
        }
    }
}
