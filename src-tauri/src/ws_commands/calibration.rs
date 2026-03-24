// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! WebSocket calibration profile commands.

use serde_json::Value;
use tauri::AppHandle;

/// `list_calibrations` — return all calibration profiles.
pub fn list_calibrations(app: &AppHandle) -> Result<Value, String> {
    let profiles = crate::calibration_service::list_profiles(app);
    Ok(serde_json::json!({ "profiles": profiles }))
}

/// `get_calibration { "id": "…" }` — return a single profile by ID.
pub fn get_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let id = msg.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"id\" (string)".to_string())?;
    crate::calibration_service::get_profile(app, id)
        .map(|p| serde_json::json!({ "profile": p }))
        .ok_or_else(|| format!("profile not found: {id}"))
}

/// `create_calibration { "name": "…", "actions": […], "break_duration_secs": n, "loop_count": n }`
pub fn create_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let name = msg.get("name").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"name\"".to_string())?
        .trim().to_owned();
    if name.is_empty() { return Err("\"name\" must not be empty".into()); }

    let actions: Vec<crate::CalibrationAction> = msg.get("actions")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .filter(|v: &Vec<_>| !v.is_empty())
        .ok_or_else(|| "\"actions\" must be a non-empty array of {label, duration_secs}".to_string())?;

    let break_secs  = msg.get("break_duration_secs").and_then(serde_json::Value::as_u64).unwrap_or(5) as u32;
    let loop_count  = msg.get("loop_count").and_then(serde_json::Value::as_u64).unwrap_or(3) as u32;
    let auto_start  = msg.get("auto_start").and_then(serde_json::Value::as_bool).unwrap_or(false);

    let profile = crate::CalibrationProfile {
        id:                   String::new(),
        name,
        actions,
        break_duration_secs:  break_secs,
        loop_count,
        auto_start,
        last_calibration_utc: None,
    };

    let created = crate::calibration_service::create_profile(app, profile);
    Ok(serde_json::json!({ "profile": created }))
}

/// `update_calibration { "id": "…", …fields… }` — partial-update an existing profile.
pub fn update_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let id = msg.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"id\"".to_string())?;

    let mut profile = crate::calibration_service::get_profile(app, id)
        .ok_or_else(|| format!("profile not found: {id}"))?;

    if let Some(name) = msg.get("name").and_then(|v| v.as_str()) {
        profile.name = name.to_owned();
    }
    if let Some(actions) = msg.get("actions").and_then(|v| serde_json::from_value::<Vec<crate::CalibrationAction>>(v.clone()).ok()) {
        if !actions.is_empty() { profile.actions = actions; }
    }
    if let Some(b) = msg.get("break_duration_secs").and_then(serde_json::Value::as_u64) {
        profile.break_duration_secs = b as u32;
    }
    if let Some(n) = msg.get("loop_count").and_then(serde_json::Value::as_u64) {
        profile.loop_count = n as u32;
    }
    if let Some(a) = msg.get("auto_start").and_then(serde_json::Value::as_bool) {
        profile.auto_start = a;
    }

    let updated = crate::calibration_service::update_profile(app, profile)?;
    Ok(serde_json::json!({ "profile": updated }))
}

/// `delete_calibration { "id": "…" }` — remove a profile.
pub fn delete_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let id = msg.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"id\"".to_string())?;
    crate::calibration_service::delete_profile(app, id)?;
    Ok(serde_json::json!({}))
}

/// `run_calibration { "id"?: "…" }` — open the calibration window and start
/// the specified (or active) profile immediately.
pub async fn run_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let profile_id = msg.get("id").and_then(|v| v.as_str()).map(str::to_owned);
    crate::open_calibration_window_inner(app, profile_id, true).await?;
    Ok(serde_json::json!({}))
}
