// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for WHOOP straps (4.0, 5.0, MG).
//!
//! Built on the [`poop`] crate, which speaks the straps' BLE protocol over the
//! Web Bluetooth API — the same `webbluetooth` session every other BLE device
//! in this app uses, so a strap coexists with a headset on one radio.
//!
//! A strap is not an EEG device. Like the Oura ring it reports vitals it derived
//! itself, so this declares no EEG channels and emits [`DeviceEvent::Vitals`]
//! rather than feeding the PPG/DSP path:
//!
//! * Heart rate from the standard Heart Rate Measurement characteristic and —
//!   when [`WhoopConfig::realtime_hr`] is set — from the proprietary live
//!   packet, which carries the strap's own clock reading.
//! * **Every R-R interval**, not just the beat rate. A strap reports a bpm and
//!   the intervals behind it as separate fields, and the intervals are what HRV
//!   is computed from, so each one is emitted rather than collapsed.
//! * Battery state of charge, with pack voltage where the source carries it.
//!
//! ## Historical heart rate is deliberately dropped
//!
//! `poop` delivers a strap's offloaded history through the same event channel
//! as live data, tagged [`HrSource::Historical`]. Those are *past* measurements,
//! and emitting them as live vitals would backfill a recording with readings
//! that did not happen during it — stamped with the moment they were
//! transferred. They are skipped here; draining history into the health store is
//! a separate concern, as it is for the ring.
//!
//! [`poop`]: https://docs.rs/poop

use std::collections::VecDeque;

use poop::prelude::*;
use tokio::sync::mpsc;
use tracing::info;

use super::{
    now_secs, BatteryFrame, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, VitalsFrame,
};

/// How to find and open a strap.
#[derive(Clone, Debug)]
pub struct WhoopConfig {
    /// Advertised-name prefix to match. `None` accepts any strap advertising a
    /// WHOOP service, which is the right default: the name is user-settable and
    /// carries no identity.
    pub name_prefix: Option<String>,
    /// How long to scan before giving up.
    pub scan_timeout_secs: u64,
    /// Ask for the proprietary live-HR packet as well as the standard
    /// characteristic.
    ///
    /// On by default here, unlike in `poop`: this adapter exists to drive a live
    /// recording session, which is exactly the case the flag is for. It does
    /// change strap state, so a caller that only wants history should clear it.
    pub realtime_hr: bool,
}

impl Default for WhoopConfig {
    fn default() -> Self {
        Self {
            name_prefix: None,
            scan_timeout_secs: 8,
            realtime_hr: true,
        }
    }
}

impl WhoopConfig {
    /// The `poop` client configuration this asks for.
    pub fn to_client_config(&self) -> WhoopClientConfig {
        WhoopClientConfig {
            name_prefix: self.name_prefix.clone(),
            scan_timeout_secs: self.scan_timeout_secs,
            // Free and available unbonded, so it stays on as a floor under the
            // proprietary stream.
            standard_hr: true,
            realtime_hr: self.realtime_hr,
            ..Default::default()
        }
    }
}

pub struct WhoopAdapter {
    rx: mpsc::Receiver<WhoopEvent>,
    handle: Option<WhoopHandle>,
    desc: DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,
}

/// The descriptor every strap reports: vitals and battery, no electrodes.
fn descriptor() -> DeviceDescriptor {
    DeviceDescriptor {
        kind: "whoop",
        caps: DeviceCaps::VITALS | DeviceCaps::BATTERY | DeviceCaps::META,
        eeg_channels: 0,
        eeg_sample_rate: 0.0,
        channel_names: Vec::new(),
        pipeline_channels: 0,
        ppg_channel_names: Vec::new(),
        imu_channel_names: Vec::new(),
        fnirs_channel_names: Vec::new(),
    }
}

impl WhoopAdapter {
    pub fn new(rx: mpsc::Receiver<WhoopEvent>, handle: WhoopHandle) -> Self {
        Self {
            rx,
            handle: Some(handle),
            desc: descriptor(),
            pending: VecDeque::new(),
        }
    }

    fn translate(&mut self, ev: WhoopEvent) {
        match ev {
            WhoopEvent::Connected { name, family } => {
                // The strap reports no stable id in this event and its name is
                // user-settable, so the family is the only identity available
                // until `Hello` arrives with a firmware string.
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name,
                    id: String::new(),
                    hardware_version: Some(format!("{family:?}")),
                    ..Default::default()
                }));
            }
            WhoopEvent::Disconnected => {
                self.pending.push_back(DeviceEvent::Disconnected);
            }
            WhoopEvent::HeartRate(hr) => self.translate_heart_rate(hr),
            WhoopEvent::Battery(b) => {
                self.pending.push_back(DeviceEvent::Battery(BatteryFrame {
                    level_pct: b.percent,
                    voltage_mv: b.millivolts.map(f32::from),
                    temperature_raw: None,
                }));
            }
            // `fw` is the key the session runner extracts a firmware version
            // from, so reporting it here populates device identity without
            // needing a dedicated event type.
            WhoopEvent::Hello(h) => {
                self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                    "source": "whoop_hello",
                    "fw": h.firmware,
                    "battery_percent": h.battery_percent,
                    "strap_unix_seconds": h.unix_seconds,
                    "preparation_bits": h.preparation_bits,
                })));
                self.pending.push_back(DeviceEvent::Battery(BatteryFrame {
                    level_pct: h.battery_percent,
                    voltage_mv: None,
                    temperature_raw: None,
                }));
            }
            WhoopEvent::Event(e) => {
                self.pending.push_back(DeviceEvent::Meta(serde_json::json!({
                    "source": "whoop_strap_event",
                    "number": e.number,
                    "name": e.name,
                    "strap_unix_seconds": e.unix_seconds,
                })));
                // A `BATTERY_LEVEL` event carries a reading on WHOOP 4.
                if let Some(b) = e.battery {
                    self.pending.push_back(DeviceEvent::Battery(BatteryFrame {
                        level_pct: b.percent,
                        voltage_mv: b.millivolts.map(f32::from),
                        temperature_raw: None,
                    }));
                }
            }
            // Offload control, replies, firmware logs, undecoded frames and
            // rejected envelopes: useful for protocol work, not session data.
            WhoopEvent::Record(_)
            | WhoopEvent::DataRange(_)
            | WhoopEvent::HistoryMeta(_)
            | WhoopEvent::CommandResponse(_)
            | WhoopEvent::ConsoleLog(_)
            | WhoopEvent::RawSensor { .. }
            | WhoopEvent::Rejected(_) => {}
        }
    }

    /// One heart-rate observation becomes one vitals sample per beat interval.
    ///
    /// The bpm rides on the first sample only. Repeating it on each interval
    /// would multiply one reported rate into several identical "measurements",
    /// while dropping the extra intervals would discard exactly the data HRV
    /// needs — so the rate is reported once and every interval is kept.
    fn translate_heart_rate(&mut self, hr: HeartRate) {
        // See the module docs: history is not live data.
        if hr.source == HrSource::Historical {
            return;
        }

        let ts = now_secs();
        // Zero is the crate's documented conversion fallback, not a measured
        // rate, so it must not be stored as one.
        let bpm = (hr.bpm > 0).then(|| f64::from(hr.bpm));

        if hr.rr_ms.is_empty() {
            // A rate with no intervals behind it is still a rate.
            if bpm.is_some() {
                self.pending.push_back(DeviceEvent::Vitals(VitalsFrame {
                    timestamp_s: ts,
                    hr_bpm: bpm,
                    ..Default::default()
                }));
            }
            return;
        }

        for (i, rr) in hr.rr_ms.iter().enumerate() {
            self.pending.push_back(DeviceEvent::Vitals(VitalsFrame {
                timestamp_s: ts,
                hr_bpm: if i == 0 { bpm } else { None },
                ibi_ms: Some(f64::from(*rr)),
                ..Default::default()
            }));
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for WhoopAdapter {
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
            // Leave the strap as it was found: the live packet is something we
            // switched on, so switch it back off before dropping the link.
            let _ = h.toggle_realtime_hr(false).await;
            if let Err(e) = h.disconnect().await {
                info!(error = %e, "WHOOP disconnect reported an error");
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    /// A handle needs a live link, so translation is driven directly and the
    /// handle left out — which also covers the `disconnect`-without-one path.
    fn adapter() -> WhoopAdapter {
        let (_tx, rx) = mpsc::channel(32);
        WhoopAdapter {
            rx,
            handle: None,
            desc: descriptor(),
            pending: VecDeque::new(),
        }
    }

    fn hr(source: HrSource, bpm: u8, rr: &[u32]) -> HeartRate {
        HeartRate {
            source,
            bpm,
            rr_ms: rr.to_vec(),
            unix_seconds: None,
        }
    }

    fn vitals(a: &WhoopAdapter) -> Vec<VitalsFrame> {
        a.pending
            .iter()
            .map(|e| match e {
                DeviceEvent::Vitals(f) => f.clone(),
                other => panic!("expected Vitals, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn descriptor_declares_vitals_and_no_eeg() {
        let a = adapter();
        let d = a.descriptor();
        assert_eq!(d.kind, "whoop");
        assert!(d.caps.contains(DeviceCaps::VITALS));
        assert!(d.caps.contains(DeviceCaps::BATTERY));
        assert!(!d.caps.contains(DeviceCaps::EEG), "a strap has no electrodes");
        assert!(!d.caps.contains(DeviceCaps::PPG), "no waveform reaches the PPG path");
        assert_eq!(d.eeg_channels, 0);
    }

    /// The rate is reported once; every interval survives. Repeating the bpm
    /// would invent measurements; dropping intervals would lose the HRV data.
    #[test]
    fn one_observation_becomes_one_sample_per_interval() {
        let mut a = adapter();
        a.translate(WhoopEvent::HeartRate(hr(
            HrSource::RealtimePacket,
            58,
            &[1020, 1035, 1011],
        )));

        let v = vitals(&a);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].hr_bpm, Some(58.0));
        assert_eq!(v[0].ibi_ms, Some(1020.0));
        assert_eq!(v[1].hr_bpm, None, "bpm must not repeat");
        assert_eq!(v[1].ibi_ms, Some(1035.0));
        assert_eq!(v[2].ibi_ms, Some(1011.0));
    }

    /// The standard characteristic often reports a rate with no intervals.
    #[test]
    fn a_rate_without_intervals_is_still_reported() {
        let mut a = adapter();
        a.translate(WhoopEvent::HeartRate(hr(HrSource::StandardService, 61, &[])));
        let v = vitals(&a);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].hr_bpm, Some(61.0));
        assert_eq!(v[0].ibi_ms, None);
    }

    /// Offloaded history arrives on the same channel as live data. Emitting it
    /// would backfill a recording with measurements from another day.
    #[test]
    fn historical_heart_rate_is_not_emitted_as_live() {
        let mut a = adapter();
        a.translate(WhoopEvent::HeartRate(hr(HrSource::Historical, 47, &[1200, 1250])));
        assert!(a.pending.is_empty(), "history must not reach the live stream");
    }

    /// A zero bpm is the crate's conversion fallback, not a measured rate —
    /// but intervals delivered alongside it are still real.
    #[test]
    fn a_zero_rate_is_not_reported_as_a_measurement() {
        let mut a = adapter();
        a.translate(WhoopEvent::HeartRate(hr(HrSource::StandardService, 0, &[])));
        assert!(a.pending.is_empty());

        a.translate(WhoopEvent::HeartRate(hr(HrSource::StandardService, 0, &[1000])));
        let v = vitals(&a);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].hr_bpm, None);
        assert_eq!(v[0].ibi_ms, Some(1000.0));
    }

    #[test]
    fn battery_carries_percent_and_voltage() {
        let mut a = adapter();
        a.translate(WhoopEvent::Battery(Battery {
            source: BatterySource::StandardService,
            percent: 72.5,
            millivolts: Some(3912),
            charging: Some(false),
        }));
        match a.pending.front().unwrap() {
            DeviceEvent::Battery(b) => {
                assert_eq!(b.level_pct, 72.5);
                assert_eq!(b.voltage_mv, Some(3912.0));
            }
            other => panic!("expected Battery, got {other:?}"),
        }
    }

    /// `Hello` is where a firmware version comes from, and the runner reads it
    /// out of a `Meta` payload's `fw` key.
    #[test]
    fn hello_reports_firmware_under_the_key_the_runner_reads() {
        let mut a = adapter();
        a.translate(WhoopEvent::Hello(Hello {
            revision: 3,
            battery_percent: 88.0,
            unix_seconds: 1_700_000_000,
            firmware: "50.42.1.0".into(),
            preparation_bits: 0,
        }));
        match a.pending.front().unwrap() {
            DeviceEvent::Meta(v) => assert_eq!(v.get("fw").and_then(|f| f.as_str()), Some("50.42.1.0")),
            other => panic!("expected Meta, got {other:?}"),
        }
    }

    /// Offload and diagnostic traffic must not turn into session events.
    #[test]
    fn protocol_chatter_is_ignored() {
        let mut a = adapter();
        a.translate(WhoopEvent::ConsoleLog("boot".into()));
        a.translate(WhoopEvent::RawSensor {
            packet_type: 40,
            version: 1,
            raw: vec![0, 1, 2],
        });
        assert!(a.pending.is_empty());
    }

    #[test]
    fn disconnect_without_a_handle_is_safe() {
        let mut a = adapter();
        futures::executor::block_on(a.disconnect());
    }
}
