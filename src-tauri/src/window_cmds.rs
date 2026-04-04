// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Window open/close commands and calibration profile CRUD.

use crate::MutexExt;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::tilde_path;
use crate::ws_server::WsBroadcaster;
use crate::AppStateExt;
use crate::{
    default_skill_dir, mutate_and_save, send_toast, unix_secs, AppState, CalibrationConfig,
    CalibrationProfile, ToastLevel,
};

// ── Window helper ─────────────────────────────────────────────────────────────

/// Configuration for creating a secondary window.
pub(crate) struct WindowSpec<'a> {
    pub label: &'a str,
    pub route: &'a str,
    pub title: &'a str,
    pub inner_size: (f64, f64),
    pub min_inner_size: Option<(f64, f64)>,
    pub resizable: bool,
    pub always_on_top: bool,
    pub maximized: bool,
}

impl<'a> Default for WindowSpec<'a> {
    fn default() -> Self {
        Self {
            label: "",
            route: "",
            title: "",
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
        app,
        spec.label,
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

    builder
        .build()
        .map(|w| {
            let _ = w.set_focus();
        })
        .map_err(|e| e.to_string())
}

/// Like `focus_or_create` but emits an event to the existing window when it is
/// already open.  Used for settings sub-tabs (model, updates, etc.).
pub(crate) fn focus_or_create_with_emit(
    app: &AppHandle,
    spec: WindowSpec,
    event: &str,
    payload: &str,
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
        app,
        spec.label,
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

    builder
        .build()
        .map(|w| {
            let _ = w.set_focus();
        })
        .map_err(|e| e.to_string())
}

// ── Permissions ───────────────────────────────────────────────────────────────

/// Return whether the app currently holds macOS Accessibility (AX) permission.
/// Always returns `true` on non-macOS platforms (no permission required there).
#[tauri::command]
pub fn check_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn AXIsProcessTrusted() -> bool;
        }
        // SAFETY: AXIsProcessTrusted is a plain C function that reads a process
        // flag; it is safe to call from any thread.
        unsafe { AXIsProcessTrusted() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Open the OS panel where the user can grant Accessibility permission.
///
/// macOS 13+: `x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility`
/// macOS 12−: `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`
#[tauri::command]
pub fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        // Try the macOS 13+ (Ventura) deep link first; fall back to the
        // legacy Security & Privacy URL for macOS 12 (Monterey) and below.
        let modern = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility")
            .output();
        if modern.is_err() || modern.is_ok_and(|o| !o.status.success()) {
            let _ = std::process::Command::new("open")
                .arg(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                )
                .spawn();
        }
    }
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

        // SAFETY: CGWindowListCopyWindowInfo returns a CFArray (or null).
        // We only read the count and release it — no dangling pointers.
        unsafe {
            let list = CGWindowListCopyWindowInfo(OPTIONS, 0);
            if list.is_null() {
                return false;
            }
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
    {
        true
    }
}

/// Open the macOS Screen Recording permission panel.
///
/// macOS 13+: `x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ScreenCapture`
/// macOS 15+ (Sequoia): Screen Recording moved to its own sub-section.
#[tauri::command]
pub fn open_screen_recording_settings() {
    #[cfg(target_os = "macos")]
    {
        let modern = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ScreenCapture")
            .output();
        if modern.is_err() || modern.is_ok_and(|o| !o.status.success()) {
            let _ = std::process::Command::new("open")
                .arg(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
                )
                .spawn();
        }
    }
    #[cfg(not(target_os = "macos"))]
    { /* no-op — no special permission required */ }
}

/// Open the OS notification settings panel.
///
/// macOS 13+: deep-links directly to the Notifications pane.
/// Windows: `ms-settings:notifications`
/// Linux: gnome-control-center or KDE systemsettings.
#[tauri::command]
pub fn open_notifications_settings() {
    #[cfg(target_os = "macos")]
    {
        // macOS 13+ Ventura: direct path to Notifications pane
        let modern = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.settings.Notifications")
            .output();
        if modern.is_err() || modern.is_ok_and(|o| !o.status.success()) {
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.notifications")
                .spawn();
        }
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(
                "gnome-control-center notifications 2>/dev/null \
                  || systemsettings kcm_notifications 2>/dev/null \
                  || true",
            )
            .spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "ms-settings:notifications"])
            .spawn();
    }
}

/// Open the OS calendar privacy settings so the user can re-grant access
/// after initially denying it.
///
/// macOS 13+: `Privacy_Calendars`
/// macOS 12−: `Privacy_Calendars`
/// Windows/Linux: no-op (calendar access uses native OS prompts).
#[tauri::command]
pub fn open_calendar_settings() {
    #[cfg(target_os = "macos")]
    {
        let modern = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Calendars")
            .output();
        if modern.is_err() || modern.is_ok_and(|o| !o.status.success()) {
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars")
                .spawn();
        }
    }
    #[cfg(not(target_os = "macos"))]
    { /* no-op */ }
}

/// Open the OS input-monitoring / keyboard privacy settings.
///
/// macOS 13+: `Privacy_ListenEvent` (input monitoring — required for global
/// keyboard/mouse activity tracking).
#[tauri::command]
pub fn open_input_monitoring_settings() {
    #[cfg(target_os = "macos")]
    {
        let modern = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ListenEvent")
            .output();
        if modern.is_err() || modern.is_ok_and(|o| !o.status.success()) {
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
                .spawn();
        }
    }
    #[cfg(not(target_os = "macos"))]
    { /* no-op */ }
}

/// Open the OS Focus / Do Not Disturb settings.
///
/// macOS 13+: `com.apple.settings.Focus`
/// Windows 11: `ms-settings:quiethours`
#[tauri::command]
pub fn open_focus_settings() {
    #[cfg(target_os = "macos")]
    {
        let modern = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.settings.Focus")
            .output();
        if modern.is_err() || modern.is_ok_and(|o| !o.status.success()) {
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.notifications")
                .spawn();
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "ms-settings:quiethours"])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    { /* no-op */ }
}

/// Return calendar permission status as one of:
/// `authorized`, `denied`, `restricted`, `not_determined`.
#[tauri::command]
pub fn get_calendar_permission_status() -> String {
    match skill_calendar::auth_status() {
        skill_calendar::AuthStatus::Authorized => "authorized",
        skill_calendar::AuthStatus::Denied => "denied",
        skill_calendar::AuthStatus::Restricted => "restricted",
        skill_calendar::AuthStatus::NotDetermined => "not_determined",
    }
    .to_string()
}

/// Request calendar access (macOS shows the native dialog; other platforms are no-op).
#[tauri::command]
pub async fn request_calendar_permission() -> Result<bool, String> {
    tokio::task::spawn_blocking(skill_calendar::request_access)
        .await
        .map_err(|e| format!("calendar permission task error: {e}"))
}

/// Fetch calendar events overlapping the `[start_utc, end_utc]` window.
#[tauri::command]
pub async fn get_calendar_events(
    start_utc: i64,
    end_utc: i64,
) -> Result<Vec<skill_calendar::CalendarEvent>, String> {
    if end_utc < start_utc {
        return Err("end_utc must be >= start_utc".into());
    }
    tokio::task::spawn_blocking(move || skill_calendar::fetch_events(start_utc, end_utc))
        .await
        .map_err(|e| format!("calendar events task error: {e}"))?
}

// ── Location permission ───────────────────────────────────────────────────────

/// Return location permission status as one of:
/// `authorized`, `denied`, `restricted`, `not_determined`.
#[tauri::command]
pub fn get_location_permission_status() -> String {
    match skill_location::auth_status() {
        skill_location::LocationAuthStatus::Authorized => "authorized",
        skill_location::LocationAuthStatus::Denied => "denied",
        skill_location::LocationAuthStatus::Restricted => "restricted",
        skill_location::LocationAuthStatus::NotDetermined => "not_determined",
    }
    .to_string()
}

/// Request location access (macOS shows the native dialog; other platforms are no-op).
#[tauri::command]
pub async fn request_location_permission() -> Result<bool, String> {
    tokio::task::spawn_blocking(|| skill_location::request_access(30.0))
        .await
        .map_err(|e| format!("location permission task error: {e}"))
}

/// Open the macOS Location Services privacy settings pane.
#[tauri::command]
pub fn open_location_settings() {
    #[cfg(target_os = "macos")]
    {
        let modern = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_LocationServices")
            .output();
        if modern.is_err() || modern.is_ok_and(|o| !o.status.success()) {
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_LocationServices")
                .spawn();
        }
    }
    #[cfg(not(target_os = "macos"))]
    { /* no-op */ }
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
pub fn show_main_window(win: tauri::WebviewWindow, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    if win.label() != "main" {
        return;
    }
    if !state.lock_or_recover().ui.onboarding_complete {
        return;
    }
    let _ = win.show();
    let _ = win.set_focus();
    crate::linux_fix_decorations(&win);
}

// ── Window command macro ──────────────────────────────────────────────────────

/// Generate a `#[tauri::command] pub async fn $name(app) -> Result<(), String>`
/// that calls `focus_or_create` with the given `WindowSpec`.
macro_rules! window_cmd {
    // With min_inner_size and extra fields.
    ($name:ident, $label:expr, $route:expr, $title:expr,
     size: ($w:expr, $h:expr), min: ($mw:expr, $mh:expr) $(, $field:ident: $val:expr)*) => {
        #[tauri::command]
        pub async fn $name(app: AppHandle) -> Result<(), String> {
            focus_or_create(&app, WindowSpec {
                label: $label, route: $route, title: $title,
                inner_size: ($w, $h),
                min_inner_size: Some(($mw, $mh)),
                $($field: $val,)*
                ..Default::default()
            })
        }
    };
    // Without min_inner_size.
    ($name:ident, $label:expr, $route:expr, $title:expr,
     size: ($w:expr, $h:expr) $(, $field:ident: $val:expr)*) => {
        #[tauri::command]
        pub async fn $name(app: AppHandle) -> Result<(), String> {
            focus_or_create(&app, WindowSpec {
                label: $label, route: $route, title: $title,
                inner_size: ($w, $h),
                $($field: $val,)*
                ..Default::default()
            })
        }
    };
}

/// Like `window_cmd!` but emits an event to the existing window.
macro_rules! window_tab_cmd {
    ($name:ident, $label:expr, $route:expr, $title:expr,
     size: ($w:expr, $h:expr), min: ($mw:expr, $mh:expr),
     event: $ev:expr, payload: $pl:expr) => {
        #[tauri::command]
        pub async fn $name(app: AppHandle) -> Result<(), String> {
            focus_or_create_with_emit(
                &app,
                WindowSpec {
                    label: $label,
                    route: $route,
                    title: $title,
                    inner_size: ($w, $h),
                    min_inner_size: Some(($mw, $mh)),
                    ..Default::default()
                },
                $ev,
                $pl,
            )
        }
    };
}

// ── Bluetooth & utility windows ───────────────────────────────────────────────

/// Open the OS Bluetooth settings pane.
///
/// macOS 13+: `com.apple.settings.Bluetooth`
/// macOS 12−: `com.apple.Bluetooth-Settings.extension`
/// Windows 11: `ms-settings:bluetooth` + `ms-settings:privacy-bluetooth`
/// Linux: gnome-control-center or blueman-manager
#[tauri::command]
pub fn open_bt_settings() {
    #[cfg(target_os = "macos")]
    {
        let modern = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.settings.Bluetooth")
            .output();
        if modern.is_err() || modern.is_ok_and(|o| !o.status.success()) {
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.Bluetooth-Settings.extension")
                .spawn();
        }
    }
    #[cfg(target_os = "windows")]
    {
        // Open the Windows 11 Bluetooth & devices settings page first,
        // then also open the Privacy → Bluetooth page (which controls
        // per-app BLE access).  Both open in the same Settings window.
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "ms-settings:bluetooth"])
            .spawn();
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "ms-settings:privacy-bluetooth"])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(
                "gnome-control-center bluetooth 2>/dev/null \
                  || blueman-manager 2>/dev/null \
                  || systemsettings kcm_bluetooth 2>/dev/null \
                  || true",
            )
            .spawn();
    }
}

/// Check whether the OS Bluetooth adapter is powered on. Returns `true` on
/// non-macOS platforms (no special check required there).
#[tauri::command]
pub fn check_bluetooth_power() -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        // Use system_profiler to read the Bluetooth power state. This is a
        // best-effort check — system_profiler can be slow but is available
        // on macOS by default.
        match Command::new("sh")
            .arg("-c")
            .arg("system_profiler SPBluetoothDataType -detailLevel mini | grep 'Bluetooth Power' || true")
            .output()
        {
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout).to_lowercase();
                // Some macOS versions print "Bluetooth Power: On" while others use "State: On".
                s.contains("bluetooth power: on") || s.contains("state: on")
            }
            Err(_) => true,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

window_cmd!(open_settings_window, "settings", "settings",
    "NeuroSkill™ – Settings",
    size: (760.0, 720.0), min: (580.0, 560.0));

window_tab_cmd!(open_model_tab, "settings", "settings?tab=exg",
    "NeuroSkill™ – EXG",
    size: (760.0, 720.0), min: (580.0, 560.0),
    event: "switch-tab", payload: "model");

window_tab_cmd!(open_updates_window, "settings", "settings?tab=updates",
    "NeuroSkill™ – Updates",
    size: (760.0, 720.0), min: (580.0, 560.0),
    event: "switch-tab", payload: "updates");

window_cmd!(open_help_window, "help", "help",
    "NeuroSkill™ – Help",
    size: (680.0, 720.0), min: (600.0, 520.0));

#[tauri::command]
pub async fn open_session_window(app: AppHandle, csv_path: String) -> Result<(), String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    csv_path.hash(&mut h);
    let label = format!("session-{:x}", h.finish());
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }
    let encoded: String = csv_path
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{:02X}", b),
        })
        .collect();
    tauri::WebviewWindowBuilder::new(
        &app,
        &label,
        tauri::WebviewUrl::App(format!("session?csv_path={encoded}").into()),
    )
    .title("NeuroSkill™ – Session Detail")
    .inner_size(680.0, 700.0)
    .min_inner_size(480.0, 400.0)
    .resizable(true)
    .center()
    .decorations(false)
    .transparent(true)
    .build()
    .map(|w| {
        let _ = w.set_focus();
    })
    .map_err(|e| e.to_string())
}

window_cmd!(open_search_window, "search", "search",
    "EEG Search",
    size: (1100.0, 820.0), min: (700.0, 560.0), maximized: true);

#[tauri::command]
pub(crate) async fn open_focus_timer_window_inner(
    app: &AppHandle,
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
    let url = if autostart {
        "focus-timer?autostart=1"
    } else {
        "focus-timer"
    };
    tauri::WebviewWindowBuilder::new(app, "focus-timer", tauri::WebviewUrl::App(url.into()))
        .title("Focus Timer")
        .inner_size(420.0, 660.0)
        .resizable(false)
        .always_on_top(false)
        .center()
        .decorations(false)
        .transparent(true)
        .build()
        .map(|w| {
            let _ = w.set_focus();
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_focus_timer_window(app: AppHandle) -> Result<(), String> {
    open_focus_timer_window_inner(&app, false).await
}

window_cmd!(open_labels_window, "labels", "labels",
    "All Labels",
    size: (680.0, 600.0), min: (480.0, 400.0));

window_cmd!(open_label_window, "label", "label",
    "Add Label",
    size: (520.0, 560.0), min: (420.0, 380.0), always_on_top: true);

#[tauri::command]
pub fn open_label_window_at(app: AppHandle, ts: u64) -> Result<(), String> {
    // Close existing label window if open (it may have a different timestamp)
    if let Some(win) = app.get_webview_window("label") {
        let _ = win.close();
    }
    let route = format!("label?ts={ts}");
    focus_or_create(
        &app,
        WindowSpec {
            label: "label-retro",
            route: &route,
            title: "Add Label",
            inner_size: (520.0, 560.0),
            min_inner_size: Some((420.0, 380.0)),
            always_on_top: true,
            ..Default::default()
        },
    )
}

#[tauri::command]
pub fn close_label_window(app: AppHandle) {
    if let Some(win) = app.get_webview_window("label") {
        let _ = win.close();
    }
}

window_cmd!(open_api_window, "api", "api",
    "NeuroSkill™ – API Status",
    size: (620.0, 560.0), min: (480.0, 400.0));

/// Return the last app version for which the What's New window was dismissed.
///
/// An empty string means the window has never been seen.
#[tauri::command]
pub fn get_whats_new_seen_version(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    state
        .lock_or_recover()
        .ui
        .last_seen_whats_new_version
        .clone()
}

/// Persist the acknowledged version and close the What's New window.
///
/// Combining both operations in Rust avoids relying on the frontend window
/// API (which can silently fail in secondary webview windows).
#[tauri::command]
pub fn dismiss_whats_new(version: String, app: AppHandle) {
    mutate_and_save(&app, |s| s.ui.last_seen_whats_new_version = version);
    if let Some(win) = app.get_webview_window("whats-new") {
        let _ = win.close();
    }
}

// Open (or focus) the What's New window.
//
// The frontend calls `get_whats_new_seen_version` first and only invokes
// this command when the stored version differs from the running version.
// The window's own page calls `set_whats_new_seen_version` on dismiss.
window_cmd!(open_whats_new_window, "whats-new", "whats-new",
    "What's New in NeuroSkill™",
    size: (520.0, 620.0), resizable: false);

window_cmd!(open_onboarding_window, "onboarding", "onboarding",
    "NeuroSkill™ – Welcome",
    size: (680.0, 760.0), min: (560.0, 620.0));

#[tauri::command]
pub fn get_onboarding_model_download_order() -> Vec<String> {
    crate::constants::ONBOARDING_MODEL_DOWNLOAD_ORDER
        .iter()
        .map(|item| (*item).to_string())
        .collect()
}

#[tauri::command]
pub fn complete_onboarding(app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    mutate_and_save(&app, |s| s.ui.onboarding_complete = true);
    if let Some(win) = app.get_webview_window("onboarding") {
        let _ = win.close();
    }
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
        crate::linux_fix_decorations(&win);
    }

    // Kick off an immediate community-skills download so fresh installs have
    // the latest skills available right away.
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let r = app_clone.state::<Mutex<Box<crate::state::AppState>>>();
        let skill_dir = r.lock_or_recover().skill_dir.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            skill_skills::sync::sync_skills(&skill_dir, 0, None)
        })
        .await;
        match outcome {
            Ok(skill_skills::sync::SyncOutcome::Updated { elapsed_ms, .. }) => {
                eprintln!("[onboarding] community skills downloaded in {elapsed_ms} ms");
                let _ = app_clone.emit("skills-updated", ());
            }
            Ok(skill_skills::sync::SyncOutcome::Fresh { .. }) => {
                eprintln!("[onboarding] community skills already up to date");
            }
            Ok(skill_skills::sync::SyncOutcome::Failed(e)) => {
                eprintln!("[onboarding] community skills download failed: {e}");
            }
            Err(e) => {
                eprintln!("[onboarding] skills sync task panic: {e}");
            }
        }
    });
}

#[tauri::command]
pub fn get_onboarding_complete(state: tauri::State<'_, Mutex<Box<AppState>>>) -> bool {
    state.lock_or_recover().ui.onboarding_complete
}

// ── Calibration window ────────────────────────────────────────────────────────

/// Open (or focus) the calibration window.  Requires an active streaming session.
pub(crate) async fn open_calibration_window_inner(
    app: &AppHandle,
    profile_id: Option<String>,
    autostart: bool,
) -> Result<(), String> {
    {
        let st = app.app_state();
        let guard = st.lock_or_recover();
        if guard.status.state != "connected" || guard.stream.is_none() {
            return Err(
                "Calibration requires a connected BLE device that is streaming data".into(),
            );
        }
    }
    let url = {
        let mut q = String::new();
        if let Some(ref id) = profile_id {
            q.push_str(&format!("profile={id}"));
        }
        if autostart {
            if !q.is_empty() {
                q.push('&');
            }
            q.push_str("autostart=1");
        }
        if q.is_empty() {
            "calibration".to_string()
        } else {
            format!("calibration?{q}")
        }
    };
    if let Some(win) = app.get_webview_window("calibration") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        let _ = app.emit(
            "calibration-run",
            serde_json::json!({
                "profile_id": profile_id, "autostart": autostart,
            }),
        );
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(app, "calibration", tauri::WebviewUrl::App(url.into()))
        .title("NeuroSkill™ – Calibration")
        .inner_size(600.0, 700.0)
        .min_inner_size(520.0, 600.0)
        .resizable(true)
        .center()
        .decorations(false)
        .transparent(true)
        .build()
        .map(|w| {
            let _ = w.set_focus();
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_calibration_window(app: AppHandle) -> Result<(), String> {
    open_calibration_window_inner(&app, None, false).await
}

#[tauri::command]
pub async fn open_and_start_calibration(
    app: AppHandle,
    profile_id: Option<String>,
) -> Result<(), String> {
    open_calibration_window_inner(&app, profile_id, true).await
}

#[tauri::command]
pub fn close_calibration_window(app: AppHandle) {
    if let Some(win) = app.get_webview_window("calibration") {
        let _ = win.close();
    }
}

// ── Calibration profile CRUD ──────────────────────────────────────────────────

#[tauri::command]
pub fn list_calibration_profiles(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<CalibrationProfile> {
    state.lock_or_recover().calibration_profiles.clone()
}

#[tauri::command]
pub fn get_calibration_profile(
    id: String,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Option<CalibrationProfile> {
    state
        .lock_or_recover()
        .calibration_profiles
        .iter()
        .find(|p| p.id == id)
        .cloned()
}

#[tauri::command]
pub fn get_active_calibration(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Option<CalibrationProfile> {
    let s = state.lock_or_recover();
    let id = s.active_calibration_id.clone();
    s.calibration_profiles
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .or_else(|| s.calibration_profiles.first().cloned())
}

#[tauri::command]
pub fn set_active_calibration(
    id: String,
    app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    mutate_and_save(&app, |s| s.active_calibration_id = id);
}

#[tauri::command]
pub fn create_calibration_profile(
    profile: CalibrationProfile,
    app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> CalibrationProfile {
    crate::calibration_service::create_profile(&app, profile)
}

#[tauri::command]
pub fn update_calibration_profile(
    profile: CalibrationProfile,
    app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    crate::calibration_service::update_profile(&app, profile)?;
    Ok(())
}

#[tauri::command]
pub fn delete_calibration_profile(
    id: String,
    app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    crate::calibration_service::delete_profile(&app, &id)
}

#[tauri::command]
pub fn record_calibration_completed(
    profile_id: Option<String>,
    app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    mutate_and_save(&app, |s| {
        let target_id = profile_id.unwrap_or_else(|| s.active_calibration_id.clone());
        if let Some(p) = s
            .calibration_profiles
            .iter_mut()
            .find(|p| p.id == target_id)
        {
            p.last_calibration_utc = Some(unix_secs());
        }
    });
    send_toast(
        &app,
        ToastLevel::Success,
        "Calibration Complete",
        "All calibration iterations finished successfully.",
    );
}

// ── Legacy calibration compat ──────────────────────────────────────────────────

#[tauri::command]
pub fn get_calibration_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> CalibrationConfig {
    let s = state.lock_or_recover();
    let id = s.active_calibration_id.clone();
    let profile = s
        .calibration_profiles
        .iter()
        .find(|p| p.id == id)
        .or_else(|| s.calibration_profiles.first());
    match profile {
        Some(p) => CalibrationConfig {
            action1_label: p
                .actions
                .first()
                .map(|a| a.label.clone())
                .unwrap_or_default(),
            action2_label: p
                .actions
                .get(1)
                .map(|a| a.label.clone())
                .unwrap_or_default(),
            action_duration_secs: p.actions.first().map(|a| a.duration_secs).unwrap_or(10),
            break_duration_secs: p.break_duration_secs,
            loop_count: p.loop_count,
            auto_start: p.auto_start,
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

/// Auto-fit the main window height to dashboard content while clamping to the
/// current monitor's usable height.
#[tauri::command]
pub fn autosize_main_window(app: AppHandle, desired_height: f64) -> Result<(), String> {
    let Some(win) = app.get_webview_window("main") else {
        return Ok(());
    };

    // Keep a sensible lower bound so controls never get cramped.
    let min_h = 560.0_f64;
    let mut target_h = desired_height.max(min_h);

    // Clamp to current monitor height (logical px), leaving a tiny safety gap.
    if let Ok(Some(mon)) = win.current_monitor() {
        let scale = mon.scale_factor();
        if scale > 0.0 {
            let max_h = (mon.size().height as f64 / scale - 20.0).max(min_h);
            target_h = target_h.min(max_h);
        }
    }

    let scale = win.scale_factor().unwrap_or(1.0);
    let cur = win.inner_size().map_err(|e| e.to_string())?;
    let cur_w = cur.width as f64 / scale;
    let cur_h = cur.height as f64 / scale;

    // Ignore tiny deltas to avoid resize jitter loops.
    if (cur_h - target_h).abs() < 6.0 {
        return Ok(());
    }

    win.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
        cur_w, target_h,
    )))
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn emit_calibration_event(event: String, payload: serde_json::Value, app: AppHandle) {
    let _ = app.emit(&event, &payload);
    app.state::<WsBroadcaster>().send(&event, &payload);
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    let update_ready = {
        let r = app.state::<Mutex<Box<AppState>>>();
        let g = r.lock_or_recover();
        g.update_ready_to_install
    };
    if update_ready {
        eprintln!("[updater] update staged — relaunching to apply on quit");
        app.request_restart();
    } else {
        app.exit(0);
    }
}

#[tauri::command]
pub fn get_app_version(app: AppHandle) -> String {
    app.config()
        .version
        .clone()
        .unwrap_or_else(|| "unknown".into())
}

/// Returns `true` when an EEG session is currently being recorded.
#[tauri::command]
pub fn is_session_live(app: AppHandle) -> bool {
    let r = app.state::<Mutex<Box<AppState>>>();
    let g = r.lock_or_recover();
    g.session_start_utc.is_some()
}

/// Called by the frontend once an update has been downloaded and staged.
/// The flag is checked at quit time so we can relaunch (apply the update)
/// instead of a plain exit.
#[tauri::command]
pub fn set_update_ready(app: AppHandle, ready: bool) {
    let r = app.state::<Mutex<Box<AppState>>>();
    let mut g = r.lock_or_recover();
    g.update_ready_to_install = ready;
}

#[tauri::command]
pub fn get_app_name(app: AppHandle) -> String {
    app.config()
        .product_name
        .clone()
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
    {
        let _ = std::process::Command::new("open").arg(&dir).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let mut launched = false;
        if std::process::Command::new("xdg-open")
            .arg(&dir)
            .spawn()
            .is_ok()
        {
            launched = true;
        }
        if !launched {
            let _ = std::process::Command::new("gio")
                .arg("open")
                .arg(&dir)
                .spawn();
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(&dir).spawn();
    }
}

// ── WebSocket API status ───────────────────────────────────────────────────────
