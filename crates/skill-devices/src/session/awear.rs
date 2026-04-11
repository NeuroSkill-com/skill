// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for the AWEAR EEG wearable.
//!
//! AWEAR is a single-channel BLE EEG device using the LUCA protocol with
//! HMAC-SHA256 authentication.  Each EEG notification contains ~256 signed
//! 16-bit samples which are converted to µV and emitted as individual
//! [`EegFrame`]s.

use std::collections::VecDeque;

use tokio::sync::mpsc;

use awear::prelude::*;
use skill_constants::{AWEAR_CHANNEL_NAMES, AWEAR_EEG_CHANNELS, AWEAR_SAMPLE_RATE, EEG_CHANNELS};

use super::{now_secs, BatteryFrame, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, EegFrame};

// ── Conversion ───────────────────────────────────────────────────────────────

/// Scale factor from raw 16-bit ADC counts to µV.
///
/// AWEAR uses a 16-bit signed ADC.  Assuming ±187.5 mV full-scale range
/// (typical for single-channel consumer EEG), the LSB is:
///   187500 µV / 32768 ≈ 5.72 µV/count
const RAW_TO_UV: f64 = 187_500.0 / 32_768.0;

// ── AwearAdapter ─────────────────────────────────────────────────────────────

pub struct AwearAdapter {
    rx: mpsc::Receiver<AwearEvent>,
    handle: Option<AwearHandle>,
    desc: DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,
}

impl AwearAdapter {
    pub fn new(rx: mpsc::Receiver<AwearEvent>, handle: AwearHandle) -> Self {
        let channel_names: Vec<String> = AWEAR_CHANNEL_NAMES.iter().map(|s| (*s).to_owned()).collect();

        Self {
            rx,
            handle: Some(handle),
            desc: DeviceDescriptor {
                kind: "awear",
                caps: DeviceCaps::EEG | DeviceCaps::BATTERY,
                eeg_channels: AWEAR_EEG_CHANNELS,
                eeg_sample_rate: AWEAR_SAMPLE_RATE,
                channel_names,
                pipeline_channels: AWEAR_EEG_CHANNELS.min(EEG_CHANNELS),
                ppg_channel_names: Vec::new(),
                imu_channel_names: Vec::new(),
                fnirs_channel_names: Vec::new(),
            },
            pending: VecDeque::new(),
        }
    }

    /// Test-only constructor without a real BLE handle.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_for_test(rx: mpsc::Receiver<AwearEvent>) -> Self {
        let channel_names: Vec<String> = AWEAR_CHANNEL_NAMES.iter().map(|s| (*s).to_owned()).collect();

        Self {
            rx,
            handle: None,
            desc: DeviceDescriptor {
                kind: "awear",
                caps: DeviceCaps::EEG | DeviceCaps::BATTERY,
                eeg_channels: AWEAR_EEG_CHANNELS,
                eeg_sample_rate: AWEAR_SAMPLE_RATE,
                channel_names,
                pipeline_channels: AWEAR_EEG_CHANNELS.min(EEG_CHANNELS),
                ppg_channel_names: Vec::new(),
                imu_channel_names: Vec::new(),
                fnirs_channel_names: Vec::new(),
            },
            pending: VecDeque::new(),
        }
    }

    fn translate(&mut self, ev: AwearEvent) {
        match ev {
            AwearEvent::Connected(name) => {
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name: name.clone(),
                    id: name,
                    ..Default::default()
                }));
            }

            AwearEvent::Disconnected => {
                self.pending.push_back(DeviceEvent::Disconnected);
            }

            AwearEvent::Ready => {
                // Device authenticated and ready — no DeviceEvent equivalent.
            }

            AwearEvent::Eeg(reading) => {
                let ts = now_secs();
                let sample_dt = 1.0 / AWEAR_SAMPLE_RATE;

                for (i, &raw) in reading.samples.iter().enumerate() {
                    self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                        channels: vec![raw as f64 * RAW_TO_UV],
                        timestamp_s: ts + (i as f64) * sample_dt,
                    }));
                }
            }

            AwearEvent::Battery(level) => {
                self.pending.push_back(DeviceEvent::Battery(BatteryFrame {
                    level_pct: level as f32,
                    voltage_mv: None,
                    temperature_raw: None,
                }));
            }

            AwearEvent::Signal(_) | AwearEvent::Misc(_) | AwearEvent::Status(_) => {
                // Diagnostic / informational — not forwarded.
            }
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for AwearAdapter {
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
            let _ = h.stop().await;
            let _ = h.disconnect().await;
        }
    }
}
