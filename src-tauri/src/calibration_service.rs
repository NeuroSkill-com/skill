// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Calibration profile CRUD — pure daemon proxy, no local cache.
//!
//! All business logic lives in skill-daemon's settings_calibration routes.

use crate::CalibrationProfile;

/// Create a new calibration profile via daemon.
pub(crate) fn create_profile(profile: CalibrationProfile) -> Result<CalibrationProfile, String> {
    let resp = crate::daemon_cmds::daemon_post(
        "/v1/calibration/profiles",
        &serde_json::to_value(&profile).map_err(|e| e.to_string())?,
    )?;
    serde_json::from_value(resp).map_err(|e| e.to_string())
}

/// Update an existing calibration profile by ID.
pub(crate) fn update_profile(profile: CalibrationProfile) -> Result<(), String> {
    let resp = crate::daemon_cmds::daemon_post(
        "/v1/calibration/profiles/update",
        &serde_json::to_value(&profile).map_err(|e| e.to_string())?,
    )?;
    if resp.get("ok").and_then(|v| v.as_bool()) == Some(true) {
        Ok(())
    } else {
        Err(resp
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("update failed")
            .to_string())
    }
}

/// Delete a calibration profile by ID.
pub(crate) fn delete_profile(id: &str) -> Result<(), String> {
    let resp = crate::daemon_cmds::daemon_post(
        "/v1/calibration/profiles/delete",
        &serde_json::json!({"id": id}),
    )?;
    if resp.get("ok").and_then(|v| v.as_bool()) == Some(true) {
        Ok(())
    } else {
        Err(resp
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("delete failed")
            .to_string())
    }
}
