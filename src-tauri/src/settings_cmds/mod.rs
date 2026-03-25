// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Device, filter, EEG model, app-settings, autostart, and update-interval Tauri commands.

pub mod activity_cmds;
pub mod device_cmds;
pub mod dnd_cmds;
pub mod hook_cmds;
pub mod screenshot_cmds;
pub mod skills_cmds;

// Re-export extracted commands so `use settings_cmds::X` keeps working in lib.rs.
pub use activity_cmds::{
    get_active_window, get_active_window_tracking, get_input_activity_tracking, get_input_buckets,
    get_last_input_activity, get_recent_active_windows, get_recent_input_activity,
    set_active_window_tracking, set_input_activity_tracking,
};
pub use device_cmds::{
    cancel_retry, forget_device, get_device_capabilities, get_devices, get_status,
    get_supported_companies, pair_device, retry_connect, set_preferred_device,
};
pub use dnd_cmds::{
    get_dnd_active, get_dnd_config, get_dnd_status, list_focus_modes, pick_ref_wav_file,
    set_dnd_config, test_dnd,
};
pub use hook_cmds::{
    get_hook_log, get_hook_log_count, get_hook_statuses, get_hooks, sanitize_hook, set_hooks,
    suggest_hook_distances, suggest_hook_keywords, HookDistanceSuggestion,
};
pub use screenshot_cmds::{
    check_ocr_models_ready, download_ocr_models, estimate_screenshot_reembed,
    get_screenshot_config, get_screenshot_metrics, get_screenshots_around, get_screenshots_dir,
    rebuild_screenshot_embeddings, search_screenshots_by_image, search_screenshots_by_text,
    search_screenshots_by_vector, set_screenshot_config,
};
pub use skills_cmds::{
    get_disabled_skills, get_skills_last_sync, get_skills_license, get_skills_refresh_interval,
    get_skills_sync_on_launch, list_skills, set_disabled_skills, set_skills_refresh_interval,
    set_skills_sync_on_launch, sync_skills_now,
};

use crate::MutexExt;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

use crate::autostart;
use crate::eeg_embeddings::download_hf_weights;
use crate::settings::{save_umap_config, UmapUserConfig};
use crate::settings::{DeviceApiConfig, NeuttsConfig, OpenBciConfig};
use crate::AppStateExt;
use crate::{
    constants::{EMBEDDING_OVERLAP_MAX_SECS, EMBEDDING_OVERLAP_MIN_SECS, LOG_CONFIG_FILE},
    emit_status, mutate_and_save, save_settings, skill_dir, AppState, EegPacket, ImuPacket,
    PpgPacket,
};
use skill_eeg::eeg_bands::BandSnapshot;
use skill_eeg::eeg_filter::{FilterConfig, PowerlineFreq};
use skill_eeg::eeg_model_config::{save_model_config, EegModelConfig, EegModelStatus};
// Hook keyword suggestions — moved to hook_cmds.rs

// ── EEG / PPG / IMU subscriptions ────────────────────────────────────────────

#[tauri::command]
pub fn subscribe_eeg(
    on_event: tauri::ipc::Channel<EegPacket>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().eeg_channel = Some(on_event);
}

#[tauri::command]
pub fn subscribe_ppg(
    on_event: tauri::ipc::Channel<PpgPacket>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().ppg_channel = Some(on_event);
}

#[tauri::command]
pub fn subscribe_imu(
    on_event: tauri::ipc::Channel<ImuPacket>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
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
        "both" => "both",
        _ => "csv",
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
pub fn get_log_config(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> crate::skill_log::LogConfig {
    state.lock_or_recover().logger.get_config()
}

#[tauri::command]
pub fn set_log_config(
    config: crate::skill_log::LogConfig,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
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
    let (skill_dir, backend_changed) = {
        let mut s = state.lock_or_recover();
        let changed = s.embedding.model_config.model_backend != config.model_backend
            || s.embedding.model_config.luna_variant != config.luna_variant;
        let dir = s.skill_dir.clone();
        s.embedding.model_config = config.clone();
        // When the model backend or variant changes, signal the embed worker to
        // reload so it picks up the new encoder without an app restart.
        if changed {
            s.embedding
                .encoder_reload_requested
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        (dir, changed)
    };
    // Persist outside the lock — disk I/O must not block other subsystems.
    save_model_config(&skill_dir, &config);
    let _ = backend_changed;
}

#[tauri::command]
pub fn get_eeg_model_status(state: tauri::State<'_, Mutex<Box<AppState>>>) -> EegModelStatus {
    state
        .lock_or_recover()
        .embedding
        .model_status
        .lock_or_recover()
        .clone()
}

/// Spawn a background thread that downloads EEG model weights from HuggingFace Hub.
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
    use skill_eeg::eeg_model_config::ExgModelBackend;
    use std::sync::atomic::Ordering;

    let s = state.lock_or_recover();
    let config = s.embedding.model_config.clone();
    let model_status = s.embedding.model_status.clone();
    let cancel = s.embedding.download_cancel.clone();
    let reload_requested = s.embedding.encoder_reload_requested.clone();
    let logger = s.logger.clone();
    drop(s); // release AppState lock before spawning

    let (hf_repo, weights_file, config_file) = match config.model_backend {
        ExgModelBackend::Zuna => (
            config.hf_repo.clone(),
            skill_constants::ZUNA_WEIGHTS_FILE.to_string(),
            skill_constants::ZUNA_CONFIG_FILE.to_string(),
        ),
        ExgModelBackend::Luna => (
            config.luna_hf_repo.clone(),
            config.luna_weights_file().to_string(),
            skill_constants::LUNA_CONFIG_FILE.to_string(),
        ),
    };

    // Clear any previous cancellation so the new attempt actually runs.
    cancel.store(false, Ordering::Relaxed);

    std::thread::Builder::new()
        .name("hf-download".into())
        .spawn(move || {
            // mark_needs_restart=false: instead of prompting restart we signal
            // the embed worker to reload in-place via encoder_reload_requested.
            if download_hf_weights(
                &hf_repo,
                &weights_file,
                &config_file,
                &model_status,
                &cancel,
                false,
                &logger,
            )
            .is_some()
            {
                // Signal the running embed worker (if any) to exit and respawn
                // so it picks up the freshly downloaded encoder immediately.
                reload_requested.store(true, Ordering::Relaxed);
                skill_log!(
                    logger,
                    "embedder",
                    "weights downloaded — signalling embed worker for in-place reload"
                );
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
pub fn set_language(
    language: String,
    app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    mutate_and_save(&app, |s| s.ui.language = language);
}

#[tauri::command]
pub fn get_accent_color(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    state.lock_or_recover().ui.accent_color.clone()
}

#[tauri::command]
pub fn set_accent_color(
    accent: String,
    app: AppHandle,
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
pub fn set_goal_notified_date(
    date: String,
    app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
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
    let Some(csv_path) =
        crate::history_cmds::find_session_csv_for_timestamp(&skill_dir, timestamp_utc)
    else {
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
    days: Option<u32>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<(String, u32)>, String> {
    let skill_dir = skill_dir(&state);
    tokio::task::spawn_blocking(move || get_daily_recording_mins_sync(&skill_dir, days))
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
        if !dir.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(std::result::Result::ok) {
            let p = entry.path();
            let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !((fname.starts_with("exg_") || fname.starts_with("muse_"))
                && fname.ends_with(".json"))
            {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let start = meta["session_start_utc"].as_u64().unwrap_or(0);
            let end = meta["session_end_utc"].as_u64().unwrap_or(start);
            *total += (end.saturating_sub(start) / 60) as u32;
        }
    }

    results.reverse();
    results
        .into_iter()
        .map(|(d, m)| (format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8]), m))
        .collect()
}

pub(crate) fn unix_to_ymd(ts: u64) -> (u32, u32, u32) {
    let days = ts / 86400;
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
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
    host: String,
    port: u16,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    if host != "127.0.0.1" && host != "0.0.0.0" {
        return Err(format!(
            "invalid host '{host}': must be '127.0.0.1' or '0.0.0.0'"
        ));
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
pub fn set_api_token(token: String, app: AppHandle, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    state.lock_or_recover().api_token = token;
    crate::save_settings(&app);
}

// ── Autostart (launch at login) ────────────────────────────────────────────────

/// Returns `true` if the app is registered to launch at login.
///
/// Reads the OS-level registration directly (plist / .desktop / registry).
#[tauri::command]
pub fn get_autostart_enabled(app: AppHandle) -> bool {
    let name = app
        .config()
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
pub fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let name = app
        .config()
        .product_name
        .as_deref()
        .unwrap_or("skill")
        .to_lowercase();
    autostart::set_enabled(&name, enabled).map_err(|e| e.to_string())
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
    secs: u64,
    app: AppHandle,
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
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().openbci_config = config;
    crate::save_settings(&app);
}

/// Return the current device API credential configuration.
///
/// `DeviceApiConfig` uses `#[serde(skip_serializing)]` on secret fields so
/// they are never written to the JSON settings file on disk.  That also means
/// a plain `-> DeviceApiConfig` return would omit the secrets from the Tauri
/// IPC response.  We return a `serde_json::Value` instead so the frontend
/// receives the full credentials loaded from the system keychain.
#[tauri::command]
pub fn get_device_api_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> serde_json::Value {
    let c = state.lock_or_recover().device_api_config.clone();
    serde_json::json!({
        "emotiv_client_id":     c.emotiv_client_id,
        "emotiv_client_secret": c.emotiv_client_secret,
        "idun_api_token":       c.idun_api_token,
    })
}

/// Persist device API credential configuration.
#[tauri::command]
pub fn set_device_api_config(
    config: DeviceApiConfig,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().device_api_config = config;
    crate::save_settings(&app);
}

/// Return the current scanner backend configuration.
#[tauri::command]
pub fn get_scanner_config(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> skill_settings::ScannerConfig {
    state.lock_or_recover().scanner_config.clone()
}

/// Return the current Cortex WebSocket connection state.
#[tauri::command]
pub fn get_cortex_ws_state(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    state.lock_or_recover().cortex_ws_state.clone()
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
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
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
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
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
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
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
pub fn get_sleep_config(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> crate::settings::SleepConfig {
    state.lock_or_recover().sleep_config.clone()
}

#[tauri::command]
pub fn set_sleep_config(
    config: crate::settings::SleepConfig,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().sleep_config = config;
    crate::save_settings(&app);
}

// ── LLM ───────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_llm_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> crate::settings::LlmConfig {
    {
        let __a = state.lock_or_recover().llm.clone();
        let __r = __a.lock_or_recover().config.clone();
        __r
    }
}

/// Update the LLM server configuration and persist it to `settings.json`.
///
/// Most model/runtime changes still require restart, but the built-in tool
/// allow-list is pushed into the running LLM chat state immediately.
#[tauri::command]
pub fn set_llm_config(
    config: crate::settings::LlmConfig,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    #[cfg(feature = "llm")]
    let cell = {
        let s = state.lock_or_recover();
        let __llm_arc = s.llm.clone();
        let mut llm = __llm_arc.lock_or_recover();
        llm.config = config.clone();
        llm.state_cell.clone()
    };
    #[cfg(not(feature = "llm"))]
    {
        let s = state.lock_or_recover();
        {
            let __a = s.llm.clone();
            __a.lock_or_recover().config = config.clone();
        }
    }

    #[cfg(feature = "llm")]
    if let Some(server) = cell.lock_or_recover().clone() {
        // Preserve the runtime-only skill_api_port when updating tools config.
        let prev_port = server.allowed_tools.lock_or_recover().skill_api_port;
        let mut new_tools = config.tools.clone();
        new_tools.skill_api_port = prev_port;
        *server.allowed_tools.lock_or_recover() = new_tools;
    }

    // Live-update the web cache configuration.
    skill_tools::web_cache::update_config(config.tools.web_cache.clone());

    save_settings(&app);
}

// ── Web cache commands ────────────────────────────────────────────────────────

#[tauri::command]
pub fn web_cache_stats() -> serde_json::Value {
    match skill_tools::web_cache::global() {
        Some(cache) => serde_json::to_value(cache.stats()).unwrap_or_default(),
        None => serde_json::json!({"total_entries": 0, "expired_entries": 0, "total_bytes": 0}),
    }
}

#[tauri::command]
pub fn web_cache_list() -> Vec<serde_json::Value> {
    match skill_tools::web_cache::global() {
        Some(cache) => cache
            .list_entries()
            .into_iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect(),
        None => Vec::new(),
    }
}

#[tauri::command]
pub fn web_cache_clear() -> u64 {
    if let Some(cache) = skill_tools::web_cache::global() {
        let stats = cache.stats();
        cache.clear();
        stats.total_entries
    } else {
        0
    }
}

#[tauri::command]
pub fn web_cache_remove_domain(domain: String) -> u64 {
    match skill_tools::web_cache::global() {
        Some(cache) => cache.remove_by_domain(&domain),
        None => 0,
    }
}

#[tauri::command]
pub fn web_cache_remove_entry(key: String) -> bool {
    match skill_tools::web_cache::global() {
        Some(cache) => cache.remove_entry(&key),
        None => false,
    }
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
/// and session CSV files exist.
#[tauri::command]
pub fn estimate_reembed(state: tauri::State<'_, Mutex<Box<AppState>>>) -> ReembedEstimate {
    let s = state.lock_or_recover();
    let skill_dir = s.skill_dir.clone();
    drop(s);

    let is_session_data = |fname: &str| -> bool {
        let has_prefix = fname.starts_with("exg_") || fname.starts_with("muse_");
        if !has_prefix {
            return false;
        }
        let is_primary =
            !fname.contains("_ppg") && !fname.contains("_metrics") && !fname.contains("_imu");
        is_primary && (fname.ends_with(".csv") || fname.ends_with(".parquet"))
    };

    let mut total_sessions = 0usize;
    let mut date_dirs = 0usize;

    if let Ok(entries) = std::fs::read_dir(&skill_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.len() == 8 && name.chars().all(|c| c.is_ascii_digit()) {
                let day_dir = entry.path();
                // Count unique session stems (prefer parquet, don't double-count).
                let mut stems = std::collections::HashSet::new();
                if let Ok(rd) = std::fs::read_dir(&day_dir) {
                    for f in rd.flatten() {
                        let fname = f.file_name().to_string_lossy().to_string();
                        if is_session_data(&fname) {
                            let stem = f
                                .path()
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_string();
                            stems.insert(stem);
                        }
                    }
                }
                if !stems.is_empty() {
                    total_sessions += stems.len();
                    date_dirs += 1;
                }
            }
        }
    }

    ReembedEstimate {
        date_dirs,
        total_sessions,
    }
}

/// Lightweight estimate for the re-embed UI.
#[derive(serde::Serialize, Clone)]
pub struct ReembedEstimate {
    pub date_dirs: usize,
    pub total_sessions: usize,
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
    let skill_dir = s.skill_dir.clone();
    let config = s.embedding.model_config.clone();
    let logger = s.logger.clone();
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
    done: usize,
    total: usize,
    date: String,
    status: String,
}

/// Read raw EEG samples from a Parquet file.
///
/// Returns `(timestamps, per_channel_samples)` where each channel buffer
/// contains f32 µV values in recording order.
fn read_eeg_parquet(
    path: &std::path::Path,
    n_ch: usize,
) -> Result<(Vec<f64>, Vec<Vec<f32>>), String> {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    let file = std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|e| format!("parquet reader {}: {e}", path.display()))?;
    let reader = builder
        .build()
        .map_err(|e| format!("parquet build {}: {e}", path.display()))?;

    let mut timestamps: Vec<f64> = Vec::new();
    let mut ch_bufs: Vec<Vec<f32>> = vec![Vec::new(); n_ch];

    for batch_result in reader {
        let batch = batch_result.map_err(|e| format!("parquet batch: {e}"))?;
        let n_rows = batch.num_rows();
        let n_cols = batch.num_columns();

        // Column 0 = timestamp_s.
        if let Some(ts_col) = batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow_array::Float64Array>()
        {
            for i in 0..n_rows {
                timestamps.push(ts_col.value(i));
            }
        }

        // Columns 1..=n_ch = EEG channels.
        for (ch, buf) in ch_bufs.iter_mut().enumerate() {
            let col_idx = ch + 1;
            if col_idx >= n_cols {
                break;
            }
            if let Some(col) = batch
                .column(col_idx)
                .as_any()
                .downcast_ref::<arrow_array::Float64Array>()
            {
                for i in 0..n_rows {
                    buf.push(col.value(i) as f32);
                }
            }
        }
    }

    Ok((timestamps, ch_bufs))
}

fn reembed_worker(
    skill_dir: std::path::PathBuf,
    config: EegModelConfig,
    logger: std::sync::Arc<crate::skill_log::SkillLogger>,
    app: AppHandle,
) {
    use burn::backend::Wgpu;
    use fast_hnsw::{distance::Cosine, labeled::LabeledIndex, Builder};
    use std::time::Instant;

    let emit = |p: &ReembedProgress| {
        let _ = app.emit("reembed-progress", p);
    };

    emit(&ReembedProgress {
        done: 0,
        total: 0,
        date: String::new(),
        status: "loading_encoder".into(),
    });

    // ── Load encoder ──────────────────────────────────────────────────────
    skill_exg::configure_cubecl_cache(&skill_dir);

    // Manual wgpu init with env-controlled validation (same as embed worker).
    let device = {
        use burn::backend::wgpu::graphics::GraphicsApi;
        use burn::backend::wgpu::{
            graphics::AutoGraphicsApi, init_device, RuntimeOptions, WgpuSetup,
        };

        let backend = AutoGraphicsApi::backend();
        let setup = pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: backend.into(),
                flags: wgpu::InstanceFlags::from_build_config().with_env(),
                ..Default::default()
            });
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: None,
                })
                .await
                .expect("[reembed] no GPU adapter found");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("skill-reembed"),
                    required_features: adapter.features(),
                    required_limits: adapter.limits(),
                    ..Default::default()
                })
                .await
                .expect("[reembed] failed to create wgpu device");
            WgpuSetup {
                instance,
                adapter,
                device,
                queue,
                backend,
            }
        });
        init_device(setup, RuntimeOptions::default())
    };

    let backend = &config.model_backend;
    let backend_str = backend.as_str();
    let weights = match backend {
        skill_eeg::eeg_model_config::ExgModelBackend::Zuna => {
            skill_exg::resolve_hf_weights(&config.hf_repo)
        }
        skill_eeg::eeg_model_config::ExgModelBackend::Luna => {
            skill_exg::resolve_luna_weights(&config.luna_hf_repo, config.luna_weights_file())
        }
    };

    let Some((w_path, c_path)) = weights else {
        skill_log!(
            logger,
            "reembed",
            "weights not found for {} — aborting",
            backend
        );
        emit(&ReembedProgress {
            done: 0,
            total: 0,
            date: String::new(),
            status: "error_no_weights".into(),
        });
        return;
    };

    enum Enc {
        Zuna(Box<zuna_rs::ZunaEncoder<Wgpu>>),
        Luna(Box<luna_rs::LunaEncoder<Wgpu>>),
    }

    let encoder = match backend {
        skill_eeg::eeg_model_config::ExgModelBackend::Zuna => {
            match zuna_rs::ZunaEncoder::<Wgpu>::load(&c_path, &w_path, device.clone()) {
                Ok((e, ms)) => {
                    skill_log!(logger, "reembed", "ZUNA encoder loaded ({ms:.0}ms)");
                    Enc::Zuna(Box::new(e))
                }
                Err(e) => {
                    skill_log!(logger, "reembed", "ZUNA load failed: {e:#}");
                    emit(&ReembedProgress {
                        done: 0,
                        total: 0,
                        date: String::new(),
                        status: "error_load".into(),
                    });
                    return;
                }
            }
        }
        skill_eeg::eeg_model_config::ExgModelBackend::Luna => {
            let cfg_path =
                crate::eeg_embeddings::luna_variant_config_path(&c_path, &config.luna_variant);
            match luna_rs::LunaEncoder::<Wgpu>::load(&cfg_path, &w_path, device.clone()) {
                Ok((e, ms)) => {
                    skill_log!(logger, "reembed", "LUNA encoder loaded ({ms:.0}ms)");
                    Enc::Luna(Box::new(e))
                }
                Err(e) => {
                    skill_log!(logger, "reembed", "LUNA load failed: {e:#}");
                    emit(&ReembedProgress {
                        done: 0,
                        total: 0,
                        date: String::new(),
                        status: "error_load".into(),
                    });
                    return;
                }
            }
        }
    };

    // ── Collect date directories with session CSVs ────────────────────────
    let mut dates: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&skill_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.len() == 8 && name.chars().all(|c| c.is_ascii_digit()) {
                dates.push(name);
            }
        }
    }
    dates.sort();

    // Collect session data files across all dates.
    // Prefer .parquet over .csv when both exist for the same session.
    let mut session_files: Vec<(String, std::path::PathBuf)> = Vec::new(); // (date, data_path)
    for date in &dates {
        let day_dir = skill_dir.join(date);
        if let Ok(rd) = std::fs::read_dir(&day_dir) {
            // Collect all EEG data files in this day.
            let mut csv_files: Vec<std::path::PathBuf> = Vec::new();
            let mut pq_files: Vec<std::path::PathBuf> = Vec::new();
            for entry in rd.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                let is_primary = (fname.starts_with("exg_") || fname.starts_with("muse_"))
                    && !fname.contains("_ppg")
                    && !fname.contains("_metrics")
                    && !fname.contains("_imu");
                if !is_primary {
                    continue;
                }
                if fname.ends_with(".parquet") {
                    pq_files.push(entry.path());
                } else if fname.ends_with(".csv") {
                    csv_files.push(entry.path());
                }
            }
            // For each session, prefer parquet if it exists, else CSV.
            // Match by stem: exg_12345.parquet vs exg_12345.csv.
            let mut used_stems: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for pq in &pq_files {
                let stem = pq
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                used_stems.insert(stem);
                session_files.push((date.clone(), pq.clone()));
            }
            for csv in &csv_files {
                let stem = csv
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                if !used_stems.contains(&stem) {
                    session_files.push((date.clone(), csv.clone()));
                }
            }
        }
    }
    session_files.sort_by(|a, b| a.1.cmp(&b.1));

    let total = session_files.len();
    let mut done = 0usize;
    let mut total_embeddings = 0usize;

    skill_log!(
        logger,
        "reembed",
        "starting re-embed: {} sessions across {} dates with model={}",
        total,
        dates.len(),
        backend
    );

    let epoch_samples = crate::constants::EMBEDDING_EPOCH_SAMPLES;
    let epoch_secs = crate::constants::EMBEDDING_EPOCH_SECS;
    let data_cfg = zuna_rs::config::DataConfig::default();
    let pos_overrides = std::collections::HashMap::<String, [f32; 3]>::new();

    // ── Process each session CSV ──────────────────────────────────────────
    for (date, csv_path) in &session_files {
        emit(&ReembedProgress {
            done,
            total,
            date: date.clone(),
            status: "processing".into(),
        });

        // Read session metadata (channel names, sample rate).
        let meta_path = csv_path.with_extension("json");
        let (channel_names, sample_rate): (Vec<String>, f64) = if meta_path.exists() {
            match std::fs::read_to_string(&meta_path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            {
                Some(meta) => {
                    let names: Vec<String> = meta["channels"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_else(|| {
                            vec!["TP9".into(), "AF7".into(), "AF8".into(), "TP10".into()]
                        });
                    let sr = meta["sample_rate_hz"].as_f64().unwrap_or(256.0);
                    (names, sr)
                }
                None => (
                    vec!["TP9".into(), "AF7".into(), "AF8".into(), "TP10".into()],
                    256.0,
                ),
            }
        } else {
            (
                vec!["TP9".into(), "AF7".into(), "AF8".into(), "TP10".into()],
                256.0,
            )
        };

        let n_ch = channel_names.len();
        // Native samples per epoch at device sample rate.
        let native_epoch = (sample_rate * epoch_secs as f64).round() as usize;

        // Read EEG samples: columns = [timestamp_s, ch1, ch2, ...].
        // Supports both CSV and Parquet formats.
        let mut ch_bufs: Vec<Vec<f32>> = vec![Vec::new(); n_ch];
        let mut timestamps: Vec<f64> = Vec::new();

        let is_parquet = csv_path.extension().and_then(|e| e.to_str()) == Some("parquet");

        if is_parquet {
            // ── Read Parquet ──────────────────────────────────────────────
            match read_eeg_parquet(csv_path, n_ch) {
                Ok((ts, bufs)) => {
                    timestamps = ts;
                    ch_bufs = bufs;
                }
                Err(e) => {
                    skill_log!(
                        logger,
                        "reembed",
                        "cannot read parquet {}: {e}",
                        csv_path.display()
                    );
                    done += 1;
                    continue;
                }
            }
        } else {
            // ── Read CSV ──────────────────────────────────────────────────
            let Ok(mut rdr) = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_path(csv_path)
            else {
                skill_log!(logger, "reembed", "cannot open CSV {}", csv_path.display());
                done += 1;
                continue;
            };
            for result in rdr.records() {
                let Ok(record) = result else { continue };
                let ts: f64 = record.get(0).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                timestamps.push(ts);
                for (ch, buf) in ch_bufs.iter_mut().enumerate() {
                    let v: f32 = record
                        .get(ch + 1)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0.0);
                    buf.push(v);
                }
            }
        }

        let total_samples = ch_bufs.first().map(std::vec::Vec::len).unwrap_or(0);
        if total_samples < native_epoch || n_ch == 0 {
            skill_log!(
                logger,
                "reembed",
                "{}: skipping {} — only {} samples (need {})",
                date,
                csv_path.file_name().unwrap_or_default().to_string_lossy(),
                total_samples,
                native_epoch
            );
            done += 1;
            continue;
        }

        // ── Open SQLite for this day ──────────────────────────────────────
        let day_dir = skill_dir.join(date);
        let db_path = day_dir.join(crate::constants::SQLITE_FILE);
        let Ok(conn) = rusqlite::Connection::open(&db_path) else {
            done += 1;
            continue;
        };
        skill_data::util::init_wal_pragmas(&conn);
        let _ = conn.execute("ALTER TABLE embeddings ADD COLUMN model_backend TEXT", []);
        let _ = conn.execute("ALTER TABLE embeddings ADD COLUMN embed_speed_ms REAL", []);

        // Build HNSW for the new model from scratch.
        let mut idx: LabeledIndex<Cosine, i64> = Builder::new()
            .m(config.hnsw_m)
            .ef_construction(config.hnsw_ef_construction)
            .build_labeled(Cosine);

        // Delete existing embeddings for this model backend in this day
        // (we're replacing them with fresh ones from raw data).
        let _ = conn.execute(
            "DELETE FROM embeddings WHERE model_backend = ?1",
            rusqlite::params![backend_str],
        );

        // ── Chunk into epochs and embed ───────────────────────────────────
        let hop = native_epoch / 2; // 50% overlap
        let mut offset = 0usize;
        let mut day_embeddings = 0usize;

        while offset + native_epoch <= total_samples {
            // Extract epoch per channel.
            let epoch_raw: Vec<Vec<f32>> = (0..n_ch)
                .map(|ch| ch_bufs[ch][offset..offset + native_epoch].to_vec())
                .collect();

            // Derive timestamp for this epoch centre.
            let epoch_centre_idx = offset + native_epoch / 2;
            let ts_secs = timestamps.get(epoch_centre_idx).copied().unwrap_or(0.0);
            // Convert to YYYYMMDDHHmmss.
            let epoch_ts = skill_exg::yyyymmddhhmmss_utc(); // fallback
            let _ = epoch_ts; // suppress unused
                              // Approximate timestamp from the CSV timestamp_s column.
            let ts_i64 = {
                let dt = chrono::DateTime::from_timestamp(ts_secs as i64, 0).unwrap_or_default();
                use chrono::Datelike;
                use chrono::Timelike;
                let d = dt;
                (d.year() as i64) * 10_000_000_000
                    + (d.month() as i64) * 100_000_000
                    + (d.day() as i64) * 1_000_000
                    + (d.hour() as i64) * 10_000
                    + (d.minute() as i64) * 100
                    + (d.second() as i64)
            };

            // Resample to EMBEDDING_EPOCH_SAMPLES if needed.
            let epoch_resampled: Vec<Vec<f32>> = epoch_raw
                .iter()
                .map(|ch| {
                    if ch.len() == epoch_samples {
                        ch.clone()
                    } else {
                        crate::eeg_embeddings::resample_linear(ch, epoch_samples)
                    }
                })
                .collect();

            // Run inference.
            let infer_start = Instant::now();
            let emb_result: Result<Vec<f32>, String> = (|| -> Result<Vec<f32>, String> {
                match &encoder {
                    Enc::Zuna(zuna_enc) => {
                        // Pad to EEG_CHANNELS.
                        let mut padded: Vec<Vec<f32>> = epoch_resampled;
                        while padded.len() < crate::constants::EEG_CHANNELS {
                            padded.push(vec![0.0f32; epoch_samples]);
                        }
                        let flat: Vec<f32> = padded.iter().flatten().copied().collect();
                        let array = ndarray::Array2::from_shape_vec(
                            (crate::constants::EEG_CHANNELS, epoch_samples),
                            flat,
                        )
                        .map_err(|e| format!("array: {e}"))?;

                        let mut pad_names: Vec<String> = channel_names.clone();
                        while pad_names.len() < crate::constants::EEG_CHANNELS {
                            pad_names.push(format!("_pad{}", pad_names.len()));
                        }
                        let ch_refs: Vec<&str> =
                            pad_names.iter().map(std::string::String::as_str).collect();

                        let mut batches = zuna_rs::load_from_named_tensor::<Wgpu>(
                            array,
                            &ch_refs,
                            sample_rate as f32,
                            config.data_norm,
                            &pos_overrides,
                            &data_cfg,
                            &device,
                        )
                        .map_err(|e| format!("preprocess: {e:#}"))?;
                        if batches.is_empty() {
                            return Err("empty batch".into());
                        }

                        let mut epochs = zuna_enc
                            .encode_batches(batches.drain(..1).collect())
                            .map_err(|e| format!("encode: {e:#}"))?;
                        let ep = epochs.pop().ok_or("no epoch")?;
                        let dim = ep.output_dim();
                        let n_tok = ep.n_tokens();
                        if dim == 0 || n_tok == 0 {
                            return Err("zero dim".into());
                        }

                        let mut mean = vec![0f32; dim];
                        for tok in ep.embeddings.chunks(dim) {
                            for (i, &v) in tok.iter().enumerate() {
                                mean[i] += v;
                            }
                        }
                        let s = 1.0 / n_tok as f32;
                        for v in &mut mean {
                            *v *= s;
                        }
                        Ok(mean)
                    }
                    Enc::Luna(luna_enc) => {
                        let flat: Vec<f32> = epoch_resampled.iter().flatten().copied().collect();
                        let ch_refs: Vec<&str> = channel_names
                            .iter()
                            .map(std::string::String::as_str)
                            .collect();
                        let batch = luna_rs::build_batch_named::<Wgpu>(
                            flat,
                            &ch_refs,
                            epoch_samples,
                            &device,
                        );
                        let result = luna_enc
                            .run_batch(&batch)
                            .map_err(|e| format!("luna: {e:#}"))?;
                        let out = &result.output;
                        let shape = &result.shape;
                        if out.is_empty() {
                            return Err("empty luna output".into());
                        }

                        if shape.len() == 2 {
                            let c = shape[0];
                            let t = shape[1];
                            let ps = luna_enc.model_cfg.patch_size;
                            let np = t / ps.max(1);
                            if np == 0 || c == 0 {
                                return Err("zero patches".into());
                            }
                            let patch_means: Vec<f32> = (0..np)
                                .map(|p| {
                                    let mut sum = 0f64;
                                    let count = (c * ps) as f64;
                                    for ch_idx in 0..c {
                                        for si in 0..ps {
                                            let idx = ch_idx * t + p * ps + si;
                                            if idx < out.len() {
                                                sum += out[idx] as f64;
                                            }
                                        }
                                    }
                                    (sum / count) as f32
                                })
                                .collect();
                            Ok(patch_means)
                        } else {
                            Ok(out.clone())
                        }
                    }
                }
            })();
            let embed_ms = infer_start.elapsed().as_secs_f64() * 1000.0;

            if let Ok(emb) = emb_result {
                let hnsw_id = idx.insert(emb.clone(), ts_i64) as i64;
                let blob: Vec<u8> = emb.iter().flat_map(|v| v.to_le_bytes()).collect();

                let _ = conn.execute(
                    "INSERT INTO embeddings \
                     (timestamp, device_id, device_name, hnsw_id, eeg_embedding, \
                      model_backend, embed_speed_ms) \
                     VALUES (?1, NULL, NULL, ?2, ?3, ?4, ?5)",
                    rusqlite::params![ts_i64, hnsw_id, blob, backend_str, embed_ms],
                );
                day_embeddings += 1;
                total_embeddings += 1;
            }

            offset += hop;
        }

        // Save the HNSW for this day/model.
        let hnsw_file = crate::constants::hnsw_index_file_for(backend_str);
        let hnsw_path = day_dir.join(&hnsw_file);
        if let Err(e) = idx.save(&hnsw_path) {
            skill_log!(logger, "reembed", "{}: HNSW save error: {e}", date);
        } else if day_embeddings > 0 {
            skill_log!(
                logger,
                "reembed",
                "{}: {} embeddings → {} (model={})",
                date,
                day_embeddings,
                hnsw_file,
                backend_str
            );
        }

        // Also tag any remaining legacy NULL rows in this day as 'zuna'.
        let _ = conn.execute(
            "UPDATE embeddings SET model_backend = 'zuna' WHERE model_backend IS NULL AND eeg_embedding IS NOT NULL",
            [],
        );

        done += 1;
    }

    // ── Rebuild global HNSW for the target model ──────────────────────────
    skill_log!(
        logger,
        "reembed",
        "rebuilding global HNSW for model={backend_str}…"
    );
    crate::global_eeg_index::rebuild_from_scratch_for(&skill_dir, backend_str);

    emit(&ReembedProgress {
        done: total,
        total,
        date: String::new(),
        status: "complete".into(),
    });
    skill_log!(
        logger,
        "reembed",
        "re-embed complete: {} embeddings from {} sessions (model={})",
        total_embeddings,
        total,
        backend_str
    );
}
