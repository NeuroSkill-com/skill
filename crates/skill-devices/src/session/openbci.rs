// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! [`DeviceAdapter`] for OpenBCI boards (Ganglion, Cyton, Cyton+Daisy, Galea).
//!
//! OpenBCI boards use a blocking `std::sync::mpsc` streaming API.  This
//! adapter bridges the blocking [`openbci::sample::StreamHandle`] into async
//! via a `tokio::sync::mpsc` channel fed by a `spawn_blocking` task.

use std::collections::VecDeque;

use openbci::sample::{Sample, StreamHandle as OpenbciStreamHandle};

use super::{
    DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo,
    EegFrame, ImuFrame,
};

// ── OpenBciAdapter ────────────────────────────────────────────────────────────

pub struct OpenBciAdapter {
    sample_rx: tokio::sync::mpsc::Receiver<Sample>,
    desc:      DeviceDescriptor,
    pending:   VecDeque<DeviceEvent>,
    /// Whether a synthetic `Connected` event has been emitted yet.
    connected_emitted: bool,
    /// Device info to emit on the first call to `next_event`.
    device_info: Option<DeviceInfo>,
}

impl OpenBciAdapter {
    /// Create a new adapter by bridging a blocking OpenBCI stream into async.
    ///
    /// `stream` is consumed by a `spawn_blocking` task that forwards samples
    /// into an async channel.  The adapter takes ownership of the stream.
    ///
    /// `info` will be emitted as a synthetic `Connected` event on the first
    /// call to [`next_event`](DeviceAdapter::next_event).
    pub fn start(
        stream: OpenbciStreamHandle,
        desc: DeviceDescriptor,
        info: DeviceInfo,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel::<Sample>(256);

        // Bridge: blocking recv → async channel.
        tokio::task::spawn_blocking(move || {
            while let Some(s) = stream.recv() {
                if tx.blocking_send(s).is_err() {
                    break;
                }
            }
        });

        Self {
            sample_rx: rx,
            desc,
            pending: VecDeque::new(),
            connected_emitted: false,
            device_info: Some(info),
        }
    }

    /// Build a [`DeviceDescriptor`] for any OpenBCI board variant.
    pub fn make_descriptor(
        kind: &'static str,
        eeg_channels: usize,
        eeg_sample_rate: f64,
        channel_names: Vec<String>,
    ) -> DeviceDescriptor {
        // All OpenBCI boards have EEG; some have accelerometers.
        let caps = DeviceCaps::EEG | DeviceCaps::IMU;
        DeviceDescriptor {
            kind,
            caps,
            eeg_channels,
            eeg_sample_rate,
            channel_names,
            pipeline_channels: eeg_channels.min(skill_constants::EEG_CHANNELS),
        }
    }
}

#[async_trait::async_trait]
impl DeviceAdapter for OpenBciAdapter {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }

    async fn next_event(&mut self) -> Option<DeviceEvent> {
        // Emit synthetic Connected on first call.
        if !self.connected_emitted {
            self.connected_emitted = true;
            if let Some(info) = self.device_info.take() {
                return Some(DeviceEvent::Connected(info));
            }
        }

        loop {
            if let Some(ev) = self.pending.pop_front() {
                return Some(ev);
            }

            let sample = self.sample_rx.recv().await?;

            // EEG frame.
            self.pending.push_back(DeviceEvent::Eeg(EegFrame {
                channels: sample.eeg,
                timestamp_s: sample.timestamp,
            }));

            // Accelerometer (Ganglion + Cyton with end_byte 0xC0).
            if let Some(accel) = sample.accel {
                self.pending.push_back(DeviceEvent::Imu(ImuFrame {
                    accel: [accel[0] as f32, accel[1] as f32, accel[2] as f32],
                    gyro: None,
                    mag: None,
                }));
            }
        }
    }

    async fn disconnect(&mut self) {
        // Dropping `sample_rx` causes the bridge task to exit.
        // The caller is responsible for calling board.stop_stream() / board.release().
        self.sample_rx.close();
    }
}
