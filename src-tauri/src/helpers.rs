// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Shared helpers: time, status/device emitters, toast, device upsert,
//! settings persistence, state access shortcuts.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::state::*;
use crate::ws_server::WsBroadcaster;
use crate::MutexExt;
use crate::settings::{
    CalibrationConfig, UserSettings,
    settings_path,
};

// ── Time helpers ──────────────────────────────────────────────────────────────

pub(crate) fn unix_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Returns today's date as `YYYYMMDD` (UTC) without any external crate.
pub(crate) fn yyyymmdd_utc() -> String {
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

// ── Status / device emitters ──────────────────────────────────────────────────

pub(crate) fn emit_status(app: &AppHandle) {
    let s_ref = app.state::<Mutex<Box<AppState>>>();
    let st = { let g = s_ref.lock_or_recover(); g.status.clone() };
    // Event name kept as "muse-status" for backward compatibility with
    // existing WS clients and frontend listeners.
    let _ = app.emit("muse-status", &st);
    app.state::<WsBroadcaster>().send("muse-status", &st);
}

pub(crate) fn emit_devices(app: &AppHandle) {
    let s_ref = app.state::<Mutex<Box<AppState>>>();
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

#[derive(Clone, Serialize)]
struct ToastPayload {
    level:   ToastLevel,
    title:   String,
    message: String,
}

/// Send an in-app toast event AND a native OS notification.
pub(crate) fn send_toast(app: &AppHandle, level: ToastLevel, title: &str, message: &str) {
    let payload = ToastPayload { level, title: title.to_owned(), message: message.to_owned() };
    let _ = app.emit("toast", &payload);
    app.state::<WsBroadcaster>().send("toast", &payload);
    let _ = app.notification().builder().title(title).body(message).show();
}

// ── State access helpers ──────────────────────────────────────────────────────

/// Read `skill_dir` from `AppState` without keeping the lock.
pub(crate) fn skill_dir(state: &Mutex<Box<AppState>>) -> std::path::PathBuf {
    state.lock_or_recover().skill_dir.clone()
}

/// Read a value from `AppState` via a short-lived lock.
pub(crate) fn read_state<T>(
    state: &Mutex<Box<AppState>>,
    f: impl FnOnce(&AppState) -> T,
) -> T {
    let g = state.lock_or_recover();
    f(&g)
}

/// Mutate `AppState` via a short-lived lock.
#[allow(dead_code)]
pub(crate) fn mutate_state(
    state: &Mutex<Box<AppState>>,
    f: impl FnOnce(&mut AppState),
) {
    let mut g = state.lock_or_recover();
    f(&mut g);
}

/// Mutate `AppState` and auto-persist settings afterwards.
pub(crate) fn mutate_and_save(
    app: &AppHandle,
    f: impl FnOnce(&mut AppState),
) {
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut g = r.lock_or_recover();
        f(&mut g);
    }
    save_settings(app);
}

// ── Settings persistence ──────────────────────────────────────────────────────

pub fn save_settings_handle(app: &AppHandle) { save_settings(app); }

pub(crate) fn save_settings(app: &AppHandle) {
    let s_ref = app.state::<Mutex<Box<AppState>>>();
    let s = s_ref.lock_or_recover();
    let data = UserSettings {
        paired:                 s.status.paired_devices.clone(),
        preferred_id:           s.preferred_id.clone(),
        filter_config:          s.status.filter_config,
        embedding_overlap_secs: s.status.embedding_overlap_secs,
        data_dir: None,
        label_shortcut:         s.label_shortcut.clone(),
        search_shortcut:        s.search_shortcut.clone(),
        settings_shortcut:      s.settings_shortcut.clone(),
        calibration_shortcut:   s.calibration_shortcut.clone(),
        help_shortcut:          s.help_shortcut.clone(),
        history_shortcut:       s.history_shortcut.clone(),
        api_shortcut:           s.api_shortcut.clone(),
        theme_shortcut:         s.theme_shortcut.clone(),
        focus_timer_shortcut:   s.focus_timer_shortcut.clone(),
        #[cfg(feature = "llm")]
        chat_shortcut:          s.chat_shortcut.clone(),
        calibration:            CalibrationConfig::default(),
        calibration_profiles:   s.calibration_profiles.clone(),
        active_calibration_id:  s.active_calibration_id.clone(),
        onboarding_complete:                s.onboarding_complete,
        last_seen_whats_new_version:        s.last_seen_whats_new_version.clone(),
        theme:                  s.theme.clone(),
        language:               s.language.clone(),
        accent_color:           s.accent_color.clone(),
        daily_goal_min:         s.daily_goal_min,
        goal_notified_date:     s.goal_notified_date.clone(),
        text_embedding_model:   s.text_embedding_model.clone(),
        hooks:                  s.hooks.clone(),
        ws_host:                s.ws_host.clone(),
        ws_port:                s.ws_port,
        update_check_interval_secs: s.update_check_interval_secs,
        openbci:                s.openbci_config.clone(),
        neutts:                 s.neutts_config.clone(),
        tts_preload:            s.tts_preload,
        track_active_window:    s.track_active_window,
        track_input_activity:   s.track_input_activity,
        do_not_disturb:         s.dnd_config.clone(),
        llm:                    s.llm.config.clone(),
        screenshot:             s.screenshot_config.clone(),
        sleep:                  s.sleep_config.clone(),
    };
    let path = settings_path(&s.skill_dir);
    drop(s);
    if let Ok(json) = serde_json::to_string_pretty(&data) {
        if let Err(e) = std::fs::write(&path, &json) {
            eprintln!("[settings] save error: {e}");
        }
    }
}

// ── Paired device upsert ──────────────────────────────────────────────────────

pub(crate) fn upsert_paired(app: &AppHandle, id: &str, name: &str) {
    let now = unix_secs();
    let s_ref = app.state::<Mutex<Box<AppState>>>();
    let mut s = s_ref.lock_or_recover();
    if let Some(d) = s.status.paired_devices.iter_mut().find(|d| d.id == id) {
        d.last_seen = now; d.name = name.to_owned();
    } else {
        s.status.paired_devices.push(PairedDevice {
            id: id.to_owned(), name: name.to_owned(), last_seen: now,
        });
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

/// Update a discovered device entry (called from BLE scanner).
pub(crate) fn upsert_discovered(app: &AppHandle, id: &str, name: &str, rssi: i16) {
    let now = unix_secs();
    let s_ref = app.state::<Mutex<Box<AppState>>>();
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
