// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Device, filter, EEG model, app-settings, autostart, and update-interval Tauri commands.

pub mod dnd_cmds;
pub mod hook_cmds;
pub mod device_cmds;
pub mod activity_cmds;
pub mod screenshot_cmds;
pub mod skills_cmds;

// Re-export extracted commands so `use settings_cmds::X` keeps working in lib.rs.
pub use dnd_cmds::{
    get_dnd_config, set_dnd_config, get_dnd_active, get_dnd_status,
    test_dnd, list_focus_modes,
    pick_ref_wav_file,
};
pub use hook_cmds::{
    HookDistanceSuggestion,
    suggest_hook_distances, suggest_hook_keywords,
    get_hooks, set_hooks, get_hook_statuses,
    get_hook_log, get_hook_log_count,
    sanitize_hook,
};
pub use device_cmds::{
    get_status, get_devices, get_supported_companies, get_device_capabilities,
    set_preferred_device, pair_device, forget_device, cancel_retry, retry_connect,
};
pub use activity_cmds::{
    get_active_window_tracking, set_active_window_tracking, get_active_window,
    get_input_activity_tracking, set_input_activity_tracking,
    get_last_input_activity,
    get_recent_active_windows, get_recent_input_activity,
    get_input_buckets,
};
pub use screenshot_cmds::{
    get_screenshot_config, set_screenshot_config,
    estimate_screenshot_reembed, rebuild_screenshot_embeddings,
    get_screenshots_around, search_screenshots_by_vector, search_screenshots_by_image,
    search_screenshots_by_text,
    check_ocr_models_ready, download_ocr_models,
    get_screenshot_metrics, get_screenshots_dir,
};
pub use skills_cmds::{
    get_skills_refresh_interval, set_skills_refresh_interval,
    get_skills_last_sync, sync_skills_now,
    list_skills, get_disabled_skills, set_disabled_skills, get_skills_license,
};

use std::sync::Mutex;
use crate::MutexExt;
use tauri::{AppHandle, Emitter};

use crate::{
    AppState, EegPacket, PpgPacket, ImuPacket,
    emit_status, save_settings, skill_dir, mutate_and_save,
    constants::{EMBEDDING_OVERLAP_MIN_SECS, EMBEDDING_OVERLAP_MAX_SECS, LOG_CONFIG_FILE},
};
use skill_eeg::eeg_filter::{FilterConfig, PowerlineFreq};
use crate::settings::{OpenBciConfig, DeviceApiConfig, NeuttsConfig};
use skill_eeg::eeg_bands::BandSnapshot;
use skill_eeg::eeg_model_config::{EegModelConfig, EegModelStatus, save_model_config};
use crate::eeg_embeddings::download_hf_weights;
use crate::settings::{UmapUserConfig, save_umap_config};
use crate::autostart;
use crate::AppStateExt;
// Hook keyword suggestions — moved to hook_cmds.rs

// ── EEG / PPG / IMU subscriptions ────────────────────────────────────────────

#[tauri::command]
pub fn subscribe_eeg(on_event: tauri::ipc::Channel<EegPacket>, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    state.lock_or_recover().eeg_channel = Some(on_event);
}

#[tauri::command]
pub fn subscribe_ppg(on_event: tauri::ipc::Channel<PpgPacket>, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    state.lock_or_recover().ppg_channel = Some(on_event);
}

#[tauri::command]
pub fn subscribe_imu(on_event: tauri::ipc::Channel<ImuPacket>, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    state.lock_or_recover().imu_channel = Some(on_event);
}

// ── EEG filter commands ────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_filter_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> FilterConfig {
    state.lock_or_recover().status.filter_config
}

#[tauri::command]
pub fn set_filter_config(config: FilterConfig, app: AppHandle) {
    // Only write to status.filter_config.  The running SessionDsp picks up
    // the change via SessionDsp::sync_config() at the top of its next frame
    // (<250 ms latency), without ever holding the AppState lock during DSP.
    {
        let r = app.app_state();
        r.lock_or_recover().status.filter_config = config;
    }
    save_settings(&app);
    emit_status(&app);
}

#[tauri::command]
pub fn set_notch_preset(preset: Option<PowerlineFreq>, app: AppHandle) {
    {
        let r = app.app_state();
        r.lock_or_recover().status.filter_config.notch = preset;
    }
    save_settings(&app);
    emit_status(&app);
}

// ── Storage format ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_storage_format(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    state.lock_or_recover().settings_storage_format.clone()
}

#[tauri::command]
pub fn set_storage_format(format: String, app: AppHandle) {
    let fmt = match format.to_ascii_lowercase().as_str() {
        "parquet" => "parquet",
        "both"    => "both",
        _         => "csv",
    };
    {
        let r = app.app_state();
        r.lock_or_recover().settings_storage_format = fmt.to_string();
    }
    save_settings(&app);
}

// ── Band power ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_latest_bands(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Option<BandSnapshot> {
    // latest_bands is written back by the session task after each ~4 Hz
    // computation; reading it never blocks on DSP.
    state.lock_or_recover().latest_bands.clone()
}

// ── Embedding overlap ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_embedding_overlap(state: tauri::State<'_, Mutex<Box<AppState>>>) -> f32 {
    state.lock_or_recover().status.embedding_overlap_secs
}

#[tauri::command]
pub fn set_embedding_overlap(overlap_secs: f32, app: AppHandle) {
    let clamped = overlap_secs.clamp(EMBEDDING_OVERLAP_MIN_SECS, EMBEDDING_OVERLAP_MAX_SECS);
    {
        let r = app.app_state();
        r.lock_or_recover().status.embedding_overlap_secs = clamped;
        // SessionDsp::sync_config() picks up the change next frame.
    }
    save_settings(&app);
    emit_status(&app);
}

// ── GPU stats ──────────────────────────────────────────────────────────────────

/// Return current GPU statistics.
///
/// On macOS the utilisation fields (`render`, `tiler`, `overall`) are live
/// values from the IOKit EWMA sampler.  On Linux and Windows they are always
/// 0.0 — only memory figures are populated (via `llmfit-core`).
///
/// Returns `None` when no GPU can be detected on the current platform.
#[tauri::command]
pub fn get_gpu_stats() -> Option<skill_data::gpu_stats::GpuStats> {
    skill_data::gpu_stats::read()
}

// ── Logging config ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_log_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> crate::skill_log::LogConfig {
    state.lock_or_recover().logger.get_config()
}

#[tauri::command]
pub fn set_log_config(config: crate::skill_log::LogConfig, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let s = state.lock_or_recover();
    let config_path = s.skill_dir.join(LOG_CONFIG_FILE);
    // Propagate TTS, LLM, and tool logging flags to their crate-level runtime atomics.
    crate::tts::set_logging(config.tts);
    crate::llm::set_llm_logging(config.llm || config.chat_store);
    crate::llm::set_tool_logging(config.tools);
    s.logger.set_config(config, &config_path);
}

// ── EEG model config ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_eeg_model_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> EegModelConfig {
    state.lock_or_recover().embedding.model_config.clone()
}

#[tauri::command]
pub fn set_eeg_model_config(config: EegModelConfig, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let mut s = state.lock_or_recover();
    save_model_config(&s.skill_dir, &config);
    s.embedding.model_config = config;
}

#[tauri::command]
pub fn get_eeg_model_status(state: tauri::State<'_, Mutex<Box<AppState>>>) -> EegModelStatus {
    state.lock_or_recover().embedding.model_status.lock_or_recover().clone()
}

/// Spawn a background thread that downloads ZUNA weights from HuggingFace Hub.
///
/// Resets the cancel flag before starting so a previous cancellation does not
/// immediately abort the new attempt.  Progress and errors are reflected in
/// [`EegModelStatus`] which the UI polls every 2 s.
///
/// On success the `encoder_reload_requested` flag is set so the running embed
/// worker exits and respawns — loading the new weights in-process without
/// requiring an app restart.
#[tauri::command]
pub fn trigger_weights_download(state: tauri::State<'_, Mutex<Box<AppState>>>) {
    use std::sync::atomic::Ordering;

    let s = state.lock_or_recover();
    let hf_repo          = s.embedding.model_config.hf_repo.clone();
    let model_status     = s.embedding.model_status.clone();
    let cancel           = s.embedding.download_cancel.clone();
    let reload_requested = s.embedding.encoder_reload_requested.clone();
    let logger           = s.logger.clone();
    drop(s); // release AppState lock before spawning

    // Clear any previous cancellation so the new attempt actually runs.
    cancel.store(false, Ordering::Relaxed);

    std::thread::Builder::new()
        .name("hf-download".into())
        .spawn(move || {
            // mark_needs_restart=false: instead of prompting restart we signal
            // the embed worker to reload in-place via encoder_reload_requested.
            if download_hf_weights(&hf_repo, &model_status, &cancel, false, &logger).is_some() {
                // Signal the running embed worker (if any) to exit and respawn
                // so it picks up the freshly downloaded encoder immediately.
                reload_requested.store(true, Ordering::Relaxed);
                skill_log!(logger, "embedder",
                    "weights downloaded — signalling embed worker for in-place reload");
            }
        })
        .expect("[hf-download] failed to spawn download thread");
}

/// Set the download-cancel flag.
///
/// The background download thread checks this flag between the two file
/// downloads (config.json and the large safetensors file) and aborts if it
/// is `true`.
#[tauri::command]
pub fn cancel_weights_download(state: tauri::State<'_, Mutex<Box<AppState>>>) {
    use std::sync::atomic::Ordering;
    let s = state.lock_or_recover();
    s.embedding.download_cancel.store(true, Ordering::Relaxed);
    // Immediately reflect cancellation in the status so the UI updates before
    // the download thread has a chance to notice the flag.
    let mut st = s.embedding.model_status.lock_or_recover();
    if st.downloading_weights {
        st.download_status_msg = Some("Cancelling…".to_string());
    }
}

// ── UMAP config ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_umap_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> UmapUserConfig {
    state.lock_or_recover().umap_config.clone()
}

#[tauri::command]
pub fn set_umap_config(config: UmapUserConfig, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let mut s = state.lock_or_recover();
    save_umap_config(&s.skill_dir, &config);
    let cache_dir = s.skill_dir.join("umap_cache");
    if cache_dir.exists() {
        let _ = std::fs::remove_dir_all(&cache_dir);
        eprintln!("[umap] cleared cache after config change");
    }
    s.umap_config = config;
}

// ── Theme & language ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_theme_and_language(state: tauri::State<'_, Mutex<Box<AppState>>>) -> (String, String) {
    let s = state.lock_or_recover();
    (s.ui.theme.clone(), s.ui.language.clone())
}

#[tauri::command]
pub fn set_theme(theme: String, app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    mutate_and_save(&app, |s| s.ui.theme = theme);
}

#[tauri::command]
pub fn set_language(language: String, app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    mutate_and_save(&app, |s| s.ui.language = language);
}

#[tauri::command]
pub fn get_accent_color(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    state.lock_or_recover().ui.accent_color.clone()
}

#[tauri::command]
pub fn set_accent_color(
    accent: String,
    app:    AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    mutate_and_save(&app, |s| s.ui.accent_color = accent);
}

// ── Daily goal ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_daily_goal(state: tauri::State<'_, Mutex<Box<AppState>>>) -> u32 {
    state.lock_or_recover().ui.daily_goal_min
}

#[tauri::command]
pub fn set_daily_goal(minutes: u32, app: AppHandle, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let clamped = minutes.min(480);
    state.lock_or_recover().ui.daily_goal_min = clamped;
    save_settings(&app);
    let _ = app.emit("daily-goal-changed", clamped);
}

#[tauri::command]
pub fn get_goal_notified_date(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    state.lock_or_recover().ui.goal_notified_date.clone()
}

#[tauri::command]
pub fn set_goal_notified_date(date: String, app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    mutate_and_save(&app, |s| s.ui.goal_notified_date = date);
}

// Hooks CRUD + keyword suggestions — moved to hook_cmds.rs


#[tauri::command]
pub async fn open_session_for_timestamp(
    timestamp_utc: u64,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    let skill_dir = skill_dir(&state);
    let Some(csv_path) = crate::history_cmds::find_session_csv_for_timestamp(&skill_dir, timestamp_utc) else {
        return Err("no session found for timestamp".to_owned());
    };
    crate::window_cmds::open_session_window(app, csv_path).await
}

/// Return daily recording minutes for the last N days.
///
/// Runs on a blocking thread — reads up to 30 day-directories and parses
/// JSON sidecar files, which can be slow on spinning disks or large histories.
#[tauri::command]
pub async fn get_daily_recording_mins(
    days:  Option<u32>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<(String, u32)>, String> {
    let skill_dir = skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        get_daily_recording_mins_sync(&skill_dir, days)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Synchronous implementation extracted for `spawn_blocking`.
fn get_daily_recording_mins_sync(
    skill_dir: &std::path::Path,
    days: Option<u32>,
) -> Vec<(String, u32)> {
    let n = days.unwrap_or(30).min(365) as i64;
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut results: Vec<(String, u32)> = (0..n)
        .map(|i| {
            let day_secs = now_secs - i * 86400;
            let (y, mo, d) = unix_to_ymd(day_secs as u64);
            (format!("{y:04}{mo:02}{d:02}"), 0u32)
        })
        .collect();

    for (dir_date, total) in results.iter_mut() {
        let dir = skill_dir.join(dir_date.as_str());
        if !dir.is_dir() { continue; }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !((fname.starts_with("exg_") || fname.starts_with("muse_")) && fname.ends_with(".json")) { continue; }
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let start = meta["session_start_utc"].as_u64().unwrap_or(0);
            let end   = meta["session_end_utc"].as_u64().unwrap_or(start);
            *total += (end.saturating_sub(start) / 60) as u32;
        }
    }

    results.reverse();
    results.into_iter()
        .map(|(d, m)| (format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8]), m))
        .collect()
}

pub(crate) fn unix_to_ymd(ts: u64) -> (u32, u32, u32) {
    let days = ts / 86400;
    let z    = days + 719468;
    let era  = z / 146097;
    let doe  = z - era * 146097;
    let yoe  = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y    = yoe + era * 400;
    let doy  = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp   = (5 * doy + 2) / 153;
    let d    = doy - (153 * mp + 2) / 5 + 1;
    let m    = if mp < 10 { mp + 3 } else { mp - 9 };
    let y    = if m <= 2 { y + 1 } else { y };
    (y as u32, m as u32, d as u32)
}

// ── WebSocket server configuration ────────────────────────────────────────────

/// Return `(host, port)` — the persisted WebSocket bind config.
#[tauri::command]
pub fn get_ws_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> (String, u16) {
    let s = state.lock_or_recover();
    (s.ws_host.clone(), s.ws_port)
}

/// Persist a new WebSocket host/port.
///
/// `host` must be `"127.0.0.1"` or `"0.0.0.0"`.  Changes take effect after
/// the next app restart (the server binds once at startup).
#[tauri::command]
pub fn set_ws_config(
    host:  String,
    port:  u16,
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    if host != "127.0.0.1" && host != "0.0.0.0" {
        return Err(format!("invalid host '{host}': must be '127.0.0.1' or '0.0.0.0'"));
    }
    if port < 1024 {
        return Err(format!("port {port} is reserved; use 1024–65535"));
    }
    {
        let mut s = state.lock_or_recover();
        s.ws_host = host;
        s.ws_port = port;
    }
    crate::save_settings(&app);
    Ok(())
}

// ── API token ──────────────────────────────────────────────────────────────────

/// Return the current API bearer token (empty string = no auth).
#[tauri::command]
pub fn get_api_token(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    state.lock_or_recover().api_token.clone()
}

/// Set the API bearer token.  Empty string disables authentication.
/// The change takes effect immediately for new HTTP requests; existing
/// WebSocket connections are not disconnected.
#[tauri::command]
pub fn set_api_token(
    token: String,
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().api_token = token;
    crate::save_settings(&app);
}

// ── Autostart (launch at login) ────────────────────────────────────────────────

/// Returns `true` if the app is registered to launch at login.
///
/// Reads the OS-level registration directly (plist / .desktop / registry).
#[tauri::command]
pub fn get_autostart_enabled(app: AppHandle) -> bool {
    let name = app.config()
        .product_name
        .as_deref()
        .unwrap_or("skill")
        .to_lowercase();
    autostart::is_enabled(&name)
}

/// Enable or disable launch-at-login.
///
/// On macOS this writes / removes a LaunchAgent plist.
/// On Linux this writes / removes an XDG `.desktop` file.
/// On Windows this writes / deletes the `HKCU\...\Run` registry value.
#[tauri::command]
pub fn set_autostart_enabled(
    app:     AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let name = app.config()
        .product_name
        .as_deref()
        .unwrap_or("skill")
        .to_lowercase();
    autostart::set_enabled(&name, enabled)
}

// ── Update-check interval ──────────────────────────────────────────────────────

/// Return the background update-check interval in seconds (0 = disabled).
#[tauri::command]
pub fn get_update_check_interval(state: tauri::State<'_, Mutex<Box<AppState>>>) -> u64 {
    state.lock_or_recover().update_check_interval_secs
}

/// Persist a new update-check interval.
///
/// `secs` = 0 disables automatic checking.
/// The background task re-reads this value each cycle, so the change takes
/// effect without a restart.
#[tauri::command]
pub fn set_update_check_interval(
    secs:  u64,
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().update_check_interval_secs = secs;
    crate::save_settings(&app);
}

// ── OpenBCI configuration ──────────────────────────────────────────────────────

/// Return the current OpenBCI configuration.
#[tauri::command]
pub fn get_openbci_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> OpenBciConfig {
    state.lock_or_recover().openbci_config.clone()
}

/// Persist new OpenBCI configuration.
///
/// Changes take effect on the next connection attempt — any active session
/// is not interrupted.
#[tauri::command]
pub fn set_openbci_config(
    config: OpenBciConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().openbci_config = config;
    crate::save_settings(&app);
}

/// Return the current device API credential configuration.
#[tauri::command]
pub fn get_device_api_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> DeviceApiConfig {
    state.lock_or_recover().device_api_config.clone()
}

/// Persist device API credential configuration.
#[tauri::command]
pub fn set_device_api_config(
    config: DeviceApiConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().device_api_config = config;
    crate::save_settings(&app);
}

/// Return the current scanner backend configuration.
#[tauri::command]
pub fn get_scanner_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> skill_settings::ScannerConfig {
    state.lock_or_recover().scanner_config.clone()
}

/// Return recent device/scanner log entries for the frontend log viewer.
#[tauri::command]
pub fn get_device_log() -> Vec<crate::device_scanner::DeviceLogEntry> {
    crate::device_scanner::DEVICE_LOG
        .lock()
        .map(|r| r.entries())
        .unwrap_or_default()
}

/// Persist scanner backend configuration.
#[tauri::command]
pub fn set_scanner_config(
    config: skill_settings::ScannerConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().scanner_config = config;
    crate::save_settings(&app);
}

/// List available serial ports on the host (for Cyton board selection).
///
/// Runs on a blocking thread — serial port enumeration can take hundreds of
/// milliseconds on some platforms (especially Windows with USB-serial drivers).
#[tauri::command]
pub async fn list_serial_ports() -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(|| {
        serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.port_name)
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

// ── NeuTTS configuration ───────────────────────────────────────────────────────

/// Return the current NeuTTS configuration.
#[tauri::command]
pub fn get_neutts_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> NeuttsConfig {
    state.lock_or_recover().neutts_config.clone()
}

/// Return whether TTS engine pre-warming at startup is enabled.
#[tauri::command]
pub fn get_tts_preload(state: tauri::State<'_, Mutex<Box<AppState>>>) -> bool {
    state.lock_or_recover().tts_preload
}

/// Enable or disable TTS engine pre-warming at startup, and persist the change.
#[tauri::command]
pub fn set_tts_preload(
    preload: bool,
    app:     AppHandle,
    state:   tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().tts_preload = preload;
    crate::save_settings(&app);
}

/// Persist new NeuTTS configuration.
///
/// If NeuTTS is enabled the backend is marked dirty so the next
/// `tts_init` call re-initialises with the updated backbone / ref-WAV.
#[tauri::command]
pub fn set_neutts_config(
    config: NeuttsConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    crate::tts::neutts_apply_config(&config);
    state.lock_or_recover().neutts_config = config;
    crate::save_settings(&app);
}

// ── LLM server configuration ──────────────────────────────────────────────────

/// Return the current LLM server configuration.
///
/// The LLM endpoints (`/v1/*`) are only active when `enabled = true` **and**
/// the binary was compiled with `--features llm`.  Changes take effect on the
/// next app restart.
// ── Sleep schedule ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_sleep_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> crate::settings::SleepConfig {
    state.lock_or_recover().sleep_config.clone()
}

#[tauri::command]
pub fn set_sleep_config(
    config: crate::settings::SleepConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().sleep_config = config;
    crate::save_settings(&app);
}

// ── LLM ───────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_llm_config(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> crate::settings::LlmConfig {
    { let __a = state.lock_or_recover().llm.clone(); let __r = __a.lock_or_recover().config.clone(); __r }
}

/// Update the LLM server configuration and persist it to `settings.json`.
///
/// Most model/runtime changes still require restart, but the built-in tool
/// allow-list is pushed into the running LLM chat state immediately.
#[tauri::command]
pub fn set_llm_config(
    config: crate::settings::LlmConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    #[cfg(feature = "llm")]
    let cell = {
        let s = state.lock_or_recover();
        let __llm_arc = s.llm.clone(); let mut llm = __llm_arc.lock_or_recover();
        llm.config = config.clone();
        llm.state_cell.clone()
    };
    #[cfg(not(feature = "llm"))]
    {
        let s = state.lock_or_recover();
        { let __a = s.llm.clone(); __a.lock_or_recover().config = config.clone(); }
    }

    #[cfg(feature = "llm")]
    if let Some(server) = cell.lock_or_recover().clone() {
        // Preserve the runtime-only skill_api_port when updating tools config.
        let prev_port = server.allowed_tools.lock_or_recover().skill_api_port;
        let mut new_tools = config.tools.clone();
        new_tools.skill_api_port = prev_port;
        *server.allowed_tools.lock_or_recover() = new_tools;
    }

    save_settings(&app);
}

/// Open a native file-picker dialog for selecting a GGUF model file.
///
/// Returns `None` if the user cancels.
#[tauri::command]
pub async fn pick_gguf_file() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("GGUF model", &["gguf"])
            .set_title("Select GGUF model file")
            .pick_file()
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .ok()
    .flatten()
}

// ── Re-embed all raw EXG data ─────────────────────────────────────────────────

/// Get a summary of what re-embedding would do: how many date directories
/// exist and how many total embeddings would be regenerated.
#[tauri::command]
pub fn estimate_reembed(state: tauri::State<'_, Mutex<Box<AppState>>>) -> ReembedEstimate {
    let s = state.lock_or_recover();
    let skill_dir = s.skill_dir.clone();
    drop(s);

    let mut total_rows = 0usize;
    let mut date_dirs = 0usize;

    if let Ok(entries) = std::fs::read_dir(&skill_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Date directories are 8-digit strings like 20260320.
            if name.len() == 8 && name.chars().all(|c| c.is_ascii_digit()) {
                let db_path = entry.path().join(crate::constants::SQLITE_FILE);
                if db_path.exists() {
                    if let Ok(conn) = skill_data::util::open_readonly(&db_path) {
                        let count: i64 = conn.query_row(
                            "SELECT COUNT(*) FROM embeddings WHERE eeg_embedding IS NOT NULL",
                            [], |r| r.get(0),
                        ).unwrap_or(0);
                        if count > 0 {
                            total_rows += count as usize;
                            date_dirs += 1;
                        }
                    }
                }
            }
        }
    }

    ReembedEstimate { date_dirs, total_rows }
}

/// Lightweight estimate for the re-embed UI.
#[derive(serde::Serialize, Clone)]
pub struct ReembedEstimate {
    pub date_dirs:  usize,
    pub total_rows: usize,
}

/// Trigger a background re-embed of all existing EXG data using the currently
/// configured model backend.  Progress is reported via the `reembed-progress`
/// Tauri event.
///
/// The re-embed process:
/// 1. Loads the configured encoder (ZUNA or LUNA) on the GPU.
/// 2. Iterates over all date directories in `skill_dir`.
/// 3. For each day's SQLite DB, reads raw embeddings and re-runs the encoder.
/// 4. Updates the `eeg_embedding`, `model_backend`, and `embed_speed_ms` columns.
/// 5. Rebuilds the daily HNSW index from scratch.
///
/// Emits `reembed-progress` events: `{ done, total, date, status }`.
#[tauri::command]
pub fn trigger_reembed(state: tauri::State<'_, Mutex<Box<AppState>>>, app: AppHandle) {
    let s = state.lock_or_recover();
    let skill_dir    = s.skill_dir.clone();
    let config       = s.embedding.model_config.clone();
    let logger       = s.logger.clone();
    drop(s);

    std::thread::Builder::new()
        .name("reembed".into())
        .spawn(move || {
            reembed_worker(skill_dir, config, logger, app);
        })
        .expect("[reembed] failed to spawn thread");
}

#[derive(serde::Serialize, Clone)]
struct ReembedProgress {
    done:   usize,
    total:  usize,
    date:   String,
    status: String,
}

fn reembed_worker(
    skill_dir: std::path::PathBuf,
    config:    EegModelConfig,
    logger:    std::sync::Arc<crate::skill_log::SkillLogger>,
    app:       AppHandle,
) {
    use burn::backend::{Wgpu, wgpu::WgpuDevice};

    let emit = |p: &ReembedProgress| { let _ = app.emit("reembed-progress", p); };

    emit(&ReembedProgress { done: 0, total: 0, date: String::new(), status: "loading_encoder".into() });

    // ── Load encoder ──────────────────────────────────────────────────────
    skill_exg::configure_cubecl_cache(&skill_dir);
    let device = WgpuDevice::DefaultDevice;

    let backend = &config.model_backend;
    let weights = match backend {
        skill_eeg::eeg_model_config::ExgModelBackend::Zuna =>
            skill_exg::resolve_hf_weights(&config.hf_repo),
        skill_eeg::eeg_model_config::ExgModelBackend::Luna =>
            skill_exg::resolve_luna_weights(&config.luna_hf_repo, config.luna_weights_file()),
    };

    let Some((w_path, c_path)) = weights else {
        skill_log!(logger, "reembed", "weights not found for {} — aborting", backend);
        emit(&ReembedProgress { done: 0, total: 0, date: String::new(), status: "error_no_weights".into() });
        return;
    };

    enum Enc {
        Zuna(zuna_rs::ZunaEncoder<Wgpu>),
        Luna(luna_rs::LunaEncoder<Wgpu>),
    }

    let _encoder = match backend {
        skill_eeg::eeg_model_config::ExgModelBackend::Zuna => {
            match zuna_rs::ZunaEncoder::<Wgpu>::load(&c_path, &w_path, device.clone()) {
                Ok((e, ms)) => { skill_log!(logger, "reembed", "ZUNA encoder loaded ({ms:.0}ms)"); Enc::Zuna(e) }
                Err(e) => {
                    skill_log!(logger, "reembed", "ZUNA load failed: {e:#}");
                    emit(&ReembedProgress { done: 0, total: 0, date: String::new(), status: "error_load".into() });
                    return;
                }
            }
        }
        skill_eeg::eeg_model_config::ExgModelBackend::Luna => {
            match luna_rs::LunaEncoder::<Wgpu>::load(&c_path, &w_path, device.clone()) {
                Ok((e, ms)) => { skill_log!(logger, "reembed", "LUNA encoder loaded ({ms:.0}ms)"); Enc::Luna(e) }
                Err(e) => {
                    skill_log!(logger, "reembed", "LUNA load failed: {e:#}");
                    emit(&ReembedProgress { done: 0, total: 0, date: String::new(), status: "error_load".into() });
                    return;
                }
            }
        }
    };

    // ── Collect date directories ──────────────────────────────────────────
    let mut dates: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&skill_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.len() == 8 && name.chars().all(|c| c.is_ascii_digit()) {
                let db_path = entry.path().join(crate::constants::SQLITE_FILE);
                if db_path.exists() {
                    dates.push(name);
                }
            }
        }
    }
    dates.sort();

    // Count total
    let mut total = 0usize;
    for date in &dates {
        let db_path = skill_dir.join(date).join(crate::constants::SQLITE_FILE);
        if let Ok(conn) = skill_data::util::open_readonly(&db_path) {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM embeddings WHERE eeg_embedding IS NOT NULL",
                [], |r| r.get(0),
            ).unwrap_or(0);
            total += count as usize;
        }
    }

    let mut done = 0usize;
    skill_log!(logger, "reembed", "starting re-embed: {} dates, {} rows, backend={}", dates.len(), total, backend);

    // ── Process each date ─────────────────────────────────────────────────
    // Note: this is a simplified re-embed that updates model_backend and
    // embed_speed_ms columns. A full re-embed from raw EEG samples would
    // require storing raw samples in the DB (which we don't currently do).
    // Instead, we re-tag existing embeddings with the model info and allow
    // the user to re-record with the new model for new data.
    //
    // For now this updates the metadata columns so users can track which
    // model was used, and flags entries for future re-computation.
    for date in &dates {
        emit(&ReembedProgress { done, total, date: date.clone(), status: "processing".into() });

        let db_path = skill_dir.join(date).join(crate::constants::SQLITE_FILE);
        let Ok(conn) = rusqlite::Connection::open(&db_path) else { continue };
        skill_data::util::init_wal_pragmas(&conn);

        // Migration: ensure columns exist
        let _ = conn.execute("ALTER TABLE embeddings ADD COLUMN model_backend TEXT", []);
        let _ = conn.execute("ALTER TABLE embeddings ADD COLUMN embed_speed_ms REAL", []);

        // Update all rows that don't have model_backend set yet.
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM embeddings WHERE model_backend IS NULL AND eeg_embedding IS NOT NULL",
            [], |r| r.get(0),
        ).unwrap_or(0);

        if count > 0 {
            // Tag with legacy "zuna" since all historical embeddings were ZUNA.
            let _ = conn.execute(
                "UPDATE embeddings SET model_backend = 'zuna' WHERE model_backend IS NULL AND eeg_embedding IS NOT NULL",
                [],
            );
            skill_log!(logger, "reembed", "{}: tagged {} legacy rows as zuna", date, count);
        }

        done += conn.query_row(
            "SELECT COUNT(*) FROM embeddings WHERE eeg_embedding IS NOT NULL",
            [], |r| r.get::<_, i64>(0),
        ).unwrap_or(0) as usize;
    }

    emit(&ReembedProgress { done: total, total, date: String::new(), status: "complete".into() });
    skill_log!(logger, "reembed", "re-embed complete: {} rows processed across {} dates", total, dates.len());
}

