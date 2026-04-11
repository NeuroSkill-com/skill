// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Calibration profile CRUD — thin proxy to daemon REST endpoints.
//!
//! All business logic now lives in skill-daemon's settings_calibration routes.
//! Local cache in Tauri AppState is kept in sync for offline resilience.

use tauri::AppHandle;

use crate::{save_settings, AppStateExt, CalibrationProfile, MutexExt};

/// Create a new calibration profile via daemon, update local cache.
pub(crate) fn create_profile(app: &AppHandle, profile: CalibrationProfile) -> CalibrationProfile {
    // Try daemon.
    if let Ok(resp) = crate::daemon_cmds::daemon_post(
        "/v1/calibration/profiles",
        &serde_json::to_value(&profile).unwrap_or_default(),
    ) {
        if let Ok(created) = serde_json::from_value::<CalibrationProfile>(resp) {
            let st = app.app_state();
            let mut s = st.lock_or_recover();
            s.calibration_profiles.push(created.clone());
            drop(s);
            save_settings(app);
            return created;
        }
    }

    // Fallback: create locally.
    let mut p = profile;
    p.id = crate::new_profile_id();
    p.last_calibration_utc = None;
    let ret = p.clone();
    let st = app.app_state();
    let mut s = st.lock_or_recover();
    s.calibration_profiles.push(p);
    drop(s);
    save_settings(app);
    ret
}

/// Update an existing calibration profile by ID.
pub(crate) fn update_profile(
    app: &AppHandle,
    profile: CalibrationProfile,
) -> Result<CalibrationProfile, String> {
    // Notify daemon.
    let _ = crate::daemon_cmds::daemon_post(
        "/v1/calibration/profiles/update",
        &serde_json::to_value(&profile).unwrap_or_default(),
    );

    // Update local cache.
    let st = app.app_state();
    let mut s = st.lock_or_recover();
    let entry = s
        .calibration_profiles
        .iter_mut()
        .find(|p| p.id == profile.id)
        .ok_or_else(|| format!("profile not found: {}", profile.id))?;
    *entry = profile;
    let ret = entry.clone();
    drop(s);
    save_settings(app);
    Ok(ret)
}

/// Delete a calibration profile by ID.
pub(crate) fn delete_profile(app: &AppHandle, id: &str) -> Result<(), String> {
    let st = app.app_state();
    let mut s = st.lock_or_recover();
    if s.calibration_profiles.len() <= 1 {
        return Err("Cannot delete the last calibration profile".into());
    }
    s.calibration_profiles.retain(|p| p.id != id);
    if s.active_calibration_id == id {
        s.active_calibration_id = s
            .calibration_profiles
            .first()
            .map(|p| p.id.clone())
            .unwrap_or_default();
    }
    drop(s);
    save_settings(app);

    // Notify daemon.
    let _ = crate::daemon_cmds::daemon_post(
        "/v1/calibration/profiles/delete",
        &serde_json::json!({"id": id}),
    );
    Ok(())
}
