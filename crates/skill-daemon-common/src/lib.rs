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
    /// Legacy connect target field (historically overloaded with id or display name).
    pub target_name: Option<String>,
    /// Canonical target device id for connection attempts (e.g. "ble:...", "usb:...").
    #[serde(default)]
    pub target_id: Option<String>,
    /// Human-readable target name resolved from paired metadata when available.
    #[serde(default)]
    pub target_display_name: Option<String>,
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

    /// Phone descriptor metadata for iroh-remote sessions.
    #[serde(default)]
    pub phone_info: Option<serde_json::Value>,
    /// Display name of the active iroh client (from pairing/auth store).
    #[serde(default)]
    pub iroh_client_name: Option<String>,
    /// True when at least one iroh device-proxy peer is currently online.
    #[serde(default)]
    pub iroh_tunnel_online: bool,
    /// Number of currently connected iroh peers on the device-proxy ALPN.
    #[serde(default)]
    pub iroh_connected_peers: usize,
    /// True when a remote BLE device is connected on any iroh peer.
    #[serde(default)]
    pub iroh_remote_device_connected: bool,
    /// True when recent sensor chunks are flowing from any iroh peer.
    #[serde(default)]
    pub iroh_streaming_active: bool,
    /// True when recent EEG-bearing chunks are flowing from any iroh peer.
    #[serde(default)]
    pub iroh_eeg_streaming_active: bool,
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
        self.phone_info = None;
        self.iroh_client_name = None;
        self.iroh_tunnel_online = false;
        self.iroh_connected_peers = 0;
        self.iroh_remote_device_connected = false;
        self.iroh_streaming_active = false;
        self.iroh_eeg_streaming_active = false;
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_transport_from_wire_ble_default() {
        assert_eq!(DeviceTransport::from_wire("ble"), DeviceTransport::Ble);
        assert_eq!(DeviceTransport::from_wire("unknown"), DeviceTransport::Ble);
        assert_eq!(DeviceTransport::from_wire(""), DeviceTransport::Ble);
    }

    #[test]
    fn device_transport_from_wire_variants() {
        assert_eq!(DeviceTransport::from_wire("usb_serial"), DeviceTransport::UsbSerial);
        assert_eq!(DeviceTransport::from_wire("wifi"), DeviceTransport::Wifi);
        assert_eq!(DeviceTransport::from_wire("cortex"), DeviceTransport::Cortex);
    }

    #[test]
    fn device_transport_serde_roundtrip() {
        for t in [
            DeviceTransport::Ble,
            DeviceTransport::UsbSerial,
            DeviceTransport::Wifi,
            DeviceTransport::Cortex,
        ] {
            let json = serde_json::to_string(&t).unwrap();
            let back: DeviceTransport = serde_json::from_str(&json).unwrap();
            assert_eq!(t, back);
        }
    }

    #[test]
    fn status_response_clear_device_resets_fields() {
        let mut s = StatusResponse::default();
        s.device_name = Some("Muse".into());
        s.device_kind = "muse".into();
        s.battery = 85.0;
        s.sample_count = 1000;
        s.has_ppg = true;
        s.channel_names = vec!["TP9".into()];

        s.clear_device();

        assert_eq!(s.state, "disconnected");
        assert!(s.device_name.is_none());
        assert!(s.device_kind.is_empty());
        assert_eq!(s.battery, 0.0);
        assert_eq!(s.sample_count, 0);
        assert!(!s.has_ppg);
        assert!(s.channel_names.is_empty());
    }

    #[test]
    fn status_response_serde_default_roundtrip() {
        let s = StatusResponse::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: StatusResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.state, s.state);
        assert_eq!(back.battery, 0.0);
    }

    #[test]
    fn event_envelope_serde_roundtrip() {
        let e = EventEnvelope {
            r#type: "TestEvent".into(),
            ts_unix_ms: 1700000000000,
            correlation_id: Some("abc".into()),
            payload: serde_json::json!({"key": "value"}),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: EventEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.r#type, "TestEvent");
        assert_eq!(back.ts_unix_ms, 1700000000000);
        assert_eq!(back.correlation_id, Some("abc".into()));
        assert_eq!(back.payload["key"], "value");
    }

    #[test]
    fn version_response_serde() {
        let v = VersionResponse {
            daemon: DAEMON_NAME.into(),
            protocol_version: PROTOCOL_VERSION,
            daemon_version: "1.0.0".into(),
        };
        let json = serde_json::to_string(&v).unwrap();
        let back: VersionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.daemon, DAEMON_NAME);
        assert_eq!(back.protocol_version, PROTOCOL_VERSION);
    }

    #[test]
    fn scanner_wifi_config_default() {
        let c = ScannerWifiConfigRequest::default();
        assert!(c.wifi_shield_ip.is_empty());
        assert!(c.galea_ip.is_empty());
    }
}
