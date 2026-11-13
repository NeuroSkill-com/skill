// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for EmotiBit boards.
//!
//! Built on the [`emotibit`] crate, which talks to a board either over WiFi
//! (stock firmware) or over Bluetooth LE (the `emotibit-ble` firmware). The BLE
//! path runs on the same `webbluetooth` session as every other BLE device here.
//!
//! An EmotiBit is not an EEG device, so this declares no EEG channels:
//!
//! * **Heart rate and inter-beat interval** (derived by the crate from the PPG
//!   stream) and **SpO2** become [`DeviceEvent::Vitals`].
//! * **Skin temperature** (`TEMP1`, the contact thermistor) joins them as
//!   `temperature_c`.
//! * **Accelerometer and gyroscope** arrive as one packet stream per axis; they
//!   are zipped into [`DeviceEvent::Imu`] frames.
//! * **Battery** comes from the crate's folded [`DeviceStatus`], which merges the
//!   voltage and percentage packets.
//! * **Everything else that is a scalar stream** (electrodermal activity and its
//!   level / response split, the SCR metrics, thermopile and contact
//!   temperatures, humidity, the magnetometer and the raw PPG waveform) becomes
//!   a [`DeviceEvent::Sensor`] and is recorded to the `_sensors` sidecar. See
//!   [`sensor_id`] for the stable channel ids.
//!
//! ## Why raw PPG is a sensor stream and not a PPG event
//!
//! The session's PPG path computes vitals once per 5 s epoch of a *fixed 64 Hz*
//! stream. An EmotiBit's PPG runs at 25 or 100 Hz, so feeding it in would write
//! wrong rows rather than none. As a sensor stream it is stored faithfully, with
//! its real timestamps, for analysis that knows the rate.
//!
//! ## What is still not forwarded
//!
//! SD-card fill, and any channel the crate adds that has no id here. They are
//! counted and logged once, so a missing stream is visible.
//!
//! [`emotibit`]: https://docs.rs/emotibit
//! [`DeviceStatus`]: emotibit::status::DeviceStatus

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use emotibit::prelude::*;
use tokio::sync::mpsc;
use tracing::info;

use super::{
    now_secs, BatteryFrame, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, ImuFrame,
    SensorFrame, VitalsFrame,
};

/// Per-axis queue cap. If one axis of a triplet never arrives, the others must
/// not grow without bound waiting for it.
const AXIS_QUEUE_CAP: usize = 4096;

/// A batch is spread over the time since its channel last reported, but only if
/// that gap is plausibly one packet interval. A longer silence (a dropout, a
/// reconnect) is not the span of this batch.
const MAX_BATCH_SPAN_S: f64 = 5.0;

/// The stable `channel` id a scalar sensor stream is recorded under, or `None`
/// for a channel that is routed elsewhere (vitals, motion, battery) or nowhere.
///
/// These ids are written into recordings, so they are never renamed: a changed
/// id would orphan every past session. They are spelled out rather than derived
/// from the crate's display names (`"PPG:IR"`, `"SCR:RIS"`) because those are
/// presentation and may be reworded.
pub fn sensor_id(tag: &str) -> Option<&'static str> {
    use emotibit::protocol::tag as t;
    Some(match tag {
        t::EDA => "eda",
        t::EDL => "eda_level",
        t::EDR => "eda_response",
        t::SCR_AMPLITUDE => "scr_amplitude",
        t::SCR_FREQUENCY => "scr_frequency",
        t::SCR_RISE_TIME => "scr_rise_time",
        t::PPG_INFRARED => "ppg_ir",
        t::PPG_RED => "ppg_red",
        t::PPG_GREEN => "ppg_green",
        t::THERMOPILE => "thermopile",
        t::TEMPERATURE_0 => "temp0",
        t::HUMIDITY_0 => "humidity",
        t::MAGNETOMETER_X => "mag_x",
        t::MAGNETOMETER_Y => "mag_y",
        t::MAGNETOMETER_Z => "mag_z",
        _ => return None,
    })
}

/// Timestamps for `n` samples that arrived together at `now`.
///
/// `prev` is when this channel last reported. A batch covers the time since
/// then, so its samples are spread evenly back from `now` over that gap rather
/// than stacked on one instant. With no usable gap (the first packet, a single
/// sample, or a long silence) they all take `now`.
fn spread_timestamps(now: f64, prev: Option<f64>, n: usize) -> Vec<f64> {
    let step = match prev {
        Some(p) if n > 1 && now > p && now - p < MAX_BATCH_SPAN_S => (now - p) / n as f64,
        _ => 0.0,
    };
    (0..n).map(|i| now - step * (n - 1 - i) as f64).collect()
}

/// How to find and open a board.
#[derive(Clone, Debug)]
pub struct EmotibitConfig {
    /// Restrict to one device id (e.g. `MD-V6-0000356`, `EmotiBit-BLE-E848`) or
    /// WiFi address. `None` takes the first available board.
    pub device: Option<String>,
    /// Which links to look on.
    pub transport: Transport,
    /// How long to scan before giving up.
    pub scan_timeout_secs: u64,
}

impl Default for EmotibitConfig {
    fn default() -> Self {
        Self {
            device: None,
            // Stock firmware is WiFi and the BLE firmware is a separate flash, so
            // an owner could have either; look for both.
            transport: Transport::Both,
            scan_timeout_secs: 10,
        }
    }
}

impl EmotibitConfig {
    /// The `emotibit` client configuration this asks for.
    pub fn to_client_config(&self) -> EmotiBitClientConfig {
        EmotiBitClientConfig {
            scan_timeout: Duration::from_secs(self.scan_timeout_secs),
            device: self.device.clone(),
            transport: self.transport,
            ..Default::default()
        }
    }
}

/// Per-axis sample queues for a three-axis sensor delivered as three streams.
#[derive(Default)]
struct Axes {
    q: [VecDeque<f64>; 3],
}

impl Axes {
    fn push(&mut self, axis: usize, samples: &[f64]) {
        let q = &mut self.q[axis];
        q.extend(samples.iter().copied());
        while q.len() > AXIS_QUEUE_CAP {
            q.pop_front();
        }
    }

    /// The oldest complete (x, y, z) triplet, if all three axes have one.
    fn pop(&mut self) -> Option<[f64; 3]> {
        if self.q.iter().any(VecDeque::is_empty) {
            return None;
        }
        Some([self.q[0].pop_front()?, self.q[1].pop_front()?, self.q[2].pop_front()?])
    }
}

pub struct EmotibitAdapter {
    rx: mpsc::Receiver<EmotiBitEvent>,
    handle: Option<EmotiBitHandle>,
    desc: DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,
    accel: Axes,
    /// Latest gyro reading per axis. Held rather than queued: it is sampled at
    /// the accelerometer's rate, and an accelerometer triplet is what paces an
    /// [`ImuFrame`].
    gyro: [Option<f64>; 3],
    /// Last battery percentage emitted, so unrelated status changes (button
    /// presses, clipping counters) do not repeat a battery event.
    last_battery: Option<f64>,
    /// When each sensor channel last reported, to time the next batch.
    last_sensor_at: HashMap<&'static str, f64>,
    /// Readings for channels with nowhere to go, for the one-time log line.
    unrouted: u64,
}

/// The descriptor every board reports: vitals, motion and battery, no electrodes.
fn descriptor() -> DeviceDescriptor {
    DeviceDescriptor {
        kind: "emotibit",
        caps: DeviceCaps::VITALS | DeviceCaps::IMU | DeviceCaps::BATTERY,
        eeg_channels: 0,
        eeg_sample_rate: 0.0,
        channel_names: Vec::new(),
        pipeline_channels: 0,
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
    }
}

impl EmotibitAdapter {
    pub fn new(rx: mpsc::Receiver<EmotiBitEvent>, handle: EmotiBitHandle) -> Self {
        Self {
            rx,
            handle: Some(handle),
            desc: descriptor(),
            pending: VecDeque::new(),
            accel: Axes::default(),
            gyro: [None; 3],
            last_battery: None,
            last_sensor_at: HashMap::new(),
            unrouted: 0,
        }
    }

    fn translate(&mut self, ev: EmotiBitEvent) {
        match ev {
            EmotiBitEvent::Connected(d) => {
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name: d.id.clone(),
                    id: d.id,
                    ..Default::default()
                }));
            }
            EmotiBitEvent::Disconnected => self.pending.push_back(DeviceEvent::Disconnected),
            EmotiBitEvent::Sensor(r) => self.translate_reading(&r),
            EmotiBitEvent::Status(s) => {
                // Only a changed percentage is a battery event; `Status` also
                // fires for button presses and clipping counters.
                if let Some(pct) = s.battery_percent.filter(|p| self.last_battery != Some(*p)) {
                    self.last_battery = Some(pct);
                    self.pending.push_back(DeviceEvent::Battery(BatteryFrame {
                        level_pct: pct as f32,
                        voltage_mv: s.battery_voltage.map(|v| (v * 1000.0) as f32),
                        temperature_raw: None,
                    }));
                }
            }
            // Info, clipping/overflow notices, button presses and replies.
            EmotiBitEvent::Message(_) => {}
        }
    }

    fn translate_reading(&mut self, r: &SensorReading) {
        use emotibit::protocol::tag;

        match r.channel.tag {
            tag::HEART_RATE => self.vitals_each(r, |v, x| v.hr_bpm = Some(x), |x| x > 0.0),
            tag::INTER_BEAT_INTERVAL => self.vitals_each(r, |v, x| v.ibi_ms = Some(x), |x| x > 0.0),
            tag::SPO2 => self.vitals_each(r, |v, x| v.spo2_pct = Some(x), |x| x > 0.0),
            tag::TEMPERATURE_1 => self.vitals_each(r, |v, x| v.temperature_c = Some(x), |_| true),
            tag::ACCELEROMETER_X => self.accel_axis(0, r),
            tag::ACCELEROMETER_Y => self.accel_axis(1, r),
            tag::ACCELEROMETER_Z => self.accel_axis(2, r),
            tag::GYROSCOPE_X => self.gyro_axis(0, r),
            tag::GYROSCOPE_Y => self.gyro_axis(1, r),
            tag::GYROSCOPE_Z => self.gyro_axis(2, r),
            // Battery is taken from the folded `Status` event instead.
            tag::BATTERY_PERCENT | tag::BATTERY_VOLTAGE => {}
            _ => match sensor_id(r.channel.tag) {
                Some(id) => self.sensor(id, r),
                None => {
                    if self.unrouted == 0 {
                        info!(
                            channel = r.channel.name,
                            "EmotiBit channel has no session route; not forwarded"
                        );
                    }
                    self.unrouted += 1;
                }
            },
        }
    }

    /// One reading becomes one [`SensorFrame`] with a timestamp per sample.
    /// Non-finite values are dropped, and a reading with none left emits nothing.
    fn sensor(&mut self, id: &'static str, r: &SensorReading) {
        let values: Vec<f64> = r.samples.iter().copied().filter(|v| v.is_finite()).collect();
        if values.is_empty() {
            return;
        }
        let now = now_secs();
        let times = spread_timestamps(now, self.last_sensor_at.insert(id, now), values.len());
        self.pending.push_back(DeviceEvent::Sensor(SensorFrame {
            channel: id.to_string(),
            unit: r.channel.unit.to_string(),
            samples: times.into_iter().zip(values).collect(),
        }));
    }

    /// One vitals sample per accepted value. A reading can carry several samples
    /// (the stock firmware batches), and each is its own measurement. Non-finite
    /// values are always dropped, and `keep` drops the firmware's zero "no value
    /// yet" markers.
    fn vitals_each(&mut self, r: &SensorReading, set: impl Fn(&mut VitalsFrame, f64), keep: impl Fn(f64) -> bool) {
        let ts = now_secs();
        for &x in r.samples.iter().filter(|x| x.is_finite() && keep(**x)) {
            let mut v = VitalsFrame {
                timestamp_s: ts,
                ..Default::default()
            };
            set(&mut v, x);
            self.pending.push_back(DeviceEvent::Vitals(v));
        }
    }

    fn accel_axis(&mut self, axis: usize, r: &SensorReading) {
        self.accel.push(axis, &r.samples);
        while let Some([x, y, z]) = self.accel.pop() {
            let gyro = match self.gyro {
                [Some(gx), Some(gy), Some(gz)] => Some([gx as f32, gy as f32, gz as f32]),
                _ => None,
            };
            self.pending.push_back(DeviceEvent::Imu(ImuFrame {
                accel: [x as f32, y as f32, z as f32],
                gyro,
                mag: None,
            }));
        }
    }

    fn gyro_axis(&mut self, axis: usize, r: &SensorReading) {
        if let Some(&v) = r.samples.last() {
            self.gyro[axis] = Some(v);
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for EmotibitAdapter {
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
            if let Err(e) = h.disconnect().await {
                info!(error = %e, "EmotiBit disconnect reported an error");
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use emotibit::status::DeviceStatus;
    use emotibit::types::channel;

    /// A handle needs a live link, so translation is driven directly and the
    /// handle left out, which also covers the `disconnect`-without-one path.
    fn adapter() -> EmotibitAdapter {
        let (_tx, rx) = mpsc::channel(32);
        EmotibitAdapter {
            rx,
            handle: None,
            desc: descriptor(),
            pending: VecDeque::new(),
            accel: Axes::default(),
            gyro: [None; 3],
            last_battery: None,
            last_sensor_at: HashMap::new(),
            unrouted: 0,
        }
    }

    fn reading(tag: &str, samples: &[f64]) -> EmotiBitEvent {
        EmotiBitEvent::Sensor(SensorReading {
            channel: channel(tag).unwrap(),
            packet_number: 0,
            device_timestamp_ms: 0,
            received_at_ms: 0.0,
            samples: samples.to_vec(),
        })
    }

    fn drain(a: &mut EmotibitAdapter) -> Vec<DeviceEvent> {
        a.pending.drain(..).collect()
    }

    #[test]
    fn descriptor_declares_vitals_and_motion_but_no_eeg() {
        let d = descriptor();
        assert_eq!(d.kind, "emotibit");
        assert!(d
            .caps
            .contains(DeviceCaps::VITALS | DeviceCaps::IMU | DeviceCaps::BATTERY));
        assert!(!d.caps.contains(DeviceCaps::EEG), "a wristband has no electrodes");
        assert!(
            !d.caps.contains(DeviceCaps::PPG),
            "25/100 Hz PPG must not reach the 64 Hz analyser"
        );
        assert_eq!(d.eeg_channels, 0);
    }

    #[test]
    fn heart_rate_and_ibi_become_vitals() {
        let mut a = adapter();
        a.translate(reading("HR", &[61.0]));
        a.translate(reading("BI", &[984.0]));
        let ev = drain(&mut a);
        assert_eq!(ev.len(), 2);
        match (&ev[0], &ev[1]) {
            (DeviceEvent::Vitals(h), DeviceEvent::Vitals(i)) => {
                assert_eq!(h.hr_bpm, Some(61.0));
                assert_eq!(i.ibi_ms, Some(984.0));
            }
            other => panic!("expected two Vitals, got {other:?}"),
        }
    }

    /// The stock firmware batches samples; each is a separate measurement.
    #[test]
    fn a_batched_reading_yields_one_sample_per_value() {
        let mut a = adapter();
        a.translate(reading("T1", &[33.1, 33.2, 33.3]));
        assert_eq!(drain(&mut a).len(), 3);
    }

    /// Zero is the "no value yet" marker, not a measurement.
    #[test]
    fn zero_heart_rate_and_nonfinite_values_are_dropped() {
        let mut a = adapter();
        a.translate(reading("HR", &[0.0]));
        a.translate(reading("T1", &[f64::NAN]));
        assert!(drain(&mut a).is_empty());
    }

    #[test]
    fn accelerometer_axes_are_zipped_into_one_frame() {
        let mut a = adapter();
        a.translate(reading("AX", &[0.1]));
        a.translate(reading("AY", &[0.2]));
        assert!(drain(&mut a).is_empty(), "two axes are not a frame");
        a.translate(reading("AZ", &[0.98]));
        match drain(&mut a).as_slice() {
            [DeviceEvent::Imu(f)] => {
                assert_eq!(f.accel, [0.1, 0.2, 0.98]);
                assert!(f.gyro.is_none(), "no gyro has arrived yet");
            }
            other => panic!("expected one Imu, got {other:?}"),
        }
    }

    #[test]
    fn gyro_is_attached_once_all_three_axes_have_arrived() {
        let mut a = adapter();
        a.translate(reading("GX", &[1.0]));
        a.translate(reading("GY", &[2.0]));
        a.translate(reading("GZ", &[3.0]));
        assert!(drain(&mut a).is_empty(), "gyro alone does not pace a frame");
        for (t, v) in [("AX", 0.0), ("AY", 0.0), ("AZ", 1.0)] {
            a.translate(reading(t, &[v]));
        }
        match drain(&mut a).as_slice() {
            [DeviceEvent::Imu(f)] => assert_eq!(f.gyro, Some([1.0, 2.0, 3.0])),
            other => panic!("expected one Imu, got {other:?}"),
        }
    }

    #[test]
    fn a_batched_accelerometer_yields_a_frame_per_triplet() {
        let mut a = adapter();
        a.translate(reading("AX", &[1.0, 2.0]));
        a.translate(reading("AY", &[1.0, 2.0]));
        a.translate(reading("AZ", &[1.0, 2.0]));
        assert_eq!(drain(&mut a).len(), 2);
    }

    #[test]
    fn a_missing_axis_cannot_grow_the_queue_without_bound() {
        let mut a = adapter();
        let burst = vec![0.0; AXIS_QUEUE_CAP + 500];
        a.translate(reading("AX", &burst));
        assert_eq!(a.accel.q[0].len(), AXIS_QUEUE_CAP);
    }

    #[test]
    fn battery_is_reported_once_per_change() {
        let mut a = adapter();
        let status = |pct, presses| DeviceStatus {
            battery_percent: Some(pct),
            battery_voltage: Some(3.9),
            short_presses: presses,
            ..Default::default()
        };
        a.translate(EmotiBitEvent::Status(status(80.0, 0)));
        a.translate(EmotiBitEvent::Status(status(80.0, 1)));
        a.translate(EmotiBitEvent::Status(status(79.0, 1)));
        let ev = drain(&mut a);
        assert_eq!(ev.len(), 2, "a button press must not repeat the battery");
        match &ev[0] {
            DeviceEvent::Battery(b) => {
                assert_eq!(b.level_pct, 80.0);
                assert_eq!(b.voltage_mv, Some(3900.0));
            }
            other => panic!("expected Battery, got {other:?}"),
        }
    }

    fn sensors(a: &mut EmotibitAdapter) -> Vec<SensorFrame> {
        drain(a)
            .into_iter()
            .map(|e| match e {
                DeviceEvent::Sensor(f) => f,
                other => panic!("expected Sensor, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn eda_is_recorded_as_a_sensor_stream_with_its_unit() {
        let mut a = adapter();
        a.translate(reading("EA", &[0.42]));
        let f = sensors(&mut a);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].channel, "eda");
        assert_eq!(f[0].unit, "µS");
        assert_eq!(f[0].samples.len(), 1);
        assert_eq!(f[0].samples[0].1, 0.42);
    }

    /// Raw PPG must not reach the PPG event: that path assumes 64 Hz.
    #[test]
    fn raw_ppg_is_a_sensor_stream_not_a_ppg_event() {
        let mut a = adapter();
        a.translate(reading("PR", &[12345.0]));
        let f = sensors(&mut a);
        assert_eq!(f[0].channel, "ppg_red");
    }

    #[test]
    fn every_documented_sensor_channel_has_a_stable_id() {
        for (tag, id) in [
            ("EA", "eda"),
            ("EL", "eda_level"),
            ("ER", "eda_response"),
            ("SA", "scr_amplitude"),
            ("SF", "scr_frequency"),
            ("SR", "scr_rise_time"),
            ("PI", "ppg_ir"),
            ("PR", "ppg_red"),
            ("PG", "ppg_green"),
            ("TH", "thermopile"),
            ("T0", "temp0"),
            ("H0", "humidity"),
            ("MX", "mag_x"),
            ("MY", "mag_y"),
            ("MZ", "mag_z"),
        ] {
            assert_eq!(sensor_id(tag), Some(id), "tag {tag}");
        }
    }

    /// Channels that have a dedicated route must not be recorded twice.
    #[test]
    fn channels_routed_elsewhere_have_no_sensor_id() {
        for tag in [
            "HR", "BI", "O2", "T1", "AX", "AY", "AZ", "GX", "GY", "GZ", "B%", "BV", "SD",
        ] {
            assert_eq!(sensor_id(tag), None, "tag {tag}");
        }
    }

    #[test]
    fn nonfinite_sensor_values_are_dropped() {
        let mut a = adapter();
        a.translate(reading("EA", &[f64::NAN, 0.5, f64::INFINITY]));
        let f = sensors(&mut a);
        assert_eq!(f[0].samples.iter().map(|s| s.1).collect::<Vec<_>>(), vec![0.5]);
        a.translate(reading("EA", &[f64::NAN]));
        assert!(drain(&mut a).is_empty(), "nothing left to record");
    }

    #[test]
    fn a_batch_is_spread_over_the_gap_since_the_channel_last_reported() {
        let t = spread_timestamps(100.0, Some(98.0), 4);
        assert_eq!(t, vec![98.5, 99.0, 99.5, 100.0]);
        // Oldest first, ending at `now`.
        assert!(t.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_batch_has_no_spread_without_a_usable_gap() {
        assert_eq!(spread_timestamps(100.0, None, 3), vec![100.0; 3], "first packet");
        assert_eq!(spread_timestamps(100.0, Some(99.0), 1), vec![100.0], "single sample");
        assert_eq!(
            spread_timestamps(100.0, Some(80.0), 3),
            vec![100.0; 3],
            "long silence is not this batch"
        );
        assert_eq!(
            spread_timestamps(100.0, Some(100.0), 3),
            vec![100.0; 3],
            "no time elapsed"
        );
    }

    #[test]
    fn sd_card_fill_is_not_forwarded() {
        let mut a = adapter();
        a.translate(reading("SD", &[41.0]));
        assert!(drain(&mut a).is_empty());
        assert_eq!(a.unrouted, 1);
    }

    #[test]
    fn disconnect_passes_through() {
        let mut a = adapter();
        a.translate(EmotiBitEvent::Disconnected);
        assert!(matches!(drain(&mut a).as_slice(), [DeviceEvent::Disconnected]));
    }
}
