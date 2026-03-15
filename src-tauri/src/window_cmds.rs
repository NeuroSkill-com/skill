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
    save_settings, mutate_and_save, unix_secs, send_toast, ToastLevel,
    default_skill_dir,
};
use crate::settings::tilde_path;
use crate::ws_server::WsBroadcaster;

// ── Window helper ─────────────────────────────────────────────────────────────

/// Configuration for creating a secondary window.
pub(crate) struct WindowSpec<'a> {
    pub label:          &'a str,
    pub route:          &'a str,
    pub title:          &'a str,
    pub inner_size:     (f64, f64),
    pub min_inner_size: Option<(f64, f64)>,
    pub resizable:      bool,
    pub always_on_top:  bool,
    pub maximized:      bool,
}

impl<'a> Default for WindowSpec<'a> {
    fn default() -> Self {
        Self {
            label: "", route: "", title: "",
            inner_size: (680.0, 720.0),
            min_inner_size: None,
            resizable: true,
            always_on_top: false,
            maximized: false,
        }
    }
}

/// Focus an existing window or create a new one from `spec`.
///
/// Deduplicates the repeated "check-existing → unminimize/show/focus → or build new"
/// pattern used by all `open_*_window` commands.
pub(crate) fn focus_or_create(app: &AppHandle, spec: WindowSpec) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(spec.label) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }
    let mut builder = tauri::WebviewWindowBuilder::new(
        app, spec.label,
        tauri::WebviewUrl::App(spec.route.into()),
    )
    .title(spec.title)
    .inner_size(spec.inner_size.0, spec.inner_size.1)
    .resizable(spec.resizable)
    .center()
    .decorations(false)
    .transparent(true);

    if let Some((w, h)) = spec.min_inner_size {
        builder = builder.min_inner_size(w, h);
    }
    if spec.always_on_top {
        builder = builder.always_on_top(true);
    }
    if spec.maximized {
        builder = builder.maximized(true);
    }

    builder.build()
        .map(|w| { let _ = w.set_focus(); })
        .map_err(|e| e.to_string())
}

/// Like `focus_or_create` but emits an event to the existing window when it is
/// already open.  Used for settings sub-tabs (model, updates, etc.).
pub(crate) fn focus_or_create_with_emit(
    app: &AppHandle, spec: WindowSpec, event: &str, payload: &str,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(spec.label) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        let _ = win.emit(event, payload.to_string());
        return Ok(());
    }
    // Fall through to normal builder
    let mut builder = tauri::WebviewWindowBuilder::new(
        app, spec.label,
        tauri::WebviewUrl::App(spec.route.into()),
    )
    .title(spec.title)
    .inner_size(spec.inner_size.0, spec.inner_size.1)
    .resizable(spec.resizable)
    .center()
    .decorations(false)
    .transparent(true);

    if let Some((w, h)) = spec.min_inner_size {
        builder = builder.min_inner_size(w, h);
    }

    builder.build()
        .map(|w| { let _ = w.set_focus(); })
        .map_err(|e| e.to_string())
}

// ── Permissions ───────────────────────────────────────────────────────────────

/// Return whether the app currently holds macOS Accessibility (AX) permission.
/// Always returns `true` on non-macOS platforms (no permission required there).
#[tauri::command]
pub fn check_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: AXIsProcessTrusted is a plain C function that reads a process
        // flag; it is safe to call from any thread.
        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" { fn AXIsProcessTrusted() -> bool; }
        unsafe { AXIsProcessTrusted() }
    }
    #[cfg(not(target_os = "macos"))]
    { true }
}

/// Open the OS panel where the user can grant Accessibility permission.
#[tauri::command]
pub fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn(); }
    #[cfg(target_os = "linux")]
    { /* no-op */ }
    #[cfg(target_os = "windows")]
    { /* no-op — SetWindowsHookEx requires no special OS permission */ }
}

/// Check whether Screen Recording permission is granted (macOS 10.15+).
/// Uses `CGWindowListCopyWindowInfo` and inspects whether the returned
/// window list contains window names (kCGWindowName) for windows owned
/// by other processes.  macOS redacts window names when the permission
/// has not been granted.  Returns `true` on non-macOS platforms.
#[tauri::command]
pub fn check_screen_recording_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::ffi::c_void;
        type CFTypeRef = *const c_void;
        type CFArrayRef = *const c_void;

        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGWindowListCopyWindowInfo(option: u32, relativeToWindow: u32) -> CFArrayRef;
        }
        #[link(name = "CoreFoundation", kind = "framework")]
        extern "C" {
            fn CFArrayGetCount(arr: CFArrayRef) -> isize;
            fn CFRelease(cf: CFTypeRef);
        }

        // kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements
        const OPTIONS: u32 = (1 << 0) | (1 << 4);

        unsafe {
            let list = CGWindowListCopyWindowInfo(OPTIONS, 0);
            if list.is_null() { return false; }
            // If the list is non-empty, the permission has been granted
            // (macOS returns an empty or heavily redacted list without
            // screen recording permission — but the count itself is
            // still > 0 even without permission).
            //
            // The reliable test: attempt a screencapture of a known window.
            // For simplicity, we check if the list has more than 2 entries
            // (with permission denied, macOS may still return the app's own
            // windows but nothing else).
            let count = CFArrayGetCount(list);
            CFRelease(list);
            // With screen recording permission, the list typically has
            // many windows (menubar, dock, other apps).  Without it,
            // only the app's own windows appear (usually 0–2).
            // A threshold of > 3 is a reasonable heuristic.
            count > 3
        }
    }
    #[cfg(not(target_os = "macos"))]
    { true }
}

/// Open the macOS Screen Recording permission panel.
#[tauri::command]
pub fn open_screen_recording_settings() {
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .spawn(); }
    #[cfg(not(target_os = "macos"))]
    { /* no-op — no special permission required */ }
}

/// Open the OS notification settings panel.
#[tauri::command]
pub fn open_notifications_settings() {
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.notifications")
        .spawn(); }
    #[cfg(target_os = "linux")]
    { let _ = std::process::Command::new("sh").arg("-c")
        .arg("gnome-control-center notifications 2>/dev/null || true").spawn(); }
    #[cfg(target_os = "windows")]
    { let _ = std::process::Command::new("ms-settings:notifications").spawn(); }
}

// ── First-launch window reveal ────────────────────────────────────────────────

/// Called from `+layout.svelte` `onMount` to reveal the main window only
/// after WKWebView has fully rendered the page.
///
/// On macOS, calling `win.show()` during Tauri's setup closure (before the
/// web-content process has loaded the frontend) produces a solid white frame.
/// Deferring the show until the JS side's `onMount` fires guarantees the
/// compositor already has pixels to display, eliminating the white screen.
///
/// On Linux and Windows the window is shown in setup as before, so calling
/// `show()` on an already-visible window is harmless.
///
/// Skips the show for windows whose label isn't "main" (e.g. settings,
/// help, calibration) — those are also wrapped by +layout.svelte and would
/// otherwise steal focus on every secondary-window open.
///
/// Also skips the show when onboarding is incomplete: `complete_onboarding`
/// will call `win.show()` itself once the user finishes onboarding.
#[tauri::command]
pub fn show_main_window(
    win:   tauri::WebviewWindow,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    if win.label() != "main" { return; }
    if !state.lock_or_recover().onboarding_complete { return; }
    let _ = win.show();
    let _ = win.set_focus();
    crate::linux_fix_decorations(&win);
}

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
    focus_or_create(&app, WindowSpec {
        label: "settings", route: "settings", title: "NeuroSkill™ – Settings",
        inner_size: (760.0, 720.0), min_inner_size: Some((580.0, 560.0)),
        ..Default::default()
    })
}

#[tauri::command]
pub async fn open_model_tab(app: AppHandle) -> Result<(), String> {
    focus_or_create_with_emit(&app, WindowSpec {
        label: "settings", route: "settings?tab=model", title: "NeuroSkill™ – Model",
        inner_size: (760.0, 720.0), min_inner_size: Some((580.0, 560.0)),
        ..Default::default()
    }, "switch-tab", "model")
}

#[tauri::command]
pub async fn open_updates_window(app: AppHandle) -> Result<(), String> {
    focus_or_create_with_emit(&app, WindowSpec {
        label: "settings", route: "settings?tab=updates", title: "NeuroSkill™ – Updates",
        inner_size: (760.0, 720.0), min_inner_size: Some((580.0, 560.0)),
        ..Default::default()
    }, "switch-tab", "updates")
}

#[tauri::command]
pub async fn open_help_window(app: AppHandle) -> Result<(), String> {
    focus_or_create(&app, WindowSpec {
        label: "help", route: "help", title: "NeuroSkill™ – Help",
        inner_size: (680.0, 720.0), min_inner_size: Some((600.0, 520.0)),
        ..Default::default()
    })
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
        let _ = win.unminimize(); let _ = win.show(); let _ = win.set_focus(); return Ok(());
    }
    let encoded: String = csv_path.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
        _ => format!("%{:02X}", b),
    }).collect();
    tauri::WebviewWindowBuilder::new(&app, &label,
        tauri::WebviewUrl::App(format!("session?csv_path={encoded}").into()))
        .title("NeuroSkill™ – Session Detail")
        .inner_size(680.0, 700.0).min_inner_size(480.0, 400.0)
        .resizable(true).center().decorations(false).transparent(true).build().map(|w| { let _ = w.set_focus(); }).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_search_window(app: AppHandle) -> Result<(), String> {
    focus_or_create(&app, WindowSpec {
        label: "search", route: "search", title: "EEG Search",
        inner_size: (1100.0, 820.0), min_inner_size: Some((700.0, 560.0)),
        maximized: true, ..Default::default()
    })
}

#[tauri::command]
pub(crate) async fn open_focus_timer_window_inner(
    app:       &AppHandle,
    autostart: bool,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("focus-timer") {
        let _ = win.unminimize();
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
        .center().decorations(false).transparent(true).build().map(|w| { let _ = w.set_focus(); }).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_focus_timer_window(app: AppHandle) -> Result<(), String> {
    open_focus_timer_window_inner(&app, false).await
}

#[tauri::command]
pub async fn open_labels_window(app: AppHandle) -> Result<(), String> {
    focus_or_create(&app, WindowSpec {
        label: "labels", route: "labels", title: "All Labels",
        inner_size: (680.0, 600.0), min_inner_size: Some((480.0, 400.0)),
        ..Default::default()
    })
}

#[tauri::command]
pub async fn open_label_window(app: AppHandle) -> Result<(), String> {
    focus_or_create(&app, WindowSpec {
        label: "label", route: "label", title: "Add Label",
        inner_size: (520.0, 560.0), min_inner_size: Some((420.0, 380.0)),
        always_on_top: true, ..Default::default()
    })
}

#[tauri::command]
pub fn close_label_window(app: AppHandle) {
    if let Some(win) = app.get_webview_window("label") { let _ = win.close(); }
}

#[tauri::command]
pub async fn open_api_window(app: AppHandle) -> Result<(), String> {
    focus_or_create(&app, WindowSpec {
        label: "api", route: "api", title: "NeuroSkill™ – API Status",
        inner_size: (620.0, 560.0), min_inner_size: Some((480.0, 400.0)),
        ..Default::default()
    })
}

/// Return the last app version for which the What's New window was dismissed.
///
/// An empty string means the window has never been seen.
#[tauri::command]
pub fn get_whats_new_seen_version(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    state.lock_or_recover().last_seen_whats_new_version.clone()
}

/// Persist the acknowledged version and close the What's New window.
///
/// Combining both operations in Rust avoids relying on the frontend window
/// API (which can silently fail in secondary webview windows).
#[tauri::command]
pub fn dismiss_whats_new(version: String, app: AppHandle) {
    mutate_and_save(&app, |s| s.last_seen_whats_new_version = version);
    if let Some(win) = app.get_webview_window("whats-new") {
        let _ = win.close();
    }
}

/// Open (or focus) the What's New window.
///
/// The frontend calls `get_whats_new_seen_version` first and only invokes
/// this command when the stored version differs from the running version.
/// The window's own page calls `set_whats_new_seen_version` on dismiss.
#[tauri::command]
pub async fn open_whats_new_window(app: AppHandle) -> Result<(), String> {
    focus_or_create(&app, WindowSpec {
        label: "whats-new", route: "whats-new", title: "What's New in NeuroSkill™",
        inner_size: (520.0, 620.0), resizable: false,
        ..Default::default()
    })
}

#[tauri::command]
pub async fn open_onboarding_window(app: AppHandle) -> Result<(), String> {
    focus_or_create(&app, WindowSpec {
        label: "onboarding", route: "onboarding", title: "NeuroSkill™ – Welcome",
        inner_size: (680.0, 760.0), min_inner_size: Some((560.0, 620.0)),
        ..Default::default()
    })
}

    #[tauri::command]
    pub fn get_onboarding_model_download_order() -> Vec<String> {
        crate::constants::ONBOARDING_MODEL_DOWNLOAD_ORDER
        .iter()
        .map(|item| (*item).to_string())
        .collect()
    }

#[tauri::command]
pub fn complete_onboarding(app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    mutate_and_save(&app, |s| s.onboarding_complete = true);
    if let Some(win) = app.get_webview_window("onboarding") { let _ = win.close(); }
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show(); let _ = win.set_focus();
        crate::linux_fix_decorations(&win);
    }
}

#[tauri::command]
pub fn get_onboarding_complete(state: tauri::State<'_, Mutex<Box<AppState>>>) -> bool {
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
        let st = app.state::<Mutex<Box<AppState>>>();
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
        let _ = win.unminimize(); let _ = win.show(); let _ = win.set_focus();
        let _ = app.emit("calibration-run", serde_json::json!({
            "profile_id": profile_id, "autostart": autostart,
        }));
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(app, "calibration", tauri::WebviewUrl::App(url.into()))
        .title("NeuroSkill™ – Calibration")
        .inner_size(600.0, 700.0).min_inner_size(520.0, 600.0)
        .resizable(true).center().decorations(false).transparent(true).build().map(|w| { let _ = w.set_focus(); }).map_err(|e| e.to_string())
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
pub fn list_calibration_profiles(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<CalibrationProfile> {
    state.lock_or_recover().calibration_profiles.clone()
}

#[tauri::command]
pub fn get_calibration_profile(
    id: String, state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Option<CalibrationProfile> {
    state.lock_or_recover().calibration_profiles.iter().find(|p| p.id == id).cloned()
}

#[tauri::command]
pub fn get_active_calibration(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Option<CalibrationProfile> {
    let s = state.lock_or_recover();
    let id = s.active_calibration_id.clone();
    s.calibration_profiles.iter().find(|p| p.id == id).cloned()
        .or_else(|| s.calibration_profiles.first().cloned())
}

#[tauri::command]
pub fn set_active_calibration(id: String, app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    mutate_and_save(&app, |s| s.active_calibration_id = id);
}

#[tauri::command]
pub fn create_calibration_profile(
    mut profile: CalibrationProfile,
    app:         AppHandle,
    _state:      tauri::State<'_, Mutex<Box<AppState>>>,
) -> CalibrationProfile {
    profile.id = new_profile_id();
    profile.last_calibration_utc = None;
    let ret = profile.clone();
    mutate_and_save(&app, |s| s.calibration_profiles.push(profile));
    ret
}

#[tauri::command]
pub fn update_calibration_profile(
    profile: CalibrationProfile, app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    let r = app.state::<Mutex<Box<AppState>>>();
    let mut s = r.lock_or_recover();
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
    id: String, app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    let r = app.state::<Mutex<Box<AppState>>>();
    let mut s = r.lock_or_recover();
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
    _state:     tauri::State<'_, Mutex<Box<AppState>>>,
) {
    mutate_and_save(&app, |s| {
        let target_id = profile_id.unwrap_or_else(|| s.active_calibration_id.clone());
        if let Some(p) = s.calibration_profiles.iter_mut().find(|p| p.id == target_id) {
            p.last_calibration_utc = Some(unix_secs());
        }
    });
    send_toast(&app, ToastLevel::Success, "Calibration Complete",
        "All calibration iterations finished successfully.");
}

// ── Legacy calibration compat ──────────────────────────────────────────────────

#[tauri::command]
pub fn get_calibration_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> CalibrationConfig {
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
pub fn get_data_dir(_state: tauri::State<'_, Mutex<Box<AppState>>>) -> (String, String) {
    // skill_dir is always ~/.skill — hardcoded, never configurable
    let fixed = tilde_path(&default_skill_dir());
    (fixed.clone(), fixed)
}

#[tauri::command]
pub fn set_data_dir(_path: String, _app: AppHandle) -> Result<(), String> {
    // skill_dir is always ~/.skill — this command is intentionally a no-op
    Ok(())
}

#[tauri::command]
pub fn open_skill_dir() {
    let dir = default_skill_dir();
    #[cfg(target_os = "macos")]
    { let _ = std::process::Command::new("open").arg(&dir).spawn(); }
    #[cfg(target_os = "linux")]
    {
        let mut launched = false;
        if std::process::Command::new("xdg-open").arg(&dir).spawn().is_ok() {
            launched = true;
        }
        if !launched {
            let _ = std::process::Command::new("gio").arg("open").arg(&dir).spawn();
        }
    }
    #[cfg(target_os = "windows")]
    { let _ = std::process::Command::new("explorer").arg(&dir).spawn(); }
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
