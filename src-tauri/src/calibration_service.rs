// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Shared calibration profile CRUD operations.
//!
//! Both the Tauri IPC commands and daemon-facing adapters delegate to the
//! functions in this module so mutation logic is defined exactly once.

use tauri::AppHandle;

use crate::{new_profile_id, save_settings, AppStateExt, CalibrationProfile, MutexExt};

/// Create a new calibration profile, persist settings, and return the profile.
pub(crate) fn create_profile(
    app: &AppHandle,
    mut profile: CalibrationProfile,
) -> CalibrationProfile {
    profile.id = new_profile_id();
    profile.last_calibration_utc = None;
    let ret = profile.clone();
    {
        let st = app.app_state();
        let mut s = st.lock_or_recover();
        s.calibration_profiles.push(profile);
    }
    save_settings(app);
    ret
}

/// Update an existing calibration profile by ID.  Returns the updated profile.
pub(crate) fn update_profile(
    app: &AppHandle,
    profile: CalibrationProfile,
) -> Result<CalibrationProfile, String> {
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

/// Delete a calibration profile by ID.  Refuses to delete the last profile.
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
    Ok(())
}
