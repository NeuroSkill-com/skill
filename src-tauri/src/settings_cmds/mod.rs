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
pub mod location_cmds;
pub mod lsl_cmds;

// Re-export extracted commands so `use settings_cmds::X` keeps working in lib.rs.
pub use activity_cmds::{get_input_buckets, get_recent_active_windows, get_recent_input_activity};
pub use device_cmds::{
    cancel_retry, forget_device, get_device_capabilities, get_supported_companies, retry_connect,
    set_preferred_device,
};
pub use dnd_cmds::pick_ref_wav_file;
pub use location_cmds::test_location;

use crate::MutexExt;
use std::sync::Mutex;
use tauri::AppHandle;

use crate::autostart;
use crate::AppStateExt;
use crate::{constants::LOG_CONFIG_FILE, emit_status, mutate_and_save, AppState};
use skill_eeg::eeg_filter::PowerlineFreq;

// ── EEG filter commands ────────────────────────────────────────────────────────

#[tauri::command]
pub fn set_notch_preset(preset: Option<PowerlineFreq>, app: AppHandle) {
    if crate::daemon_cmds::set_notch_preset(preset).is_ok() {
        {
            let r = app.app_state();
            r.lock_or_recover().status.filter_config.notch = preset;
        }
        emit_status(&app);
    }
}

// ── Embedding overlap ─────────────────────────────────────────────────────────

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

// ── EXG model catalog ─────────────────────────────────────────────────────────

// ── UMAP config ───────────────────────────────────────────────────────────────

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
pub fn get_accent_color(_state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    crate::daemon_cmds::fetch_accent_color().unwrap_or_default()
}

#[tauri::command]
pub fn set_accent_color(
    accent: String,
    app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    if crate::daemon_cmds::set_accent_color(accent.clone()).is_ok() {
        app.app_state().lock_or_recover().ui.accent_color = accent;
    }
}

// ── Daily goal ────────────────────────────────────────────────────────────────

// Hooks CRUD + keyword suggestions — moved to hook_cmds.rs

#[tauri::command]
pub async fn open_session_for_timestamp(
    timestamp_utc: u64,
    app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    let Some(csv_path) = crate::daemon_cmds::find_history_session(timestamp_utc)
        .ok()
        .flatten()
    else {
        return Err("no session found for timestamp".to_owned());
    };
    crate::window_cmds::open_session_window(app, csv_path).await
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
pub fn get_update_check_interval(_state: tauri::State<'_, Mutex<Box<AppState>>>) -> u64 {
    crate::daemon_cmds::fetch_update_check_interval().unwrap_or(0)
}

/// Persist a new update-check interval.
///
/// `secs` = 0 disables automatic checking.
/// The background task re-reads this value each cycle, so the change takes
/// effect without a restart.
#[tauri::command]
pub fn set_update_check_interval(
    secs: u64,
    _app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    if let Ok(persisted) = crate::daemon_cmds::set_update_check_interval(secs) {
        state.lock_or_recover().update_check_interval_secs = persisted;
    }
}

// ── Device config/status ──────────────────────────────────────────────────────

// ── NeuTTS configuration ───────────────────────────────────────────────────────

// ── File pickers ──────────────────────────────────────────────────────────────

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

/// Open a native file-picker dialog for selecting EXG model weights.
///
/// Returns `None` if the user cancels.
#[tauri::command]
pub async fn pick_exg_weights_file() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("Model weights", &["safetensors", "pth", "bin", "pt"])
            .set_title("Select EXG model weights")
            .pick_file()
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .ok()
    .flatten()
}

// ── Re-embed all raw EXG data ─────────────────────────────────────────────────
