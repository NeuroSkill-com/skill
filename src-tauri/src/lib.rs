// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
mod constants;
use constants::{EEG_CHANNELS, EMBEDDING_OVERLAP_SECS};

#[macro_use]
mod skill_log;
use skill_log::SkillLogger;

/// Convenience wrapper around [`skill_log!`] for code that holds an
/// `&AppHandle` but not a direct reference to the logger.
///
/// Requires `Arc<SkillLogger>` to be registered as Tauri managed state
/// (done once in `run()` → `setup`).
///
/// ```rust
/// app_log!(app, "bluetooth", "connected: {name}");
/// ```
macro_rules! app_log {
    ($app:expr, $tag:literal, $($arg:tt)*) => {{
        let _lg = $app.state::<std::sync::Arc<SkillLogger>>();
        skill_log!(_lg, $tag, $($arg)*);
    }};
}

/// GPU stats reading is macOS-only (IOKit + CoreFoundation frameworks).
#[cfg(target_os = "macos")]
mod gpu_stats;

mod eeg_model_config;
use eeg_model_config::{
    EegModelConfig, EegModelStatus,
    load_model_config,
};

mod eeg_embeddings;
use eeg_embeddings::EegAccumulator;

mod eeg_filter;
use eeg_filter::{EegFilter, FilterConfig, SpectrogramColumn};

mod eeg_bands;
use eeg_bands::{BandAnalyzer, BandSnapshot};

mod eeg_quality;
use eeg_quality::{QualityMonitor, SignalQuality};

mod commands;
mod job_queue;
mod label_store;
mod artifact_detection;
mod head_pose;
mod ppg_analysis;
mod ws_commands;
mod label_index;
mod ws_server;
mod api;

use ws_server::WsBroadcaster;

// ── Extracted modules ─────────────────────────────────────────────────────────

mod autostart;

mod tts;
pub mod device;
use tts::{tts_init, tts_speak, tts_unload, tts_list_voices, tts_list_neutts_voices, tts_get_voice, tts_set_voice};
pub(crate) use tts::{neutts_apply_config, init_tts_dirs, init_neutts_samples_dir,
                     init_espeak_bundled_data_path, tts_shutdown};

mod settings;
pub(crate) use settings::{
    UmapUserConfig, CalibrationAction, CalibrationProfile, CalibrationConfig, new_profile_id,
    load_umap_config, load_settings, settings_path,
    default_skill_dir, tilde_path, expand_tilde,
    default_label_shortcut, default_search_shortcut, default_settings_shortcut,
    default_calibration_shortcut, default_help_shortcut, default_history_shortcut,
    default_api_shortcut, default_theme_shortcut, default_focus_timer_shortcut,
    default_theme, default_daily_goal_min, default_embedding_model,
    default_ws_host, default_ws_port, default_update_check_interval, UserSettings,
    NeuttsConfig,
};

mod tray;
pub(crate) use tray::{refresh_tray, build_menu, icon_disconnected};

mod shortcut_cmds;
pub(crate) use shortcut_cmds::apply_all_shortcuts;
use shortcut_cmds::{
    get_label_shortcut, set_label_shortcut,
    get_search_shortcut, set_search_shortcut,
    get_settings_shortcut, set_settings_shortcut,
    get_calibration_shortcut, set_calibration_shortcut,
    get_help_shortcut, set_help_shortcut,
    get_history_shortcut, set_history_shortcut,
    get_api_shortcut, set_api_shortcut,
    get_theme_shortcut, set_theme_shortcut,
    get_focus_timer_shortcut, set_focus_timer_shortcut,
};

mod about;
use about::{get_about_info, open_about_window};

mod window_cmds;
pub(crate) use window_cmds::open_calibration_window_inner;
use window_cmds::{
    open_bt_settings, open_settings_window, open_updates_window, open_model_tab, open_help_window,
    open_search_window, open_session_window, open_label_window, open_labels_window,
    open_focus_timer_window, open_api_window, open_onboarding_window,
    complete_onboarding, get_onboarding_complete, close_label_window,
    open_calibration_window, open_and_start_calibration, close_calibration_window,
    list_calibration_profiles, get_calibration_profile, get_active_calibration,
    set_active_calibration, create_calibration_profile, update_calibration_profile,
    delete_calibration_profile, record_calibration_completed,
    get_calibration_config, set_calibration_config,
    emit_calibration_event, quit_app, get_app_version, get_app_name,
    get_data_dir, set_data_dir, get_ws_clients, get_ws_request_log, get_ws_port,
};

mod label_cmds;
pub(crate) use label_cmds::{EmbedderState, init_embedder};
use label_cmds::{
    query_annotations, delete_label, update_label, get_queue_stats, submit_label,
    list_embedding_models, get_embedding_model, set_embedding_model,
    reembed_all_labels, get_stale_label_count,
    rebuild_label_index, search_labels_by_text, search_labels_by_eeg,
};

mod settings_cmds;
use settings_cmds::{
    subscribe_eeg, subscribe_ppg, subscribe_imu,
    get_status, get_devices, set_preferred_device, forget_device, cancel_retry, retry_connect,
    get_filter_config, set_filter_config, set_notch_preset,
    get_latest_bands, get_embedding_overlap, set_embedding_overlap,
    get_gpu_stats, get_log_config, set_log_config,
    get_eeg_model_config, set_eeg_model_config, get_eeg_model_status,
    trigger_weights_download, cancel_weights_download,
    get_umap_config, set_umap_config, get_theme_and_language, set_theme, set_language,
    get_daily_goal, set_daily_goal, get_goal_notified_date, set_goal_notified_date,
    get_daily_recording_mins,
    get_ws_config, set_ws_config,
    get_autostart_enabled, set_autostart_enabled,
    get_update_check_interval, set_update_check_interval,
    get_openbci_config, set_openbci_config, list_serial_ports,
    get_neutts_config, set_neutts_config, pick_ref_wav_file,
    get_tts_preload, set_tts_preload,
};

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use btleplug::api::{Central, CentralEvent, CentralState, Manager as BtManager,
                    Peripheral as BtPeripheral, ScanFilter};
use btleplug::platform::{Adapter as BtPlatformAdapter, Manager as BtPlatformManager};
use futures_util::StreamExt;
use muse_rs::prelude::*;
use openbci::board::ganglion::{GanglionBoard, GanglionConfig};
use openbci::board::cyton::CytonBoard;
use openbci::board::cyton_daisy::CytonDaisyBoard;
use openbci::board::cyton_wifi::{CytonWifiBoard, CytonWifiConfig};
use openbci::board::cyton_daisy_wifi::{CytonDaisyWifiBoard, CytonDaisyWifiConfig};
use openbci::board::ganglion_wifi::{GanglionWifiBoard, GanglionWifiConfig};
use openbci::board::galea::GaleaBoard;
use openbci::board::Board as OpenBciBoard;
use serde::{Deserialize, Serialize};
use tauri::{
    ipc::Channel,
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};
use tauri_plugin_notification::NotificationExt;

// ── Mutex poison recovery ─────────────────────────────────────────────────────

/// Extension trait that recovers gracefully from a poisoned [`std::sync::Mutex`]
/// instead of re-panicking on every subsequent `.lock()` call.
///
/// # Why not plain `.unwrap()`?
///
/// Rust poisons a `Mutex` whenever a thread panics while holding it.  A bare
/// `.lock().unwrap()` then re-panics on *every* subsequent acquisition —
/// turning one background-thread crash into a total process freeze.
///
/// # Recovery strategy
///
/// `.lock_or_recover()` extracts the inner guard from the [`PoisonError`] via
/// `.into_inner()`, giving callers access to whatever state was present at
/// crash time, and logs a warning so the incident is visible in the log file.
///
/// This trade-off is appropriate here because:
/// * All durable data (labels, embeddings, settings) is also persisted to
///   disk, so a brief window of slightly-stale in-memory state is acceptable.
/// * The UI must stay responsive even if a background EEG/PPG processing
///   thread crashes — a cascading panic would lock the entire app.
/// * The poisoning thread is already logged separately (via the stderr tee);
///   no diagnostic information is lost.
pub(crate) trait MutexExt<T> {
    /// Acquire the lock, recovering the guard even if the mutex is poisoned.
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for std::sync::Mutex<T> {
    #[inline]
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poison| {
            eprintln!(
                "[mutex] WARNING: recovered from poisoned lock at {}:{}; \
                 a previous thread panicked while holding this lock. \
                 State may be inconsistent — check the log file.",
                file!(), line!()
            );
            poison.into_inner()
        })
    }
}

// ── Persistent data structure (written to disk) ───────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PairedDevice {
    pub id:        String,
    pub name:      String,
    pub last_seen: u64,
}

// ── Runtime-only discovered device (merged from scan + paired list) ───────────

#[derive(Clone, Debug, Serialize)]
pub struct DiscoveredDevice {
    pub id:           String,
    pub name:         String,
    pub last_seen:    u64,   // unix seconds
    pub last_rssi:    i16,   // dBm (0 = unknown/not-in-range)
    pub is_paired:    bool,
    pub is_preferred: bool,
}

// ── EEG packet forwarded to the frontend for live visualisation ───────────────

#[derive(Clone, Serialize)]
pub struct EegPacket {
    electrode: usize,
    samples:   Vec<f64>,
    timestamp: f64,
}

/// PPG packet forwarded to the frontend for live visualisation.
/// Carries raw 24-bit ADC values for one of the 3 optical channels.
#[derive(Clone, Serialize)]
pub struct PpgPacket {
    /// 0 = ambient, 1 = infrared, 2 = red
    channel:   usize,
    samples:   Vec<f64>,
    timestamp: f64,
}

/// A batch of IMU samples forwarded to the frontend via Tauri IPC channel.
#[derive(Clone, Serialize)]
pub struct ImuPacket {
    /// "accel" or "gyro"
    sensor:    String,
    /// 3 XYZ samples per BLE notification
    samples:   [[f32; 3]; 3],
    timestamp: f64,
}

// ── Live streaming / scanning handles ────────────────────────────────────────

pub struct StreamHandle  { pub cancel_tx: tokio::sync::oneshot::Sender<()> }
pub struct ScannerHandle { pub cancel_tx: tokio::sync::oneshot::Sender<()> }

// ── Shared frontend-visible status ───────────────────────────────────────────

#[derive(Clone, Serialize)]
pub struct MuseStatus {
    pub state:          String,  // disconnected | scanning | connected | bt_off
    pub device_name:    Option<String>,
    pub device_id:      Option<String>,
    /// Factory serial number from the headset control channel (`"sn"` field).
    /// Arrives a few seconds after connection in the device-info response.
    /// Example: `"AAAA-BBBB-CCCC"`.
    pub serial_number:  Option<String>,
    /// Hardware MAC address from the headset control channel (`"ma"` field).
    /// Example: `"AA-BB-CC-DD-EE-FF"`.
    pub mac_address:    Option<String>,
    /// Firmware version string from the headset control channel (`"fw"` field).
    /// Example: `"1.2.6"`.
    pub firmware_version: Option<String>,
    /// Hardware version string from the headset control channel (`"hw"` field).
    /// Example: `"10.0"`.
    pub hardware_version: Option<String>,
    /// Bootloader version from the headset control channel (`"bl"` field).
    pub bootloader_version: Option<String>,
    /// Headset preset / type from the headset control channel (`"tp"` field).
    /// Example: `"p21"` (Muse 2), `"p50"` (Muse S).
    pub headset_preset: Option<String>,
    pub csv_path:       Option<String>,
    pub sample_count:   u64,
    pub battery:        f32,
    pub eeg:            Vec<f64>,
    pub paired_devices: Vec<PairedDevice>,
    pub bt_error:       Option<String>,
    pub target_name:    Option<String>,
    /// Current EEG filter configuration (mirrored here so the frontend
    /// receives it alongside every status update without a separate call).
    pub filter_config:         FilterConfig,
    /// Per-channel signal quality, updated every filter hop (~8 Hz).
    /// One entry per channel in electrode order [TP9, AF7, AF8, TP10].
    pub channel_quality:       Vec<SignalQuality>,
    /// Overlap between consecutive ZUNA embedding epochs (seconds).
    /// Configurable in Settings; persisted across restarts.
    pub embedding_overlap_secs: f32,
    /// Current auto-retry attempt number (0 = not retrying).
    pub retry_attempt: u32,
    /// Seconds remaining until the next auto-retry (0 = not counting down).
    pub retry_countdown_secs: u32,
    /// Latest raw PPG values [ambient, infrared, red] (last sample of each channel).
    pub ppg: Vec<f64>,
    /// PPG sample count (total across all 3 channels).
    pub ppg_sample_count: u64,
    /// Latest accelerometer reading [x, y, z] in g.
    pub accel: [f32; 3],
    /// Latest gyroscope reading [x, y, z] in °/s.
    pub gyro: [f32; 3],
    /// Battery fuel-gauge voltage in mV (Classic firmware only; 0 on Athena).
    pub fuel_gauge_mv: f32,
    /// Raw temperature ADC value (Classic firmware only; 0 on Athena).
    pub temperature_raw: u16,
    /// Which device family is connected: "muse" | "ganglion" | "unknown".
    /// Used by the frontend to adapt the UI (hide PPG/battery for Ganglion, etc.)
    pub device_kind: String,
}

impl Default for MuseStatus {
    fn default() -> Self {
        Self {
            state:         "disconnected".into(),
            device_name:   None,
            device_id:     None,
            serial_number: None,
            mac_address:   None,
            firmware_version:  None,
            hardware_version:  None,
            bootloader_version: None,
            headset_preset:    None,
            csv_path:      None,
            sample_count:  0,
            battery:       0.0,
            eeg:           vec![f64::NAN; 4],
            paired_devices: Vec::new(),
            bt_error:      None,
            target_name:   None,
            filter_config:          FilterConfig::default(),
            channel_quality:        vec![SignalQuality::default(); 4],
            embedding_overlap_secs: EMBEDDING_OVERLAP_SECS,
            retry_attempt:         0,
            retry_countdown_secs:  0,
            ppg:                   vec![0.0; 3],
            ppg_sample_count:      0,
            accel:                 [0.0; 3],
            gyro:                  [0.0; 3],
            fuel_gauge_mv:         0.0,
            temperature_raw:       0,
            device_kind:           "unknown".into(),
        }
    }
}

// ── Full app state (Mutex-managed) ────────────────────────────────────────────

pub struct AppState {
    pub status:       MuseStatus,
    pub stream:       Option<StreamHandle>,
    pub scanner:      Option<ScannerHandle>,
    pub discovered:   Vec<DiscoveredDevice>,
    pub preferred_id: Option<String>,
    /// Dedicated Rust→JS pipe for high-frequency EEG packets.
    /// Set by the `subscribe_eeg` command; persists for the window lifetime.
    pub eeg_channel:  Option<Channel<EegPacket>>,
    /// Dedicated Rust→JS pipe for PPG optical packets.
    /// Set by the `subscribe_ppg` command; persists for the window lifetime.
    pub ppg_channel:  Option<Channel<PpgPacket>>,
    /// Dedicated Rust→JS pipe for IMU (accelerometer + gyroscope) packets.
    /// Set by the `subscribe_imu` command; persists for the window lifetime.
    pub imu_channel:  Option<Channel<ImuPacket>>,
    /// EMA accumulator for battery smoothing (not sent to frontend directly).
    pub battery_ema:  Option<f32>,
    /// GPU-accelerated EEG filter (overlap-save, all 4 channels batched).
    pub filter:        EegFilter,
    /// GPU-accelerated EEG band power analyzer (512-sample Hann FFT, 4 ch batch).
    pub band_analyzer: BandAnalyzer,
    /// Lightweight rolling-window signal quality monitor (raw samples, no GPU).
    pub quality: QualityMonitor,
    /// When true, a non-BT session failure silently retries instead of
    /// showing the "No Muse headset found nearby" error card.  Set by BT
    /// restoration and by the manual Retry button; cleared on BT-off,
    /// explicit cancel, or successful device discovery.
    pub pending_reconnect: bool,
    /// Number of consecutive retry attempts (0 = first try, resets on connect).
    pub retry_attempt: u32,
    /// Per-channel sample accumulator that feeds 5-second epochs to the ZUNA
    /// wgpu encoder and persists embeddings in the HNSW index at ~/.skill/.
    pub accumulator: EegAccumulator,
    /// Root data directory for all persisted skill data (`~/.skill/`).
    pub skill_dir: std::path::PathBuf,
    /// Persisted EEG model configuration (HNSW params, HF repo, data_norm).
    pub model_config: EegModelConfig,
    /// Live status of the background embed worker, shared via `Arc<Mutex>`.
    pub model_status: std::sync::Arc<std::sync::Mutex<EegModelStatus>>,
    /// Cancellation flag for ZUNA weight downloads.  Set to `true` by the
    /// `cancel_weights_download` command; reset to `false` before each new
    /// download attempt.  Shared with the embed worker thread.
    pub download_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Structured logger: per-subsystem config + daily log file writer.
    pub logger: std::sync::Arc<SkillLogger>,
    /// Unix-second timestamp of the current session's start (set when a Muse
    /// session is initiated; used to populate the CSV companion metadata JSON).
    pub session_start_utc: Option<u64>,
    /// Persistent label store (`~/.skill/labels.sqlite`).
    pub label_store: Option<label_store::LabelStore>,
    /// Currently registered global shortcut accelerator string (empty = none).
    pub label_shortcut:       String,
    /// Global shortcut for the search window.
    pub search_shortcut:      String,
    /// Global shortcut for the settings window.
    pub settings_shortcut:    String,
    /// Global shortcut for the calibration window.
    pub calibration_shortcut: String,
    /// Global shortcut for the help window.
    pub help_shortcut:        String,
    /// Global shortcut for the history window.
    pub history_shortcut:     String,
    /// Global shortcut for the API status window.
    pub api_shortcut:         String,
    /// Global shortcut to toggle dark/light theme.
    pub theme_shortcut:       String,
    /// Global shortcut to open the focus timer / Pomodoro window.
    pub focus_timer_shortcut: String,
    /// All saved calibration profiles.
    pub calibration_profiles: Vec<CalibrationProfile>,
    /// ID of the currently active/selected calibration profile.
    pub active_calibration_id: String,
    /// Whether the first-run onboarding wizard has been completed.
    pub onboarding_complete: bool,
    /// Real-time blink detector (fed raw EEG samples).
    pub artifact_detector: artifact_detection::ArtifactDetector,
    /// Real-time head orientation tracker (fed IMU samples).
    pub head_pose: head_pose::HeadPoseTracker,
    /// UMAP projection configuration (repulsion, subsampling, timeout, etc.).
    pub umap_config: UmapUserConfig,
    /// UI theme: "system" | "light" | "dark".
    pub theme: String,
    /// UI language code (e.g. "en"). Empty = system default.
    pub language: String,
    /// Daily recording goal in minutes (0 = disabled).
    pub daily_goal_min: u32,
    /// ISO date (YYYY-MM-DD) on which the goal notification was last fired.
    pub goal_notified_date: String,
    /// fastembed model code currently in use for label/context embeddings.
    pub text_embedding_model: String,
    /// WebSocket bind host: `"127.0.0.1"` (loopback) or `"0.0.0.0"` (LAN).
    pub ws_host: String,
    /// Preferred WebSocket port (falls back to OS-assigned if in use).
    pub ws_port: u16,
    /// Seconds between automatic background update checks (0 = disabled).
    /// Re-read each cycle by the background updater task; changes apply
    /// immediately without a restart.  Persisted to settings.json.
    pub update_check_interval_secs: u64,
    /// OpenBCI board configuration (board type, serial port, channel labels).
    pub openbci_config: crate::settings::OpenBciConfig,

    /// NeuTTS voice-cloning TTS configuration.
    pub neutts_config: NeuttsConfig,

    /// Whether to pre-warm the active TTS engine at startup.
    pub tts_preload: bool,
}

impl Default for AppState {
    fn default() -> Self {
        let cfg = FilterConfig::default();

        // Bootstrap: always read settings from the default location first
        // to check for a custom data_dir override.
        let default_dir = default_skill_dir();
        let _ = std::fs::create_dir_all(&default_dir);
        let bootstrap = load_settings(&default_dir);
        let skill_dir = match &bootstrap.data_dir {
            Some(d) if !d.is_empty() => {
                let p = std::path::PathBuf::from(expand_tilde(d));
                let _ = std::fs::create_dir_all(&p);
                p
            }
            _ => default_dir,
        };
        let _ = std::fs::create_dir_all(&skill_dir);

        // Register skill_dir with the TTS module before any worker thread starts.
        init_tts_dirs(&skill_dir);
        // init_neutts_samples_dir is called later in setup() once app_handle is available.

        let model_config    = load_model_config(&skill_dir);
        let model_status    = std::sync::Arc::new(
            std::sync::Mutex::new(EegModelStatus::default())
        );
        let download_cancel = std::sync::Arc::new(
            std::sync::atomic::AtomicBool::new(false)
        );

        // Tee stderr → ~/.skill/YYYYMMDD/log_<ts>.txt  (captures ALL eprintln! output).
        let log_config = skill_log::load_log_config(&skill_dir);
        skill_log::ensure_log_config(&skill_dir);
        let today_dir = skill_dir.join(yyyymmdd_utc());
        let log_path  = today_dir.join(format!("log_{}.txt", unix_secs()));
        skill_log::tee_stderr_to_file(&log_path);   // fd-level redirect, done once
        let logger = std::sync::Arc::new(SkillLogger::new(log_config));

        Self {
            status:            MuseStatus::default(),
            stream:            None,
            scanner:           None,
            discovered:        Vec::new(),
            preferred_id:      None,
            eeg_channel:       None,
            ppg_channel:       None,
            imu_channel:       None,
            battery_ema:       None,
            filter:            EegFilter::new(cfg),
            band_analyzer:     BandAnalyzer::new(),
            quality:           QualityMonitor::new(EEG_CHANNELS),
            pending_reconnect: false,
            retry_attempt:     0,
            accumulator:       EegAccumulator::new(
                                   skill_dir.clone(),
                                   model_config.clone(),
                                   model_status.clone(),
                                   download_cancel.clone(),
                                   logger.clone(),
                               ),
            label_store: label_store::LabelStore::open(&skill_dir),
            label_shortcut:       default_label_shortcut(),
            search_shortcut:      default_search_shortcut(),
            settings_shortcut:    default_settings_shortcut(),
            calibration_shortcut: default_calibration_shortcut(),
            help_shortcut:        default_help_shortcut(),
            history_shortcut:     default_history_shortcut(),
            api_shortcut:         default_api_shortcut(),
            theme_shortcut:       default_theme_shortcut(),
            focus_timer_shortcut: default_focus_timer_shortcut(),
            calibration_profiles: vec![CalibrationProfile::default()],
            active_calibration_id: "default".into(),
            onboarding_complete: false,
            artifact_detector: artifact_detection::ArtifactDetector::new(),
            head_pose: head_pose::HeadPoseTracker::new(),
            umap_config: load_umap_config(&skill_dir),
            theme: default_theme(),
            language: String::new(),
            daily_goal_min: default_daily_goal_min(),
            goal_notified_date: String::new(),
            text_embedding_model: default_embedding_model(),
            ws_host: default_ws_host(),
            ws_port: default_ws_port(),
            update_check_interval_secs: default_update_check_interval(),
            openbci_config: crate::settings::OpenBciConfig::default(),
            neutts_config: NeuttsConfig::default(),
            tts_preload:   true,
            skill_dir,
            model_config,
            model_status,
            download_cancel,
            logger,
            session_start_utc: None,
        }
    }
}

// ── Settings persistence ──────────────────────────────────────────────────────

/// Thin alias usable from async contexts where `&AppHandle` is not clonable.
pub fn save_settings_handle(app: &AppHandle) { save_settings(app); }

/// Persist all user-configurable state to `~/.skill/settings.json`.
pub(crate) fn save_settings(app: &AppHandle) {
    let s_ref = app.state::<Mutex<AppState>>();
    let s = s_ref.lock_or_recover();
    let data = UserSettings {
        paired:                 s.status.paired_devices.clone(),
        preferred_id:           s.preferred_id.clone(),
        filter_config:          s.status.filter_config,
        embedding_overlap_secs: s.status.embedding_overlap_secs,
        data_dir: {
            let d = tilde_path(&s.skill_dir);
            let def = tilde_path(&default_skill_dir());
            if d == def { None } else { Some(d) }
        },
        label_shortcut:         s.label_shortcut.clone(),
        search_shortcut:        s.search_shortcut.clone(),
        settings_shortcut:      s.settings_shortcut.clone(),
        calibration_shortcut:   s.calibration_shortcut.clone(),
        help_shortcut:          s.help_shortcut.clone(),
        history_shortcut:       s.history_shortcut.clone(),
        api_shortcut:           s.api_shortcut.clone(),
        theme_shortcut:         s.theme_shortcut.clone(),
        focus_timer_shortcut:   s.focus_timer_shortcut.clone(),
        calibration:            CalibrationConfig::default(),
        calibration_profiles:   s.calibration_profiles.clone(),
        active_calibration_id:  s.active_calibration_id.clone(),
        onboarding_complete:    s.onboarding_complete,
        theme:                  s.theme.clone(),
        language:               s.language.clone(),
        daily_goal_min:         s.daily_goal_min,
        goal_notified_date:     s.goal_notified_date.clone(),
        text_embedding_model:   s.text_embedding_model.clone(),
        ws_host:                s.ws_host.clone(),
        ws_port:                s.ws_port,
        update_check_interval_secs: s.update_check_interval_secs,
        openbci:                s.openbci_config.clone(),
        neutts:                 s.neutts_config.clone(),
        tts_preload:            s.tts_preload,
    };
    let path = settings_path(&s.skill_dir);
    drop(s);
    if let Ok(json) = serde_json::to_string_pretty(&data) {
        if let Err(e) = std::fs::write(&path, &json) {
            eprintln!("[settings] save error: {e}");
        }
    }
}

// ── Paired device upsert (called from BLE session) ────────────────────────────

fn upsert_paired(app: &AppHandle, id: &str, name: &str) {
    let now = unix_secs();
    let s_ref = app.state::<Mutex<AppState>>();
    let mut s = s_ref.lock_or_recover();
    if let Some(d) = s.status.paired_devices.iter_mut().find(|d| d.id == id) {
        d.last_seen = now; d.name = name.to_owned();
    } else {
        s.status.paired_devices.push(PairedDevice { id: id.to_owned(), name: name.to_owned(), last_seen: now });
    }
    let pref = s.preferred_id.clone();
    for d in s.discovered.iter_mut() {
        if d.id == id { d.is_paired = true; d.last_seen = now; d.name = name.to_owned(); }
        d.is_preferred = pref.as_deref() == Some(&d.id);
    }
    if !s.discovered.iter().any(|d| d.id == id) {
        s.discovered.push(DiscoveredDevice {
            id: id.to_owned(), name: name.to_owned(),
            last_seen: now, last_rssi: 0, is_paired: true,
            is_preferred: pref.as_deref() == Some(id),
        });
        s.discovered.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
    }
    drop(s);
    save_settings(app);
}

// ── Discovered device update (called from background scanner) ─────────────────

fn upsert_discovered(app: &AppHandle, id: &str, name: &str, rssi: i16) {
    let now = unix_secs();
    let s_ref = app.state::<Mutex<AppState>>();
    let mut s = s_ref.lock_or_recover();
    let is_paired    = s.status.paired_devices.iter().any(|d| d.id == id);
    let is_preferred = s.preferred_id.as_deref() == Some(id);
    if let Some(d) = s.discovered.iter_mut().find(|d| d.id == id) {
        d.last_seen = now; d.last_rssi = rssi;
        d.is_paired = is_paired; d.is_preferred = is_preferred;
        d.name = name.to_owned();
    } else {
        s.discovered.push(DiscoveredDevice {
            id: id.to_owned(), name: name.to_owned(),
            last_seen: now, last_rssi: rssi, is_paired, is_preferred,
        });
    }
    s.discovered.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
}

// ── Misc helpers ──────────────────────────────────────────────────────────────

pub(crate) fn unix_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Returns today's date as `YYYYMMDD` (UTC) without any external crate.
fn yyyymmdd_utc() -> String {
    let mut days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86_400;

    let mut y = 1970u32;
    loop {
        let leap = (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
        let in_year = if leap { 366 } else { 365 };
        if days < in_year { break; }
        days -= in_year;
        y += 1;
    }
    let leap = (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    let month_len: [u64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 1u32;
    for &ml in &month_len {
        if days < ml { break; }
        days -= ml;
        m += 1;
    }
    format!("{y:04}{m:02}{d:02}", d = days + 1)
}

fn new_csv_path(app: &AppHandle) -> PathBuf {
    // ~/.{appname}/{YYYYMMDD}/muse_{unix}.csv
    let name = app.config()
        .product_name
        .as_deref()
        .unwrap_or("skill")
        .to_lowercase();

    let base = app.path()
        .home_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(format!(".{name}"))
        .join(yyyymmdd_utc());

    let _ = std::fs::create_dir_all(&base);
    base.join(format!("muse_{}.csv", unix_secs()))
}

/// Derive the PPG CSV path from an EEG CSV path.
/// `muse_1700000000.csv` → `muse_1700000000_ppg.csv`
fn ppg_csv_path(eeg_path: &std::path::Path) -> PathBuf {
    let stem = eeg_path.file_stem().and_then(|s| s.to_str()).unwrap_or("muse");
    eeg_path.with_file_name(format!("{stem}_ppg.csv"))
}

/// Derive the metrics CSV path from an EEG CSV path.
/// `muse_1700000000.csv` → `muse_1700000000_metrics.csv`
fn metrics_csv_path(eeg_path: &std::path::Path) -> PathBuf {
    let stem = eeg_path.file_stem().and_then(|s| s.to_str()).unwrap_or("muse");
    eeg_path.with_file_name(format!("{stem}_metrics.csv"))
}

/// Write (or overwrite) a JSON sidecar file next to the CSV recording.
///
/// The file has the same base name as the CSV with a `.json` extension, e.g.
/// `muse_1700000000.json` next to `muse_1700000000.csv`.  It captures device
/// identity, filter settings, and session timing so the CSV is fully
/// self-describing without needing the app to be running.
///
/// Called at **session start** (initial snapshot) and again at **session end**
/// (final stats).  The second write overwrites the first, adding end-time
/// and final sample count.
fn write_session_meta(app: &AppHandle, csv_path: &std::path::Path) {
    let s_ref = app.state::<Mutex<AppState>>();
    let s = s_ref.lock_or_recover();

    let session_end_utc   = unix_secs();
    let session_start_utc = s.session_start_utc;
    let duration_secs     = session_start_utc.map(|st| session_end_utc.saturating_sub(st));

    // Snapshot everything we know about the connected BLE device.
    let meta = serde_json::json!({
        // ── Recording ────────────────────────────────────────────────────
        "csv_file":            csv_path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "ppg_csv_file":        ppg_csv_path(csv_path).file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "metrics_csv_file":    metrics_csv_path(csv_path).file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "session_start_utc":   session_start_utc,
        "session_end_utc":     session_end_utc,
        "session_duration_s":  duration_secs,
        "total_samples":       s.status.sample_count,
        "ppg_total_samples":   s.status.ppg_sample_count,
        "sample_rate_hz":      EEG_SAMPLE_RATE,
        "ppg_sample_rate_hz":  PPG_SAMPLE_RATE,
        "channels":            ["TP9", "AF7", "AF8", "TP10"],
        "ppg_channels":        ["ambient", "infrared", "red"],
        "channel_count":       4,

        // ── BLE Device Identity ──────────────────────────────────────────
        "device": {
            "name":               s.status.device_name,
            "id":                 s.status.device_id,
            "serial_number":      s.status.serial_number,
            "mac_address":        s.status.mac_address,
            "firmware_version":   s.status.firmware_version,
            "hardware_version":   s.status.hardware_version,
            "bootloader_version": s.status.bootloader_version,
            "preset":             s.status.headset_preset,
        },

        // ── Battery ──────────────────────────────────────────────────────
        "battery_pct_end":       s.status.battery,

        // ── Signal quality at session end ────────────────────────────────
        "channel_quality":       s.status.channel_quality,

        // ── Filter / processing config ───────────────────────────────────
        "filter_config":         s.status.filter_config,
        "embedding_overlap_secs": s.status.embedding_overlap_secs,

        // ── App ──────────────────────────────────────────────────────────
        "app_version":           env!("CARGO_PKG_VERSION"),
        "platform":              std::env::consts::OS,
        "arch":                  std::env::consts::ARCH,
    });
    drop(s);

    let meta_path = csv_path.with_extension("json");
    match serde_json::to_string_pretty(&meta) {
        Ok(json) => {
            match std::fs::write(&meta_path, &json) {
                Ok(_)  => eprintln!("[session] wrote metadata → {}", meta_path.display()),
                Err(e) => eprintln!("[session] ERROR writing metadata {}: {e}", meta_path.display()),
            }
        }
        Err(e) => eprintln!("[session] ERROR serialising metadata: {e}"),
    }
}

pub(crate) fn emit_status(app: &AppHandle) {
    let s_ref = app.state::<Mutex<AppState>>();
    let st = { let g = s_ref.lock_or_recover(); g.status.clone() };
    let _ = app.emit("muse-status", &st);
    app.state::<WsBroadcaster>().send("muse-status", &st);
}

pub(crate) fn emit_devices(app: &AppHandle) {
    let s_ref = app.state::<Mutex<AppState>>();
    let d = { let g = s_ref.lock_or_recover(); g.discovered.clone() };
    let _ = app.emit("devices-updated", &d);
}

// ── Toast / notification helpers ──────────────────────────────────────────────

/// Toast severity levels — serialised to the frontend as lowercase strings.
#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// Payload emitted via the `"toast"` Tauri event so the frontend can show
/// in-app toast banners.  Also fires a native OS notification (macOS
/// Notification Centre / Linux libnotify) for events that matter when the
/// window is hidden.
#[derive(Clone, Serialize)]
struct ToastPayload {
    level:   ToastLevel,
    title:   String,
    message: String,
}

/// Send an in-app toast event AND a native OS notification.
pub(crate) fn send_toast(app: &AppHandle, level: ToastLevel, title: &str, message: &str) {
    let payload = ToastPayload {
        level,
        title:   title.to_owned(),
        message: message.to_owned(),
    };
    // In-app toast (all open windows)
    let _ = app.emit("toast", &payload);
    // Also broadcast to WebSocket clients
    app.state::<WsBroadcaster>().send("toast", &payload);
    // Native OS notification (best-effort — permission may not be granted)
    let _ = app.notification()
        .builder()
        .title(title)
        .body(message)
        .show();
}

/// Reconnect backoff: 1 s → 2 s → 3 s → 5 s, then stays at 5 s indefinitely.
/// Never exceeds 5 seconds so reconnection stays snappy regardless of history.
fn retry_delay_secs(attempt: u32) -> u32 {
    match attempt {
        0 => 1,
        1 => 2,
        2 => 3,
        _ => 5,
    }
}

fn go_disconnected(app: &AppHandle, error: Option<String>, is_bt: bool) {
    // Decide whether to auto-retry.
    // Retry when:  pending_reconnect is active  AND  failure is not BT-level
    //              AND  not an explicit user cancel (error=None).
    // Also retry on unexpected device disconnect (error=None but we WERE
    // connected and pending_reconnect is true — device walked away).
    let (retry, attempt) = {
        let r = app.state::<Mutex<AppState>>();
        let s = r.lock_or_recover();
        let want_retry = s.pending_reconnect && !is_bt;
        (want_retry, s.retry_attempt)
    };

    let delay = if retry { retry_delay_secs(attempt) } else { 0 };

    {
        let r = app.state::<Mutex<AppState>>();
        let mut s = r.lock_or_recover();
        if is_bt {
            s.pending_reconnect = false;    // BT-off — stop retrying
            s.retry_attempt     = 0;
        } else if !retry {
            // Explicit cancel or no pending_reconnect — reset
            s.retry_attempt = 0;
        }
        // Stay in "scanning" during silent retry so the error card never flashes.
        s.status.state         = if retry   { "scanning".into()    }
                                  else if is_bt { "bt_off".into()  }
                                  else          { "disconnected".into() };
        s.status.device_name       = None;
        s.status.device_id         = None;
        s.status.device_kind       = "unknown".into();
        s.status.serial_number     = None;
        s.status.mac_address       = None;
        s.status.firmware_version  = None;
        s.status.hardware_version  = None;
        s.status.bootloader_version = None;
        s.status.headset_preset    = None;
        s.status.battery              = 0.0;
        s.status.eeg                  = vec![f64::NAN; 4];
        s.status.ppg                  = vec![0.0; 3];
        s.status.ppg_sample_count     = 0;
        s.status.bt_error      = if retry { None } else { error };
        s.status.target_name   = None;
        s.status.retry_attempt        = if retry { attempt + 1 } else { 0 };
        s.status.retry_countdown_secs = delay;
        s.stream               = None;
        s.battery_ema          = None;
        s.filter.reset();
        s.band_analyzer.reset();
        s.quality.reset();
        s.accumulator.reset();
        s.accumulator.update_device(None, None);
        s.artifact_detector.reset();
        s.head_pose.reset();
        s.status.channel_quality = vec![SignalQuality::default(); 4];
    }
    refresh_tray(app);
    emit_status(app);

    if retry {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            app_log!(app, "bluetooth", "[reconnect] scheduling attempt #{} in {}s (backoff schedule: 1→2→3→5s)",
                      attempt + 1, delay);
            // Countdown loop: tick once per second, update status each tick.
            for remaining in (1..=delay).rev() {
                {
                    let r = app.state::<Mutex<AppState>>();
                    let s = r.lock_or_recover();
                    if !s.pending_reconnect { return; }   // cancelled during countdown
                }
                {
                    let r = app.state::<Mutex<AppState>>();
                    let mut s = r.lock_or_recover();
                    s.status.retry_countdown_secs = remaining;
                }
                emit_status(&app);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            // Time's up — attempt reconnection.
            let preferred = {
                let r = app.state::<Mutex<AppState>>();
                let mut s = r.lock_or_recover();
                if !s.pending_reconnect { return; }   // cancelled while we slept
                s.retry_attempt += 1;
                s.status.retry_countdown_secs = 0;
                s.preferred_id.clone()
                    .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()))
            };
            app_log!(app, "bluetooth", "[reconnect] attempt #{} — waited {delay}s — target={preferred:?}",
                      attempt + 1);
            start_session(&app, preferred);
        });
    }
}

fn classify_bt_error(raw: &str) -> (String, bool) {
    let lo = raw.to_lowercase();
    let is_bt = lo.contains("adapter") || lo.contains("powered") || lo.contains("bluetooth")
             || lo.contains("permission") || lo.contains("access denied")
             || lo.contains("org.bluez")  || lo.contains("dbus");
    let msg = if is_bt {
        "Bluetooth is off or unavailable.\n\
         \n\
         • Enable Bluetooth in System Settings\n\
         • macOS: System Settings → Privacy & Security → Bluetooth\n\
         • Linux: make sure bluetoothd is running"
    } else {
        "Connection failed. Make sure the headset is powered on and in range."
    };
    (msg.into(), is_bt)
}

async fn bluetooth_ok() -> Result<(), (String, bool)> {
    let mgr = BtPlatformManager::new().await
        .map_err(|e| classify_bt_error(&e.to_string()))?;
    let adapters = mgr.adapters().await
        .map_err(|e| classify_bt_error(&e.to_string()))?;
    if adapters.is_empty() {
        return Err((
            "No Bluetooth adapter detected.\n\
             \n\
             • Enable Bluetooth in System Settings\n\
             • Linux: sudo systemctl start bluetooth".into(),
            true,
        ));
    }
    Ok(())
}

// ── Background BLE scanner (runs independently of the main connection) ─────────

/// Emit the bt_off UI state once per outage (edge-triggered via `emitted` flag).
fn scanner_bt_off(app: &AppHandle, emitted: &mut bool) {
    if *emitted { return; }
    *emitted = true;
    app_log!(app, "bluetooth", "off");
    send_toast(app, ToastLevel::Error, "Bluetooth Off",
        "Bluetooth is unavailable — turn it on to connect.");
    let do_emit = {
        let s = app.state::<Mutex<AppState>>();
        let mut g = s.lock_or_recover();
        // Also catch "scanning" — that state is used during the pending-reconnect
        // retry loop; if BT drops mid-retry we still need to show the banner.
        let idle = matches!(g.status.state.as_str(), "disconnected" | "scanning");
        if idle {
            g.status.state      = "bt_off".into();
            g.status.bt_error   = Some(
                "Bluetooth is off — turn it on to connect to your BCI device.".into()
            );
            g.pending_reconnect = false;   // abort any in-flight retry sleep
            true
        } else { false }
    };
    if do_emit { refresh_tray(app); emit_status(app); }
}

/// Clear the bt_off state, auto-reconnect, and start scanning when BT is on.
/// No-ops if `emitted` is false (BT was never off during this run).
async fn scanner_bt_on(
    app: &AppHandle,
    emitted: &mut bool,
    scanning: &mut bool,
    adapter: &BtPlatformAdapter,
) {
    if !*emitted { return; }
    *emitted = false;
    app_log!(app, "bluetooth", "on");
    send_toast(app, ToastLevel::Info, "Bluetooth Restored",
        "Bluetooth is back — reconnecting…");

    let (do_emit, preferred_id) = {
        let s = app.state::<Mutex<AppState>>();
        let mut g = s.lock_or_recover();
        if g.status.state == "bt_off" {
            g.status.state         = "disconnected".into();
            g.status.bt_error      = None;
            g.pending_reconnect    = true;   // auto-retry until device found
            (true, g.preferred_id.clone())
        } else { (false, None) }
    };
    if do_emit {
        refresh_tray(app);
        emit_status(app);
        if preferred_id.is_some() {
            start_session(app, preferred_id);
        }
    }

    if !*scanning && adapter.start_scan(ScanFilter::default()).await.is_ok() {
        app_log!(app, "bluetooth", "[scanner] BLE scan started");
        *scanning = true;
    }
}

async fn run_background_scanner(app: AppHandle, stop_rx: tokio::sync::oneshot::Receiver<()>) {
    tokio::pin!(stop_rx);
    let mut bt_off_emitted = false;

    'outer: loop {
        // Acquire the adapter.  On macOS (CoreBluetooth) this always succeeds
        // because CBCentralManager is created even when the radio is off.
        // On Linux (BlueZ) bluetoothd must be running; retry every 2 s.
        let adapter = loop {
            if let Ok(mgr) = BtPlatformManager::new().await {
                match mgr.adapters().await {
                    Ok(mut v) if !v.is_empty() => break v.remove(0),
                    _ => {}
                }
            }
            tokio::select! {
                biased;
                _ = &mut stop_rx => return,
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            }
        };

        // Subscribe to events BEFORE reading current state to close the
        // TOCTOU gap between the two calls.
        let mut events = match adapter.events().await {
            Ok(s) => s,
            Err(e) => {
                app_log!(app, "bluetooth", "[scanner] events() failed: {e}");
                tokio::select! {
                    biased;
                    _ = &mut stop_rx => return,
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                }
                continue 'outer;
            }
        };

        // Synchronise with the current radio state immediately.
        let mut scanning = false;
        match adapter.adapter_state().await.unwrap_or(CentralState::Unknown) {
            CentralState::PoweredOn => {
                scanner_bt_on(&app, &mut bt_off_emitted, &mut scanning, &adapter).await;
                // Normal first-time startup: bt_off_emitted was false so
                // scanner_bt_on was a no-op; start scanning now.
                if !scanning && adapter.start_scan(ScanFilter::default()).await.is_ok() {
                    app_log!(app, "bluetooth", "[scanner] BLE scan started");
                    scanning = true;
                }
            }
            _ => scanner_bt_off(&app, &mut bt_off_emitted),
        }

        // Combined event-listener + peripheral-poll loop.
        let mut poll_tick = tokio::time::interval(Duration::from_secs(3));
        loop {
            tokio::select! {
                biased;

                _ = &mut stop_rx => {
                    if scanning { let _ = adapter.stop_scan().await; }
                    app_log!(app, "bluetooth", "[scanner] stopped");
                    return;
                }

                maybe_event = events.next() => {
                    let Some(event) = maybe_event else { continue 'outer; };
                    match event {
                        // btleplug translates centralManagerDidUpdateState (macOS)
                        // and BlueZ PropertiesChanged (Linux) into these variants.
                        CentralEvent::StateUpdate(CentralState::PoweredOn) => {
                            scanner_bt_on(&app, &mut bt_off_emitted, &mut scanning, &adapter).await;
                        }
                        CentralEvent::StateUpdate(_) => {
                            // PoweredOff | Unknown | Resetting | Unauthorized | Unsupported
                            if scanning {
                                let _ = adapter.stop_scan().await;
                                scanning = false;
                            }
                            scanner_bt_off(&app, &mut bt_off_emitted);
                        }

                        _ => {}
                    }
                }

                _ = poll_tick.tick(), if scanning => {
                    match adapter.peripherals().await {
                        Err(_) => { let _ = adapter.stop_scan().await; continue 'outer; }
                        Ok(peripherals) => {
                            for p in peripherals {
                                if let Ok(Some(props)) = p.properties().await {
                                    if let Some(ref name) = props.local_name {
                                        let n = name.to_lowercase();
                                        let is_known = n.starts_with("muse")
                                            || n.starts_with("ganglion")
                                            || n.starts_with("simblee");
                                        if is_known {
                                            let id   = p.id().to_string();
                                            let rssi = props.rssi.unwrap_or(0);
                                            upsert_discovered(&app, &id, name, rssi);
                                            app_log!(app, "bluetooth",
                                                "[scanner] {name} id={id} rssi={rssi} dBm"
                                            );
                                        }
                                    }
                                }
                            }
                            emit_devices(&app);
                        }
                    }
                }
            }
        }
    }
}

fn start_background_scanner(app: &AppHandle) {
    let s_ref = app.state::<Mutex<AppState>>();
    let already = { let g = s_ref.lock_or_recover(); g.scanner.is_some() };
    if already { return; }
    let (tx, rx) = tokio::sync::oneshot::channel();
    s_ref.lock_or_recover().scanner = Some(ScannerHandle { cancel_tx: tx });
    let clone = app.clone();
    tauri::async_runtime::spawn(async move { run_background_scanner(clone, rx).await; });
}

// ── Muse session helpers ──────────────────────────────────────────────────────

pub(crate) fn start_session(app: &AppHandle, preferred_id: Option<String>) {
    {
        let r = app.state::<Mutex<AppState>>();
        let mut s = r.lock_or_recover();
        if s.stream.is_some() { return; }
        s.pending_reconnect = true; // always retry on disconnect until user cancels
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    // Choose target: explicit arg → persisted preferred → most-recently-seen paired
    let target = preferred_id.or_else(|| {
        let r = app.state::<Mutex<AppState>>();
        let s = r.lock_or_recover();
        s.preferred_id.clone()
            .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()))
    });

    // Determine backend from the preferred device's known name.
    // Ganglion advertises as "Ganglion-XXXX" or "Simblee"; everything else
    // (including unknown/new devices) falls back to the Muse path.
    let target_name: Option<String> = target.as_ref().and_then(|id| {
        let r = app.state::<Mutex<AppState>>();
        let s = r.lock_or_recover();
        s.status.paired_devices.iter()
            .find(|d| &d.id == id)
            .map(|d| d.name.clone())
            .or_else(|| s.discovered.iter().find(|d| &d.id == id).map(|d| d.name.clone()))
    });
    let is_ganglion = target_name.as_deref().map(|n| {
        let n = n.to_lowercase();
        n.starts_with("ganglion") || n.starts_with("simblee")
    }).unwrap_or(false);

    app.state::<Mutex<AppState>>().lock_or_recover().stream = Some(StreamHandle { cancel_tx: tx });
    let csv  = new_csv_path(app);
    let app2 = app.clone();

    if is_ganglion {
        tauri::async_runtime::spawn(async move {
            run_openbci_ganglion_session(app2, rx, csv, target).await;
        });
    } else {
        tauri::async_runtime::spawn(async move {
            run_muse_session(app2, rx, csv, target).await;
        });
    }
}

pub(crate) fn cancel_session(app: &AppHandle) {
    let tx = app.state::<Mutex<AppState>>().lock_or_recover().stream.take().map(|sh| sh.cancel_tx);
    if let Some(tx) = tx { let _ = tx.send(()); }
}

// ── CSV writer: wide format (one row per sample, one column per channel) ────────

const EEG_SAMPLE_RATE: f64 = 256.0;

struct CsvState {
    wtr:     csv::Writer<std::fs::File>,
    /// Number of EEG channels in this CSV (4 for Muse/Ganglion, 8/16/24 for Cyton/Galea).
    n_eeg:   usize,
    /// Queued µV values per EEG channel.
    bufs:    Vec<VecDeque<f64>>,
    /// Per-sample Unix timestamps (seconds) matching each value in `bufs`.
    ts_bufs: Vec<VecDeque<f64>>,
    /// Rows written so far — used to drive periodic disk flushes.
    written: u64,
    /// Separate CSV writer for PPG data (created lazily on first PPG sample).
    ppg_wtr:     Option<csv::Writer<std::fs::File>>,
    /// Queued raw ADC values per PPG channel (0=ambient, 1=infrared, 2=red).
    ppg_bufs:    [VecDeque<f64>; 3],
    /// Per-sample Unix timestamps for PPG channels.
    ppg_ts_bufs: [VecDeque<f64>; 3],
    /// PPG rows written.
    ppg_written: u64,
    /// Separate CSV writer for derived metrics (~4 Hz, created lazily).
    metrics_wtr:     Option<csv::Writer<std::fs::File>>,
    /// Metrics rows written.
    metrics_written: u64,
}

const PPG_SAMPLE_RATE: f64 = 64.0;

impl CsvState {
    fn open(path: &std::path::Path) -> Result<Self, csv::Error> {
        Self::open_with_labels(path, &["TP9", "AF7", "AF8", "TP10"])
    }

    fn open_with_labels(path: &std::path::Path, labels: &[&str]) -> Result<Self, csv::Error> {
        let n = labels.len();
        let mut wtr = csv::Writer::from_path(path)?;
        let mut header = vec!["timestamp_s"];
        header.extend_from_slice(labels);
        wtr.write_record(&header)?;
        Ok(Self {
            wtr,
            n_eeg:   n,
            bufs:    (0..n).map(|_| VecDeque::new()).collect(),
            ts_bufs: (0..n).map(|_| VecDeque::new()).collect(),
            written: 0,
            ppg_wtr:     None,
            ppg_bufs:    std::array::from_fn(|_| VecDeque::new()),
            ppg_ts_bufs: std::array::from_fn(|_| VecDeque::new()),
            ppg_written: 0,
            metrics_wtr:     None,
            metrics_written: 0,
        })
    }

    /// Buffer `samples` for `electrode` (0-3) and flush any complete rows to disk.
    ///
    /// A "complete row" is one where all 4 channels have at least one queued sample.
    /// Packet timestamp is the Unix time of the FIRST sample; subsequent samples are
    /// offset by `i / EEG_SAMPLE_RATE` seconds.
    fn push_eeg(&mut self, electrode: usize, samples: &[f64], packet_ts: f64, sample_rate: f64) {
        if electrode >= self.n_eeg { return; }
        for (i, &v) in samples.iter().enumerate() {
            self.bufs[electrode].push_back(v);
            self.ts_bufs[electrode].push_back(packet_ts + i as f64 / sample_rate);
        }

        // Write every row for which all channels have at least one queued sample.
        let ready = self.bufs.iter().map(|b| b.len()).min().unwrap_or(0);
        let n = self.n_eeg;
        for _ in 0..ready {
            let ts = self.ts_bufs[0].pop_front().unwrap();
            for k in 1..n { self.ts_bufs[k].pop_front(); }

            let mut row = vec![format!("{:.6}", ts)];
            for k in 0..n {
                row.push(format!("{:.4}", self.bufs[k].pop_front().unwrap()));
            }
            let refs: Vec<&str> = row.iter().map(String::as_str).collect();
            let _ = self.wtr.write_record(&refs);
            self.written += 1;
        }

        // Flush roughly once per second of data.
        if self.written > 0 && self.written.is_multiple_of(256) {
            let _ = self.wtr.flush();
        }
    }

    /// Buffer PPG samples for `channel` (0-2) and flush complete rows.
    /// PPG CSV is created lazily from the EEG CSV path (same dir, `_ppg` suffix).
    fn push_ppg(&mut self, eeg_csv_path: &std::path::Path, channel: usize, samples: &[f64], packet_ts: f64,
                ppg_vitals: Option<&crate::ppg_analysis::PpgMetrics>) {
        if channel >= 3 { return; }

        // Lazily create PPG CSV file.
        if self.ppg_wtr.is_none() {
            let ppg_path = ppg_csv_path(eeg_csv_path);
            match csv::Writer::from_path(&ppg_path) {
                Ok(mut w) => {
                    let _ = w.write_record([
                        "timestamp_s", "ambient", "infrared", "red",
                        "hr_bpm", "rmssd_ms", "sdnn_ms", "pnn50_pct", "lf_hf_ratio",
                        "respiratory_rate_bpm", "spo2_pct", "perfusion_index_pct", "stress_index",
                    ]);
                    eprintln!("[csv] PPG file opened: {}", ppg_path.display());
                    self.ppg_wtr = Some(w);
                }
                Err(e) => {
                    eprintln!("[csv] failed to create PPG file {}: {e}", ppg_path.display());
                    return;
                }
            }
        }

        for (i, &v) in samples.iter().enumerate() {
            self.ppg_bufs[channel].push_back(v);
            self.ppg_ts_bufs[channel].push_back(packet_ts + i as f64 / PPG_SAMPLE_RATE);
        }

        let ready = self.ppg_bufs.iter().map(|b| b.len()).min().unwrap_or(0);
        if let Some(ref mut wtr) = self.ppg_wtr {
            for _ in 0..ready {
                let ts = self.ppg_ts_bufs[0].pop_front().unwrap();
                for k in 1..3 { self.ppg_ts_bufs[k].pop_front(); }

                let mut row = vec![format!("{:.6}", ts)];
                for k in 0..3 {
                    row.push(format!("{:.1}", self.ppg_bufs[k].pop_front().unwrap()));
                }
                // Append PPG-derived vitals (repeated for every sample row;
                // empty when not yet computed).
                if let Some(v) = ppg_vitals {
                    row.push(format!("{:.1}", v.hr));
                    row.push(format!("{:.2}", v.rmssd));
                    row.push(format!("{:.2}", v.sdnn));
                    row.push(format!("{:.2}", v.pnn50));
                    row.push(format!("{:.4}", v.lf_hf_ratio));
                    row.push(format!("{:.2}", v.respiratory_rate));
                    row.push(format!("{:.2}", v.spo2_estimate));
                    row.push(format!("{:.4}", v.perfusion_index));
                    row.push(format!("{:.1}", v.stress_index));
                } else {
                    for _ in 0..9 { row.push(String::new()); }
                }
                let refs: Vec<&str> = row.iter().map(String::as_str).collect();
                let _ = wtr.write_record(&refs);
                self.ppg_written += 1;
            }
            // Flush roughly once per second at 64 Hz.
            if self.ppg_written > 0 && self.ppg_written.is_multiple_of(64) {
                let _ = wtr.flush();
            }
        }
    }

    /// Write a BandSnapshot row to the `_metrics.csv` file (~4 Hz).
    /// Created lazily on first call from the EEG CSV path.
    fn push_metrics(&mut self, eeg_csv_path: &std::path::Path, snap: &BandSnapshot) {
        // Lazily create metrics CSV file.
        if self.metrics_wtr.is_none() {
            let path = metrics_csv_path(eeg_csv_path);
            match csv::Writer::from_path(&path) {
                Ok(mut w) => {
                    let _ = w.write_record(METRICS_CSV_HEADER);
                    eprintln!("[csv] Metrics file opened: {}", path.display());
                    self.metrics_wtr = Some(w);
                }
                Err(e) => {
                    eprintln!("[csv] failed to create metrics file {}: {e}", path.display());
                    return;
                }
            }
        }

        if let Some(ref mut wtr) = self.metrics_wtr {
            let opt_f64 = |v: Option<f64>| v.map_or(String::new(), |x| format!("{:.4}", x));
            let opt_u64 = |v: Option<u64>| v.map_or(String::new(), |x| x.to_string());
            let opt_u16 = |v: Option<u16>| v.map_or(String::new(), |x| x.to_string());

            // Per-channel absolute + relative band powers (6 bands × 4 channels = 48 values)
            let mut row: Vec<String> = Vec::with_capacity(100);
            row.push(format!("{:.6}", snap.timestamp));

            for ch in &snap.channels {
                row.push(format!("{:.6}", ch.delta));
                row.push(format!("{:.6}", ch.theta));
                row.push(format!("{:.6}", ch.alpha));
                row.push(format!("{:.6}", ch.beta));
                row.push(format!("{:.6}", ch.gamma));
                row.push(format!("{:.6}", ch.high_gamma));
                row.push(format!("{:.6}", ch.rel_delta));
                row.push(format!("{:.6}", ch.rel_theta));
                row.push(format!("{:.6}", ch.rel_alpha));
                row.push(format!("{:.6}", ch.rel_beta));
                row.push(format!("{:.6}", ch.rel_gamma));
                row.push(format!("{:.6}", ch.rel_high_gamma));
            }

            // Cross-channel / derived EEG indices
            row.push(format!("{:.6}", snap.faa));
            row.push(format!("{:.4}", snap.tar));
            row.push(format!("{:.4}", snap.bar));
            row.push(format!("{:.4}", snap.dtr));
            row.push(format!("{:.6}", snap.pse));
            row.push(format!("{:.2}", snap.apf));
            row.push(format!("{:.4}", snap.bps));
            row.push(format!("{:.2}", snap.snr));
            row.push(format!("{:.6}", snap.coherence));
            row.push(format!("{:.6}", snap.mu_suppression));
            row.push(format!("{:.2}", snap.mood));
            row.push(format!("{:.4}", snap.tbr));
            row.push(format!("{:.2}", snap.sef95));
            row.push(format!("{:.2}", snap.spectral_centroid));
            row.push(format!("{:.4}", snap.hjorth_activity));
            row.push(format!("{:.6}", snap.hjorth_mobility));
            row.push(format!("{:.6}", snap.hjorth_complexity));
            row.push(format!("{:.6}", snap.permutation_entropy));
            row.push(format!("{:.6}", snap.higuchi_fd));
            row.push(format!("{:.6}", snap.dfa_exponent));
            row.push(format!("{:.6}", snap.sample_entropy));
            row.push(format!("{:.6}", snap.pac_theta_gamma));
            row.push(format!("{:.6}", snap.laterality_index));

            // PPG vitals
            row.push(opt_f64(snap.hr));
            row.push(opt_f64(snap.rmssd));
            row.push(opt_f64(snap.sdnn));
            row.push(opt_f64(snap.pnn50));
            row.push(opt_f64(snap.lf_hf_ratio));
            row.push(opt_f64(snap.respiratory_rate));
            row.push(opt_f64(snap.spo2_estimate));
            row.push(opt_f64(snap.perfusion_index));
            row.push(opt_f64(snap.stress_index));

            // Artifact events
            row.push(opt_u64(snap.blink_count));
            row.push(opt_f64(snap.blink_rate));


            // Head pose
            row.push(opt_f64(snap.head_pitch));
            row.push(opt_f64(snap.head_roll));
            row.push(opt_f64(snap.stillness));
            row.push(opt_u64(snap.nod_count));
            row.push(opt_u64(snap.shake_count));

            // Composite scores
            row.push(opt_f64(snap.meditation));
            row.push(opt_f64(snap.cognitive_load));
            row.push(opt_f64(snap.drowsiness));

            // Temperature
            row.push(opt_u16(snap.temperature_raw));

            // GPU utilisation
            row.push(opt_f64(snap.gpu_overall));
            row.push(opt_f64(snap.gpu_render));
            row.push(opt_f64(snap.gpu_tiler));

            let refs: Vec<&str> = row.iter().map(String::as_str).collect();
            let _ = wtr.write_record(&refs);
            self.metrics_written += 1;

            // Flush ~once per second at 4 Hz.
            if self.metrics_written.is_multiple_of(4) {
                let _ = wtr.flush();
            }
        }
    }

    fn flush(&mut self) {
        let _ = self.wtr.flush();
        if let Some(ref mut w) = self.ppg_wtr {
            let _ = w.flush();
        }
        if let Some(ref mut w) = self.metrics_wtr {
            let _ = w.flush();
        }
    }
}

/// Column headers for the `_metrics.csv` file.
///
/// Layout:
/// - timestamp
/// - 4 channels × (6 absolute + 6 relative) = 48 band power columns
/// - 22 cross-channel EEG indices
/// - 9 PPG vitals
/// - 4 artifact events
/// - 5 head pose
/// - 3 composite scores
/// - 1 temperature
const METRICS_CSV_HEADER: [&str; 95] = [
    "timestamp_s",
    // ── Per-channel band powers (TP9, AF7, AF8, TP10) ──
    "TP9_delta", "TP9_theta", "TP9_alpha", "TP9_beta", "TP9_gamma", "TP9_high_gamma",
    "TP9_rel_delta", "TP9_rel_theta", "TP9_rel_alpha", "TP9_rel_beta", "TP9_rel_gamma", "TP9_rel_high_gamma",
    "AF7_delta", "AF7_theta", "AF7_alpha", "AF7_beta", "AF7_gamma", "AF7_high_gamma",
    "AF7_rel_delta", "AF7_rel_theta", "AF7_rel_alpha", "AF7_rel_beta", "AF7_rel_gamma", "AF7_rel_high_gamma",
    "AF8_delta", "AF8_theta", "AF8_alpha", "AF8_beta", "AF8_gamma", "AF8_high_gamma",
    "AF8_rel_delta", "AF8_rel_theta", "AF8_rel_alpha", "AF8_rel_beta", "AF8_rel_gamma", "AF8_rel_high_gamma",
    "TP10_delta", "TP10_theta", "TP10_alpha", "TP10_beta", "TP10_gamma", "TP10_high_gamma",
    "TP10_rel_delta", "TP10_rel_theta", "TP10_rel_alpha", "TP10_rel_beta", "TP10_rel_gamma", "TP10_rel_high_gamma",
    // ── Cross-channel EEG indices ──
    "faa", "tar", "bar", "dtr", "pse", "apf", "bps", "snr",
    "coherence", "mu_suppression", "mood", "tbr", "sef95", "spectral_centroid",
    "hjorth_activity", "hjorth_mobility", "hjorth_complexity",
    "permutation_entropy", "higuchi_fd", "dfa_exponent",
    "sample_entropy", "pac_theta_gamma", "laterality_index",
    // ── PPG vitals ──
    "hr_bpm", "rmssd_ms", "sdnn_ms", "pnn50_pct", "lf_hf_ratio",
    "respiratory_rate_bpm", "spo2_pct", "perfusion_index_pct", "stress_index",
    // ── Artifact events ──
    "blink_count", "blink_rate_per_min",
    // ── Head pose ──
    "head_pitch_deg", "head_roll_deg", "stillness", "nod_count", "shake_count",
    // ── Composite scores ──
    "meditation", "cognitive_load", "drowsiness",
    // ── Telemetry ──
    "temperature_raw",
    // ── GPU utilisation ──
    "gpu_overall_pct", "gpu_render_pct", "gpu_tiler_pct",
];

// ── Main streaming task ───────────────────────────────────────────────────────

// ── OpenBCI Ganglion BLE session ──────────────────────────────────────────────

async fn run_openbci_ganglion_session(
    app:          AppHandle,
    cancel_rx:    tokio::sync::oneshot::Receiver<()>,
    csv_path:     PathBuf,
    preferred_id: Option<String>,
) {
    use openbci::board::ganglion::GanglionFilter;
    tokio::pin!(cancel_rx);

    // 0. BT check (same as Muse path)
    if let Err((msg, is_bt)) = bluetooth_ok().await {
        go_disconnected(&app, Some(msg), is_bt); return;
    }

    // 1. → "scanning"
    {
        let r = app.state::<Mutex<AppState>>();
        let mut s = r.lock_or_recover();
        s.session_start_utc         = Some(unix_secs());
        s.status.state              = "scanning".into();
        s.status.device_kind        = "ganglion".into();
        s.status.device_name        = None;
        s.status.device_id          = None;
        s.status.serial_number      = None;
        s.status.mac_address        = None;
        s.status.firmware_version   = None;
        s.status.hardware_version   = None;
        s.status.bootloader_version = None;
        s.status.headset_preset     = None;
        s.status.csv_path           = Some(csv_path.to_string_lossy().into_owned());
        s.status.bt_error           = None;
        s.status.battery            = 0.0;
        s.status.eeg                = vec![f64::NAN; 4];
        s.status.sample_count       = 0;
        s.status.ppg                = vec![0.0; 3];
        s.status.ppg_sample_count   = 0;
        s.status.target_name        = preferred_id.as_ref().and_then(|id|
            s.status.paired_devices.iter().find(|d| &d.id == id).map(|d| d.name.clone())
        );
    }
    refresh_tray(&app); emit_status(&app);

    // 2. Prepare (connect BLE) — the openbci crate uses blocking btleplug calls
    //    internally, so we run them on the blocking thread pool.
    let preferred_mac = preferred_id.clone();
    // Read OpenBCI config (scan timeout, preferred MAC)
    let scan_timeout_secs = {
        let r = app.state::<Mutex<AppState>>();
        let g = r.lock_or_recover();
        g.openbci_config.scan_timeout_secs
    };

    let board_result = tokio::select! {
        biased;
        _ = &mut cancel_rx => { go_disconnected(&app, None, false); return; }
        r = tokio::task::spawn_blocking(move || {
            let filter = GanglionFilter {
                mac_address: preferred_mac,
                device_name: None,
            };
            let cfg = GanglionConfig {
                scan_timeout: std::time::Duration::from_secs(scan_timeout_secs.into()),
                filter,
                ..Default::default()
            };
            let mut board = GanglionBoard::new(cfg);
            board.prepare().map(|_| board)
        }) => r,
    };

    let mut board = match board_result {
        Ok(Ok(b))  => b,
        Ok(Err(e)) => {
            let (m, bt) = classify_bt_error(&e.to_string());
            go_disconnected(&app, Some(m), bt); return;
        }
        Err(_) => {
            go_disconnected(&app, Some("Ganglion scan task panicked".into()), false); return;
        }
    };

    // 3. Derive the device name from the board (openbci sets electrode_layout or
    //    we fall back to a generic name).  Ganglion doesn't expose a name getter
    //    on the Board trait, so we synthesise one from preferred_id.
    let dev_name = preferred_id.as_ref()
        .and_then(|id| {
            let r = app.state::<Mutex<AppState>>();
            let s = r.lock_or_recover();
            s.status.paired_devices.iter()
                .find(|d| &d.id == id)
                .map(|d| d.name.clone())
                .or_else(|| s.discovered.iter().find(|d| &d.id == id).map(|d| d.name.clone()))
        })
        .unwrap_or_else(|| "Ganglion".into());

    let dev_id = preferred_id.clone().unwrap_or_else(|| dev_name.clone());

    // 4. → "connected"
    {
        let r = app.state::<Mutex<AppState>>();
        let mut s = r.lock_or_recover();
        s.status.state       = "connected".into();
        s.status.device_name = Some(dev_name.clone());
        s.status.device_id   = Some(dev_id.clone());
        s.status.bt_error    = None;
        s.status.target_name = None;
        s.retry_attempt                = 0;
        s.status.retry_attempt         = 0;
        s.status.retry_countdown_secs  = 0;
        s.accumulator.update_device(Some(dev_id.clone()), Some(dev_name.clone()));
    }
    app_log!(app, "bluetooth", "[ganglion] connected: {dev_name} (id={dev_id})");
    upsert_paired(&app, &dev_id, &dev_name);
    refresh_tray(&app); emit_status(&app); emit_devices(&app);
    write_session_meta(&app, &csv_path);

    let connect_payload = serde_json::json!({
        "device_name": dev_name,
        "device_id":   dev_id,
        "timestamp":   unix_secs(),
    });
    let _ = app.emit("device-connected", &connect_payload);
    app.state::<WsBroadcaster>().send("device-connected", &connect_payload);
    send_toast(&app, ToastLevel::Success, "Connected",
        &format!("{dev_name} is now streaming EEG data."));

    // 5. Open CSV — use configured channel labels (fall back to "Ch N")
    let ch_labels: Vec<String> = {
        let r = app.state::<Mutex<AppState>>();
        let s = r.lock_or_recover();
        let cfg_labels = &s.openbci_config.channel_labels;
        (0..4).map(|i| {
            cfg_labels.get(i)
                .filter(|l| !l.is_empty())
                .cloned()
                .unwrap_or_else(|| crate::constants::GANGLION_CHANNEL_NAMES[i].to_string())
        }).collect()
    };
    let label_refs: Vec<&str> = ch_labels.iter().map(|s| s.as_str()).collect();
    let mut csv = match CsvState::open_with_labels(&csv_path, &label_refs) {
        Ok(c)  => c,
        Err(e) => {
            write_session_meta(&app, &csv_path);
            go_disconnected(&app, Some(format!("CSV error: {e}")), false);
            return;
        }
    };
    write_session_meta(&app, &csv_path);

    // 6. Start streaming (blocking call; wraps background reader thread)
    let stream_handle = match board.start_stream() {
        Ok(h)  => h,
        Err(e) => {
            go_disconnected(&app, Some(format!("Ganglion start_stream: {e}")), false);
            return;
        }
    };

    // 7. Bridge the blocking mpsc StreamHandle into async world via a channel.
    //    The blocking thread drains the receiver; each Sample is forwarded as
    //    four per-channel EEG packets matching the Muse event format exactly.
    let (sample_tx, mut sample_rx) =
        tokio::sync::mpsc::channel::<openbci::sample::Sample>(256);
    let bridge_handle = tokio::task::spawn_blocking(move || {
        // Runs on the blocking thread pool; blocks on recv() until stream ends.
        while let Some(s) = stream_handle.recv() {
            if sample_tx.blocking_send(s).is_err() { break; }
        }
        // Explicitly stop the board's reader thread by dropping the StreamHandle.
        // (It was moved into this closure and drops here.)
    });

    // 8. Event loop — same select! pattern as run_muse_session.
    let mut user_cancelled = false;
    loop {
        tokio::select! {
            biased;

            _ = &mut cancel_rx => {
                user_cancelled = true;
                break;
            }

            maybe_sample = sample_rx.recv() => {
                let Some(sample) = maybe_sample else {
                    app_log!(app, "bluetooth", "[ganglion] sample bridge closed");
                    break;
                };

                // Forward each channel as a separate EEG packet — identical
                // to the Muse path so the filter/band/embedding pipeline is
                // completely reused without modification.
                let ts_ms = sample.timestamp * 1000.0;

                let (drained, ipc_ch, _count, band_snap, spec_col) = {
                    let sr = app.state::<Mutex<AppState>>();
                    let mut s = sr.lock_or_recover();

                    // Push one sample per channel.  The filter fires when all
                    // four channels have accumulated HOP (32) samples.
                    let mut filter_fired = false;
                    let mut band_fired   = false;
                    for (ch, &uv) in sample.eeg.iter().enumerate().take(EEG_CHANNELS) {
                        let one = [uv];
                        if ch < EEG_CHANNELS { s.status.eeg[ch] = uv; }

                        csv.push_eeg(ch, &one, sample.timestamp, EEG_SAMPLE_RATE);

                        if s.filter.push(ch, &one)       { filter_fired = true; }
                        if s.band_analyzer.push(ch, &one) { band_fired  = true; }
                        s.quality.push(ch, &one);
                        s.artifact_detector.push(ch, &one);
                        s.accumulator.push(ch, &[uv as f32]);
                    }
                    s.status.sample_count += 1;

                    let drained: Vec<(usize, Vec<f64>)> = if filter_fired {
                        (0..EEG_CHANNELS)
                            .map(|ch| (ch, s.filter.drain(ch)))
                            .filter(|(_, v)| !v.is_empty())
                            .collect()
                    } else { Vec::new() };

                    let spec_col = s.filter.take_spec_col();

                    let band_snap: Option<BandSnapshot> = if band_fired {
                        let snap = s.band_analyzer.latest.clone();
                        if let Some(ref sn) = snap { s.accumulator.update_bands(sn.clone()); }
                        snap
                    } else { None };

                    if filter_fired { s.status.channel_quality = s.quality.all_qualities(); }

                    // Accel from Ganglion
                    if let Some(accel) = sample.accel {
                        let a = [accel[0] as f32, accel[1] as f32, accel[2] as f32];
                        s.status.accel = a;
                        s.head_pose.update(a, [0.0f32; 3]);
                    }

                    let ipc = s.eeg_channel.clone();
                    (drained, ipc, s.status.sample_count, band_snap, spec_col)
                };

                // Forward filtered samples to frontend IPC
                if !drained.is_empty() {
                    for (ch, samples) in drained {
                        let pkt = EegPacket { electrode: ch, samples, timestamp: ts_ms };
                        if let Some(ref ipc_ch) = ipc_ch {
                            let _ = ipc_ch.send(pkt);
                        }
                    }
                }

                if let Some(col) = spec_col {
                    let _ = app.emit("eeg-spectrogram", &col);
                }
                if let Some(snap) = band_snap {
                    let _ = app.emit("eeg-bands", &snap);
                    app.state::<WsBroadcaster>().send("eeg-bands", &snap);
                }
            }
        }
    }

    // 9. Stop streaming and release the board (best-effort)
    let _ = board.stop_stream();
    // Wait for the bridge thread to exit
    let _ = bridge_handle.await;
    let _ = board.release();

    // 10. Finalise CSV + metadata
    csv.flush();
    write_session_meta(&app, &csv_path);

    if !user_cancelled {
        let r = app.state::<Mutex<AppState>>();
        let mut s = r.lock_or_recover();
        if s.status.sample_count > 0 { s.pending_reconnect = true; }
    }

    let error_msg = if user_cancelled { None } else { Some("DEVICE_DISCONNECTED".into()) };

    // Clear device_kind on disconnect
    {
        let r = app.state::<Mutex<AppState>>();
        let mut s = r.lock_or_recover();
        s.status.device_kind = "unknown".into();
    }
    go_disconnected(&app, error_msg, false);
}

// ── Generic OpenBCI session (all non-Ganglion-BLE boards) ────────────────────

/// Connect to any OpenBCI board (Ganglion WiFi, Cyton serial/WiFi, Galea…)
/// using the current `openbci_config` and run the EEG session loop.
///
/// The first `min(board.channel_count(), EEG_CHANNELS)` channels are routed
/// through the existing filter / band / embedding pipeline.  All channels are
/// written to the session CSV.
#[tauri::command]
async fn connect_openbci(app: AppHandle) -> Result<(), String> {
    use crate::settings::OpenBciBoard as Brd;

    // Read config under a short-lived lock
    let (board_kind, cfg) = {
        let r = app.state::<Mutex<AppState>>();
        let s = r.lock_or_recover();
        (s.openbci_config.board.clone(), s.openbci_config.clone())
    };

    if board_kind.is_ble() {
        return Err("Use the main Connect button for Ganglion BLE.".into());
    }

    // Guard: don't start if already active
    {
        let r = app.state::<Mutex<AppState>>();
        let s = r.lock_or_recover();
        if s.stream.is_some() {
            return Err("Already connected or connecting.".into());
        }
    }

    // Build the boxed Board
    let board: Box<dyn openbci::board::Board> = match board_kind.clone() {
        Brd::GanglionWifi => Box::new(GanglionWifiBoard::new(GanglionWifiConfig {
            shield_ip:    cfg.wifi_shield_ip.clone(),
            local_port:   cfg.wifi_local_port,
            http_timeout: 10,
        })),
        Brd::Cyton => {
            let port = if cfg.serial_port.is_empty() {
                serialport::available_ports()
                    .unwrap_or_default().into_iter().next()
                    .map(|p| p.port_name)
                    .ok_or("No serial ports found. Connect the USB dongle and try again.")?
            } else { cfg.serial_port.clone() };
            Box::new(CytonBoard::new(port))
        }
        Brd::CytonWifi => Box::new(CytonWifiBoard::new(CytonWifiConfig {
            shield_ip:    cfg.wifi_shield_ip.clone(),
            local_port:   cfg.wifi_local_port,
            http_timeout: 10,
        })),
        Brd::CytonDaisy => {
            let port = if cfg.serial_port.is_empty() {
                serialport::available_ports()
                    .unwrap_or_default().into_iter().next()
                    .map(|p| p.port_name)
                    .ok_or("No serial ports found. Connect the USB dongle and try again.")?
            } else { cfg.serial_port.clone() };
            Box::new(CytonDaisyBoard::new(port))
        }
        Brd::CytonDaisyWifi => Box::new(CytonDaisyWifiBoard::new(CytonDaisyWifiConfig {
            shield_ip:    cfg.wifi_shield_ip.clone(),
            local_port:   cfg.wifi_local_port,
            http_timeout: 10,
        })),
        Brd::Galea => Box::new(GaleaBoard::new(cfg.galea_ip.clone())),
        Brd::Ganglion => unreachable!(),
    };

    let ch_count    = board_kind.channel_count();
    let sample_rate = board_kind.sample_rate();
    let kind_str    = format!("openbci_{}", serde_json::to_string(&board_kind)
                              .unwrap_or_default().trim_matches('"'));

    // Register cancel channel + set "scanning" state
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let csv_path = {
        let r = app.state::<Mutex<AppState>>();
        let mut s = r.lock_or_recover();
        s.stream = Some(StreamHandle { cancel_tx: tx });
        s.status.state       = "scanning".into();
        s.status.device_kind = kind_str;
        new_csv_path(&app)
    };
    emit_status(&app);

    let app2 = app.clone();
    tokio::spawn(async move {
        run_openbci_board_session(app2, rx, csv_path, board, ch_count, sample_rate).await;
    });

    Ok(())
}

/// Board-agnostic session loop.  Called for every non-Ganglion-BLE board.
async fn run_openbci_board_session(
    app:         AppHandle,
    cancel_rx:   tokio::sync::oneshot::Receiver<()>,
    csv_path:    PathBuf,
    board:       Box<dyn openbci::board::Board>,
    ch_count:    usize,
    sample_rate: f64,
) {
    tokio::pin!(cancel_rx);

    // 1. Connect (blocking — serial open / TCP / UDP bind)
    let connect_result = tokio::select! {
        biased;
        _ = &mut cancel_rx => { go_disconnected(&app, None, false); return; }
        r = tokio::task::spawn_blocking(move || {
            let mut b = board;
            b.prepare().map(|_| b)
        }) => r,
    };

    let mut board = match connect_result {
        Ok(Ok(b))  => b,
        Ok(Err(e)) => {
            go_disconnected(&app, Some(format!("OpenBCI connect error: {e}")), false);
            return;
        }
        Err(e) => {
            go_disconnected(&app, Some(format!("OpenBCI thread error: {e}")), false);
            return;
        }
    };

    // 2. Mark connected; build channel labels
    let ch_labels: Vec<String> = {
        let r = app.state::<Mutex<AppState>>();
        let mut s = r.lock_or_recover();
        s.status.state = "connected".into();
        let cfg_labels = &s.openbci_config.channel_labels;
        let _defaults   = &s.openbci_config.board;
        (0..ch_count).map(|i| {
            cfg_labels.get(i)
                .filter(|l| !l.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("Ch{}", i + 1))
        }).collect()
    };
    emit_status(&app);

    // 3. Open CSV with all channel labels
    let label_refs: Vec<&str> = ch_labels.iter().map(|s| s.as_str()).collect();
    let mut csv = match CsvState::open_with_labels(&csv_path, &label_refs) {
        Ok(c)  => c,
        Err(e) => {
            write_session_meta(&app, &csv_path);
            go_disconnected(&app, Some(format!("CSV error: {e}")), false);
            return;
        }
    };
    write_session_meta(&app, &csv_path);

    // 4. Start streaming
    let stream_handle = match board.start_stream() {
        Ok(h)  => h,
        Err(e) => {
            go_disconnected(&app, Some(format!("OpenBCI start_stream: {e}")), false);
            return;
        }
    };

    // 5. Bridge blocking mpsc → async
    let (sample_tx, mut sample_rx) =
        tokio::sync::mpsc::channel::<openbci::sample::Sample>(256);
    let bridge = tokio::task::spawn_blocking(move || {
        while let Some(s) = stream_handle.recv() {
            if sample_tx.blocking_send(s).is_err() { break; }
        }
    });

    // 6. Event loop
    let pipeline_ch = ch_count.min(EEG_CHANNELS); // channels to analyse
    let mut user_cancelled = false;

    loop {
        tokio::select! {
            biased;
            _ = &mut cancel_rx => { user_cancelled = true; break; }

            maybe_sample = sample_rx.recv() => {
                let Some(sample) = maybe_sample else { break; };
                let ts_ms = sample.timestamp * 1000.0;

                let (drained, ipc_ch, band_snap, spec_col) = {
                    let sr = app.state::<Mutex<AppState>>();
                    let mut s = sr.lock_or_recover();

                    let mut filter_fired = false;
                    let mut band_fired   = false;

                    for (ch, &uv) in sample.eeg.iter().enumerate() {
                        let one = [uv];
                        // Write all channels to CSV
                        csv.push_eeg(ch, &one, sample.timestamp, sample_rate);

                        // Route first `pipeline_ch` channels through the analysis pipeline
                        if ch < pipeline_ch {
                            if ch < EEG_CHANNELS { s.status.eeg[ch] = uv; }
                            if s.filter.push(ch, &one)        { filter_fired = true; }
                            if s.band_analyzer.push(ch, &one) { band_fired   = true; }
                            s.quality.push(ch, &one);
                            s.artifact_detector.push(ch, &one);
                            s.accumulator.push(ch, &[uv as f32]);
                        }
                    }
                    s.status.sample_count += 1;

                    let drained: Vec<(usize, Vec<f64>)> = if filter_fired {
                        (0..pipeline_ch)
                            .map(|ch| (ch, s.filter.drain(ch)))
                            .filter(|(_, v)| !v.is_empty())
                            .collect()
                    } else { Vec::new() };

                    let spec_col  = s.filter.take_spec_col();
                    let band_snap: Option<BandSnapshot> = if band_fired {
                        let snap = s.band_analyzer.latest.clone();
                        if let Some(ref sn) = snap { s.accumulator.update_bands(sn.clone()); }
                        snap
                    } else { None };

                    let ipc = s.eeg_channel.clone();
                    (drained, ipc, band_snap, spec_col)
                };

                if !drained.is_empty() {
                    for (ch, samples) in drained {
                        let pkt = EegPacket { electrode: ch, samples, timestamp: ts_ms };
                        if let Some(ref ipc_ch) = ipc_ch {
                            let _ = ipc_ch.send(pkt);
                        }
                    }
                }
                if let Some(col) = spec_col {
                    let _ = app.emit("eeg-spectrogram", &col);
                }
                if let Some(snap) = band_snap {
                    let _ = app.emit("eeg-bands", &snap);
                    app.state::<WsBroadcaster>().send("eeg-bands", &snap);
                }
            }
        }
    }

    // 7. Clean up
    let _ = board.stop_stream();
    let _ = bridge.await;
    let _ = board.release();

    csv.flush();
    write_session_meta(&app, &csv_path);

    {
        let r = app.state::<Mutex<AppState>>();
        let mut s = r.lock_or_recover();
        s.status.device_kind = "unknown".into();
    }

    let error_msg = if user_cancelled { None } else { Some("DEVICE_DISCONNECTED".into()) };
    go_disconnected(&app, error_msg, false);
}

// ── Muse session ──────────────────────────────────────────────────────────────

async fn run_muse_session(
    app:          AppHandle,
    cancel_rx:    tokio::sync::oneshot::Receiver<()>,
    csv_path:     PathBuf,
    preferred_id: Option<String>,
) {
    tokio::pin!(cancel_rx);

    // 0. BT check
    if let Err((msg, is_bt)) = bluetooth_ok().await {
        go_disconnected(&app, Some(msg), is_bt); return;
    }

    // 1. → "scanning"
    {
        let r = app.state::<Mutex<AppState>>();
        let mut s = r.lock_or_recover();
        s.session_start_utc   = Some(unix_secs());
        s.status.state        = "scanning".into();
        s.status.device_kind  = "muse".into();
        s.status.device_name       = None;  // clear so we never show stale name during scan
        s.status.device_id         = None;
        s.status.serial_number     = None;
        s.status.mac_address       = None;
        s.status.firmware_version  = None;
        s.status.hardware_version  = None;
        s.status.bootloader_version = None;
        s.status.headset_preset    = None;
        s.status.csv_path          = Some(csv_path.to_string_lossy().into_owned());
        s.status.bt_error          = None;
        s.status.battery           = 0.0;
        s.status.eeg               = vec![f64::NAN; 4];
        s.status.sample_count      = 0;
        s.status.ppg               = vec![0.0; 3];
        s.status.ppg_sample_count  = 0;
        s.status.target_name  = preferred_id.as_ref().and_then(|id|
            s.status.paired_devices.iter().find(|d| &d.id == id).map(|d| d.name.clone())
        );
    }
    refresh_tray(&app); emit_status(&app);

    // 2. Scan
    let config = MuseClientConfig { scan_timeout_secs: 10, enable_ppg: true, ..Default::default() };
    let client = MuseClient::new(config);
    let all_devices = tokio::select! {
        biased;
        _ = &mut cancel_rx => { go_disconnected(&app, None, false); return; }
        r = client.scan_all() => match r {
            Err(e) => { let (m,b) = classify_bt_error(&e.to_string()); go_disconnected(&app, Some(m), b); return; }
            Ok(d)  => d,
        }
    };

    // 3. Pick device
    let device = match &preferred_id {
        Some(id) => all_devices.iter().find(|d| &d.id == id).or_else(|| all_devices.first()).cloned(),
        None     => all_devices.into_iter().next(),
    };
    let device = match device {
        Some(d) => d,
        None => {
            go_disconnected(&app, Some(
                "NO_MUSE_NEARBY".into()
            ), false);
            return;
        }
    };

    // 3b. Pin the real BLE ID into status now, before connect_to() takes ownership
    //     of `device`. This means Connected(name) in the event loop can always
    //     find the correct ID instead of falling back to the device name.
    //     Keep pending_reconnect=true so we auto-reconnect if the device
    //     disconnects unexpectedly (walks out of range, battery dies, etc.).
    {
        let sr = app.state::<Mutex<AppState>>();
        let mut g = sr.lock_or_recover();
        g.status.device_id    = Some(device.id.clone());
        g.retry_attempt       = 0; // reset backoff on successful discovery
    }

    // 4. Connect
    let (mut rx, handle) = tokio::select! {
        biased;
        _ = &mut cancel_rx => { go_disconnected(&app, None, false); return; }
        r = client.connect_to(device) => match r {
            Err(e) => { let (m,b) = classify_bt_error(&e.to_string()); go_disconnected(&app, Some(m), b); return; }
            Ok(v)  => v,
        }
    };

    // 5. Start streaming
    tokio::select! {
        biased;
        _ = &mut cancel_rx => { let _ = handle.disconnect().await; go_disconnected(&app, None, false); return; }
        r = handle.start(false, false) => { if let Err(e) = r { app_log!(app, "bluetooth", "[muse] start: {e}"); } }
    }

    // 6. Open CSV
    let mut csv = match CsvState::open(&csv_path) {
        Ok(c)  => c,
        Err(e) => {
            write_session_meta(&app, &csv_path);
            go_disconnected(&app, Some(format!("CSV error: {e}")), false);
            return;
        }
    };

    // 6b. Write initial JSON sidecar immediately so the file always exists,
    //     even if the app crashes or the device disconnects before session end.
    write_session_meta(&app, &csv_path);

    // 7. Event loop.
    //    Disconnect detection is handled inside muse-rs: it spawns a watcher
    //    task that listens for btleplug's CentralEvent::DeviceDisconnected and
    //    sends MuseEvent::Disconnected through the channel.  The notification
    //    stream also sends MuseEvent::Disconnected when GATT subscriptions end.
    //    Either way, handle_event() processes the disconnect (emits events,
    //    toast) and the loop exits when the channel closes (rx.recv() → None).
    let mut user_cancelled = false;
    loop {
        tokio::select! {
            biased;
            _ = &mut cancel_rx => {
                let _ = handle.disconnect().await;
                user_cancelled = true;
                break;
            }
            ev = rx.recv() => {
                match ev {
                    Some(e) => {
                        let is_disconnect = matches!(e, MuseEvent::Disconnected);
                        handle_event(e, &app, &mut csv, &csv_path).await;
                        if is_disconnect {
                            app_log!(app, "bluetooth", "[muse] event loop: received MuseEvent::Disconnected, breaking");
                            break;
                        }
                    }
                    None => {
                        app_log!(app, "bluetooth", "[muse] event loop: channel closed");
                        break;
                    }
                }
            }
        }
    }

    // 8. Finalise: flush CSV, overwrite JSON sidecar with final stats.
    csv.flush();
    write_session_meta(&app, &csv_path);

    // If the device disconnected unexpectedly (not user-cancelled) and we had
    // received at least some data, enable auto-retry with exponential backoff.
    if !user_cancelled {
        let r = app.state::<Mutex<AppState>>();
        let mut s = r.lock_or_recover();
        if s.status.sample_count > 0 {
            s.pending_reconnect = true;
        }
    }
    let error_msg = if user_cancelled { None } else { Some("DEVICE_DISCONNECTED".into()) };
    go_disconnected(&app, error_msg, false);
}

// ── Per-event handler ─────────────────────────────────────────────────────────

async fn handle_event(
    event:    MuseEvent,
    app:      &AppHandle,
    csv:      &mut CsvState,
    csv_path: &std::path::Path,
) {
    match event {
        // ── Connected ────────────────────────────────────────────────────────
        MuseEvent::Connected(name) => {
            // device_id was stored in run_muse_session (step 3b) from the real
            // MuseDevice.id before connect_to() was called — use it directly so
            // that the paired entry and the background-scanner entry share the
            // same BLE ID and never appear as duplicates.
            let dev_id = {
                let sr = app.state::<Mutex<AppState>>();
                let g  = sr.lock_or_recover();
                g.status.device_id.clone().unwrap_or_else(|| name.clone())
            };
            {
                let r = app.state::<Mutex<AppState>>();
                let mut s = r.lock_or_recover();
                s.status.state       = "connected".into();
                s.status.device_name = Some(name.clone());
                s.status.bt_error    = None;
                s.status.target_name = None;
                // Reset retry state on successful connection.
                s.retry_attempt                = 0;
                s.status.retry_attempt         = 0;
                s.status.retry_countdown_secs  = 0;
                // device_id already set; no name-based lookup needed
                s.accumulator.update_device(Some(dev_id.clone()), Some(name.clone()));
            }
            app_log!(app, "bluetooth", "[muse] connected: {name} (id={dev_id})");
            upsert_paired(app, &dev_id, &name);
            refresh_tray(app); emit_status(app); emit_devices(app);
            write_session_meta(app, csv_path); // update JSON with device name/id

            // Emit dedicated connection event for frontend and WS clients.
            let connect_payload = serde_json::json!({
                "device_name": name,
                "device_id":   dev_id,
                "timestamp":   unix_secs(),
            });
            let _ = app.emit("device-connected", &connect_payload);
            app.state::<WsBroadcaster>().send("device-connected", &connect_payload);

            send_toast(app, ToastLevel::Success, "Connected", &format!("{name} is now streaming EEG data."));
        }

        MuseEvent::Disconnected => {
            let (name, device_id) = {
                let sr = app.state::<Mutex<AppState>>();
                let g  = sr.lock_or_recover();
                (
                    g.status.device_name.clone().unwrap_or_else(|| "unknown".into()),
                    g.status.device_id.clone(),
                )
            };
            app_log!(app, "bluetooth", "[muse] disconnected: {name}");

            // Emit a dedicated disconnect event so the frontend and WS clients
            // can react immediately (before the session teardown completes).
            let disconnect_payload = serde_json::json!({
                "device_name": name,
                "device_id":   device_id,
                "timestamp":   unix_secs(),
                "reason":      "device_disconnected",
            });
            let _ = app.emit("device-disconnected", &disconnect_payload);
            app.state::<WsBroadcaster>().send("device-disconnected", &disconnect_payload);

            send_toast(app, ToastLevel::Warning, "Connection Lost", &format!("{name} disconnected."));
            // loop breaks on next iteration (rx closed)
        }

        // ── EEG ──────────────────────────────────────────────────────────────
        MuseEvent::Eeg(r) => {
            // 1. Compute per-packet timestamp for CSV (raw, always wall-clock or
            //    firmware milliseconds converted to seconds).
            let packet_ts_s = if r.timestamp > 0.0 {
                r.timestamp / 1000.0          // Classic firmware: ms → s
            } else {
                SystemTime::now()             // Athena firmware: timestamp = 0
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
            };

            // 2. Always write **raw** samples to CSV (scientific reproducibility).
            csv.push_eeg(r.electrode, &r.samples, packet_ts_s, EEG_SAMPLE_RATE);

            // 3. Update status and push to the GPU filter — one lock acquisition.
            //
            //    • `filter.push` either fires the GPU batch (active filter) or
            //      stores samples directly in `pending` (passthrough).
            //    • When `fired == true` we drain all 4 channels' `pending` queues
            //      inside the same lock so no concurrent mutation is possible.
            //    • The IPC channel is cloned out of the lock so we can `.send`
            //      outside the critical section (avoids holding the mutex across
            //      any I/O or wgpu synchronisation that `fft_batch` performs).
            let (drained, ipc_ch, count, band_snap, spec_col) = {
                let sr  = app.state::<Mutex<AppState>>();
                let mut s = sr.lock_or_recover();

                // Update last-known value and total sample count.
                if r.electrode < 4 {
                    if let Some(&v) = r.samples.last() {
                        s.status.eeg[r.electrode] = v;
                    }
                }
                s.status.sample_count += r.samples.len() as u64;
                let count = s.status.sample_count;

                // ── Signal filter (overlap-save, WINDOW=256) ─────────────────
                // Push to filter; GPU fft_batch + ifft_batch runs here if all
                // channels have accumulated HOP=32 new samples.
                let filter_fired = s.filter.push(r.electrode, &r.samples);

                // Drain filtered output while still under the lock.
                let drained: Vec<(usize, Vec<f64>)> = if filter_fired {
                    (0..EEG_CHANNELS)
                        .map(|ch| (ch, s.filter.drain(ch)))
                        .filter(|(_, v)| !v.is_empty())
                        .collect()
                } else {
                    Vec::new()
                };

                // ── Spectrogram column (zero extra GPU cost) ──────────────────
                // Extracted from the filter's fft_batch output as a side-effect.
                // `take_spec_col` clears the field so it is only emitted once.
                let spec_col: Option<SpectrogramColumn> = s.filter.take_spec_col();

                // ── Band power analyzer (Hann FFT, BAND_WINDOW=512) ──────────
                // Operates on **raw** samples (independent of the signal filter)
                // so that band powers reflect true spectral content, not the
                // filter's passband.  GPU fft_batch runs when all 4 channels
                // have accumulated BAND_HOP=64 new samples.
                let band_fired = s.band_analyzer.push(r.electrode, &r.samples);
                let band_snap: Option<BandSnapshot> = if band_fired {
                    let snap = s.band_analyzer.latest.clone();
                    // Feed the GPU-computed band snapshot to the accumulator so
                    // it can be attached to the next epoch — no duplicate FFT.
                    if let Some(ref sn) = snap {
                        s.accumulator.update_bands(sn.clone());
                    }
                    snap
                } else {
                    None
                };

                // ── Signal quality monitor (raw samples, CPU-only) ────────────
                // Push every packet; recompute all channels when the filter fires
                // so quality updates at the same cadence as the waveform (~8 Hz).
                s.quality.push(r.electrode, &r.samples);
                if filter_fired {
                    s.status.channel_quality = s.quality.all_qualities();
                }

                // ── Artifact detection (blinks) ───────────────────────────────
                s.artifact_detector.push(r.electrode, &r.samples);

                // ── ZUNA embedding accumulator (raw samples, wgpu offloaded) ──
                // Convert f64 → f32 (ZUNA works in f32) and feed into the
                // per-channel ring buffer.  When all 4 channels accumulate
                // EMBEDDING_EPOCH_SAMPLES (1 280 @ 256 Hz = 5 s) the epoch is
                // shipped to the background wgpu encoder thread.
                let samples_f32: Vec<f32> = r.samples.iter().map(|&v| v as f32).collect();
                s.accumulator.push(r.electrode, &samples_f32);

                let ipc = s.eeg_channel.clone();
                (drained, ipc, count, band_snap, spec_col)
            };

            // 4. Forward filtered (or passthrough) samples to the frontend
            //    (Tauri IPC channel) and to all WebSocket clients.
            //    Timestamp uses the current wall clock because the overlap-save
            //    algorithm introduces ~HOP/sample_rate ≈ 125 ms of latency.
            if !drained.is_empty() {
                let now_ts_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
                    * 1000.0;
                for (ch, samples) in drained {
                    let pkt = EegPacket { electrode: ch, samples, timestamp: now_ts_ms };
                    if let Some(ref ipc_ch) = ipc_ch {
                        let _ = ipc_ch.send(pkt);
                    }
                }
            }

            // Spectrogram column: every filter hop (8 Hz, 125 ms).
            if let Some(col) = spec_col {
                let _ = app.emit("eeg-spectrogram", &col);
            }

            // Band snapshot: every band hop (4 Hz, 250 ms).
            // Inject the latest cached PPG metrics so the frontend receives
            // heart-rate / HRV / SpO₂ alongside the EEG band powers.
            if let Some(mut snap) = band_snap {
                {
                    let sr = app.state::<Mutex<AppState>>();
                    let s  = sr.lock_or_recover();

                    // PPG metrics
                    if let Some(ppg) = s.accumulator.latest_ppg() {
                        snap.hr               = Some(ppg.hr);
                        snap.rmssd            = Some(ppg.rmssd);
                        snap.sdnn             = Some(ppg.sdnn);
                        snap.pnn50            = Some(ppg.pnn50);
                        snap.lf_hf_ratio      = Some(ppg.lf_hf_ratio);
                        snap.respiratory_rate = Some(ppg.respiratory_rate);
                        snap.spo2_estimate    = Some(ppg.spo2_estimate);
                        snap.perfusion_index  = Some(ppg.perfusion_index);
                        snap.stress_index     = Some(ppg.stress_index);
                    }

                    // Artifact detection (blinks)
                    let art = s.artifact_detector.metrics();
                    snap.blink_count = Some(art.blink_count);
                    snap.blink_rate  = Some(art.blink_rate);

                    // Head pose
                    let hp = s.head_pose.metrics();
                    snap.head_pitch  = Some(hp.pitch);
                    snap.head_roll   = Some(hp.roll);
                    snap.stillness   = Some(hp.stillness);
                    snap.nod_count   = Some(hp.nod_count);
                    snap.shake_count = Some(hp.shake_count);

                    // Temperature
                    if s.status.temperature_raw > 0 {
                        snap.temperature_raw = Some(s.status.temperature_raw);
                    }

                    // ── Composite scores ─────────────────────────────────────
                    // Meditation: high alpha + low beta + stillness + HRV
                    let alpha_dom = snap.channels.iter()
                        .map(|ch| ch.rel_alpha as f64).sum::<f64>() / 4.0;
                    let beta_dom = snap.channels.iter()
                        .map(|ch| ch.rel_beta as f64).sum::<f64>() / 4.0;
                    let alpha_component = (alpha_dom * 200.0).min(40.0); // 0–40
                    let beta_penalty = (beta_dom * 100.0).min(20.0);     // 0–20 (inverted)
                    let still_component = hp.stillness * 0.2;            // 0–20
                    let hrv_component = if let Some(ppg) = s.accumulator.latest_ppg() {
                        (ppg.rmssd / 100.0 * 20.0).min(20.0) // 0–20
                    } else { 10.0 }; // neutral if no PPG
                    let meditation = (alpha_component - beta_penalty + still_component + hrv_component)
                        .clamp(0.0, 100.0);
                    snap.meditation = Some((meditation * 10.0).round() / 10.0);

                    // Cognitive load: frontal theta / parietal alpha
                    // AF7 (ch 1) + AF8 (ch 2) theta / TP9 (ch 0) + TP10 (ch 3) alpha
                    let frontal_theta = (snap.channels[1].rel_theta as f64
                        + snap.channels[2].rel_theta as f64) / 2.0;
                    let parietal_alpha = (snap.channels[0].rel_alpha as f64
                        + snap.channels[3].rel_alpha as f64) / 2.0;
                    let cog_ratio = if parietal_alpha > 0.01 {
                        frontal_theta / parietal_alpha
                    } else { 1.0 };
                    // Map ratio 0–3 → 0–100 (sigmoid-like)
                    let cognitive_load = (100.0 / (1.0 + (-2.5 * (cog_ratio - 1.0)).exp()))
                        .clamp(0.0, 100.0);
                    snap.cognitive_load = Some((cognitive_load * 10.0).round() / 10.0);

                    // Drowsiness: TAR + alpha spindle proxy
                    let tar = snap.tar as f64;
                    // TAR > 1.5 → drowsy.  Map 0–3 → 0–100.
                    let tar_component = (tar / 3.0 * 80.0).min(80.0);
                    // Alpha dominance as spindle proxy.
                    let alpha_spindle = (alpha_dom * 100.0).min(20.0);
                    let drowsiness = (tar_component + alpha_spindle).clamp(0.0, 100.0);
                    snap.drowsiness = Some((drowsiness * 10.0).round() / 10.0);
                }
                // GPU utilisation (sampled at same rate as metrics, ~4 Hz)
                #[cfg(target_os = "macos")]
                if let Some(gpu) = gpu_stats::read() {
                    snap.gpu_overall = Some(gpu.overall as f64);
                    snap.gpu_render  = Some(gpu.render as f64);
                    snap.gpu_tiler   = Some(gpu.tiler as f64);
                }

                // Write derived metrics row to _metrics.csv (~4 Hz).
                csv.push_metrics(csv_path, &snap);

                let _ = app.emit("eeg-bands", &snap);
                app.state::<WsBroadcaster>().send("eeg-bands", &snap);
            }

            if count % 256 == 0 { emit_status(app); }
        }

        // ── PPG ───────────────────────────────────────────────────────────────
        MuseEvent::Ppg(r) => {
            let packet_ts_s = if r.timestamp > 0.0 {
                r.timestamp / 1000.0
            } else {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
            };

            let samples_f64: Vec<f64> = r.samples.iter().map(|&v| v as f64).collect();

            // Update status, accumulate for epoch storage, forward to frontend.
            let (ipc, ppg_vitals) = {
                let sr = app.state::<Mutex<AppState>>();
                let mut s = sr.lock_or_recover();
                if r.ppg_channel < 3 {
                    if let Some(last) = samples_f64.last() {
                        s.status.ppg[r.ppg_channel] = *last;
                    }
                }
                s.status.ppg_sample_count += samples_f64.len() as u64;
                // Feed PPG into epoch accumulator for SQLite storage.
                s.accumulator.push_ppg(r.ppg_channel, &samples_f64);
                let vitals = s.accumulator.latest_ppg().cloned();
                (s.ppg_channel.clone(), vitals)
            };

            // Write to PPG CSV (with vitals columns when available).
            csv.push_ppg(csv_path, r.ppg_channel, &samples_f64, packet_ts_s, ppg_vitals.as_ref());
            if let Some(ch) = ipc {
                let now_ms = packet_ts_s * 1000.0;
                let _ = ch.send(PpgPacket {
                    channel:   r.ppg_channel,
                    samples:   samples_f64,
                    timestamp: now_ms,
                });
            }
        }

        // ── Accelerometer ─────────────────────────────────────────────────────
        MuseEvent::Accelerometer(imu) => {
            let sr = app.state::<Mutex<AppState>>();
            let mut s = sr.lock_or_recover();
            let last = imu.samples[2];
            s.status.accel = [last.x, last.y, last.z];
            // Head pose is updated in the gyro handler (needs both sensors).
            let ipc = s.imu_channel.clone();
            drop(s);
            if let Some(ch) = ipc {
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64() * 1000.0;
                let _ = ch.send(ImuPacket {
                    sensor: "accel".into(),
                    samples: [
                        [imu.samples[0].x, imu.samples[0].y, imu.samples[0].z],
                        [imu.samples[1].x, imu.samples[1].y, imu.samples[1].z],
                        [imu.samples[2].x, imu.samples[2].y, imu.samples[2].z],
                    ],
                    timestamp: now_ms,
                });
            }
        }

        // ── Gyroscope ─────────────────────────────────────────────────────────
        MuseEvent::Gyroscope(imu) => {
            let sr = app.state::<Mutex<AppState>>();
            let mut s = sr.lock_or_recover();
            let last = imu.samples[2];
            s.status.gyro = [last.x, last.y, last.z];
            // Feed all 3 samples to head pose tracker (paired with latest accel).
            let accel = s.status.accel;
            for sample in &imu.samples {
                s.head_pose.update(accel, [sample.x, sample.y, sample.z]);
            }
            let ipc = s.imu_channel.clone();
            drop(s);
            if let Some(ch) = ipc {
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64() * 1000.0;
                let _ = ch.send(ImuPacket {
                    sensor: "gyro".into(),
                    samples: [
                        [imu.samples[0].x, imu.samples[0].y, imu.samples[0].z],
                        [imu.samples[1].x, imu.samples[1].y, imu.samples[1].z],
                        [imu.samples[2].x, imu.samples[2].y, imu.samples[2].z],
                    ],
                    timestamp: now_ms,
                });
            }
        }

        // ── Telemetry (battery) ───────────────────────────────────────────────
        MuseEvent::Telemetry(t) => {
            const ALPHA: f32 = 0.1;
            let r = app.state::<Mutex<AppState>>();
            let mut s = r.lock_or_recover();
            let prev_battery = s.status.battery;
            let first_reading = s.battery_ema.is_none();
            let smoothed = match s.battery_ema {
                None    => t.battery_level,
                Some(v) => ALPHA * t.battery_level + (1.0 - ALPHA) * v,
            };
            s.battery_ema    = Some(smoothed);
            s.status.battery = smoothed;
            s.status.fuel_gauge_mv  = t.fuel_gauge_voltage;
            s.status.temperature_raw = t.temperature;
            drop(s);
            emit_status(app);
            // Update JSON sidecar on first battery reading so it has a value
            // even if the app crashes before session end.
            if first_reading { write_session_meta(app, csv_path); }
            // Low-battery toast: fire once when crossing below 20% or 10%.
            if smoothed < 10.0 && prev_battery >= 10.0 {
                send_toast(app, ToastLevel::Error, "Battery Critical",
                    &format!("Battery at {:.0}% — charge soon.", smoothed));
            } else if smoothed < 20.0 && prev_battery >= 20.0 {
                send_toast(app, ToastLevel::Warning, "Low Battery",
                    &format!("Battery at {:.0}% — consider charging.", smoothed));
            }
        }

        MuseEvent::Control(c) => {
            app_log!(app, "bluetooth", "[muse] ctrl: {}", c.raw);
            // The device-info response (`v1` / `s` command reply) contains:
            //   "sn" → factory serial number (e.g. "AAAA-BBBB-CCCC")
            //   "ma" → hardware MAC address  (e.g. "AA-BB-CC-DD-EE-FF")
            // We grab whichever fields are present and update status so the
            // frontend can display them. Other control responses (rc-only, etc.)
            // simply won't have these keys and are silently ignored here.
            let sn = c.fields.get("sn").and_then(|v| v.as_str()).map(str::to_owned);
            let ma = c.fields.get("ma").and_then(|v| v.as_str()).map(str::to_owned);
            let fw = c.fields.get("fw").and_then(|v| v.as_str()).map(str::to_owned);
            let hw = c.fields.get("hw").and_then(|v| v.as_str()).map(str::to_owned);
            let bl = c.fields.get("bl").and_then(|v| v.as_str()).map(str::to_owned);
            let tp = c.fields.get("tp").and_then(|v| v.as_str()).map(str::to_owned);
            if sn.is_some() || ma.is_some() || fw.is_some() || hw.is_some() {
                let r = app.state::<Mutex<AppState>>();
                let mut s = r.lock_or_recover();
                if let Some(v) = sn { s.status.serial_number     = Some(v); }
                if let Some(v) = ma { s.status.mac_address       = Some(v); }
                if let Some(v) = fw { s.status.firmware_version  = Some(v); }
                if let Some(v) = hw { s.status.hardware_version  = Some(v); }
                if let Some(v) = bl { s.status.bootloader_version = Some(v); }
                if let Some(v) = tp { s.status.headset_preset    = Some(v); }
                drop(s);
                emit_status(app);
                write_session_meta(app, csv_path); // update JSON with new device info
            }
        }
    }
}

// ── History window + commands ──────────────────────────────────────────────────

#[tauri::command]
async fn open_history_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("history") {
        let _ = win.show(); let _ = win.set_focus(); return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "history", tauri::WebviewUrl::App("history".into()))
        .title("NeuroSkill™ – History")
        .inner_size(920.0, 780.0)
        .min_inner_size(700.0, 560.0)
        .resizable(true)
        .center()
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// A session entry read from a JSON sidecar file.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct SessionEntry {
    csv_file:          String,
    csv_path:          String,
    session_start_utc: Option<u64>,
    session_end_utc:   Option<u64>,
    session_duration_s: Option<u64>,
    device_name:       Option<String>,
    device_id:         Option<String>,
    serial_number:     Option<String>,
    mac_address:       Option<String>,
    firmware_version:  Option<String>,
    hardware_version:  Option<String>,
    headset_preset:    Option<String>,
    battery_pct:       Option<f64>,
    total_samples:     Option<u64>,
    sample_rate_hz:    Option<u64>,
    labels:            Vec<label_store::LabelRow>,
    file_size_bytes:   u64,
}

/// Scan all `~/.skill/*/muse_*.json` sidecar files and return session entries
/// sorted by start time descending (newest first).
#[tauri::command]
fn list_sessions(state: tauri::State<'_, Mutex<AppState>>) -> Vec<SessionEntry> {
    let (skill_dir, logger) = {
        let s = state.lock_or_recover();
        (s.skill_dir.clone(), s.logger.clone())
    };

    skill_log!(logger, "history", "scanning {:?}", skill_dir);

    // 1. Scan all JSON sidecar files (no lock needed)
    let mut raw: Vec<(SessionEntry, Option<u64>, Option<u64>)> = Vec::new();

    let entries = match std::fs::read_dir(&skill_dir) {
        Ok(e) => e,
        Err(e) => { skill_log!(logger, "history", "read_dir failed: {e}"); return vec![]; },
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let dir_files = match std::fs::read_dir(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        // Collect all files in this date directory
        let files: Vec<_> = dir_files.filter_map(|e| e.ok()).collect();

        // First pass: find JSON sidecars
        for jf in &files {
            let jp = jf.path();
            let fname = jp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !fname.starts_with("muse_") || !fname.ends_with(".json") { continue; }

            let json_str = match std::fs::read_to_string(&jp) {
                Ok(s) => s,
                Err(e) => { skill_log!(logger, "history", "read error: {e}"); continue; },
            };
            let meta: serde_json::Value = match serde_json::from_str(&json_str) {
                Ok(v) => v,
                Err(e) => { skill_log!(logger, "history", "parse error: {e}"); continue; },
            };

            skill_log!(logger, "history", "sidecar {:?}: start={} end={} samples={}",
                jp,
                meta.get("session_start_utc").map(|v| v.to_string()).unwrap_or("null".into()),
                meta.get("session_end_utc").map(|v| v.to_string()).unwrap_or("null".into()),
                meta.get("total_samples").map(|v| v.to_string()).unwrap_or("null".into()),
            );

            let csv_file = meta["csv_file"].as_str().unwrap_or("").to_string();
            let csv_full = path.join(&csv_file);
            let csv_size = std::fs::metadata(&csv_full).map(|m| m.len()).unwrap_or(0);

            let start = meta["session_start_utc"].as_u64();
            let end   = meta["session_end_utc"].as_u64();

            // Support both old flat format and new nested "device" format.
            let dev = meta.get("device");
            let str_field = |obj: Option<&serde_json::Value>, nested_key: &str, flat_key: &str| -> Option<String> {
                obj.and_then(|d| d.get(nested_key)).and_then(|v| v.as_str()).map(str::to_owned)
                    .or_else(|| meta.get(flat_key).and_then(|v| v.as_str()).map(str::to_owned))
            };

            raw.push((SessionEntry {
                csv_file,
                csv_path:           csv_full.to_string_lossy().into_owned(),
                session_start_utc:  start,
                session_end_utc:    end,
                session_duration_s: meta.get("session_duration_s").and_then(|v| v.as_u64())
                                        .or_else(|| start.zip(end).map(|(s, e)| e.saturating_sub(s))),
                device_name:        str_field(dev, "name", "device_name"),
                device_id:          str_field(dev, "id", "device_id"),
                serial_number:      str_field(dev, "serial_number", "serial_number"),
                mac_address:        str_field(dev, "mac_address", "mac_address"),
                firmware_version:   str_field(dev, "firmware_version", "firmware_version"),
                hardware_version:   str_field(dev, "hardware_version", "hardware_version"),
                headset_preset:     str_field(dev, "preset", "headset_preset"),
                battery_pct:        meta.get("battery_pct_end").and_then(|v| v.as_f64())
                                        .or_else(|| meta.get("battery_pct").and_then(|v| v.as_f64())),
                total_samples:      meta["total_samples"].as_u64(),
                sample_rate_hz:     meta["sample_rate_hz"].as_u64(),
                labels:             vec![],
                file_size_bytes:    csv_size,
            }, start, end));
        }

        // Second pass: find orphaned CSVs (no JSON sidecar)
        for cf in &files {
            let cp = cf.path();
            let cfname = cp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !cfname.starts_with("muse_") || !cfname.ends_with(".csv") { continue; }
            // Skip derived companion files — _metrics.csv and _ppg.csv are not sessions.
            if cfname.ends_with("_metrics.csv") || cfname.ends_with("_ppg.csv") { continue; }
            let json_path = cp.with_extension("json");
            if json_path.exists() { continue; } // already handled above

            let meta = std::fs::metadata(&cp);
            let csv_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            // Start time from filename: muse_{ts}.csv
            let ts: Option<u64> = cfname
                .strip_prefix("muse_")
                .and_then(|s| s.strip_suffix(".csv"))
                .and_then(|s| s.parse().ok());
            // End time from file modification time
            let end_ts: Option<u64> = meta.ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            // Estimate sample count from CSV line count (minus header)
            let samples: Option<u64> = std::fs::read_to_string(&cp).ok()
                .map(|s| {
                    let lines = s.lines().count();
                    if lines > 1 { (lines - 1) as u64 } else { 0 }
                });

            skill_log!(logger, "history", "found orphan CSV: {:?} start={:?} end={:?} samples={:?}", cp, ts, end_ts, samples);

            raw.push((SessionEntry {
                csv_file:           cfname.to_string(),
                csv_path:           cp.to_string_lossy().into_owned(),
                session_start_utc:  ts,
                session_end_utc:    end_ts,
                session_duration_s: ts.zip(end_ts).map(|(s, e)| e.saturating_sub(s)),
                device_name:        None,
                device_id:          None,
                serial_number:      None,
                mac_address:        None,
                firmware_version:   None,
                hardware_version:   None,
                headset_preset:     None,
                battery_pct:        None,
                total_samples:      samples,
                sample_rate_hz:     Some(256),
                labels:             vec![],
                file_size_bytes:    csv_size,
            }, ts, end_ts));
        }
    }

    // Override start/end/duration with ground-truth timestamps from _metrics.csv.
    patch_session_timestamps(&mut raw);

    // 2. Re-acquire lock briefly to query labels for each session
    {
        let s = state.lock_or_recover();
        if let Some(store) = &s.label_store {
            for (session, start, end) in raw.iter_mut() {
                if let (Some(s), Some(e)) = (start, end) {
                    session.labels = store.query_range(*s, *e);
                }
            }
        }
    }

    let mut sessions: Vec<SessionEntry> = raw.into_iter().map(|(s, _, _)| s).collect();
    sessions.sort_by(|a, b| b.session_start_utc.cmp(&a.session_start_utc));
    skill_log!(logger, "history", "returning {} sessions", sessions.len());
    sessions
}

// ── CSV timestamp helpers ─────────────────────────────────────────────────────

/// Read the first and last valid `timestamp_s` (column 0) from a
/// `_metrics.csv` file.  Used to fix session start/end/duration when the
/// JSON sidecar is missing or was written before the session ended cleanly
/// (e.g. after a crash).
///
/// Returns `Some((first_unix_secs, last_unix_secs))` or `None` if the file
/// does not exist or contains no valid rows.
fn read_metrics_csv_time_range(metrics_path: &std::path::Path) -> Option<(u64, u64)> {
    if !metrics_path.exists() { return None; }

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(metrics_path)
        .ok()?;

    let mut first: Option<u64> = None;
    let mut last:  Option<u64> = None;

    for result in rdr.records() {
        let rec = match result { Ok(r) => r, Err(_) => continue };
        // Column 0 is `timestamp_s` — a Unix-seconds float (e.g. 1700000047.123456)
        let ts = match rec.get(0).and_then(|s| s.parse::<f64>().ok()) {
            Some(t) if t > 1_000_000_000.0 => t as u64,   // sanity: after year 2001
            _ => continue,
        };
        if first.is_none() { first = Some(ts); }
        last = Some(ts);
    }

    Some((first?, last?))
}

/// Apply `_metrics.csv` timestamps to a list of `(SessionEntry, start, end)`
/// triples.  Overwrites `session_start_utc`, `session_end_utc`, and
/// `session_duration_s` whenever the metrics file provides tighter, verified
/// bounds.  The `start`/`end` references must stay in sync because they are
/// used downstream for label hydration.
fn patch_session_timestamps(raw: &mut [(SessionEntry, Option<u64>, Option<u64>)]) {
    for (session, start, end) in raw.iter_mut() {
        let metrics_path = metrics_csv_path(std::path::Path::new(&session.csv_path));
        if let Some((first_ts, last_ts)) = read_metrics_csv_time_range(&metrics_path) {
            *start                     = Some(first_ts);
            *end                       = Some(last_ts);
            session.session_start_utc  = Some(first_ts);
            session.session_end_utc    = Some(last_ts);
            session.session_duration_s = Some(last_ts.saturating_sub(first_ts));
        }
    }
}

/// Return recording day directories as `YYYYMMDD` strings, newest first.
///
/// Only directories that contain at least one valid session file are returned:
///   • a `muse_*.json` sidecar, OR
///   • an orphaned `muse_*.csv` with no matching sidecar
///
/// This filters out dirs that hold only log files (or are completely empty),
/// so the frontend never has to handle an "empty day" as the default view.
#[tauri::command]
fn list_session_days(state: tauri::State<'_, Mutex<AppState>>) -> Vec<String> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    let mut days: Vec<String> = std::fs::read_dir(&skill_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            if !(s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) && e.path().is_dir()) {
                return None;
            }
            // Check for at least one valid session file before including this day.
            let has_sessions = std::fs::read_dir(e.path())
                .into_iter()
                .flatten()
                .flatten()
                .any(|f| {
                    let fname = f.file_name();
                    let fname = fname.to_string_lossy();
                    if fname.starts_with("muse_") && fname.ends_with(".json") {
                        return true; // JSON sidecar
                    }
                    if fname.starts_with("muse_") && fname.ends_with(".csv") {
                        // Skip derived companion files — _metrics.csv and _ppg.csv are not sessions.
                        if fname.ends_with("_metrics.csv") || fname.ends_with("_ppg.csv") {
                            return false;
                        }
                        // Orphaned CSV — only counts if there is no sidecar
                        return !f.path().with_extension("json").exists();
                    }
                    false
                });
            if has_sessions { Some(s.to_string()) } else { None }
        })
        .collect();
    days.sort_by(|a, b| b.cmp(a)); // newest first
    days
}

/// Load all sessions belonging to a single recording day (`YYYYMMDD`).
/// This is the async-friendly counterpart to `list_sessions` — callers
/// iterate over `list_session_days()` and invoke this for each day in turn.
#[tauri::command]
fn list_sessions_for_day(
    day: String,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Vec<SessionEntry> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    let day_dir = skill_dir.join(&day);
    if !day_dir.is_dir() { return vec![]; }

    let files: Vec<_> = std::fs::read_dir(&day_dir)
        .into_iter().flatten().flatten().collect();
    let mut raw: Vec<(SessionEntry, Option<u64>, Option<u64>)> = Vec::new();

    // First pass: JSON sidecars
    for jf in &files {
        let jp = jf.path();
        let fname = jp.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !fname.starts_with("muse_") || !fname.ends_with(".json") { continue; }

        let json_str = match std::fs::read_to_string(&jp) { Ok(s) => s, Err(_) => continue };
        let meta: serde_json::Value = match serde_json::from_str(&json_str) { Ok(v) => v, Err(_) => continue };

        let csv_file = meta["csv_file"].as_str().unwrap_or("").to_string();
        let csv_full = day_dir.join(&csv_file);
        let csv_size = std::fs::metadata(&csv_full).map(|m| m.len()).unwrap_or(0);
        let start = meta["session_start_utc"].as_u64();
        let end   = meta["session_end_utc"].as_u64();
        let dev   = meta.get("device");
        let str_field = |obj: Option<&serde_json::Value>, nk: &str, fk: &str| -> Option<String> {
            obj.and_then(|d| d.get(nk)).and_then(|v| v.as_str()).map(str::to_owned)
                .or_else(|| meta.get(fk).and_then(|v| v.as_str()).map(str::to_owned))
        };
        raw.push((SessionEntry {
            csv_file,
            csv_path:           csv_full.to_string_lossy().into_owned(),
            session_start_utc:  start,
            session_end_utc:    end,
            session_duration_s: meta.get("session_duration_s").and_then(|v| v.as_u64())
                                    .or_else(|| start.zip(end).map(|(s, e)| e.saturating_sub(s))),
            device_name:        str_field(dev, "name", "device_name"),
            device_id:          str_field(dev, "id", "device_id"),
            serial_number:      str_field(dev, "serial_number", "serial_number"),
            mac_address:        str_field(dev, "mac_address", "mac_address"),
            firmware_version:   str_field(dev, "firmware_version", "firmware_version"),
            hardware_version:   str_field(dev, "hardware_version", "hardware_version"),
            headset_preset:     str_field(dev, "preset", "headset_preset"),
            battery_pct:        meta.get("battery_pct_end").and_then(|v| v.as_f64())
                                    .or_else(|| meta.get("battery_pct").and_then(|v| v.as_f64())),
            total_samples:      meta["total_samples"].as_u64(),
            sample_rate_hz:     meta["sample_rate_hz"].as_u64(),
            labels:             vec![],
            file_size_bytes:    csv_size,
        }, start, end));
    }

    // Second pass: orphaned CSVs
    for cf in &files {
        let cp = cf.path();
        let cfname = cp.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !cfname.starts_with("muse_") || !cfname.ends_with(".csv") { continue; }
        // Skip derived companion files — _metrics.csv and _ppg.csv are not sessions.
        if cfname.ends_with("_metrics.csv") || cfname.ends_with("_ppg.csv") { continue; }
        if cp.with_extension("json").exists() { continue; }
        let meta_fs = std::fs::metadata(&cp);
        let csv_size = meta_fs.as_ref().map(|m| m.len()).unwrap_or(0);
        let ts: Option<u64> = cfname.strip_prefix("muse_")
            .and_then(|s| s.strip_suffix(".csv"))
            .and_then(|s| s.parse().ok());
        let end_ts: Option<u64> = meta_fs.ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        raw.push((SessionEntry {
            csv_file:           cfname.to_string(),
            csv_path:           cp.to_string_lossy().into_owned(),
            session_start_utc:  ts,
            session_end_utc:    end_ts,
            session_duration_s: ts.zip(end_ts).map(|(s, e)| e.saturating_sub(s)),
            device_name:        None, device_id: None, serial_number: None,
            mac_address:        None, firmware_version: None, hardware_version: None,
            headset_preset:     None,
            battery_pct:        None,
            total_samples:      None,
            sample_rate_hz:     Some(256),
            labels:             vec![],
            file_size_bytes:    csv_size,
        }, ts, end_ts));
    }

    // Override start/end/duration with ground-truth timestamps from _metrics.csv.
    // This fixes orphaned CSVs (where end = unreliable mtime) and sessions whose
    // sidecar was only written at session-start (app crashed before clean shutdown).
    patch_session_timestamps(&mut raw);

    // Hydrate labels
    {
        let s = state.lock_or_recover();
        if let Some(store) = &s.label_store {
            for (session, start, end) in raw.iter_mut() {
                if let (Some(s), Some(e)) = (start, end) {
                    session.labels = store.query_range(*s, *e);
                }
            }
        }
    }

    let mut sessions: Vec<SessionEntry> = raw.into_iter().map(|(s, _, _)| s).collect();
    sessions.sort_by(|a, b| b.session_start_utc.cmp(&a.session_start_utc));
    sessions
}

// ── Streaming session list ────────────────────────────────────────────────────

/// An event emitted by `stream_sessions`.
///
/// Sequence:
/// 1. `{ kind:"started", total_days:N }`
/// 2. N × `{ kind:"day",  day:"YYYYMMDD", sessions:[…] }`
/// 3. `{ kind:"done",  total_sessions:N }`
#[derive(Serialize, Clone)]
struct SessionStreamEvent {
    kind:           String,
    /// "started" only
    #[serde(skip_serializing_if = "Option::is_none")]
    total_days:     Option<usize>,
    /// "day" only
    #[serde(skip_serializing_if = "Option::is_none")]
    day:            Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sessions:       Option<Vec<SessionEntry>>,
    /// "done" only
    #[serde(skip_serializing_if = "Option::is_none")]
    total_sessions: Option<usize>,
}

/// Stream all recorded sessions to the frontend one day at a time.
///
/// Each channel message is a `SessionStreamEvent`.  All file I/O runs on a
/// Tokio blocking thread so the async runtime and the UI are never stalled.
#[tauri::command]
async fn stream_sessions(
    on_event: tauri::ipc::Channel<SessionStreamEvent>,
    state:    tauri::State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();

    tokio::task::spawn_blocking(move || {
        // ── 1. Enumerate day directories ─────────────────────────────────
        let mut days: Vec<String> = std::fs::read_dir(&skill_dir)
            .into_iter().flatten().flatten()
            .filter_map(|e| {
                let name = e.file_name();
                let s    = name.to_string_lossy();
                if s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit())
                   && e.path().is_dir()
                {
                    Some(s.to_string())
                } else {
                    None
                }
            })
            .collect();
        days.sort_by(|a, b| b.cmp(a)); // newest first

        let _ = on_event.send(SessionStreamEvent {
            kind:           "started".into(),
            total_days:     Some(days.len()),
            day:            None,
            sessions:       None,
            total_sessions: None,
        });

        // Open a read-only label store on this thread for label hydration.
        let label_store = label_store::LabelStore::open(&skill_dir);

        // ── 2. Load each day and emit one event per day ───────────────────
        let mut total_sessions = 0usize;
        for day in &days {
            let day_dir = skill_dir.join(day);
            if !day_dir.is_dir() { continue; }

            let files: Vec<_> = match std::fs::read_dir(&day_dir) {
                Ok(rd) => rd.flatten().collect(),
                Err(_) => continue,
            };
            let mut raw: Vec<(SessionEntry, Option<u64>, Option<u64>)> = Vec::new();

            // JSON sidecars
            for jf in &files {
                let jp    = jf.path();
                let fname = jp.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !fname.starts_with("muse_") || !fname.ends_with(".json") { continue; }

                let json_str = match std::fs::read_to_string(&jp) { Ok(s) => s, Err(_) => continue };
                let meta: serde_json::Value = match serde_json::from_str(&json_str) { Ok(v) => v, Err(_) => continue };

                let csv_file = meta["csv_file"].as_str().unwrap_or("").to_string();
                let csv_full = day_dir.join(&csv_file);
                let csv_size = std::fs::metadata(&csv_full).map(|m| m.len()).unwrap_or(0);
                let start    = meta["session_start_utc"].as_u64();
                let end      = meta["session_end_utc"].as_u64();
                let dev      = meta.get("device");
                let str_field = |obj: Option<&serde_json::Value>, nk: &str, fk: &str| -> Option<String> {
                    obj.and_then(|d| d.get(nk)).and_then(|v| v.as_str()).map(str::to_owned)
                        .or_else(|| meta.get(fk).and_then(|v| v.as_str()).map(str::to_owned))
                };
                raw.push((SessionEntry {
                    csv_file,
                    csv_path:           csv_full.to_string_lossy().into_owned(),
                    session_start_utc:  start,
                    session_end_utc:    end,
                    session_duration_s: meta.get("session_duration_s").and_then(|v| v.as_u64())
                                            .or_else(|| start.zip(end).map(|(s, e)| e.saturating_sub(s))),
                    device_name:        str_field(dev, "name",             "device_name"),
                    device_id:          str_field(dev, "id",               "device_id"),
                    serial_number:      str_field(dev, "serial_number",    "serial_number"),
                    mac_address:        str_field(dev, "mac_address",      "mac_address"),
                    firmware_version:   str_field(dev, "firmware_version", "firmware_version"),
                    hardware_version:   str_field(dev, "hardware_version", "hardware_version"),
                    headset_preset:     str_field(dev, "preset",           "headset_preset"),
                    battery_pct:        meta.get("battery_pct_end").and_then(|v| v.as_f64())
                                            .or_else(|| meta.get("battery_pct").and_then(|v| v.as_f64())),
                    total_samples:      meta["total_samples"].as_u64(),
                    sample_rate_hz:     meta["sample_rate_hz"].as_u64(),
                    labels:             vec![],
                    file_size_bytes:    csv_size,
                }, start, end));
            }

            // Orphaned CSVs (no sidecar JSON)
            for cf in &files {
                let cp    = cf.path();
                let cfname = cp.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !cfname.starts_with("muse_") || !cfname.ends_with(".csv") { continue; }
                // Skip derived companion files — _metrics.csv and _ppg.csv are not sessions.
                if cfname.ends_with("_metrics.csv") || cfname.ends_with("_ppg.csv") { continue; }
                if cp.with_extension("json").exists() { continue; }
                let meta_fs  = std::fs::metadata(&cp);
                let csv_size = meta_fs.as_ref().map(|m| m.len()).unwrap_or(0);
                let ts: Option<u64> = cfname.strip_prefix("muse_")
                    .and_then(|s| s.strip_suffix(".csv"))
                    .and_then(|s| s.parse().ok());
                let end_ts: Option<u64> = meta_fs.ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());
                raw.push((SessionEntry {
                    csv_file:           cfname.to_string(),
                    csv_path:           cp.to_string_lossy().into_owned(),
                    session_start_utc:  ts,
                    session_end_utc:    end_ts,
                    session_duration_s: ts.zip(end_ts).map(|(s, e)| e.saturating_sub(s)),
                    device_name:        None, device_id: None, serial_number: None,
                    mac_address:        None, firmware_version: None, hardware_version: None,
                    headset_preset:     None,
                    battery_pct:        None,
                    total_samples:      None,
                    sample_rate_hz:     Some(256),
                    labels:             vec![],
                    file_size_bytes:    csv_size,
                }, ts, end_ts));
            }

            // Override start/end/duration with ground-truth timestamps from _metrics.csv.
            patch_session_timestamps(&mut raw);

            // Hydrate labels
            if let Some(store) = &label_store {
                for (session, start, end) in raw.iter_mut() {
                    if let (Some(s), Some(e)) = (start, end) {
                        session.labels = store.query_range(*s, *e);
                    }
                }
            }

            let mut sessions: Vec<SessionEntry> = raw.into_iter().map(|(s, _, _)| s).collect();
            sessions.sort_by(|a, b| b.session_start_utc.cmp(&a.session_start_utc));
            total_sessions += sessions.len();

            let _ = on_event.send(SessionStreamEvent {
                kind:           "day".into(),
                total_days:     None,
                day:            Some(day.clone()),
                sessions:       Some(sessions),
                total_sessions: None,
            });
        }

        // ── 3. Signal completion ──────────────────────────────────────────
        let _ = on_event.send(SessionStreamEvent {
            kind:           "done".into(),
            total_days:     None,
            day:            None,
            sessions:       None,
            total_sessions: Some(total_sessions),
        });
    })
    .await
    .map_err(|e| e.to_string())
}

/// Aggregate history stats — total sessions/hours and week-over-week breakdown.
/// Scans only JSON sidecars (fast), never reads CSV data.
#[derive(Serialize)]
struct HistoryStats {
    total_sessions: usize,
    total_secs:     u64,
    this_week_secs: u64,
    last_week_secs: u64,
}

#[tauri::command]
async fn get_history_stats(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<HistoryStats, ()> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    Ok(tokio::task::spawn_blocking(move || {
        // Week boundaries (Monday 00:00 UTC).
        // Jan 1, 1970 was a Thursday; with Mon=0 the offset is +3.
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let days_since_epoch = now_secs / 86400;
        let weekday           = (days_since_epoch + 3) % 7; // 0=Mon … 6=Sun
        let this_week_start   = (days_since_epoch - weekday) * 86400;
        let last_week_start   = this_week_start.saturating_sub(7 * 86400);

        let mut total_sessions = 0usize;
        let mut total_secs     = 0u64;
        let mut this_week_secs = 0u64;
        let mut last_week_secs = 0u64;

        let day_dirs = std::fs::read_dir(&skill_dir)
            .into_iter().flatten().flatten()
            .filter(|e| {
                let n = e.file_name(); let s = n.to_string_lossy();
                s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) && e.path().is_dir()
            });

        for day_entry in day_dirs {
            let json_files = std::fs::read_dir(day_entry.path())
                .into_iter().flatten().flatten()
                .filter(|e| {
                    let n = e.file_name(); let s = n.to_string_lossy();
                    s.starts_with("muse_") && s.ends_with(".json")
                });
            for jf in json_files {
                let Ok(text) = std::fs::read_to_string(jf.path()) else { continue };
                let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                let Some(start) = meta["session_start_utc"].as_u64() else { continue };
                let end = meta["session_end_utc"].as_u64().unwrap_or(start);
                let dur = end.saturating_sub(start);
                total_sessions += 1;
                total_secs     += dur;
                if start >= this_week_start       { this_week_secs += dur; }
                else if start >= last_week_start  { last_week_secs += dur; }
            }
        }
        HistoryStats { total_sessions, total_secs, this_week_secs, last_week_secs }
    })
    .await
    .unwrap_or(HistoryStats { total_sessions: 0, total_secs: 0,
                               this_week_secs: 0, last_week_secs: 0 }))
}

/// Delete a session's CSV + JSON sidecar files.
#[tauri::command]
fn delete_session(csv_path: String) -> Result<(), String> {
    let csv = std::path::PathBuf::from(&csv_path);
    let json = csv.with_extension("json");
    let ppg  = ppg_csv_path(&csv);
    let met  = metrics_csv_path(&csv);
    if csv.exists()  { std::fs::remove_file(&csv).map_err(|e| e.to_string())?; }
    if json.exists() { std::fs::remove_file(&json).map_err(|e| e.to_string())?; }
    if ppg.exists()  { std::fs::remove_file(&ppg).map_err(|e| e.to_string())?; }
    if met.exists()  { std::fs::remove_file(&met).map_err(|e| e.to_string())?; }
    Ok(())
}

// ── Embedding sessions (for compare picker) ──────────────────────────────────

/// One contiguous recording range discovered from embedding timestamps.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct EmbeddingSession {
    start_utc: u64,
    end_utc:   u64,
    n_epochs:  u64,
    /// YYYYMMDD directory the epochs live in (may span multiple days).
    day:       String,
}

/// Scan every `YYYYMMDD/eeg.sqlite` and return distinct recording sessions.
///
/// Two consecutive embeddings are considered part of the same session if they
/// are ≤ `GAP_SECS` apart (default 120 s — two minutes without data starts a
/// new session).  This makes the picker independent of CSV sidecar files.
#[tauri::command]
fn list_embedding_sessions(state: tauri::State<'_, Mutex<AppState>>) -> Vec<EmbeddingSession> {
    const GAP_SECS: u64 = 120;

    let skill_dir = state.lock_or_recover().skill_dir.clone();

    // Collect (utc_seconds, day_label) from every database.
    let mut all_ts: Vec<(u64, String)> = Vec::new();

    let entries = match std::fs::read_dir(&skill_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let day_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        // Only YYYYMMDD directories
        if day_name.len() != 8 || !day_name.bytes().all(|b| b.is_ascii_digit()) { continue; }

        let db_path = path.join(crate::constants::SQLITE_FILE);
        if !db_path.exists() { continue; }

        let conn = match rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(_) => continue,
        };
        // Allow up to 2 s for a write-locked DB (e.g. active recording) before giving up.
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");

        // Read all epoch timestamps as ISO-ish strings, convert to unix secs.
        let mut stmt = match conn.prepare("SELECT timestamp FROM embeddings ORDER BY timestamp") {
            Ok(s) => s,
            Err(_) => continue,
        };
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0));
        if let Ok(rows) = rows {
            for row in rows.filter_map(|r| r.ok()) {
                let utc = crate::commands::ts_to_unix(row);
                all_ts.push((utc, day_name.clone()));
            }
        }
    }

    if all_ts.is_empty() { return vec![]; }

    // Sort globally by timestamp.
    all_ts.sort_by_key(|(ts, _)| *ts);

    // Split into sessions using the gap threshold.
    let mut sessions: Vec<EmbeddingSession> = Vec::new();
    let mut start = all_ts[0].0;
    let mut end   = start;
    let mut count: u64 = 1;
    let mut day   = all_ts[0].1.clone();

    for &(ts, ref d) in &all_ts[1..] {
        if ts.saturating_sub(end) > GAP_SECS {
            // Flush current session
            sessions.push(EmbeddingSession { start_utc: start, end_utc: end, n_epochs: count, day: day.clone() });
            start = ts;
            end   = ts;
            count = 1;
            day   = d.clone();
        } else {
            end = ts;
            count += 1;
        }
    }
    // Flush last
    sessions.push(EmbeddingSession { start_utc: start, end_utc: end, n_epochs: count, day: day.clone() });

    // Return newest first
    sessions.reverse();
    sessions
}

// ── Session compare ───────────────────────────────────────────────────────────

/// Aggregated band-power metrics for a time range, returned by `get_session_metrics`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct SessionMetrics {
    /// Number of embedding epochs in the range.
    n_epochs:         usize,
    /// Mean relative band powers (0.0–1.0), averaged across all epochs.
    rel_delta:        f64,
    rel_theta:        f64,
    rel_alpha:        f64,
    rel_beta:         f64,
    rel_gamma:        f64,
    rel_high_gamma:   f64,
    /// Mean derived scores (0–100).
    relaxation:       f64,
    engagement:       f64,
    /// Mean Frontal Alpha Asymmetry.
    faa:              f64,
    /// Mean Theta / Alpha ratio.
    tar:              f64,
    /// Mean Beta / Alpha ratio.
    bar:              f64,
    /// Mean Delta / Theta ratio.
    dtr:              f64,
    /// Mean Power Spectral Entropy.
    pse:              f64,
    /// Mean Alpha Peak Frequency (Hz).
    apf:              f64,
    /// Mean Band-Power Slope (1/f).
    bps:              f64,
    /// Mean SNR (dB).
    snr:              f64,
    /// Mean inter-channel coherence.
    coherence:        f64,
    /// Mean Mu suppression index.
    mu_suppression:   f64,
    /// Mean Mood index (0–100).
    mood:             f64,
    tbr:              f64,
    sef95:            f64,
    spectral_centroid: f64,
    hjorth_activity:  f64,
    hjorth_mobility:  f64,
    hjorth_complexity: f64,
    permutation_entropy: f64,
    higuchi_fd:       f64,
    dfa_exponent:     f64,
    sample_entropy:   f64,
    pac_theta_gamma:  f64,
    laterality_index: f64,
    // PPG-derived
    hr:               f64,
    rmssd:            f64,
    sdnn:             f64,
    pnn50:            f64,
    lf_hf_ratio:      f64,
    respiratory_rate: f64,
    spo2_estimate:    f64,
    perfusion_index:  f64,
    stress_index:     f64,
    // Artifact events
    blink_count: f64,
    blink_rate:  f64,
    // Head pose
    head_pitch:       f64,
    head_roll:        f64,
    stillness:        f64,
    nod_count:        f64,
    shake_count:      f64,
    // Composite scores
    meditation:       f64,
    cognitive_load:   f64,
    drowsiness:       f64,
}

// ── Time-series epoch data ────────────────────────────────────────────────────

/// A single epoch's metrics, returned as part of a time-series query.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct EpochRow {
    /// Epoch timestamp (Unix seconds UTC).
    t: f64,
    /// Relative band powers.
    rd: f64, rt: f64, ra: f64, rb: f64, rg: f64,
    /// Core scores.
    relaxation: f64, engagement: f64,
    faa: f64,
    /// Band ratios.
    tar: f64, bar: f64, dtr: f64, tbr: f64,
    /// Spectral.
    pse: f64, apf: f64, sef95: f64, sc: f64, bps: f64, snr: f64,
    /// Cross-channel.
    coherence: f64, mu: f64,
    /// Hjorth.
    ha: f64, hm: f64, hc: f64,
    /// Nonlinear.
    pe: f64, hfd: f64, dfa: f64, se: f64, pac: f64, lat: f64,
    /// Mood.
    mood: f64,
    /// PPG vitals.
    hr: f64, rmssd: f64, sdnn: f64, pnn50: f64, lf_hf: f64,
    resp: f64, spo2: f64, perf: f64, stress: f64,
    /// Artifact events.
    blinks: f64, blink_r: f64,
    /// Head pose.
    pitch: f64, roll: f64, still: f64, nods: f64, shakes: f64,
    /// Composite.
    med: f64, cog: f64, drow: f64,
    /// GPU utilisation (0–1).
    gpu: f64, gpu_render: f64, gpu_tiler: f64,
}

// ── CSV-based metrics loading ──────────────────────────────────────────────────

/// Combined summary + time-series data loaded directly from `_metrics.csv`.
/// This is the primary data source for historical session display —
/// it works even when no SQLite epoch data exists.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct CsvMetricsResult {
    /// Number of rows in the CSV (≈ snapshots at ~4 Hz).
    n_rows: usize,
    /// Aggregated averages across all rows.
    summary: SessionMetrics,
    /// Per-row time-series for charts.
    timeseries: Vec<EpochRow>,
}

/// Read a `_metrics.csv` file and return aggregated summary + time-series.
/// Column indices follow `METRICS_CSV_HEADER` (94 columns).
fn load_metrics_csv(csv_path: &std::path::Path) -> Option<CsvMetricsResult> {
    let metrics_path = metrics_csv_path(csv_path);
    if !metrics_path.exists() {
        eprintln!("[csv-metrics] no metrics file: {}", metrics_path.display());
        return None;
    }

    let mut rdr = match csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(&metrics_path)
    {
        Ok(r)  => r,
        Err(e) => { eprintln!("[csv-metrics] open error: {e}"); return None; }
    };

    let mut rows: Vec<EpochRow> = Vec::new();
    let mut sum = SessionMetrics::default();
    let mut count = 0usize;

    for result in rdr.records() {
        let rec = match result {
            Ok(r)  => r,
            Err(_) => continue,
        };
        // Minimum: need at least 49 columns (timestamp + 48 band powers)
        if rec.len() < 49 { continue; }

        let f = |i: usize| -> f64 {
            rec.get(i).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)
        };

        let timestamp = f(0);
        if timestamp <= 0.0 { continue; }

        // Average the 4 channels' relative band powers for summary
        // Rel columns per channel: offset+6..offset+11 (rel_delta..rel_high_gamma)
        // Ch offsets: TP9=1, AF7=13, AF8=25, TP10=37
        let avg_rel = |band_offset: usize| -> f64 {
            let mut s = 0.0;
            for ch_base in &[1usize, 13, 25, 37] {
                s += f(ch_base + 6 + band_offset); // +6 = skip absolute powers
            }
            s / 4.0
        };

        let rd = avg_rel(0); // rel_delta
        let rt = avg_rel(1); // rel_theta
        let ra = avg_rel(2); // rel_alpha
        let rb = avg_rel(3); // rel_beta
        let rg = avg_rel(4); // rel_gamma

        // Cross-channel indices start at col 49
        let faa_v  = f(49);  let tar_v  = f(50);  let bar_v  = f(51);  let dtr_v  = f(52);
        let pse_v  = f(53);  let apf_v  = f(54);  let bps_v  = f(55);  let snr_v  = f(56);
        let coh_v  = f(57);  let mu_v   = f(58);  let mood_v = f(59);
        let tbr_v  = f(60);  let sef_v  = f(61);  let sc_v   = f(62);
        let ha_v   = f(63);  let hm_v   = f(64);  let hc_v   = f(65);
        let pe_v   = f(66);  let hfd_v  = f(67);  let dfa_v  = f(68);
        let se_v   = f(69);  let pac_v  = f(70);  let lat_v  = f(71);

        // PPG vitals (cols 72-80)
        let hr_v    = f(72); let rmssd_v = f(73); let sdnn_v  = f(74);
        let pnn_v   = f(75); let lfhf_v  = f(76); let resp_v  = f(77);
        let spo_v   = f(78); let perf_v  = f(79); let stress_v= f(80);

        // Artifact events (cols 81-82)
        let blinks_v  = f(81); let blink_r_v  = f(82);

        // Head pose (cols 83-87)
        let pitch_v = f(83); let roll_v = f(84); let still_v = f(85);
        let nods_v  = f(86); let shakes_v = f(87);

        // Composite (cols 88-90)
        let med_v = f(88); let cog_v = f(89); let drow_v = f(90);

        // GPU utilisation (cols 92-94, after temperature_raw at 91)
        let gpu_v = f(92); let gpu_r_v = f(93); let gpu_t_v = f(94);

        // Compute focus/relaxation/engagement per-channel, then average
        // (matches eeg_embeddings.rs logic exactly)
        let mut sr = 0.0f64; let mut se2 = 0.0f64;
        for ch_base in &[1usize, 13, 25, 37] {
            let a = f(ch_base + 6 + 2); // rel_alpha
            let b = f(ch_base + 6 + 3); // rel_beta
            let t = f(ch_base + 6 + 1); // rel_theta
            let d1 = a + t;
            let d2 = b + t;
            if d1 > 1e-6 { se2 += b / d1; }
            if d2 > 1e-6 { sr += a / d2; }
        }
        let relax_v   = sigmoid100((sr / 4.0) as f32, 2.5, 1.0) as f64;
        let engage_v  = sigmoid100((se2 / 4.0) as f32, 2.0, 0.8) as f64;

        let row = EpochRow {
            t: timestamp,
            rd, rt, ra, rb, rg,
            relaxation: relax_v, engagement: engage_v,
            faa: faa_v,
            tar: tar_v, bar: bar_v, dtr: dtr_v, tbr: tbr_v,
            pse: pse_v, apf: apf_v, sef95: sef_v, sc: sc_v, bps: bps_v, snr: snr_v,
            coherence: coh_v, mu: mu_v,
            ha: ha_v, hm: hm_v, hc: hc_v,
            pe: pe_v, hfd: hfd_v, dfa: dfa_v, se: se_v, pac: pac_v, lat: lat_v,
            mood: mood_v,
            hr: hr_v, rmssd: rmssd_v, sdnn: sdnn_v, pnn50: pnn_v, lf_hf: lfhf_v,
            resp: resp_v, spo2: spo_v, perf: perf_v, stress: stress_v,
            blinks: blinks_v, blink_r: blink_r_v,
            pitch: pitch_v, roll: roll_v, still: still_v, nods: nods_v, shakes: shakes_v,
            med: med_v, cog: cog_v, drow: drow_v,
            gpu: gpu_v, gpu_render: gpu_r_v, gpu_tiler: gpu_t_v,
        };

        // Accumulate for averages
        sum.rel_delta += rd;   sum.rel_theta += rt;   sum.rel_alpha += ra;
        sum.rel_beta  += rb;   sum.rel_gamma += rg;
        sum.relaxation += relax_v;  sum.engagement += engage_v;
        sum.faa += faa_v;      sum.tar += tar_v;      sum.bar += bar_v;
        sum.dtr += dtr_v;      sum.tbr += tbr_v;
        sum.pse += pse_v;      sum.apf += apf_v;      sum.bps += bps_v;
        sum.snr += snr_v;      sum.coherence += coh_v; sum.mu_suppression += mu_v;
        sum.mood += mood_v;    sum.sef95 += sef_v;     sum.spectral_centroid += sc_v;
        sum.hjorth_activity += ha_v; sum.hjorth_mobility += hm_v; sum.hjorth_complexity += hc_v;
        sum.permutation_entropy += pe_v; sum.higuchi_fd += hfd_v; sum.dfa_exponent += dfa_v;
        sum.sample_entropy += se_v; sum.pac_theta_gamma += pac_v; sum.laterality_index += lat_v;
        sum.hr += hr_v;        sum.rmssd += rmssd_v;   sum.sdnn += sdnn_v;
        sum.pnn50 += pnn_v;    sum.lf_hf_ratio += lfhf_v; sum.respiratory_rate += resp_v;
        sum.spo2_estimate += spo_v; sum.perfusion_index += perf_v; sum.stress_index += stress_v;
        sum.blink_count += blinks_v; sum.blink_rate += blink_r_v;
        sum.head_pitch += pitch_v; sum.head_roll += roll_v; sum.stillness += still_v;
        sum.nod_count += nods_v; sum.shake_count += shakes_v;
        sum.meditation += med_v; sum.cognitive_load += cog_v; sum.drowsiness += drow_v;

        rows.push(row);
        count += 1;
    }

    if count == 0 { return None; }

    let n = count as f64;
    sum.n_epochs = count;
    sum.rel_delta /= n;  sum.rel_theta /= n;  sum.rel_alpha /= n;
    sum.rel_beta  /= n;  sum.rel_gamma /= n;
    sum.relaxation /= n;  sum.engagement /= n;
    sum.faa /= n;        sum.tar /= n;         sum.bar /= n;
    sum.dtr /= n;        sum.tbr /= n;
    sum.pse /= n;        sum.apf /= n;         sum.bps /= n;
    sum.snr /= n;        sum.coherence /= n;   sum.mu_suppression /= n;
    sum.mood /= n;       sum.sef95 /= n;       sum.spectral_centroid /= n;
    sum.hjorth_activity /= n; sum.hjorth_mobility /= n; sum.hjorth_complexity /= n;
    sum.permutation_entropy /= n; sum.higuchi_fd /= n; sum.dfa_exponent /= n;
    sum.sample_entropy /= n; sum.pac_theta_gamma /= n; sum.laterality_index /= n;
    sum.hr /= n;         sum.rmssd /= n;        sum.sdnn /= n;
    sum.pnn50 /= n;      sum.lf_hf_ratio /= n;  sum.respiratory_rate /= n;
    sum.spo2_estimate /= n; sum.perfusion_index /= n; sum.stress_index /= n;
    sum.blink_rate /= n;
    sum.head_pitch /= n; sum.head_roll /= n;    sum.stillness /= n;
    sum.meditation /= n; sum.cognitive_load /= n; sum.drowsiness /= n;
    // blink_count, nod_count, shake_count: keep totals (don't average)

    eprintln!("[csv-metrics] loaded {} rows from {}", count, metrics_path.display());

    Some(CsvMetricsResult {
        n_rows: count,
        summary: sum,
        timeseries: rows,
    })
}

/// Sigmoid mapping (0, ∞) → (0, 100) with tuneable steepness and midpoint.
fn sigmoid100(x: f32, k: f32, mid: f32) -> f32 {
    100.0 / (1.0 + (-k * (x - mid)).exp())
}

/// Return per-epoch time-series data for a session range.
/// Used for historical charts in compare and history views.
/// Run schema migrations on a connection so new columns are available
/// even in databases that haven't been opened by the embedding worker yet.
fn migrate_embeddings_schema(conn: &rusqlite::Connection) {
    // Add the metrics_json column to databases created before the JSON schema.
    // All other column additions are no longer needed for new rows; old DBs
    // simply have NULL metrics_json and return 0.0 from json_extract().
    let _ = conn.execute("ALTER TABLE embeddings ADD COLUMN metrics_json TEXT", []);
}

fn get_session_timeseries_impl(
    skill_dir: &std::path::Path,
    start_utc: u64,
    end_utc:   u64,
) -> Vec<EpochRow> {
    use crate::commands::unix_to_ts;
    let ts_start = unix_to_ts(start_utc);
    let ts_end   = unix_to_ts(end_utc);
    let mut rows: Vec<EpochRow> = Vec::new();

    let entries = match std::fs::read_dir(skill_dir) {
        Ok(e)  => e,
        Err(_) => return rows,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let db_path = path.join(crate::constants::SQLITE_FILE);
        if !db_path.exists() { continue; }

        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c)  => c,
            Err(_) => continue,
        };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        migrate_embeddings_schema(&conn);

        let mut stmt = match conn.prepare(
            "SELECT timestamp,
                    json_extract(metrics_json, '$.rel_delta'),
                    json_extract(metrics_json, '$.rel_theta'),
                    json_extract(metrics_json, '$.rel_alpha'),
                    json_extract(metrics_json, '$.rel_beta'),
                    json_extract(metrics_json, '$.rel_gamma'),
                    json_extract(metrics_json, '$.relaxation_score'),
                    json_extract(metrics_json, '$.engagement_score'),
                    json_extract(metrics_json, '$.faa'),
                    json_extract(metrics_json, '$.tar'),
                    json_extract(metrics_json, '$.bar'),
                    json_extract(metrics_json, '$.dtr'),
                    json_extract(metrics_json, '$.pse'),
                    json_extract(metrics_json, '$.apf'),
                    json_extract(metrics_json, '$.bps'),
                    json_extract(metrics_json, '$.snr'),
                    json_extract(metrics_json, '$.coherence'),
                    json_extract(metrics_json, '$.mu_suppression'),
                    json_extract(metrics_json, '$.mood'),
                    json_extract(metrics_json, '$.tbr'),
                    json_extract(metrics_json, '$.sef95'),
                    json_extract(metrics_json, '$.spectral_centroid'),
                    json_extract(metrics_json, '$.hjorth_activity'),
                    json_extract(metrics_json, '$.hjorth_mobility'),
                    json_extract(metrics_json, '$.hjorth_complexity'),
                    json_extract(metrics_json, '$.permutation_entropy'),
                    json_extract(metrics_json, '$.higuchi_fd'),
                    json_extract(metrics_json, '$.dfa_exponent'),
                    json_extract(metrics_json, '$.sample_entropy'),
                    json_extract(metrics_json, '$.pac_theta_gamma'),
                    json_extract(metrics_json, '$.laterality_index'),
                    json_extract(metrics_json, '$.hr'),
                    json_extract(metrics_json, '$.rmssd'),
                    json_extract(metrics_json, '$.sdnn'),
                    json_extract(metrics_json, '$.pnn50'),
                    json_extract(metrics_json, '$.lf_hf_ratio'),
                    json_extract(metrics_json, '$.respiratory_rate'),
                    json_extract(metrics_json, '$.spo2_estimate'),
                    json_extract(metrics_json, '$.perfusion_idx'),
                    json_extract(metrics_json, '$.stress_index'),
                    json_extract(metrics_json, '$.blink_count'),
                    json_extract(metrics_json, '$.blink_rate'),
                    json_extract(metrics_json, '$.head_pitch'),
                    json_extract(metrics_json, '$.head_roll'),
                    json_extract(metrics_json, '$.stillness'),
                    json_extract(metrics_json, '$.nod_count'),
                    json_extract(metrics_json, '$.shake_count'),
                    json_extract(metrics_json, '$.meditation'),
                    json_extract(metrics_json, '$.cognitive_load'),
                    json_extract(metrics_json, '$.drowsiness')
             FROM embeddings
             WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp ASC"
        ) {
            Ok(s)  => s,
            Err(_) => continue,
        };

        let iter = stmt.query_map(rusqlite::params![ts_start, ts_end], |row| {
            let ts_val: i64 = row.get(0)?;
            let utc = crate::commands::ts_to_unix(ts_val);
            let g = |i: usize| -> f64 { row.get::<_, Option<f64>>(i).unwrap_or(None).unwrap_or(0.0) };
            Ok(EpochRow {
                t: utc as f64,
                rd: g(1), rt: g(2), ra: g(3), rb: g(4), rg: g(5),
                relaxation: g(6), engagement: g(7), faa: g(8),
                tar: g(9), bar: g(10), dtr: g(11), pse: g(12), apf: g(13),
                bps: g(14), snr: g(15), coherence: g(16), mu: g(17), mood: g(18),
                tbr: g(19), sef95: g(20), sc: g(21),
                ha: g(22), hm: g(23), hc: g(24),
                pe: g(25), hfd: g(26), dfa: g(27), se: g(28), pac: g(29), lat: g(30),
                hr: g(31), rmssd: g(32), sdnn: g(33), pnn50: g(34), lf_hf: g(35),
                resp: g(36), spo2: g(37), perf: g(38), stress: g(39),
                blinks: g(40), blink_r: g(41),
                pitch: g(42), roll: g(43), still: g(44), nods: g(45), shakes: g(46),
                med: g(47), cog: g(48), drow: g(49),
                gpu: 0.0, gpu_render: 0.0, gpu_tiler: 0.0, // not stored in SQLite
            })
        });

        if let Ok(iter) = iter {
            for row in iter.filter_map(|r| r.ok()) {
                rows.push(row);
            }
        }
    }

    rows.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    rows
}

/// Core implementation: query aggregated band-power metrics from all daily
/// `eeg.sqlite` databases that overlap `[start_utc, end_utc]`.
/// Used by both the Tauri IPC command and the WebSocket `compare` handler.
pub(crate) fn get_session_metrics_impl(
    skill_dir: &std::path::Path,
    start_utc: u64,
    end_utc:   u64,
) -> SessionMetrics {
    use crate::commands::unix_to_ts;

    let ts_start = unix_to_ts(start_utc);
    let ts_end   = unix_to_ts(end_utc);

    let mut total = SessionMetrics::default();
    let mut count = 0u64;

    let entries = match std::fs::read_dir(skill_dir) {
        Ok(e)  => e,
        Err(_) => return total,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let db_path = path.join(crate::constants::SQLITE_FILE);
        if !db_path.exists() { continue; }

        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c)  => c,
            Err(_) => continue,
        };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        migrate_embeddings_schema(&conn);

        let mut stmt = match conn.prepare(
            "SELECT json_extract(metrics_json, '$.rel_delta'),
                    json_extract(metrics_json, '$.rel_theta'),
                    json_extract(metrics_json, '$.rel_alpha'),
                    json_extract(metrics_json, '$.rel_beta'),
                    json_extract(metrics_json, '$.rel_gamma'),
                    json_extract(metrics_json, '$.rel_high_gamma'),
                    json_extract(metrics_json, '$.relaxation_score'),
                    json_extract(metrics_json, '$.engagement_score'),
                    json_extract(metrics_json, '$.faa'),
                    json_extract(metrics_json, '$.tar'),
                    json_extract(metrics_json, '$.bar'),
                    json_extract(metrics_json, '$.dtr'),
                    json_extract(metrics_json, '$.pse'),
                    json_extract(metrics_json, '$.apf'),
                    json_extract(metrics_json, '$.bps'),
                    json_extract(metrics_json, '$.snr'),
                    json_extract(metrics_json, '$.coherence'),
                    json_extract(metrics_json, '$.mu_suppression'),
                    json_extract(metrics_json, '$.mood'),
                    json_extract(metrics_json, '$.tbr'),
                    json_extract(metrics_json, '$.sef95'),
                    json_extract(metrics_json, '$.spectral_centroid'),
                    json_extract(metrics_json, '$.hjorth_activity'),
                    json_extract(metrics_json, '$.hjorth_mobility'),
                    json_extract(metrics_json, '$.hjorth_complexity'),
                    json_extract(metrics_json, '$.permutation_entropy'),
                    json_extract(metrics_json, '$.higuchi_fd'),
                    json_extract(metrics_json, '$.dfa_exponent'),
                    json_extract(metrics_json, '$.sample_entropy'),
                    json_extract(metrics_json, '$.pac_theta_gamma'),
                    json_extract(metrics_json, '$.laterality_index'),
                    json_extract(metrics_json, '$.hr'),
                    json_extract(metrics_json, '$.rmssd'),
                    json_extract(metrics_json, '$.sdnn'),
                    json_extract(metrics_json, '$.pnn50'),
                    json_extract(metrics_json, '$.lf_hf_ratio'),
                    json_extract(metrics_json, '$.respiratory_rate'),
                    json_extract(metrics_json, '$.spo2_estimate'),
                    json_extract(metrics_json, '$.perfusion_idx'),
                    json_extract(metrics_json, '$.stress_index'),
                    json_extract(metrics_json, '$.blink_count'),
                    json_extract(metrics_json, '$.blink_rate'),
                    json_extract(metrics_json, '$.head_pitch'),
                    json_extract(metrics_json, '$.head_roll'),
                    json_extract(metrics_json, '$.stillness'),
                    json_extract(metrics_json, '$.nod_count'),
                    json_extract(metrics_json, '$.shake_count'),
                    json_extract(metrics_json, '$.meditation'),
                    json_extract(metrics_json, '$.cognitive_load'),
                    json_extract(metrics_json, '$.drowsiness')
             FROM embeddings
             WHERE timestamp >= ?1 AND timestamp <= ?2"
        ) {
            Ok(s)  => s,
            Err(_) => continue,
        };

        // Use a Vec instead of fixed-size array for 50 columns
        let rows = stmt.query_map(rusqlite::params![ts_start, ts_end], |row| {
            let mut v = Vec::with_capacity(50);
            for i in 0..50 {
                v.push(row.get::<_, Option<f64>>(i)?);
            }
            Ok(v)
        });

        if let Ok(rows) = rows {
            for row in rows.filter_map(|r| r.ok()) {
                let v = row;
                if v[0].is_none() && v[1].is_none() { continue; }
                total.rel_delta      += v[0].unwrap_or(0.0);
                total.rel_theta      += v[1].unwrap_or(0.0);
                total.rel_alpha      += v[2].unwrap_or(0.0);
                total.rel_beta       += v[3].unwrap_or(0.0);
                total.rel_gamma      += v[4].unwrap_or(0.0);
                total.rel_high_gamma += v[5].unwrap_or(0.0);
                total.relaxation     += v[6].unwrap_or(0.0);
                total.engagement     += v[7].unwrap_or(0.0);
                total.faa            += v[8].unwrap_or(0.0);
                total.tar            += v[9].unwrap_or(0.0);
                total.bar            += v[10].unwrap_or(0.0);
                total.dtr            += v[11].unwrap_or(0.0);
                total.pse            += v[12].unwrap_or(0.0);
                total.apf            += v[13].unwrap_or(0.0);
                total.bps            += v[14].unwrap_or(0.0);
                total.snr            += v[15].unwrap_or(0.0);
                total.coherence      += v[16].unwrap_or(0.0);
                total.mu_suppression += v[17].unwrap_or(0.0);
                total.mood           += v[18].unwrap_or(0.0);
                total.tbr            += v[19].unwrap_or(0.0);
                total.sef95          += v[20].unwrap_or(0.0);
                total.spectral_centroid += v[21].unwrap_or(0.0);
                total.hjorth_activity   += v[22].unwrap_or(0.0);
                total.hjorth_mobility   += v[23].unwrap_or(0.0);
                total.hjorth_complexity  += v[24].unwrap_or(0.0);
                total.permutation_entropy += v[25].unwrap_or(0.0);
                total.higuchi_fd     += v[26].unwrap_or(0.0);
                total.dfa_exponent   += v[27].unwrap_or(0.0);
                total.sample_entropy += v[28].unwrap_or(0.0);
                total.pac_theta_gamma += v[29].unwrap_or(0.0);
                total.laterality_index += v[30].unwrap_or(0.0);
                total.hr               += v[31].unwrap_or(0.0);
                total.rmssd            += v[32].unwrap_or(0.0);
                total.sdnn             += v[33].unwrap_or(0.0);
                total.pnn50            += v[34].unwrap_or(0.0);
                total.lf_hf_ratio      += v[35].unwrap_or(0.0);
                total.respiratory_rate += v[36].unwrap_or(0.0);
                total.spo2_estimate    += v[37].unwrap_or(0.0);
                total.perfusion_index  += v[38].unwrap_or(0.0);
                total.stress_index     += v[39].unwrap_or(0.0);
                total.blink_count      += v[40].unwrap_or(0.0);
                total.blink_rate       += v[41].unwrap_or(0.0);
                total.head_pitch       += v[42].unwrap_or(0.0);
                total.head_roll        += v[43].unwrap_or(0.0);
                total.stillness        += v[44].unwrap_or(0.0);
                total.nod_count        += v[45].unwrap_or(0.0);
                total.shake_count      += v[46].unwrap_or(0.0);
                total.meditation       += v[47].unwrap_or(0.0);
                total.cognitive_load   += v[48].unwrap_or(0.0);
                total.drowsiness       += v[49].unwrap_or(0.0);
                count += 1;
            }
        }
    }

    if count > 0 {
        let n = count as f64;
        total.rel_delta      /= n;
        total.rel_theta      /= n;
        total.rel_alpha      /= n;
        total.rel_beta       /= n;
        total.rel_gamma      /= n;
        total.rel_high_gamma /= n;
        total.relaxation     /= n;
        total.engagement     /= n;
        total.faa            /= n;
        total.tar            /= n;
        total.bar            /= n;
        total.dtr            /= n;
        total.pse            /= n;
        total.apf            /= n;
        total.bps            /= n;
        total.snr            /= n;
        total.coherence      /= n;
        total.mu_suppression /= n;
        total.mood           /= n;
        total.tbr            /= n;
        total.sef95          /= n;
        total.spectral_centroid /= n;
        total.hjorth_activity   /= n;
        total.hjorth_mobility   /= n;
        total.hjorth_complexity /= n;
        total.permutation_entropy /= n;
        total.higuchi_fd     /= n;
        total.dfa_exponent   /= n;
        total.sample_entropy /= n;
        total.pac_theta_gamma /= n;
        total.laterality_index /= n;
        total.hr               /= n;
        total.rmssd            /= n;
        total.sdnn             /= n;
        total.pnn50            /= n;
        total.lf_hf_ratio      /= n;
        total.respiratory_rate /= n;
        total.spo2_estimate    /= n;
        total.perfusion_index  /= n;
        total.stress_index     /= n;
        total.blink_count /= n;
        total.blink_rate  /= n;
        total.head_pitch  /= n;
        total.head_roll        /= n;
        total.stillness        /= n;
        total.nod_count        /= n;
        total.shake_count      /= n;
        total.meditation       /= n;
        total.cognitive_load   /= n;
        total.drowsiness       /= n;
        total.n_epochs        = count as usize;
    }

    total
}

// ── Sleep staging ─────────────────────────────────────────────────────────────

/// A single epoch classified into a sleep stage.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct SleepEpoch {
    /// Unix seconds (UTC) of this epoch.
    utc: u64,
    /// Sleep stage: 0 = Wake, 1 = N1, 2 = N2, 3 = N3, 5 = REM.
    /// (Stage numbering follows AASM convention; 4 is unused.)
    stage: u8,
    /// Relative band powers for this epoch.
    rel_delta: f64,
    rel_theta: f64,
    rel_alpha: f64,
    rel_beta:  f64,
}

/// Summary statistics for a sleep session.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct SleepSummary {
    total_epochs:  usize,
    wake_epochs:   usize,
    n1_epochs:     usize,
    n2_epochs:     usize,
    n3_epochs:     usize,
    rem_epochs:    usize,
    /// Epoch duration in seconds (from embedding interval).
    epoch_secs:    f64,
}

/// Result returned by [`get_sleep_stages`].
#[derive(Serialize, Deserialize, Clone, Debug)]
struct SleepStages {
    epochs:  Vec<SleepEpoch>,
    summary: SleepSummary,
}

/// Classify each embedding epoch in `[start_utc, end_utc]` into a sleep stage.
///
/// The classifier uses relative band-power ratios following simplified AASM
/// heuristics (Muse has only 4 dry electrodes — frontal + temporal — so this
/// is an approximation, not a clinical polysomnograph):
///
/// | Stage | Dominant activity | Rule (applied in order) |
/// |-------|-------------------|-------------------------|
/// | Wake  | α / low β, eye blinks | `rel_alpha > 0.30` **or** `rel_beta > 0.30` |
/// | REM   | mixed low-voltage, θ | `rel_theta > 0.30` **and** `rel_alpha < 0.15` **and** `rel_delta < 0.45` |
/// | N1    | θ replaces α | `rel_theta > 0.25` **and** `rel_delta < 0.50` |
/// | N3    | slow-wave, δ dominant | `rel_delta > 0.50` |
/// | N2    | everything else (spindles/K-complexes not resolvable on Muse) |
///
/// These thresholds are tuned for the Muse 2 / Muse S headband.
#[tauri::command]
fn get_sleep_stages(
    start_utc: u64,
    end_utc:   u64,
    state:     tauri::State<'_, Mutex<AppState>>,
) -> SleepStages {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    get_sleep_stages_impl(&skill_dir, start_utc, end_utc)
}

pub(crate) fn get_sleep_stages_impl(
    skill_dir: &std::path::Path,
    start_utc: u64,
    end_utc:   u64,
) -> SleepStages {
    use crate::commands::{unix_to_ts, ts_to_unix};

    let ts_start = unix_to_ts(start_utc);
    let ts_end   = unix_to_ts(end_utc);

    // Collect raw epochs from all day-directories.
    struct RawEpoch { utc: u64, rd: f64, rt: f64, ra: f64, rb: f64 }
    let mut raw: Vec<RawEpoch> = Vec::new();

    let entries = match std::fs::read_dir(skill_dir) {
        Ok(e) => e,
        Err(_) => return SleepStages { epochs: vec![], summary: SleepSummary::default() },
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let db_path = path.join(crate::constants::SQLITE_FILE);
        if !db_path.exists() { continue; }

        let conn = match rusqlite::Connection::open_with_flags(
            &db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) { Ok(c) => c, Err(_) => continue };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");

        let mut stmt = match conn.prepare(
            "SELECT timestamp,
                    json_extract(metrics_json, '$.rel_delta'),
                    json_extract(metrics_json, '$.rel_theta'),
                    json_extract(metrics_json, '$.rel_alpha'),
                    json_extract(metrics_json, '$.rel_beta')
             FROM embeddings
             WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp"
        ) { Ok(s) => s, Err(_) => continue };

        let rows = stmt.query_map(rusqlite::params![ts_start, ts_end], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, Option<f64>>(3)?,
                row.get::<_, Option<f64>>(4)?,
            ))
        });
        if let Ok(rows) = rows {
            for row in rows.filter_map(|r| r.ok()) {
                let (ts, rd, rt, ra, rb) = row;
                if rd.is_none() && rt.is_none() { continue; }
                raw.push(RawEpoch {
                    utc: ts_to_unix(ts),
                    rd: rd.unwrap_or(0.0),
                    rt: rt.unwrap_or(0.0),
                    ra: ra.unwrap_or(0.0),
                    rb: rb.unwrap_or(0.0),
                });
            }
        }
    }

    raw.sort_by_key(|e| e.utc);

    // Classify each epoch.
    let mut summary = SleepSummary::default();
    let epochs: Vec<SleepEpoch> = raw.iter().map(|e| {
        let stage = classify_sleep(e.rd, e.rt, e.ra, e.rb);
        match stage {
            0 => summary.wake_epochs += 1,
            1 => summary.n1_epochs   += 1,
            2 => summary.n2_epochs   += 1,
            3 => summary.n3_epochs   += 1,
            5 => summary.rem_epochs  += 1,
            _ => {}
        }
        SleepEpoch {
            utc: e.utc, stage,
            rel_delta: e.rd, rel_theta: e.rt,
            rel_alpha: e.ra, rel_beta:  e.rb,
        }
    }).collect();

    summary.total_epochs = epochs.len();
    // Estimate epoch duration from median inter-epoch gap.
    if epochs.len() >= 2 {
        let mut gaps: Vec<f64> = epochs.windows(2)
            .map(|w| (w[1].utc as f64) - (w[0].utc as f64))
            .filter(|g| *g > 0.0 && *g < 30.0)
            .collect();
        if !gaps.is_empty() {
            gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
            summary.epoch_secs = gaps[gaps.len() / 2];
        } else {
            summary.epoch_secs = 2.5; // default
        }
    } else {
        summary.epoch_secs = 2.5;
    }

    SleepStages { epochs, summary }
}

/// Classify a single epoch into a sleep stage from relative band powers.
fn classify_sleep(rd: f64, rt: f64, ra: f64, rb: f64) -> u8 {
    // Wake: strong alpha or beta
    if ra > 0.30 || rb > 0.30 { return 0; }
    // REM: theta-dominant, low alpha, moderate-or-low delta
    if rt > 0.30 && ra < 0.15 && rd < 0.45 { return 5; }
    // N3 (slow-wave / deep): delta-dominant
    if rd > 0.50 { return 3; }
    // N1 (light drowsiness): theta rising, delta not yet dominant
    if rt > 0.25 && rd < 0.50 { return 1; }
    // N2 (default light sleep): everything else
    2
}

// ── Analysis helpers ──────────────────────────────────────────────────────────
//
// These functions compute derived insights from existing data (timeseries,
// sleep stages, search results, UMAP coordinates, session history).
// They return `serde_json::Value` for easy inclusion in WS responses.
// Placed in lib.rs so they have access to private struct fields.

/// Round to 2 decimal places.
fn r2f(v: f64) -> f64 { (v * 100.0).round() / 100.0 }

/// Linear regression slope over a sequence of values.
/// Positive slope = increasing trend over time.
fn linear_slope(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 2 { return 0.0; }
    let x_mean = (n - 1) as f64 / 2.0;
    let y_mean = values.iter().sum::<f64>() / n as f64;
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for (i, &y) in values.iter().enumerate() {
        let dx = i as f64 - x_mean;
        num += dx * (y - y_mean);
        den += dx * dx;
    }
    if den.abs() < 1e-15 { 0.0 } else { num / den }
}

/// Descriptive statistics for a slice of f64 values.
fn metric_stats_vec(values: &[f64]) -> serde_json::Value {
    if values.is_empty() { return serde_json::json!(null); }
    let n = values.len();
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean = values.iter().sum::<f64>() / n as f64;
    let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
    let stddev = variance.sqrt();
    let median = if n.is_multiple_of(2) { (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0 } else { sorted[n / 2] };
    let p25 = sorted[n / 4];
    let p75 = sorted[3 * n / 4];
    let slope = linear_slope(values);
    serde_json::json!({
        "min": r2f(sorted[0]), "max": r2f(sorted[n - 1]),
        "mean": r2f(mean), "median": r2f(median),
        "stddev": r2f(stddev), "p25": r2f(p25), "p75": r2f(p75),
        "trend": r2f(slope),
    })
}

/// Extract a named metric from an EpochRow.
fn epoch_field(row: &EpochRow, name: &str) -> f64 {
    match name {
        "relaxation" => row.relaxation, "engagement" => row.engagement,
        "faa" => row.faa, "tar" => row.tar, "bar" => row.bar, "dtr" => row.dtr, "tbr" => row.tbr,
        "mood" => row.mood, "hr" => row.hr, "rmssd" => row.rmssd, "sdnn" => row.sdnn,
        "stress" => row.stress, "snr" => row.snr, "coherence" => row.coherence,
        "stillness" => row.still, "blink_rate" => row.blink_r,
        "meditation" => row.med, "cognitive_load" => row.cog, "drowsiness" => row.drow,
        "rel_delta" => row.rd, "rel_theta" => row.rt, "rel_alpha" => row.ra, "rel_beta" => row.rb,
        "pse" => row.pse, "apf" => row.apf, "sef95" => row.sef95,
        _ => 0.0,
    }
}

/// Extract a named metric from SessionMetrics.
fn session_field(m: &SessionMetrics, name: &str) -> f64 {
    match name {
        "relaxation" => m.relaxation, "engagement" => m.engagement,
        "faa" => m.faa, "tar" => m.tar, "bar" => m.bar, "dtr" => m.dtr, "tbr" => m.tbr,
        "mood" => m.mood, "hr" => m.hr, "rmssd" => m.rmssd, "sdnn" => m.sdnn,
        "stress" => m.stress_index, "snr" => m.snr, "coherence" => m.coherence,
        "stillness" => m.stillness, "blink_rate" => m.blink_rate,
        "meditation" => m.meditation, "cognitive_load" => m.cognitive_load, "drowsiness" => m.drowsiness,
        "rel_delta" => m.rel_delta, "rel_theta" => m.rel_theta, "rel_alpha" => m.rel_alpha, "rel_beta" => m.rel_beta,
        "pse" => m.pse, "apf" => m.apf, "sef95" => m.sef95,
        _ => 0.0,
    }
}

/// Metric names used for compare insights and status comparisons.
const INSIGHT_METRICS: &[&str] = &[
    "relaxation", "engagement", "meditation", "cognitive_load", "drowsiness",
    "mood", "faa", "tar", "bar", "dtr", "tbr",
    "hr", "rmssd", "stress", "snr", "coherence", "stillness",
    "blink_rate", "rel_alpha", "rel_beta", "rel_theta", "rel_delta",
    "pse", "apf", "sef95",
];

/// Key composite metrics for status today-vs-average comparison.
const STATUS_METRICS: &[&str] = &[
    "relaxation", "engagement", "meditation", "cognitive_load",
    "drowsiness", "mood", "hr", "snr", "stillness",
];

// ── Compare insights ─────────────────────────────────────────────────────────

/// Compute per-metric stats, deltas, and trends for an A/B session comparison.
///
/// Fetches timeseries internally; takes already-computed aggregate metrics by
/// reference to avoid duplicate work.
pub(crate) fn compute_compare_insights(
    skill_dir: &std::path::Path,
    a_start: u64, a_end: u64,
    b_start: u64, b_end: u64,
    avg_a: &SessionMetrics,
    avg_b: &SessionMetrics,
) -> serde_json::Value {
    let ts_a = get_session_timeseries_impl(skill_dir, a_start, a_end);
    let ts_b = get_session_timeseries_impl(skill_dir, b_start, b_end);

    let mut stats_a = serde_json::Map::new();
    let mut stats_b = serde_json::Map::new();
    let mut deltas  = serde_json::Map::new();
    let mut improved: Vec<String> = Vec::new();
    let mut declined: Vec<String> = Vec::new();
    let mut stable:   Vec<String> = Vec::new();

    for &metric in INSIGHT_METRICS {
        let vals_a: Vec<f64> = ts_a.iter().map(|r| epoch_field(r, metric)).collect();
        let vals_b: Vec<f64> = ts_b.iter().map(|r| epoch_field(r, metric)).collect();
        stats_a.insert(metric.into(), metric_stats_vec(&vals_a));
        stats_b.insert(metric.into(), metric_stats_vec(&vals_b));

        let ma = session_field(avg_a, metric);
        let mb = session_field(avg_b, metric);
        let abs_delta = mb - ma;
        let pct = if ma.abs() > 1e-6 { abs_delta / ma.abs() * 100.0 } else { 0.0 };
        let direction = if pct > 5.0 { "up" } else if pct < -5.0 { "down" } else { "stable" };

        deltas.insert(metric.into(), serde_json::json!({
            "a": r2f(ma), "b": r2f(mb),
            "abs": r2f(abs_delta), "pct": r2f(pct),
            "direction": direction,
        }));
        match direction {
            "up"   => improved.push(metric.into()),
            "down" => declined.push(metric.into()),
            _      => stable.push(metric.into()),
        }
    }

    serde_json::json!({
        "stats_a": stats_a,
        "stats_b": stats_b,
        "deltas": deltas,
        "improved": improved,
        "declined": declined,
        "stable": stable,
        "n_epochs_a": ts_a.len(),
        "n_epochs_b": ts_b.len(),
    })
}

// ── Sleep analysis ───────────────────────────────────────────────────────────

/// Compute derived sleep-quality metrics from classified sleep stages.
pub(crate) fn analyze_sleep_stages(stages: &SleepStages) -> serde_json::Value {
    let epochs = &stages.epochs;
    let summary = &stages.summary;
    if epochs.is_empty() { return serde_json::json!(null); }

    let epoch_secs = if summary.epoch_secs > 0.0 { summary.epoch_secs } else { 5.0 };
    let total = summary.total_epochs as f64;
    let wake  = summary.wake_epochs as f64;

    // Sleep efficiency: (total − wake) / total × 100
    let efficiency = if total > 0.0 { (total - wake) / total * 100.0 } else { 0.0 };

    // Stage durations in minutes
    let stage_minutes = serde_json::json!({
        "wake": r2f(wake * epoch_secs / 60.0),
        "n1":   r2f(summary.n1_epochs as f64 * epoch_secs / 60.0),
        "n2":   r2f(summary.n2_epochs as f64 * epoch_secs / 60.0),
        "n3":   r2f(summary.n3_epochs as f64 * epoch_secs / 60.0),
        "rem":  r2f(summary.rem_epochs as f64 * epoch_secs / 60.0),
        "total":r2f(total * epoch_secs / 60.0),
    });

    // Sleep onset latency: time from first epoch to first non-wake epoch
    let first_sleep_idx = epochs.iter().position(|e| e.stage != 0);
    let onset_latency_min = match first_sleep_idx {
        Some(idx) if idx > 0 => r2f(epochs[idx].utc.saturating_sub(epochs[0].utc) as f64 / 60.0),
        _ => 0.0,
    };

    // REM latency: time from sleep onset to first REM epoch
    let rem_latency_min = first_sleep_idx.and_then(|si| {
        let start = epochs[si].utc;
        epochs[si..].iter()
            .find(|e| e.stage == 5)
            .map(|e| r2f(e.utc.saturating_sub(start) as f64 / 60.0))
    });

    // Transitions and awakenings
    let mut transitions = 0u32;
    let mut awakenings  = 0u32;
    for w in epochs.windows(2) {
        if w[0].stage != w[1].stage {
            transitions += 1;
            if w[1].stage == 0 && w[0].stage != 0 { awakenings += 1; }
        }
    }

    // Bout analysis per stage
    let stage_ids: &[(u8, &str)] = &[(0,"wake"),(1,"n1"),(2,"n2"),(3,"n3"),(5,"rem")];
    let mut bouts = serde_json::Map::new();
    for &(sid, name) in stage_ids {
        let mut lengths: Vec<f64> = Vec::new();
        let mut cur = 0u32;
        for e in epochs {
            if e.stage == sid { cur += 1; }
            else {
                if cur > 0 { lengths.push(cur as f64 * epoch_secs / 60.0); }
                cur = 0;
            }
        }
        if cur > 0 { lengths.push(cur as f64 * epoch_secs / 60.0); }
        if !lengths.is_empty() {
            let count = lengths.len();
            let mean = lengths.iter().sum::<f64>() / count as f64;
            let max  = lengths.iter().cloned().fold(0.0f64, f64::max);
            bouts.insert(name.into(), serde_json::json!({
                "count": count, "mean_min": r2f(mean), "max_min": r2f(max),
            }));
        }
    }

    serde_json::json!({
        "efficiency_pct":     r2f(efficiency),
        "onset_latency_min":  onset_latency_min,
        "rem_latency_min":    rem_latency_min,
        "stage_minutes":      stage_minutes,
        "transitions":        transitions,
        "awakenings":         awakenings,
        "bouts":              bouts,
    })
}

// ── Search analysis ──────────────────────────────────────────────────────────

/// Compute search-result insights: distance stats, temporal distribution, top days.
pub(crate) fn analyze_search_results(result: &commands::SearchResult) -> serde_json::Value {
    use std::collections::HashMap;

    // Distance statistics across all neighbors
    let all_distances: Vec<f64> = result.results.iter()
        .flat_map(|q| q.neighbors.iter().map(|n| n.distance as f64))
        .collect();
    let distance_stats = metric_stats_vec(&all_distances);

    // Temporal distribution (hour-of-day histogram)
    let mut hour_dist: HashMap<u8, u32> = HashMap::new();
    let mut day_dist:  HashMap<String, u32> = HashMap::new();
    let mut all_utcs: Vec<u64> = Vec::new();

    for q in &result.results {
        for n in &q.neighbors {
            all_utcs.push(n.timestamp_unix);
            let hour = ((n.timestamp_unix % 86400) / 3600) as u8;
            *hour_dist.entry(hour).or_insert(0) += 1;
            *day_dist.entry(n.date.clone()).or_insert(0) += 1;
        }
    }

    let mut hourly = serde_json::Map::new();
    for h in 0..24u8 {
        if let Some(&c) = hour_dist.get(&h) {
            hourly.insert(format!("{h:02}"), c.into());
        }
    }

    let mut top_days: Vec<(String, u32)> = day_dist.into_iter().collect();
    top_days.sort_by(|a, b| b.1.cmp(&a.1));
    top_days.truncate(10);

    let time_span_hours = if all_utcs.len() >= 2 {
        let mn = *all_utcs.iter().min().unwrap();
        let mx = *all_utcs.iter().max().unwrap();
        mx.saturating_sub(mn) as f64 / 3600.0
    } else { 0.0 };

    // Neighbor metrics averages (from the subset that have metrics)
    let metric_names = ["relaxation","engagement","meditation","cognitive_load",
                        "drowsiness","hr","snr","mood"];
    let mut neighbor_metrics = serde_json::Map::new();
    for &name in &metric_names {
        let vals: Vec<f64> = result.results.iter()
            .flat_map(|q| q.neighbors.iter())
            .filter_map(|n| n.metrics.as_ref())
            .filter_map(|m| match name {
                "relaxation"     => m.relaxation,
                "engagement"     => m.engagement,
                "meditation"     => m.meditation,
                "cognitive_load" => m.cognitive_load,
                "drowsiness"     => m.drowsiness,
                "hr"             => m.hr,
                "snr"            => m.snr,
                "mood"           => m.mood,
                _ => None,
            })
            .collect();
        if !vals.is_empty() {
            neighbor_metrics.insert(name.into(), serde_json::json!(r2f(
                vals.iter().sum::<f64>() / vals.len() as f64
            )));
        }
    }

    serde_json::json!({
        "distance_stats":         distance_stats,
        "temporal_distribution":  hourly,
        "top_days":               top_days.iter().map(|(d,c)| serde_json::json!([d, c])).collect::<Vec<_>>(),
        "time_span_hours":        r2f(time_span_hours),
        "total_neighbors":        all_distances.len(),
        "neighbor_metrics":       neighbor_metrics,
    })
}

// ── UMAP analysis ────────────────────────────────────────────────────────────

/// Compute cluster separation metrics and outliers from UMAP 3D coordinates.
pub(crate) fn analyze_umap_points(
    embedding: &[Vec<f64>],
    session_ids: &[u8],    // 0 = A, 1 = B
    timestamps: &[u64],
    _n_a: usize,
) -> serde_json::Value {
    let n = embedding.len().min(session_ids.len());
    if n == 0 { return serde_json::json!(null); }

    // Centroids
    let (mut ca, mut cb) = ([0.0f64; 3], [0.0f64; 3]);
    let (mut na, mut nb) = (0usize, 0usize);
    for i in 0..n {
        let c = if session_ids[i] == 0 { &mut ca } else { &mut cb };
        let cnt = if session_ids[i] == 0 { &mut na } else { &mut nb };
        for d in 0..3 { c[d] += embedding[i][d]; }
        *cnt += 1;
    }
    if na > 0 { for c in ca.iter_mut() { *c /= na as f64; } }
    if nb > 0 { for c in cb.iter_mut() { *c /= nb as f64; } }

    // Inter-cluster distance
    let inter_dist = ((ca[0]-cb[0]).powi(2) + (ca[1]-cb[1]).powi(2) + (ca[2]-cb[2]).powi(2)).sqrt();

    // Intra-cluster spread (mean distance to own centroid)
    let dist_to = |pt: &[f64], c: &[f64; 3]| -> f64 {
        ((pt[0]-c[0]).powi(2) + (pt[1]-c[1]).powi(2) + (pt[2]-c[2]).powi(2)).sqrt()
    };
    let (mut spread_a, mut spread_b) = (0.0f64, 0.0f64);
    for i in 0..n {
        if session_ids[i] == 0 { spread_a += dist_to(&embedding[i], &ca); }
        else                   { spread_b += dist_to(&embedding[i], &cb); }
    }
    if na > 0 { spread_a /= na as f64; }
    if nb > 0 { spread_b /= nb as f64; }

    // Separation score: inter / (0.5*(intra_a + intra_b))  — higher is better
    let avg_intra = (spread_a + spread_b) / 2.0;
    let separation = if avg_intra > 1e-9 { inter_dist / avg_intra } else { 0.0 };

    // Outliers: points > 2 std-devs from their own centroid
    let mut all_dists_a: Vec<f64> = Vec::new();
    let mut all_dists_b: Vec<f64> = Vec::new();
    for i in 0..n {
        let d = dist_to(&embedding[i], if session_ids[i] == 0 { &ca } else { &cb });
        if session_ids[i] == 0 { all_dists_a.push(d); } else { all_dists_b.push(d); }
    }
    let std_a = if all_dists_a.len() > 1 {
        let m = all_dists_a.iter().sum::<f64>() / all_dists_a.len() as f64;
        (all_dists_a.iter().map(|x| (x - m).powi(2)).sum::<f64>() / all_dists_a.len() as f64).sqrt()
    } else { 1.0 };
    let std_b = if all_dists_b.len() > 1 {
        let m = all_dists_b.iter().sum::<f64>() / all_dists_b.len() as f64;
        (all_dists_b.iter().map(|x| (x - m).powi(2)).sum::<f64>() / all_dists_b.len() as f64).sqrt()
    } else { 1.0 };

    let mut outliers: Vec<serde_json::Value> = Vec::new();
    let mut oi_a = 0usize;
    let mut oi_b = 0usize;
    for i in 0..n {
        let c = if session_ids[i] == 0 { &ca } else { &cb };
        let d = dist_to(&embedding[i], c);
        let threshold = if session_ids[i] == 0 { spread_a + 2.0 * std_a } else { spread_b + 2.0 * std_b };
        if d > threshold {
            if outliers.len() < 20 { // cap to 20
                outliers.push(serde_json::json!({
                    "x": r2f(embedding[i][0]), "y": r2f(embedding[i][1]), "z": r2f(embedding[i][2]),
                    "session": if session_ids[i] == 0 { "A" } else { "B" },
                    "utc": timestamps.get(i).copied().unwrap_or(0),
                    "distance_to_centroid": r2f(d),
                }));
            }
            if session_ids[i] == 0 { oi_a += 1; } else { oi_b += 1; }
        }
    }

    serde_json::json!({
        "centroid_a": [r2f(ca[0]), r2f(ca[1]), r2f(ca[2])],
        "centroid_b": [r2f(cb[0]), r2f(cb[1]), r2f(cb[2])],
        "inter_cluster_distance": r2f(inter_dist),
        "intra_spread_a": r2f(spread_a),
        "intra_spread_b": r2f(spread_b),
        "separation_score": r2f(separation),
        "n_outliers_a": oi_a,
        "n_outliers_b": oi_b,
        "outliers": outliers,
    })
}

// ── Status history ───────────────────────────────────────────────────────────

/// Compute recording history stats: totals, streak, today vs 7-day average.
pub(crate) fn compute_status_history(
    skill_dir: &std::path::Path,
    sessions_json: &[serde_json::Value],
) -> serde_json::Value {
    if sessions_json.is_empty() { return serde_json::json!(null); }

    let now = unix_secs();
    let today_day = now / 86400;

    let mut total_secs   = 0u64;
    let mut longest_secs = 0u64;
    let mut day_set      = std::collections::BTreeSet::<u64>::new();
    let total_sessions   = sessions_json.len();
    let mut total_epochs = 0u64;

    for s in sessions_json {
        let start = s["start_utc"].as_u64().unwrap_or(0);
        let end   = s["end_utc"].as_u64().unwrap_or(0);
        let n_ep  = s["n_epochs"].as_u64().unwrap_or(0);
        let dur   = end.saturating_sub(start);
        total_secs += dur;
        longest_secs = longest_secs.max(dur);
        total_epochs += n_ep;
        day_set.insert(start / 86400);
    }

    let recording_days = day_set.len();
    let total_hours    = total_secs as f64 / 3600.0;
    let avg_session_min = if total_sessions > 0 { total_hours * 60.0 / total_sessions as f64 } else { 0.0 };

    // Streak: consecutive recording days ending today (or yesterday)
    let mut streak = 0u32;
    let mut check = today_day;
    loop {
        if day_set.contains(&check) {
            streak += 1;
            if check == 0 { break; }
            check -= 1;
        } else if check == today_day {
            // Today might not have data yet — check yesterday
            if check == 0 { break; }
            check -= 1;
        } else {
            break;
        }
    }

    // Today vs 7-day average
    let today_start = today_day * 86400;
    let week_start  = today_day.saturating_sub(7) * 86400;
    let today_metrics = get_session_metrics_impl(skill_dir, today_start, now);
    let week_metrics  = get_session_metrics_impl(skill_dir, week_start, now);

    let mut today_vs_avg = serde_json::Map::new();
    if today_metrics.n_epochs > 0 && week_metrics.n_epochs > 0 {
        for &metric in STATUS_METRICS {
            let tv = session_field(&today_metrics, metric);
            let wv = session_field(&week_metrics, metric);
            let delta_pct = if wv.abs() > 1e-6 { (tv - wv) / wv.abs() * 100.0 } else { 0.0 };
            let direction = if delta_pct > 5.0 { "up" } else if delta_pct < -5.0 { "down" } else { "stable" };
            today_vs_avg.insert(metric.into(), serde_json::json!({
                "today": r2f(tv), "avg_7d": r2f(wv),
                "delta_pct": r2f(delta_pct), "direction": direction,
            }));
        }
    }

    serde_json::json!({
        "total_sessions":        total_sessions,
        "total_recording_hours": r2f(total_hours),
        "total_epochs":          total_epochs,
        "recording_days":        recording_days,
        "current_streak_days":   streak,
        "longest_session_min":   r2f(longest_secs as f64 / 60.0),
        "avg_session_min":       r2f(avg_session_min),
        "today_vs_avg":          today_vs_avg,
    })
}

// ── UMAP embedding comparison ─────────────────────────────────────────────────

/// A single 2D point in UMAP space, tagged with its session (0 = A, 1 = B).
#[derive(Serialize, Deserialize, Clone, Debug)]
struct UmapPoint {
    x: f32,
    y: f32,
    /// Third UMAP dimension (3D projection).
    z: f32,
    /// 0 = session A, 1 = session B.
    session: u8,
    /// Unix seconds UTC of the source epoch.
    utc: u64,
    /// User-defined label text, if any label's EEG window overlaps this epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

/// Load all labels from `labels.sqlite` whose EEG window overlaps [start, end].
/// Returns Vec<(eeg_start_unix, eeg_end_unix, text)>.
pub(crate) fn load_labels_range(
    skill_dir: &std::path::Path,
    start_utc: u64,
    end_utc:   u64,
) -> Vec<(u64, u64, String)> {
    let labels_db = skill_dir.join(crate::constants::LABELS_FILE);
    if !labels_db.exists() { return vec![]; }
    let conn = match rusqlite::Connection::open_with_flags(
        &labels_db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) { Ok(c) => c, Err(_) => return vec![] };
    let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
    let mut stmt = match conn.prepare(
        "SELECT eeg_start, eeg_end, text FROM labels
         WHERE eeg_end >= ?1 AND eeg_start <= ?2
         ORDER BY eeg_start"
    ) { Ok(s) => s, Err(_) => return vec![] };
    stmt.query_map(
        rusqlite::params![start_utc as i64, end_utc as i64],
        |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64, row.get::<_, String>(2)?))
    ).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// Find the first label whose EEG window contains `epoch_utc`.
pub(crate) fn find_label_for_epoch(labels: &[(u64, u64, String)], epoch_utc: u64) -> Option<String> {
    labels.iter()
        .find(|(start, end, _)| epoch_utc >= *start && epoch_utc <= *end)
        .map(|(_, _, text)| text.clone())
}

/// Result of UMAP projection comparing two sessions.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct UmapResult {
    points:  Vec<UmapPoint>,
    n_a:     usize,
    n_b:     usize,
    dim:     usize,
}

/// Load all embedding vectors from daily SQLite DBs in [start, end] UTC range.
pub(crate) fn load_embeddings_range(
    skill_dir: &std::path::Path,
    start_utc: u64,
    end_utc:   u64,
) -> Vec<(u64, Vec<f32>)> {
    use crate::commands::unix_to_ts;
    let ts_start = unix_to_ts(start_utc);
    let ts_end   = unix_to_ts(end_utc);

    let mut out: Vec<(u64, Vec<f32>)> = Vec::new();
    let entries = match std::fs::read_dir(skill_dir) {
        Ok(e) => e, Err(_) => return out,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let db_path = path.join(crate::constants::SQLITE_FILE);
        if !db_path.exists() { continue; }
        let conn = match rusqlite::Connection::open_with_flags(
            &db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) { Ok(c) => c, Err(_) => continue };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        let mut stmt = match conn.prepare(
            "SELECT timestamp, eeg_embedding FROM embeddings
             WHERE timestamp >= ?1 AND timestamp <= ?2 ORDER BY timestamp"
        ) { Ok(s) => s, Err(_) => continue };

        let rows = stmt.query_map(rusqlite::params![ts_start, ts_end], |row| {
            let ts: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let emb: Vec<f32> = blob.chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            Ok((crate::commands::ts_to_unix(ts), emb))
        });
        if let Ok(rows) = rows {
            for r in rows.flatten() { out.push(r); }
        }
    }
    out.sort_by_key(|e| e.0);
    out
}

/// Tauri command: compute UMAP 3D projection (synchronous fallback).
#[tauri::command]
fn compute_umap_compare(
    a_start_utc: u64,
    a_end_utc:   u64,
    b_start_utc: u64,
    b_end_utc:   u64,
    state:       tauri::State<'_, Mutex<AppState>>,
) -> UmapResult {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    match ws_commands::umap_compute_inner(&skill_dir, a_start_utc, a_end_utc, b_start_utc, b_end_utc, None) {
        Ok(val) => serde_json::from_value(val).unwrap_or_default(),
        Err(e) => {
            eprintln!("[umap] compute error: {e}");
            UmapResult::default()
        }
    }
}

/// Enqueue a UMAP comparison as a background job.  Returns a ticket immediately
/// with the estimated completion time so the UI stays responsive.
#[tauri::command]
fn enqueue_umap_compare(
    a_start_utc: u64,
    a_end_utc:   u64,
    b_start_utc: u64,
    b_end_utc:   u64,
    state:       tauri::State<'_, Mutex<AppState>>,
    queue:       tauri::State<'_, std::sync::Arc<job_queue::JobQueue>>,
) -> job_queue::JobTicket {
    let skill_dir = state.lock_or_recover().skill_dir.clone();

    let n_a = load_embeddings_range(&skill_dir, a_start_utc, a_end_utc).len();
    let n_b = load_embeddings_range(&skill_dir, b_start_utc, b_end_utc).len();
    let n = n_a + n_b;
    // Time estimate: KNN is O(n²) on GPU, training is O(epochs × edges).
    let ucfg = load_umap_config(&skill_dir);
    let est_epochs = ucfg.n_epochs.clamp(50, 2000) as u64;
    let estimated_ms = 3000u64
        + (n as u64) * (n as u64) / 20_000
        + (n as u64) * est_epochs / 2000;

    let sd = skill_dir.clone();
    let prog_map = queue.progress_map();
    queue.submit_with_id(estimated_ms, move |job_id| {
        let pm = prog_map;
        let cb: Box<dyn Fn(fast_umap::EpochProgress) + Send> = Box::new(move |ep| {
            let mut map = pm.lock_or_recover();
            map.insert(job_id, job_queue::JobProgress {
                epoch:        ep.epoch,
                total_epochs: ep.total_epochs,
                loss:         ep.loss,
                best_loss:    ep.best_loss,
                elapsed_secs: ep.elapsed_secs,
                epoch_ms:     ep.epoch_ms,
            });
        });
        ws_commands::umap_compute_inner(&sd, a_start_utc, a_end_utc, b_start_utc, b_end_utc, Some(cb))
    })
}

/// Poll the job queue for a result by job id.
#[tauri::command]
fn poll_job(
    job_id: u64,
    queue:  tauri::State<'_, std::sync::Arc<job_queue::JobQueue>>,
) -> job_queue::JobPollResult {
    queue.poll(job_id)
}

/// Tauri IPC wrapper for [`get_session_metrics_impl`].
#[tauri::command]
fn get_session_metrics(
    start_utc: u64,
    end_utc:   u64,
    state:     tauri::State<'_, Mutex<AppState>>,
) -> SessionMetrics {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    get_session_metrics_impl(&skill_dir, start_utc, end_utc)
}

/// Return per-epoch time-series data for charts.
#[tauri::command]
fn get_session_timeseries(
    start_utc: u64,
    end_utc:   u64,
    state:     tauri::State<'_, Mutex<AppState>>,
) -> Vec<EpochRow> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    get_session_timeseries_impl(&skill_dir, start_utc, end_utc)
}

/// Load metrics directly from a session's `_metrics.csv` file.
/// This is the primary path for history view — works without SQLite epochs.
#[tauri::command]
fn get_csv_metrics(csv_path: String) -> Option<CsvMetricsResult> {
    load_metrics_csv(std::path::Path::new(&csv_path))
}

/// Open the session comparison window (or focus it if already open).
#[tauri::command]
async fn open_compare_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("compare") {
        let _ = win.show(); let _ = win.set_focus(); return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "compare", tauri::WebviewUrl::App("compare".into()))
        .title("NeuroSkill™ – Compare")
        .inner_size(780.0, 640.0)
        .min_inner_size(600.0, 440.0)
        .resizable(true)
        .center()
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Open compare window pre-selecting two specific sessions by their UTC ranges.
/// If the window is already open, emit an event to re-select and close/reopen.
#[tauri::command]
async fn open_compare_window_with_sessions(
    app: AppHandle,
    start_a: i64, end_a: i64,
    start_b: i64, end_b: i64,
) -> Result<(), String> {
    // Close the existing compare window if open so we can open with fresh URL.
    if let Some(win) = app.get_webview_window("compare") {
        let _ = win.close();
        // Give it a moment to close
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let url_path = format!(
        "compare?startA={}&endA={}&startB={}&endB={}",
        start_a, end_a, start_b, end_b
    );
    tauri::WebviewWindowBuilder::new(&app, "compare", tauri::WebviewUrl::App(url_path.into()))
        .title("NeuroSkill™ – Compare")
        .inner_size(780.0, 640.0)
        .min_inner_size(600.0, 440.0)
        .resizable(true)
        .center()
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ── App entry-point ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Show a native blocking confirmation dialog on a background thread and exit
/// the app only if the user clicks "Yes".  Must be called from any context
/// (event handler, tray closure, etc.) — spawning its own thread means it
/// never blocks the UI event loop.
fn confirm_and_quit(app: AppHandle) {
    std::thread::spawn(move || {
        let yes = rfd::MessageDialog::new()
            .set_title("Quit NeuroSkill™")
            .set_description("Are you sure you want to quit NeuroSkill™?")
            .set_buttons(rfd::MessageButtons::YesNo)
            .show() == rfd::MessageDialogResult::Yes;
        if yes {
            app.exit(0);
        }
    });
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .manage(Mutex::new(AppState::default()))
        .manage(job_queue::JobQueue::new())
        .manage(std::sync::Arc::new(EmbedderState(std::sync::Mutex::new(None))))
        .manage(std::sync::Arc::new(label_index::LabelIndexState::new()))
        .setup(|app| {
            // Resolve bundled resource paths and register them with the TTS
            // subsystem.  Both calls must happen before any tts_init command
            // fires (i.e. before any worker thread starts) because they write
            // into OnceCell statics — the first write wins.
            {
                use tauri::Manager;
                let resource_dir = app.path().resource_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("resources"));

                // ── espeak-ng data ────────────────────────────────────────────
                // Contents/Resources/espeak-ng-data/ is bundled by build.rs from
                // espeak-static/share/espeak-ng-data/.  Registering it here
                // (via a OnceCell) ensures phonemise() always finds the data,
                // even on machines that have no system espeak-ng installed.
                init_espeak_bundled_data_path(&resource_dir);

                // ── NeuTTS preset voice samples ───────────────────────────────
                let samples_dir = resource_dir.join("neutts-samples");
                init_neutts_samples_dir(samples_dir);
            }

            // hides dock icon
            // app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            
            // ── WebSocket server + mDNS ───────────────────────────────────────
            // Read ws_host / ws_port from persisted settings before binding so
            // the user's preference (loopback vs LAN, custom port) is honoured
            // on every launch.  load_settings() is cheap (one file read).
            let app_name = app.package_info().name.to_lowercase();
            let ws_cfg = {
                let dir = app.state::<Mutex<AppState>>().lock_or_recover().skill_dir.clone();
                let s   = load_settings(&dir);
                (s.ws_host, s.ws_port)
            };
            let (broadcaster, serve_handle) = ws_server::bind_with(ws_cfg.0, ws_cfg.1);
            ws_server::register_mdns(&app_name, serve_handle.port);
            let ws_app = app.handle().clone();
            tauri::async_runtime::spawn(async move { serve_handle.serve(ws_app).await; });
            app.manage(broadcaster);

            // Register Arc<SkillLogger> as its own managed state so any
            // function with &AppHandle can get the logger without locking
            // AppState (avoids deadlock in nested lock scenarios).
            let logger_arc = app.state::<Mutex<AppState>>().lock_or_recover().logger.clone();
            app.manage(logger_arc);

            // Load persisted user settings from ~/.skill/settings.json
            let skill_dir = {
                let r = app.state::<Mutex<AppState>>();
                let g = r.lock_or_recover();
                g.skill_dir.clone()
            };
            let data = load_settings(&skill_dir);
            {
                let r = app.state::<Mutex<AppState>>();
                let mut s = r.lock_or_recover();
                s.status.paired_devices = data.paired.clone();
                s.preferred_id          = data.preferred_id.clone();
                // Restore EEG filter config persisted from the previous session.
                s.status.filter_config  = data.filter_config;
                s.filter.set_config(data.filter_config);
                // Restore embedding overlap from the previous session.
                s.status.embedding_overlap_secs = data.embedding_overlap_secs;
                s.accumulator.set_overlap_secs(data.embedding_overlap_secs);
                // Restore global shortcuts.
                s.label_shortcut        = data.label_shortcut;
                s.search_shortcut       = data.search_shortcut;
                s.settings_shortcut     = data.settings_shortcut;
                s.calibration_shortcut  = data.calibration_shortcut;
                s.help_shortcut         = data.help_shortcut;
                s.history_shortcut      = data.history_shortcut;
                s.api_shortcut          = data.api_shortcut;
                s.theme_shortcut        = data.theme_shortcut;
                s.focus_timer_shortcut  = data.focus_timer_shortcut;
                // Restore calibration profiles (migrate from legacy config if needed).
                let mut profiles = data.calibration_profiles;
                if profiles.is_empty() {
                    profiles.push(CalibrationProfile::from_legacy(&data.calibration));
                }
                s.calibration_profiles = profiles;
                s.active_calibration_id = if data.active_calibration_id.is_empty() {
                    s.calibration_profiles.first().map(|p| p.id.clone()).unwrap_or_default()
                } else {
                    data.active_calibration_id
                };
                // Restore onboarding state.
                s.onboarding_complete = data.onboarding_complete;
                // Restore theme & language.
                s.theme    = data.theme;
                s.language = data.language;
                // Restore daily goal & notification state.
                s.daily_goal_min      = data.daily_goal_min;
                s.goal_notified_date  = data.goal_notified_date;
                // Restore embedding model selection.
                s.text_embedding_model = data.text_embedding_model.clone();
                // Restore WebSocket bind config.
                s.ws_host = data.ws_host.clone();
                s.ws_port = data.ws_port;
                // Restore update-check interval.
                s.update_check_interval_secs = data.update_check_interval_secs;
                // Restore OpenBCI config.
                s.openbci_config = data.openbci;
                // Restore NeuTTS config and sync the TTS module's statics.
                s.neutts_config = data.neutts.clone();
                s.tts_preload   = data.tts_preload;
                neutts_apply_config(&data.neutts);
                // Seed discovered list from paired

                for pd in &data.paired {
                    s.discovered.push(DiscoveredDevice {
                        id: pd.id.clone(), name: pd.name.clone(),
                        last_seen: pd.last_seen, last_rssi: 0,
                        is_paired: true,
                        is_preferred: data.preferred_id.as_deref() == Some(&pd.id),
                    });
                }
            }

            // Pre-warm the active TTS engine in the background if enabled.
            if data.tts_preload {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    crate::tts::tts_init(app_handle).await.ok();
                });
            }

            // Initialise the fastembed text embedder on a background thread
            // so the UI doesn't stall on first-time model download.
            {
                let model_code = {
                    let r = app.state::<Mutex<AppState>>();
                    let mc = r.lock_or_recover().text_embedding_model.clone();
                    mc
                };
                let skill_dir_emb = skill_dir.clone();
                let embedder_arc  = std::sync::Arc::clone(
                    &*app.state::<std::sync::Arc<EmbedderState>>()
                );
                let logger_emb = app.state::<std::sync::Arc<SkillLogger>>().inner().clone();
                std::thread::spawn(move || {
                    init_embedder(&embedder_arc, &model_code, &skill_dir_emb, &logger_emb);
                });
            }

            // Load label HNSW indices (text + EEG spaces) from disk.
            {
                let label_idx = std::sync::Arc::clone(
                    &*app.state::<std::sync::Arc<label_index::LabelIndexState>>()
                );
                let sd = skill_dir.clone();
                std::thread::spawn(move || label_idx.load(&sd));
            }

            // Register all global shortcuts from persisted settings.
            if let Err(e) = apply_all_shortcuts(app.handle()) {
                eprintln!("[shortcut] failed to register shortcuts: {e}");
            }

            // Set native macOS application menu (the menu bar at the top of
            // the screen).  The first submenu title becomes the app name shown
            // in the menu bar, so we name it APP_DISPLAY_NAME.
            // "About NeuroSkill™" is a regular MenuItem wired to our custom About
            // window; the standard Hide / Show-All / Quit predefined items fill
            // the rest of the standard positions.
            // On Windows / Linux there is no global app menu; this block is
            // compiled out entirely.
            #[cfg(target_os = "macos")]
            {
                use tauri::menu::{MenuBuilder, SubmenuBuilder, MenuItem, PredefinedMenuItem};
                let app_submenu = SubmenuBuilder::new(app, constants::APP_DISPLAY_NAME)
                    .item(&MenuItem::with_id(
                        app, "about",
                        format!("About {}", constants::APP_DISPLAY_NAME),
                        true, None::<&str>,
                    )?)
                    .separator()
                    .item(&PredefinedMenuItem::hide(app, None)?)
                    .item(&PredefinedMenuItem::hide_others(app, None)?)
                    .item(&PredefinedMenuItem::show_all(app, None)?)
                    .separator()
                    .item(&MenuItem::with_id(
                        app, "macos_quit",
                        format!("Quit {}", constants::APP_DISPLAY_NAME),
                        true, Some("Cmd+Q"),
                    )?)
                    .build()?;
                // Standard Window menu — gives macOS the Cmd-W / Cmd-M bindings.
                let window_submenu = SubmenuBuilder::new(app, "Window")
                    .item(&PredefinedMenuItem::minimize(app, None)?)
                    .item(&PredefinedMenuItem::maximize(app, None)?)
                    .separator()
                    .item(&PredefinedMenuItem::close_window(app, None)?)
                    .build()?;
                let app_menu = MenuBuilder::new(app)
                    .item(&app_submenu)
                    .item(&window_submenu)
                    .build()?;
                app.set_menu(app_menu).ok();
            }

            // Global menu-event handler — fires for EVERY menu in the app
            // (native app-menu bar on macOS AND tray menu on all platforms).
            // Handling "about" here instead of inside the tray on_menu_event
            // prevents the window from opening twice: without this split,
            // a tray click would hit both the tray handler and this global
            // handler simultaneously.
            app.on_menu_event(|app, event| {
                if event.id().as_ref() == "about" {
                    let a = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = open_about_window(a).await;
                    });
                } else if event.id().as_ref() == "macos_quit" {
                    confirm_and_quit(app.clone());
                }
            });

            // Build tray
            let init_status = { let r = app.state::<Mutex<AppState>>(); let g = r.lock_or_recover(); g.status.clone() };
            let init_menu   = build_menu(app.handle(), &init_status)?;

            TrayIconBuilder::with_id("main")
                .icon(icon_disconnected())
                .tooltip("NeuroSkill™ – Disconnected")
                .menu(&init_menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| {
                    let id = event.id.as_ref();
                    if id == "open_skill" {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    } else if id == "disconnect" || id == "cancel" {
                        {
                            let r = app.state::<Mutex<AppState>>();
                            let mut s = r.lock_or_recover();
                            s.pending_reconnect = false; // user explicitly disconnected
                            s.retry_attempt = 0;
                        }
                        cancel_session(app);
                    } else if id == "scan" || id == "retry" {
                        start_session(app, None);
                    } else if id == "open_bt" {
                        open_bt_settings();
                    } else if id == "calibrate" {
                        let a = app.clone();
                        tauri::async_runtime::spawn(async move { let _ = open_calibration_window_inner(&a, None, false).await; });
                    } else if id == "search" {
                        let a = app.clone();
                        tauri::async_runtime::spawn(async move { let _ = open_search_window(a).await; });
                    } else if id == "label" {
                        let a = app.clone();
                        tauri::async_runtime::spawn(async move { let _ = open_label_window(a).await; });
                    } else if id == "history" {
                        let a = app.clone();
                        tauri::async_runtime::spawn(async move { let _ = open_history_window(a).await; });
                    } else if id == "compare" {
                        let a = app.clone();
                        tauri::async_runtime::spawn(async move { let _ = open_compare_window(a).await; });
                    } else if id == "settings" {
                        let a = app.clone();
                        tauri::async_runtime::spawn(async move { let _ = open_settings_window(a).await; });
                    } else if id == "help" {
                        let a = app.clone();
                        tauri::async_runtime::spawn(async move { let _ = open_help_window(a).await; });
                    } else if id == "api" {
                        let a = app.clone();
                        tauri::async_runtime::spawn(async move { let _ = open_api_window(a).await; });
                    } else if id == "focus_timer" {
                        let a = app.clone();
                        tauri::async_runtime::spawn(async move { let _ = open_focus_timer_window(a).await; });
                    } else if id == "check_update" {
                        let a = app.clone();
                        tauri::async_runtime::spawn(async move { let _ = open_updates_window(a).await; });
                    } else if id == "quit" {
                        confirm_and_quit(app.app_handle().clone());
                    } else if let Some(dev_id) = id.strip_prefix("connect:") {
                        start_session(app, Some(dev_id.to_owned()));
                    } else if let Some(dev_id) = id.strip_prefix("forget:") {
                        let dev_id = dev_id.to_owned();
                        forget_device(dev_id, app.clone());
                    }
                })
                .on_tray_icon_event(|_tray, _event| {
                    // Menu is shown on both left and right click via
                    // show_menu_on_left_click(true).  "Open NeuroSkill™" menu item
                    // handles showing the main window.
                })
                .build(app)?;

            // Hide on startup; intercept close to hide instead of destroy so
            // the webview and all its state survive intact across open/close
            // cycles — onMount runs exactly once for the lifetime of the app.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.hide();
                let w = win.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                });
            }

            // Start background BLE scanner
            let app_scan = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                start_background_scanner(&app_scan);
            });

            // Auto-connect with persistent retry
            let app_auto = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(900)).await;
                let preferred = {
                    let r = app_auto.state::<Mutex<AppState>>();
                    let mut s = r.lock_or_recover();
                    let pref = s.preferred_id.clone()
                        .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()));
                    if pref.is_some() {
                        s.pending_reconnect = true; // always retry on disconnect
                    }
                    pref
                };
                start_session(&app_auto, preferred);
            });

            // Auto-open calibration window on startup (if configured).
            let app_cal = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(1200)).await;
                let auto_start_id: Option<String> = {
                    let r = app_cal.state::<Mutex<AppState>>();
                    let s = r.lock_or_recover();
                    let active_id = &s.active_calibration_id;
                    s.calibration_profiles.iter()
                        .find(|p| &p.id == active_id)
                        .filter(|p| p.auto_start)
                        .map(|p| p.id.clone())
                };
                if let Some(id) = auto_start_id {
                    let _ = open_calibration_window_inner(&app_cal, Some(id), false).await;
                }
            });

            // Auto-open onboarding wizard on first run.
            let app_onboard = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(600)).await;
                let done = {
                    let r = app_onboard.state::<Mutex<AppState>>();
                    let val = r.lock_or_recover().onboarding_complete;
                    val
                };
                if !done {
                    let _ = open_onboarding_window(app_onboard).await;
                }
            });

            // ── Background update-check task ──────────────────────────────
            // Wakes up every `update_check_interval_secs` (re-read each loop
            // iteration so a settings change takes effect without a restart).
            // Emits "update-available" so UpdatesTab can show the banner.
            let app_upd = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri_plugin_updater::UpdaterExt;
                // Initial delay — give the app time to fully start.
                tokio::time::sleep(Duration::from_secs(30)).await;
                loop {
                    let interval_secs = {
                        let r = app_upd.state::<Mutex<AppState>>();
                        let guard = r.lock_or_recover();
                        guard.update_check_interval_secs
                    };
                    if interval_secs == 0 {
                        // Disabled — sleep for a minute and re-check the setting.
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        continue;
                    }
                    tokio::time::sleep(Duration::from_secs(interval_secs)).await;
                    eprintln!("[updater] running background update check");
                    match app_upd.updater() {
                        Err(e) => {
                            eprintln!("[updater] cannot get updater: {e}");
                        }
                        Ok(updater) => {
                            // Enforce a 30-second deadline so a hung or
                            // unreachable endpoint (malformed JSON, 404,
                            // DNS stall, etc.) never blocks the task forever.
                            let result = tokio::time::timeout(
                                Duration::from_secs(30),
                                updater.check(),
                            ).await;
                            match result {
                                Err(_elapsed) => {
                                    eprintln!("[updater] check timed out after 30 s");
                                }
                                Ok(Ok(Some(update))) => {
                                    eprintln!("[updater] update available: {}", update.version);
                                    let payload = serde_json::json!({
                                        "version": update.version,
                                        "date":    update.date,
                                        "body":    update.body,
                                    });
                                    let _ = app_upd.emit("update-available", payload);
                                }
                                Ok(Ok(None)) => {
                                    eprintln!("[updater] up to date");
                                    let _ = app_upd.emit("update-checked", ());
                                }
                                Ok(Err(e)) => {
                                    // Non-fatal: endpoint unreachable, JSON missing/
                                    // malformed, signature mismatch, etc. The loop
                                    // will retry after the next interval sleep.
                                    eprintln!("[updater] check failed: {e}");
                                }
                            }
                        }
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            subscribe_eeg,
            subscribe_ppg,
            subscribe_imu,
            get_status, get_devices,
            set_preferred_device, forget_device, retry_connect, cancel_retry,
            open_bt_settings, open_settings_window, open_updates_window, open_model_tab, open_help_window,
            get_filter_config, set_filter_config, set_notch_preset,
            get_latest_bands,
            get_embedding_overlap, set_embedding_overlap,
            get_gpu_stats,
            get_log_config, set_log_config,
            get_eeg_model_config, set_eeg_model_config, get_eeg_model_status,
            trigger_weights_download, cancel_weights_download,
            get_umap_config, set_umap_config,
            get_theme_and_language, set_theme, set_language,
            get_daily_goal, set_daily_goal,
            get_goal_notified_date, set_goal_notified_date,
            get_daily_recording_mins,
            quit_app, open_label_window, open_labels_window, open_focus_timer_window, submit_label, close_label_window,
            query_annotations, delete_label, update_label, get_queue_stats,
            list_embedding_models, get_embedding_model, set_embedding_model,
            reembed_all_labels, get_stale_label_count,
            rebuild_label_index, search_labels_by_text, search_labels_by_eeg,
            open_search_window,
            open_history_window, list_sessions, list_session_days, list_sessions_for_day,
            stream_sessions, get_history_stats, delete_session,
            open_compare_window, open_compare_window_with_sessions, get_session_metrics, get_session_timeseries, get_csv_metrics, list_embedding_sessions, get_sleep_stages, compute_umap_compare, enqueue_umap_compare, poll_job,
            get_label_shortcut, set_label_shortcut,
            get_search_shortcut, set_search_shortcut,
            get_settings_shortcut, set_settings_shortcut,
            get_calibration_shortcut, set_calibration_shortcut,
            get_help_shortcut, set_help_shortcut,
            get_history_shortcut, set_history_shortcut,
            get_api_shortcut, set_api_shortcut,
            get_theme_shortcut, set_theme_shortcut,
            get_focus_timer_shortcut, set_focus_timer_shortcut,
            open_calibration_window, open_and_start_calibration, close_calibration_window,
            list_calibration_profiles, get_calibration_profile, get_active_calibration,
            set_active_calibration, create_calibration_profile, update_calibration_profile,
            delete_calibration_profile, record_calibration_completed,
            get_calibration_config, set_calibration_config,
            emit_calibration_event,
            get_app_version,
            get_app_name,
            get_data_dir, set_data_dir,
            get_ws_clients, get_ws_request_log, get_ws_port,
            get_ws_config, set_ws_config,
            get_autostart_enabled, set_autostart_enabled,
            get_update_check_interval, set_update_check_interval,
            get_openbci_config, set_openbci_config, list_serial_ports,
            get_neutts_config, set_neutts_config, pick_ref_wav_file,
            get_tts_preload, set_tts_preload,
            tts_unload, tts_get_voice, tts_list_neutts_voices,
            connect_openbci,
            open_api_window,
            open_onboarding_window, complete_onboarding, get_onboarding_complete,
            commands::search_embeddings,
            commands::enqueue_search_embeddings,
            commands::stream_search_embeddings,
            commands::find_session_for_timestamp,
            commands::interactive_search,
            commands::save_dot_file,
            commands::save_svg_file,
            open_session_window,
            tts_init,
            tts_speak,
            tts_list_voices,
            tts_set_voice,
            get_about_info,
            open_about_window,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            match event {
                // Closing the last window fires ExitRequested with code == None.
                // Suppress it so the app keeps running in the tray.
                // An explicit quit (tray → Quit → app.exit(0)) sets code = Some(0)
                // and is allowed through unchanged.
                tauri::RunEvent::ExitRequested { api, code, .. } => {
                    if code.is_none() {
                        api.prevent_exit();
                    }
                }

                // Explicitly release TTS backends (especially the NeuTTS llama.cpp/Metal
                // context) before the process calls exit().  Without this, `exit()` fires
                // C++ static destructors while Metal resource sets are still live, which
                // hits the `ggml_metal_device_free` assertion and crashes on shutdown.
                tauri::RunEvent::Exit => {
                    tts_shutdown();
                }

                _ => {}
            }
        });
}

// ── Unit tests ─────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::retry_delay_secs;

    #[test]
    fn backoff_schedule_1_2_3_5() {
        // First four attempts follow the flat schedule
        assert_eq!(retry_delay_secs(0), 1, "attempt 0 → 1 s");
        assert_eq!(retry_delay_secs(1), 2, "attempt 1 → 2 s");
        assert_eq!(retry_delay_secs(2), 3, "attempt 2 → 3 s");
        assert_eq!(retry_delay_secs(3), 5, "attempt 3 → 5 s");
    }

    #[test]
    fn backoff_capped_at_5s() {
        // All attempts beyond 2 must stay at exactly 5 s — never grow above it
        for attempt in 3u32..=100 {
            assert_eq!(
                retry_delay_secs(attempt), 5,
                "attempt {attempt} should be capped at 5 s"
            );
        }
    }
}
