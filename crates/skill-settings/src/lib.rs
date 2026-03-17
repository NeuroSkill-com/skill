// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Persistent configuration types and disk I/O.
//!
//! This module owns every struct that is serialised to disk
//! (`~/.skill/settings.json`, `umap_config.json`) as well as the helpers
//! to load and compute defaults.  It has **no dependency on `AppState`**.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

use skill_constants::{
    SETTINGS_FILE, UMAP_CONFIG_FILE,
    CALIBRATION_ACTION1_LABEL, CALIBRATION_ACTION2_LABEL,
    CALIBRATION_ACTION_DURATION_SECS, CALIBRATION_BREAK_DURATION_SECS,
    CALIBRATION_LOOP_COUNT, CALIBRATION_AUTO_START,
    EMBEDDING_OVERLAP_SECS,
};
use skill_eeg::eeg_filter::FilterConfig;

// Re-export PairedDevice from skill-data so consumers can use
// `skill_settings::PairedDevice`.
pub use skill_data::device::PairedDevice;

// Re-export NeuttsConfig from skill-tts.
pub use skill_tts::NeuttsConfig;
pub use skill_tts::config::default_neutts_backbone_repo;

// Re-export LLM config types from skill-llm.
pub use skill_llm::config::{LlmConfig, LlmToolConfig, ToolExecutionMode};

// Re-export ScreenshotConfig from skill-screenshots.
pub use skill_screenshots::ScreenshotConfig;

// ── OpenBCI board configuration ───────────────────────────────────────────────

/// Which OpenBCI board the user wants to use (persisted in settings.json).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OpenBciBoard {
    /// Ganglion 4-channel board over Bluetooth LE (default).
    #[default]
    Ganglion,
    /// Ganglion 4-channel board via the OpenBCI WiFi Shield.
    GanglionWifi,
    /// Cyton 8-channel board over USB serial dongle.
    Cyton,
    /// Cyton 8-channel board via the OpenBCI WiFi Shield (1 kHz).
    CytonWifi,
    /// Cyton + Daisy 16-channel board over USB serial dongle (125 Hz).
    CytonDaisy,
    /// Cyton + Daisy 16-channel board via the OpenBCI WiFi Shield (125 Hz).
    CytonDaisyWifi,
    /// Galea 24-channel research headset over UDP (250 Hz).
    Galea,
}

impl OpenBciBoard {
    /// Number of EEG channels produced by this board.
    pub fn channel_count(&self) -> usize {
        match self {
            Self::Ganglion | Self::GanglionWifi => 4,
            Self::Cyton    | Self::CytonWifi    => 8,
            Self::CytonDaisy | Self::CytonDaisyWifi => 16,
            Self::Galea                          => 24,
        }
    }

    /// Nominal sampling rate in Hz.
    pub fn sample_rate(&self) -> f64 {
        match self {
            Self::Ganglion | Self::GanglionWifi  => 200.0,
            Self::Cyton    | Self::CytonDaisy    => 250.0,
            Self::CytonWifi                      => 1000.0,
            Self::CytonDaisyWifi                 => 125.0,
            Self::Galea                          => 250.0,
        }
    }

    /// Returns `true` for boards that connect via BLE (Ganglion only).
    pub fn is_ble(&self) -> bool { matches!(self, Self::Ganglion) }

    /// Returns `true` for boards that use a serial USB dongle.
    pub fn is_serial(&self) -> bool {
        matches!(self, Self::Cyton | Self::CytonDaisy)
    }

    /// Returns `true` for boards that use the WiFi Shield.
    pub fn is_wifi(&self) -> bool {
        matches!(self, Self::GanglionWifi | Self::CytonWifi | Self::CytonDaisyWifi)
    }
}

/// User-configurable OpenBCI settings, persisted in `settings.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenBciConfig {
    /// Which board variant to use.
    pub board: OpenBciBoard,

    // ── BLE (Ganglion) ────────────────────────────────────────────────────────
    /// BLE scan timeout in seconds for Ganglion BLE (default 10).
    pub scan_timeout_secs: u32,

    // ── Serial (Cyton, CytonDaisy) ────────────────────────────────────────────
    /// Serial port path for Cyton/CytonDaisy (e.g. `/dev/ttyUSB0` or `COM3`).
    /// Leave empty to auto-select the first detected port.
    pub serial_port: String,

    // ── WiFi Shield (GanglionWifi, CytonWifi, CytonDaisyWifi) ────────────────
    /// IP address of the OpenBCI WiFi Shield (empty = auto-discover via SSDP).
    pub wifi_shield_ip: String,
    /// Local TCP port the driver listens on for incoming board data (default 3000).
    pub wifi_local_port: u16,

    // ── Galea ─────────────────────────────────────────────────────────────────
    /// IP address of the Galea headset (empty = accept from any source).
    pub galea_ip: String,

    // ── Common ────────────────────────────────────────────────────────────────
    /// Human-readable label for each channel.
    /// Empty strings fall back to the board's default label for that position.
    pub channel_labels: Vec<String>,
}

impl Default for OpenBciConfig {
    fn default() -> Self {
        Self {
            board:            OpenBciBoard::default(),
            scan_timeout_secs: 10,
            serial_port:      String::new(),
            wifi_shield_ip:   String::new(),
            wifi_local_port:  3000,
            galea_ip:         String::new(),
            channel_labels:   Vec::new(),
        }
    }
}

// ── Sleep schedule ─────────────────────────────────────────────────────────────

/// Named sleep-schedule preset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SleepPreset {
    /// Default 23:00–07:00 (8 h).
    #[default]
    Default,
    /// Early bird: 21:30–05:30 (8 h).
    EarlyBird,
    /// Night owl: 01:00–09:00 (8 h).
    NightOwl,
    /// Short sleeper: 00:00–06:00 (6 h).
    ShortSleeper,
    /// Long sleeper: 22:00–08:00 (10 h).
    LongSleeper,
    /// User-edited values that don't match any built-in preset.
    Custom,
}

/// User-configurable sleep schedule, persisted in `settings.json`.
///
/// Times are stored as `"HH:MM"` strings in 24-hour format so they
/// round-trip cleanly through JSON without time-zone ambiguity.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SleepConfig {
    /// Bedtime in `"HH:MM"` 24-h format.
    pub bedtime: String,
    /// Wake-up time in `"HH:MM"` 24-h format.
    pub wake_time: String,
    /// Which preset (if any) these times correspond to.
    pub preset: SleepPreset,
}

impl Default for SleepConfig {
    fn default() -> Self {
        Self {
            bedtime:   "23:00".into(),
            wake_time: "07:00".into(),
            preset:    SleepPreset::Default,
        }
    }
}

impl SleepConfig {
    /// Sleep duration in minutes (handles overnight wrap).
    pub fn duration_minutes(&self) -> u32 {
        let (bh, bm) = parse_hhmm(&self.bedtime);
        let (wh, wm) = parse_hhmm(&self.wake_time);
        let bed  = bh * 60 + bm;
        let wake = wh * 60 + wm;
        if wake >= bed { wake - bed } else { (24 * 60 - bed) + wake }
    }
}

/// Parse `"HH:MM"` → `(hour, minute)`.  Falls back to `(0, 0)` on bad input.
fn parse_hhmm(s: &str) -> (u32, u32) {
    let mut parts = s.splitn(2, ':');
    let h: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let m: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    (h.min(23), m.min(59))
}

// ── UMAP config ────────────────────────────────────────────────────────────────

/// User-configurable UMAP parameters, persisted to `~/.skill/umap_config.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UmapUserConfig {
    pub repulsion_strength: f32,
    pub neg_sample_rate:    usize,
    pub timeout_secs:       u64,
    pub n_epochs:           usize,
    pub n_neighbors:        usize,
    /// Milliseconds to sleep between training epochs (0 = max throughput).
    pub cooldown_ms:        u64,
}

impl Default for UmapUserConfig {
    fn default() -> Self {
        Self {
            repulsion_strength: 3.0,
            neg_sample_rate:    15,
            timeout_secs:       120,
            n_epochs:           500,
            n_neighbors:        15,
            cooldown_ms:        0,
        }
    }
}

pub fn load_umap_config(skill_dir: &Path) -> UmapUserConfig {
    skill_data::util::load_json_or_default(&skill_dir.join(UMAP_CONFIG_FILE))
}

pub fn save_umap_config(skill_dir: &Path, cfg: &UmapUserConfig) {
    let _ = std::fs::create_dir_all(skill_dir);
    skill_data::util::save_json(&skill_dir.join(UMAP_CONFIG_FILE), cfg);
}

// ── Calibration types ─────────────────────────────────────────────────────────

/// A single action phase within a calibration profile.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CalibrationAction {
    pub label:         String,
    pub duration_secs: u32,
}

/// A named calibration protocol that can be selected, run, and stored.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CalibrationProfile {
    pub id:                   String,
    pub name:                 String,
    pub actions:              Vec<CalibrationAction>,
    pub break_duration_secs:  u32,
    pub loop_count:           u32,
    pub auto_start:           bool,
    pub last_calibration_utc: Option<u64>,
}

impl Default for CalibrationProfile {
    fn default() -> Self {
        Self {
            id:   "default".into(),
            name: "Default".into(),
            actions: vec![
                CalibrationAction { label: CALIBRATION_ACTION1_LABEL.into(), duration_secs: CALIBRATION_ACTION_DURATION_SECS },
                CalibrationAction { label: CALIBRATION_ACTION2_LABEL.into(), duration_secs: CALIBRATION_ACTION_DURATION_SECS },
            ],
            break_duration_secs:  CALIBRATION_BREAK_DURATION_SECS,
            loop_count:           CALIBRATION_LOOP_COUNT,
            auto_start:           CALIBRATION_AUTO_START,
            last_calibration_utc: None,
        }
    }
}

impl CalibrationProfile {
    pub fn from_legacy(cfg: &CalibrationConfig) -> Self {
        Self {
            id:   "default".into(),
            name: "Default".into(),
            actions: vec![
                CalibrationAction { label: cfg.action1_label.clone(),  duration_secs: cfg.action_duration_secs },
                CalibrationAction { label: cfg.action2_label.clone(), duration_secs: cfg.action_duration_secs },
            ],
            break_duration_secs:  cfg.break_duration_secs,
            loop_count:           cfg.loop_count,
            auto_start:           cfg.auto_start,
            last_calibration_utc: cfg.last_calibration_utc,
        }
    }
}

/// Generate a stable time-based profile ID.
pub fn new_profile_id() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("cal_{ms}")
}

/// Legacy two-action config — kept only for migration from old settings files.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CalibrationConfig {
    pub action1_label:        String,
    pub action2_label:        String,
    pub action_duration_secs: u32,
    pub break_duration_secs:  u32,
    pub loop_count:           u32,
    pub auto_start:           bool,
    pub last_calibration_utc: Option<u64>,
}

impl Default for CalibrationConfig {
    fn default() -> Self {
        Self {
            action1_label:        CALIBRATION_ACTION1_LABEL.into(),
            action2_label:        CALIBRATION_ACTION2_LABEL.into(),
            action_duration_secs: CALIBRATION_ACTION_DURATION_SECS,
            break_duration_secs:  CALIBRATION_BREAK_DURATION_SECS,
            loop_count:           CALIBRATION_LOOP_COUNT,
            auto_start:           CALIBRATION_AUTO_START,
            last_calibration_utc: None,
        }
    }
}

// ── Path helpers ──────────────────────────────────────────────────────────────

/// The skill data directory.
///
/// | Platform | Path |
/// |---|---|
/// | macOS / Linux | `~/.skill` |
/// | Windows | `%LOCALAPPDATA%\NeuroSkill` |
pub fn default_skill_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| {
                std::env::var("APPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| std::env::temp_dir())
            })
            .join("NeuroSkill")
    }
    #[cfg(not(target_os = "windows"))]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(skill_constants::SKILL_DIR)
    }
}

pub fn settings_path(skill_dir: &Path) -> PathBuf {
    skill_dir.join(SETTINGS_FILE)
}

/// Collapse an absolute path back to a shorter human-readable form.
///
/// On Unix the home directory is abbreviated to `~`.
/// On Windows `HOME` is typically unset; we fall back to `USERPROFILE`.
pub fn tilde_path(p: &Path) -> String {
    let home_str = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if !home_str.is_empty() {
        let h = home_str.trim_end_matches(['/', '\\']);
        let s = p.to_string_lossy();
        if s == h { return "~".into(); }
        if let Some(rest) = s.strip_prefix(h) {
            if rest.starts_with('/') || rest.starts_with('\\') {
                return format!("~{rest}");
            }
        }
    }
    p.to_string_lossy().to_string()
}

// ── Default values ────────────────────────────────────────────────────────────

pub fn default_ws_host() -> String { skill_constants::WS_HOST.into() }
pub fn default_ws_port() -> u16    { skill_constants::WS_DEFAULT_PORT }
pub fn default_update_check_interval() -> u64 {
    skill_constants::UPDATER_CHECK_INTERVAL_SECS
}
pub fn default_theme()        -> String { "system".into() }
pub fn default_accent_color() -> String { "violet".into() }
pub fn default_daily_goal_min()       -> u32    { 60 }
pub fn default_embedding_model()      -> String { "Xenova/bge-small-en-v1.5".into() }
pub fn default_overlap_secs()         -> f32    { EMBEDDING_OVERLAP_SECS }
pub fn default_label_shortcut()       -> String { "CmdOrCtrl+Shift+L".into() }
pub fn default_search_shortcut()      -> String { "CmdOrCtrl+Shift+S".into() }
pub fn default_settings_shortcut()    -> String { "CmdOrCtrl+,".into() }
pub fn default_calibration_shortcut() -> String { "CmdOrCtrl+Shift+C".into() }
pub fn default_help_shortcut()        -> String { "CmdOrCtrl+Shift+H".into() }
pub fn default_history_shortcut()     -> String { "CmdOrCtrl+Shift+J".into() }
pub fn default_api_shortcut()         -> String { "CmdOrCtrl+Shift+A".into() }
pub fn default_theme_shortcut()       -> String { "CmdOrCtrl+Shift+T".into() }
pub fn default_focus_timer_shortcut() -> String { "CmdOrCtrl+Shift+P".into() }
#[cfg(feature = "llm")]
pub fn default_chat_shortcut()        -> String { "CmdOrCtrl+Shift+I".into() }
pub fn default_hook_distance_threshold() -> f32 { 0.1 }
pub fn default_hook_recent_limit() -> usize { 12 }
pub fn default_hook_scenario() -> String { "any".into() }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HookRule {
    pub name: String,
    pub enabled: bool,
    pub keywords: Vec<String>,
    #[serde(default = "default_hook_scenario")]
    pub scenario: String,
    pub command: String,
    pub text: String,
    #[serde(default = "default_hook_distance_threshold")]
    pub distance_threshold: f32,
    #[serde(default = "default_hook_recent_limit")]
    pub recent_limit: usize,
}

impl Default for HookRule {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
            keywords: Vec::new(),
            scenario: default_hook_scenario(),
            command: String::new(),
            text: String::new(),
            distance_threshold: default_hook_distance_threshold(),
            recent_limit: default_hook_recent_limit(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct HookLastTrigger {
    pub triggered_at_utc: u64,
    pub distance: f32,
    pub label_id: Option<i64>,
    pub label_text: Option<String>,
    pub label_eeg_start_utc: Option<u64>,
}

impl Default for HookLastTrigger {
    fn default() -> Self {
        Self {
            triggered_at_utc: 0,
            distance: 0.0,
            label_id: None,
            label_text: None,
            label_eeg_start_utc: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HookStatus {
    pub hook: HookRule,
    pub last_trigger: Option<HookLastTrigger>,
}

// ── UserSettings (serialised to settings.json) ────────────────────────────────

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct UserSettings {
    pub paired:                 Vec<PairedDevice>,
    pub preferred_id:           Option<String>,
    pub filter_config:          FilterConfig,
    #[serde(default = "default_overlap_secs")]
    pub embedding_overlap_secs: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dir:               Option<String>,
    #[serde(default = "default_label_shortcut")]
    pub label_shortcut:         String,
    #[serde(default = "default_search_shortcut")]
    pub search_shortcut:        String,
    #[serde(default = "default_settings_shortcut")]
    pub settings_shortcut:      String,
    #[serde(default = "default_calibration_shortcut")]
    pub calibration_shortcut:   String,
    #[serde(default = "default_help_shortcut")]
    pub help_shortcut:          String,
    #[serde(default = "default_history_shortcut")]
    pub history_shortcut:       String,
    #[serde(default = "default_api_shortcut")]
    pub api_shortcut:           String,
    #[serde(default = "default_theme_shortcut")]
    pub theme_shortcut:         String,
    #[serde(default = "default_focus_timer_shortcut")]
    pub focus_timer_shortcut:   String,
    #[cfg(feature = "llm")]
    #[serde(default = "default_chat_shortcut")]
    pub chat_shortcut:          String,
    /// Legacy two-action config — read once to migrate; never written back.
    #[serde(default, skip_serializing)]
    pub calibration:            CalibrationConfig,
    #[serde(default)]
    pub calibration_profiles:   Vec<CalibrationProfile>,
    #[serde(default)]
    pub active_calibration_id:  String,
    #[serde(default)]
    pub onboarding_complete:    bool,
    #[serde(default = "default_theme")]
    pub theme:                  String,
    #[serde(default)]
    pub language:               String,
    #[serde(default = "default_accent_color")]
    pub accent_color:           String,
    #[serde(default = "default_daily_goal_min")]
    pub daily_goal_min:         u32,
    #[serde(default)]
    pub goal_notified_date:     String,
    #[serde(default = "default_embedding_model")]
    pub text_embedding_model:   String,
    #[serde(default)]
    pub hooks:                  Vec<HookRule>,
    /// WebSocket server bind host.
    #[serde(default = "default_ws_host")]
    pub ws_host: String,
    /// Preferred WebSocket server port.
    #[serde(default = "default_ws_port")]
    pub ws_port: u16,
    /// Seconds between automatic background update checks (0 = disabled).
    #[serde(default = "default_update_check_interval")]
    pub update_check_interval_secs: u64,
    /// OpenBCI board configuration.
    #[serde(default)]
    pub openbci: OpenBciConfig,
    /// NeuTTS voice-cloning TTS configuration.
    #[serde(default)]
    pub neutts: NeuttsConfig,
    /// Pre-warm the active TTS engine at startup.
    #[serde(default = "default_tts_preload")]
    pub tts_preload: bool,
    /// Track the active window while a session is running.
    #[serde(default = "default_track_active_window")]
    pub track_active_window: bool,
    /// Track the last keyboard and mouse input timestamps.
    #[serde(default = "default_track_input_activity")]
    pub track_input_activity: bool,
    /// Automatic Do Not Disturb when focus is sustained.
    #[serde(default)]
    pub do_not_disturb: DoNotDisturbConfig,
    /// Last app version for which the "What's New" window was shown.
    #[serde(default)]
    pub last_seen_whats_new_version: String,
    /// Embedded OpenAI-compatible LLM inference server.
    #[serde(default)]
    pub llm: LlmConfig,
    /// Screenshot capture + vision embedding configuration.
    #[serde(default)]
    pub screenshot: ScreenshotConfig,
    /// Sleep schedule configuration.
    #[serde(default)]
    pub sleep: SleepConfig,
}

pub fn default_tts_preload() -> bool { true }
pub fn default_track_active_window() -> bool { true }
pub fn default_track_input_activity() -> bool { true }

// ── Do Not Disturb automation ─────────────────────────────────────────────────

pub fn default_dnd_threshold() -> f32 { 60.0 }
pub fn default_dnd_duration_secs() -> u32 { 60 }
pub fn default_dnd_exit_duration_secs() -> u32 { 300 }
pub fn default_dnd_focus_lookback_secs() -> u32 { 60 }
pub fn default_dnd_mode_identifier() -> String {
    "com.apple.donotdisturb.mode.default".to_owned()
}
pub fn default_dnd_exit_notification() -> bool { true }
pub fn default_dnd_snr_exit_db() -> f32 { 0.0 }

/// Configuration for the "auto Do Not Disturb when focus is sustained" feature.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DoNotDisturbConfig {
    /// Whether the feature is enabled.  Default: `false`.
    pub enabled: bool,
    /// Focus score (0–100) that must be sustained to activate DND.
    #[serde(default = "default_dnd_threshold")]
    pub focus_threshold: f32,
    /// Seconds focus must remain above the threshold before DND activates.
    #[serde(default = "default_dnd_duration_secs")]
    pub duration_secs: u32,
    /// Seconds the focus score must remain *below* the threshold before DND
    /// is cleared.
    #[serde(default = "default_dnd_exit_duration_secs")]
    pub exit_duration_secs: u32,
    /// Lookback window in seconds.
    #[serde(default = "default_dnd_focus_lookback_secs")]
    pub focus_lookback_secs: u32,
    /// The focus mode identifier to activate.
    #[serde(default = "default_dnd_mode_identifier")]
    pub focus_mode_identifier: String,
    /// Whether to send an OS notification when focus mode is deactivated.
    #[serde(default = "default_dnd_exit_notification")]
    pub exit_notification: bool,
    /// SNR threshold (dB) below which focus mode is forcibly deactivated.
    /// Default: 0.0 dB.
    #[serde(default = "default_dnd_snr_exit_db")]
    pub snr_exit_db: f32,
}

impl Default for DoNotDisturbConfig {
    fn default() -> Self {
        Self {
            enabled:               false,
            focus_threshold:       default_dnd_threshold(),
            duration_secs:         default_dnd_duration_secs(),
            exit_duration_secs:    default_dnd_exit_duration_secs(),
            focus_lookback_secs:   default_dnd_focus_lookback_secs(),
            focus_mode_identifier: default_dnd_mode_identifier(),
            exit_notification:     default_dnd_exit_notification(),
            snr_exit_db:           default_dnd_snr_exit_db(),
        }
    }
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            paired:                 Vec::new(),
            preferred_id:           None,
            filter_config:          FilterConfig::default(),
            embedding_overlap_secs: EMBEDDING_OVERLAP_SECS,
            data_dir:               None,
            label_shortcut:         default_label_shortcut(),
            search_shortcut:        default_search_shortcut(),
            settings_shortcut:      default_settings_shortcut(),
            calibration_shortcut:   default_calibration_shortcut(),
            help_shortcut:          default_help_shortcut(),
            history_shortcut:       default_history_shortcut(),
            api_shortcut:           default_api_shortcut(),
            theme_shortcut:         default_theme_shortcut(),
            focus_timer_shortcut:   default_focus_timer_shortcut(),
            #[cfg(feature = "llm")]
            chat_shortcut:          default_chat_shortcut(),
            calibration:            CalibrationConfig::default(),
            calibration_profiles:   Vec::new(),
            active_calibration_id:  String::new(),
            onboarding_complete:    false,
            theme:                  default_theme(),
            language:               String::new(),
            daily_goal_min:         default_daily_goal_min(),
            goal_notified_date:     String::new(),
            text_embedding_model:   default_embedding_model(),
            hooks:                  Vec::new(),
            ws_host:                default_ws_host(),
            ws_port:                default_ws_port(),
            update_check_interval_secs: default_update_check_interval(),
            openbci:                OpenBciConfig::default(),
            neutts:                 NeuttsConfig::default(),
            tts_preload:            default_tts_preload(),
            track_active_window:    default_track_active_window(),
            track_input_activity:          default_track_input_activity(),
            do_not_disturb:                DoNotDisturbConfig::default(),
            last_seen_whats_new_version:   String::new(),
            llm:                           LlmConfig::default(),
            accent_color:                  default_accent_color(),
            screenshot:                    ScreenshotConfig::default(),
            sleep:                         SleepConfig::default(),
        }
    }
}

pub fn load_settings(skill_dir: &Path) -> UserSettings {
    let mut s: UserSettings = skill_data::util::load_json_or_default(&settings_path(skill_dir));

    // ── Shortcut migrations ──────────────────────────────────────────────
    if s.search_shortcut   == "CmdOrCtrl+Shift+F" { s.search_shortcut   = default_search_shortcut(); }
    if s.settings_shortcut == "CmdOrCtrl+Shift+S" { s.settings_shortcut = default_settings_shortcut(); }

    s
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── OpenBciBoard::channel_count ───────────────────────────────────────────

    #[test]
    fn ganglion_has_4_channels() {
        assert_eq!(OpenBciBoard::Ganglion.channel_count(), 4);
        assert_eq!(OpenBciBoard::GanglionWifi.channel_count(), 4);
    }

    #[test]
    fn cyton_has_8_channels() {
        assert_eq!(OpenBciBoard::Cyton.channel_count(), 8);
        assert_eq!(OpenBciBoard::CytonWifi.channel_count(), 8);
    }

    #[test]
    fn cyton_daisy_has_16_channels() {
        assert_eq!(OpenBciBoard::CytonDaisy.channel_count(), 16);
        assert_eq!(OpenBciBoard::CytonDaisyWifi.channel_count(), 16);
    }

    #[test]
    fn galea_has_24_channels() {
        assert_eq!(OpenBciBoard::Galea.channel_count(), 24);
    }

    // ── OpenBciBoard::sample_rate ─────────────────────────────────────────────

    #[test]
    fn ganglion_sample_rate_is_200() {
        assert!((OpenBciBoard::Ganglion.sample_rate()     - 200.0).abs() < 1e-6);
        assert!((OpenBciBoard::GanglionWifi.sample_rate() - 200.0).abs() < 1e-6);
    }

    #[test]
    fn cyton_sample_rate_is_250() {
        assert!((OpenBciBoard::Cyton.sample_rate()     - 250.0).abs() < 1e-6);
        assert!((OpenBciBoard::CytonDaisy.sample_rate() - 250.0).abs() < 1e-6);
        assert!((OpenBciBoard::Galea.sample_rate()     - 250.0).abs() < 1e-6);
    }

    #[test]
    fn cyton_wifi_sample_rate_is_1000() {
        assert!((OpenBciBoard::CytonWifi.sample_rate() - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn cyton_daisy_wifi_sample_rate_is_125() {
        assert!((OpenBciBoard::CytonDaisyWifi.sample_rate() - 125.0).abs() < 1e-6);
    }

    // ── OpenBciBoard connection predicates ────────────────────────────────────

    #[test]
    fn ganglion_is_ble_only() {
        assert!(OpenBciBoard::Ganglion.is_ble());
        assert!(!OpenBciBoard::GanglionWifi.is_ble());
        assert!(!OpenBciBoard::Cyton.is_ble());
        assert!(!OpenBciBoard::Galea.is_ble());
    }

    #[test]
    fn serial_boards_are_cyton_and_cyton_daisy() {
        assert!(OpenBciBoard::Cyton.is_serial());
        assert!(OpenBciBoard::CytonDaisy.is_serial());
        assert!(!OpenBciBoard::Ganglion.is_serial());
        assert!(!OpenBciBoard::CytonWifi.is_serial());
        assert!(!OpenBciBoard::CytonDaisyWifi.is_serial());
        assert!(!OpenBciBoard::Galea.is_serial());
    }

    #[test]
    fn wifi_boards_are_wifi_variants() {
        assert!(OpenBciBoard::GanglionWifi.is_wifi());
        assert!(OpenBciBoard::CytonWifi.is_wifi());
        assert!(OpenBciBoard::CytonDaisyWifi.is_wifi());
        assert!(!OpenBciBoard::Ganglion.is_wifi());
        assert!(!OpenBciBoard::Cyton.is_wifi());
        assert!(!OpenBciBoard::Galea.is_wifi());
    }

    #[test]
    fn exactly_one_connection_type_per_board() {
        for board in [
            OpenBciBoard::Ganglion, OpenBciBoard::GanglionWifi,
            OpenBciBoard::Cyton,    OpenBciBoard::CytonWifi,
            OpenBciBoard::CytonDaisy, OpenBciBoard::CytonDaisyWifi,
            OpenBciBoard::Galea,
        ] {
            let kinds = [board.is_ble(), board.is_serial(), board.is_wifi()]
                .iter()
                .filter(|&&b| b)
                .count();
            assert!(kinds <= 1, "{board:?} reports more than one connection type");
        }
    }

    #[test]
    fn default_board_is_ganglion() {
        assert_eq!(OpenBciBoard::default(), OpenBciBoard::Ganglion);
    }

    // ── CalibrationProfile defaults ───────────────────────────────────────────

    #[test]
    fn default_calibration_profile_has_two_actions() {
        let p = CalibrationProfile::default();
        assert_eq!(p.actions.len(), 2);
    }

    #[test]
    fn default_calibration_profile_action_labels_match_constants() {
        let p = CalibrationProfile::default();
        assert_eq!(p.actions[0].label, skill_constants::CALIBRATION_ACTION1_LABEL);
        assert_eq!(p.actions[1].label, skill_constants::CALIBRATION_ACTION2_LABEL);
    }

    #[test]
    fn default_calibration_profile_durations_match_constants() {
        let p = CalibrationProfile::default();
        assert_eq!(p.actions[0].duration_secs, skill_constants::CALIBRATION_ACTION_DURATION_SECS);
        assert_eq!(p.actions[1].duration_secs, skill_constants::CALIBRATION_ACTION_DURATION_SECS);
        assert_eq!(p.break_duration_secs, skill_constants::CALIBRATION_BREAK_DURATION_SECS);
        assert_eq!(p.loop_count,          skill_constants::CALIBRATION_LOOP_COUNT);
        assert_eq!(p.auto_start,          skill_constants::CALIBRATION_AUTO_START);
    }

    #[test]
    fn default_calibration_profile_id_is_default() {
        assert_eq!(CalibrationProfile::default().id, "default");
    }

    // ── UmapUserConfig defaults ───────────────────────────────────────────────

    #[test]
    fn default_umap_config_n_neighbors_is_15() {
        assert_eq!(UmapUserConfig::default().n_neighbors, 15);
    }

    #[test]
    fn default_umap_config_n_epochs_is_500() {
        assert_eq!(UmapUserConfig::default().n_epochs, 500);
    }

    #[test]
    fn default_umap_config_timeout_is_120s() {
        assert_eq!(UmapUserConfig::default().timeout_secs, 120);
    }

    // ── tilde_path ────────────────────────────────────────────────────────────

    #[test]
    fn tilde_path_contracts_home() {
        if let Ok(home) = std::env::var("HOME") {
            let p = std::path::Path::new(&home).join(".skill").join("settings.json");
            let result = tilde_path(&p);
            assert!(result.starts_with("~/"), "expected '~/...' got '{result}'");
        }
    }

    #[test]
    fn tilde_path_leaves_non_home_path_unchanged() {
        let p = std::path::Path::new("/tmp/some/path.json");
        assert_eq!(tilde_path(p), "/tmp/some/path.json");
    }

    // ── OpenBciConfig defaults ────────────────────────────────────────────────

    #[test]
    fn default_openbci_config_scan_timeout_is_10() {
        assert_eq!(OpenBciConfig::default().scan_timeout_secs, 10);
    }

    #[test]
    fn default_openbci_config_wifi_port_is_3000() {
        assert_eq!(OpenBciConfig::default().wifi_local_port, 3000);
    }

    #[test]
    fn default_openbci_config_has_empty_serial_port() {
        assert!(OpenBciConfig::default().serial_port.is_empty());
    }

    // ── new_profile_id ────────────────────────────────────────────────────────

    #[test]
    fn new_profile_id_starts_with_cal_prefix() {
        let id = new_profile_id();
        assert!(id.starts_with("cal_"), "expected 'cal_...', got '{id}'");
    }

    #[test]
    fn new_profile_id_is_unique_across_calls() {
        let a = new_profile_id();
        let b = new_profile_id();
        assert!(a.starts_with("cal_"));
        assert!(b.starts_with("cal_"));
        assert!(!a.is_empty());
        assert!(!b.is_empty());
    }
}
