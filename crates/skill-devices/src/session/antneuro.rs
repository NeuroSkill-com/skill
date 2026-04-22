// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for ANT Neuro eego amplifiers.
//!
//! ANT Neuro manufactures research-grade EEG amplifiers (eego mylab, eego
//! sport, eego rt) supporting 8–256 REF channels at up to 16,384 Hz via
//! USB.  This adapter uses the async [`AntNeuroClient`] which streams
//! [`AntNeuroEvent`]s over a tokio channel.
//!
//! Each [`AntNeuroEvent::Eeg`] block may contain multiple samples; the
//! adapter unpacks them into individual [`EegFrame`]s.

use std::collections::VecDeque;

use antneuro::prelude::*;
use skill_constants::EEG_CHANNELS;
use tokio::sync::mpsc;

use super::{now_secs, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, EegFrame};

pub struct AntNeuroAdapter {
    rx: mpsc::Receiver<AntNeuroEvent>,
    handle: Option<AntNeuroHandle>,
    desc: DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,
}

impl AntNeuroAdapter {
    pub fn new(
        rx: mpsc::Receiver<AntNeuroEvent>,
        handle: AntNeuroHandle,
        eeg_channels: usize,
        sample_rate: f64,
    ) -> Self {
        Self {
            rx,
            handle: Some(handle),
            desc: DeviceDescriptor {
                kind: "antneuro",
                caps: DeviceCaps::EEG,
                eeg_channels,
                eeg_sample_rate: sample_rate,
                channel_names: default_channel_names(eeg_channels),
                pipeline_channels: eeg_channels.min(EEG_CHANNELS),
                ppg_channel_names: Vec::new(),
                imu_channel_names: Vec::new(),
                fnirs_channel_names: Vec::new(),
            },
            pending: VecDeque::new(),
        }
    }

    fn translate(&mut self, ev: AntNeuroEvent) {
        match ev {
            AntNeuroEvent::Connected(info) => {
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name: format!("ANT Neuro eego ({})", info.serial),
                    id: info.serial.clone(),
                    serial_number: Some(info.serial),
                    ..Default::default()
                }));
            }
            AntNeuroEvent::Disconnected => {
                self.pending.push_back(DeviceEvent::Disconnected);
            }
            AntNeuroEvent::Eeg(data) => {
                // Update channel count if it changed (first data block
                // reports the actual hardware configuration).
                if data.channel_count > 0 && data.channel_count != self.desc.eeg_channels {
                    self.desc.eeg_channels = data.channel_count;
                    self.desc.pipeline_channels = data.channel_count.min(EEG_CHANNELS);
                    self.desc.channel_names = default_channel_names(data.channel_count);
                }

                let ch = data.channel_count.max(1);
                let ts_base = if data.timestamp_ms > 0.0 {
                    data.timestamp_ms / 1000.0
                } else {
                    now_secs()
                };
                let sample_dt = 1.0 / self.desc.eeg_sample_rate;

                for s in 0..data.sample_count {
                    let offset = s * ch;
                    if offset + ch > data.samples.len() {
                        break;
                    }
                    let channels: Vec<f64> = data.samples[offset..offset + ch].to_vec();
                    self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                        channels,
                        timestamp_s: ts_base + (s as f64) * sample_dt,
                    }));
                }
            }
            AntNeuroEvent::Impedance(_) | AntNeuroEvent::Error(_) => {
                // Impedance and non-fatal errors — not forwarded.
            }
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for AntNeuroAdapter {
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
            h.disconnect();
        }
    }
}

/// Generate electrode names for ANT Neuro Waveguard cap configurations.
///
/// Returns standard 10-20/10-10/10-5 electrode labels for the most common
/// eego amplifier + cap combinations.  Falls back to generic `Ch1…ChN`
/// when the count does not match a known layout.
pub fn default_channel_names(count: usize) -> Vec<String> {
    match count {
        // eego 8 / eego rt 8 / eego sport 8 — minimal 10-20 subset
        8 => sv(&["Fp1", "Fp2", "F3", "F4", "C3", "C4", "P3", "P4"]),
        // eego 24 — Waveguard Net/Original 24-ch
        24 => sv(&[
            "Fp1", "Fp2", "F7", "F3", "Fz", "F4", "F8", "T7", "C3", "Cz", "C4", "T8", "P7", "P3", "Pz", "P4", "P8",
            "O1", "Oz", "O2", "FC1", "FC2", "CP1", "CP2",
        ]),
        // Standard 10-20 (21 electrodes) — Waveguard Connect 21-ch
        21 => sv(&[
            "Fp1", "Fp2", "F7", "F3", "Fz", "F4", "F8", "T7", "C3", "Cz", "C4", "T8", "P7", "P3", "Pz", "P4", "P8",
            "O1", "O2", "A1", "A2",
        ]),
        // IFCN 25-ch — Waveguard Connect 25-ch
        25 => sv(&[
            "Fp1", "Fp2", "F7", "F3", "Fz", "F4", "F8", "F9", "F10", "T7", "C3", "Cz", "C4", "T8", "T9", "T10", "P7",
            "P3", "Pz", "P4", "P8", "P9", "P10", "O1", "O2",
        ]),
        // Extended 10-20 (32 electrodes) — Waveguard Original/Touch 32-ch
        32 => sv(&[
            "Fp1", "Fp2", "F7", "F3", "Fz", "F4", "F8", "FC5", "FC1", "FC2", "FC6", "T7", "C3", "Cz", "C4", "T8",
            "CP5", "CP1", "CP2", "CP6", "P7", "P3", "Pz", "P4", "P8", "PO3", "POz", "PO4", "O1", "Oz", "O2", "AFz",
        ]),
        // Full 10-10 (64 electrodes) — Waveguard Original/Touch 64-ch
        64 => sv(&[
            "Fp1", "Fpz", "Fp2", "AF7", "AF3", "AFz", "AF4", "AF8", "F7", "F5", "F3", "F1", "Fz", "F2", "F4", "F6",
            "F8", "FT7", "FC5", "FC3", "FC1", "FCz", "FC2", "FC4", "FC6", "FT8", "T7", "C5", "C3", "C1", "Cz", "C2",
            "C4", "C6", "T8", "TP7", "CP5", "CP3", "CP1", "CPz", "CP2", "CP4", "CP6", "TP8", "P7", "P5", "P3", "P1",
            "Pz", "P2", "P4", "P6", "P8", "PO7", "PO3", "POz", "PO4", "PO8", "O1", "Oz", "O2", "Iz", "A1", "A2",
        ]),
        // 128/256-ch — too many for a static list; use generic names.
        _ => (1..=count).map(|i| format!("Ch{i}")).collect(),
    }
}

fn sv(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}
