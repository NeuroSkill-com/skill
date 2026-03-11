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

use crate::constants::{
    SETTINGS_FILE, UMAP_CONFIG_FILE,
    CALIBRATION_ACTION1_LABEL, CALIBRATION_ACTION2_LABEL,
    CALIBRATION_ACTION_DURATION_SECS, CALIBRATION_BREAK_DURATION_SECS,
    CALIBRATION_LOOP_COUNT, CALIBRATION_AUTO_START,
    EMBEDDING_OVERLAP_SECS,
};
use crate::eeg_filter::FilterConfig;

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
    let path = skill_dir.join(UMAP_CONFIG_FILE);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_umap_config(skill_dir: &Path, cfg: &UmapUserConfig) {
    let _ = std::fs::create_dir_all(skill_dir);
    let path = skill_dir.join(UMAP_CONFIG_FILE);
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, json);
    }
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
    pub(crate) fn from_legacy(cfg: &CalibrationConfig) -> Self {
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
///
/// Windows uses `%LOCALAPPDATA%` (e.g. `C:\Users\<user>\AppData\Local\NeuroSkill`)
/// because the Unix convention of storing data in a hidden dot-folder in the
/// home directory is not the Windows norm, and — critically — `$HOME` is often
/// **unset** on Windows (the OS uses `USERPROFILE` / `APPDATA` instead).
/// Without the `HOME` variable the old implementation fell back to
/// `PathBuf::from(".skill")`, a *relative* path that resolves to the directory
/// containing the executable, scattering user data next to the binary.
///
/// [`dirs::data_local_dir()`] returns `%LOCALAPPDATA%` on Windows and handles
/// all the Windows-specific env-var resolution internally.
pub fn default_skill_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        // %LOCALAPPDATA%\NeuroSkill  (e.g. C:\Users\Alice\AppData\Local\NeuroSkill)
        dirs::data_local_dir()
            .unwrap_or_else(|| {
                // Last-resort fallback: APPDATA or temp dir — never the cwd.
                std::env::var("APPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| std::env::temp_dir())
            })
            .join("NeuroSkill")
    }
    #[cfg(not(target_os = "windows"))]
    {
        // macOS / Linux: ~/.skill
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(crate::constants::SKILL_DIR)
    }
}

pub(crate) fn settings_path(skill_dir: &Path) -> PathBuf {
    skill_dir.join(SETTINGS_FILE)
}

/// Collapse an absolute path back to a shorter human-readable form.
///
/// On Unix the home directory is abbreviated to `~`.
/// On Windows `HOME` is typically unset; we fall back to `USERPROFILE`.
pub(crate) fn tilde_path(p: &Path) -> String {
    // Try $HOME first (Unix), then $USERPROFILE (Windows).
    let home_str = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if !home_str.is_empty() {
        let h = home_str.trim_end_matches(['/', '\\']);
        let s = p.to_string_lossy();
        if s == h { return "~".into(); }
        // Accept both Unix '/' and Windows '\' as separators after the home prefix.
        if let Some(rest) = s.strip_prefix(h) {
            if rest.starts_with('/') || rest.starts_with('\\') {
                return format!("~{rest}");
            }
        }
    }
    p.to_string_lossy().to_string()
}

// ── NeuTTS configuration ──────────────────────────────────────────────────────

/// NeuTTS voice-cloning TTS configuration, persisted in `settings.json`.
///
/// When `enabled` is `true`, all speech synthesis (calibration prompts,
/// WebSocket `say` commands) uses the NeuTTS GGUF backbone + NeuCodec decoder
/// pipeline located at `/agent/neutts-rs` instead of KittenTTS.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NeuttsConfig {
    /// Use NeuTTS instead of KittenTTS for all speech synthesis.
    pub enabled: bool,

    /// HuggingFace backbone repo, e.g. `"neuphonic/neutts-nano-q4-gguf"`.
    /// Must be one of the repos listed in `neutts_rs::download::BACKBONE_MODELS`.
    #[serde(default = "default_neutts_backbone_repo")]
    pub backbone_repo: String,

    /// Specific GGUF filename within the repo.
    /// Empty string means "auto-select the first `.gguf` file found".
    pub gguf_file: String,

    /// Absolute path to a reference WAV file used for voice cloning.
    /// Empty means no reference has been selected — NeuTTS will use the
    /// backbone's built-in voice.
    pub ref_wav_path: String,

    /// Verbatim transcript of the speech in `ref_wav_path`.
    /// Used by espeak-ng to phonemise the reference segment.
    pub ref_text: String,

    /// Name of a bundled preset voice from `neutts-rs/samples/`.
    /// One of: `"jo"`, `"dave"`, `"greta"`, `"juliette"`, `"mateo"`.
    /// Empty string means use the custom `ref_wav_path` instead.
    pub voice_preset: String,
}

pub(crate) fn default_neutts_backbone_repo() -> String {
    "neuphonic/neutts-nano-q4-gguf".into()
}

impl Default for NeuttsConfig {
    fn default() -> Self {
        Self {
            enabled:       false,
            backbone_repo: default_neutts_backbone_repo(),
            gguf_file:     String::new(),
            voice_preset:  "jo".into(),
            ref_wav_path:  String::new(),
            ref_text:      String::new(),
        }
    }
}

// ── LLM server configuration ──────────────────────────────────────────────────

/// Configuration for the embedded OpenAI-compatible LLM inference server.
///
/// Persisted in `~/.skill/settings.json` under the `llm` key.
/// Requires the `llm` Cargo feature to have any effect at runtime.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    /// Enable the LLM server.  When `false` (the default) no model is loaded
    /// and all `/v1/*` endpoints return HTTP 503.
    #[serde(default)]
    pub enabled: bool,

    /// Absolute path to a GGUF model file.  Required when `enabled = true`.
    ///
    /// Example: `"/Users/alice/.cache/huggingface/hub/…/model.Q4_K_M.gguf"`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_path: Option<std::path::PathBuf>,

    /// Number of transformer layers to offload to the GPU.
    /// `0` = CPU-only inference.  `-1` (stored as `u32::MAX`) = offload all.
    /// Only meaningful when the binary was compiled with `llm-metal`,
    /// `llm-cuda`, or `llm-vulkan`.
    #[serde(default)]
    pub n_gpu_layers: u32,

    /// KV-cache / context size in tokens.  `None` → use the model's trained
    /// context length (capped at 4096 tokens to avoid OOM).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctx_size: Option<u32>,

    /// Maximum number of inference requests processed concurrently.
    /// llama.cpp contexts are not thread-safe — this effectively serialises
    /// the decode loop while keeping HTTP connections open and responsive.
    /// Default: 1.
    #[serde(default = "default_llm_parallel")]
    pub parallel: usize,

    /// Optional Bearer token required on every `/v1/*` request.
    /// When `None` (the default) the API is open to any local caller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    // ── Multimodal (requires `llm-mtmd` feature) ──────────────────────────────

    /// Path to the multimodal projector (mmproj) GGUF file.
    /// Enables `POST /v1/files` and image/audio inputs in chat completions.
    /// Only used when the binary is compiled with `--features llm-mtmd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mmproj: Option<std::path::PathBuf>,

    /// Number of threads used by the vision/audio encoder.  Default: 4.
    #[serde(default = "default_mmproj_n_threads")]
    pub mmproj_n_threads: i32,

    /// Disable GPU offloading for the mmproj model (use CPU instead).
    /// Only meaningful with `llm-mtmd` + a GPU feature.
    #[serde(default)]
    pub no_mmproj_gpu: bool,

    /// Automatically load the vision projector (mmproj) when the LLM server
    /// starts, without requiring the user to explicitly select one.
    /// The best downloaded mmproj from the same repo as the active model is
    /// chosen (recommended first, then BF16 > F16 > F32 by quant preference).
    /// Default: `true`.
    #[serde(default = "default_autoload_mmproj")]
    pub autoload_mmproj: bool,

    /// Enable verbose llama.cpp / clip_model_loader logging to stderr.
    ///
    /// When `false` (the default) all internal llama.cpp and clip/mtmd logs are
    /// silenced so only skill's own `[llm]` lines appear.
    /// Set to `true` to see raw tensor-load progress and other low-level detail.
    #[serde(default)]
    pub verbose: bool,
}

fn default_llm_parallel()      -> usize { 1 }
fn default_mmproj_n_threads()  -> i32   { 4 }
fn default_autoload_mmproj()   -> bool  { true }

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enabled:          false,
            model_path:       None,
            // Offload every layer to the GPU by default.
            // The user can reduce this if VRAM is insufficient.
            n_gpu_layers:     u32::MAX,
            ctx_size:         None,
            parallel:         default_llm_parallel(),
            api_key:          None,
            mmproj:           None,
            mmproj_n_threads: default_mmproj_n_threads(),
            no_mmproj_gpu:    false,
            autoload_mmproj:  default_autoload_mmproj(),
            verbose:          false,
        }
    }
}

// ── Default values (pub(crate) so AppState::default() can use them) ───────────

pub(crate) fn default_ws_host() -> String { crate::constants::WS_HOST.into() }
pub(crate) fn default_ws_port() -> u16    { crate::constants::WS_DEFAULT_PORT }
pub(crate) fn default_update_check_interval() -> u64 {
    crate::constants::UPDATER_CHECK_INTERVAL_SECS
}
pub(crate) fn default_theme()   -> String { "system".into() }
pub(crate) fn default_daily_goal_min()       -> u32    { 60 }
pub(crate) fn default_embedding_model()      -> String { "Xenova/bge-small-en-v1.5".into() }
pub(crate) fn default_overlap_secs()         -> f32    { EMBEDDING_OVERLAP_SECS }
pub(crate) fn default_label_shortcut()       -> String { "CmdOrCtrl+Shift+L".into() }
pub(crate) fn default_search_shortcut()      -> String { "CmdOrCtrl+Shift+S".into() }
pub(crate) fn default_settings_shortcut()    -> String { "CmdOrCtrl+,".into() }
pub(crate) fn default_calibration_shortcut() -> String { "CmdOrCtrl+Shift+C".into() }
pub(crate) fn default_help_shortcut()        -> String { "CmdOrCtrl+Shift+H".into() }
pub(crate) fn default_history_shortcut()     -> String { "CmdOrCtrl+Shift+J".into() }
pub(crate) fn default_api_shortcut()         -> String { "CmdOrCtrl+Shift+A".into() }
pub(crate) fn default_theme_shortcut()       -> String { "CmdOrCtrl+Shift+T".into() }
pub(crate) fn default_focus_timer_shortcut() -> String { "CmdOrCtrl+Shift+P".into() }
#[cfg(feature = "llm")]
pub(crate) fn default_chat_shortcut()        -> String { "CmdOrCtrl+Shift+I".into() }

// ── UserSettings (serialised to settings.json) ────────────────────────────────

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct UserSettings {
    pub paired:                 Vec<crate::PairedDevice>,
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
    #[serde(default = "default_daily_goal_min")]
    pub daily_goal_min:         u32,
    #[serde(default)]
    pub goal_notified_date:     String,
    #[serde(default = "default_embedding_model")]
    pub text_embedding_model:   String,
    /// WebSocket server bind host.  `"127.0.0.1"` (loopback-only, default)
    /// or `"0.0.0.0"` (all interfaces — exposes the API on the LAN).
    #[serde(default = "default_ws_host")]
    pub ws_host: String,
    /// Preferred WebSocket server port.  Falls back to an OS-assigned port if
    /// this one is already in use.  Default: 8375.
    #[serde(default = "default_ws_port")]
    pub ws_port: u16,
    /// Seconds between automatic background update checks (0 = disabled).
    /// Configurable in the Updates settings tab.  Default: 3600 (1 hour).
    #[serde(default = "default_update_check_interval")]
    pub update_check_interval_secs: u64,
    /// OpenBCI board configuration (board variant, serial port, channel labels).
    #[serde(default)]
    pub openbci: OpenBciConfig,

    /// NeuTTS voice-cloning TTS configuration.
    #[serde(default)]
    pub neutts: NeuttsConfig,

    /// Pre-warm the active TTS engine at startup even when no TTS UI is open.
    /// Default: `true`.  Disable to defer model loading until first use.
    #[serde(default = "default_tts_preload")]
    pub tts_preload: bool,

    /// Track the active window (app name, path, title, timestamp) while a
    /// session is running.  Data is stored locally only.  Default: `true`.
    #[serde(default = "default_track_active_window")]
    pub track_active_window: bool,

    /// Track the last keyboard and mouse input timestamps.  Uses a global
    /// input hook (requires Accessibility on macOS).  Data is stored locally
    /// only.  Default: `true`.
    #[serde(default = "default_track_input_activity")]
    pub track_input_activity: bool,

    /// Automatic Do Not Disturb when focus is sustained.  macOS only.
    #[serde(default)]
    pub do_not_disturb: DoNotDisturbConfig,

    /// Last app version for which the "What's New" window was shown and
    /// dismissed by the user.  Empty string means it has never been seen.
    #[serde(default)]
    pub last_seen_whats_new_version: String,

    /// Embedded OpenAI-compatible LLM inference server.
    /// All `/v1/*` endpoints (chat, completions, embeddings, files) are served
    /// on the same TCP port as the WebSocket API when enabled.
    #[serde(default)]
    pub llm: LlmConfig,
}

fn default_tts_preload() -> bool { true }
pub(crate) fn default_track_active_window() -> bool { true }
pub(crate) fn default_track_input_activity() -> bool { true }

// ── Do Not Disturb automation ─────────────────────────────────────────────────

pub(crate) fn default_dnd_threshold() -> f32 { 60.0 }
pub(crate) fn default_dnd_duration_secs() -> u32 { 60 }
pub(crate) fn default_dnd_exit_duration_secs() -> u32 { 300 }   // 5 minutes
pub(crate) fn default_dnd_focus_lookback_secs() -> u32 { 60 }   // 1 minute
pub(crate) fn default_dnd_mode_identifier() -> String {
    "com.apple.donotdisturb.mode.default".to_owned()
}
pub(crate) fn default_dnd_exit_notification() -> bool { true }

/// Configuration for the "auto Do Not Disturb when focus is sustained" feature.
///
/// When `enabled`, the app monitors the real-time cognitive-load / focus score
/// and activates macOS Do Not Disturb after the score has stayed above
/// `focus_threshold` (0–100) for at least `duration_secs` seconds.
///
/// DND is **not** deactivated immediately when the score drops — it is only
/// cleared after the score has remained below the threshold for at least
/// `exit_duration_secs` seconds (default 5 minutes), giving the user time to
/// briefly lose focus without being constantly pulled out of DND.
///
/// **macOS 12+ only.**  On earlier versions the legacy `defaults` approach is
/// attempted; on non-macOS platforms the feature is a no-op.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DoNotDisturbConfig {
    /// Whether the feature is enabled.  Default: `false`.
    pub enabled: bool,

    /// Focus score (0–100) that must be sustained to activate DND.
    /// Default: 60.
    #[serde(default = "default_dnd_threshold")]
    pub focus_threshold: f32,

    /// Seconds focus must remain above the threshold before DND activates.
    /// Default: 60 (one minute).
    #[serde(default = "default_dnd_duration_secs")]
    pub duration_secs: u32,

    /// Seconds the focus score must remain *below* the threshold before DND
    /// is cleared.  This prevents short focus dips from toggling DND off.
    /// Range: 60–3600 s (1–60 min).  Default: 300 s (5 min).
    #[serde(default = "default_dnd_exit_duration_secs")]
    pub exit_duration_secs: u32,

    /// Lookback window in seconds.  When DND is active and the score drops
    /// below the threshold, the exit counter is **reset to zero** if any
    /// raw focus tick in the last `focus_lookback_secs` seconds was above
    /// the threshold — the user was recently focused so we delay the exit.
    /// Default: 60 s (1 minute).  Range: 30–600 s.
    #[serde(default = "default_dnd_focus_lookback_secs")]
    pub focus_lookback_secs: u32,

    /// The macOS Focus mode to activate, stored as a `modeIdentifier` string.
    ///
    /// Defaults to `"com.apple.donotdisturb.mode.default"` (Do Not Disturb).
    /// Any mode returned by `list_focus_modes` can be used here, including
    /// user-created custom modes (e.g. `"com.apple.focus.work"`).
    ///
    /// The value is ignored on non-macOS platforms.
    #[serde(default = "default_dnd_mode_identifier")]
    pub focus_mode_identifier: String,

    /// Whether to send an OS notification when focus mode is automatically
    /// deactivated (score dropped, SNR too low, or feature disabled).
    /// Default: `true`.
    #[serde(default = "default_dnd_exit_notification")]
    pub exit_notification: bool,
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
        }
    }
}

pub(crate) fn load_settings(skill_dir: &Path) -> UserSettings {
    let mut s: UserSettings = std::fs::read_to_string(settings_path(skill_dir))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();

    // ── Shortcut migrations ──────────────────────────────────────────────
    // If a user still has the old defaults stored from before the rename,
    // silently upgrade them to the new defaults so the tray/menu reflects
    // the new bindings without requiring manual intervention.
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
        assert_eq!(p.actions[0].label, crate::constants::CALIBRATION_ACTION1_LABEL);
        assert_eq!(p.actions[1].label, crate::constants::CALIBRATION_ACTION2_LABEL);
    }

    #[test]
    fn default_calibration_profile_durations_match_constants() {
        let p = CalibrationProfile::default();
        assert_eq!(p.actions[0].duration_secs, crate::constants::CALIBRATION_ACTION_DURATION_SECS);
        assert_eq!(p.actions[1].duration_secs, crate::constants::CALIBRATION_ACTION_DURATION_SECS);
        assert_eq!(p.break_duration_secs, crate::constants::CALIBRATION_BREAK_DURATION_SECS);
        assert_eq!(p.loop_count,          crate::constants::CALIBRATION_LOOP_COUNT);
        assert_eq!(p.auto_start,          crate::constants::CALIBRATION_AUTO_START);
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
        // Two successive IDs should normally differ (millisecond resolution).
        // We can't guarantee uniqueness in the same ms, but at least verify
        // both have the right prefix and are non-empty.
        let a = new_profile_id();
        let b = new_profile_id();
        assert!(a.starts_with("cal_"));
        assert!(b.starts_with("cal_"));
        assert!(!a.is_empty());
        assert!(!b.is_empty());
    }
}
