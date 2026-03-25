// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! WebSocket calendar commands.
//!
//! All timestamps are UTC unix seconds (i64).
//!
//! ## Commands
//!
//! ### `calendar_events`
//! Fetch calendar events that overlap a time range.
//!
//! ```json
//! { "command": "calendar_events", "start_utc": 1742860800, "end_utc": 1742947200 }
//! ```
//! Response:
//! ```json
//! { "events": [ { "id": "...", "title": "...", "start_utc": ..., ... } ] }
//! ```
//!
//! ### `calendar_status`
//! Return the calendar access authorisation status and platform.
//!
//! ```json
//! { "command": "calendar_status" }
//! ```
//! Response:
//! ```json
//! { "status": "authorized", "platform": "macos" }
//! ```
//!
//! ### `calendar_request_permission`
//! Prompt the user to grant calendar access (macOS only, no-op elsewhere).
//!
//! ```json
//! { "command": "calendar_request_permission" }
//! ```
//! Response:
//! ```json
//! { "granted": true, "status": "authorized" }
//! ```

use serde_json::Value;
use tauri::AppHandle;

// ── calendar_events ───────────────────────────────────────────────────────────

/// Fetch calendar events overlapping `[start_utc, end_utc]`.
pub fn calendar_events(_app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let start_utc = msg
        .get("start_utc")
        .and_then(serde_json::Value::as_i64)
        .ok_or("missing required field: \"start_utc\" (i64)")?;
    let end_utc = msg
        .get("end_utc")
        .and_then(serde_json::Value::as_i64)
        .ok_or("missing required field: \"end_utc\" (i64)")?;

    if end_utc < start_utc {
        return Err("\"end_utc\" must be >= \"start_utc\"".into());
    }

    let events = skill_calendar::fetch_events(start_utc, end_utc)
        .map_err(|e| format!("calendar fetch error: {e}"))?;

    let json_events = serde_json::to_value(&events).map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "events": json_events,
        "count":  events.len(),
    }))
}

// ── calendar_status ───────────────────────────────────────────────────────────

/// Return the current calendar access authorisation status and platform name.
pub fn calendar_status(_app: &AppHandle) -> Result<Value, String> {
    let status = skill_calendar::auth_status();
    let status_str = auth_status_str(status);

    let platform = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };

    Ok(serde_json::json!({
        "status":   status_str,
        "platform": platform,
    }))
}

// ── calendar_request_permission ───────────────────────────────────────────────

/// Request calendar access (macOS: shows system dialog; other platforms: no-op).
pub fn calendar_request_permission(_app: &AppHandle) -> Result<Value, String> {
    let granted = skill_calendar::request_access();
    let status = skill_calendar::auth_status();
    Ok(serde_json::json!({
        "granted": granted,
        "status":  auth_status_str(status),
    }))
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn auth_status_str(s: skill_calendar::AuthStatus) -> &'static str {
    match s {
        skill_calendar::AuthStatus::Authorized => "authorized",
        skill_calendar::AuthStatus::Denied => "denied",
        skill_calendar::AuthStatus::Restricted => "restricted",
        skill_calendar::AuthStatus::NotDetermined => "not_determined",
    }
}
