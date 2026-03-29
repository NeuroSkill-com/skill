// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for Cognionics / CGX EEG headsets.
//!
//! CGX headsets stream multi-channel EEG data over USB serial (FTDI dongle).
//! The [`cognionics`] crate handles device scanning, serial framing, and
//! 24-bit ADC decoding.  This adapter translates [`CgxEvent`]s into the
//! unified [`DeviceEvent`] vocabulary.
//!
//! ## Supported models
//!
//! | Model | EEG ch | ExG | ACC | Rate |
//! |---|---|---|---|---|
//! | Quick-20 | 20 | 4 | ✗ | 500 Hz |
//! | Quick-20r-v1 | 20 | 1 | ✓ | 500 Hz |
//! | Quick-20m | 20 | 1 | ✓ | 500 Hz |
//! | Quick-20r | 20 | 2 | ✓ | 500 Hz |
//! | Quick-32r | 30 | 2 | ✓ | 500 Hz |
//! | Quick-8r | 9 | 1 | ✓ | 500 Hz |
//! | AIM-2 | 0 | 11 | ✗ | 500 Hz |
//! | Dev Kit | 8 | 0 | ✓ | 500 Hz |
//! | Patch-v1 | 2 | 3 | ✓ | 250 Hz |
//! | Patch-v2 | 2 | 2 | ✓ | 250 Hz |
//!
//! The exact channel layout and sample rate are auto-detected from the USB
//! descriptor string and read from [`CgxHandle`] at connection time.

use std::collections::VecDeque;

use cognionics::prelude::*;
use tokio::sync::mpsc;

use skill_constants::EEG_CHANNELS;

use super::{now_secs, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, EegFrame, ImuFrame};

// ── CognionicsAdapter ─────────────────────────────────────────────────────────

pub struct CognionicsAdapter {
    rx: mpsc::Receiver<CgxEvent>,
    handle: Option<CgxHandle>,
    desc: DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,
    /// Indices into the signal-channel vector that correspond to
    /// accelerometer axes (ACCX, ACCY, ACCZ).  Empty if the model has no
    /// accelerometer.  These are indices into the *signal* channels
    /// (i.e. the `CgxSample::channels` vec), not the raw wire order.
    acc_signal_indices: Vec<usize>,
    /// Indices into the signal-channel vector for ExG channels.
    exg_signal_indices: Vec<usize>,
}

impl CognionicsAdapter {
    /// Create a new adapter from a cognionics event receiver and handle.
    ///
    /// The `handle` provides device metadata (model, channel count, channel
    /// names, sample rate) auto-detected from the USB descriptor string.
    /// All per-model differences (Quick-20 vs Quick-32r vs Patch-v2 etc.)
    /// are resolved by the cognionics crate before this point.
    pub fn new(rx: mpsc::Receiver<CgxEvent>, handle: CgxHandle) -> Self {
        let cfg = &handle.device_config;
        let eeg_count = cfg.eeg_indices.len();
        let sample_rate = handle.sampling_rate();
        let has_acc = !cfg.acc_indices.is_empty();

        // Build the EEG-only channel labels from the device config.
        let eeg_channel_names: Vec<String> = cfg.eeg_indices.iter().map(|&i| cfg.channels[i].to_string()).collect();

        // Build ExG channel labels (used for metadata events only).
        let _exg_channel_names: Vec<String> = cfg.exg_indices.iter().map(|&i| cfg.channels[i].to_string()).collect();

        // Determine IMU channel names.
        let imu_channel_names: Vec<String> = if has_acc {
            vec!["ACCX".into(), "ACCY".into(), "ACCZ".into()]
        } else {
            Vec::new()
        };

        // Map raw-wire ACC / ExG indices to their position in the
        // *signal-channel* vector (which excludes Packet Counter and TRIGGER).
        let signal_indices = cfg.signal_channel_indices();
        let acc_signal_indices: Vec<usize> = cfg
            .acc_indices
            .iter()
            .filter_map(|raw| signal_indices.iter().position(|&si| si == *raw))
            .collect();
        let exg_signal_indices: Vec<usize> = cfg
            .exg_indices
            .iter()
            .filter_map(|raw| signal_indices.iter().position(|&si| si == *raw))
            .collect();

        let mut caps = DeviceCaps::META;
        if eeg_count > 0 {
            caps |= DeviceCaps::EEG;
        }
        if has_acc {
            caps |= DeviceCaps::IMU;
        }

        Self {
            rx,
            handle: Some(handle),
            desc: DeviceDescriptor {
                kind: "cognionics",
                caps,
                eeg_channels: eeg_count,
                eeg_sample_rate: sample_rate,
                channel_names: eeg_channel_names,
                pipeline_channels: eeg_count.min(EEG_CHANNELS),
                ppg_channel_names: Vec::new(),
                imu_channel_names,
                fnirs_channel_names: Vec::new(),
            },
            pending: VecDeque::new(),
            acc_signal_indices,
            exg_signal_indices,
        }
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
                // ── EEG channels ──────────────────────────────────────────
                let eeg_count = self.desc.eeg_channels;
                if eeg_count > 0 && sample.channels.len() >= eeg_count {
                    let channels = sample.channels[..eeg_count].to_vec();
                    self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                        channels,
                        timestamp_s: sample.timestamp,
                    }));
                }

                // ── Accelerometer → IMU ───────────────────────────────────
                if self.acc_signal_indices.len() == 3 {
                    let ch = &sample.channels;
                    let ax = self.acc_signal_indices[0];
                    let ay = self.acc_signal_indices[1];
                    let az = self.acc_signal_indices[2];
                    if ch.len() > ax && ch.len() > ay && ch.len() > az {
                        self.pending.push_back(DeviceEvent::Imu(ImuFrame {
                            accel: [ch[ax] as f32, ch[ay] as f32, ch[az] as f32],
                            gyro: None,
                            mag: None,
                        }));
                    }
                }

                // ── ExG channels as metadata ──────────────────────────────
                if !self.exg_signal_indices.is_empty() {
                    let exg_vals: Vec<f64> = self
                        .exg_signal_indices
                        .iter()
                        .filter_map(|&i| sample.channels.get(i).copied())
                        .collect();
                    if !exg_vals.is_empty() {
                        self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                            "source": "cgx_exg",
                            "timestamp_s": sample.timestamp,
                            "channels": exg_vals,
                        })));
                    }
                }
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
