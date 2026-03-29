// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for Cognionics / CGX EEG headsets.
//!
//! CGX headsets (Quick-20, Quick-20r, Quick-32r, Quick-8r, etc.) stream
//! multi-channel EEG data over USB serial (FTDI dongle) at up to 500 Hz.
//! The [`cognionics`] crate handles device scanning, serial framing, and
//! 24-bit ADC decoding.  This adapter translates [`CgxEvent`]s into the
//! unified [`DeviceEvent`] vocabulary.

use std::collections::VecDeque;

use cognionics::prelude::*;
use tokio::sync::mpsc;

use skill_constants::{CGX_CHANNEL_NAMES, CGX_EEG_CHANNELS, CGX_SAMPLE_RATE, EEG_CHANNELS};

use super::{now_secs, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, EegFrame};

// ── CognionicsAdapter ─────────────────────────────────────────────────────────

pub struct CognionicsAdapter {
    rx: mpsc::Receiver<CgxEvent>,
    handle: Option<CgxHandle>,
    desc: DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,
}

impl CognionicsAdapter {
    /// Create a new adapter from a cognionics event receiver and handle.
    ///
    /// The `handle` provides device metadata (model, channel names, sample
    /// rate) and is used to stop the background reader on disconnect.
    pub fn new(rx: mpsc::Receiver<CgxEvent>, handle: CgxHandle) -> Self {
        let eeg_channels = handle.num_eeg_channels();
        let sample_rate = handle.sampling_rate();
        let channel_names: Vec<String> = handle.signal_channel_names().iter().map(|s| (*s).to_string()).collect();

        // Determine IMU channel names from the device config's accelerometer indices.
        let imu_channel_names: Vec<String> = if !handle.device_config.acc_indices.is_empty() {
            vec!["ACCX".into(), "ACCY".into(), "ACCZ".into()]
        } else {
            Vec::new()
        };

        let caps = if imu_channel_names.is_empty() {
            DeviceCaps::EEG | DeviceCaps::META
        } else {
            DeviceCaps::EEG | DeviceCaps::IMU | DeviceCaps::META
        };

        Self {
            rx,
            handle: Some(handle),
            desc: DeviceDescriptor {
                kind: "cognionics",
                caps,
                eeg_channels,
                eeg_sample_rate: sample_rate,
                channel_names,
                pipeline_channels: eeg_channels.min(EEG_CHANNELS),
                ppg_channel_names: Vec::new(),
                imu_channel_names,
                fnirs_channel_names: Vec::new(),
            },
            pending: VecDeque::new(),
        }
    }

    /// Construct with default Quick-20r parameters (for use when handle
    /// metadata is not yet available, e.g. auto-scan).
    pub fn new_default(rx: mpsc::Receiver<CgxEvent>, handle: CgxHandle) -> Self {
        Self::new(rx, handle)
    }

    fn translate(&mut self, ev: CgxEvent) {
        match ev {
            CgxEvent::Connected(description) => {
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name: description.clone(),
                    id: description,
                    ..Default::default()
                }));
            }
            CgxEvent::Disconnected => {
                self.pending.push_back(DeviceEvent::Disconnected);
            }
            CgxEvent::Sample(sample) => {
                // Extract only EEG channels (exclude ExG/ACC from the signal
                // channels list).  The cognionics crate's `signal_channel_names`
                // includes EEG + ExG + ACC; we take only the first
                // `eeg_channels` values which correspond to the EEG electrodes.
                let eeg_count = self.desc.eeg_channels;
                let channels: Vec<f64> = if sample.channels.len() >= eeg_count {
                    sample.channels[..eeg_count].to_vec()
                } else {
                    sample.channels.clone()
                };

                self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                    channels,
                    timestamp_s: sample.timestamp,
                }));
            }
            CgxEvent::PacketLoss {
                lost,
                prev_counter,
                curr_counter,
            } => {
                self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                    "source": "cgx_packet_loss",
                    "timestamp_s": now_secs(),
                    "lost": lost,
                    "prev_counter": prev_counter,
                    "curr_counter": curr_counter,
                })));
            }
            CgxEvent::Error(msg) => {
                self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                    "source": "cgx_error",
                    "timestamp_s": now_secs(),
                    "message": msg,
                })));
            }
            CgxEvent::Info(msg) => {
                self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                    "source": "cgx_info",
                    "timestamp_s": now_secs(),
                    "message": msg,
                })));
            }
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for CognionicsAdapter {
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
            h.stop();
        }
    }
}
