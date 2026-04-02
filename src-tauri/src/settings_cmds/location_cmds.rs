// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Location services settings Tauri commands.

use crate::MutexExt;
use std::sync::Mutex;
use tauri::AppHandle;

use crate::AppState;

/// Return whether location services are enabled by the user.
#[tauri::command]
pub fn get_location_enabled(state: tauri::State<'_, Mutex<Box<AppState>>>) -> bool {
    state.lock_or_recover().location_enabled
}

/// Enable or disable location services.
///
/// When enabling:
/// 1. Pre-checks permission synchronously — returns immediately if Denied or
///    Restricted without touching the main thread's run loop.
/// 2. If NotDetermined, spawns a blocking task that requests the macOS
///    permission dialog (fixed to use dispatch_async + semaphore, not
///    dispatch_sync, so CoreLocation delegate callbacks are not deadlocked).
/// 3. Fetches a test fix and returns `{ enabled, permission, fix? }`.
///
/// When disabling, simply turns off the setting and persists.
#[tauri::command]
pub async fn set_location_enabled(
    enabled: bool,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<serde_json::Value, String> {
    use serde_json::json;

    if !enabled {
        state.lock_or_recover().location_enabled = false;
        crate::save_settings(&app);
        return Ok(json!({ "enabled": false }));
    }

    // ── Permission pre-check ──────────────────────────────────────────────
    // auth_status() is a fast property read (dispatch_sync with no run-loop
    // spin).  Check it here so we can return instantly for terminal states
    // without ever entering the 30-second request_access path.
    let auth = tokio::task::spawn_blocking(skill_location::auth_status)
        .await
        .map_err(|e| format!("auth check error: {e}"))?;

    match auth {
        skill_location::LocationAuthStatus::Denied => {
            return Ok(json!({
                "enabled": false,
                "permission": "denied",
                "error": "Location permission was denied. \
                          Enable it in System Settings → Privacy & Security → Location Services.",
            }));
        }
        skill_location::LocationAuthStatus::Restricted => {
            return Ok(json!({
                "enabled": false,
                "permission": "restricted",
                "error": "Location access is restricted on this device.",
            }));
        }
        _ => {}
    }

    // ── Request permission (if needed) + fetch fix ────────────────────────
    let result = tokio::task::spawn_blocking(|| {
        // If still NotDetermined, show the system permission dialog.
        // The underlying ObjC uses dispatch_async + semaphore so the main
        // run loop can service CoreLocation delegate callbacks without deadlock.
        if skill_location::auth_status() == skill_location::LocationAuthStatus::NotDetermined {
            skill_location::request_access(30.0);
        }

        let post_auth = skill_location::auth_status();
        let perm_str = match post_auth {
            skill_location::LocationAuthStatus::Authorized => "authorized",
            skill_location::LocationAuthStatus::Denied => "denied",
            skill_location::LocationAuthStatus::Restricted => "restricted",
            skill_location::LocationAuthStatus::NotDetermined => "not_determined",
        };

        // If permission was ultimately denied, bail early.
        if matches!(
            post_auth,
            skill_location::LocationAuthStatus::Denied
                | skill_location::LocationAuthStatus::Restricted
        ) {
            return json!({
                "enabled": false,
                "permission": perm_str,
                "error": "Location permission denied.",
            });
        }

        match skill_location::fetch_location(10.0) {
            Ok(fix) => json!({
                "enabled": true,
                "permission": perm_str,
                "fix": {
                    "latitude":            fix.latitude,
                    "longitude":           fix.longitude,
                    "source":              format!("{:?}", fix.source),
                    "country":             fix.country,
                    "region":              fix.region,
                    "city":                fix.city,
                    "timezone":            fix.timezone,
                    "horizontal_accuracy": fix.horizontal_accuracy,
                    "altitude":            fix.altitude,
                },
            }),
            Err(e) => json!({
                "enabled": true,
                "permission": perm_str,
                "error": e.to_string(),
            }),
        }
    })
    .await
    .map_err(|e| format!("location task error: {e}"))?;

    let enabled_result = result
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let has_error = result.get("error").is_some();

    // Persist as enabled if we either got a fix or got a non-fatal error
    // (e.g. IP fallback returned ok but no CoreLocation fix).
    if enabled_result || (!has_error && result.get("fix").is_some()) {
        state.lock_or_recover().location_enabled = true;
        crate::save_settings(&app);
    }

    Ok(result)
}

/// Test the location API without changing any setting.
///
/// Returns a JSON object with the location fix or error.
#[tauri::command]
pub async fn test_location() -> Result<serde_json::Value, String> {
    use serde_json::json;

    tokio::task::spawn_blocking(|| match skill_location::fetch_location(10.0) {
        Ok(fix) => Ok(json!({
            "ok":                  true,
            "source":              format!("{:?}", fix.source),
            "latitude":            fix.latitude,
            "longitude":           fix.longitude,
            "country":             fix.country,
            "region":              fix.region,
            "city":                fix.city,
            "timezone":            fix.timezone,
            "horizontal_accuracy": fix.horizontal_accuracy,
            "altitude":            fix.altitude,
        })),
        Err(e) => Ok(json!({
            "ok":    false,
            "error": e.to_string(),
        })),
    })
    .await
    .map_err(|e| format!("location task error: {e}"))?
}
