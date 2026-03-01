// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Window open/close commands and calibration profile CRUD.

use std::sync::Mutex;
use crate::MutexExt;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    AppState, CalibrationProfile, CalibrationConfig, new_profile_id,
    save_settings, unix_secs, send_toast, ToastLevel,
    default_skill_dir, tilde_path, expand_tilde,
};
use crate::ws_server::WsBroadcaster;

// ── Bluetooth & utility windows ───────────────────────────────────────────────

#[tauri::command]
pub fn open_bt_settings() {
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.Bluetooth-Settings.extension").spawn(); }
    #[cfg(target_os = "windows")]
    { let _ = std::process::Command::new("rundll32")
        .args(["shell32.dll,Control_RunDLL","bthprops.cpl"]).spawn(); }
    #[cfg(target_os = "linux")]
    { let _ = std::process::Command::new("sh").arg("-c")
        .arg("gnome-control-center bluetooth 2>/dev/null || blueman-manager 2>/dev/null").spawn(); }
}

#[tauri::command]
pub async fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show(); let _ = win.set_focus(); return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App("settings".into()))
        .title("NeuroSkill™ – Settings")
        .inner_size(680.0, 720.0).min_inner_size(580.0, 560.0)
        .center().build().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_model_tab(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show(); let _ = win.set_focus();
        let _ = win.emit("switch-tab", "model");
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "settings",
        tauri::WebviewUrl::App("settings?tab=model".into()))
        .title("NeuroSkill™ – Model")
        .inner_size(680.0, 720.0).min_inner_size(580.0, 560.0)
        .center().build().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_updates_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show(); let _ = win.set_focus();
        let _ = win.emit("switch-tab", "updates");
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "settings",
        tauri::WebviewUrl::App("settings?tab=updates".into()))
        .title("NeuroSkill™ – Updates")
        .inner_size(680.0, 720.0).min_inner_size(580.0, 560.0)
        .center().build().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_help_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("help") {
        let _ = win.show(); let _ = win.set_focus(); return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "help", tauri::WebviewUrl::App("help".into()))
        .title("NeuroSkill™ – Help")
        .inner_size(680.0, 720.0).min_inner_size(600.0, 520.0)
        .center().build().map(|_| ()).map_err(|e| e.to_string())
}

// NOTE: open_history_window, open_compare_window, open_compare_window_with_sessions
// remain in lib.rs because they live inside the history-data section alongside
// SessionEntry, list_sessions, etc. which are not yet extracted.

#[tauri::command]
pub async fn open_session_window(app: AppHandle, csv_path: String) -> Result<(), String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    csv_path.hash(&mut h);
    let label = format!("session-{:x}", h.finish());
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.show(); let _ = win.set_focus(); return Ok(());
    }
    let encoded: String = csv_path.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
        _ => format!("%{:02X}", b),
    }).collect();
    tauri::WebviewWindowBuilder::new(&app, &label,
        tauri::WebviewUrl::App(format!("session?csv_path={encoded}").into()))
        .title("NeuroSkill™ – Session Detail")
        .inner_size(680.0, 700.0).min_inner_size(480.0, 400.0)
        .resizable(true).center().build().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_search_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("search") {
        let _ = win.show(); let _ = win.set_focus(); return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "search", tauri::WebviewUrl::App("search".into()))
        .title("EEG Search")
        .inner_size(1100.0, 820.0).min_inner_size(700.0, 560.0)
        .resizable(true).maximized(true).center().build().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn open_focus_timer_window_inner(
    app:       &AppHandle,
    autostart: bool,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("focus-timer") {
        let _ = win.show();
        let _ = win.set_focus();
        if autostart {
            let _ = app.emit("focus-timer-start", serde_json::json!({}));
        }
        return Ok(());
    }
    let url = if autostart { "focus-timer?autostart=1" } else { "focus-timer" };
    tauri::WebviewWindowBuilder::new(app, "focus-timer",
        tauri::WebviewUrl::App(url.into()))
        .title("Focus Timer")
        .inner_size(420.0, 660.0).resizable(false).always_on_top(false)
        .center().build().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_focus_timer_window(app: AppHandle) -> Result<(), String> {
    open_focus_timer_window_inner(&app, false).await
}

#[tauri::command]
pub async fn open_labels_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("labels") {
        let _ = win.show(); let _ = win.set_focus(); return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "labels", tauri::WebviewUrl::App("labels".into()))
        .title("All Labels")
        .inner_size(680.0, 600.0).min_inner_size(480.0, 400.0)
        .resizable(true).center().build().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_label_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("label") {
        let _ = win.show(); let _ = win.set_focus(); return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "label", tauri::WebviewUrl::App("label".into()))
        .title("Add Label")
        .inner_size(520.0, 560.0).min_inner_size(420.0, 380.0)
        .resizable(true).always_on_top(true)
        .center().build().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn close_label_window(app: AppHandle) {
    if let Some(win) = app.get_webview_window("label") { let _ = win.close(); }
}

#[tauri::command]
pub async fn open_api_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("api") {
        let _ = win.show(); let _ = win.set_focus(); return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "api", tauri::WebviewUrl::App("api".into()))
        .title("NeuroSkill™ – API Status")
        .inner_size(620.0, 560.0).min_inner_size(480.0, 400.0)
        .resizable(true).center().build().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_onboarding_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("onboarding") {
        let _ = win.show(); let _ = win.set_focus(); return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "onboarding",
        tauri::WebviewUrl::App("onboarding".into()))
        .title("NeuroSkill™ – Welcome")
        .inner_size(620.0, 700.0).min_inner_size(520.0, 580.0)
        .resizable(true).center().build().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn complete_onboarding(app: AppHandle, state: tauri::State<'_, Mutex<AppState>>) {
    state.lock_or_recover().onboarding_complete = true;
    save_settings(&app);
    if let Some(win) = app.get_webview_window("onboarding") { let _ = win.close(); }
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show(); let _ = win.set_focus();
    }
}

#[tauri::command]
pub fn get_onboarding_complete(state: tauri::State<'_, Mutex<AppState>>) -> bool {
    state.lock_or_recover().onboarding_complete
}

// ── Calibration window ────────────────────────────────────────────────────────

/// Open (or focus) the calibration window.  Requires an active streaming session.
pub(crate) async fn open_calibration_window_inner(
    app:        &AppHandle,
    profile_id: Option<String>,
    autostart:  bool,
) -> Result<(), String> {
    {
        let st = app.state::<Mutex<AppState>>();
        let guard = st.lock_or_recover();
        if guard.status.state != "connected" || guard.stream.is_none() {
            return Err("Calibration requires a connected BLE device that is streaming data".into());
        }
    }
    let url = {
        let mut q = String::new();
        if let Some(ref id) = profile_id { q.push_str(&format!("profile={id}")); }
        if autostart {
            if !q.is_empty() { q.push('&'); }
            q.push_str("autostart=1");
        }
        if q.is_empty() { "calibration".to_string() } else { format!("calibration?{q}") }
    };
    if let Some(win) = app.get_webview_window("calibration") {
        let _ = win.show(); let _ = win.set_focus();
        let _ = app.emit("calibration-run", serde_json::json!({
            "profile_id": profile_id, "autostart": autostart,
        }));
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(app, "calibration", tauri::WebviewUrl::App(url.into()))
        .title("NeuroSkill™ – Calibration")
        .inner_size(600.0, 700.0).min_inner_size(520.0, 600.0)
        .resizable(true).center().build().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_calibration_window(app: AppHandle) -> Result<(), String> {
    open_calibration_window_inner(&app, None, false).await
}

#[tauri::command]
pub async fn open_and_start_calibration(
    app:        AppHandle,
    profile_id: Option<String>,
) -> Result<(), String> {
    open_calibration_window_inner(&app, profile_id, true).await
}

#[tauri::command]
pub fn close_calibration_window(app: AppHandle) {
    if let Some(win) = app.get_webview_window("calibration") { let _ = win.close(); }
}

// ── Calibration profile CRUD ──────────────────────────────────────────────────

#[tauri::command]
pub fn list_calibration_profiles(state: tauri::State<'_, Mutex<AppState>>) -> Vec<CalibrationProfile> {
    state.lock_or_recover().calibration_profiles.clone()
}

#[tauri::command]
pub fn get_calibration_profile(
    id: String, state: tauri::State<'_, Mutex<AppState>>,
) -> Option<CalibrationProfile> {
    state.lock_or_recover().calibration_profiles.iter().find(|p| p.id == id).cloned()
}

#[tauri::command]
pub fn get_active_calibration(state: tauri::State<'_, Mutex<AppState>>) -> Option<CalibrationProfile> {
    let s = state.lock_or_recover();
    let id = s.active_calibration_id.clone();
    s.calibration_profiles.iter().find(|p| p.id == id).cloned()
        .or_else(|| s.calibration_profiles.first().cloned())
}

#[tauri::command]
pub fn set_active_calibration(id: String, app: AppHandle, state: tauri::State<'_, Mutex<AppState>>) {
    state.lock_or_recover().active_calibration_id = id;
    save_settings(&app);
}

#[tauri::command]
pub fn create_calibration_profile(
    mut profile: CalibrationProfile,
    app:         AppHandle,
    state:       tauri::State<'_, Mutex<AppState>>,
) -> CalibrationProfile {
    profile.id = new_profile_id();
    profile.last_calibration_utc = None;
    let ret = profile.clone();
    state.lock_or_recover().calibration_profiles.push(profile);
    save_settings(&app);
    ret
}

#[tauri::command]
pub fn update_calibration_profile(
    profile: CalibrationProfile, app: AppHandle, state: tauri::State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = state.lock_or_recover();
    let entry = s.calibration_profiles.iter_mut()
        .find(|p| p.id == profile.id)
        .ok_or_else(|| format!("profile not found: {}", profile.id))?;
    *entry = profile;
    drop(s);
    save_settings(&app);
    Ok(())
}

#[tauri::command]
pub fn delete_calibration_profile(
    id: String, app: AppHandle, state: tauri::State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = state.lock_or_recover();
    if s.calibration_profiles.len() <= 1 {
        return Err("Cannot delete the last calibration profile".into());
    }
    s.calibration_profiles.retain(|p| p.id != id);
    if s.active_calibration_id == id {
        s.active_calibration_id = s.calibration_profiles.first()
            .map(|p| p.id.clone()).unwrap_or_default();
    }
    drop(s);
    save_settings(&app);
    Ok(())
}

#[tauri::command]
pub fn record_calibration_completed(
    profile_id: Option<String>,
    app:        AppHandle,
    state:      tauri::State<'_, Mutex<AppState>>,
) {
    {
        let mut s = state.lock_or_recover();
        let target_id = profile_id.unwrap_or_else(|| s.active_calibration_id.clone());
        if let Some(p) = s.calibration_profiles.iter_mut().find(|p| p.id == target_id) {
            p.last_calibration_utc = Some(unix_secs());
        }
    }
    save_settings(&app);
    send_toast(&app, ToastLevel::Success, "Calibration Complete",
        "All calibration iterations finished successfully.");
}

// ── Legacy calibration compat ──────────────────────────────────────────────────

#[tauri::command]
pub fn get_calibration_config(state: tauri::State<'_, Mutex<AppState>>) -> CalibrationConfig {
    let s = state.lock_or_recover();
    let id = s.active_calibration_id.clone();
    let profile = s.calibration_profiles.iter().find(|p| p.id == id)
        .or_else(|| s.calibration_profiles.first());
    match profile {
        Some(p) => CalibrationConfig {
            action1_label:        p.actions.first().map(|a| a.label.clone()).unwrap_or_default(),
            action2_label:        p.actions.get(1).map(|a| a.label.clone()).unwrap_or_default(),
            action_duration_secs: p.actions.first().map(|a| a.duration_secs).unwrap_or(10),
            break_duration_secs:  p.break_duration_secs,
            loop_count:           p.loop_count,
            auto_start:           p.auto_start,
            last_calibration_utc: p.last_calibration_utc,
        },
        None => CalibrationConfig::default(),
    }
}

#[tauri::command]
pub fn set_calibration_config(_config: CalibrationConfig, _app: AppHandle) {
    // No-op: use update_calibration_profile instead.
}

// ── Misc app-level commands ────────────────────────────────────────────────────

#[tauri::command]
pub fn emit_calibration_event(event: String, payload: serde_json::Value, app: AppHandle) {
    let _ = app.emit(&event, &payload);
    app.state::<WsBroadcaster>().send(&event, &payload);
}

#[tauri::command]
pub fn quit_app(app: AppHandle) { app.exit(0); }

#[tauri::command]
pub fn get_app_version(app: AppHandle) -> String {
    app.config().version.clone().unwrap_or_else(|| "unknown".into())
}

#[tauri::command]
pub fn get_app_name(app: AppHandle) -> String {
    app.config().product_name.clone()
        .unwrap_or_else(|| app.package_info().name.clone())
}

#[tauri::command]
pub fn get_data_dir(state: tauri::State<'_, Mutex<AppState>>) -> (String, String) {
    let s = state.lock_or_recover();
    let current = tilde_path(&s.skill_dir);
    let default  = tilde_path(&default_skill_dir());
    (current, default)
}

#[tauri::command]
pub fn set_data_dir(path: String, app: AppHandle) -> Result<(), String> {
    if !path.is_empty() {
        let p = std::path::PathBuf::from(expand_tilde(&path));
        std::fs::create_dir_all(&p).map_err(|e| format!("Cannot create directory: {e}"))?;
        let test = p.join(".skill_write_test");
        std::fs::write(&test, b"ok").map_err(|e| format!("Directory not writable: {e}"))?;
        let _ = std::fs::remove_file(test);
    }
    {
        let r = app.state::<Mutex<AppState>>();
        let mut s = r.lock_or_recover();
        s.skill_dir = if path.is_empty() {
            default_skill_dir()
        } else {
            std::path::PathBuf::from(expand_tilde(&path))
        };
    }
    save_settings(&app);
    Ok(())
}

// ── WebSocket API status ───────────────────────────────────────────────────────

#[tauri::command]
pub fn get_ws_clients(broadcaster: tauri::State<'_, WsBroadcaster>) -> Vec<crate::ws_server::WsClient> {
    broadcaster.tracker.lock_or_recover().clients.clone()
}

#[tauri::command]
pub fn get_ws_request_log(broadcaster: tauri::State<'_, WsBroadcaster>) -> Vec<crate::ws_server::WsRequestLog> {
    broadcaster.tracker.lock_or_recover().requests.clone()
}

#[tauri::command]
pub fn get_ws_port(broadcaster: tauri::State<'_, WsBroadcaster>) -> u16 {
    broadcaster.tracker.lock_or_recover().port
}
