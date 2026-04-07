use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const DAEMON_NAME: &str = "skill-daemon";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionResponse {
    pub daemon: String,
    pub protocol_version: u32,
    pub daemon_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub r#type: String,
    pub ts_unix_ms: u64,
    pub correlation_id: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsPortResponse {
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsClient {
    pub peer: String,
    pub connected_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsRequestLog {
    pub timestamp: u64,
    pub peer: String,
    pub command: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceLogEntry {
    pub ts: u64,
    pub tag: String,
    pub msg: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceTransport {
    Ble,
    UsbSerial,
    Wifi,
    Cortex,
}

impl DeviceTransport {
    pub fn from_wire(s: &str) -> Self {
        match s {
            "usb_serial" => Self::UsbSerial,
            "wifi" => Self::Wifi,
            "cortex" => Self::Cortex,
            _ => Self::Ble,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDeviceResponse {
    pub id: String,
    pub name: String,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusResponse {
    pub state: String,
    pub device_name: Option<String>,
    /// Device kind tag matching `DeviceKind::as_str()` (e.g. "muse", "brainbit", "openbci").
    #[serde(default)]
    pub device_kind: String,
    /// Device ID (BLE address, serial path, etc.).
    #[serde(default)]
    pub device_id: Option<String>,
    pub sample_count: u64,
    pub battery: f32,
    pub device_error: Option<String>,
    pub target_name: Option<String>,
    pub retry_attempt: u32,
    pub retry_countdown_secs: u32,
    pub paired_devices: Vec<PairedDeviceResponse>,

    // ── Device descriptor fields (set on connect) ─────────────────────────
    /// CSV recording path for the current session.
    #[serde(default)]
    pub csv_path: Option<String>,
    /// EEG channel labels from the device descriptor.
    #[serde(default)]
    pub channel_names: Vec<String>,
    /// PPG channel labels.
    #[serde(default)]
    pub ppg_channel_names: Vec<String>,
    /// IMU channel labels.
    #[serde(default)]
    pub imu_channel_names: Vec<String>,
    /// fNIRS channel labels.
    #[serde(default)]
    pub fnirs_channel_names: Vec<String>,
    /// Hardware EEG channel count.
    #[serde(default)]
    pub eeg_channel_count: usize,
    /// Hardware EEG sample rate (Hz).
    #[serde(default)]
    pub eeg_sample_rate_hz: f64,
    /// Per-channel signal quality strings ("good", "fair", "poor", "no_signal").
    #[serde(default)]
    pub channel_quality: Vec<String>,

    // ── Device identity (populated by adapters that report it) ────────────
    /// Factory serial number.
    #[serde(default)]
    pub serial_number: Option<String>,
    /// Hardware MAC address.
    #[serde(default)]
    pub mac_address: Option<String>,
    /// Firmware version string.
    #[serde(default)]
    pub firmware_version: Option<String>,
    /// Hardware version / revision.
    #[serde(default)]
    pub hardware_version: Option<String>,

    // ── Capability flags ──────────────────────────────────────────────────
    /// Device has a PPG (heart-rate) sensor.
    #[serde(default)]
    pub has_ppg: bool,
    /// Device has an IMU (accelerometer + gyroscope).
    #[serde(default)]
    pub has_imu: bool,
    /// Device has electrodes at central scalp sites.
    #[serde(default)]
    pub has_central_electrodes: bool,
    /// Device supports a full 10-20 montage.
    #[serde(default)]
    pub has_full_montage: bool,
    /// PPG sample count this session.
    #[serde(default)]
    pub ppg_sample_count: u64,
}

impl StatusResponse {
    /// Reset all device-specific fields when transitioning to disconnected.
    pub fn clear_device(&mut self) {
        self.state = "disconnected".into();
        self.device_name = None;
        self.device_kind.clear();
        self.device_id = None;
        self.device_error = None;
        self.csv_path = None;
        self.channel_names.clear();
        self.ppg_channel_names.clear();
        self.imu_channel_names.clear();
        self.fnirs_channel_names.clear();
        self.eeg_channel_count = 0;
        self.eeg_sample_rate_hz = 0.0;
        self.channel_quality.clear();
        self.serial_number = None;
        self.mac_address = None;
        self.firmware_version = None;
        self.hardware_version = None;
        self.has_ppg = false;
        self.has_imu = false;
        self.has_central_electrodes = false;
        self.has_full_montage = false;
        self.sample_count = 0;
        self.battery = 0.0;
        self.ppg_sample_count = 0;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredDeviceResponse {
    pub id: String,
    pub name: String,
    pub last_seen: u64,
    pub last_rssi: i16,
    pub is_paired: bool,
    pub is_preferred: bool,
    pub transport: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetPreferredDeviceRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairDeviceRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgetDeviceRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionControlRequest {
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerStateResponse {
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerWifiConfigRequest {
    pub wifi_shield_ip: String,
    pub galea_ip: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerCortexConfigRequest {
    pub emotiv_client_id: String,
    pub emotiv_client_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LslDiscoveredStreamResponse {
    pub name: String,
    pub stream_type: String,
    pub channels: usize,
    pub sample_rate: f64,
    pub source_id: String,
    pub hostname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}
