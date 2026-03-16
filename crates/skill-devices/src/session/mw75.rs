// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for the Neurable MW75 Neuro headphones.
//!
//! MW75 delivers all 12 EEG channels in a single packet at 500 Hz.
//! The adapter translates `Mw75Event`s into [`DeviceEvent`]s directly.
//!
//! ## BLE → RFCOMM handoff
//!
//! The MW75 connection lifecycle involves BLE activation followed by an
//! RFCOMM data stream.  This multi-phase logic is handled by the connection
//! factory in `session_connect.rs` (Tauri side).  By the time the adapter
//! is constructed, the event channel is already connected to the active
//! transport (BLE or RFCOMM).

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::mpsc;

use mw75::prelude::*;
use skill_constants::{EEG_CHANNELS, MW75_CHANNEL_NAMES, MW75_EEG_CHANNELS, MW75_SAMPLE_RATE};

use super::{
    BatteryFrame, DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo,
    EegFrame, now_secs,
};

// ── Mw75Adapter ───────────────────────────────────────────────────────────────

pub struct Mw75Adapter {
    rx:      mpsc::Receiver<Mw75Event>,
    handle:  Arc<Mw75Handle>,
    desc:    DeviceDescriptor,
    pending: VecDeque<DeviceEvent>,

    /// RFCOMM guard — kept alive so the RFCOMM stream is not dropped.
    #[cfg(feature = "mw75-rfcomm")]
    _rfcomm: Option<mw75::rfcomm::RfcommHandle>,
}

impl Mw75Adapter {
    /// Create a new adapter from an active MW75 event channel and handle.
    ///
    /// If an initial [`DeviceInfo`] is provided (e.g. because the RFCOMM
    /// factory already knows the device name), a synthetic `Connected` event
    /// is queued so the session runner sees it.
    pub fn new(
        rx: mpsc::Receiver<Mw75Event>,
        handle: Arc<Mw75Handle>,
        initial_info: Option<DeviceInfo>,
    ) -> Self {
        let channel_names: Vec<String> =
            MW75_CHANNEL_NAMES.iter().map(|s| (*s).to_owned()).collect();

        let mut pending = VecDeque::new();
        if let Some(info) = initial_info {
            pending.push_back(DeviceEvent::Connected(info));
        }

        Self {
            rx,
            handle,
            desc: DeviceDescriptor {
                kind: "mw75",
                caps: DeviceCaps::EEG | DeviceCaps::BATTERY,
                eeg_channels: MW75_EEG_CHANNELS,
                eeg_sample_rate: MW75_SAMPLE_RATE,
                channel_names,
                pipeline_channels: MW75_EEG_CHANNELS.min(EEG_CHANNELS),
            },
            pending,
            #[cfg(feature = "mw75-rfcomm")]
            _rfcomm: None,
        }
    }

    /// Attach an RFCOMM handle to keep the stream alive for the adapter's
    /// lifetime.
    #[cfg(feature = "mw75-rfcomm")]
    pub fn set_rfcomm(&mut self, rfcomm: mw75::rfcomm::RfcommHandle) {
        self._rfcomm = Some(rfcomm);
    }

    fn translate(&mut self, ev: Mw75Event) {
        match ev {
            Mw75Event::Connected(name) => {
                self.pending.push_back(DeviceEvent::Connected(DeviceInfo {
                    name: name.clone(),
                    id: name,
                    ..Default::default()
                }));
            }

            Mw75Event::Disconnected => {
                self.pending.push_back(DeviceEvent::Disconnected);
            }

            Mw75Event::Eeg(pkt) => {
                let ts = if pkt.timestamp > 0.0 {
                    pkt.timestamp
                } else {
                    now_secs()
                };
                let channels: Vec<f64> = pkt.channels.iter().map(|&v| v as f64).collect();
                self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                    channels,
                    timestamp_s: ts,
                }));
            }

            Mw75Event::Battery(bat) => {
                self.pending.push_back(DeviceEvent::Battery(BatteryFrame {
                    level_pct: bat.level as f32,
                    voltage_mv: None,
                    temperature_raw: None,
                }));
            }

            Mw75Event::Activated(_) | Mw75Event::RawData(_) | Mw75Event::OtherEvent { .. } => {
                // Not forwarded — activation is handled by the connect factory;
                // raw data and other events are for debugging only.
            }
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for Mw75Adapter {
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
        #[cfg(feature = "mw75-rfcomm")]
        if let Some(ref r) = self._rfcomm {
            r.shutdown();
        }
        let _ = self.handle.disconnect().await;
    }
}
