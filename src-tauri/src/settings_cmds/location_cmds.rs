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
/// When enabling on macOS:
/// 1. Requests CoreLocation permission (shows native dialog if `NotDetermined`).
/// 2. Tests the location API.
/// 3. Returns the result as a JSON object with `{ enabled, permission, fix? }`.
///
/// When disabling, simply turns off the setting.
#[tauri::command]
pub async fn set_location_enabled(
    enabled: bool,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<serde_json::Value, String> {
    use serde_json::json;

    if !enabled {
        // Disable: just turn it off and persist.
        state.lock_or_recover().location_enabled = false;
        crate::save_settings(&app);
        return Ok(json!({
            "enabled": false,
            "permission": "n/a",
        }));
    }

    // Enable: request permission, test the API, then persist.
    let result = tokio::task::spawn_blocking(|| {
        // Step 1: Check / request permission (macOS only; no-op elsewhere).
        let auth = skill_location::auth_status();
        if auth == skill_location::LocationAuthStatus::NotDetermined {
            skill_location::request_access(30.0);
        }

        let final_auth = skill_location::auth_status();
        let perm_str = match final_auth {
            skill_location::LocationAuthStatus::Authorized => "authorized",
            skill_location::LocationAuthStatus::Denied => "denied",
            skill_location::LocationAuthStatus::Restricted => "restricted",
            skill_location::LocationAuthStatus::NotDetermined => "not_determined",
        };

        // Step 2: Test the location API regardless of CoreLocation status
        // (on non-macOS or denied, this will use IP geolocation fallback).
        match skill_location::fetch_location(10.0) {
            Ok(fix) => json!({
                "enabled": true,
                "permission": perm_str,
                "fix": {
                    "latitude": fix.latitude,
                    "longitude": fix.longitude,
                    "source": format!("{:?}", fix.source),
                    "country": fix.country,
                    "region": fix.region,
                    "city": fix.city,
                    "timezone": fix.timezone,
                    "horizontal_accuracy": fix.horizontal_accuracy,
                    "altitude": fix.altitude,
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

    // If we got a fix (even via IP fallback), enable the setting.
    let got_fix = result.get("fix").is_some();
    let got_error = result.get("error").is_some();

    if got_fix || !got_error {
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
            "ok": true,
            "source": format!("{:?}", fix.source),
            "latitude": fix.latitude,
            "longitude": fix.longitude,
            "country": fix.country,
            "region": fix.region,
            "city": fix.city,
            "timezone": fix.timezone,
            "horizontal_accuracy": fix.horizontal_accuracy,
            "altitude": fix.altitude,
        })),
        Err(e) => Ok(json!({
            "ok": false,
            "error": e.to_string(),
        })),
    })
    .await
    .map_err(|e| format!("location task error: {e}"))?
}
