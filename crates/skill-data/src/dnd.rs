// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! macOS Do Not Disturb / Focus-mode control via the `macos-focus` crate.
//!
//! `macos-focus` is a pure-Rust library that controls Focus Mode by writing
//! to `~/Library/DoNotDisturb/DB/Assertions.json` and posting the appropriate
//! Cocoa distributed notifications + `launchctl kickstart` for `donotdisturbd`.
//! No Swift, no Objective-C, no private frameworks, no special entitlements.
//!
//! On macOS < 12 it falls back to `defaults write` + `killall NotificationCenter`.
//! Linux support is implemented via desktop-native command paths:
//! - GNOME: `gsettings org.gnome.desktop.notifications show-banners`
//! - KDE: `qdbus(6) ... org.kde.osdService.setDoNotDisturb`
//! - Fallback: `xdg-desktop-portal` inhibit request via `gdbus`
//!
//! Windows support uses the per-user notification banner toggle:
//! - `HKCU\Software\Microsoft\Windows\CurrentVersion\PushNotifications\ToastEnabled`

#[cfg(target_os = "macos")]
use skill_constants::DND_CLIENT_ID as CLIENT_ID;

#[cfg(target_os = "linux")]
use skill_constants::DND_LINUX_MODE_ID as LINUX_MODE_ID;

#[cfg(target_os = "windows")]
use skill_constants::DND_WINDOWS_MODE_ID as WINDOWS_MODE_ID;

#[cfg(target_os = "linux")]
static PORTAL_INHIBIT_HANDLE: std::sync::OnceLock<std::sync::Mutex<Option<String>>> = std::sync::OnceLock::new();

// ── Shared types ──────────────────────────────────────────────────────────────

/// A Focus mode entry returned by [`list_focus_modes`].
///
/// Sent to the frontend as JSON: `{ identifier: string, name: string }`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FocusModeOption {
    /// The `modeIdentifier` string stored in `Assertions.json`, e.g.
    /// `"com.apple.donotdisturb.mode.default"` or `"com.apple.focus.work"`.
    pub identifier: String,
    /// Human-readable display name shown in System Settings, e.g.
    /// `"Do Not Disturb"` or `"Work"`.
    pub name: String,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Query the OS directly to see whether any Focus / DND mode is currently
/// active.
///
/// Unlike the `dnd_active` flag in `AppState` — which only tracks what *this
/// app* has set — this reads `~/Library/DoNotDisturb/DB/Assertions.json`
/// on macOS 12+ (or runs `defaults read` on older releases) to get the true
/// OS-level state.  This lets the app detect when DND was activated by another
/// app or by the user manually, and also recover from a previous crash where
/// `dnd_active` was left `false` even though the OS still had DND on.
///
/// Returns `Some(true/false)` on supported platforms, `None` when the platform
/// backend cannot determine OS state.
pub fn query_os_active() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        use macos_focus::FocusManager;
        match FocusManager::new() {
            Ok(mgr) => match mgr.is_active() {
                Ok(v) => Some(v),
                Err(e) => {
                    eprintln!("[dnd] query_os_active failed: {e}");
                    None
                }
            },
            Err(e) => {
                eprintln!("[dnd] query_os_active init failed: {e}");
                None
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        query_linux_dnd_active()
    }
    #[cfg(target_os = "windows")]
    {
        query_windows_dnd_active()
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "linux"), not(target_os = "windows")))]
    None
}

/// Enable or disable a Focus mode by its `modeIdentifier` string.
///
/// `mode_identifier` is only used when `enabled` is `true`; the `disable`
/// path clears whatever mode is active regardless of the identifier.
///
/// Returns `true` on success (or on non-macOS where it is a no-op).
pub fn set_dnd(enabled: bool, mode_identifier: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        use macos_focus::{FocusManager, FocusMode};

        let mgr = match FocusManager::new() {
            Ok(m) => m.with_client_id(CLIENT_ID),
            Err(e) => {
                eprintln!("[dnd] init failed: {e}");
                return false;
            }
        };

        let result = if enabled {
            let mode = FocusMode::from_identifier(mode_identifier);
            eprintln!("[dnd] enabling mode {:?} ({})", mode_identifier, mode);
            mgr.enable(mode)
        } else {
            mgr.disable()
        };

        match result {
            Ok(()) => {
                eprintln!("[dnd] {} OK", if enabled { "enable" } else { "disable" });
                true
            }
            Err(e) => {
                eprintln!("[dnd] {} failed: {e}", if enabled { "enable" } else { "disable" });
                false
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let _ = mode_identifier;
        set_linux_dnd(enabled)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = mode_identifier;
        set_windows_dnd(enabled)
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "linux"), not(target_os = "windows")))]
    {
        let _ = (enabled, mode_identifier);
        true
    }
}

/// Return all Focus modes configured on this Mac, ordered as the OS stores
/// them.
///
/// On macOS 12+ this reads `~/Library/DoNotDisturb/DB/ModeConfigurations.json`
/// and returns one [`FocusModeOption`] per configured mode.  If the file is
/// unreadable the well-known Apple first-party modes are returned as a
/// fallback so the UI always has something sensible to show.
///
/// On macOS < 12 only Do Not Disturb is returned (Focus Mode was introduced
/// in Monterey).
///
/// On non-macOS platforms the list is empty.
pub fn list_focus_modes() -> Vec<FocusModeOption> {
    #[cfg(target_os = "macos")]
    {
        use macos_focus::FocusManager;

        let Ok(mgr) = FocusManager::new() else {
            return builtin_modes();
        };
        match mgr.available_modes() {
            Ok(modes) => modes
                .iter()
                .map(|m| FocusModeOption {
                    identifier: m.identifier().to_owned(),
                    name: m.display_name().to_owned(),
                })
                .collect(),
            Err(_) => builtin_modes(),
        }
    }
    #[cfg(target_os = "linux")]
    {
        vec![FocusModeOption {
            identifier: LINUX_MODE_ID.to_owned(),
            name: "Do Not Disturb".to_owned(),
        }]
    }
    #[cfg(target_os = "windows")]
    {
        vec![FocusModeOption {
            identifier: WINDOWS_MODE_ID.to_owned(),
            name: "Do Not Disturb".to_owned(),
        }]
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "linux"), not(target_os = "windows")))]
    Vec::new()
}

#[cfg(target_os = "linux")]
fn run_cmd(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

#[cfg(target_os = "linux")]
fn run_cmd_ok(program: &str, args: &[&str]) -> bool {
    std::process::Command::new(program)
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn query_linux_dnd_active() -> Option<bool> {
    if let Some(v) = query_gnome_show_banners() {
        return Some(v);
    }
    if let Some(v) = query_kde_dnd() {
        return Some(v);
    }
    query_portal_dnd_local_state()
}

#[cfg(target_os = "linux")]
fn set_linux_dnd(enabled: bool) -> bool {
    let portal_cleared = if enabled { false } else { set_portal_dnd(false) };

    if set_gnome_dnd(enabled) {
        eprintln!("[dnd] linux gnome {} OK", if enabled { "enable" } else { "disable" });
        return true;
    }
    if set_kde_dnd(enabled) {
        eprintln!("[dnd] linux kde {} OK", if enabled { "enable" } else { "disable" });
        return true;
    }

    if enabled && set_portal_dnd(true) {
        eprintln!("[dnd] linux portal enable OK");
        return true;
    }
    if !enabled && portal_cleared {
        eprintln!("[dnd] linux portal disable OK");
        return true;
    }

    eprintln!(
        "[dnd] linux {} failed: no supported desktop backend found",
        if enabled { "enable" } else { "disable" }
    );
    false
}

#[cfg(target_os = "linux")]
fn query_gnome_show_banners() -> Option<bool> {
    let output = run_cmd("gsettings", &["get", "org.gnome.desktop.notifications", "show-banners"])?;
    if output.eq_ignore_ascii_case("true") {
        return Some(false);
    }
    if output.eq_ignore_ascii_case("false") {
        return Some(true);
    }
    None
}

#[cfg(target_os = "linux")]
fn set_gnome_dnd(enabled: bool) -> bool {
    let show_banners = if enabled { "false" } else { "true" };
    run_cmd_ok(
        "gsettings",
        &["set", "org.gnome.desktop.notifications", "show-banners", show_banners],
    )
}

#[cfg(target_os = "linux")]
fn query_kde_dnd() -> Option<bool> {
    for bin in ["qdbus6", "qdbus"] {
        if let Some(v) = run_cmd(
            bin,
            &[
                "org.freedesktop.Notifications",
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications.Inhibited",
            ],
        ) {
            if v.eq_ignore_ascii_case("true") {
                return Some(true);
            }
            if v.eq_ignore_ascii_case("false") {
                return Some(false);
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn set_kde_dnd(enabled: bool) -> bool {
    let value = if enabled { "true" } else { "false" };
    for bin in ["qdbus6", "qdbus"] {
        if run_cmd_ok(
            bin,
            &[
                "org.kde.plasmashell",
                "/org/kde/osdService",
                "org.kde.osdService.setDoNotDisturb",
                value,
            ],
        ) {
            return true;
        }
    }
    false
}

#[cfg(target_os = "linux")]
fn portal_handle_slot() -> &'static std::sync::Mutex<Option<String>> {
    PORTAL_INHIBIT_HANDLE.get_or_init(|| std::sync::Mutex::new(None))
}

#[cfg(target_os = "linux")]
fn query_portal_dnd_local_state() -> Option<bool> {
    let slot = portal_handle_slot();
    let guard = slot.lock().ok()?;
    if guard.is_some() {
        Some(true)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn set_portal_dnd(enabled: bool) -> bool {
    if enabled {
        let slot = portal_handle_slot();
        if let Ok(guard) = slot.lock() {
            if guard.is_some() {
                return true;
            }
        }

        let token = format!("neuroskill_dnd_{}", std::process::id());
        let options = format!("{{'reason': <'NeuroSkill focus automation'>, 'handle_token': <'{token}'>}}");
        let out = run_cmd(
            "gdbus",
            &[
                "call",
                "--session",
                "--dest",
                "org.freedesktop.portal.Desktop",
                "--object-path",
                "/org/freedesktop/portal/desktop",
                "--method",
                "org.freedesktop.portal.Inhibit.Inhibit",
                "com.neuroskill.app",
                "",
                "8",
                &options,
            ],
        );
        let Some(output) = out else {
            return false;
        };
        let Some(handle) = parse_gdbus_object_path(&output) else {
            return false;
        };

        if let Ok(mut guard) = slot.lock() {
            *guard = Some(handle);
            return true;
        }
        false
    } else {
        let slot = portal_handle_slot();
        let handle = {
            let Ok(mut guard) = slot.lock() else {
                return false;
            };
            guard.take()
        };

        let Some(handle) = handle else {
            return false;
        };

        run_cmd_ok(
            "gdbus",
            &[
                "call",
                "--session",
                "--dest",
                "org.freedesktop.portal.Desktop",
                "--object-path",
                &handle,
                "--method",
                "org.freedesktop.portal.Request.Close",
            ],
        )
    }
}

#[cfg(target_os = "linux")]
fn parse_gdbus_object_path(output: &str) -> Option<String> {
    if let Some(start) = output.find("'/") {
        let rem = &output[start + 1..];
        let end = rem.find('\'')?;
        let path = &rem[..end];
        if path.starts_with('/') {
            return Some(path.to_owned());
        }
    }
    if output.starts_with('/') {
        let path = output.split_whitespace().next().unwrap_or("");
        if path.starts_with('/') {
            return Some(path.to_owned());
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn run_cmd_windows(program: &str, args: &[&str]) -> Option<String> {
    use std::os::windows::process::CommandExt;

    // CREATE_NO_WINDOW — prevent console-window flicker when running from a GUI app.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let out = std::process::Command::new(program)
        .creation_flags(CREATE_NO_WINDOW)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(target_os = "windows")]
fn run_cmd_windows_ok(program: &str, args: &[&str]) -> bool {
    use std::os::windows::process::CommandExt;

    // CREATE_NO_WINDOW — prevent console-window flicker when running from a GUI app.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    std::process::Command::new(program)
        .creation_flags(CREATE_NO_WINDOW)
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn parse_reg_dword(output: &str) -> Option<u32> {
    for line in output.lines() {
        if !line.contains("ToastEnabled") {
            continue;
        }
        for token in line.split_whitespace().rev() {
            if let Some(hex) = token.strip_prefix("0x") {
                if let Ok(v) = u32::from_str_radix(hex, 16) {
                    return Some(v);
                }
            }
            if let Ok(v) = token.parse::<u32>() {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn query_windows_dnd_active() -> Option<bool> {
    let out = run_cmd_windows(
        "reg",
        &[
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\PushNotifications",
            "/v",
            "ToastEnabled",
        ],
    )?;

    let enabled = parse_reg_dword(&out)?;
    Some(enabled == 0)
}

#[cfg(target_os = "windows")]
fn set_windows_dnd(enabled: bool) -> bool {
    let value = if enabled { "0" } else { "1" };
    let ok = run_cmd_windows_ok(
        "reg",
        &[
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\PushNotifications",
            "/v",
            "ToastEnabled",
            "/t",
            "REG_DWORD",
            "/d",
            value,
            "/f",
        ],
    );
    if ok {
        eprintln!(
            "[dnd] windows {} OK (ToastEnabled={value})",
            if enabled { "enable" } else { "disable" }
        );
    } else {
        eprintln!("[dnd] windows {} failed", if enabled { "enable" } else { "disable" });
    }
    ok
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Fallback list of first-party Focus modes used when `ModeConfigurations.json`
/// cannot be read (e.g. macOS < 12 or unexpected file layout).
#[cfg(target_os = "macos")]
fn builtin_modes() -> Vec<FocusModeOption> {
    use macos_focus::FocusMode;
    [
        FocusMode::DoNotDisturb,
        FocusMode::Work,
        FocusMode::Personal,
        FocusMode::Sleep,
        FocusMode::Driving,
        FocusMode::Fitness,
        FocusMode::Gaming,
        FocusMode::Mindfulness,
        FocusMode::Reading,
    ]
    .iter()
    .map(|m| FocusModeOption {
        identifier: m.identifier().to_owned(),
        name: m.display_name().to_owned(),
    })
    .collect()
}
