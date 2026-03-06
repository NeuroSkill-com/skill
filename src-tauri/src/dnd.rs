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
//! On all other platforms the functions are no-ops.

// TODO(linux/windows): implement DND for non-macOS platforms.
//
// Linux options to investigate:
//   • org.freedesktop.Notifications.SetInhibited (D-Bus, GNOME ≥ 41 / KDE)
//   • org.freedesktop.portal.Inhibit (xdg-desktop-portal, cross-desktop)
//   • GNOME-specific: `gsettings set org.gnome.desktop.notifications show-banners false`
//   • KDE-specific: `kwriteconfig6 --file plasmanotifyrc …` + `qdbus … configure`
//
// Windows options to investigate:
//   • WinRT Windows.UI.Notifications.NotificationSetting / FocusAssist via
//     IQuietHoursSettings (undocumented COM interface)
//   • Registry: HKCU\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings
//     key `NOC_GLOBAL_SETTING_ALLOW_TOASTS_ABOVE_LOCK` / `TOASTS_ENABLED`
//
// For now, silently succeed so the caller's logic is unaffected on these
// platforms.

const CLIENT_ID: &str = "com.neuroskill.app.dnd";

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
            Ok(m)  => m.with_client_id(CLIENT_ID),
            Err(e) => { eprintln!("[dnd] init failed: {e}"); return false; }
        };

        let result = if enabled {
            let mode = FocusMode::from_identifier(mode_identifier);
            eprintln!("[dnd] enabling mode {:?} ({})", mode_identifier, mode);
            mgr.enable(mode)
        } else {
            mgr.disable()
        };

        match result {
            Ok(())  => { eprintln!("[dnd] {} OK", if enabled { "enable" } else { "disable" }); true }
            Err(e)  => { eprintln!("[dnd] {} failed: {e}", if enabled { "enable" } else { "disable" }); false }
        }
    }
    #[cfg(not(target_os = "macos"))]
    { let _ = (enabled, mode_identifier); true }
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

        let mgr = match FocusManager::new() {
            Ok(m)  => m,
            Err(_) => return builtin_modes(),
        };
        match mgr.available_modes() {
            Ok(modes) => modes.iter().map(|m| FocusModeOption {
                identifier: m.identifier().to_owned(),
                name:       m.display_name().to_owned(),
            }).collect(),
            Err(_) => builtin_modes(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    Vec::new()
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
        name:       m.display_name().to_owned(),
    })
    .collect()
}
