//! GATT UUIDs, BLE command sequences, GAIA protocol, and wire-format constants
//! for MW75 Neuro.
//!
//! # Protocol layers
//!
//! The MW75 (and related M&D headphones like MW08, MW09, MG20, MH40W) use a
//! **Qualcomm chipset** with two communication layers:
//!
//! 1. **BLE GATT** — Commands (ANC, EQ, battery, etc.) using the Qualcomm
//!    GAIA (Generic Application Interface Architecture) v3 protocol.
//! 2. **RFCOMM (SPP)** — Streaming data (EEG in MW75 Neuro) over Bluetooth
//!    Classic, channel 25.
//!
//! # GAIA v3 packet structure
//!
//! All M&D-specific commands use the custom GAIA vendor ID `0x099A` (2458).
//! The wire format for received data (responses/notifications) is:
//!
//! ```text
//! Offset  Size  Field
//! ──────  ────  ─────
//!   0       2   Vendor ID (uint16 BE) — 0x099A for M&D custom commands
//!   2       1   Command description / feature ID
//!   3       1   Command type ID (see [`GaiaCommand`])
//!   4+      N   Payload bytes (see [`GaiaPayload`])
//! ```
//!
//! Outgoing BLE commands are written as 5-byte sequences:
//!
//! ```text
//! [0x09, 0x9A, 0x03, <command_id>, <payload>]
//!   ^^^^^^^^^^^         ^^^^^^^^^^   ^^^^^^^
//!   Vendor 0x099A       Command      Value/query
//! ```
//!
//! # EEG-specific service
//!
//! The MW75 Neuro adds an EEG-specific BLE GATT service on top of the
//! standard GAIA commands.  All UUIDs belong to the vendor namespace
//! `000011XX-d102-11e1-9b23-00025b00a5a5`.

use uuid::Uuid;

// ══════════════════════════════════════════════════════════════════════════════
//  GAIA Protocol — Qualcomm Generic Application Interface Architecture
// ══════════════════════════════════════════════════════════════════════════════

// ── Vendor IDs ───────────────────────────────────────────────────────────────

/// GAIA v1/v2 legacy vendor ID (Qualcomm/QTIL).
pub const GAIA_VENDOR_QTIL_V1V2: u16 = 0x000A; // 10

/// GAIA v3 standard vendor ID (Qualcomm/QTIL).
pub const GAIA_VENDOR_QTIL_V3: u16 = 0x001D; // 29

/// M&D (Gemo) custom GAIA vendor ID.
///
/// All headphone-specific commands (ANC, EQ, battery, auto-off, etc.) use
/// this vendor ID.
///
/// The 5-byte BLE command prefix `[0x09, 0x9A, ...]` encodes this as the
/// first two bytes of the GAIA packet header.
pub const GAIA_VENDOR_MD: u16 = 0x099A; // 2458

// ── GAIA v3 Packet Offsets ───────────────────────────────────────────────────

/// Byte offset of the vendor ID in a GAIA v3 response/notification packet.
pub const GAIA_VENDOR_OFFSET: usize = 0;

/// Byte offset of the command description field.
pub const GAIA_COMMAND_DESC_OFFSET: usize = 2;

/// Byte offset of the command type (command ID) field.
pub const GAIA_COMMAND_TYPE_OFFSET: usize = 3;

/// Byte offset where the payload begins.
pub const GAIA_PAYLOAD_OFFSET: usize = 4;

// ── GAIA Command IDs ──────────────────────────────

/// GAIA command IDs used by M&D headphones.
///
/// These are the command type bytes (byte offset 3) in the GAIA v3 protocol.
/// Each command is sent as `[0x09, 0x9A, 0x03, <command>, <payload>]` over
/// BLE, where `0x099A` is the M&D vendor ID.
///
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GaiaCommand {
    /// Register for battery level notifications.
    /// Payload: `0x01` to register.
    RegisterBattery = 0x06,

    /// Get or set product identifier.
    /// Payload: `0xFF` to query; response is 2 bytes.
    ProductId = 0x10,

    /// Get or set ANC / World Volume mode.
    /// Payload: `0xFF` to query, or a [`GaiaAncMode`] value to set.
    WorldVolume = 0x11,

    /// Get or set auto-off timer.
    /// Payload: `0xFF` to query, or a [`GaiaAutoOff`] value to set.
    AutoOff = 0x12,

    /// Get or set in-ear detection.
    /// Payload: `0xFF` to query, `0x01` disable, `0x02` enable.
    InEarDetection = 0x13,

    /// Query battery level.
    /// Payload: `0xFF` to query; response is 1 byte (0–100%).
    GetBatteryLevel = 0x14,

    /// Set device Bluetooth name.
    /// Payload: `[length, ...name_bytes]`.
    DeviceName = 0x15,

    /// Get or set find-device tone.
    /// Payload: `0xFF` to query, `0x01` disable, `0x02` enable.
    FindDevice = 0x17,

    /// EEG enable/disable (MW75 Neuro specific).
    /// Payload: `0x01` enable, `0x00` disable.
    EegMode = 0x60,

    /// Raw data mode enable/disable (MW75 Neuro specific).
    /// Payload: `0x01` enable, `0x00` disable.
    RawMode = 0x41,

    /// RFCOMM connection status notification.
    /// Payload: `0x00` = not connected, `0x01` = connected.
    RfcommStatus = 0x88,

    /// Battery notification (alternate channel, used on some models).
    BatteryNotification82 = 0x82,

    /// Physical button press event (ANC cycle button).
    /// Response payload indicates new ANC state.
    VolumeButtonPress = 0x91,

    /// Unknown command byte sometimes seen in notifications.
    UnknownE0 = 0xE0,

    /// Battery notification / success response.
    /// Also used as the general success response code (`0xF1`).
    BatteryNotification = 0xF1,
}

impl GaiaCommand {
    /// Try to convert a raw byte to a known command ID.
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x06 => Some(Self::RegisterBattery),
            0x10 => Some(Self::ProductId),
            0x11 => Some(Self::WorldVolume),
            0x12 => Some(Self::AutoOff),
            0x13 => Some(Self::InEarDetection),
            0x14 => Some(Self::GetBatteryLevel),
            0x15 => Some(Self::DeviceName),
            0x17 => Some(Self::FindDevice),
            0x41 => Some(Self::RawMode),
            0x60 => Some(Self::EegMode),
            0x82 => Some(Self::BatteryNotification82),
            0x88 => Some(Self::RfcommStatus),
            0x91 => Some(Self::VolumeButtonPress),
            0xE0 => Some(Self::UnknownE0),
            0xF1 => Some(Self::BatteryNotification),
            _ => None,
        }
    }
}

// ── GAIA Payload Values ───────────────────────────

/// Common query payload — sent as the last byte to request the current value.
pub const GAIA_QUERY: u8 = 0xFF;

/// Payload values for GAIA commands.
///
pub mod gaia_payload {
    // ── ANC / World Volume (command 0x11) ────────────────────────────────
    /// ANC and ambient sound off.
    pub const WV_OFF: u8 = 0x01;
    /// ANC High (maximum noise cancellation).
    pub const WV_ANC_HIGH: u8 = 0x02;
    /// ANC Low (moderate noise cancellation).
    pub const WV_ANC_LOW: u8 = 0x03;
    /// Ambient mode — voice listening (optimized for speech).
    pub const WV_AMBIENT_VOICE: u8 = 0x04;
    /// Ambient mode — ambient awareness (full transparency).
    pub const WV_AMBIENT_AWARENESS: u8 = 0x05;
    /// ANC Adaptive mode.
    pub const WV_ANC_ADAPTIVE: u8 = 0x06;

    // ── Auto-Off Timer (command 0x12) ────────────────────────────────────
    /// Auto-off disabled (never).
    pub const AUTO_OFF_NEVER: u8 = 0x01;
    /// Auto-off after 30 minutes.
    pub const AUTO_OFF_30_MIN: u8 = 0x02;
    /// Auto-off after 1 hour.
    pub const AUTO_OFF_1_HOUR: u8 = 0x03;
    /// Auto-off after 3 hours.
    pub const AUTO_OFF_3_HOURS: u8 = 0x04;

    // ── In-Ear Detection (command 0x13) ──────────────────────────────────
    /// In-ear detection disabled.
    pub const IN_EAR_DISABLE: u8 = 0x01;
    /// In-ear detection enabled.
    pub const IN_EAR_ENABLE: u8 = 0x02;

    // ── Find Device Tone (command 0x17) ──────────────────────────────────
    /// Find-device tone disabled.
    pub const FIND_TONE_DISABLE: u8 = 0x01;
    /// Find-device tone enabled.
    pub const FIND_TONE_ENABLE: u8 = 0x02;

    // ── Battery Registration (command 0x06) ──────────────────────────────
    /// Register for battery level notifications.
    pub const REGISTER_BATTERY: u8 = 0x01;
}

// ── GAIA Response Values ─────────────────────────

/// Response codes in GAIA payloads.
//
pub mod gaia_response {
    // ── Generic result codes ─────────────────────────────────────────────
    /// Command failed.
    pub const SET_FAILED: u8 = 0xF0;
    /// Command succeeded.
    pub const SET_SUCCESS: u8 = 0xF1;

    // ── ANC / World Volume responses (command 0x11) ──────────────────────
    /// ANC and ambient off.
    pub const WV_ALL_OFF: u8 = 0x01;
    /// ANC High active.
    pub const WV_ANC_HIGH: u8 = 0x02;
    /// ANC Low active.
    pub const WV_ANC_LOW: u8 = 0x03;
    /// Ambient — voice listening.
    pub const WV_AMBIENT_VOICE: u8 = 0x04;
    /// Ambient — awareness.
    pub const WV_AMBIENT_AWARENESS: u8 = 0x05;
    /// ANC Adaptive active.
    pub const WV_ANC_ADAPTIVE: u8 = 0x06;

    // ── Auto-Off responses (command 0x12) ────────────────────────────────
    /// Auto-off: never.
    pub const AUTO_OFF_NEVER: u8 = 0x01;
    /// Auto-off: 30 minutes.
    pub const AUTO_OFF_30_MIN: u8 = 0x02;
    /// Auto-off: 1 hour.
    pub const AUTO_OFF_1_HOUR: u8 = 0x03;
    /// Auto-off: 3 hours.
    pub const AUTO_OFF_3_HOURS: u8 = 0x04;

    // ── In-Ear Detection responses (command 0x13) ────────────────────────
    /// In-ear detection disabled.
    pub const IN_EAR_DISABLE: u8 = 0x01;
    /// In-ear detection enabled.
    pub const IN_EAR_ENABLE: u8 = 0x02;

    // ── Find Device responses (command 0x17) ─────────────────────────────
    /// Find-device tone disabled.
    pub const FIND_TONE_DISABLE: u8 = 0x01;
    /// Find-device tone enabled.
    pub const FIND_TONE_ENABLE: u8 = 0x02;

    // ── Button press event responses (command 0x91) ──────────────────────
    /// Button press: ANC/Ambient off.
    pub const BTN_WV_OFF: u8 = 0x01;
    /// Button press: ANC Max (High).
    pub const BTN_WV_ANC_MAX: u8 = 0x02;
    /// Button press: Ambient Awareness.
    pub const BTN_WV_AMBIENT_AWARENESS: u8 = 0x04;
}

// ── Typed enums for ANC, Auto-Off, etc. ──────────────────────────────────────

/// ANC (Active Noise Cancellation) / World Volume mode.
///
/// Maps to the payload values for command `0x11` (WorldVolume).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GaiaAncMode {
    /// All noise processing off (no ANC, no ambient).
    Off,
    /// ANC High — maximum noise cancellation.
    AncHigh,
    /// ANC Low — moderate noise cancellation.
    AncLow,
    /// Ambient — voice listening (optimized for speech).
    AmbientVoice,
    /// Ambient — awareness (full transparency).
    AmbientAwareness,
    /// ANC Adaptive — automatically adjusts to environment.
    AncAdaptive,
}

impl GaiaAncMode {
    /// Convert to the wire payload byte for sending.
    pub fn to_payload(self) -> u8 {
        match self {
            Self::Off => gaia_payload::WV_OFF,
            Self::AncHigh => gaia_payload::WV_ANC_HIGH,
            Self::AncLow => gaia_payload::WV_ANC_LOW,
            Self::AmbientVoice => gaia_payload::WV_AMBIENT_VOICE,
            Self::AmbientAwareness => gaia_payload::WV_AMBIENT_AWARENESS,
            Self::AncAdaptive => gaia_payload::WV_ANC_ADAPTIVE,
        }
    }

    /// Parse a response byte into an ANC mode.
    pub fn from_response(b: u8) -> Option<Self> {
        match b {
            gaia_response::WV_ALL_OFF => Some(Self::Off),
            gaia_response::WV_ANC_HIGH => Some(Self::AncHigh),
            gaia_response::WV_ANC_LOW => Some(Self::AncLow),
            gaia_response::WV_AMBIENT_VOICE => Some(Self::AmbientVoice),
            gaia_response::WV_AMBIENT_AWARENESS => Some(Self::AmbientAwareness),
            gaia_response::WV_ANC_ADAPTIVE => Some(Self::AncAdaptive),
            _ => None,
        }
    }

    /// Parse a button press event response byte.
    pub fn from_button_press(b: u8) -> Option<Self> {
        match b {
            gaia_response::BTN_WV_OFF => Some(Self::Off),
            gaia_response::BTN_WV_ANC_MAX => Some(Self::AncHigh),
            gaia_response::BTN_WV_AMBIENT_AWARENESS => Some(Self::AmbientAwareness),
            _ => None,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::AncHigh => "ANC High",
            Self::AncLow => "ANC Low",
            Self::AmbientVoice => "Ambient (Voice)",
            Self::AmbientAwareness => "Ambient (Awareness)",
            Self::AncAdaptive => "ANC Adaptive",
        }
    }
}

impl std::fmt::Display for GaiaAncMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Auto-off timer setting.
///
/// Maps to the payload values for command `0x12` (AutoOff).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GaiaAutoOff {
    /// Auto-off disabled.
    Never,
    /// Auto-off after 30 minutes.
    ThirtyMinutes,
    /// Auto-off after 1 hour.
    OneHour,
    /// Auto-off after 3 hours.
    ThreeHours,
}

impl GaiaAutoOff {
    /// Convert to the wire payload byte for sending.
    pub fn to_payload(self) -> u8 {
        match self {
            Self::Never => gaia_payload::AUTO_OFF_NEVER,
            Self::ThirtyMinutes => gaia_payload::AUTO_OFF_30_MIN,
            Self::OneHour => gaia_payload::AUTO_OFF_1_HOUR,
            Self::ThreeHours => gaia_payload::AUTO_OFF_3_HOURS,
        }
    }

    /// Parse a response byte into an auto-off setting.
    pub fn from_response(b: u8) -> Option<Self> {
        match b {
            gaia_response::AUTO_OFF_NEVER => Some(Self::Never),
            gaia_response::AUTO_OFF_30_MIN => Some(Self::ThirtyMinutes),
            gaia_response::AUTO_OFF_1_HOUR => Some(Self::OneHour),
            gaia_response::AUTO_OFF_3_HOURS => Some(Self::ThreeHours),
            _ => None,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Never => "Never",
            Self::ThirtyMinutes => "30 minutes",
            Self::OneHour => "1 hour",
            Self::ThreeHours => "3 hours",
        }
    }
}

impl std::fmt::Display for GaiaAutoOff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// EQ preset selector.
///
/// The M&D Connect app supports 5 presets plus a user-customizable preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GaiaEqPreset {
    /// Preset 1.
    Eq1,
    /// Preset 2.
    Eq2,
    /// Preset 3.
    Eq3,
    /// Preset 4.
    Eq4,
    /// Preset 5.
    Eq5,
    /// User-defined custom EQ.
    Custom,
}

// ── BLE Command Builder ─────────────────────────────────────────────────────

/// Build a 5-byte GAIA BLE command from a command ID and payload byte.
///
/// The wire format is:
/// ```text
/// [0x09, 0x9A, 0x03, command_id, payload]
/// ```
///
/// Where `0x09, 0x9A` encodes the M&D vendor ID `0x099A`.
///
/// # Example
///
/// ```
/// use mw75::protocol::{build_gaia_command, GaiaCommand, GAIA_QUERY};
/// let cmd = build_gaia_command(GaiaCommand::GetBatteryLevel, GAIA_QUERY);
/// assert_eq!(cmd, [0x09, 0x9A, 0x03, 0x14, 0xFF]);
/// ```
pub const fn build_gaia_command(command: GaiaCommand, payload: u8) -> [u8; 5] {
    [0x09, 0x9A, 0x03, command as u8, payload]
}

/// Build a variable-length GAIA BLE command with a multi-byte payload.
///
/// Used for commands like [`GaiaCommand::DeviceName`] where the payload
/// is longer than 1 byte.
///
/// # Example
///
/// ```
/// use mw75::protocol::{build_gaia_command_bytes, GaiaCommand};
/// let name = b"MW75 Neuro";
/// let mut payload = vec![name.len() as u8];
/// payload.extend_from_slice(name);
/// let cmd = build_gaia_command_bytes(GaiaCommand::DeviceName, &payload);
/// assert_eq!(cmd[0], 0x09);
/// assert_eq!(cmd[3], 0x15); // DeviceName command ID
/// ```
pub fn build_gaia_command_bytes(command: GaiaCommand, payload: &[u8]) -> Vec<u8> {
    let mut cmd = vec![0x09, 0x9A, 0x03, command as u8];
    cmd.extend_from_slice(payload);
    cmd
}

// ══════════════════════════════════════════════════════════════════════════════
//  MW75 Neuro EEG-Specific BLE Service
// ══════════════════════════════════════════════════════════════════════════════

// ── BLE Service & Characteristics ────────────────────────────────────────────

/// Primary GATT service UUID advertised by MW75 Neuro devices.
///
/// Used as a scan filter to identify MW75 headphones among nearby BLE peripherals.
/// Part of the vendor namespace `000011XX-d102-11e1-9b23-00025b00a5a5`.
pub const MW75_SERVICE_UUID: Uuid =
    Uuid::from_u128(0x00001100_d102_11e1_9b23_00025b00a5a5);

/// Command characteristic — the host writes activation commands here.
///
/// Commands are fixed-length byte sequences (typically 5 bytes) that control
/// EEG mode, raw mode, and battery queries.
pub const MW75_COMMAND_CHAR: Uuid =
    Uuid::from_u128(0x00001101_d102_11e1_9b23_00025b00a5a5);

/// Status characteristic — the device sends activation responses here.
///
/// Subscribe to notifications on this characteristic to receive confirmation
/// of command execution (EEG enabled, raw mode enabled, battery level, etc.).
pub const MW75_STATUS_CHAR: Uuid =
    Uuid::from_u128(0x00001102_d102_11e1_9b23_00025b00a5a5);

/// Data characteristic — the device may stream EEG data here over BLE.
///
pub const MW75_DATA_CHAR: Uuid =
    Uuid::from_u128(0x00001103_d102_11e1_9b23_00025b00a5a5);

/// Secondary status characteristic (alternate namespace).
///
/// Some firmware versions use this for status notifications.
pub const MW75_STATUS_CHAR_ALT: Uuid =
    Uuid::from_u128(0x00001105_d102_11e1_9b23_00025b00a5a6);

/// Secondary data characteristic (alternate namespace).
pub const MW75_DATA_CHAR_ALT: Uuid =
    Uuid::from_u128(0x00001107_d102_11e1_9b23_00025b00a5a5);

/// SPP (Serial Port Profile) UUID for RFCOMM data streaming.
///
/// Standard Bluetooth SPP UUID used as an alternative to the custom UUIDs.
pub const SPP_UUID: Uuid =
    Uuid::from_u128(0x00001101_0000_1000_8000_00805F9B34FB);

/// Custom SPP UUID used by the MW75 Neuro for RFCOMM data streaming.
///
/// Part of the vendor namespace `000011XX-D102-11E1-9B23-00025B00A5A5`.
pub const MW75_SPP_UUID: Uuid =
    Uuid::from_u128(0x00001101_d102_11e1_9b23_00025b00a5a5);

// ── Pre-built BLE Command Sequences ─────────────────────────────────────────

/// Enable EEG streaming mode on the MW75.
///
/// Wire format: `[0x09, 0x9A, 0x03, 0x60, 0x01]`
/// GAIA: vendor=0x099A, command=EegMode(0x60), payload=enable(0x01)
pub const ENABLE_EEG_CMD: [u8; 5] = build_gaia_command(GaiaCommand::EegMode, 0x01);

/// Disable EEG streaming mode on the MW75.
///
/// Wire format: `[0x09, 0x9A, 0x03, 0x60, 0x00]`
/// GAIA: vendor=0x099A, command=EegMode(0x60), payload=disable(0x00)
pub const DISABLE_EEG_CMD: [u8; 5] = build_gaia_command(GaiaCommand::EegMode, 0x00);

/// Enable raw data mode on the MW75.
///
/// Wire format: `[0x09, 0x9A, 0x03, 0x41, 0x01]`
/// GAIA: vendor=0x099A, command=RawMode(0x41), payload=enable(0x01)
pub const ENABLE_RAW_MODE_CMD: [u8; 5] = build_gaia_command(GaiaCommand::RawMode, 0x01);

/// Disable raw data mode on the MW75.
///
/// Wire format: `[0x09, 0x9A, 0x03, 0x41, 0x00]`
/// GAIA: vendor=0x099A, command=RawMode(0x41), payload=disable(0x00)
pub const DISABLE_RAW_MODE_CMD: [u8; 5] = build_gaia_command(GaiaCommand::RawMode, 0x00);

/// Query battery level.
///
/// Wire format: `[0x09, 0x9A, 0x03, 0x14, 0xFF]`
/// GAIA: vendor=0x099A, command=GetBatteryLevel(0x14), payload=query(0xFF)
pub const BATTERY_CMD: [u8; 5] = build_gaia_command(GaiaCommand::GetBatteryLevel, GAIA_QUERY);

/// Query ANC / World Volume state.
///
/// Wire format: `[0x09, 0x9A, 0x03, 0x11, 0xFF]`
pub const GET_WORLD_VOLUME_CMD: [u8; 5] =
    build_gaia_command(GaiaCommand::WorldVolume, GAIA_QUERY);

/// Query auto-off timer setting.
///
/// Wire format: `[0x09, 0x9A, 0x03, 0x12, 0xFF]`
pub const GET_AUTO_OFF_CMD: [u8; 5] =
    build_gaia_command(GaiaCommand::AutoOff, GAIA_QUERY);

/// Query in-ear detection setting.
///
/// Wire format: `[0x09, 0x9A, 0x03, 0x13, 0xFF]`
pub const GET_IN_EAR_DETECTION_CMD: [u8; 5] =
    build_gaia_command(GaiaCommand::InEarDetection, GAIA_QUERY);

/// Query find-device tone setting.
///
/// Wire format: `[0x09, 0x9A, 0x03, 0x17, 0xFF]`
pub const GET_FIND_DEVICE_CMD: [u8; 5] =
    build_gaia_command(GaiaCommand::FindDevice, GAIA_QUERY);

/// Query product ID.
///
/// Wire format: `[0x09, 0x9A, 0x03, 0x10, 0xFF]`
pub const GET_PRODUCT_ID_CMD: [u8; 5] =
    build_gaia_command(GaiaCommand::ProductId, GAIA_QUERY);

/// Register for battery level notifications.
///
/// Wire format: `[0x09, 0x9A, 0x03, 0x06, 0x01]`
pub const REGISTER_BATTERY_CMD: [u8; 5] =
    build_gaia_command(GaiaCommand::RegisterBattery, gaia_payload::REGISTER_BATTERY);

/// "Fetch all" sequence — queries sent by the M&D Connect app on connection.
///
/// The app sends these four queries immediately after connecting:
/// 1. World Volume (ANC state)
/// 2. Auto-Off timer
/// 3. In-Ear Detection
/// 4. Find Device tone
pub const FETCH_ALL_COMMANDS: [[u8; 5]; 4] = [
    GET_WORLD_VOLUME_CMD,
    GET_AUTO_OFF_CMD,
    GET_IN_EAR_DETECTION_CMD,
    GET_FIND_DEVICE_CMD,
];

// ══════════════════════════════════════════════════════════════════════════════
//  EEG Streaming Protocol Constants
// ══════════════════════════════════════════════════════════════════════════════

/// EEG event ID found in byte[1] of data packets.
pub const EEG_EVENT_ID: u8 = 239;

/// Total size of one MW75 data packet in bytes.
///
/// ```text
/// [sync(1)] [event_id(1)] [data_len(1)] [counter(1)] [ref(4)] [drl(4)]
/// [ch1..ch12(48)] [feature_status(1)] [checksum(2)] = 63 bytes
/// ```
pub const PACKET_SIZE: usize = 63;

/// Sync byte that marks the start of every MW75 data packet.
pub const SYNC_BYTE: u8 = 0xAA;

/// EEG ADC-to-microvolt scaling factor.
///
/// `µV = raw_adc_float × EEG_SCALING_FACTOR`
pub const EEG_SCALING_FACTOR: f32 = 0.023842;

/// Sentinel value indicating an invalid or saturated ADC reading.
pub const SENTINEL_VALUE: i32 = 8388607;

/// Number of EEG channels per packet.
pub const NUM_EEG_CHANNELS: usize = 12;

/// RFCOMM channel number used for data streaming.
pub const RFCOMM_CHANNEL: u8 = 25;

// ── Sample Rate ──────────────────────────────────────────────────────────────

/// EEG streaming sample rate.
///
/// The MW75 Neuro supports two sample rates:
///
/// * **500 Hz** — raw mode enabled (sends raw ADC data via RFCOMM).
///   Activation sends both `ENABLE_EEG` and `ENABLE_RAW_MODE`.
/// * **256 Hz** — raw mode disabled (sends processed data via RFCOMM).
///   Activation sends only `ENABLE_EEG` (no raw mode command).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleRate {
    /// 256 Hz — processed EEG data (raw mode disabled).
    Hz256,
    /// 500 Hz — raw ADC data (raw mode enabled).
    Hz500,
}

impl SampleRate {
    /// The sample rate as a numeric frequency in Hz.
    pub fn hz(self) -> f64 {
        match self {
            Self::Hz256 => 256.0,
            Self::Hz500 => 500.0,
        }
    }

    /// Human-readable label (e.g. `"256 Hz"`, `"500 Hz"`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Hz256 => "256 Hz",
            Self::Hz500 => "500 Hz",
        }
    }

    /// Whether raw mode should be enabled for this sample rate.
    pub fn needs_raw_mode(self) -> bool {
        match self {
            Self::Hz256 => false,
            Self::Hz500 => true,
        }
    }

    /// Interval between samples in microseconds.
    pub fn interval_micros(self) -> u64 {
        match self {
            Self::Hz256 => 3906, // 1_000_000 / 256 ≈ 3906
            Self::Hz500 => 2000, // 1_000_000 / 500 = 2000
        }
    }
}

impl Default for SampleRate {
    /// Default sample rate is 500 Hz (raw mode) for backward compatibility.
    fn default() -> Self {
        Self::Hz500
    }
}

impl std::fmt::Display for SampleRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ── Timing Constants ─────────────────────────────────────────────────────────

/// Delay after sending ENABLE_EEG command (milliseconds).
pub const BLE_ACTIVATION_DELAY_MS: u64 = 100;

/// Delay between BLE commands (milliseconds).
pub const BLE_COMMAND_DELAY_MS: u64 = 500;

/// BLE discovery scan timeout (seconds).
pub const BLE_DISCOVERY_TIMEOUT_SECS: u64 = 4;

/// Seconds without data before declaring the connection lost.
pub const DATA_PACKET_TIMEOUT_SECS: f64 = 8.0;

// ── BLE Response Codes ───────────────────────────────────────────────────────

/// Success response code from the MW75 status characteristic.
///
/// Most commands return `0xF1` for success. This is also
/// [`gaia_response::SET_SUCCESS`].
pub const BLE_SUCCESS_CODE: u8 = gaia_response::SET_SUCCESS;

/// Failure/alternative response code.
///
/// Raw mode may return `0xF0` when the mode change is still pending
/// (timing-dependent). This is also [`gaia_response::SET_FAILED`].
pub const BLE_SUCCESS_CODE_ALT: u8 = gaia_response::SET_FAILED;

/// Command type byte for EEG enable/disable responses.
pub const BLE_EEG_COMMAND: u8 = GaiaCommand::EegMode as u8;

/// Command type byte for raw mode enable/disable responses.
pub const BLE_RAW_MODE_COMMAND: u8 = GaiaCommand::RawMode as u8;

/// Command type byte for battery query responses.
pub const BLE_BATTERY_COMMAND: u8 = GaiaCommand::GetBatteryLevel as u8;

/// Unknown command byte sometimes seen in responses.
pub const BLE_UNKNOWN_E0_COMMAND: u8 = GaiaCommand::UnknownE0 as u8;

/// RFCOMM connection status byte. The device sends `[09 9a 03 88 XX]`
/// periodically after activation, where XX indicates RFCOMM state:
/// `0x00` = not connected, `0x01` = connected (expected).
pub const BLE_RFCOMM_STATUS_COMMAND: u8 = GaiaCommand::RfcommStatus as u8;

// ── Device Discovery ─────────────────────────────────────────────────────────

/// Device name pattern used to identify MW75 headphones during BLE scanning.
///
/// Any device whose advertised name contains this substring (case-insensitive)
/// is considered a candidate MW75 device.
pub const MW75_DEVICE_NAME_PATTERN: &str = "MW75";

// ── Supported device models ──────────────────────────────────────────────────

/// M&D device models supported by the GAIA protocol.
///
/// The M&D Connect app identifies these via BLE advertisement data and
/// product ID queries.  The `MW08Plugin` GAIA protocol is shared across
/// all models, with the MW75 Neuro adding EEG-specific commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MdDeviceModel {
    /// MW75 Neuro — over-ear headphones with EEG.
    Mw75Neuro,
    /// MW08 — true wireless earbuds.
    Mw08,
    /// MW08 Sport — true wireless earbuds (sport variant).
    Mw08Sport,
    /// MW08 Sport (internal variant).
    Mw08SportInternal,
    /// MW09 — true wireless earbuds.
    Mw09,
    /// MG20 — gaming headset.
    Mg20,
    /// MH40 Wireless — on-ear headphones.
    Mh40Wireless,
    /// MA880 — true wireless earbuds.
    Ma880,
}

// ── Human-readable labels ─────────────────────────────────────────────────────

/// EEG channel labels in packet order (Ch1–Ch12).
pub const EEG_CHANNEL_NAMES: [&str; 12] = [
    "Ch1", "Ch2", "Ch3", "Ch4", "Ch5", "Ch6",
    "Ch7", "Ch8", "Ch9", "Ch10", "Ch11", "Ch12",
];

// ══════════════════════════════════════════════════════════════════════════════
//  GAIA Response Parser
// ══════════════════════════════════════════════════════════════════════════════

/// A parsed GAIA response/notification received from the device.
///
/// Created by [`parse_gaia_response`] from raw BLE notification bytes.
#[derive(Debug, Clone)]
pub struct GaiaResponse {
    /// The command this response is for.
    pub command: GaiaCommand,
    /// The raw payload bytes (after the command byte).
    pub payload: Vec<u8>,
}

/// Parse a BLE notification/response into a structured [`GaiaResponse`].
///
/// Expects the standard 5-byte format: `[0x09, 0x9A, 0x03, cmd, payload]`
/// or longer for multi-byte payloads.
///
/// Returns `None` if the data is too short or has an unrecognized command byte.
///
/// # Example
///
/// ```
/// use mw75::protocol::{parse_gaia_response, GaiaCommand};
/// let data = [0x09, 0x9A, 0x03, 0x14, 0x55]; // battery = 85%
/// let resp = parse_gaia_response(&data).unwrap();
/// assert_eq!(resp.command, GaiaCommand::GetBatteryLevel);
/// assert_eq!(resp.payload, &[0x55]);
/// ```
pub fn parse_gaia_response(data: &[u8]) -> Option<GaiaResponse> {
    if data.len() < 5 {
        return None;
    }
    // Verify vendor prefix
    if data[0] != 0x09 || data[1] != 0x9A || data[2] != 0x03 {
        return None;
    }
    let command = GaiaCommand::from_byte(data[3])?;
    let payload = data[4..].to_vec();
    Some(GaiaResponse { command, payload })
}

// ══════════════════════════════════════════════════════════════════════════════
//  Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── UUID tests ───────────────────────────────────────────────────────────

    #[test]
    fn service_uuid_format() {
        let s = MW75_SERVICE_UUID.to_string();
        assert!(s.contains("00001100"), "UUID should contain service base: {s}");
    }

    #[test]
    fn command_char_uuid_format() {
        let s = MW75_COMMAND_CHAR.to_string();
        assert!(s.contains("00001101"));
    }

    #[test]
    fn status_char_uuid_format() {
        let s = MW75_STATUS_CHAR.to_string();
        assert!(s.contains("00001102"));
    }

    #[test]
    fn uuids_are_distinct() {
        assert_ne!(MW75_SERVICE_UUID, MW75_COMMAND_CHAR);
        assert_ne!(MW75_SERVICE_UUID, MW75_STATUS_CHAR);
        assert_ne!(MW75_COMMAND_CHAR, MW75_STATUS_CHAR);
    }

    // ── GAIA vendor ID tests ────────────────────────────────────────────────

    #[test]
    fn gaia_vendor_id_values() {
        assert_eq!(GAIA_VENDOR_QTIL_V1V2, 0x000A);
        assert_eq!(GAIA_VENDOR_QTIL_V3, 0x001D);
        assert_eq!(GAIA_VENDOR_MD, 0x099A);
    }

    #[test]
    fn gaia_vendor_md_matches_command_prefix() {
        // 0x099A splits into bytes 0x09 and 0x9A — the first two bytes of all commands
        assert_eq!((GAIA_VENDOR_MD >> 8) as u8, 0x09);
        assert_eq!((GAIA_VENDOR_MD & 0xFF) as u8, 0x9A);
    }

    // ── Command builder tests ───────────────────────────────────────────────

    #[test]
    fn build_command_basic() {
        let cmd = build_gaia_command(GaiaCommand::GetBatteryLevel, GAIA_QUERY);
        assert_eq!(cmd, [0x09, 0x9A, 0x03, 0x14, 0xFF]);
    }

    #[test]
    fn build_command_matches_prebuilt() {
        // Verify all pre-built constants match the builder
        assert_eq!(ENABLE_EEG_CMD, build_gaia_command(GaiaCommand::EegMode, 0x01));
        assert_eq!(DISABLE_EEG_CMD, build_gaia_command(GaiaCommand::EegMode, 0x00));
        assert_eq!(ENABLE_RAW_MODE_CMD, build_gaia_command(GaiaCommand::RawMode, 0x01));
        assert_eq!(DISABLE_RAW_MODE_CMD, build_gaia_command(GaiaCommand::RawMode, 0x00));
        assert_eq!(BATTERY_CMD, build_gaia_command(GaiaCommand::GetBatteryLevel, GAIA_QUERY));
    }

    #[test]
    fn build_command_bytes_variable_length() {
        let payload = b"TestName";
        let mut expected_payload = vec![payload.len() as u8];
        expected_payload.extend_from_slice(payload);
        let cmd = build_gaia_command_bytes(GaiaCommand::DeviceName, &expected_payload);
        assert_eq!(cmd[0], 0x09);
        assert_eq!(cmd[1], 0x9A);
        assert_eq!(cmd[2], 0x03);
        assert_eq!(cmd[3], GaiaCommand::DeviceName as u8);
        assert_eq!(cmd[4], payload.len() as u8);
        assert_eq!(&cmd[5..], payload);
    }

    // ── Pre-built command structure tests ────────────────────────────────────

    #[test]
    fn enable_eeg_cmd_structure() {
        assert_eq!(ENABLE_EEG_CMD.len(), 5);
        assert_eq!(ENABLE_EEG_CMD[0], 0x09);
        assert_eq!(ENABLE_EEG_CMD[3], 0x60); // EEG command type
        assert_eq!(ENABLE_EEG_CMD[4], 0x01); // enable
    }

    #[test]
    fn disable_eeg_cmd_structure() {
        assert_eq!(DISABLE_EEG_CMD.len(), 5);
        assert_eq!(DISABLE_EEG_CMD[3], 0x60);
        assert_eq!(DISABLE_EEG_CMD[4], 0x00); // disable
    }

    #[test]
    fn enable_disable_eeg_differ_only_in_last_byte() {
        assert_eq!(ENABLE_EEG_CMD[..4], DISABLE_EEG_CMD[..4]);
        assert_ne!(ENABLE_EEG_CMD[4], DISABLE_EEG_CMD[4]);
    }

    #[test]
    fn enable_raw_mode_cmd_structure() {
        assert_eq!(ENABLE_RAW_MODE_CMD.len(), 5);
        assert_eq!(ENABLE_RAW_MODE_CMD[3], 0x41); // raw mode command type
        assert_eq!(ENABLE_RAW_MODE_CMD[4], 0x01);
    }

    #[test]
    fn disable_raw_mode_cmd_structure() {
        assert_eq!(DISABLE_RAW_MODE_CMD[3], 0x41);
        assert_eq!(DISABLE_RAW_MODE_CMD[4], 0x00);
    }

    #[test]
    fn battery_cmd_structure() {
        assert_eq!(BATTERY_CMD.len(), 5);
        assert_eq!(BATTERY_CMD[3], 0x14); // battery command type
        assert_eq!(BATTERY_CMD[4], 0xFF);
    }

    #[test]
    fn all_commands_share_prefix() {
        for cmd in [
            &ENABLE_EEG_CMD,
            &DISABLE_EEG_CMD,
            &ENABLE_RAW_MODE_CMD,
            &DISABLE_RAW_MODE_CMD,
            &BATTERY_CMD,
            &GET_WORLD_VOLUME_CMD,
            &GET_AUTO_OFF_CMD,
            &GET_IN_EAR_DETECTION_CMD,
            &GET_FIND_DEVICE_CMD,
            &GET_PRODUCT_ID_CMD,
            &REGISTER_BATTERY_CMD,
        ] {
            assert_eq!(cmd[0], 0x09, "Wrong prefix byte 0");
            assert_eq!(cmd[1], 0x9A, "Wrong prefix byte 1");
            assert_eq!(cmd[2], 0x03, "Wrong prefix byte 2");
        }
    }

    #[test]
    fn fetch_all_has_four_commands() {
        assert_eq!(FETCH_ALL_COMMANDS.len(), 4);
        assert_eq!(FETCH_ALL_COMMANDS[0], GET_WORLD_VOLUME_CMD);
        assert_eq!(FETCH_ALL_COMMANDS[1], GET_AUTO_OFF_CMD);
        assert_eq!(FETCH_ALL_COMMANDS[2], GET_IN_EAR_DETECTION_CMD);
        assert_eq!(FETCH_ALL_COMMANDS[3], GET_FIND_DEVICE_CMD);
    }

    // ── GaiaCommand enum tests ──────────────────────────────────────────────

    #[test]
    fn gaia_command_from_byte_known() {
        assert_eq!(GaiaCommand::from_byte(0x11), Some(GaiaCommand::WorldVolume));
        assert_eq!(GaiaCommand::from_byte(0x14), Some(GaiaCommand::GetBatteryLevel));
        assert_eq!(GaiaCommand::from_byte(0x60), Some(GaiaCommand::EegMode));
        assert_eq!(GaiaCommand::from_byte(0x91), Some(GaiaCommand::VolumeButtonPress));
    }

    #[test]
    fn gaia_command_from_byte_unknown() {
        assert_eq!(GaiaCommand::from_byte(0x00), None);
        assert_eq!(GaiaCommand::from_byte(0x99), None);
        assert_eq!(GaiaCommand::from_byte(0xFE), None);
    }

    #[test]
    fn gaia_command_roundtrip() {
        let commands = [
            GaiaCommand::RegisterBattery,
            GaiaCommand::ProductId,
            GaiaCommand::WorldVolume,
            GaiaCommand::AutoOff,
            GaiaCommand::InEarDetection,
            GaiaCommand::GetBatteryLevel,
            GaiaCommand::DeviceName,
            GaiaCommand::FindDevice,
            GaiaCommand::EegMode,
            GaiaCommand::RawMode,
            GaiaCommand::RfcommStatus,
            GaiaCommand::BatteryNotification82,
            GaiaCommand::VolumeButtonPress,
            GaiaCommand::UnknownE0,
            GaiaCommand::BatteryNotification,
        ];
        for cmd in commands {
            let byte = cmd as u8;
            let parsed = GaiaCommand::from_byte(byte).unwrap();
            assert_eq!(parsed, cmd);
        }
    }

    // ── ANC mode tests ──────────────────────────────────────────────────────

    #[test]
    fn anc_mode_payload_roundtrip() {
        let modes = [
            GaiaAncMode::Off,
            GaiaAncMode::AncHigh,
            GaiaAncMode::AncLow,
            GaiaAncMode::AmbientVoice,
            GaiaAncMode::AmbientAwareness,
            GaiaAncMode::AncAdaptive,
        ];
        for mode in modes {
            let payload = mode.to_payload();
            let parsed = GaiaAncMode::from_response(payload).unwrap();
            assert_eq!(parsed, mode, "Roundtrip failed for {:?}", mode);
        }
    }

    #[test]
    fn anc_mode_from_button_press() {
        assert_eq!(
            GaiaAncMode::from_button_press(gaia_response::BTN_WV_OFF),
            Some(GaiaAncMode::Off)
        );
        assert_eq!(
            GaiaAncMode::from_button_press(gaia_response::BTN_WV_ANC_MAX),
            Some(GaiaAncMode::AncHigh)
        );
        assert_eq!(
            GaiaAncMode::from_button_press(gaia_response::BTN_WV_AMBIENT_AWARENESS),
            Some(GaiaAncMode::AmbientAwareness)
        );
        assert_eq!(GaiaAncMode::from_button_press(0x99), None);
    }

    #[test]
    fn anc_mode_display() {
        assert_eq!(GaiaAncMode::Off.to_string(), "Off");
        assert_eq!(GaiaAncMode::AncHigh.to_string(), "ANC High");
        assert_eq!(GaiaAncMode::AncAdaptive.to_string(), "ANC Adaptive");
        assert_eq!(GaiaAncMode::AmbientAwareness.to_string(), "Ambient (Awareness)");
    }

    // ── Auto-off tests ──────────────────────────────────────────────────────

    #[test]
    fn auto_off_payload_roundtrip() {
        let values = [
            GaiaAutoOff::Never,
            GaiaAutoOff::ThirtyMinutes,
            GaiaAutoOff::OneHour,
            GaiaAutoOff::ThreeHours,
        ];
        for val in values {
            let payload = val.to_payload();
            let parsed = GaiaAutoOff::from_response(payload).unwrap();
            assert_eq!(parsed, val);
        }
    }

    #[test]
    fn auto_off_display() {
        assert_eq!(GaiaAutoOff::Never.to_string(), "Never");
        assert_eq!(GaiaAutoOff::ThirtyMinutes.to_string(), "30 minutes");
        assert_eq!(GaiaAutoOff::OneHour.to_string(), "1 hour");
        assert_eq!(GaiaAutoOff::ThreeHours.to_string(), "3 hours");
    }

    // ── Response parser tests ───────────────────────────────────────────────

    #[test]
    fn parse_battery_response() {
        let data = [0x09, 0x9A, 0x03, 0x14, 85];
        let resp = parse_gaia_response(&data).unwrap();
        assert_eq!(resp.command, GaiaCommand::GetBatteryLevel);
        assert_eq!(resp.payload, &[85]);
    }

    #[test]
    fn parse_anc_response() {
        let data = [0x09, 0x9A, 0x03, 0x11, 0x02]; // ANC High
        let resp = parse_gaia_response(&data).unwrap();
        assert_eq!(resp.command, GaiaCommand::WorldVolume);
        let mode = GaiaAncMode::from_response(resp.payload[0]).unwrap();
        assert_eq!(mode, GaiaAncMode::AncHigh);
    }

    #[test]
    fn parse_auto_off_response() {
        let data = [0x09, 0x9A, 0x03, 0x12, 0x03]; // 1 hour
        let resp = parse_gaia_response(&data).unwrap();
        assert_eq!(resp.command, GaiaCommand::AutoOff);
        let auto_off = GaiaAutoOff::from_response(resp.payload[0]).unwrap();
        assert_eq!(auto_off, GaiaAutoOff::OneHour);
    }

    #[test]
    fn parse_success_response() {
        let data = [0x09, 0x9A, 0x03, 0x11, 0xF1]; // WorldVolume set success
        let resp = parse_gaia_response(&data).unwrap();
        assert_eq!(resp.payload[0], gaia_response::SET_SUCCESS);
    }

    #[test]
    fn parse_failure_response() {
        let data = [0x09, 0x9A, 0x03, 0x11, 0xF0]; // WorldVolume set failed
        let resp = parse_gaia_response(&data).unwrap();
        assert_eq!(resp.payload[0], gaia_response::SET_FAILED);
    }

    #[test]
    fn parse_rejects_short_data() {
        assert!(parse_gaia_response(&[0x09, 0x9A, 0x03]).is_none());
        assert!(parse_gaia_response(&[]).is_none());
    }

    #[test]
    fn parse_rejects_wrong_prefix() {
        let data = [0xFF, 0xFF, 0xFF, 0x14, 85];
        assert!(parse_gaia_response(&data).is_none());
    }

    #[test]
    fn parse_rejects_unknown_command() {
        let data = [0x09, 0x9A, 0x03, 0x99, 0xFF];
        assert!(parse_gaia_response(&data).is_none());
    }

    #[test]
    fn parse_multi_byte_payload() {
        let data = [0x09, 0x9A, 0x03, 0x10, 0xAB, 0xCD]; // product ID = [0xAB, 0xCD]
        let resp = parse_gaia_response(&data).unwrap();
        assert_eq!(resp.command, GaiaCommand::ProductId);
        assert_eq!(resp.payload, &[0xAB, 0xCD]);
    }

    // ── Protocol constant tests ─────────────────────────────────────────────

    #[test]
    fn protocol_constants() {
        assert_eq!(EEG_EVENT_ID, 239);
        assert_eq!(PACKET_SIZE, 63);
        assert_eq!(SYNC_BYTE, 0xAA);
        assert_eq!(NUM_EEG_CHANNELS, 12);
        assert_eq!(RFCOMM_CHANNEL, 25);
    }

    #[test]
    fn scaling_factor_reasonable() {
        let uv = EEG_SCALING_FACTOR * 1000.0;
        assert!(uv > 20.0 && uv < 30.0, "Unexpected µV: {uv}");
    }

    #[test]
    fn channel_names_count() {
        assert_eq!(EEG_CHANNEL_NAMES.len(), NUM_EEG_CHANNELS);
    }

    #[test]
    fn channel_names_format() {
        for (i, name) in EEG_CHANNEL_NAMES.iter().enumerate() {
            let expected = format!("Ch{}", i + 1);
            assert_eq!(*name, expected.as_str());
        }
    }

    #[test]
    fn response_codes_match_gaia() {
        assert_eq!(BLE_SUCCESS_CODE, gaia_response::SET_SUCCESS);
        assert_eq!(BLE_SUCCESS_CODE_ALT, gaia_response::SET_FAILED);
        assert_eq!(BLE_EEG_COMMAND, GaiaCommand::EegMode as u8);
        assert_eq!(BLE_RAW_MODE_COMMAND, GaiaCommand::RawMode as u8);
        assert_eq!(BLE_BATTERY_COMMAND, GaiaCommand::GetBatteryLevel as u8);
        assert_eq!(BLE_RFCOMM_STATUS_COMMAND, GaiaCommand::RfcommStatus as u8);
    }

    #[test]
    fn sentinel_value() {
        assert_eq!(SENTINEL_VALUE, 8388607);
        assert_eq!(SENTINEL_VALUE, (1 << 23) - 1);
    }

    // ── Payload value consistency tests ─────────────────────────────────────

    #[test]
    fn payload_and_response_values_match() {
        // For most settings, the payload to SET a value is the same byte
        // as the response confirming that value
        assert_eq!(gaia_payload::WV_OFF, gaia_response::WV_ALL_OFF);
        assert_eq!(gaia_payload::WV_ANC_HIGH, gaia_response::WV_ANC_HIGH);
        assert_eq!(gaia_payload::WV_ANC_LOW, gaia_response::WV_ANC_LOW);
        assert_eq!(gaia_payload::WV_AMBIENT_VOICE, gaia_response::WV_AMBIENT_VOICE);
        assert_eq!(gaia_payload::WV_AMBIENT_AWARENESS, gaia_response::WV_AMBIENT_AWARENESS);
        assert_eq!(gaia_payload::WV_ANC_ADAPTIVE, gaia_response::WV_ANC_ADAPTIVE);

        assert_eq!(gaia_payload::AUTO_OFF_NEVER, gaia_response::AUTO_OFF_NEVER);
        assert_eq!(gaia_payload::AUTO_OFF_30_MIN, gaia_response::AUTO_OFF_30_MIN);
        assert_eq!(gaia_payload::AUTO_OFF_1_HOUR, gaia_response::AUTO_OFF_1_HOUR);
        assert_eq!(gaia_payload::AUTO_OFF_3_HOURS, gaia_response::AUTO_OFF_3_HOURS);

        assert_eq!(gaia_payload::IN_EAR_DISABLE, gaia_response::IN_EAR_DISABLE);
        assert_eq!(gaia_payload::IN_EAR_ENABLE, gaia_response::IN_EAR_ENABLE);

        assert_eq!(gaia_payload::FIND_TONE_DISABLE, gaia_response::FIND_TONE_DISABLE);
        assert_eq!(gaia_payload::FIND_TONE_ENABLE, gaia_response::FIND_TONE_ENABLE);
    }

    // ── SampleRate tests ────────────────────────────────────────────────────

    #[test]
    fn sample_rate_hz_values() {
        assert!((SampleRate::Hz256.hz() - 256.0).abs() < f64::EPSILON);
        assert!((SampleRate::Hz500.hz() - 500.0).abs() < f64::EPSILON);
    }

    #[test]
    fn sample_rate_needs_raw_mode() {
        assert!(!SampleRate::Hz256.needs_raw_mode());
        assert!(SampleRate::Hz500.needs_raw_mode());
    }

    #[test]
    fn sample_rate_default_is_500() {
        assert_eq!(SampleRate::default(), SampleRate::Hz500);
    }

    #[test]
    fn sample_rate_display() {
        assert_eq!(SampleRate::Hz256.to_string(), "256 Hz");
        assert_eq!(SampleRate::Hz500.to_string(), "500 Hz");
    }

    #[test]
    fn sample_rate_interval_micros() {
        assert_eq!(SampleRate::Hz500.interval_micros(), 2000);
        assert_eq!(SampleRate::Hz256.interval_micros(), 3906);
    }

    #[test]
    fn query_commands_use_0xff() {
        // All "get" pre-built commands should end with 0xFF
        for cmd in [
            GET_WORLD_VOLUME_CMD,
            GET_AUTO_OFF_CMD,
            GET_IN_EAR_DETECTION_CMD,
            GET_FIND_DEVICE_CMD,
            GET_PRODUCT_ID_CMD,
            BATTERY_CMD,
        ] {
            assert_eq!(cmd[4], GAIA_QUERY, "Query command {:02X} should use 0xFF", cmd[3]);
        }
    }
}
