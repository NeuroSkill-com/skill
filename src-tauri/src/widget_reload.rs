// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Triggers WidgetKit timeline reloads from the Tauri app so desktop
// widgets pick up state changes (device connect, session start/stop,
// flow state change) without waiting for the next scheduled refresh.
//
// Uses the WidgetKit Objective-C API via objc2 — no Swift bridge needed.

#[cfg(target_os = "macos")]
use objc2::msg_send;
#[cfg(target_os = "macos")]
use objc2::runtime::{AnyClass, AnyObject};

/// Ask WidgetKit to reload all widget timelines.
///
/// Safe to call from any thread.  No-op on macOS < 14 or if WidgetKit
/// is not linked (the class lookup returns `None`).
#[cfg(target_os = "macos")]
pub fn reload_all_widgets() {
    std::thread::spawn(|| {
        // WidgetCenter is only available on macOS 14+.  If the class
        // doesn't exist we silently skip — no crash, no log spam.
        let cls: Option<&AnyClass> = AnyClass::get(c"WGWidgetCenter");
        let Some(cls) = cls else { return };

        // SAFETY: msg_send calls WidgetKit ObjC methods. The class existence
        // check above guarantees these selectors are valid.
        unsafe {
            // +[WGWidgetCenter sharedCenter]
            let center: *mut AnyObject = msg_send![cls, sharedCenter];
            if center.is_null() {
                return;
            }
            // -[WGWidgetCenter reloadAllTimelines]
            let _: () = msg_send![center, reloadAllTimelines];
        }
    });
}

/// Show a one-time onboarding notification suggesting widget installation.
/// Called on first launch after the widget extension is embedded.
#[cfg(target_os = "macos")]
pub fn suggest_widgets_onboarding() {
    use std::path::PathBuf;
    let marker = widget_onboarding_marker();
    if marker.exists() {
        return; // already shown
    }
    // Write marker so we only show once
    let _ = std::fs::create_dir_all(marker.parent().unwrap_or(&PathBuf::from(".")));
    let _ = std::fs::write(&marker, "1");

    // The actual notification is sent via tauri-plugin-notification from the
    // setup code. Here we just set the flag and the setup code reads it.
}

#[cfg(target_os = "macos")]
fn widget_onboarding_marker() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("skill")
        .join("daemon")
        .join(".widget-onboarding-shown")
}

/// Returns true if the widget onboarding notification should be shown.
#[cfg(target_os = "macos")]
pub fn should_show_widget_onboarding() -> bool {
    !widget_onboarding_marker().exists()
}

#[cfg(not(target_os = "macos"))]
pub fn reload_all_widgets() {}
