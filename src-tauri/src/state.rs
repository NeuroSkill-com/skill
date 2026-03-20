// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Core shared types: `AppState`, `DeviceStatus`, IPC packet structs, handles.

#[cfg(not(feature = "llm"))]
use std::sync::Mutex;

use serde::Serialize;
use tauri::ipc::Channel;

use crate::constants::{EEG_CHANNELS, EMBEDDING_OVERLAP_SECS};
use skill_eeg::eeg_bands::BandSnapshot;
use skill_eeg::eeg_filter::FilterConfig;
use skill_eeg::eeg_model_config::{EegModelConfig, EegModelStatus, load_model_config};
use skill_eeg::eeg_quality::SignalQuality;
use crate::active_window::ActiveWindowInfo;
use skill_data::activity_store::ActivityStore;
use crate::screenshot;
use skill_data::screenshot_store;
use crate::skill_log::SkillLogger;
use crate::settings::{
    CalibrationProfile, DoNotDisturbConfig, HookLastTrigger, HookRule,
    NeuttsConfig, ScreenshotConfig, UmapUserConfig,
    default_skill_dir, default_label_shortcut, default_search_shortcut,
    default_settings_shortcut, default_calibration_shortcut, default_help_shortcut,
    default_history_shortcut, default_api_shortcut, default_theme_shortcut,
    default_focus_timer_shortcut, default_theme, default_accent_color,
    default_daily_goal_min, default_embedding_model, default_ws_host, default_ws_port,
    default_update_check_interval, default_track_active_window, default_track_input_activity,
    load_umap_config,
};
use crate::tts::init_tts_dirs;
use skill_data::label_store;
use crate::{unix_secs, yyyymmdd_utc};

#[cfg(feature = "llm")]
use crate::settings::default_chat_shortcut;

// Re-export from skill-data (canonical definition).
pub use skill_data::device::PairedDevice;

// ── Runtime-only discovered device ────────────────────────────────────────────

#[derive(Clone, Debug, Serialize)]
pub struct DiscoveredDevice {
    pub id:           String,
    pub name:         String,
    pub last_seen:    u64,
    pub last_rssi:    i16,
    pub is_paired:    bool,
    pub is_preferred: bool,
    /// How this device was discovered (ble, usb_serial, wifi, cortex).
    pub transport:    crate::device_scanner::Transport,
}

// ── EEG / PPG / IMU IPC packets ───────────────────────────────────────────────

/// EEG packet forwarded to the frontend for live visualisation.
#[derive(Clone, Serialize)]
pub struct EegPacket {
    pub electrode: usize,
    pub samples:   Vec<f64>,
    pub timestamp: f64,
}

/// PPG packet forwarded to the frontend for live visualisation.
#[derive(Clone, Serialize)]
pub struct PpgPacket {
    pub channel:   usize,
    pub samples:   Vec<f64>,
    pub timestamp: f64,
}

/// IMU packet forwarded to the frontend via Tauri IPC channel.
#[derive(Clone, Serialize)]
pub struct ImuPacket {
    pub sensor:    String,
    pub samples:   [[f32; 3]; 3],
    pub timestamp: f64,
}

// ── Session / scanner handles ─────────────────────────────────────────────────

pub struct StreamHandle  { pub cancel_tx: tokio::sync::oneshot::Sender<()> }
pub struct ScannerHandle { pub cancel_tx: tokio::sync::oneshot::Sender<()> }

// ── Shared frontend-visible status ────────────────────────────────────────────

#[derive(Clone, Serialize)]
pub struct DeviceStatus {
    pub state:               String,
    pub device_name:         Option<String>,
    pub device_id:           Option<String>,
    pub serial_number:       Option<String>,
    pub mac_address:         Option<String>,
    pub firmware_version:    Option<String>,
    pub hardware_version:    Option<String>,
    pub bootloader_version:  Option<String>,
    pub headset_preset:      Option<String>,
    pub csv_path:            Option<String>,
    pub sample_count:        u64,
    pub battery:             f32,
    pub eeg:                 Vec<f64>,
    pub paired_devices:      Vec<PairedDevice>,
    pub device_error:            Option<String>,
    pub target_name:         Option<String>,
    pub filter_config:       FilterConfig,
    pub channel_quality:     Vec<SignalQuality>,
    pub embedding_overlap_secs: f32,
    pub retry_attempt:       u32,
    pub retry_countdown_secs: u32,
    pub ppg:                 Vec<f64>,
    pub ppg_sample_count:    u64,
    pub accel:               [f32; 3],
    pub gyro:                [f32; 3],
    pub fuel_gauge_mv:       f32,
    pub temperature_raw:     u16,
    pub device_kind:         String,
    /// Channel labels for the connected device (set at session start).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub channel_names:       Vec<String>,
    /// Hardware EEG channel count of the connected device.
    pub eeg_channel_count:   usize,
    /// Hardware EEG sample rate of the connected device (Hz).
    pub eeg_sample_rate_hz:  f64,
    /// Device has a PPG (heart-rate) sensor.
    pub has_ppg:             bool,
    /// Device has an IMU (accelerometer + gyroscope).
    pub has_imu:             bool,
    /// Device has electrodes at central scalp sites (C3/C4/Cz).
    pub has_central_electrodes: bool,
    /// Device supports a full 10-20 montage (or superset).
    pub has_full_montage:    bool,
}

impl Default for DeviceStatus {
    fn default() -> Self {
        Self {
            state:              "disconnected".into(),
            device_name:        None,
            device_id:          None,
            serial_number:      None,
            mac_address:        None,
            firmware_version:   None,
            hardware_version:   None,
            bootloader_version: None,
            headset_preset:     None,
            csv_path:           None,
            sample_count:       0,
            battery:            0.0,
            eeg:                vec![f64::NAN; EEG_CHANNELS],
            paired_devices:     Vec::new(),
            device_error:           None,
            target_name:        None,
            filter_config:      FilterConfig::default(),
            channel_quality:    Vec::new(),
            embedding_overlap_secs: EMBEDDING_OVERLAP_SECS,
            retry_attempt:      0,
            retry_countdown_secs: 0,
            ppg:                vec![0.0; 3],
            ppg_sample_count:   0,
            accel:              [0.0; 3],
            gyro:               [0.0; 3],
            fuel_gauge_mv:      0.0,
            temperature_raw:    0,
            device_kind:        "unknown".into(),
            channel_names:      Vec::new(),
            eeg_channel_count:  0,
            eeg_sample_rate_hz: 0.0,
            has_ppg:            false,
            has_imu:            false,
            has_central_electrodes: false,
            has_full_montage:   false,
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
        self.state               = new_state.into();
        self.device_name         = None;
        self.device_id           = None;
        self.device_kind         = "unknown".into();
        self.serial_number       = None;
        self.mac_address         = None;
        self.firmware_version    = None;
        self.hardware_version    = None;
        self.bootloader_version  = None;
        self.headset_preset      = None;
        self.battery             = 0.0;
        self.eeg                 = vec![f64::NAN; EEG_CHANNELS];
        self.ppg                 = vec![0.0; 3];
        self.ppg_sample_count    = 0;
        self.device_error            = None;
        self.target_name         = None;
        self.retry_attempt       = 0;
        self.retry_countdown_secs = 0;
        self.channel_quality     = Vec::new();
        self.channel_names       = Vec::new();
        self.eeg_channel_count   = 0;
        self.eeg_sample_rate_hz  = 0.0;
        self.has_ppg             = false;
        self.has_imu             = false;
        self.has_central_electrodes = false;
        self.has_full_montage    = false;
    }

    /// Reset transient fields for a new scanning cycle.
    pub fn reset_for_scanning(
        &mut self,
        device_kind: &str,
        csv_path: &std::path::Path,
        preferred_id: Option<&str>,
    ) {
        self.state               = "scanning".into();
        self.device_kind         = device_kind.into();
        self.device_name         = None;
        self.device_id           = None;
        self.serial_number       = None;
        self.mac_address         = None;
        self.firmware_version    = None;
        self.hardware_version    = None;
        self.bootloader_version  = None;
        self.headset_preset      = None;
        self.csv_path            = Some(csv_path.to_string_lossy().into_owned());
        self.device_error            = None;
        self.battery             = 0.0;
        self.eeg                 = vec![f64::NAN; EEG_CHANNELS];
        self.sample_count        = 0;
        self.ppg                 = vec![0.0; 3];
        self.ppg_sample_count    = 0;
        self.target_name         = preferred_id.and_then(|id| {
            self.paired_devices.iter().find(|d| d.id == id).map(|d| d.name.clone())
        });
        // Populate capability flags from the device kind.
        self.apply_capabilities_from_kind();
    }

    /// Derive and set capability booleans from the current `device_kind` string.
    pub fn apply_capabilities_from_kind(&mut self) {
        use skill_data::device::DeviceKind;
        let kind = DeviceKind::from_kind_str(&self.device_kind);
        let caps = kind.capabilities();
        self.has_ppg                = caps.has_ppg;
        self.has_imu                = caps.has_imu;
        self.has_central_electrodes = caps.has_central_electrodes;
        self.has_full_montage       = caps.has_full_montage;
    }
}

// ── Sub-state: keyboard shortcuts ─────────────────────────────────────────────

/// All user-configurable global keyboard shortcuts.
///
/// Extracted from `AppState` so shortcut logic can be read/written without
/// contending on unrelated fields.
pub struct ShortcutState {
    pub label_shortcut:       String,
    pub search_shortcut:      String,
    pub settings_shortcut:    String,
    pub calibration_shortcut: String,
    pub help_shortcut:        String,
    pub history_shortcut:     String,
    pub api_shortcut:         String,
    pub theme_shortcut:       String,
    pub focus_timer_shortcut: String,
    #[cfg(feature = "llm")]
    pub chat_shortcut:        String,
}

impl Default for ShortcutState {
    fn default() -> Self {
        Self {
            label_shortcut:       default_label_shortcut(),
            search_shortcut:      default_search_shortcut(),
            settings_shortcut:    default_settings_shortcut(),
            calibration_shortcut: default_calibration_shortcut(),
            help_shortcut:        default_help_shortcut(),
            history_shortcut:     default_history_shortcut(),
            api_shortcut:         default_api_shortcut(),
            theme_shortcut:       default_theme_shortcut(),
            focus_timer_shortcut: default_focus_timer_shortcut(),
            #[cfg(feature = "llm")]
            chat_shortcut:        default_chat_shortcut(),
        }
    }
}

// ── Sub-state: UI preferences ─────────────────────────────────────────────────

/// User-facing appearance and onboarding preferences.
pub struct UiPrefsState {
    pub theme:        String,
    pub language:     String,
    pub accent_color: String,
    pub daily_goal_min: u32,
    pub goal_notified_date: String,
    pub onboarding_complete: bool,
    pub last_seen_whats_new_version: String,
    pub text_embedding_model: String,
}

impl Default for UiPrefsState {
    fn default() -> Self {
        Self {
            theme:                       default_theme(),
            language:                    String::new(),
            accent_color:                default_accent_color(),
            daily_goal_min:              default_daily_goal_min(),
            goal_notified_date:          String::new(),
            onboarding_complete:         false,
            last_seen_whats_new_version: String::new(),
            text_embedding_model:        default_embedding_model(),
        }
    }
}

// ── Sub-state: input / activity tracking ──────────────────────────────────────

/// Keyboard, mouse and active-window tracking state.
///
/// The `Arc<Atomic*>` fields are shared with background threads (input
/// monitor, active-window poller) that update them without locking `AppState`.
pub struct InputTrackingState {
    pub track_active_window:    bool,
    pub current_active_window:  Option<ActiveWindowInfo>,
    pub track_input_activity:   bool,
    pub input_activity_enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub last_keyboard_ts:       std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub last_mouse_ts:          std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub kbd_event_count:        std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub mouse_event_count:      std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub activity_store:         Option<std::sync::Arc<ActivityStore>>,
}

impl Default for InputTrackingState {
    fn default() -> Self {
        Self {
            track_active_window:    default_track_active_window(),
            current_active_window:  None,
            track_input_activity:   default_track_input_activity(),
            input_activity_enabled: std::sync::Arc::new(
                std::sync::atomic::AtomicBool::new(default_track_input_activity())
            ),
            last_keyboard_ts:  std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_mouse_ts:     std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            kbd_event_count:   std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            mouse_event_count: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            activity_store:    None,
        }
    }
}

// ── Sub-state: EEG embedding model ────────────────────────────────────────────

/// EEG model weights, download progress, and encoder reload flag.
pub struct EmbeddingModelState {
    pub model_config:             EegModelConfig,
    pub model_status:             std::sync::Arc<std::sync::Mutex<EegModelStatus>>,
    pub download_cancel:          std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub encoder_reload_requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl EmbeddingModelState {
    pub fn new(skill_dir: &std::path::Path) -> Self {
        Self {
            model_config:             load_model_config(skill_dir),
            model_status:             std::sync::Arc::new(std::sync::Mutex::new(EegModelStatus::default())),
            download_cancel:          std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            encoder_reload_requested: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

// ── Full app state (Mutex-managed) ────────────────────────────────────────────

pub struct AppState {
    // ── Device session ────────────────────────────────────────────────────
    pub status:       DeviceStatus,
    pub stream:       Option<StreamHandle>,
    pub scanner:      Option<ScannerHandle>,
    pub discovered:   Vec<DiscoveredDevice>,
    pub preferred_id: Option<String>,
    pub eeg_channel:  Option<Channel<EegPacket>>,
    pub ppg_channel:  Option<Channel<PpgPacket>>,
    pub imu_channel:  Option<Channel<ImuPacket>>,
    pub battery_ema:  Option<f32>,
    pub latest_bands: Option<BandSnapshot>,
    pub pending_reconnect: bool,
    pub retry_attempt: u32,
    pub session_start_utc: Option<u64>,

    // ── Infrastructure ────────────────────────────────────────────────────
    pub skill_dir:        std::path::PathBuf,
    pub logger:           std::sync::Arc<SkillLogger>,
    pub label_store:      Option<label_store::LabelStore>,

    // ── Grouped sub-states ────────────────────────────────────────────────
    pub shortcuts:    ShortcutState,
    pub ui:           UiPrefsState,
    pub input:        InputTrackingState,
    pub embedding:    EmbeddingModelState,

    // ── Calibration ───────────────────────────────────────────────────────
    pub calibration_profiles: Vec<CalibrationProfile>,
    pub active_calibration_id: String,
    pub umap_config: UmapUserConfig,

    // ── Hooks ─────────────────────────────────────────────────────────────
    pub hooks: Vec<HookRule>,
    pub hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,

    // ── Network / services ────────────────────────────────────────────────
    pub ws_host: String,
    pub ws_port: u16,
    pub update_check_interval_secs: u64,

    // ── Device configs ────────────────────────────────────────────────────
    pub openbci_config:    crate::settings::OpenBciConfig,
    pub device_api_config: crate::settings::DeviceApiConfig,
    pub scanner_config:    crate::settings::ScannerConfig,

    // ── TTS ───────────────────────────────────────────────────────────────
    pub neutts_config: NeuttsConfig,
    pub tts_preload:   bool,

    // ── Independently-locked sub-states ───────────────────────────────────
    pub dnd: std::sync::Arc<std::sync::Mutex<DndRuntimeState>>,
    pub llm: std::sync::Arc<std::sync::Mutex<LlmState>>,

    // ── Storage / recording ───────────────────────────────────────────────
    pub settings_storage_format: String,
    pub sleep_config:       crate::settings::SleepConfig,
    pub screenshot_config:  ScreenshotConfig,
    pub screenshot_store: Option<std::sync::Arc<screenshot_store::ScreenshotStore>>,
    pub screenshot_metrics: std::sync::Arc<screenshot::ScreenshotMetrics>,
    pub health_store: Option<std::sync::Arc<skill_data::health_store::HealthStore>>,
}

// ── DND runtime state (independently locked) ──────────────────────────────────

/// Do-Not-Disturb runtime state.  Lives behind its own `Arc<Mutex<>>` so the
/// DND polling loop and session runner can access it without contending on the
/// main `AppState` lock.
pub struct DndRuntimeState {
    pub config:         DoNotDisturbConfig,
    pub active:         bool,
    pub os_active:      Option<bool>,
    pub focus_samples:  std::collections::VecDeque<f64>,
    pub below_ticks:    u32,
    pub score_history:  std::collections::VecDeque<f64>,
    pub snr_low_ticks:  u32,
}

impl Default for DndRuntimeState {
    fn default() -> Self {
        Self {
            config:         DoNotDisturbConfig::default(),
            active:         false,
            os_active:      None,
            focus_samples:  std::collections::VecDeque::new(),
            below_ticks:    0,
            score_history:  std::collections::VecDeque::new(),
            snr_low_ticks:  0,
        }
    }
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
            config:     crate::settings::LlmConfig::default(),
            downloads:  std::collections::HashMap::new(),
            logs:       crate::llm::new_log_buffer(),
            state_cell: crate::llm::new_state_cell(),
            loading:    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            start_error: std::sync::Arc::new(std::sync::Mutex::new(None)),
            catalog:    crate::llm::catalog::LlmCatalog::load(skill_dir),
            chat_store: crate::llm::chat_store::ChatStore::open(skill_dir),
        }
    }

    /// Create a new `LlmState` when the `llm` feature is disabled.
    #[cfg(not(feature = "llm"))]
    pub fn new(_skill_dir: &std::path::Path) -> Self {
        Self {
            config:     crate::settings::LlmConfig::default(),
            state_cell: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        let skill_dir = default_skill_dir();
        let _ = std::fs::create_dir_all(&skill_dir);

        init_tts_dirs(&skill_dir);

        let health_store = skill_data::health_store::HealthStore::open(&skill_dir).map(std::sync::Arc::new);

        let log_config = crate::skill_log::load_log_config(&skill_dir);
        crate::skill_log::ensure_log_config(&skill_dir);
        let today_dir = skill_dir.join(yyyymmdd_utc());
        let log_path  = today_dir.join(format!("log_{}.txt", unix_secs()));
        crate::skill_log::tee_stderr_to_file(&log_path);
        let logger = std::sync::Arc::new(SkillLogger::new(log_config));

        let mut input = InputTrackingState::default();
        input.activity_store = ActivityStore::open(&skill_dir).map(std::sync::Arc::new);

        Self {
            status:            DeviceStatus::default(),
            stream:            None,
            scanner:           None,
            discovered:        Vec::new(),
            preferred_id:      None,
            eeg_channel:       None,
            ppg_channel:       None,
            imu_channel:       None,
            battery_ema:       None,
            latest_bands:      None,
            pending_reconnect: false,
            retry_attempt:     0,
            session_start_utc: None,
            label_store: label_store::LabelStore::open(&skill_dir),

            shortcuts: ShortcutState::default(),
            ui:        UiPrefsState::default(),
            input,
            embedding: EmbeddingModelState::new(&skill_dir),

            calibration_profiles: vec![CalibrationProfile::default()],
            active_calibration_id: "default".into(),
            umap_config: load_umap_config(&skill_dir),
            hooks: Vec::new(),
            hook_runtime: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            ws_host: default_ws_host(),
            ws_port: default_ws_port(),
            update_check_interval_secs: default_update_check_interval(),
            openbci_config: crate::settings::OpenBciConfig::default(),
            device_api_config: crate::settings::DeviceApiConfig::default(),
            scanner_config: crate::settings::ScannerConfig::default(),
            neutts_config: NeuttsConfig::default(),
            tts_preload:   true,
            llm: std::sync::Arc::new(std::sync::Mutex::new(LlmState::new(&skill_dir))),
            skill_dir,
            logger,
            dnd: std::sync::Arc::new(std::sync::Mutex::new(DndRuntimeState::default())),
            settings_storage_format: "csv".into(),
            sleep_config:       crate::settings::SleepConfig::default(),
            screenshot_config:  ScreenshotConfig::default(),
            screenshot_store:   None,
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
