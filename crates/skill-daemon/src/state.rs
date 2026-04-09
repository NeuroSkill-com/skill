// SPDX-License-Identifier: GPL-3.0-only
//! Shared daemon state type.

/// Type alias for the persistent BLE discovery cache.
/// Maps `"ble:<uuid>"` → `(local_name, rssi, last_seen_unix_secs)`.
pub type BleDeviceCache = Arc<Mutex<HashMap<String, (Option<String>, i16, u64)>>>;

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

use tokio::sync::{broadcast, oneshot};

use skill_daemon_common::{
    DeviceLogEntry, DiscoveredDeviceResponse, EventEnvelope, ScannerCortexConfigRequest, ScannerWifiConfigRequest,
    StatusResponse,
};
use skill_iroh::{IrohPeerMap, IrohRuntimeState};
use skill_iroh::{SharedDeviceEventTx, SharedIrohAuth, SharedIrohRuntime};
use skill_settings::{HookRule, LslPairedStream};

#[cfg(feature = "llm")]
use skill_llm::{LlmConfig, LlmLogBuffer, LlmStateCell};

use crate::tracker::DaemonTracker;
use skill_label_index::LabelIndexState;

/// Shared application state threaded through all axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub auth_token: Arc<Mutex<String>>,
    pub events_tx: broadcast::Sender<EventEnvelope>,
    pub tracker: Arc<Mutex<DaemonTracker>>,
    pub status: Arc<Mutex<StatusResponse>>,
    pub devices: Arc<Mutex<Vec<DiscoveredDeviceResponse>>>,
    pub scanner_running: Arc<Mutex<bool>>,
    /// Persistent BLE discovery cache: ble_id → (local_name, rssi, last_seen_secs).
    /// Updated by the event-driven BLE listener; read each scanner tick.
    pub ble_device_cache: BleDeviceCache,
    /// Set to `true` while a BLE device is actively connecting.
    /// The BLE listener task stops scanning when this is set so that only one
    /// `CBCentralManager` is scanning at a time — having two concurrent scans
    /// on macOS prevents `peripheral.connect()` callbacks from firing.
    pub ble_scan_paused: Arc<std::sync::atomic::AtomicBool>,
    /// Rolling log of all daemon tracing output.
    /// Tuple is `(next_sequence_number, lines)` where each line is `"<seq>\t<text>"`.
    pub app_log: Arc<Mutex<(u64, VecDeque<String>)>>,
    pub scanner_stop_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    pub scanner_wifi_config: Arc<Mutex<ScannerWifiConfigRequest>>,
    pub scanner_cortex_config: Arc<Mutex<ScannerCortexConfigRequest>>,
    pub device_log: Arc<Mutex<VecDeque<DeviceLogEntry>>>,
    /// Base directory for user skill data (sessions, labels, embeddings).
    pub skill_dir: Arc<Mutex<PathBuf>>,
    /// Hook rules (daemon-authoritative).
    pub hooks: Arc<Mutex<Vec<HookRule>>>,
    /// Remote-access iroh auth store (phone pairing, TOTP, client registry).
    pub iroh_auth: SharedIrohAuth,
    /// Live state of the remote-access iroh tunnel (online flag, endpoint_id, …).
    pub iroh_runtime: SharedIrohRuntime,
    /// Maps local TCP source port → iroh peer endpoint_id for axum middleware.
    pub iroh_peer_map: IrohPeerMap,
    /// Sender half of the device-proxy event channel consumed by the session runner.
    pub iroh_device_tx: SharedDeviceEventTx,
    /// rlsl-iroh sink endpoint id when running.
    pub lsl_iroh_endpoint_id: Arc<Mutex<Option<String>>>,
    pub lsl_auto_connect: Arc<Mutex<bool>>,
    pub lsl_paired_streams: Arc<Mutex<Vec<LslPairedStream>>>,
    pub lsl_idle_timeout_secs: Arc<Mutex<Option<u64>>>,
    pub lsl_virtual_source: Arc<Mutex<Option<skill_lsl::VirtualLslSource>>>,
    pub track_active_window: Arc<AtomicBool>,
    pub track_input_activity: Arc<AtomicBool>,
    /// Latest EEG band power snapshot (~4 Hz update from session runner).
    pub latest_bands: Arc<Mutex<Option<serde_json::Value>>>,
    /// Multi-token auth store.
    pub token_store: Arc<Mutex<crate::auth::TokenStore>>,
    pub llm_status: Arc<Mutex<String>>,
    pub llm_model_name: Arc<Mutex<String>>,
    #[cfg(not(feature = "llm"))]
    pub llm_mmproj_name: Arc<Mutex<Option<String>>>,
    #[cfg(not(feature = "llm"))]
    pub llm_logs: Arc<Mutex<Vec<serde_json::Value>>>,
    pub llm_catalog: Arc<Mutex<skill_llm::catalog::LlmCatalog>>,
    pub llm_downloads: Arc<Mutex<HashMap<String, Arc<Mutex<skill_llm::catalog::DownloadProgress>>>>>,
    #[cfg(feature = "llm")]
    pub llm_config: Arc<Mutex<LlmConfig>>,
    #[cfg(feature = "llm")]
    pub llm_log_buffer: LlmLogBuffer,
    #[cfg(feature = "llm")]
    pub llm_state_cell: LlmStateCell,
    /// Active OpenBCI session handle (cancel sender).
    pub session_handle: Arc<Mutex<Option<crate::session_runner::SessionHandle>>>,
    /// Shared EXG model status (download progress, encoder state, etc.).
    pub exg_model_status: Arc<Mutex<skill_eeg::eeg_model_config::EegModelStatus>>,
    /// Cancel flag for the EXG weights download thread.
    pub exg_download_cancel: Arc<AtomicBool>,
    /// Daemon-owned HNSW indices for label search (text, context, EEG).
    pub label_index: Arc<LabelIndexState>,
}

impl AppState {
    pub fn new(auth_token: String, skill_dir: PathBuf) -> Self {
        let (events_tx, _) = broadcast::channel(4096);
        let settings = skill_settings::load_settings(&skill_dir);
        let token_store = crate::auth::TokenStore::load(&skill_dir);
        let hooks = settings.hooks.clone();
        let llm_catalog = skill_llm::catalog::LlmCatalog::load(&skill_dir);
        #[cfg(feature = "llm")]
        let llm_config = settings.llm.clone();
        Self {
            auth_token: Arc::new(Mutex::new(auth_token)),
            events_tx,
            tracker: Arc::new(Mutex::new(DaemonTracker::default())),
            status: Arc::new(Mutex::new(StatusResponse {
                state: "disconnected".to_string(),
                ..Default::default()
            })),
            devices: Arc::new(Mutex::new(Vec::new())),
            scanner_running: Arc::new(Mutex::new(false)),
            ble_device_cache: Arc::new(Mutex::new(HashMap::new())),
            ble_scan_paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            app_log: Arc::new(Mutex::new((0, VecDeque::with_capacity(512)))),
            scanner_stop_tx: Arc::new(Mutex::new(None)),
            scanner_wifi_config: Arc::new(Mutex::new(ScannerWifiConfigRequest {
                wifi_shield_ip: String::new(),
                galea_ip: String::new(),
            })),
            scanner_cortex_config: Arc::new(Mutex::new(ScannerCortexConfigRequest {
                emotiv_client_id: String::new(),
                emotiv_client_secret: String::new(),
            })),
            device_log: Arc::new(Mutex::new(VecDeque::with_capacity(256))),
            skill_dir: Arc::new(Mutex::new(skill_dir.clone())),
            hooks: Arc::new(Mutex::new(hooks)),
            iroh_auth: Arc::new(Mutex::new(skill_iroh::IrohAuthStore::open(&skill_dir))),
            iroh_runtime: Arc::new(Mutex::new(IrohRuntimeState::default())),
            iroh_peer_map: skill_iroh::new_peer_map(),
            iroh_device_tx: Arc::new(Mutex::new(None)),
            lsl_iroh_endpoint_id: Arc::new(Mutex::new(None)),
            lsl_auto_connect: Arc::new(Mutex::new(settings.lsl_auto_connect)),
            lsl_paired_streams: Arc::new(Mutex::new(settings.lsl_paired_streams.clone())),
            lsl_idle_timeout_secs: Arc::new(Mutex::new(settings.lsl_idle_timeout_secs)),
            lsl_virtual_source: Arc::new(Mutex::new(None)),
            track_active_window: Arc::new(AtomicBool::new(settings.track_active_window)),
            track_input_activity: Arc::new(AtomicBool::new(settings.track_input_activity)),
            latest_bands: Arc::new(Mutex::new(None)),
            token_store: Arc::new(Mutex::new(token_store)),
            llm_status: Arc::new(Mutex::new("stopped".to_string())),
            llm_model_name: Arc::new(Mutex::new(String::new())),
            #[cfg(not(feature = "llm"))]
            llm_mmproj_name: Arc::new(Mutex::new(None)),
            #[cfg(not(feature = "llm"))]
            llm_logs: Arc::new(Mutex::new(Vec::new())),
            llm_catalog: Arc::new(Mutex::new(llm_catalog)),
            llm_downloads: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(feature = "llm")]
            llm_config: Arc::new(Mutex::new(llm_config)),
            #[cfg(feature = "llm")]
            llm_log_buffer: skill_llm::new_log_buffer(),
            #[cfg(feature = "llm")]
            llm_state_cell: skill_llm::new_state_cell(),
            session_handle: Arc::new(Mutex::new(None)),
            exg_model_status: Arc::new(Mutex::new(skill_eeg::eeg_model_config::EegModelStatus::default())),
            exg_download_cancel: Arc::new(AtomicBool::new(false)),
            label_index: Arc::new(LabelIndexState::new()),
        }
    }
}
