// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for Vital Neuro EEG headphones.
//!
//! Vital Neuro is a dual-mode headphone: audio over Bluetooth Classic (A2DP,
//! handled by the OS) and EEG over BLE, which is what the `vital` crate and
//! this adapter speak.  The front end is a TI ADS1299 running **3 channels at
//! 250 SPS**, plus an accelerometer and battery/charger telemetry.
//!
//! One vendor notification carries **all three channels for two consecutive
//! sample instants**, so each `VitalEvent::Eeg` fans out into two aligned
//! [`EegFrame`]s 4 ms apart rather than one frame per channel.

use std::collections::VecDeque;

use tokio::sync::mpsc;

use skill_constants::{EEG_CHANNELS, VITAL_CHANNEL_NAMES, VITAL_EEG_CHANNELS, VITAL_SAMPLE_RATE};

// Deliberately not `vital::prelude::*`: the prelude exports its own `EegFrame`
// and would shadow the unified one this module emits.
use vital::impedance::ImpedanceEstimator;
use vital::protocol::{EEG_SAMPLES_PER_PACKET, EEG_SAMPLE_INTERVAL_MS};
use vital::types::{EegFrame as VitalEegFrame, ImpedanceFrame as VitalImpedanceFrame, VitalEvent};
use vital::VitalHandle;

use super::{
    now_secs, BatteryFrame, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, EegFrame, ImuFrame,
};

/// Impedance cycles averaged per estimate (~256 ms at 62.5 Hz excitation).
///
/// A single impedance sample is one point on a sine wave, so the estimator
/// needs whole cycles before it means anything.  16 trades latency for a
/// stable number — electrode contact does not change faster than that.
const IMPEDANCE_AVERAGING_CYCLES: usize = 16;

// ── VitalAdapter ──────────────────────────────────────────────────────────────

pub struct VitalAdapter {
    rx: mpsc::Receiver<VitalEvent>,
    handle: Option<VitalHandle>,
    desc: DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,

    /// Read once over GATT at connect time and attached to the `Connected`
    /// event — the device sends no firmware notification of its own.
    firmware_version: Option<String>,

    /// Transport id of the device this adapter is attached to, when the
    /// caller connected to a specific one.
    ///
    /// Reported instead of the name, which is the only identifier the vendor
    /// event carries and which the owner can change: the session runner uses
    /// this as the device's stable key.
    device_id: Option<String>,

    /// Demodulates impedance notifications into one ohms figure per electrode.
    /// Only fed in [`vital::protocol::SensorMode::Impedance`].
    impedance: ImpedanceEstimator,
}

/// Static descriptor for the hardware — three channels, 250 Hz, IMU, battery.
fn descriptor() -> DeviceDescriptor {
    DeviceDescriptor {
        kind: "vital",
        caps: DeviceCaps::EEG | DeviceCaps::IMU | DeviceCaps::BATTERY | DeviceCaps::META,
        eeg_channels: VITAL_EEG_CHANNELS,
        eeg_sample_rate: VITAL_SAMPLE_RATE,
        channel_names: VITAL_CHANNEL_NAMES.iter().map(|s| (*s).to_owned()).collect(),
        pipeline_channels: VITAL_EEG_CHANNELS.min(EEG_CHANNELS),
        ppg_channel_names: Vec::new(),
        imu_channel_names: vec!["AccelX".into(), "AccelY".into(), "AccelZ".into()],
        fnirs_channel_names: Vec::new(),
    }
}

impl VitalAdapter {
    pub fn new(
        rx: mpsc::Receiver<VitalEvent>,
        handle: VitalHandle,
        firmware_version: Option<String>,
        device_id: Option<String>,
    ) -> Self {
        Self {
            rx,
            handle: Some(handle),
            desc: descriptor(),
            pending: VecDeque::new(),
            firmware_version,
            device_id,
            impedance: ImpedanceEstimator::with_averaging(IMPEDANCE_AVERAGING_CYCLES),
        }
    }

    /// Test-only constructor without a real BLE handle.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn new_for_test(rx: mpsc::Receiver<VitalEvent>) -> Self {
        Self {
            rx,
            handle: None,
            desc: descriptor(),
            pending: VecDeque::new(),
            firmware_version: None,
            device_id: None,
            impedance: ImpedanceEstimator::with_averaging(IMPEDANCE_AVERAGING_CYCLES),
        }
    }

    /// Test-only: attach the transport id the real constructor receives.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_device_id(mut self, id: &str) -> Self {
        self.device_id = Some(id.to_owned());
        self
    }

    /// Fan one notification out into the two sample instants it carries.
    ///
    /// Channels the active sensor mode does not stream are filled with `0.0`
    /// rather than dropped: every adapter must emit frames whose width equals
    /// `descriptor.eeg_channels`, and the single-channel modes would otherwise
    /// change the frame shape mid-session.
    fn push_eeg(&mut self, frame: &VitalEegFrame) {
        // The device supplies no clock; `vital` extrapolates one host-side in
        // milliseconds. Guard against a frame built without it.
        let base_s = if frame.timestamp > 0.0 {
            frame.timestamp / 1000.0
        } else {
            now_secs()
        };
        let sample_dt = EEG_SAMPLE_INTERVAL_MS / 1000.0;

        for i in 0..EEG_SAMPLES_PER_PACKET {
            let channels: Vec<f64> = (0..VITAL_EEG_CHANNELS)
                .map(|c| frame.channel(c).map_or(0.0, |s| s.uv[i]))
                .collect();
            self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                channels,
                timestamp_s: base_s + (i as f64) * sample_dt,
            }));
        }
    }

    /// Accumulate an impedance notification, emitting one `Meta` event per
    /// completed estimate.
    ///
    /// Impedance shares the EEG characteristic on this device, so these only
    /// arrive while the sensor mode is `Impedance` — never during a normal
    /// recording.
    fn push_impedance(&mut self, frame: &VitalImpedanceFrame) {
        let Some(estimate) = self.impedance.push(frame) else {
            return;
        };
        let ohms: Vec<Option<f64>> = estimate.ohms().to_vec();
        let (worst_channel, worst_ohms) = match estimate.worst() {
            Some((ch, ohms)) => (Some(ch), Some(ohms)),
            None => (None, None),
        };
        self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
            "type": "impedance",
            "ohms": ohms,
            "worst_channel": worst_channel,
            "worst_ohms": worst_ohms,
            // Bit n = channel n's positive electrode reported off. The only
            // lead-off field this device populates, and only in this mode.
            "lead_off_positive": frame.lead_off.positive,
        })));
    }

    fn translate(&mut self, ev: VitalEvent) {
        match ev {
            VitalEvent::Connected(name) => {
                let id = self.device_id.clone().unwrap_or_else(|| name.clone());
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name,
                    id,
                    firmware_version: self.firmware_version.clone(),
                    ..Default::default()
                }));
            }

            VitalEvent::Disconnected => {
                // Samples from before a reconnect must not be averaged with
                // samples from after it.
                self.impedance.reset();
                self.pending.push_back(DeviceEvent::Disconnected);
            }

            VitalEvent::Eeg(frame) => self.push_eeg(&frame),

            VitalEvent::Impedance(frame) => self.push_impedance(&frame),

            VitalEvent::Accelerometer(a) => {
                let g = a.g();
                self.pending.push_back(DeviceEvent::Imu(ImuFrame {
                    accel: [g[0] as f32, g[1] as f32, g[2] as f32],
                    gyro: None,
                    mag: None,
                }));
            }

            VitalEvent::Battery(b) => {
                self.pending.push_back(DeviceEvent::Battery(BatteryFrame {
                    // `percent()` can report up to 104 between 4150 and 4199 mV
                    // — the device's own curve overshoots its clamp.
                    level_pct: f32::from(b.percent_clamped()),
                    voltage_mv: Some(f32::from(b.millivolts())),
                    temperature_raw: None,
                }));
            }

            VitalEvent::ModeChanged(mode) => {
                // The partial window belongs to the old mode — and in EEG mode
                // the same characteristic carries something else entirely, so
                // carrying it across the boundary would average two different
                // signals into one impedance figure.
                self.impedance.reset();
                // Which stream the EEG characteristic is now carrying; nothing
                // in the notification bytes distinguishes EEG from impedance.
                self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                    "type": "mode",
                    "mode": mode.name(),
                })));
            }

            // Only meaningful while deliberately driving a firmware update,
            // which this adapter never does.
            VitalEvent::FirmwareProgress(_) => {}
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for VitalAdapter {
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
            // Park the front end before dropping the link so the device is not
            // left sampling into a closed connection.
            let _ = h.stop().await;
            let _ = h.disconnect().await;
        }
    }
}
