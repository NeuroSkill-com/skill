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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub state: String,
    pub device_name: Option<String>,
    pub sample_count: u64,
    pub battery: f32,
    pub device_error: Option<String>,
    pub target_name: Option<String>,
    pub retry_attempt: u32,
    pub retry_countdown_secs: u32,
    pub paired_devices: Vec<PairedDeviceResponse>,
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
