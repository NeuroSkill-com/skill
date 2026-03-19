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
use emotiv::protocol::{CORTEX_STOP_ALL_STREAMS, CORTEX_CLOSE_SESSION};
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
    /// Whether the descriptor has been auto-adjusted from the first EEG packet.
    /// Cortex may send fewer channels than EPOC's 14 if an Insight (5-ch) or
    /// MN8 (2-ch) is connected.
    auto_detected: bool,
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
            auto_detected: false,
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
            auto_detected: false,
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
                eprintln!("[emotiv-adapter] CortexEvent::Disconnected received");
                self.pending.push_back(DeviceEvent::Disconnected);
            }

            CortexEvent::Eeg(data) => {
                // Auto-detect actual channel count from the first EEG packet.
                // The Cortex API streams exactly as many channels as the
                // connected headset has (14 for EPOC, 5 for Insight, etc.).
                let actual_ch = data.samples.len();
                if !self.auto_detected && actual_ch > 0 && actual_ch != self.desc.eeg_channels {
                    self.auto_detected = true;
                    self.desc.eeg_channels = actual_ch;
                    self.desc.pipeline_channels = actual_ch.min(EEG_CHANNELS);
                    // Trim or extend channel names to match.
                    self.desc.channel_names.truncate(actual_ch);
                    while self.desc.channel_names.len() < actual_ch {
                        self.desc.channel_names.push(format!("Ch{}", self.desc.channel_names.len() + 1));
                    }
                } else if !self.auto_detected {
                    self.auto_detected = true;
                }

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

            CortexEvent::DataLabels(labels) if labels.stream_name == "eeg" => {
                // The Cortex API sends EEG column labels after subscribing.
                // The first two are always COUNTER and INTERPOLATED; the rest
                // are electrode names (e.g. ["AF3","F7",…] for EPOC, or
                // ["AF3","AF4","T7","T8","Pz"] for Insight).
                let eeg_labels: Vec<String> = labels.labels.iter()
                    .filter(|l| {
                        let u = l.to_uppercase();
                        u != "COUNTER" && u != "INTERPOLATED"
                            && u != "MARKER" && u != "MARKER_HARDWARE"
                            && u != "TIMESTAMP"
                    })
                    .cloned()
                    .collect();

                if !eeg_labels.is_empty() && eeg_labels.len() != self.desc.eeg_channels {
                    self.desc.eeg_channels     = eeg_labels.len();
                    self.desc.pipeline_channels = eeg_labels.len().min(EEG_CHANNELS);
                    self.desc.channel_names     = eeg_labels;
                    self.auto_detected = true;
                }
            }

            CortexEvent::Warning { code, ref message }
                if code == CORTEX_STOP_ALL_STREAMS || code == CORTEX_CLOSE_SESSION =>
            {
                eprintln!("[emotiv-adapter] disconnect warning code={code} message={message}");
                self.pending.push_back(DeviceEvent::Disconnected);
            }

            CortexEvent::Warning { code, ref message } => {
                eprintln!("[emotiv-adapter] warning code={code} message={message}");
                // Other warnings are informational — not forwarded.
            }

            CortexEvent::Error(ref e) => {
                eprintln!("[emotiv-adapter] error: {e}");
                self.pending.push_back(DeviceEvent::Disconnected);
            }

            // Performance metrics, band power, mental commands, facial expressions,
            // system events, records, markers, profiles — not forwarded.
            other => {
                eprintln!("[emotiv-adapter] ignored event: {other:?}");
            }
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
            match self.rx.recv().await {
                Some(vendor_ev) => self.translate(vendor_ev),
                None => {
                    eprintln!("[emotiv-adapter] event channel closed (rx returned None)");
                    return None;
                }
            }
        }
    }

    async fn disconnect(&mut self) {
        if let Some(ref h) = self.handle {
            let _ = h.close_session().await;
        }
    }
}
