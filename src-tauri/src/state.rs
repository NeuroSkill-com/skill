// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Core shared types: `AppState`, `DeviceStatus`, IPC packet structs, handles.

use serde::Serialize;

use crate::constants::{EEG_CHANNELS, EMBEDDING_OVERLAP_SECS};
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
use skill_eeg::eeg_filter::FilterConfig;
use skill_eeg::eeg_model_config::{load_model_config, EegModelStatus, ExgModelConfig};
use skill_eeg::eeg_quality::SignalQuality;

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
    pub transport: skill_daemon_common::DeviceTransport,
}

// ── EEG / PPG / IMU IPC packets ───────────────────────────────────────────────

// ── Session / scanner handles ─────────────────────────────────────────────────

pub struct StreamHandle {
    #[allow(dead_code)]
    pub cancel_tx: tokio::sync::oneshot::Sender<()>,
}
pub struct ScannerHandle {
    #[allow(dead_code)]
    pub cancel_tx: tokio::sync::oneshot::Sender<()>,
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
    pub target_id: Option<String>,
    pub target_display_name: Option<String>,
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
    /// True when at least one iroh tunnel peer is currently online.
    pub iroh_tunnel_online: bool,
    /// Number of active iroh device-proxy peers.
    pub iroh_connected_peers: usize,
    /// True when a remote BLE device is connected on any iroh peer.
    pub iroh_remote_device_connected: bool,
    /// True when recent sensor chunks are actively flowing over iroh.
    pub iroh_streaming_active: bool,
    /// True when recent EEG-bearing chunks are actively flowing over iroh.
    pub iroh_eeg_streaming_active: bool,
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
            target_id: None,
            target_display_name: None,
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
            iroh_tunnel_online: false,
            iroh_connected_peers: 0,
            iroh_remote_device_connected: false,
            iroh_streaming_active: false,
            iroh_eeg_streaming_active: false,
            eeg_channel_count: 0,
            eeg_sample_rate_hz: 0.0,
            has_ppg: false,
            has_imu: false,
            has_central_electrodes: false,
            has_full_montage: false,
        }
    }
}

// DeviceStatus reset/scanning methods removed — state resets are daemon-owned.

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
/// Daemon is authoritative for activity workers and persistence; these fields
/// are local UI mirrors only.
pub struct InputTrackingState {
    pub track_active_window: bool,
    pub track_input_activity: bool,
    pub input_activity_enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Default for InputTrackingState {
    fn default() -> Self {
        Self {
            track_active_window: default_track_active_window(),
            track_input_activity: default_track_input_activity(),
            input_activity_enabled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                default_track_input_activity(),
            )),
        }
    }
}

// ── Sub-state: EEG embedding model ────────────────────────────────────────────

/// EEG model weights, download progress, and encoder reload flag.
pub struct EmbeddingModelState {
    pub model_config: ExgModelConfig,
    pub model_status: std::sync::Arc<std::sync::Mutex<EegModelStatus>>,
    #[allow(dead_code)]
    pub encoder_reload_requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl EmbeddingModelState {
    pub fn new(skill_dir: &std::path::Path) -> Self {
        Self {
            model_config: load_model_config(skill_dir),
            model_status: std::sync::Arc::new(std::sync::Mutex::new(EegModelStatus::default())),
            encoder_reload_requested: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            )),
        }
    }
}

// ── Full app state (Mutex-managed) ────────────────────────────────────────────

pub struct AppState {
    // ── Device session ────────────────────────────────────────────────────
    pub status: DeviceStatus,
    pub stream: Option<StreamHandle>,
    #[allow(dead_code)]
    pub scanner: Option<ScannerHandle>,
    pub discovered: Vec<DiscoveredDevice>,
    pub preferred_id: Option<String>,
    pub session_start_utc: Option<u64>,

    // ── Infrastructure ────────────────────────────────────────────────────
    pub skill_dir: std::path::PathBuf,
    pub logger: std::sync::Arc<SkillLogger>,

    // ── Grouped sub-states ────────────────────────────────────────────────
    pub shortcuts: ShortcutState,
    pub ui: UiPrefsState,
    pub input: InputTrackingState,
    pub embedding: EmbeddingModelState,

    // ── Calibration ───────────────────────────────────────────────────────
    pub calibration_profiles: Vec<CalibrationProfile>,
    pub active_calibration_id: String,
    #[allow(dead_code)]
    pub umap_config: UmapUserConfig,

    // ── Hooks ─────────────────────────────────────────────────────────────
    pub hooks: Vec<HookRule>,
    #[allow(dead_code)]
    pub hook_runtime:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,

    // ── Network / services ────────────────────────────────────────────────
    pub ws_host: String,
    pub ws_port: u16,
    pub api_token: String,
    pub hf_endpoint: String,
    pub update_check_interval_secs: u64,
    /// Set by the frontend when an update has been downloaded and is ready
    /// to install on next restart / relaunch.
    pub update_ready_to_install: bool,

    // ── Device configs ────────────────────────────────────────────────────
    pub openbci_config: crate::settings::OpenBciConfig,
    pub device_api_config: crate::settings::DeviceApiConfig,
    pub scanner_config: crate::settings::ScannerConfig,

    /// Location services enabled by the user (default false).
    pub location_enabled: bool,

    /// High-level inference device preference: `"gpu"` or `"cpu"`.
    pub inference_device: String,
    /// Last-saved `llm.n_gpu_layers` before a CPU override was applied.
    pub llm_gpu_layers_saved: u32,
    /// Inference device preference for EXG embeddings: `"gpu"` or `"cpu"`.
    pub exg_inference_device: String,

    // ── Smart alarm ────────────────────────────────────────────────────────

    // ── TTS ───────────────────────────────────────────────────────────────
    pub neutts_config: NeuttsConfig,
    pub tts_preload: bool,

    // ── Independently-locked sub-states ───────────────────────────────────
    pub dnd: std::sync::Arc<std::sync::Mutex<DndRuntimeState>>,
    pub llm: std::sync::Arc<std::sync::Mutex<LlmState>>,

    // ── Storage / recording ───────────────────────────────────────────────
    pub settings_storage_format: String,
    pub sleep_config: crate::settings::SleepConfig,
    pub screenshot_config: ScreenshotConfig,
}

// ── DND runtime state (independently locked) ──────────────────────────────────

/// Do-Not-Disturb runtime state.  Lives behind its own `Arc<Mutex<>>` so the
/// DND polling loop and session runner can access it without contending on the
/// main `AppState` lock.
#[derive(Default)]
#[allow(dead_code)]
pub struct DndRuntimeState {
    pub config: DoNotDisturbConfig,
    pub active: bool,
    pub os_active: Option<bool>,
    #[allow(dead_code)]
    pub last_error: Option<String>,
    pub focus_samples: std::collections::VecDeque<f64>,
    pub below_ticks: u32,
    #[allow(dead_code)]
    pub score_history: std::collections::VecDeque<f64>,
    #[allow(dead_code)]
    pub snr_low_ticks: u32,
}

// ── LLM sub-state (heap-allocated) ────────────────────────────────────────────

pub struct LlmState {
    pub config: crate::settings::LlmConfig,
    pub catalog: crate::llm::catalog::LlmCatalog,
    #[allow(dead_code)]
    pub logs: crate::llm::LlmLogBuffer,
    #[cfg(feature = "llm")]
    #[allow(dead_code)]
    pub loading: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "llm")]
    #[allow(dead_code)]
    pub start_error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl LlmState {
    pub fn new(skill_dir: &std::path::Path) -> Self {
        Self {
            config: crate::settings::LlmConfig::default(),
            logs: crate::llm::new_log_buffer(),
            catalog: crate::llm::catalog::LlmCatalog::load(skill_dir),
            #[cfg(feature = "llm")]
            loading: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(feature = "llm")]
            start_error: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        let skill_dir = default_skill_dir();
        let _ = std::fs::create_dir_all(&skill_dir);

        init_tts_dirs(&skill_dir);

        let log_config = crate::skill_log::load_log_config(&skill_dir);
        crate::skill_log::ensure_log_config(&skill_dir);
        let today_dir = skill_dir.join(yyyymmdd_utc());
        let log_path = today_dir.join(format!("log_{}.txt", unix_secs()));
        crate::skill_log::tee_stderr_to_file(&log_path);
        let logger = std::sync::Arc::new(SkillLogger::new(log_config));
        logger.write("logger", &format!("session log: {}", log_path.display()));

        let input = InputTrackingState::default();

        Self {
            status: DeviceStatus::default(),
            stream: None,
            scanner: None,
            discovered: Vec::new(),
            preferred_id: None,
            session_start_utc: None,

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
            hf_endpoint: skill_settings::default_hf_endpoint(),
            update_check_interval_secs: default_update_check_interval(),
            update_ready_to_install: false,
            openbci_config: crate::settings::OpenBciConfig::default(),
            location_enabled: false,
            inference_device: skill_settings::default_inference_device(),
            llm_gpu_layers_saved: skill_settings::default_llm_gpu_layers_saved(),
            exg_inference_device: skill_settings::default_exg_inference_device(),
            device_api_config: crate::settings::DeviceApiConfig::default(),
            scanner_config: crate::settings::ScannerConfig::default(),
            neutts_config: NeuttsConfig::default(),
            tts_preload: true,
            llm: std::sync::Arc::new(std::sync::Mutex::new(LlmState::new(&skill_dir))),
            skill_dir,
            logger,
            dnd: std::sync::Arc::new(std::sync::Mutex::new(DndRuntimeState::default())),
            settings_storage_format: "csv".into(),
            sleep_config: crate::settings::SleepConfig::default(),
            screenshot_config: ScreenshotConfig::default(),
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

    /// Obtain a clone of the `DndRuntimeState` arc for independent locking.
    #[allow(dead_code)]
    pub fn dnd_arc(&self) -> std::sync::Arc<std::sync::Mutex<DndRuntimeState>> {
        self.dnd.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_creation_does_not_panic() {
        // Test that we can create an AppState without panicking
        let state = AppState::default();
        // Only check that the path is non-empty, not that it exists
        assert!(!state.skill_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_app_state_new_boxed_does_not_panic() {
        // Test that the boxed creation works
        let state_boxed = AppState::new_boxed();
        let _state = &*state_boxed;
        // If we get here without panicking, the test passes
    }

    #[test]
    fn test_dnd_arc_cloning_works() {
        let state = AppState::default();
        let dnd_arc = state.dnd_arc();

        // Verify both references point to the same underlying data
        {
            let mut guard = state.dnd.lock().unwrap();
            guard.active = true;
        }
        {
            let guard = dnd_arc.lock().unwrap();
            assert!(guard.active, "cloned Arc should see the mutation");
        }
    }
}
