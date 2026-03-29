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
use emotiv::protocol::{
    CORTEX_CLOSE_SESSION, CORTEX_STOP_ALL_STREAMS, HEADSET_CONNECTION_FAILED, HEADSET_DISCONNECTED,
};
use skill_constants::{emotiv_sample_rate_from_id, EEG_CHANNELS, EMOTIV_EPOC_CHANNEL_NAMES, EMOTIV_EPOC_EEG_CHANNELS};

use super::{BatteryFrame, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, EegFrame, ImuFrame};

// ── EmotivAdapter ─────────────────────────────────────────────────────────────

/// Non-electrode column names in the Cortex EEG stream that must be
/// filtered out when mapping DataLabels to electrode names.
const EEG_NON_ELECTRODE: &[&str] = &[
    "COUNTER",
    "INTERPOLATED",
    "MARKER",
    "MARKER_HARDWARE",
    "MARKERS",
    "TIMESTAMP",
    "RAW_CQ",
    "BATTERY",
];

/// Returns `true` if a Cortex EEG column label is an actual electrode.
fn is_electrode(label: &str) -> bool {
    let u = label.to_uppercase();
    !EEG_NON_ELECTRODE.iter().any(|&non| u == non)
}

pub struct EmotivAdapter {
    rx: mpsc::Receiver<CortexEvent>,
    handle: Option<CortexHandle>,
    desc: DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,
    /// Whether the descriptor has been auto-adjusted from DataLabels.
    auto_detected: bool,
    /// Headset ID (e.g. "INSIGHT-5AF2C39E") for display purposes.
    headset_id: String,
    /// Indices into the raw EEG sample array that correspond to actual
    /// electrodes (set from DataLabels).  Empty until DataLabels arrives,
    /// in which case all samples are forwarded.
    electrode_indices: Vec<usize>,
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
        headset_id: String,
    ) -> Self {
        let sample_rate = emotiv_sample_rate_from_id(&headset_id);
        Self {
            rx,
            handle: Some(handle),
            desc: DeviceDescriptor {
                kind: "emotiv",
                caps: DeviceCaps::EEG | DeviceCaps::IMU | DeviceCaps::BATTERY,
                eeg_channels,
                eeg_sample_rate: sample_rate,
                channel_names,
                pipeline_channels: eeg_channels.min(EEG_CHANNELS),
                ppg_channel_names: Vec::new(),
                imu_channel_names: vec![
                    "AccelX".into(),
                    "AccelY".into(),
                    "AccelZ".into(),
                    "GyroX".into(),
                    "GyroY".into(),
                    "GyroZ".into(),
                ],
                fnirs_channel_names: Vec::new(),
            },
            pending: VecDeque::new(),
            auto_detected: false,
            headset_id,
            electrode_indices: Vec::new(),
        }
    }

    /// Convenience constructor for the common EPOC X / EPOC+ (14-channel) case.
    ///
    /// If `initial_info` is provided, a synthetic `Connected` event is queued
    /// so the session runner sees it immediately.  Use this when the connect
    /// factory has already consumed `SessionCreated` from the channel (to wait
    /// for the auth flow to complete before subscribing).
    pub fn new_epoc(
        rx: mpsc::Receiver<CortexEvent>,
        handle: CortexHandle,
        headset_id: String,
        initial_info: Option<DeviceInfo>,
    ) -> Self {
        let channel_names: Vec<String> = EMOTIV_EPOC_CHANNEL_NAMES.iter().map(|s| (*s).to_owned()).collect();
        let mut adapter = Self::new(rx, handle, EMOTIV_EPOC_EEG_CHANNELS, channel_names, headset_id);
        if let Some(info) = initial_info {
            adapter.pending.push_back(DeviceEvent::Connected(info));
        }
        adapter
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
                eeg_sample_rate: emotiv_sample_rate_from_id("TEST-HEADSET"),
                channel_names,
                pipeline_channels: eeg_channels.min(EEG_CHANNELS),
                ppg_channel_names: Vec::new(),
                imu_channel_names: vec![
                    "AccelX".into(),
                    "AccelY".into(),
                    "AccelZ".into(),
                    "GyroX".into(),
                    "GyroY".into(),
                    "GyroZ".into(),
                ],
                fnirs_channel_names: Vec::new(),
            },
            pending: VecDeque::new(),
            auto_detected: false,
            headset_id: "TEST-HEADSET".into(),
            electrode_indices: Vec::new(),
        }
    }

    /// Replay events that were consumed during the connect phase (e.g.
    /// DataLabels from subscribe confirmation).  Translates each event
    /// through the normal handler and queues the resulting DeviceEvents.
    pub fn replay(&mut self, events: Vec<CortexEvent>) {
        for ev in events {
            self.translate(ev);
        }
    }

    fn translate(&mut self, ev: CortexEvent) {
        match ev {
            CortexEvent::SessionCreated(session_id) => {
                // Use headset ID as the display name (e.g. "INSIGHT-5AF2C39E")
                // so the UI shows the actual device model.
                let name = if self.headset_id.is_empty() {
                    "Emotiv".to_owned()
                } else {
                    self.headset_id.clone()
                };
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name,
                    id: session_id,
                    ..Default::default()
                }));
            }

            CortexEvent::Disconnected => {
                self.pending.push_back(DeviceEvent::Disconnected);
            }

            CortexEvent::Eeg(data) => {
                // Extract only electrode values from the raw Cortex EEG array.
                // The array contains non-electrode columns (COUNTER, INTERPOLATED,
                // RAW_CQ, MARKERS, etc.) that must be skipped.
                let channels: Vec<f64> = if !self.electrode_indices.is_empty() {
                    self.electrode_indices
                        .iter()
                        .map(|&i| data.samples.get(i).copied().unwrap_or(f64::NAN))
                        .collect()
                } else {
                    // DataLabels hasn't arrived yet — forward all samples.
                    data.samples.clone()
                };

                if !channels.is_empty() {
                    self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                        channels,
                        timestamp_s: data.time,
                    }));
                }
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
                // The Cortex API sends column labels after subscribing, e.g.:
                //   ["COUNTER","INTERPOLATED","AF3","T7","Pz","T8","AF4",
                //    "RAW_CQ","MARKER_HARDWARE","MARKERS"]
                // We need to know which indices are actual electrodes so we
                // can extract only those from the raw EEG sample array.
                let mut indices = Vec::new();
                let mut names = Vec::new();
                for (i, label) in labels.labels.iter().enumerate() {
                    if is_electrode(label) {
                        indices.push(i);
                        names.push(label.clone());
                    }
                }

                if !names.is_empty() {
                    self.electrode_indices = indices;
                    self.desc.eeg_channels = names.len();
                    self.desc.pipeline_channels = names.len().min(EEG_CHANNELS);
                    self.desc.channel_names = names;
                    self.auto_detected = true;
                }
            }

            CortexEvent::Warning { code, .. }
                if code == CORTEX_STOP_ALL_STREAMS
                    || code == CORTEX_CLOSE_SESSION
                    || code == HEADSET_DISCONNECTED
                    || code == HEADSET_CONNECTION_FAILED =>
            {
                self.pending.push_back(DeviceEvent::Disconnected);
            }

            CortexEvent::Error(ref msg) => {
                // Only treat connection-level errors as disconnects.
                // Subscribe failures (e.g. "Subscribe 'eeg' failed") are
                // NOT disconnects — the WebSocket is still open and other
                // streams (mot, dev) may still be working.  Treating them
                // as disconnects causes an infinite reconnect loop.
                let is_subscribe_error = msg.contains("Subscribe '") && msg.contains("failed");
                if !is_subscribe_error {
                    self.pending.push_back(DeviceEvent::Disconnected);
                }
                // Subscribe errors are logged by the connect flow and
                // surfaced as a toast — no action needed here.
            }

            // Performance metrics, band power, mental commands, facial expressions,
            // system events, records, markers, profiles, other warnings — not
            // forwarded to session runner.
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
        // The Cortex WebSocket may continue sending non-data events
        // (heartbeats, system events, performance metrics) even after
        // the headset physically disconnects.  These events are silently
        // consumed by translate() without producing DeviceEvents, which
        // means this function never returns and the session runner's
        // watchdog never fires.
        //
        // Fix: track how long we've been spinning without producing a
        // DeviceEvent.  If it exceeds the watchdog timeout, return None
        // to let the session runner detect the stall.
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(skill_constants::DATA_WATCHDOG_SECS);

        loop {
            if let Some(ev) = self.pending.pop_front() {
                return Some(ev);
            }
            match tokio::time::timeout_at(deadline, self.rx.recv()).await {
                Ok(Some(vendor_ev)) => self.translate(vendor_ev),
                Ok(None) => return None, // channel closed
                Err(_) => return None,   // no data events for DATA_WATCHDOG_SECS
            }
        }
    }

    async fn disconnect(&mut self) {
        if let Some(ref h) = self.handle {
            let _ = h.close_session().await;
        }
    }
}
