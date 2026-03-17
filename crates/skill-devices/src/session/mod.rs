// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Unified device session abstraction.
//!
//! This module defines a device-agnostic event vocabulary and the
//! [`DeviceAdapter`] trait that each hardware driver implements.  The Tauri
//! session runner consumes a `Box<dyn DeviceAdapter>` and drives the shared
//! DSP / CSV / DND / emit pipeline without knowing which headset is connected.
//!
//! ## Capability model
//!
//! Instead of compile-time feature flags, each adapter declares its
//! [`DeviceCaps`] at construction time.  The session runner inspects caps to
//! decide which event types to expect, which CSV columns to create, and
//! whether PPG / IMU visualisation should be enabled.
//!
//! ## Adapter implementations
//!
//! * [`muse::MuseAdapter`]   — Muse 2 / Muse S (4 ch @ 256 Hz, PPG, IMU)
//! * [`mw75::Mw75Adapter`]   — Neurable MW75 Neuro (12 ch @ 500 Hz)
//! * [`hermes::HermesAdapter`] — Hermes V1 (8 ch @ 250 Hz, IMU)
//! * [`openbci::OpenBciAdapter`] — Ganglion / Cyton / Galea (4–24 ch)

pub mod muse;
pub mod mw75;
pub mod hermes;
pub mod openbci;
pub mod emotiv;
pub mod idun;

#[cfg(test)]
mod tests;

use std::time::{SystemTime, UNIX_EPOCH};

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

// ── Capability flags ──────────────────────────────────────────────────────────

bitflags! {
    /// Data streams a device can produce.
    ///
    /// Declared by each adapter at construction time; inspected by the
    /// generic session runner to decide which processing paths to enable.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct DeviceCaps: u32 {
        /// Multi-channel EEG voltage data.
        const EEG     = 0b0000_0001;
        /// Optical PPG (photoplethysmography) data.
        const PPG     = 0b0000_0010;
        /// Inertial measurement unit (accelerometer, gyroscope, magnetometer).
        const IMU     = 0b0000_0100;
        /// Battery level / telemetry.
        const BATTERY = 0b0000_1000;
        /// Device-specific metadata (e.g. Muse Control JSON responses).
        const META    = 0b0001_0000;
    }
}

// ── Device descriptor ─────────────────────────────────────────────────────────

/// Static properties of a connected device.
///
/// Built by the adapter at construction time and returned by
/// [`DeviceAdapter::descriptor`].
#[derive(Debug, Clone)]
pub struct DeviceDescriptor {
    /// Short device-kind tag used in logs and status (`"muse"`, `"mw75"`, …).
    pub kind: &'static str,
    /// Capabilities this device supports.
    pub caps: DeviceCaps,
    /// Total EEG channel count on the hardware.
    pub eeg_channels: usize,
    /// Hardware EEG sample rate in Hz.
    pub eeg_sample_rate: f64,
    /// Human-readable channel labels (length == `eeg_channels`).
    pub channel_names: Vec<String>,
    /// Number of channels routed through the DSP pipeline
    /// (`min(eeg_channels, EEG_CHANNELS)`).
    pub pipeline_channels: usize,
}

// ── Unified event types ───────────────────────────────────────────────────────

/// Information about a newly connected device.
#[derive(Debug, Clone, Default)]
pub struct DeviceInfo {
    pub name: String,
    pub id:   String,
    pub serial_number:      Option<String>,
    pub firmware_version:   Option<String>,
    pub hardware_version:   Option<String>,
    pub bootloader_version: Option<String>,
    pub mac_address:        Option<String>,
    pub headset_preset:     Option<String>,
}

/// A normalised multi-channel EEG data frame.
///
/// All adapters emit frames where `channels.len() == descriptor.eeg_channels`.
/// Muse's per-electrode delivery is accumulated inside [`muse::MuseAdapter`]
/// and emitted as aligned multi-channel frames.
#[derive(Debug, Clone)]
pub struct EegFrame {
    /// Channel values in µV.  Length equals the device's channel count.
    pub channels: Vec<f64>,
    /// Timestamp in seconds since Unix epoch.
    pub timestamp_s: f64,
}

/// A normalised PPG (optical) data frame.
#[derive(Debug, Clone)]
pub struct PpgFrame {
    /// Optical channel index: 0 = ambient, 1 = infrared, 2 = red.
    pub channel: usize,
    /// Raw sample values (typically 6 per notification at 64 Hz).
    pub samples: Vec<f64>,
    /// Timestamp in seconds since Unix epoch for the first sample.
    pub timestamp_s: f64,
}

/// A normalised inertial measurement.
#[derive(Debug, Clone)]
pub struct ImuFrame {
    /// Accelerometer reading in g (X, Y, Z).
    pub accel: [f32; 3],
    /// Gyroscope reading in °/s (X, Y, Z), if available.
    pub gyro: Option<[f32; 3]>,
    /// Magnetometer reading in gauss (X, Y, Z), if available.
    pub mag: Option<[f32; 3]>,
}

/// A battery / telemetry update.
#[derive(Debug, Clone)]
pub struct BatteryFrame {
    /// State-of-charge in percent (0–100).
    pub level_pct: f32,
    /// Fuel-gauge terminal voltage in mV (Muse Classic only).
    pub voltage_mv: Option<f32>,
    /// Raw temperature ADC value (Muse Classic only).
    pub temperature_raw: Option<u16>,
}

/// The unified event enum.
///
/// Every [`DeviceAdapter`] translates its vendor-specific events into this
/// vocabulary.  The session runner processes these without knowing which
/// hardware is connected.
#[derive(Debug, Clone)]
pub enum DeviceEvent {
    /// BLE / transport link established.
    Connected(DeviceInfo),
    /// BLE / transport link lost.
    Disconnected,
    /// One aligned multi-channel EEG sample.
    Eeg(EegFrame),
    /// One PPG optical packet.
    Ppg(PpgFrame),
    /// One inertial measurement.
    Imu(ImuFrame),
    /// Battery / telemetry update.
    Battery(BatteryFrame),
    /// Device-specific opaque metadata (e.g. Muse Control JSON).
    Meta(serde_json::Value),
}

// ── DeviceAdapter trait ───────────────────────────────────────────────────────

/// Trait that each device driver implements to plug into the generic session
/// runner.
///
/// An adapter owns its vendor event channel and connection handle.  It
/// translates vendor events into [`DeviceEvent`]s on each call to
/// [`next_event`](DeviceAdapter::next_event).
///
/// The trait is object-safe so the session runner can work with
/// `Box<dyn DeviceAdapter>`.
#[async_trait::async_trait]
pub trait DeviceAdapter: Send {
    /// Static descriptor for this device (caps, channel count, sample rate, …).
    fn descriptor(&self) -> &DeviceDescriptor;

    /// Receive the next event, translating from the vendor format.
    ///
    /// Returns `None` when the event stream is exhausted (channel closed or
    /// device disconnected).
    async fn next_event(&mut self) -> Option<DeviceEvent>;

    /// Cleanly disconnect the device.
    ///
    /// Called by the session runner when the user cancels or after the event
    /// loop exits.  Implementations should be idempotent.
    async fn disconnect(&mut self);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Current UNIX timestamp in seconds with sub-second precision.
pub fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
