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
        // Browser extension pairing — opens settings with pair prompt
        "pair-browser" => "/settings?pair-browser",
        _ => "/",
    }
}

/// Handle an incoming `neuroskill://` deep-link URL.
///
/// Shows the main window and navigates it to the matching frontend route.
/// Called from the Tauri URL event handler registered in `setup.rs`.
pub fn handle_deep_link(app: &tauri::AppHandle, url: &str) {
    // Parse the path and query from the URL
    let stripped = url.strip_prefix("neuroskill://").unwrap_or("");
    let mut parts = stripped.splitn(2, '?');
    let path = parts.next().unwrap_or("");
    let query = parts.next().unwrap_or("");

    let route = route_for_path(path);

    // Special handling for browser pairing — pass session_id + name to the frontend
    let extra = if path.trim_matches('/') == "pair-browser" {
        format!("&{}", query)
    } else {
        String::new()
    };

    // Show + focus the main window
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();

        let full_route = format!("{route}{extra}");
        let js = format!(
            "if (typeof window.__skill_navigate === 'function') {{ \
                window.__skill_navigate('{r}'); \
            }} else {{ \
                window.location.hash = '{r}'; \
            }}",
            r = full_route.replace('\'', "\\'"),
        );
        let _ = win.eval(&js);
    }
}
