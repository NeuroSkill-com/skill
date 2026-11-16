// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Companion devices: a second (third, ...) device recorded in parallel with
//! the session's own, into the same recording and on the same clock — a
//! wristband (EmotiBit: electrodermal activity, PPG, temperature, motion) worn
//! beside an EEG cap, say.
//!
//! The session has one device, and that device decides when the session
//! starts and ends. A companion only adds data to it:
//!
//! * [`DeviceEvent::Vitals`], [`DeviceEvent::Sensor`] and
//!   [`DeviceEvent::Marker`] pass through as they are, to the `_vitals`,
//!   `_sensors` and `_markers` sidecars.
//! * Motion and battery become sensor channels (`accel_x` ... `battery_pct`),
//!   because the session's motion file and battery level are its own device's.
//! * EEG, PPG and fNIRS frames of a companion are dropped: those pipelines are
//!   the session device's (counted, and logged once).
//! * A companion that is not there, or goes away, does not end or delay the
//!   session: it is looked for again every few seconds, and each time it comes
//!   or goes a marker (`"<kind> connected"` / `"<kind> disconnected"`, source
//!   `companion`) is recorded, so a hole in its data is explained in the
//!   recording itself.
//!
//! Configured in `UserSettings::companion_devices`: connect targets, as for a
//! session (`emotibit`, `emotibit:192.168.1.40`, `EmotiBit-BLE-E848`).

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use skill_devices::session::{
    now_secs, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, MarkerFrame, SensorFrame,
};
use tokio::sync::mpsc;
use tracing::{info, warn};

/// How long to wait before a companion that is missing, or was lost, is looked
/// for again.
#[cfg(not(test))]
const RETRY_EVERY: Duration = Duration::from_secs(5);
#[cfg(test)]
const RETRY_EVERY: Duration = Duration::from_millis(50);

/// Events of all companions waiting for the session: seconds of an EmotiBit.
/// When the session falls further behind than this, the companions wait — their
/// own queues then decide what is lost, not this one.
const QUEUE: usize = 4096;

/// The marker source of a companion's coming and going.
pub const MARKER_SOURCE: &str = "companion";

/// A connection attempt to one companion.
pub type Connecting = Pin<Box<dyn Future<Output = anyhow::Result<Box<dyn DeviceAdapter>>> + Send>>;

/// How to connect to one companion; called again after every loss.
pub type Connector = Box<dyn Fn() -> Connecting + Send + Sync>;

/// The session's device together with its companions; see the module docs.
pub struct WithCompanions {
    primary: Box<dyn DeviceAdapter>,
    rx: mpsc::Receiver<DeviceEvent>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    descriptor: DeviceDescriptor,
}

impl WithCompanions {
    /// `primary` with one companion per `(label, connector)`; the label names
    /// the companion in markers and logs (`"emotibit"`).
    pub fn new(primary: Box<dyn DeviceAdapter>, companions: Vec<(String, Connector)>) -> Self {
        let (tx, rx) = mpsc::channel(QUEUE);
        let tasks = companions
            .into_iter()
            .map(|(label, connect)| tokio::spawn(follow(label, connect, tx.clone())))
            .collect();
        let descriptor = merged(primary.descriptor());
        Self {
            primary,
            rx,
            tasks,
            descriptor,
        }
    }
}

/// The session device's descriptor, plus what companions bring.
fn merged(primary: &DeviceDescriptor) -> DeviceDescriptor {
    let mut d = primary.clone();
    d.caps |= DeviceCaps::VITALS;
    d
}

#[async_trait::async_trait]
impl DeviceAdapter for WithCompanions {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.descriptor
    }

    async fn next_event(&mut self) -> Option<DeviceEvent> {
        tokio::select! {
            ev = self.primary.next_event() => {
                if matches!(ev, Some(DeviceEvent::Connected(_))) {
                    // some devices only know what they are once connected
                    self.descriptor = merged(self.primary.descriptor());
                }
                // `None`: the session's device is gone, and the session with it
                ev
            }
            Some(ev) = self.rx.recv() => Some(ev),
        }
    }

    async fn disconnect(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
        self.primary.disconnect().await;
    }

    fn eeg_scale_uv(&self) -> Option<f64> {
        self.primary.eeg_scale_uv()
    }
}

impl Drop for WithCompanions {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

fn marker(label: &str, what: &str) -> DeviceEvent {
    DeviceEvent::Marker(MarkerFrame {
        timestamp_s: now_secs(),
        text: format!("{label} {what}"),
        source: MARKER_SOURCE.into(),
    })
}

/// Connect to one companion and forward what it sends, again and again, until
/// the session is gone.
async fn follow(label: String, connect: Connector, tx: mpsc::Sender<DeviceEvent>) {
    let mut told = false;
    let mut dropped = 0u64;
    loop {
        match connect().await {
            Ok(mut adapter) => {
                info!(companion = %label, "companion connected");
                told = false;
                if tx.send(marker(&label, "connected")).await.is_err() {
                    adapter.disconnect().await;
                    return;
                }
                loop {
                    let ev = tokio::select! {
                        ev = adapter.next_event() => ev,
                        () = tx.closed() => {
                            adapter.disconnect().await;
                            return;
                        }
                    };
                    let Some(ev) = ev else { break };
                    if matches!(ev, DeviceEvent::Disconnected) {
                        break;
                    }
                    for out in translate(ev, &mut dropped, &label) {
                        if tx.send(out).await.is_err() {
                            adapter.disconnect().await;
                            return;
                        }
                    }
                }
                adapter.disconnect().await;
                warn!(companion = %label, "companion lost; looking for it again");
                if tx.send(marker(&label, "disconnected")).await.is_err() {
                    return;
                }
            }
            Err(e) => {
                // once per absence: it is asked for every few seconds
                if !told {
                    warn!(companion = %label, %e, "companion not found; the session goes on without it and keeps looking");
                    told = true;
                }
            }
        }
        tokio::select! {
            () = tokio::time::sleep(RETRY_EVERY) => {}
            () = tx.closed() => return,
        }
    }
}

/// What a companion's event becomes in the session; see the module docs.
fn translate(ev: DeviceEvent, dropped: &mut u64, label: &str) -> Vec<DeviceEvent> {
    let sensor = |channel: &str, unit: &str, ts: f64, value: f64| {
        DeviceEvent::Sensor(SensorFrame {
            channel: channel.into(),
            unit: unit.into(),
            samples: vec![(ts, value)],
        })
    };
    match ev {
        DeviceEvent::Vitals(_) | DeviceEvent::Sensor(_) | DeviceEvent::Marker(_) => vec![ev],
        DeviceEvent::Imu(f) => {
            let ts = f.timestamp_s.unwrap_or_else(now_secs);
            let mut out = Vec::with_capacity(9);
            let axes = ["x", "y", "z"];
            for (i, axis) in axes.iter().enumerate() {
                out.push(sensor(&format!("accel_{axis}"), "g", ts, f64::from(f.accel[i])));
            }
            if let Some(gyro) = f.gyro {
                for (i, axis) in axes.iter().enumerate() {
                    out.push(sensor(&format!("gyro_{axis}"), "°/s", ts, f64::from(gyro[i])));
                }
            }
            if let Some(mag) = f.mag {
                for (i, axis) in axes.iter().enumerate() {
                    out.push(sensor(&format!("mag_{axis}"), "G", ts, f64::from(mag[i])));
                }
            }
            out
        }
        DeviceEvent::Battery(f) => vec![sensor("battery_pct", "%", now_secs(), f64::from(f.level_pct))],
        DeviceEvent::Eeg(_) | DeviceEvent::Ppg(_) | DeviceEvent::Fnirs(_) => {
            if *dropped == 0 {
                warn!(companion = %label, "a companion's EEG / PPG / fNIRS frames are not recorded: those belong to the session's own device");
            }
            *dropped += 1;
            Vec::new()
        }
        DeviceEvent::Connected(_) | DeviceEvent::Disconnected | DeviceEvent::Meta(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_devices::session::{BatteryFrame, DeviceInfo, EegFrame, ImuFrame, VitalsFrame};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Sends what it was given, one event per `pace`, then ends (or stays).
    struct Scripted {
        desc: DeviceDescriptor,
        events: VecDeque<DeviceEvent>,
        pace: Duration,
        stay: bool,
    }

    fn descriptor(kind: &'static str, caps: DeviceCaps) -> DeviceDescriptor {
        DeviceDescriptor {
            kind,
            caps,
            eeg_channels: 4,
            eeg_sample_rate: 256.0,
            channel_names: (0..4).map(|i| format!("Ch{i}")).collect(),
            pipeline_channels: 4,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        }
    }

    #[async_trait::async_trait]
    impl DeviceAdapter for Scripted {
        fn descriptor(&self) -> &DeviceDescriptor {
            &self.desc
        }
        async fn next_event(&mut self) -> Option<DeviceEvent> {
            tokio::time::sleep(self.pace).await;
            match self.events.pop_front() {
                Some(ev) => Some(ev),
                None if self.stay => std::future::pending().await,
                None => None,
            }
        }
        async fn disconnect(&mut self) {}
    }

    fn eeg(k: usize) -> DeviceEvent {
        DeviceEvent::Eeg(EegFrame {
            channels: vec![k as f64; 4],
            timestamp_s: k as f64,
        })
    }

    fn vitals(hr: f64) -> DeviceEvent {
        DeviceEvent::Vitals(VitalsFrame {
            timestamp_s: now_secs(),
            hr_bpm: Some(hr),
            ibi_ms: None,
            spo2_pct: None,
            temperature_c: None,
        })
    }

    fn wristband(events: Vec<DeviceEvent>, stay: bool) -> Box<dyn DeviceAdapter> {
        let mut all = VecDeque::from([DeviceEvent::Connected(DeviceInfo::default())]);
        all.extend(events);
        Box::new(Scripted {
            desc: descriptor("emotibit", DeviceCaps::VITALS | DeviceCaps::IMU),
            events: all,
            pace: Duration::from_millis(2),
            stay,
        })
    }

    async fn drain(adapter: &mut WithCompanions) -> Vec<DeviceEvent> {
        let mut all = Vec::new();
        while let Some(ev) = adapter.next_event().await {
            all.push(ev);
        }
        all
    }

    fn sensors(events: &[DeviceEvent]) -> Vec<(String, f64)> {
        events
            .iter()
            .filter_map(|e| match e {
                DeviceEvent::Sensor(f) => Some((f.channel.clone(), f.samples[0].1)),
                _ => None,
            })
            .collect()
    }

    fn markers(events: &[DeviceEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                DeviceEvent::Marker(m) if m.source == MARKER_SOURCE => Some(m.text.clone()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_companion_adds_its_data_and_nothing_of_the_session_is_lost() {
        let primary = Box::new(Scripted {
            desc: descriptor("lsl", DeviceCaps::EEG),
            events: (0..200).map(eeg).collect(),
            pace: Duration::from_millis(1),
            stay: false,
        });
        let connect: Connector = Box::new(|| {
            Box::pin(async {
                Ok(wristband(
                    vec![
                        vitals(61.0),
                        DeviceEvent::Sensor(SensorFrame {
                            channel: "eda".into(),
                            unit: "µS".into(),
                            samples: vec![(1.0, 0.5), (2.0, 0.6)],
                        }),
                        DeviceEvent::Imu(ImuFrame {
                            timestamp_s: Some(7.0),
                            accel: [0.0, 0.0, 1.0],
                            gyro: Some([1.0, 2.0, 3.0]),
                            mag: None,
                        }),
                        DeviceEvent::Battery(BatteryFrame {
                            level_pct: 87.0,
                            voltage_mv: None,
                            temperature_raw: None,
                        }),
                        eeg(999),
                    ],
                    true,
                ))
            })
        });
        let mut both = WithCompanions::new(primary, vec![("emotibit".into(), connect)]);
        assert_eq!(both.descriptor().kind, "lsl", "the session is its own device's");
        assert!(both.descriptor().caps.contains(DeviceCaps::EEG | DeviceCaps::VITALS));

        let all = drain(&mut both).await;
        // every sample of the session's device, in order, and none of the companion's EEG
        let eeg_stamps: Vec<f64> = all
            .iter()
            .filter_map(|e| match e {
                DeviceEvent::Eeg(f) => Some(f.timestamp_s),
                _ => None,
            })
            .collect();
        assert_eq!(eeg_stamps, (0..200).map(|k| k as f64).collect::<Vec<_>>());
        assert_eq!(all.iter().filter(|e| matches!(e, DeviceEvent::Vitals(_))).count(), 1);
        assert_eq!(
            sensors(&all),
            [
                ("eda", 0.5),
                ("accel_x", 0.0),
                ("accel_y", 0.0),
                ("accel_z", 1.0),
                ("gyro_x", 1.0),
                ("gyro_y", 2.0),
                ("gyro_z", 3.0),
                ("battery_pct", 87.0)
            ]
            .map(|(c, v)| (c.to_string(), v))
        );
        // its motion and battery are not the session device's
        assert!(!all
            .iter()
            .any(|e| matches!(e, DeviceEvent::Imu(_) | DeviceEvent::Battery(_))));
        // nor does its connection start or end anything
        assert!(!all
            .iter()
            .any(|e| matches!(e, DeviceEvent::Connected(_) | DeviceEvent::Disconnected)));
        assert_eq!(markers(&all), ["emotibit connected"]);
    }

    #[tokio::test]
    async fn a_missing_companion_is_looked_for_and_a_lost_one_comes_back() {
        let primary = Box::new(Scripted {
            desc: descriptor("lsl", DeviceCaps::EEG),
            events: (0..400).map(eeg).collect(),
            pace: Duration::from_millis(2),
            stay: false,
        });
        // not there twice, then there for two readings, then gone, then there to stay
        let attempts = Arc::new(AtomicUsize::new(0));
        let seen = attempts.clone();
        let connect: Connector = Box::new(move || {
            let n = seen.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                match n {
                    0 | 1 => anyhow::bail!("no EmotiBit found"),
                    2 => Ok(wristband(vec![vitals(60.0), vitals(61.0)], false)),
                    _ => Ok(wristband(vec![vitals(62.0)], true)),
                }
            })
        });
        let mut both = WithCompanions::new(primary, vec![("emotibit".into(), connect)]);
        let all = drain(&mut both).await;

        assert_eq!(
            all.iter().filter(|e| matches!(e, DeviceEvent::Eeg(_))).count(),
            400,
            "the session ran to its own end"
        );
        assert_eq!(
            markers(&all),
            ["emotibit connected", "emotibit disconnected", "emotibit connected"]
        );
        let rates: Vec<f64> = all
            .iter()
            .filter_map(|e| match e {
                DeviceEvent::Vitals(v) => v.hr_bpm,
                _ => None,
            })
            .collect();
        assert_eq!(rates, [60.0, 61.0, 62.0]);
        assert_eq!(attempts.load(Ordering::SeqCst), 4);
    }
}
