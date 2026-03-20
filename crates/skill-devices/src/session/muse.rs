// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for the Muse 2 / Muse S headband.
//!
//! Muse delivers EEG as one-electrode-at-a-time packets (12 samples per
//! electrode per notification).  This adapter accumulates per-electrode
//! samples and emits aligned multi-channel [`EegFrame`]s — one frame per
//! time-step across all channels — so the session runner never has to deal
//! with partial data.

use std::collections::VecDeque;

use tokio::sync::mpsc;

use muse_rs::prelude::*;
use skill_constants::{CHANNEL_NAMES, EEG_CHANNELS, MUSE_SAMPLE_RATE};

use super::{
    BatteryFrame, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo,
    EegFrame, ImuFrame, PpgFrame, now_secs,
};

// ── MuseAdapter ───────────────────────────────────────────────────────────────

/// Number of EEG electrodes on a Muse (Classic or Athena, base 4).
const MUSE_EEG_CHANNELS: usize = 4;

pub struct MuseAdapter {
    rx:     mpsc::Receiver<MuseEvent>,
    handle: MuseHandle,
    desc:   DeviceDescriptor,

    /// Per-electrode sample queues for aligning Muse's one-electrode-at-a-time
    /// delivery into multi-channel frames.
    ch_bufs: [VecDeque<f64>; MUSE_EEG_CHANNELS],
    /// Per-electrode timestamp of the first buffered sample (seconds).
    ch_ts: [f64; MUSE_EEG_CHANNELS],

    /// Pre-built multi-channel frames waiting to be yielded by `next_event`.
    pending: VecDeque<DeviceEvent>,

    /// Last known accel values so gyro events can pair with them.
    last_accel: [f32; 3],
}

impl MuseAdapter {
    pub fn new(rx: mpsc::Receiver<MuseEvent>, handle: MuseHandle) -> Self {
        let channel_names: Vec<String> =
            CHANNEL_NAMES.iter().map(|s| (*s).to_owned()).collect();

        Self {
            rx,
            handle,
            desc: DeviceDescriptor {
                kind: "muse",
                caps: DeviceCaps::EEG
                    | DeviceCaps::PPG
                    | DeviceCaps::IMU
                    | DeviceCaps::BATTERY
                    | DeviceCaps::META,
                eeg_channels: MUSE_EEG_CHANNELS,
                eeg_sample_rate: MUSE_SAMPLE_RATE as f64,
                channel_names,
                pipeline_channels: MUSE_EEG_CHANNELS.min(EEG_CHANNELS),
            },
            ch_bufs: Default::default(),
            ch_ts: [0.0; MUSE_EEG_CHANNELS],
            pending: VecDeque::new(),
            last_accel: [0.0; 3],
        }
    }

    // ── Channel accumulator ──────────────────────────────────────────────────

    /// Buffer samples from one electrode and drain aligned multi-channel frames.
    fn accumulate_eeg(&mut self, r: &EegReading) {
        let ch = r.electrode;
        if ch >= MUSE_EEG_CHANNELS {
            return;
        }

        let ts = if r.timestamp > 0.0 {
            r.timestamp / 1000.0
        } else {
            now_secs()
        };

        if self.ch_bufs[ch].is_empty() {
            self.ch_ts[ch] = ts;
        }
        self.ch_bufs[ch].extend(r.samples.iter().copied());

        // Drain as many complete frames as possible (all channels have data).
        loop {
            let min_len = self.ch_bufs[..MUSE_EEG_CHANNELS]
                .iter()
                .map(|b| b.len())
                .min()
                .unwrap_or(0);
            if min_len == 0 {
                break;
            }

            // Use the average of the per-channel timestamps for this frame.
            let avg_ts: f64 =
                self.ch_ts[..MUSE_EEG_CHANNELS].iter().sum::<f64>() / MUSE_EEG_CHANNELS as f64;

            for _ in 0..min_len {
                let channels: Vec<f64> = (0..MUSE_EEG_CHANNELS)
                    .filter_map(|c| self.ch_bufs[c].pop_front())
                    .collect();
                self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                    channels,
                    timestamp_s: avg_ts,
                }));
            }

            // Advance timestamps by the samples drained.
            let dt = min_len as f64 / self.desc.eeg_sample_rate;
            for ts in &mut self.ch_ts[..MUSE_EEG_CHANNELS] {
                *ts += dt;
            }
        }
    }

    /// Translate a vendor `MuseEvent` into zero or more `DeviceEvent`s,
    /// pushing them onto `self.pending`.
    fn translate(&mut self, ev: MuseEvent) {
        match ev {
            MuseEvent::Connected(name) => {
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name: name.clone(),
                    id: name,
                    ..Default::default()
                }));
            }

            MuseEvent::Disconnected => {
                self.pending.push_back(DeviceEvent::Disconnected);
            }

            MuseEvent::Eeg(r) => {
                self.accumulate_eeg(&r);
            }

            MuseEvent::Ppg(r) => {
                let ts = if r.timestamp > 0.0 {
                    r.timestamp / 1000.0
                } else {
                    now_secs()
                };
                let samples: Vec<f64> = r.samples.iter().map(|&v| v as f64).collect();
                self.pending.push_back(DeviceEvent::Ppg(PpgFrame {
                    channel: r.ppg_channel,
                    samples,
                    timestamp_s: ts,
                }));
            }

            MuseEvent::Accelerometer(imu) => {
                let last = imu.samples[2];
                self.last_accel = [last.x, last.y, last.z];
                // Emit all 3 sub-samples.
                for s in &imu.samples {
                    self.pending.push_back(DeviceEvent::Imu(ImuFrame {
                        accel: [s.x, s.y, s.z],
                        gyro: None,
                        mag: None,
                    }));
                }
            }

            MuseEvent::Gyroscope(imu) => {
                for s in &imu.samples {
                    self.pending.push_back(DeviceEvent::Imu(ImuFrame {
                        accel: self.last_accel,
                        gyro: Some([s.x, s.y, s.z]),
                        mag: None,
                    }));
                }
            }

            MuseEvent::Telemetry(t) => {
                self.pending.push_back(DeviceEvent::Battery(BatteryFrame {
                    level_pct: t.battery_level,
                    voltage_mv: Some(t.fuel_gauge_voltage),
                    temperature_raw: Some(t.temperature),
                }));
            }

            MuseEvent::Control(c) => {
                let val = serde_json::Value::Object(c.fields);
                self.pending.push_back(DeviceEvent::Meta(val));
            }
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for MuseAdapter {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }

    async fn next_event(&mut self) -> Option<DeviceEvent> {
        loop {
            // Drain any pending events first.
            if let Some(ev) = self.pending.pop_front() {
                return Some(ev);
            }

            // Wait for the next vendor event.
            let vendor_ev = self.rx.recv().await?;
            self.translate(vendor_ev);
            // Loop back to drain pending.
        }
    }

    async fn disconnect(&mut self) {
        let _ = self.handle.disconnect().await;
    }
}
