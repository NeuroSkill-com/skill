// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Core shared types: `AppState`, `DeviceStatus`, IPC packet structs, handles.

#[cfg(not(feature = "llm"))]
use std::sync::Mutex;

use serde::Serialize;
use tauri::ipc::Channel;

use crate::active_window::ActiveWindowInfo;
use crate::constants::{EEG_CHANNELS, EMBEDDING_OVERLAP_SECS};
use crate::screenshot;
use crate::settings::{
    default_accent_color, default_api_shortcut, default_calibration_shortcut,
    default_daily_goal_min, default_embedding_model, default_focus_timer_shortcut,
    default_help_shortcut, default_history_shortcut, default_label_shortcut,
    default_search_shortcut, default_settings_shortcut, default_skill_dir, default_theme,
    default_theme_shortcut, default_track_active_window, default_track_input_activity,
    default_update_check_interval, default_ws_host, default_ws_port, load_umap_config,
    CalibrationProfile, DoNotDisturbConfig, HookLastTrigger, HookRule, NeuttsConfig,
    ScreenshotConfig, UmapUserConfig,
};
use crate::skill_log::SkillLogger;
use crate::tts::init_tts_dirs;
use crate::{unix_secs, yyyymmdd_utc};
use skill_data::activity_store::ActivityStore;
use skill_data::label_store;
use skill_data::screenshot_store;
use skill_eeg::eeg_bands::BandSnapshot;
use skill_eeg::eeg_filter::FilterConfig;
use skill_eeg::eeg_model_config::{load_model_config, EegModelStatus, ExgModelConfig};
use skill_eeg::eeg_quality::SignalQuality;
use std::collections::VecDeque;

#[cfg(feature = "llm")]
use crate::settings::default_chat_shortcut;

// Re-export from skill-data (canonical definition).
pub use skill_data::device::PairedDevice;

// ── Runtime-only discovered device ────────────────────────────────────────────

#[derive(Clone, Debug, Serialize)]
pub struct DiscoveredDevice {
    pub id: String,
    pub name: String,
    pub last_seen: u64,
    pub last_rssi: i16,
    pub is_paired: bool,
    pub is_preferred: bool,
    /// How this device was discovered (ble, usb_serial, wifi, cortex).
    pub transport: crate::device_scanner::Transport,
}

// ── EEG / PPG / IMU IPC packets ───────────────────────────────────────────────

/// EEG packet forwarded to the frontend for live visualisation.
#[derive(Clone, Serialize)]
pub struct EegPacket {
    pub electrode: usize,
    pub samples: Vec<f64>,
    pub timestamp: f64,
}

/// PPG packet forwarded to the frontend for live visualisation.
#[derive(Clone, Serialize)]
pub struct PpgPacket {
    pub channel: usize,
    pub samples: Vec<f64>,
    pub timestamp: f64,
}

/// IMU packet forwarded to the frontend via Tauri IPC channel.
#[derive(Clone, Serialize)]
pub struct ImuPacket {
    pub sensor: String,
    pub samples: [[f32; 3]; 3],
    pub timestamp: f64,
}

// ── Session / scanner handles ─────────────────────────────────────────────────

pub struct StreamHandle {
    pub cancel_tx: tokio::sync::oneshot::Sender<()>,
}
pub struct ScannerHandle {
    pub cancel_tx: tokio::sync::oneshot::Sender<()>,
}

// ── Secondary (concurrent) sessions ──────────────────────────────────────────

/// A lightweight concurrent session that records to its own CSV while
/// the primary session owns the dashboard and embedding pipeline.
#[derive(Clone, Serialize)]
pub struct SecondarySessionInfo {
    pub id: String,
    pub device_name: String,
    pub device_kind: String,
    pub channels: usize,
    pub sample_rate: f64,
    pub sample_count: u64,
    pub csv_path: String,
    pub started_at: u64,
    pub battery: f32,
}

/// Cancel handle for a secondary session (not serialisable).
pub struct SecondarySessionHandle {
    pub cancel: tokio_util::sync::CancellationToken,
    pub info: SecondarySessionInfo,
}

// ── Shared frontend-visible status ────────────────────────────────────────────

#[derive(Clone, Serialize)]
pub struct DeviceStatus {
    pub state: String,
    pub device_name: Option<String>,
    pub device_id: Option<String>,
    pub serial_number: Option<String>,
    pub mac_address: Option<String>,
    pub firmware_version: Option<String>,
    pub hardware_version: Option<String>,
    pub bootloader_version: Option<String>,
    pub headset_preset: Option<String>,
    pub csv_path: Option<String>,
    pub sample_count: u64,
    pub battery: f32,
    pub eeg: Vec<f64>,
    pub paired_devices: Vec<PairedDevice>,
    pub device_error: Option<String>,
    pub target_name: Option<String>,
    pub filter_config: FilterConfig,
    pub channel_quality: Vec<SignalQuality>,
    pub embedding_overlap_secs: f32,
    pub retry_attempt: u32,
    pub retry_countdown_secs: u32,
    pub ppg: Vec<f64>,
    pub ppg_sample_count: u64,
    pub accel: [f32; 3],
    pub gyro: [f32; 3],
    pub fuel_gauge_mv: f32,
    pub temperature_raw: u16,
    pub device_kind: String,
    /// EEG channel labels for the connected device (set at session start).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub channel_names: Vec<String>,
    /// PPG channel labels for the connected device.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ppg_channel_names: Vec<String>,
    /// IMU channel labels for the connected device.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub imu_channel_names: Vec<String>,
    /// fNIRS channel labels for the connected device.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fnirs_channel_names: Vec<String>,
    /// fNIRS oxygenation proxy (0–100), derived from red/IR channel ratios.
    pub fnirs_oxygenation_pct: f64,
    /// fNIRS workload proxy (0–100), derived from bilateral activation magnitude.
    pub fnirs_workload: f64,
    /// fNIRS lateralisation proxy (-100..100), left-vs-right activation balance.
    pub fnirs_lateralization: f64,
    /// fNIRS ΔHbO proxy (left channel, arbitrary units).
    pub fnirs_hbo_left: f64,
    /// fNIRS ΔHbO proxy (right channel, arbitrary units).
    pub fnirs_hbo_right: f64,
    /// fNIRS ΔHbR proxy (left channel, arbitrary units).
    pub fnirs_hbr_left: f64,
    /// fNIRS ΔHbR proxy (right channel, arbitrary units).
    pub fnirs_hbr_right: f64,
    /// fNIRS ΔHbT proxy (left channel, arbitrary units).
    pub fnirs_hbt_left: f64,
    /// fNIRS ΔHbT proxy (right channel, arbitrary units).
    pub fnirs_hbt_right: f64,
    /// Rolling left/right ΔHbO connectivity proxy (Pearson r, -1..1).
    pub fnirs_connectivity: f64,
    /// Phone descriptor from the remote iOS client (model, OS, locale, etc.).
    /// Populated when a remote device streams via iroh with `MSG_PHONE_INFO`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_info: Option<serde_json::Value>,
    /// Display name of the connected iroh client (from the auth store).
    /// Set when a remote session starts via iroh's device-proxy channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iroh_client_name: Option<String>,
    /// Hardware EEG channel count of the connected device.
    pub eeg_channel_count: usize,
    /// Hardware EEG sample rate of the connected device (Hz).
    pub eeg_sample_rate_hz: f64,
    /// Device has a PPG (heart-rate) sensor.
    pub has_ppg: bool,
    /// Device has an IMU (accelerometer + gyroscope).
    pub has_imu: bool,
    /// Device has electrodes at central scalp sites (C3/C4/Cz).
    pub has_central_electrodes: bool,
    /// Device supports a full 10-20 montage (or superset).
    pub has_full_montage: bool,
}

impl Default for DeviceStatus {
    fn default() -> Self {
        Self {
            state: "disconnected".into(),
            device_name: None,
            device_id: None,
            serial_number: None,
            mac_address: None,
            firmware_version: None,
            hardware_version: None,
            bootloader_version: None,
            headset_preset: None,
            csv_path: None,
            sample_count: 0,
            battery: 0.0,
            eeg: vec![f64::NAN; EEG_CHANNELS],
            paired_devices: Vec::new(),
            device_error: None,
            target_name: None,
            filter_config: FilterConfig::default(),
            channel_quality: Vec::new(),
            embedding_overlap_secs: EMBEDDING_OVERLAP_SECS,
            retry_attempt: 0,
            retry_countdown_secs: 0,
            ppg: vec![0.0; 3],
            ppg_sample_count: 0,
            accel: [0.0; 3],
            gyro: [0.0; 3],
            fuel_gauge_mv: 0.0,
            temperature_raw: 0,
            device_kind: "unknown".into(),
            channel_names: Vec::new(),
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
            fnirs_oxygenation_pct: 0.0,
            fnirs_workload: 0.0,
            fnirs_lateralization: 0.0,
            fnirs_hbo_left: 0.0,
            fnirs_hbo_right: 0.0,
            fnirs_hbr_left: 0.0,
            fnirs_hbr_right: 0.0,
            fnirs_hbt_left: 0.0,
            fnirs_hbt_right: 0.0,
            fnirs_connectivity: 0.0,
            phone_info: None,
            iroh_client_name: None,
            eeg_channel_count: 0,
            eeg_sample_rate_hz: 0.0,
            has_ppg: false,
            has_imu: false,
            has_central_electrodes: false,
            has_full_montage: false,
        }
    }
}

impl DeviceStatus {
    /// Reset device-specific fields when transitioning to a disconnected state.
    ///
    /// Sets the state string and clears all device identity, telemetry, and
    /// error fields.  Call this instead of manually zeroing 15+ fields in
    /// `go_disconnected` / reconnect paths.
    pub fn reset_disconnected(&mut self, new_state: &str) {
        self.state = new_state.into();
        self.device_name = None;
        self.device_id = None;
        self.device_kind = "unknown".into();
        self.serial_number = None;
        self.mac_address = None;
        self.firmware_version = None;
        self.hardware_version = None;
        self.bootloader_version = None;
        self.headset_preset = None;
        self.battery = 0.0;
        self.eeg = vec![f64::NAN; EEG_CHANNELS];
        self.ppg = vec![0.0; 3];
        self.ppg_sample_count = 0;
        self.device_error = None;
        self.target_name = None;
        self.retry_attempt = 0;
        self.retry_countdown_secs = 0;
        self.channel_quality = Vec::new();
        self.channel_names = Vec::new();
        self.ppg_channel_names = Vec::new();
        self.imu_channel_names = Vec::new();
        self.fnirs_channel_names = Vec::new();
        self.fnirs_oxygenation_pct = 0.0;
        self.fnirs_workload = 0.0;
        self.fnirs_lateralization = 0.0;
        self.fnirs_hbo_left = 0.0;
        self.fnirs_hbo_right = 0.0;
        self.fnirs_hbr_left = 0.0;
        self.fnirs_hbr_right = 0.0;
        self.fnirs_hbt_left = 0.0;
        self.fnirs_hbt_right = 0.0;
        self.fnirs_connectivity = 0.0;
        self.phone_info = None;
        self.iroh_client_name = None;
        self.eeg_channel_count = 0;
        self.eeg_sample_rate_hz = 0.0;
        self.has_ppg = false;
        self.has_imu = false;
        self.has_central_electrodes = false;
        self.has_full_montage = false;
    }

    /// Reset transient fields for a new scanning cycle.
    pub fn reset_for_scanning(
        &mut self,
        device_kind: &str,
        csv_path: &std::path::Path,
        preferred_id: Option<&str>,
    ) {
        self.state = "scanning".into();
        self.device_kind = device_kind.into();
        self.device_name = None;
        self.device_id = None;
        self.serial_number = None;
        self.mac_address = None;
        self.firmware_version = None;
        self.hardware_version = None;
        self.bootloader_version = None;
        self.headset_preset = None;
        self.csv_path = Some(csv_path.to_string_lossy().into_owned());
        self.device_error = None;
        self.battery = 0.0;
        self.eeg = vec![f64::NAN; EEG_CHANNELS];
        self.sample_count = 0;
        self.channel_names = Vec::new();
        self.ppg_channel_names = Vec::new();
        self.imu_channel_names = Vec::new();
        self.fnirs_channel_names = Vec::new();
        self.fnirs_oxygenation_pct = 0.0;
        self.fnirs_workload = 0.0;
        self.fnirs_lateralization = 0.0;
        self.fnirs_hbo_left = 0.0;
        self.fnirs_hbo_right = 0.0;
        self.fnirs_hbr_left = 0.0;
        self.fnirs_hbr_right = 0.0;
        self.fnirs_hbt_left = 0.0;
        self.fnirs_hbt_right = 0.0;
        self.fnirs_connectivity = 0.0;
        self.eeg_channel_count = 0;
        self.eeg_sample_rate_hz = 0.0;
        self.ppg = vec![0.0; 3];
        self.ppg_sample_count = 0;
        self.target_name = preferred_id.and_then(|id| {
            self.paired_devices
                .iter()
                .find(|d| d.id == id)
                .map(|d| d.name.clone())
        });
        // Populate capability flags from the device kind.
        self.apply_capabilities_from_kind();
    }

    /// Derive and set capability booleans from the current `device_kind` string.
    pub fn apply_capabilities_from_kind(&mut self) {
        use skill_data::device::DeviceKind;
        let kind = DeviceKind::from_kind_str(&self.device_kind);
        let caps = kind.capabilities();
        self.has_ppg = caps.has_ppg;
        self.has_imu = caps.has_imu;
        self.has_central_electrodes = caps.has_central_electrodes;
        self.has_full_montage = caps.has_full_montage;
    }
}

// ── Sub-state: keyboard shortcuts ─────────────────────────────────────────────

/// All user-configurable global keyboard shortcuts.
///
/// Extracted from `AppState` so shortcut logic can be read/written without
/// contending on unrelated fields.
pub struct ShortcutState {
    pub label_shortcut: String,
    pub search_shortcut: String,
    pub settings_shortcut: String,
    pub calibration_shortcut: String,
    pub help_shortcut: String,
    pub history_shortcut: String,
    pub api_shortcut: String,
    pub theme_shortcut: String,
    pub focus_timer_shortcut: String,
    #[cfg(feature = "llm")]
    pub chat_shortcut: String,
}

impl Default for ShortcutState {
    fn default() -> Self {
        Self {
            label_shortcut: default_label_shortcut(),
            search_shortcut: default_search_shortcut(),
            settings_shortcut: default_settings_shortcut(),
            calibration_shortcut: default_calibration_shortcut(),
            help_shortcut: default_help_shortcut(),
            history_shortcut: default_history_shortcut(),
            api_shortcut: default_api_shortcut(),
            theme_shortcut: default_theme_shortcut(),
            focus_timer_shortcut: default_focus_timer_shortcut(),
            #[cfg(feature = "llm")]
            chat_shortcut: default_chat_shortcut(),
        }
    }
}

// ── Sub-state: UI preferences ─────────────────────────────────────────────────

/// User-facing appearance and onboarding preferences.
pub struct UiPrefsState {
    pub theme: String,
    pub language: String,
    pub accent_color: String,
    pub daily_goal_min: u32,
    pub goal_notified_date: String,
    pub onboarding_complete: bool,
    pub last_seen_whats_new_version: String,
    pub text_embedding_model: String,
    pub main_window_auto_fit: bool,
}

impl Default for UiPrefsState {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            language: String::new(),
            accent_color: default_accent_color(),
            daily_goal_min: default_daily_goal_min(),
            goal_notified_date: String::new(),
            onboarding_complete: false,
            last_seen_whats_new_version: String::new(),
            text_embedding_model: default_embedding_model(),
            main_window_auto_fit: true,
        }
    }
}

// ── Sub-state: input / activity tracking ──────────────────────────────────────

/// Keyboard, mouse and active-window tracking state.
///
/// The `Arc<Atomic*>` fields are shared with background threads (input
/// monitor, active-window poller) that update them without locking `AppState`.
pub struct InputTrackingState {
    pub track_active_window: bool,
    pub current_active_window: Option<ActiveWindowInfo>,
    pub track_input_activity: bool,
    pub input_activity_enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub last_keyboard_ts: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub last_mouse_ts: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub kbd_event_count: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub mouse_event_count: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub activity_store: Option<std::sync::Arc<ActivityStore>>,
}

impl Default for InputTrackingState {
    fn default() -> Self {
        Self {
            track_active_window: default_track_active_window(),
            current_active_window: None,
            track_input_activity: default_track_input_activity(),
            input_activity_enabled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                default_track_input_activity(),
            )),
            last_keyboard_ts: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_mouse_ts: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            kbd_event_count: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            mouse_event_count: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            activity_store: None,
        }
    }
}

// ── Sub-state: EEG embedding model ────────────────────────────────────────────

/// EEG model weights, download progress, and encoder reload flag.
pub struct EmbeddingModelState {
    pub model_config: ExgModelConfig,
    pub model_status: std::sync::Arc<std::sync::Mutex<EegModelStatus>>,
    pub download_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub encoder_reload_requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl EmbeddingModelState {
    pub fn new(skill_dir: &std::path::Path) -> Self {
        Self {
            model_config: load_model_config(skill_dir),
            model_status: std::sync::Arc::new(std::sync::Mutex::new(EegModelStatus::default())),
            download_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            encoder_reload_requested: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            )),
        }
    }
}

// ── Full app state (Mutex-managed) ────────────────────────────────────────────

#[derive(Default)]
pub struct FnirsRuntime {
    pub baseline_ir_left: Option<f64>,
    pub baseline_red_left: Option<f64>,
    pub baseline_ir_right: Option<f64>,
    pub baseline_red_right: Option<f64>,
    pub hbo_left_hist: VecDeque<f64>,
    pub hbo_right_hist: VecDeque<f64>,
}

pub struct AppState {
    // ── Device session ────────────────────────────────────────────────────
    pub status: DeviceStatus,
    pub stream: Option<StreamHandle>,
    pub scanner: Option<ScannerHandle>,
    pub discovered: Vec<DiscoveredDevice>,
    pub preferred_id: Option<String>,
    pub eeg_channel: Option<Channel<EegPacket>>,
    pub ppg_channel: Option<Channel<PpgPacket>>,
    pub imu_channel: Option<Channel<ImuPacket>>,
    pub battery_ema: Option<f32>,
    pub latest_bands: Option<BandSnapshot>,
    pub fnirs_runtime: FnirsRuntime,
    pub pending_reconnect: bool,
    pub retry_attempt: u32,
    pub session_start_utc: Option<u64>,

    /// Accumulated SNR (dB) for the current session — used to compute the
    /// average SNR written to the session sidecar JSON for quality filtering.
    pub snr_sum: f64,
    pub snr_count: u64,

    /// Concurrent secondary sessions recording in the background.
    /// Key is the session id (e.g. "lsl:OpenBCI", "ble:Muse-1234").
    pub secondary_sessions: std::collections::HashMap<String, SecondarySessionHandle>,

    // ── Infrastructure ────────────────────────────────────────────────────
    pub skill_dir: std::path::PathBuf,
    pub logger: std::sync::Arc<SkillLogger>,
    pub label_store: Option<label_store::LabelStore>,

    // ── Grouped sub-states ────────────────────────────────────────────────
    pub shortcuts: ShortcutState,
    pub ui: UiPrefsState,
    pub input: InputTrackingState,
    pub embedding: EmbeddingModelState,

    // ── Calibration ───────────────────────────────────────────────────────
    pub calibration_profiles: Vec<CalibrationProfile>,
    pub active_calibration_id: String,
    pub umap_config: UmapUserConfig,

    // ── Hooks ─────────────────────────────────────────────────────────────
    pub hooks: Vec<HookRule>,
    pub hook_runtime:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,

    // ── Network / services ────────────────────────────────────────────────
    pub ws_host: String,
    pub ws_port: u16,
    pub api_token: String,
    pub update_check_interval_secs: u64,
    /// Set by the frontend when an update has been downloaded and is ready
    /// to install on next restart / relaunch.
    pub update_ready_to_install: bool,

    // ── Device configs ────────────────────────────────────────────────────
    pub openbci_config: crate::settings::OpenBciConfig,
    pub device_api_config: crate::settings::DeviceApiConfig,
    pub scanner_config: crate::settings::ScannerConfig,

    /// rlsl-iroh sink endpoint ID (set when the sink is running).
    pub lsl_iroh_endpoint_id: Option<String>,
    /// Running virtual LSL EEG source (32 ch, 256 Hz) for testing.
    /// `None` when stopped, `Some` while the outlet thread is live.
    pub lsl_virtual_source: Option<skill_lsl::VirtualLslSource>,

    /// Location services enabled by the user (default false).
    pub location_enabled: bool,
    /// Auto-scan for LSL streams and connect paired ones automatically.
    pub lsl_auto_connect: bool,
    /// LSL streams the user has "paired" for auto-connect.
    pub lsl_paired_streams: Vec<skill_settings::LslPairedStream>,
    /// Idle watchdog for LSL sessions: stop after this many seconds of silence.
    /// `None` disables the watchdog for LSL entirely.
    pub lsl_idle_timeout_secs: Option<u64>,

    /// High-level inference device preference: `"gpu"` or `"cpu"`.
    pub inference_device: String,
    /// Last-saved `llm.n_gpu_layers` before a CPU override was applied.
    pub llm_gpu_layers_saved: u32,
    /// Inference device preference for EXG embeddings: `"gpu"` or `"cpu"`.
    pub exg_inference_device: String,

    /// Emotiv Cortex WebSocket connection state for the UI.
    /// One of: `"disconnected"`, `"connecting"`, `"connected"`.
    pub cortex_ws_state: String,

    // ── Smart alarm ────────────────────────────────────────────────────────
    pub alarm_config: Option<crate::ws_commands::dnd_sleep::AlarmConfig>,

    // ── TTS ───────────────────────────────────────────────────────────────
    pub neutts_config: NeuttsConfig,
    pub tts_preload: bool,

    // ── Independently-locked sub-states ───────────────────────────────────
    pub dnd: std::sync::Arc<std::sync::Mutex<DndRuntimeState>>,
    pub llm: std::sync::Arc<std::sync::Mutex<LlmState>>,

    // ── Storage / recording ───────────────────────────────────────────────
    pub settings_storage_format: String,
    /// Maximum number of EEG channels to process through the DSP pipeline.
    /// Channels beyond this limit are still recorded to CSV but not processed.
    /// Range: 2–1024.  Default: 24.  Capped at `EEG_CHANNELS` (24) for DSP arrays.
    pub max_pipeline_channels: usize,
    pub sleep_config: crate::settings::SleepConfig,
    pub screenshot_config: ScreenshotConfig,
    pub screenshot_store: Option<std::sync::Arc<screenshot_store::ScreenshotStore>>,
    pub screenshot_metrics: std::sync::Arc<screenshot::ScreenshotMetrics>,
    pub health_store: Option<std::sync::Arc<skill_data::health_store::HealthStore>>,
}

// ── DND runtime state (independently locked) ──────────────────────────────────

/// Do-Not-Disturb runtime state.  Lives behind its own `Arc<Mutex<>>` so the
/// DND polling loop and session runner can access it without contending on the
/// main `AppState` lock.
#[derive(Default)]
pub struct DndRuntimeState {
    pub config: DoNotDisturbConfig,
    pub active: bool,
    pub os_active: Option<bool>,
    pub last_error: Option<String>,
    pub focus_samples: std::collections::VecDeque<f64>,
    pub below_ticks: u32,
    pub score_history: std::collections::VecDeque<f64>,
    pub snr_low_ticks: u32,
}

// ── LLM sub-state (heap-allocated) ────────────────────────────────────────────

pub struct LlmState {
    pub config: crate::settings::LlmConfig,
    #[cfg(feature = "llm")]
    pub catalog: crate::llm::catalog::LlmCatalog,
    #[cfg(feature = "llm")]
    pub downloads: std::collections::HashMap<
        String,
        std::sync::Arc<std::sync::Mutex<crate::llm::catalog::DownloadProgress>>,
    >,
    #[cfg(feature = "llm")]
    pub logs: crate::llm::LlmLogBuffer,
    #[cfg(feature = "llm")]
    pub state_cell: crate::llm::LlmStateCell,
    #[cfg(not(feature = "llm"))]
    pub state_cell: std::sync::Arc<Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>>,
    #[cfg(feature = "llm")]
    pub loading: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "llm")]
    pub start_error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    #[cfg(feature = "llm")]
    pub chat_store: Option<crate::llm::chat_store::ChatStore>,
}

impl LlmState {
    /// Create a new `LlmState` initialised from the given data directory.
    #[cfg(feature = "llm")]
    pub fn new(skill_dir: &std::path::Path) -> Self {
        Self {
            config: crate::settings::LlmConfig::default(),
            downloads: std::collections::HashMap::new(),
            logs: crate::llm::new_log_buffer(),
            state_cell: crate::llm::new_state_cell(),
            loading: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            start_error: std::sync::Arc::new(std::sync::Mutex::new(None)),
            catalog: crate::llm::catalog::LlmCatalog::load(skill_dir),
            chat_store: crate::llm::chat_store::ChatStore::open(skill_dir),
        }
    }

    /// Create a new `LlmState` when the `llm` feature is disabled.
    #[cfg(not(feature = "llm"))]
    pub fn new(_skill_dir: &std::path::Path) -> Self {
        Self {
            config: crate::settings::LlmConfig::default(),
            state_cell: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        let skill_dir = default_skill_dir();
        let _ = std::fs::create_dir_all(&skill_dir);

        init_tts_dirs(&skill_dir);

        let health_store =
            skill_data::health_store::HealthStore::open(&skill_dir).map(std::sync::Arc::new);

        let log_config = crate::skill_log::load_log_config(&skill_dir);
        crate::skill_log::ensure_log_config(&skill_dir);
        let today_dir = skill_dir.join(yyyymmdd_utc());
        let log_path = today_dir.join(format!("log_{}.txt", unix_secs()));
        crate::skill_log::tee_stderr_to_file(&log_path);
        let logger = std::sync::Arc::new(SkillLogger::new(log_config));
        logger.write("logger", &format!("session log: {}", log_path.display()));

        let input = InputTrackingState {
            activity_store: ActivityStore::open(&skill_dir).map(std::sync::Arc::new),
            ..Default::default()
        };

        Self {
            status: DeviceStatus::default(),
            stream: None,
            scanner: None,
            discovered: Vec::new(),
            preferred_id: None,
            eeg_channel: None,
            ppg_channel: None,
            imu_channel: None,
            battery_ema: None,
            latest_bands: None,
            fnirs_runtime: FnirsRuntime::default(),
            pending_reconnect: false,
            retry_attempt: 0,
            session_start_utc: None,
            snr_sum: 0.0,
            snr_count: 0,
            secondary_sessions: std::collections::HashMap::new(),
            label_store: label_store::LabelStore::open(&skill_dir),

            shortcuts: ShortcutState::default(),
            ui: UiPrefsState::default(),
            input,
            embedding: EmbeddingModelState::new(&skill_dir),

            calibration_profiles: vec![CalibrationProfile::default()],
            active_calibration_id: "default".into(),
            umap_config: load_umap_config(&skill_dir),
            hooks: Vec::new(),
            hook_runtime: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            ws_host: default_ws_host(),
            ws_port: default_ws_port(),
            api_token: String::new(),
            update_check_interval_secs: default_update_check_interval(),
            update_ready_to_install: false,
            openbci_config: crate::settings::OpenBciConfig::default(),
            location_enabled: false,
            lsl_iroh_endpoint_id: None,
            lsl_virtual_source: None,
            lsl_auto_connect: false,
            lsl_paired_streams: Vec::new(),
            lsl_idle_timeout_secs: skill_settings::default_lsl_idle_timeout_secs(),
            inference_device: skill_settings::default_inference_device(),
            llm_gpu_layers_saved: skill_settings::default_llm_gpu_layers_saved(),
            exg_inference_device: skill_settings::default_exg_inference_device(),
            device_api_config: crate::settings::DeviceApiConfig::default(),
            scanner_config: crate::settings::ScannerConfig::default(),
            cortex_ws_state: "disconnected".into(),
            alarm_config: None,
            neutts_config: NeuttsConfig::default(),
            tts_preload: true,
            llm: std::sync::Arc::new(std::sync::Mutex::new(LlmState::new(&skill_dir))),
            skill_dir,
            logger,
            dnd: std::sync::Arc::new(std::sync::Mutex::new(DndRuntimeState::default())),
            settings_storage_format: "csv".into(),
            max_pipeline_channels: skill_constants::EEG_CHANNELS, // 32
            sleep_config: crate::settings::SleepConfig::default(),
            screenshot_config: ScreenshotConfig::default(),
            screenshot_store: None,
            screenshot_metrics: std::sync::Arc::new(screenshot::ScreenshotMetrics::new()),
            health_store,
        }
    }
}

impl AppState {
    /// Construct a heap-allocated `AppState` on a dedicated thread with a
    /// larger stack so the struct is never materialised on the main thread's
    /// (often limited) stack.  The stack budget was reduced from 32 MB to
    /// 8 MB after `LlmState` and `DndRuntimeState` were extracted behind
    /// their own `Arc<Mutex<>>`.
    pub fn new_boxed() -> Box<Self> {
        std::thread::Builder::new()
            .name("appstate-init".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(|| Box::new(Self::default()))
            .expect("[appstate] failed to spawn init thread")
            .join()
            .expect("[appstate] init thread panicked")
    }

    /// Obtain a clone of the `LlmState` arc for independent locking.
    pub fn llm_arc(&self) -> std::sync::Arc<std::sync::Mutex<LlmState>> {
        self.llm.clone()
    }

    /// Obtain a clone of the `DndRuntimeState` arc for independent locking.
    pub fn dnd_arc(&self) -> std::sync::Arc<std::sync::Mutex<DndRuntimeState>> {
        self.dnd.clone()
    }
}
