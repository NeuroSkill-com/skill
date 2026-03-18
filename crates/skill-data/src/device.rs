// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! EEG device abstraction — capability flags for connected hardware.
//!
//! All per-device decisions in the Rust backend should derive from
//! [`DeviceKind`] rather than matching on raw device-name strings.
//!
//! ## Adding a new device
//! 1. Add a variant to [`DeviceKind`].
//! 2. Fill in [`DeviceCapabilities`] via [`DeviceKind::capabilities`].
//! 3. Add a detection clause in [`DeviceKind::from_name`].
//! 4. Mirror the change in `src/lib/device.ts`.

use serde::{Deserialize, Serialize};

// ── Paired (persisted) device ─────────────────────────────────────────────────

/// A BLE device that has been paired and persisted to `settings.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PairedDevice {
    pub id:        String,
    pub name:      String,
    pub last_seen: u64,
}

// ── Device family ─────────────────────────────────────────────────────────────

/// Known EEG device families.
///
/// `Unknown` is used for unrecognised names or while disconnected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    /// Muse 1 / 2 / S / Monitor — 4-channel frontal + temporal (TP9/AF7/AF8/TP10).
    Muse,
    /// OpenBCI Ganglion — 4-channel BLE.
    Ganglion,
    /// OpenBCI Cyton (8-ch) / Cyton+Daisy (16-ch) — serial or WiFi.
    OpenBci,
    /// Neurable MW75 Neuro — 12-channel over-ear headphones (500 Hz).
    Mw75,
    /// RE-AK Nucleus Hermes — 8-channel (250 Hz).
    Hermes,
    /// Emotiv EPOC / Insight / Flex / MN8 — 14/5/32-channel via Cortex WebSocket API.
    Emotiv,
    /// IDUN Guardian — single-channel bipolar in-ear EEG earbud (1 ch @ 250 Hz, IMU).
    Idun,
    /// Unrecognised or not yet connected.
    Unknown,
}

// ── Capability flags ──────────────────────────────────────────────────────────

/// Static capability description for a device family.
#[derive(Debug, Clone)]
pub struct DeviceCapabilities {
    pub kind: DeviceKind,

    /// Nominal EEG channel count.
    pub channel_count: usize,

    /// Device has a PPG (photoplethysmography) sensor.
    pub has_ppg: bool,

    /// Device has an IMU (accelerometer + gyroscope).
    pub has_imu: bool,

    /// Device has electrodes at central scalp sites (C3, C4, Cz or equivalent).
    ///
    /// When `false`, metrics that require central placement — such as
    /// **mu-rhythm suppression** — are not meaningful and should be hidden.
    pub has_central_electrodes: bool,

    /// Whether the device supports a full 10-20 montage (or superset).
    pub has_full_montage: bool,

    /// Nominal sample rate (Hz).
    pub sample_rate_hz: f32,
}

// ── Capability tables ─────────────────────────────────────────────────────────

impl DeviceKind {
    /// Derive the device family from the BLE / USB advertising name.
    ///
    /// Matching is case-insensitive.  Returns [`DeviceKind::Unknown`] for
    /// `None` (not connected) or an unrecognised name.
    pub fn from_name(name: Option<&str>) -> Self {
        let Some(n) = name else { return Self::Unknown };
        let n = n.to_lowercase();

        if n.starts_with("muse")                                        { return Self::Muse;     }
        if n.starts_with("ganglion") || n.starts_with("simblee")       { return Self::Ganglion; }
        if n.starts_with("openbci") || n.starts_with("cyton")          { return Self::OpenBci;  }
        if n.contains("mw75")                                           { return Self::Mw75;     }
        if n.starts_with("hermes")                                      { return Self::Hermes;   }
        if n.starts_with("emotiv") || n.starts_with("epoc")
            || n.starts_with("insight") || n.starts_with("flex")
            || n.starts_with("mn8")                                     { return Self::Emotiv;   }
        if n.starts_with("idun") || n.starts_with("ige")
            || n.starts_with("guardian")                                { return Self::Idun;     }

        Self::Unknown
    }

    /// Return the static [`DeviceCapabilities`] for this device family.
    pub fn capabilities(self) -> DeviceCapabilities {
        match self {
            Self::Muse => DeviceCapabilities {
                kind:                   Self::Muse,
                channel_count:          4,
                has_ppg:                true,
                has_imu:                true,
                has_central_electrodes: false, // TP9 / AF7 / AF8 / TP10 only
                has_full_montage:       false,
                sample_rate_hz:         256.0,
            },
            Self::Ganglion => DeviceCapabilities {
                kind:                   Self::Ganglion,
                channel_count:          4,
                has_ppg:                false,
                has_imu:                false,
                has_central_electrodes: true,  // user-configurable 10-20
                has_full_montage:       false,
                sample_rate_hz:         200.0,
            },
            Self::OpenBci => DeviceCapabilities {
                kind:                   Self::OpenBci,
                channel_count:          8, // Cyton; Cyton+Daisy = 16
                has_ppg:                false,
                has_imu:                false,
                has_central_electrodes: true, // standard 10-20 includes C3/C4/Cz
                has_full_montage:       true,
                sample_rate_hz:         250.0,
            },
            Self::Mw75 => DeviceCapabilities {
                kind:                   Self::Mw75,
                channel_count:          12, // 6 per ear cup
                has_ppg:                false,
                has_imu:                false,
                has_central_electrodes: false, // temporal sites only (FT7/T7/TP7/CP5/P7/C5 + R)
                has_full_montage:       false,
                sample_rate_hz:         500.0,
            },
            Self::Hermes => DeviceCapabilities {
                kind:                   Self::Hermes,
                channel_count:          8,
                has_ppg:                false,
                has_imu:                true,
                has_central_electrodes: true,  // montage-dependent
                has_full_montage:       false,
                sample_rate_hz:         250.0,
            },
            Self::Emotiv => DeviceCapabilities {
                kind:                   Self::Emotiv,
                channel_count:          14, // EPOC; Insight = 5; Flex = 32
                has_ppg:                false,
                has_imu:                true,
                has_central_electrodes: true, // FC5/FC6 near-central
                has_full_montage:       false,
                sample_rate_hz:         128.0,
            },
            Self::Idun => DeviceCapabilities {
                kind:                   Self::Idun,
                channel_count:          1,   // single bipolar channel
                has_ppg:                false,
                has_imu:                true, // 6-DOF IMU (accel + gyro)
                has_central_electrodes: false, // in-ear canal placement
                has_full_montage:       false,
                sample_rate_hz:         250.0,
            },
            Self::Unknown => DeviceCapabilities {
                kind:                   Self::Unknown,
                channel_count:          0,
                has_ppg:                false,
                has_imu:                false,
                has_central_electrodes: false,
                has_full_montage:       false,
                sample_rate_hz:         0.0,
            },
        }
    }

    /// Convenience: `true` when this is any Muse variant.
    #[inline]
    pub fn is_muse(self) -> bool { self == Self::Muse }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_name_muse() {
        assert_eq!(DeviceKind::from_name(Some("Muse-2-ABCD")), DeviceKind::Muse);
        assert_eq!(DeviceKind::from_name(Some("MuseS-1234")), DeviceKind::Muse);
    }

    #[test]
    fn from_name_ganglion() {
        assert_eq!(DeviceKind::from_name(Some("Ganglion-1234")), DeviceKind::Ganglion);
        assert_eq!(DeviceKind::from_name(Some("Simblee-001")), DeviceKind::Ganglion);
    }

    #[test]
    fn from_name_openbci() {
        assert_eq!(DeviceKind::from_name(Some("OpenBCI-Cyton")), DeviceKind::OpenBci);
        assert_eq!(DeviceKind::from_name(Some("Cyton-ABCD")), DeviceKind::OpenBci);
    }

    #[test]
    fn from_name_mw75() {
        assert_eq!(DeviceKind::from_name(Some("Headphones-MW75-v2")), DeviceKind::Mw75);
    }

    #[test]
    fn from_name_hermes() {
        assert_eq!(DeviceKind::from_name(Some("Hermes-ABC")), DeviceKind::Hermes);
    }

    #[test]
    fn from_name_emotiv() {
        assert_eq!(DeviceKind::from_name(Some("Emotiv-EPOC-X")), DeviceKind::Emotiv);
        assert_eq!(DeviceKind::from_name(Some("EPOC-X-1234")), DeviceKind::Emotiv);
        assert_eq!(DeviceKind::from_name(Some("Insight-5ch")), DeviceKind::Emotiv);
        assert_eq!(DeviceKind::from_name(Some("FLEX-Saline")), DeviceKind::Emotiv);
        assert_eq!(DeviceKind::from_name(Some("MN8-Earbuds")), DeviceKind::Emotiv);
    }

    #[test]
    fn from_name_idun() {
        assert_eq!(DeviceKind::from_name(Some("IDUN-Guardian")), DeviceKind::Idun);
        assert_eq!(DeviceKind::from_name(Some("Guardian-001")), DeviceKind::Idun);
        assert_eq!(DeviceKind::from_name(Some("IGE-1234")), DeviceKind::Idun);
    }

    #[test]
    fn from_name_unknown() {
        assert_eq!(DeviceKind::from_name(None), DeviceKind::Unknown);
        assert_eq!(DeviceKind::from_name(Some("random-device")), DeviceKind::Unknown);
    }
}
