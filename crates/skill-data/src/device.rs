// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! EEG device abstraction — capability flags for connected hardware.
//!
//! **This module is the single source of truth** for device families,
//! capabilities, and the supported-devices catalog.  The Svelte frontend
//! receives this data via Tauri commands — it does **not** keep its own copy.
//!
//! ## Adding a new device
//! 1. Add a variant to [`DeviceKind`].
//! 2. Fill in [`DeviceCapabilities`] via [`DeviceKind::capabilities`].
//! 3. Add a detection clause in [`DeviceKind::from_name`].
//! 4. Add entries to `SUPPORTED_COMPANIES` if the device should appear in
//!    the "Supported Devices" UI.

use serde::{Deserialize, Serialize};

// ── Paired (persisted) device ─────────────────────────────────────────────────

/// A BLE device that has been paired and persisted to `settings.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PairedDevice {
    pub id: String,
    pub name: String,
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
    /// AWEAR EEG wearable — single-channel BLE EEG (1 ch @ 256 Hz).
    Awear,
    /// Mendi fNIRS headband — optical channels + IMU + battery telemetry (BLE).
    Mendi,
    /// Cognionics / CGX EEG headsets — Quick-20/20r/32r/8r, AIM-2, Patch, Dev Kit.
    /// Connects over USB serial (FTDI dongle) at up to 500 Hz.
    Cognionics,
    /// AttentivU EEG glasses — 4-channel ExG (250 Hz) + 9-axis IMU (BLE).
    /// Broadcasts as "AttentivU-XXXX" or "AtU-XXXX" / "AtUXXXX".
    #[serde(rename = "attentivu")]
    AttentivU,
    /// BrainBit EEG headbands — Original, 2, Pro, Flex 4/8 (4ch @ 250 Hz, BLE via NeuroSDK2).
    BrainBit,
    /// g.tec Unicorn Hybrid Black — 8-channel EEG headset (250 Hz, BLE via Unicorn API).
    #[serde(rename = "gtec")]
    GTec,
    /// NeuroField Q21 — 20-channel research-grade EEG amplifier (256 Hz, PCAN-USB CAN bus).
    NeuroField,
    /// BrainMaster Atlantis/Discovery/Freedom — 2/4/24-channel neurofeedback amplifiers (USB serial).
    BrainMaster,
    /// NeuroSky MindWave / MindWave Mobile — single-channel ThinkGear (serial).
    NeuroSky,
    /// Neurosity Crown / Notion — cloud-streamed 8-channel EEG.
    Neurosity,
    /// Brain Products BrainVision RDA stream (TCP/IP).
    BrainVision,
    /// Unrecognised or not yet connected.
    Unknown,
}

// ── Capability flags ──────────────────────────────────────────────────────────

/// Static capability description for a device family.
#[derive(Debug, Clone, Serialize)]
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

    /// Human-readable electrode labels in channel order (as reported by firmware).
    pub electrode_names: Vec<String>,
}

// ── Capability tables ─────────────────────────────────────────────────────────

impl DeviceKind {
    /// Derive the device family from the BLE / USB advertising name.
    ///
    /// Matching is case-insensitive.  Returns [`DeviceKind::Unknown`] for
    /// `None` (not connected) or an unrecognised name.
    /// Detect device kind from a transport-prefixed identifier and/or display name.
    ///
    /// The `device_id` carries a transport prefix (e.g. `"cortex:EPOCX-1234"`,
    /// `"usb:/dev/ttyUSB0"`, `"cgx:/dev/ttyUSB0"`) that narrows the match
    /// before falling back to `from_name` on the display name.
    pub fn from_id_and_name(device_id: Option<&str>, device_name: Option<&str>) -> Self {
        if let Some(id) = device_id.map(str::to_ascii_lowercase) {
            if id.starts_with("neurofield:") {
                return Self::NeuroField;
            }
            if id.starts_with("brainbit:") {
                return Self::BrainBit;
            }
            if id.starts_with("gtec:") {
                return Self::GTec;
            }
            if id.starts_with("brainmaster:") {
                return Self::BrainMaster;
            }
            if id.starts_with("cortex:") {
                return Self::Emotiv;
            }
            if id.starts_with("cgx:") {
                return Self::Cognionics;
            }
            if id.starts_with("usb:") {
                // USB devices need name-based disambiguation.
                let n = device_name.map(str::to_ascii_lowercase).unwrap_or_default();
                if n.contains("cyton") {
                    return Self::OpenBci;
                }
                if n.contains("ganglion") || n.contains("simblee") {
                    return Self::Ganglion;
                }
                return Self::OpenBci;
            }
        }
        Self::from_name(device_name)
    }

    /// Detect device kind from a BLE advertising name or display name.
    pub fn from_name(name: Option<&str>) -> Self {
        let Some(n) = name else { return Self::Unknown };
        let n = n.to_lowercase();

        if n.starts_with("muse") {
            return Self::Muse;
        }
        if n.starts_with("ganglion") || n.starts_with("simblee") {
            return Self::Ganglion;
        }
        if n.starts_with("openbci") || n.starts_with("cyton") {
            return Self::OpenBci;
        }
        if n.contains("mw75") || n.contains("neurable") {
            return Self::Mw75;
        }
        if n.starts_with("hermes") {
            return Self::Hermes;
        }
        if n.starts_with("emotiv")
            || n.starts_with("epoc")
            || n.starts_with("insight")
            || n.starts_with("flex")
            || n.starts_with("mn8")
        {
            return Self::Emotiv;
        }
        if n.starts_with("idun") || n.starts_with("ige") || n.starts_with("guardian") {
            return Self::Idun;
        }
        if n.starts_with("awear") || n.starts_with("luca") {
            return Self::Awear;
        }
        if n.starts_with("mendi") {
            return Self::Mendi;
        }
        if n.contains("cognionics")
            || n.contains("cgx")
            || n.starts_with("quick-")
            || n.starts_with("aim-")
            || n.starts_with("patch")
        {
            return Self::Cognionics;
        }
        if n.starts_with("atu") || n.starts_with("attentivu") {
            return Self::AttentivU;
        }
        if n.contains("brainbit") {
            return Self::BrainBit;
        }
        if n.contains("unicorn") || n.contains("g.tec") || n.starts_with("un-") {
            return Self::GTec;
        }
        if n.contains("neurofield") || n.contains("q21") {
            return Self::NeuroField;
        }
        if n.contains("brainmaster") || n.contains("atlantis") || n.contains("discovery") || n.contains("freedom") {
            return Self::BrainMaster;
        }
        if n.contains("neurosky") || n.contains("mindwave") {
            return Self::NeuroSky;
        }
        if n.contains("neurosity") || n.contains("crown") || n.contains("notion") {
            return Self::Neurosity;
        }
        if n.contains("brainvision") || n.contains("rda") {
            return Self::BrainVision;
        }

        Self::Unknown
    }

    /// Return the static [`DeviceCapabilities`] for this device family.
    pub fn capabilities(self) -> DeviceCapabilities {
        match self {
            Self::Muse => DeviceCapabilities {
                kind: Self::Muse,
                channel_count: 4,
                has_ppg: true,
                has_imu: true,
                has_central_electrodes: false, // TP9 / AF7 / AF8 / TP10 only
                has_full_montage: false,
                sample_rate_hz: 256.0,
                electrode_names: sv(&["TP9", "AF7", "AF8", "TP10"]),
            },
            Self::Ganglion => DeviceCapabilities {
                kind: Self::Ganglion,
                channel_count: 4,
                has_ppg: false,
                has_imu: false,
                has_central_electrodes: true, // user-configurable 10-20
                has_full_montage: false,
                sample_rate_hz: 200.0,
                electrode_names: sv(&["Ch1", "Ch2", "Ch3", "Ch4"]),
            },
            Self::OpenBci => DeviceCapabilities {
                kind: Self::OpenBci,
                channel_count: 8, // Cyton; Cyton+Daisy = 16
                has_ppg: false,
                has_imu: false,
                has_central_electrodes: true, // standard 10-20 includes C3/C4/Cz
                has_full_montage: true,
                sample_rate_hz: 250.0,
                electrode_names: sv(&["Fp1", "Fp2", "C3", "C4", "P7", "P8", "O1", "O2"]),
            },
            Self::Mw75 => DeviceCapabilities {
                kind: Self::Mw75,
                channel_count: 12, // 6 per ear cup
                has_ppg: false,
                has_imu: false,
                has_central_electrodes: false, // temporal sites only (FT7/T7/TP7/CP5/P7/C5 + R)
                has_full_montage: false,
                sample_rate_hz: 500.0,
                electrode_names: sv(&[
                    "FT7", "T7", "TP7", "CP5", "P7", "C5", "FT8", "T8", "TP8", "CP6", "P8", "C6",
                ]),
            },
            Self::Hermes => DeviceCapabilities {
                kind: Self::Hermes,
                channel_count: 8,
                has_ppg: false,
                has_imu: true,
                has_central_electrodes: true, // montage-dependent
                has_full_montage: false,
                sample_rate_hz: 250.0,
                electrode_names: sv(&["Fp1", "Fp2", "AF3", "AF4", "F3", "F4", "FC1", "FC2"]),
            },
            Self::Emotiv => DeviceCapabilities {
                kind: Self::Emotiv,
                channel_count: 14, // EPOC; Insight = 5; Flex = 32
                has_ppg: false,
                has_imu: true,
                has_central_electrodes: true, // FC5/FC6 near-central
                has_full_montage: false,
                sample_rate_hz: 128.0,
                electrode_names: sv(&[
                    "AF3", "F7", "F3", "FC5", "T7", "P7", "O1", "O2", "P8", "T8", "FC6", "F4", "F8", "AF4",
                ]),
            },
            Self::Idun => DeviceCapabilities {
                kind: Self::Idun,
                channel_count: 1, // single bipolar channel
                has_ppg: false,
                has_imu: true,                 // 6-DOF IMU (accel + gyro)
                has_central_electrodes: false, // in-ear canal placement
                has_full_montage: false,
                sample_rate_hz: 250.0,
                electrode_names: sv(&["EEG"]),
            },
            Self::Awear => DeviceCapabilities {
                kind: Self::Awear,
                channel_count: 1,
                has_ppg: false,
                has_imu: false,
                has_central_electrodes: false,
                has_full_montage: false,
                sample_rate_hz: 256.0,
                electrode_names: sv(&["EEG"]),
            },
            // Static defaults for the Quick-20r (most common CGX model).
            // Actual channel count, electrode names, sample rate, and IMU
            // availability are determined at runtime from the USB descriptor
            // and vary per model:
            //   Quick-20/20r/20m  — 20 EEG, 500 Hz, ACC on r/m variants
            //   Quick-32r         — 30 EEG, 500 Hz, ACC
            //   Quick-8r          —  9 EEG, 500 Hz, ACC
            //   AIM-2             —  0 EEG (11 ExG), 500 Hz
            //   Dev Kit           —  8 EEG, 500 Hz, ACC
            //   Patch-v1/v2       —  2 EEG, 250 Hz, ACC
            Self::Cognionics => DeviceCapabilities {
                kind: Self::Cognionics,
                channel_count: 20,
                has_ppg: false,
                has_imu: true,
                has_central_electrodes: true,
                has_full_montage: true,
                sample_rate_hz: 500.0,
                electrode_names: sv(&[
                    "F7", "Fp1", "Fp2", "F8", "F3", "Fz", "F4", "C3", "Cz", "T6", "T5", "Pz", "P4", "T3", "P3", "O1",
                    "O2", "C4", "T4", "A2",
                ]),
            },
            Self::Mendi => DeviceCapabilities {
                kind: Self::Mendi,
                channel_count: 0, // fNIRS optical channels (not EEG electrodes)
                has_ppg: false,
                has_imu: true,
                has_central_electrodes: false,
                has_full_montage: false,
                sample_rate_hz: 0.0,
                electrode_names: Vec::new(),
            },
            Self::AttentivU => DeviceCapabilities {
                kind: Self::AttentivU,
                channel_count: 4, // 4 ExG channels (EEG/EOG)
                has_ppg: false,
                has_imu: true,                 // 9-axis IMU
                has_central_electrodes: false, // forehead/temple placement
                has_full_montage: false,
                sample_rate_hz: 250.0,
                electrode_names: sv(&["ExG0", "ExG1", "ExG2", "ExG3"]),
            },
            Self::BrainBit => DeviceCapabilities {
                kind: Self::BrainBit,
                channel_count: 4,
                has_ppg: false,
                has_imu: false,
                has_central_electrodes: false,
                has_full_montage: false,
                sample_rate_hz: 250.0,
                electrode_names: sv(&["O1", "O2", "T3", "T4"]),
            },
            Self::GTec => DeviceCapabilities {
                kind: Self::GTec,
                channel_count: 8,
                has_ppg: false,
                has_imu: true,
                has_central_electrodes: true,
                has_full_montage: false,
                sample_rate_hz: 250.0,
                electrode_names: sv(&["EEG 1", "EEG 2", "EEG 3", "EEG 4", "EEG 5", "EEG 6", "EEG 7", "EEG 8"]),
            },
            Self::NeuroField => DeviceCapabilities {
                kind: Self::NeuroField,
                channel_count: 20,
                has_ppg: false,
                has_imu: false,
                has_central_electrodes: true,
                has_full_montage: true,
                sample_rate_hz: 256.0,
                electrode_names: sv(&[
                    "F7", "T3", "T4", "T5", "T6", "Cz", "Fz", "Pz", "F3", "C4", "C3", "P4", "P3", "O2", "O1", "F8",
                    "F4", "Fp1", "Fp2", "HR",
                ]),
            },
            Self::BrainMaster => DeviceCapabilities {
                kind: Self::BrainMaster,
                channel_count: 4,
                has_ppg: false,
                has_imu: false,
                has_central_electrodes: true,
                has_full_montage: false,
                sample_rate_hz: 256.0,
                electrode_names: sv(&["EEG1", "EEG2", "EEG3", "EEG4"]),
            },
            Self::NeuroSky => DeviceCapabilities {
                kind: Self::NeuroSky,
                channel_count: 1,
                has_ppg: false,
                has_imu: false,
                has_central_electrodes: false,
                has_full_montage: false,
                sample_rate_hz: 512.0,
                electrode_names: sv(&["Fp1"]),
            },
            Self::Neurosity => DeviceCapabilities {
                kind: Self::Neurosity,
                channel_count: 8,
                has_ppg: false,
                has_imu: true,
                has_central_electrodes: true,
                has_full_montage: false,
                sample_rate_hz: 256.0,
                electrode_names: sv(&["CP3", "C3", "F5", "PO3", "PO4", "F6", "C4", "CP4"]),
            },
            Self::BrainVision => DeviceCapabilities {
                kind: Self::BrainVision,
                channel_count: 16,
                has_ppg: false,
                has_imu: false,
                has_central_electrodes: true,
                has_full_montage: true,
                sample_rate_hz: 500.0,
                electrode_names: sv(&[
                    "Fp1", "Fp2", "F3", "F4", "C3", "C4", "P3", "P4", "O1", "O2", "F7", "F8", "T7", "T8", "P7", "P8",
                ]),
            },
            Self::Unknown => DeviceCapabilities {
                kind: Self::Unknown,
                channel_count: 0,
                has_ppg: false,
                has_imu: false,
                has_central_electrodes: false,
                has_full_montage: false,
                sample_rate_hz: 0.0,
                electrode_names: Vec::new(),
            },
        }
    }

    /// Convenience: `true` when this is any Muse variant.
    #[inline]
    pub fn is_muse(self) -> bool {
        self == Self::Muse
    }

    /// Return the `&'static str` tag used in IPC messages (matches serde rename).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Muse => "muse",
            Self::Ganglion => "ganglion",
            Self::OpenBci => "open_bci",
            Self::Mw75 => "mw75",
            Self::Hermes => "hermes",
            Self::Emotiv => "emotiv",
            Self::Idun => "idun",
            Self::Awear => "awear",
            Self::Cognionics => "cognionics",
            Self::Mendi => "mendi",
            Self::AttentivU => "attentivu",
            Self::BrainBit => "brainbit",
            Self::GTec => "gtec",
            Self::NeuroField => "neurofield",
            Self::BrainMaster => "brainmaster",
            Self::NeuroSky => "neurosky",
            Self::Neurosity => "neurosity",
            Self::BrainVision => "brainvision",
            Self::Unknown => "unknown",
        }
    }

    /// Parse a kind tag string back into a [`DeviceKind`].
    ///
    /// Accepts the canonical tags returned by [`as_str`](Self::as_str) as well
    /// as common runtime values produced by the session layer (e.g.
    /// `"openbci_cyton"`).  Falls back to [`from_name`](Self::from_name) for
    /// any unrecognised string so BLE advertising names still work.
    pub fn from_kind_str(s: &str) -> Self {
        match s {
            "muse" => Self::Muse,
            "ganglion" => Self::Ganglion,
            "open_bci" => Self::OpenBci,
            "mw75" => Self::Mw75,
            "hermes" => Self::Hermes,
            "emotiv" => Self::Emotiv,
            "idun" => Self::Idun,
            "awear" => Self::Awear,
            "cognionics" => Self::Cognionics,
            "mendi" => Self::Mendi,
            "attentivu" => Self::AttentivU,
            "brainbit" => Self::BrainBit,
            "gtec" => Self::GTec,
            "neurofield" => Self::NeuroField,
            "brainmaster" => Self::BrainMaster,
            "neurosky" => Self::NeuroSky,
            "neurosity" => Self::Neurosity,
            "brainvision" | "rda" => Self::BrainVision,
            "unknown" => Self::Unknown,
            other => {
                // Handle runtime kind strings like "openbci_cyton", "openbci_cyton_daisy", etc.
                if other.starts_with("openbci") {
                    return Self::OpenBci;
                }
                // Fall back to advertising-name detection for anything else.
                Self::from_name(Some(other))
            }
        }
    }
}

/// Helper: convert a `&[&str]` to `Vec<String>`.
fn sv(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| (*s).to_owned()).collect()
}

// ── Supported-devices catalog (UI) ────────────────────────────────────────────

/// A single device model shown in the "Supported Devices" UI.
#[derive(Debug, Clone, Serialize)]
pub struct SupportedDevice {
    /// i18n key for the device name.
    pub name_key: String,
    /// Path to the device image (relative to `/devices/`).
    pub image: String,
    /// If `true`, this device connects via the SkillClient iOS app only
    /// (BLE on phone → iroh tunnel → desktop processing).
    #[serde(skip_serializing_if = "is_false")]
    pub ios_only: bool,
}

fn is_false(v: &bool) -> bool {
    !v
}

/// A company / brand grouping in the "Supported Devices" UI.
#[derive(Debug, Clone, Serialize)]
pub struct SupportedCompany {
    /// Short identifier used for expand/collapse state.
    pub id: String,
    /// i18n key for the company name.
    pub name_key: String,
    /// Path to the company logo (relative to `/logos/`).
    pub logo: String,
    /// Individual device models.
    pub devices: Vec<SupportedDevice>,
    /// i18n keys for setup instructions (rendered as ordered steps).
    pub instruction_keys: Vec<String>,
}

/// The canonical list of supported companies and their devices.
///
/// This is the **single source of truth** — the Svelte frontend fetches
/// this via the `get_supported_companies` Tauri command.
pub fn supported_companies() -> Vec<SupportedCompany> {
    // Sorted alphabetically by company name.
    vec![
        // ── A ─────────────────────────────────────────────────────────────
        SupportedCompany {
            id: "attentivu".into(),
            name_key: "settings.supportedDevices.company.attentivu".into(),
            logo: "/devices/attentivu-glasses.png".into(),
            devices: vec![SupportedDevice {
                name_key: "settings.supportedDevices.device.attentivuGlasses".into(),
                ios_only: true,
                image: "/devices/attentivu-glasses.png".into(),
            }],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.attentivu1".into(),
                "settings.supportedDevices.instruction.attentivu2".into(),
                "settings.supportedDevices.instruction.attentivu3".into(),
            ],
        },
        SupportedCompany {
            id: "awear".into(),
            name_key: "settings.supportedDevices.company.awear".into(),
            logo: "/logos/awear.png".into(),
            devices: vec![SupportedDevice {
                name_key: "settings.supportedDevices.device.awearEeg".into(),
                ios_only: false,
                image: "/devices/awear-eeg.png".into(),
            }],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.awear1".into(),
                "settings.supportedDevices.instruction.awear2".into(),
            ],
        },
        // ── B ─────────────────────────────────────────────────────
        SupportedCompany {
            id: "brainbit".into(),
            name_key: "settings.supportedDevices.company.brainbit".into(),
            logo: "/logos/brainbit.png".into(),
            devices: vec![
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.brainbitOriginal".into(),
                    ios_only: false,
                    image: "/devices/brainbit-headband.png".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.brainbit2".into(),
                    ios_only: false,
                    image: "/devices/brainbit-pro.png".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.brainbitFlex".into(),
                    ios_only: false,
                    image: "/devices/brainbit-flex.png".into(),
                },
            ],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.brainbit1".into(),
                "settings.supportedDevices.instruction.brainbit2".into(),
            ],
        },
        SupportedCompany {
            id: "brainmaster".into(),
            name_key: "settings.supportedDevices.company.brainmaster".into(),
            logo: "/logos/brainmaster.png".into(),
            devices: vec![
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.brainmasterAtlantis4".into(),
                    ios_only: false,
                    image: "/devices/brainmaster-atlantis.jpg".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.brainmasterDiscovery".into(),
                    ios_only: false,
                    image: "/devices/brainmaster-discovery.jpg".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.brainmasterFreedom".into(),
                    ios_only: false,
                    image: "/devices/brainmaster-freedom.jpg".into(),
                },
            ],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.brainmaster1".into(),
                "settings.supportedDevices.instruction.brainmaster2".into(),
            ],
        },
        // ── C ─────────────────────────────────────────────────────────────
        SupportedCompany {
            id: "cognionics".into(),
            name_key: "settings.supportedDevices.company.cognionics".into(),
            logo: "/logos/cognionics.png".into(),
            devices: vec![
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.cgxQuick20r".into(),
                    ios_only: false,
                    image: "/devices/cgx-quick-20r.png".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.cgxQuick32r".into(),
                    ios_only: false,
                    image: "/devices/cgx-quick-32r.png".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.cgxQuick8r".into(),
                    ios_only: false,
                    image: "/devices/cgx-quick-8r.png".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.cgxAim2".into(),
                    ios_only: false,
                    image: "/devices/cgx-aim-2.png".into(),
                },
            ],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.cognionics1".into(),
                "settings.supportedDevices.instruction.cognionics2".into(),
            ],
        },
        // ── E ─────────────────────────────────────────────────────────────
        SupportedCompany {
            id: "emotiv".into(),
            name_key: "settings.supportedDevices.company.emotiv".into(),
            logo: "/logos/emotiv.png".into(),
            devices: vec![
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.epocX".into(),
                    ios_only: false,
                    image: "/devices/emotiv-epoc-x.webp".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.flexSaline".into(),
                    ios_only: false,
                    image: "/devices/emotiv-flex-saline.webp".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.insight".into(),
                    ios_only: false,
                    image: "/devices/emotiv-insight.webp".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.mn8".into(),
                    ios_only: false,
                    image: "/devices/emotiv-mn8.webp".into(),
                },
            ],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.emotiv1".into(),
                "settings.supportedDevices.instruction.emotiv2".into(),
            ],
        },
        // ── G ─────────────────────────────────────────────────────
        SupportedCompany {
            id: "gtec".into(),
            name_key: "settings.supportedDevices.company.gtec".into(),
            logo: "/logos/gtec.png".into(),
            devices: vec![SupportedDevice {
                name_key: "settings.supportedDevices.device.unicornHybridBlack".into(),
                ios_only: false,
                image: "/devices/gtec-unicorn.jpg".into(),
            }],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.gtec1".into(),
                "settings.supportedDevices.instruction.gtec2".into(),
            ],
        },
        // ── I ─────────────────────────────────────────────────────────────
        SupportedCompany {
            id: "idun".into(),
            name_key: "settings.supportedDevices.company.idun".into(),
            logo: "/logos/idun.png".into(),
            devices: vec![SupportedDevice {
                name_key: "settings.supportedDevices.device.guardian".into(),
                ios_only: false,
                image: "/devices/idun-guardian.png".into(),
            }],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.idun1".into(),
                "settings.supportedDevices.instruction.idun2".into(),
            ],
        },
        SupportedCompany {
            id: "muse".into(),
            name_key: "settings.supportedDevices.company.muse".into(),
            logo: "/logos/muse.png".into(),
            devices: vec![
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.muse2016".into(),
                    ios_only: false,
                    image: "/devices/muse-gen1.jpg".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.muse2".into(),
                    ios_only: false,
                    image: "/devices/muse-gen2.jpg".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.museS".into(),
                    ios_only: false,
                    image: "/devices/muse-s-gen1.jpg".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.museSAthena".into(),
                    ios_only: false,
                    image: "/devices/muse-s-athena.jpg".into(),
                },
            ],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.muse1".into(),
                "settings.supportedDevices.instruction.muse2".into(),
            ],
        },
        // ── M ─────────────────────────────────────────────────────────────
        SupportedCompany {
            id: "mendi".into(),
            name_key: "settings.supportedDevices.company.mendi".into(),
            logo: "/logos/mendi.png".into(),
            devices: vec![SupportedDevice {
                name_key: "settings.supportedDevices.device.mendiHeadband".into(),
                ios_only: false,
                image: "/devices/mendi-headband.png".into(),
            }],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.mendi1".into(),
                "settings.supportedDevices.instruction.mendi2".into(),
            ],
        },
        // ── N ─────────────────────────────────────────────────────
        SupportedCompany {
            id: "neurofield".into(),
            name_key: "settings.supportedDevices.company.neurofield".into(),
            logo: "/logos/neurofield.png".into(),
            devices: vec![SupportedDevice {
                name_key: "settings.supportedDevices.device.neurofieldQ21".into(),
                ios_only: false,
                image: "/devices/neurofield-q21.png".into(),
            }],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.neurofield1".into(),
                "settings.supportedDevices.instruction.neurofield2".into(),
            ],
        },
        SupportedCompany {
            id: "neurosity".into(),
            name_key: "settings.supportedDevices.company.neurosity".into(),
            logo: "/logos/neurosity.png".into(),
            devices: vec![SupportedDevice {
                name_key: "settings.supportedDevices.device.neurosityCrownNotion".into(),
                ios_only: false,
                image: "/devices/neurosity-crown.png".into(),
            }],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.neurosity1".into(),
                "settings.supportedDevices.instruction.neurosity2".into(),
            ],
        },
        SupportedCompany {
            id: "neurosky".into(),
            name_key: "settings.supportedDevices.company.neurosky".into(),
            logo: "/logos/neurosky.png".into(),
            devices: vec![SupportedDevice {
                name_key: "settings.supportedDevices.device.neuroskyMindWave".into(),
                ios_only: false,
                image: "/devices/neurosky-mindwave.webp".into(),
            }],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.neurosky1".into(),
                "settings.supportedDevices.instruction.neurosky2".into(),
            ],
        },
        // ── N (Neurable) ──────────────────────────────────────────────
        SupportedCompany {
            id: "neurable".into(),
            name_key: "settings.supportedDevices.company.neurable".into(),
            logo: "/logos/neurable.png".into(),
            devices: vec![SupportedDevice {
                name_key: "settings.supportedDevices.device.mw75Neuro".into(),
                ios_only: false,
                image: "/devices/muse-mw75.jpg".into(),
            }],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.neurable1".into(),
                "settings.supportedDevices.instruction.neurable2".into(),
            ],
        },
        // ── O ─────────────────────────────────────────────────────────────
        SupportedCompany {
            id: "openbci".into(),
            name_key: "settings.supportedDevices.company.openbci".into(),
            logo: "/logos/openbci.png".into(),
            devices: vec![
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.cyton".into(),
                    ios_only: false,
                    image: "/devices/openbci-cyton.png".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.cytonDaisy".into(),
                    ios_only: false,
                    image: "/devices/openbci-cyton-daisy.jpg".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.galea".into(),
                    ios_only: false,
                    image: "/devices/openbci-galea.jpg".into(),
                },
                SupportedDevice {
                    name_key: "settings.supportedDevices.device.ganglion".into(),
                    ios_only: false,
                    image: "/devices/openbci-ganglion.jpg".into(),
                },
            ],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.openbci1".into(),
                "settings.supportedDevices.instruction.openbci2".into(),
            ],
        },
        // ── O ─────────────────────────────────────────────────────────────
        SupportedCompany {
            id: "oura".into(),
            name_key: "settings.supportedDevices.company.oura".into(),
            logo: "/logos/oura.svg".into(),
            devices: vec![SupportedDevice {
                name_key: "settings.supportedDevices.device.ouraRing".into(),
                ios_only: false,
                image: "/devices/oura-ring.svg".into(),
            }],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.oura1".into(),
                "settings.supportedDevices.instruction.oura2".into(),
                "settings.supportedDevices.instruction.oura3".into(),
            ],
        },
        SupportedCompany {
            id: "brainvision".into(),
            name_key: "settings.supportedDevices.company.brainvision".into(),
            logo: "/logos/brainvision.jpg".into(),
            devices: vec![SupportedDevice {
                name_key: "settings.supportedDevices.device.brainvisionRda".into(),
                ios_only: false,
                image: "/devices/brainvision-rda.jpg".into(),
            }],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.brainvision1".into(),
                "settings.supportedDevices.instruction.brainvision2".into(),
            ],
        },
        // ── R ─────────────────────────────────────────────────────────────
        SupportedCompany {
            id: "reak".into(),
            name_key: "settings.supportedDevices.company.reak".into(),
            logo: "/logos/reak.png".into(),
            devices: vec![SupportedDevice {
                name_key: "settings.supportedDevices.device.nucleusHermes".into(),
                ios_only: false,
                image: "/devices/re-ak-nucleus-hermes.png".into(),
            }],
            instruction_keys: vec![
                "settings.supportedDevices.instruction.reak1".into(),
                "settings.supportedDevices.instruction.reak2".into(),
            ],
        },
    ]
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
        assert_eq!(DeviceKind::from_name(Some("Neurable-XYZ")), DeviceKind::Mw75);
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
    fn from_name_attentivu() {
        assert_eq!(DeviceKind::from_name(Some("AttentivU-1234")), DeviceKind::AttentivU);
        assert_eq!(DeviceKind::from_name(Some("AtU1234")), DeviceKind::AttentivU);
        assert_eq!(DeviceKind::from_name(Some("AtU-5678")), DeviceKind::AttentivU);
        assert_eq!(DeviceKind::from_name(Some("atu9999")), DeviceKind::AttentivU);
    }

    #[test]
    fn from_name_unknown() {
        assert_eq!(DeviceKind::from_name(None), DeviceKind::Unknown);
        assert_eq!(DeviceKind::from_name(Some("random-device")), DeviceKind::Unknown);
    }

    #[test]
    fn from_name_new_devices() {
        assert_eq!(DeviceKind::from_name(Some("MindWave Mobile")), DeviceKind::NeuroSky);
        assert_eq!(DeviceKind::from_name(Some("Neurosity Crown")), DeviceKind::Neurosity);
        assert_eq!(DeviceKind::from_name(Some("BrainVision RDA")), DeviceKind::BrainVision);
    }

    #[test]
    fn from_name_cognionics() {
        assert_eq!(DeviceKind::from_name(Some("CGX Quick-20r")), DeviceKind::Cognionics);
        assert_eq!(DeviceKind::from_name(Some("cognionics-device")), DeviceKind::Cognionics);
        assert_eq!(
            DeviceKind::from_name(Some("Quick-20r v2 Q20r-ABCD")),
            DeviceKind::Cognionics
        );
        assert_eq!(DeviceKind::from_name(Some("Quick-32r")), DeviceKind::Cognionics);
        assert_eq!(DeviceKind::from_name(Some("AIM-2 device")), DeviceKind::Cognionics);
        assert_eq!(DeviceKind::from_name(Some("Patch-v2")), DeviceKind::Cognionics);
    }

    #[test]
    fn from_name_mendi() {
        assert_eq!(DeviceKind::from_name(Some("Mendi")), DeviceKind::Mendi);
        assert_eq!(DeviceKind::from_name(Some("mendi-1234")), DeviceKind::Mendi);
    }

    #[test]
    fn from_kind_str_canonical() {
        assert_eq!(DeviceKind::from_kind_str("muse"), DeviceKind::Muse);
        assert_eq!(DeviceKind::from_kind_str("ganglion"), DeviceKind::Ganglion);
        assert_eq!(DeviceKind::from_kind_str("open_bci"), DeviceKind::OpenBci);
        assert_eq!(DeviceKind::from_kind_str("mw75"), DeviceKind::Mw75);
        assert_eq!(DeviceKind::from_kind_str("hermes"), DeviceKind::Hermes);
        assert_eq!(DeviceKind::from_kind_str("emotiv"), DeviceKind::Emotiv);
        assert_eq!(DeviceKind::from_kind_str("idun"), DeviceKind::Idun);
        assert_eq!(DeviceKind::from_kind_str("cognionics"), DeviceKind::Cognionics);
        assert_eq!(DeviceKind::from_kind_str("mendi"), DeviceKind::Mendi);
        assert_eq!(DeviceKind::from_kind_str("neurosky"), DeviceKind::NeuroSky);
        assert_eq!(DeviceKind::from_kind_str("neurosity"), DeviceKind::Neurosity);
        assert_eq!(DeviceKind::from_kind_str("brainvision"), DeviceKind::BrainVision);
        assert_eq!(DeviceKind::from_kind_str("unknown"), DeviceKind::Unknown);
    }

    #[test]
    fn from_kind_str_runtime_openbci() {
        assert_eq!(DeviceKind::from_kind_str("openbci_cyton"), DeviceKind::OpenBci);
        assert_eq!(DeviceKind::from_kind_str("openbci_cyton_daisy"), DeviceKind::OpenBci);
        assert_eq!(DeviceKind::from_kind_str("openbci_galea"), DeviceKind::OpenBci);
    }

    #[test]
    fn from_kind_str_fallback_to_from_name() {
        // An advertising name should still work via fallback.
        assert_eq!(DeviceKind::from_kind_str("Muse-2-ABCD"), DeviceKind::Muse);
        assert_eq!(DeviceKind::from_kind_str("random-thing"), DeviceKind::Unknown);
    }

    // ── from_id_and_name ──────────────────────────────────────────────

    #[test]
    fn from_id_and_name_cortex_prefix() {
        assert_eq!(
            DeviceKind::from_id_and_name(Some("cortex:EPOCX-1234"), None),
            DeviceKind::Emotiv
        );
        assert_eq!(
            DeviceKind::from_id_and_name(Some("cortex:EPOCX-1234"), Some("unknown")),
            DeviceKind::Emotiv
        );
    }

    #[test]
    fn from_id_and_name_usb_prefix() {
        assert_eq!(
            DeviceKind::from_id_and_name(Some("usb:/dev/ttyUSB0"), None),
            DeviceKind::OpenBci
        );
        assert_eq!(
            DeviceKind::from_id_and_name(Some("usb:COM3"), Some("Cyton-1234")),
            DeviceKind::OpenBci
        );
        assert_eq!(
            DeviceKind::from_id_and_name(Some("usb:/dev/ttyUSB0"), Some("Ganglion")),
            DeviceKind::Ganglion
        );
        assert_eq!(
            DeviceKind::from_id_and_name(Some("usb:COM4"), Some("Simblee-1234")),
            DeviceKind::Ganglion
        );
    }

    #[test]
    fn from_id_and_name_cgx_prefix() {
        assert_eq!(
            DeviceKind::from_id_and_name(Some("cgx:/dev/ttyUSB0"), None),
            DeviceKind::Cognionics
        );
    }

    #[test]
    fn from_id_and_name_falls_back_to_name() {
        assert_eq!(
            DeviceKind::from_id_and_name(None, Some("ganglion-1234")),
            DeviceKind::Ganglion
        );
        assert_eq!(DeviceKind::from_id_and_name(None, Some("mendi")), DeviceKind::Mendi);
        assert_eq!(DeviceKind::from_id_and_name(None, None), DeviceKind::Unknown);
    }

    #[test]
    fn capabilities_include_electrode_names() {
        let caps = DeviceKind::Muse.capabilities();
        assert_eq!(caps.electrode_names, &["TP9", "AF7", "AF8", "TP10"]);
        assert_eq!(caps.channel_count, caps.electrode_names.len());
    }

    #[test]
    fn supported_companies_non_empty() {
        let companies = supported_companies();
        assert!(!companies.is_empty());
        for c in &companies {
            assert!(!c.id.is_empty());
            assert!(!c.devices.is_empty());
            assert!(!c.instruction_keys.is_empty());
        }
    }

    // ── DeviceKind::as_str roundtrip ─────────────────────────────────────

    #[test]
    fn as_str_roundtrip_all_variants() {
        let kinds = [
            DeviceKind::Muse,
            DeviceKind::Ganglion,
            DeviceKind::OpenBci,
            DeviceKind::BrainBit,
            DeviceKind::Emotiv,
            DeviceKind::Hermes,
            DeviceKind::Idun,
            DeviceKind::Mendi,
            DeviceKind::Mw75,
            DeviceKind::Cognionics,
            DeviceKind::AttentivU,
            DeviceKind::Unknown,
        ];
        for kind in kinds {
            let s = kind.as_str();
            let back = DeviceKind::from_kind_str(s);
            assert_eq!(kind, back, "roundtrip failed for {s}");
        }
    }

    #[test]
    fn is_muse_true_for_muse() {
        assert!(DeviceKind::Muse.is_muse());
    }

    #[test]
    fn is_muse_false_for_others() {
        assert!(!DeviceKind::Emotiv.is_muse());
        assert!(!DeviceKind::Ganglion.is_muse());
        assert!(!DeviceKind::Unknown.is_muse());
    }

    #[test]
    fn capabilities_muse_has_ppg() {
        let caps = DeviceKind::Muse.capabilities();
        assert!(caps.has_ppg);
        assert!(caps.has_imu);
        assert_eq!(caps.channel_count, 4);
    }

    #[test]
    fn capabilities_ganglion_no_ppg() {
        let caps = DeviceKind::Ganglion.capabilities();
        assert!(!caps.has_ppg);
        assert_eq!(caps.channel_count, 4);
    }

    #[test]
    fn capabilities_openbci_8_channels() {
        let caps = DeviceKind::OpenBci.capabilities();
        assert_eq!(caps.channel_count, 8);
    }

    #[test]
    fn capabilities_emotiv_has_imu() {
        let caps = DeviceKind::Emotiv.capabilities();
        assert!(caps.has_imu);
    }

    #[test]
    fn capabilities_hermes_8_channels() {
        let caps = DeviceKind::Hermes.capabilities();
        assert_eq!(caps.channel_count, 8);
        assert!(caps.has_imu);
    }

    #[test]
    fn capabilities_mw75_12_channels() {
        let caps = DeviceKind::Mw75.capabilities();
        assert_eq!(caps.channel_count, 12);
    }

    #[test]
    fn capabilities_idun_1_channel() {
        let caps = DeviceKind::Idun.capabilities();
        assert_eq!(caps.channel_count, 1);
        assert!(caps.has_imu);
    }

    #[test]
    fn capabilities_cognionics() {
        let caps = DeviceKind::Cognionics.capabilities();
        assert!(caps.channel_count >= 8);
    }

    #[test]
    fn capabilities_brainbit() {
        let caps = DeviceKind::BrainBit.capabilities();
        assert_eq!(caps.channel_count, 4);
    }

    #[test]
    fn capabilities_attentivu() {
        let caps = DeviceKind::AttentivU.capabilities();
        assert_eq!(caps.channel_count, 4);
        assert!(caps.has_imu);
    }

    #[test]
    fn capabilities_mendi_fnirs() {
        let caps = DeviceKind::Mendi.capabilities();
        assert!(caps.has_imu);
    }

    #[test]
    fn capabilities_unknown_zero() {
        let caps = DeviceKind::Unknown.capabilities();
        assert_eq!(caps.channel_count, 0);
        assert!(!caps.has_ppg);
        assert!(!caps.has_imu);
    }

    #[test]
    fn capabilities_all_have_kind() {
        let kinds = [
            DeviceKind::Muse,
            DeviceKind::Ganglion,
            DeviceKind::OpenBci,
            DeviceKind::Mw75,
            DeviceKind::Hermes,
            DeviceKind::Emotiv,
            DeviceKind::Idun,
            DeviceKind::Awear,
            DeviceKind::Mendi,
            DeviceKind::Cognionics,
            DeviceKind::AttentivU,
            DeviceKind::BrainBit,
            DeviceKind::Unknown,
        ];
        for kind in kinds {
            let caps = kind.capabilities();
            assert_eq!(caps.kind, kind);
        }
    }

    #[test]
    fn device_kind_serde_roundtrip() {
        for kind in [DeviceKind::Muse, DeviceKind::Emotiv, DeviceKind::Unknown] {
            let json = serde_json::to_string(&kind).unwrap();
            let back: DeviceKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn paired_device_serde_roundtrip() {
        let pd = PairedDevice {
            id: "AA:BB:CC".into(),
            name: "Muse-1234".into(),
            last_seen: 1700000000,
        };
        let json = serde_json::to_string(&pd).unwrap();
        let back: PairedDevice = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, pd.id);
        assert_eq!(back.name, pd.name);
        assert_eq!(back.last_seen, 1700000000);
    }
}
