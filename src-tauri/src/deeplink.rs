// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Handles `neuroskill://` deep-link URLs from widget taps.
// Maps URL paths to SvelteKit frontend routes and navigates the main window.

use tauri::Manager;

/// Map a `neuroskill://` URL path to a SvelteKit route.
fn route_for_path(path: &str) -> &str {
    match path.trim_matches('/') {
        "dashboard" | "" => "/",
        "devices" => "/settings", // device pairing lives in settings
        "activity" => "/history",
        "session" => "/session",
        "heart-rate" => "/", // dashboard shows PPG metrics
        "settings" => "/settings",
        "calibration" => "/calibration",
        "focus-timer" => "/focus-timer",
        "compare" => "/compare",
        _ => "/",
    }
}

/// Handle an incoming `neuroskill://` deep-link URL.
///
/// Shows the main window and navigates it to the matching frontend route.
/// Called from the Tauri URL event handler registered in `setup.rs`.
pub fn handle_deep_link(app: &tauri::AppHandle, url: &str) {
    // Parse the path component from the URL
    let path = url
        .strip_prefix("neuroskill://")
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");

    let route = route_for_path(path);

    // Show + focus the main window
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();

        // Navigate to the route via JS
        let js = format!(
            "if (typeof window.__skill_navigate === 'function') {{ \
                window.__skill_navigate('{}'); \
            }} else {{ \
                window.location.hash = '{}'; \
            }}",
            route, route
        );
        let _ = win.eval(&js);
    }
}
