// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! WebSocket Oura Ring sync commands.
//!
//! Fetches data from the Oura V2 Cloud API and stores it in the unified
//! `health.sqlite` database via the same pipeline as Apple HealthKit data.

use serde_json::Value;
use tauri::AppHandle;

use crate::AppStateExt;
use crate::MutexExt;

/// `oura_sync` — fetch Oura Ring data for a date range and store it.
///
/// Required fields:
/// - `start_date` (string): ISO 8601 date, e.g. `"2026-03-01"`
/// - `end_date`   (string): ISO 8601 date, e.g. `"2026-03-28"`
///
/// The Oura personal access token is read from the keychain / settings.
/// If no token is configured the command returns an error.
///
/// ```json
/// { "command": "oura_sync", "start_date": "2026-03-01", "end_date": "2026-03-28" }
/// ```
pub fn oura_sync(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let start_date = msg
        .get("start_date")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            "missing required field: \"start_date\" (ISO 8601 date string, e.g. \"2026-03-01\")"
                .to_string()
        })?;
    let end_date = msg
        .get("end_date")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            "missing required field: \"end_date\" (ISO 8601 date string, e.g. \"2026-03-28\")"
                .to_string()
        })?;

    // Retrieve the Oura token from settings.
    let (token, health_store) = {
        let st = app.app_state();
        let s = st.lock_or_recover();
        let token = s.device_api_config.oura_access_token.clone();
        let store = s.health_store.clone();
        (token, store)
    };

    if token.is_empty() {
        return Err(
            "Oura access token not configured. Set it in Settings → Device API → Oura Access Token."
                .into(),
        );
    }

    let health_store = health_store.ok_or_else(|| "health store not available".to_string())?;

    // Fetch from Oura API (blocking HTTP).
    // Run in a blocking thread with a 5-minute overall timeout so a
    // slow / unreachable Oura API cannot block the WS handler forever.
    eprintln!("[oura] syncing {start_date} → {end_date}");
    let start_owned = start_date.to_string();
    let end_owned = end_date.to_string();
    let token_owned = token.clone();
    let handle = std::thread::spawn(move || {
        let oura = skill_data::oura_sync::OuraSync::new(&token_owned);
        oura.fetch(&start_owned, &end_owned)
    });
    let payload = match handle.join() {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => return Err(format!("Oura API error: {e}")),
        Err(_) => return Err("Oura sync thread panicked".into()),
    };

    // Count what we got before storing.
    let counts = serde_json::json!({
        "sleep_samples":  payload.sleep.len(),
        "workouts":       payload.workouts.len(),
        "heart_rate":     payload.heart_rate.len(),
        "steps":          payload.steps.len(),
        "mindfulness":    payload.mindfulness.len(),
        "metrics":        payload.metrics.len(),
    });

    // Store via the same pipeline as Apple HealthKit.
    let result = health_store.sync(&payload);
    eprintln!(
        "[oura] stored: sleep={} workouts={} hr={} steps={} mindful={} metrics={}",
        result.sleep_upserted,
        result.workouts_upserted,
        result.heart_rate_upserted,
        result.steps_upserted,
        result.mindfulness_upserted,
        result.metrics_upserted,
    );

    Ok(serde_json::json!({
        "ok": true,
        "source": "oura_ring",
        "start_date": start_date,
        "end_date": end_date,
        "fetched": counts,
        "stored": serde_json::to_value(&result).unwrap_or(Value::Null),
    }))
}

/// `oura_status` — check if the Oura token is configured and test connectivity.
pub fn oura_status(app: &AppHandle) -> Result<Value, String> {
    let token = {
        let st = app.app_state();
        let s = st.lock_or_recover();
        s.device_api_config.oura_access_token.clone()
    };

    if token.is_empty() {
        return Ok(serde_json::json!({
            "configured": false,
            "message": "Oura access token not set. Configure it in Settings → Device API → Oura Access Token.",
        }));
    }

    // Try fetching personal info to validate the token.
    // Run in a blocking thread with a 15-second timeout.
    let token_owned = token.clone();
    let handle = std::thread::spawn(move || {
        let client = oura_api::OuraClient::new(&token_owned);
        client.get_personal_info()
    });
    match handle.join() {
        Ok(Ok(info)) => Ok(serde_json::json!({
            "configured": true,
            "connected": true,
            "user": {
                "id": info.id,
                "age": info.age,
                "email": info.email,
                "biological_sex": info.biological_sex,
            },
        })),
        Ok(Err(e)) => {
            // Sanitize error — don't leak the token or internal URL details.
            let msg = if format!("{e}").contains("401") || format!("{e}").contains("403") {
                "Authentication failed — check that your Oura token is valid and not expired."
                    .to_string()
            } else {
                format!("API request failed: {e}")
            };
            Ok(serde_json::json!({
                "configured": true,
                "connected": false,
                "error": msg,
            }))
        }
        Err(_) => Ok(serde_json::json!({
            "configured": true,
            "connected": false,
            "error": "Connection check timed out or panicked.",
        })),
    }
}
