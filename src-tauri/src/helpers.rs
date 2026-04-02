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

use crate::settings::{save_secrets_from_settings, settings_path, CalibrationConfig, UserSettings};
use crate::state::*;
use crate::ws_server::WsBroadcaster;
use crate::MutexExt;

// ── Time helpers ──────────────────────────────────────────────────────────────

pub(crate) fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Returns today's date as `YYYYMMDD` (UTC).
/// Delegates to [`skill_data::util::yyyymmdd_utc`] — the canonical implementation.
pub(crate) fn yyyymmdd_utc() -> String {
    skill_data::util::yyyymmdd_utc()
}

// ── Status / device emitters ──────────────────────────────────────────────────

pub(crate) fn emit_status(app: &AppHandle) {
    let s_ref = app.app_state();
    let st = {
        let g = s_ref.lock_or_recover();
        g.status.clone()
    };
    // Renamed from "muse-status" to "status" — device-agnostic.
    let _ = app.emit("status", &st);
    app.state::<WsBroadcaster>().send("status", &st);
}

pub(crate) fn emit_devices(app: &AppHandle) {
    let s_ref = app.app_state();
    let d = {
        let g = s_ref.lock_or_recover();
        g.discovered.clone()
    };
    let _ = app.emit("devices-updated", &d);
}

/// Emit the Cortex WebSocket connection state to the frontend.
pub(crate) fn emit_cortex_ws_state(app: &AppHandle) {
    let state = {
        let s_ref = app.app_state();
        let g = s_ref.lock_or_recover();
        g.cortex_ws_state.clone()
    };
    let _ = app.emit("cortex-ws-state", &state);
}

/// Update the Cortex WS state in AppState and emit it to the frontend.
/// Sends a toast notification on meaningful transitions (connected ↔ disconnected).
pub(crate) fn set_cortex_ws_state(app: &AppHandle, state: &str) {
    let prev = {
        let s_ref = app.app_state();
        let mut g = s_ref.lock_or_recover();
        if g.cortex_ws_state == state {
            return;
        }
        let prev = g.cortex_ws_state.clone();
        g.cortex_ws_state = state.to_owned();
        prev
    };
    emit_cortex_ws_state(app);

    // Notify the user on meaningful transitions.
    match (prev.as_str(), state) {
        ("connected", "disconnected") => {
            // Remove stale Cortex-discovered devices so the UI doesn't show
            // green "paired" badges for headsets that are no longer reachable.
            remove_discovered_by_prefix(app, "cortex:");
            send_toast(
                app,
                crate::ToastLevel::Warning,
                "Emotiv Launcher Disconnected",
                "The Cortex service is no longer reachable. Make sure EMOTIV Launcher is running.",
            );
        }
        ("disconnected", "connected") => {
            // Only toast on the first successful probe after being truly
            // disconnected.  The scanner cycles through "connecting" →
            // "connected" on every 10 s poll; we don't want a toast each time.
            send_toast(
                app,
                crate::ToastLevel::Success,
                "Emotiv Launcher Connected",
                "Cortex service is reachable. Headsets will appear in the device list.",
            );
        }
        _ => {}
    }
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
    level: ToastLevel,
    title: String,
    message: String,
}

/// Send an in-app toast event AND a native OS notification.
pub(crate) fn send_toast(app: &AppHandle, level: ToastLevel, title: &str, message: &str) {
    let payload = ToastPayload {
        level,
        title: title.to_owned(),
        message: message.to_owned(),
    };
    let _ = app.emit("toast", &payload);
    app.state::<WsBroadcaster>().send("toast", &payload);
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(message)
        .show();
}

// ── State access helpers ──────────────────────────────────────────────────────

/// Extension trait that reduces the verbose
/// `app.state::<Mutex<Box<AppState>>>()` pattern (137+ call sites).
///
/// Implemented as a blanket impl for anything that implements `Manager<Wry>`,
/// so it works on `AppHandle`, `&AppHandle`, `App`, `WebviewWindow`, etc.
pub(crate) trait AppStateExt: Manager<tauri::Wry> {
    /// Obtain a reference to the `Mutex<Box<AppState>>` managed state.
    fn app_state(&self) -> tauri::State<'_, Mutex<Box<AppState>>> {
        self.state::<Mutex<Box<AppState>>>()
    }
}

impl<T: Manager<tauri::Wry>> AppStateExt for T {}

/// Read `skill_dir` from `AppState` without keeping the lock.
pub(crate) fn skill_dir(state: &Mutex<Box<AppState>>) -> std::path::PathBuf {
    state.lock_or_recover().skill_dir.clone()
}

/// Read a value from `AppState` via a short-lived lock.
pub(crate) fn read_state<T>(state: &Mutex<Box<AppState>>, f: impl FnOnce(&AppState) -> T) -> T {
    let g = state.lock_or_recover();
    f(&g)
}

/// Mutate `AppState` via a short-lived lock.
#[allow(dead_code)]
pub(crate) fn mutate_state(state: &Mutex<Box<AppState>>, f: impl FnOnce(&mut AppState)) {
    let mut g = state.lock_or_recover();
    f(&mut g);
}

/// Mutate `AppState` and auto-persist settings afterwards.
pub(crate) fn mutate_and_save(app: &AppHandle, f: impl FnOnce(&mut AppState)) {
    {
        let r = app.app_state();
        let mut g = r.lock_or_recover();
        f(&mut g);
    }
    save_settings(app);
}

// ── Settings persistence (debounced) ──────────────────────────────────────────

/// Debounce interval for settings persistence.  Multiple `save_settings` calls
/// within this window are collapsed into a single disk write.
const SETTINGS_DEBOUNCE_MS: u64 = 500;

/// Atomic flag: `true` while a debounce timer is already pending.
static SAVE_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn save_settings_handle(app: &AppHandle) {
    save_settings(app);
}

/// Schedule a settings save.  The actual disk write is debounced: if another
/// `save_settings` call arrives within [`SETTINGS_DEBOUNCE_MS`], only one
/// write is performed.  This prevents I/O storms when the user rapidly
/// toggles multiple settings.
pub(crate) fn save_settings(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    // If a timer is already pending, skip — it will pick up the latest state.
    if SAVE_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(SETTINGS_DEBOUNCE_MS)).await;
        SAVE_PENDING.store(false, Ordering::Release);
        save_settings_now(&handle);
    });
}

/// Immediately flush settings to disk (no debounce).  Called by the
/// debounce timer and by shutdown paths that must persist before exit.
pub(crate) fn save_settings_now(app: &AppHandle) {
    let s_ref = app.app_state();
    let s = s_ref.lock_or_recover();
    let data = UserSettings {
        paired: s.status.paired_devices.clone(),
        preferred_id: s.preferred_id.clone(),
        filter_config: s.status.filter_config,
        embedding_overlap_secs: s.status.embedding_overlap_secs,
        data_dir: None,
        label_shortcut: s.shortcuts.label_shortcut.clone(),
        search_shortcut: s.shortcuts.search_shortcut.clone(),
        settings_shortcut: s.shortcuts.settings_shortcut.clone(),
        calibration_shortcut: s.shortcuts.calibration_shortcut.clone(),
        help_shortcut: s.shortcuts.help_shortcut.clone(),
        history_shortcut: s.shortcuts.history_shortcut.clone(),
        api_shortcut: s.shortcuts.api_shortcut.clone(),
        theme_shortcut: s.shortcuts.theme_shortcut.clone(),
        focus_timer_shortcut: s.shortcuts.focus_timer_shortcut.clone(),
        #[cfg(feature = "llm")]
        chat_shortcut: s.shortcuts.chat_shortcut.clone(),
        calibration: CalibrationConfig::default(),
        calibration_profiles: s.calibration_profiles.clone(),
        active_calibration_id: s.active_calibration_id.clone(),
        onboarding_complete: s.ui.onboarding_complete,
        last_seen_whats_new_version: s.ui.last_seen_whats_new_version.clone(),
        theme: s.ui.theme.clone(),
        language: s.ui.language.clone(),
        accent_color: s.ui.accent_color.clone(),
        daily_goal_min: s.ui.daily_goal_min,
        goal_notified_date: s.ui.goal_notified_date.clone(),
        text_embedding_model: s.ui.text_embedding_model.clone(),
        hooks: s.hooks.clone(),
        ws_host: s.ws_host.clone(),
        ws_port: s.ws_port,
        api_token: s.api_token.clone(),
        update_check_interval_secs: s.update_check_interval_secs,
        openbci: s.openbci_config.clone(),
        device_api: s.device_api_config.clone(),
        neutts: s.neutts_config.clone(),
        tts_preload: s.tts_preload,
        track_active_window: s.input.track_active_window,
        track_input_activity: s.input.track_input_activity,
        main_window_auto_fit: s.ui.main_window_auto_fit,
        do_not_disturb: s.dnd.lock_or_recover().config.clone(),
        llm: {
            let __a = s.llm.clone();
            let __r = __a.lock_or_recover().config.clone();
            __r
        },
        screenshot: s.screenshot_config.clone(),
        sleep: s.sleep_config.clone(),
        storage_format: s.settings_storage_format.clone(),
        scanner: s.scanner_config.clone(),
        location_enabled: s.location_enabled,
        lsl_auto_connect: s.lsl_auto_connect,
        lsl_paired_streams: s.lsl_paired_streams.clone(),
        lsl_idle_timeout_secs: s.lsl_idle_timeout_secs,
        inference_device: s.inference_device.clone(),
        llm_gpu_layers_saved: s.llm_gpu_layers_saved,
        exg_inference_device: s.exg_inference_device.clone(),
    };
    let path = settings_path(&s.skill_dir);
    drop(s);

    // Persist secrets to the system keychain (encrypted, survives updates).
    save_secrets_from_settings(&data);

    if let Ok(json) = serde_json::to_string_pretty(&data) {
        if let Err(e) = std::fs::write(&path, &json) {
            eprintln!("[settings] save error: {e}");
        }
    }
}

// ── Transport inference ───────────────────────────────────────────────────────

/// Infer the transport type from a device ID.
///
/// Device IDs are prefixed by the scanner backend that discovered them:
/// * `usb:<port>`   → USB serial
/// * `cortex:<id>`  → Emotiv Cortex WebSocket
/// * anything else  → BLE (the default / legacy format)
pub(crate) fn transport_from_id(id: &str) -> crate::device_scanner::Transport {
    use crate::device_scanner::Transport;
    if id.starts_with("usb:") {
        Transport::UsbSerial
    } else if id.starts_with("cortex:") {
        Transport::Cortex
    } else {
        Transport::Ble
    }
}

// ── Paired device upsert ──────────────────────────────────────────────────────

pub(crate) fn upsert_paired(app: &AppHandle, id: &str, name: &str) {
    let now = unix_secs();
    let transport = transport_from_id(id);
    let s_ref = app.app_state();
    let mut s = s_ref.lock_or_recover();
    if let Some(d) = s.status.paired_devices.iter_mut().find(|d| d.id == id) {
        d.last_seen = now;
        d.name = name.to_owned();
    } else {
        s.status.paired_devices.push(PairedDevice {
            id: id.to_owned(),
            name: name.to_owned(),
            last_seen: now,
        });
    }
    let pref = s.preferred_id.clone();
    for d in s.discovered.iter_mut() {
        if d.id == id {
            d.is_paired = true;
            d.last_seen = now;
            d.name = name.to_owned();
        }
        d.is_preferred = pref.as_deref() == Some(&d.id);
    }
    if !s.discovered.iter().any(|d| d.id == id) {
        s.discovered.push(DiscoveredDevice {
            id: id.to_owned(),
            name: name.to_owned(),
            last_seen: now,
            last_rssi: 0,
            is_paired: true,
            is_preferred: pref.as_deref() == Some(id),
            transport,
        });
        s.discovered.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
    }
    drop(s);
    save_settings(app);
}

/// Update a discovered device entry (called from device scanner backends).
pub(crate) fn upsert_discovered(app: &AppHandle, id: &str, name: &str, rssi: i16) {
    let now = unix_secs();
    let transport = transport_from_id(id);
    let s_ref = app.app_state();
    let mut s = s_ref.lock_or_recover();
    // Exact match first; for Cortex devices also accept a legacy
    // "cortex:emotiv" paired entry as a match for any "cortex:<headset>"
    // discovered device (the legacy entry was created before individual
    // headset IDs were tracked).
    let is_paired = s.status.paired_devices.iter().any(|d| d.id == id)
        || (id.starts_with("cortex:")
            && id != "cortex:emotiv"
            && s.status
                .paired_devices
                .iter()
                .any(|d| d.id == "cortex:emotiv"));
    let is_preferred = s.preferred_id.as_deref() == Some(id)
        || (id.starts_with("cortex:") && s.preferred_id.as_deref() == Some("cortex:emotiv"));
    if let Some(d) = s.discovered.iter_mut().find(|d| d.id == id) {
        d.last_seen = now;
        d.last_rssi = rssi;
        d.is_paired = is_paired;
        d.is_preferred = is_preferred;
        d.name = name.to_owned();
        d.transport = transport;
    } else {
        s.discovered.push(DiscoveredDevice {
            id: id.to_owned(),
            name: name.to_owned(),
            last_seen: now,
            last_rssi: rssi,
            is_paired,
            is_preferred,
            transport,
        });
    }
    s.discovered.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
}

/// Remove all discovered devices whose id starts with `prefix`.
/// Used when a scanner backend goes offline (e.g. Cortex Launcher disconnects)
/// so stale entries don't linger in the device list with green badges.
pub(crate) fn remove_discovered_by_prefix(app: &AppHandle, prefix: &str) {
    let changed = {
        let s_ref = app.app_state();
        let mut s = s_ref.lock_or_recover();
        let before = s.discovered.len();
        s.discovered.retain(|d| !d.id.starts_with(prefix));
        s.discovered.len() != before
    };
    if changed {
        emit_devices(app);
    }
}
