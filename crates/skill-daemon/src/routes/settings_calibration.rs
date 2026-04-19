// SPDX-License-Identifier: GPL-3.0-only
//! Calibration profile CRUD routes (daemon-authoritative).
//!
//! Profiles are stored in settings.json (matching the existing Tauri model)
//! and served via REST so the Tauri client can be a thin proxy.

use axum::{extract::State, Json};
use serde::Deserialize;
use skill_settings::CalibrationProfile;

use crate::{
    routes::settings_io::{load_user_settings, save_user_settings},
    state::AppState,
};

fn new_profile_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub(crate) async fn list_profiles(State(state): State<AppState>) -> Json<Vec<CalibrationProfile>> {
    let settings = load_user_settings(&state);
    Json(settings.calibration_profiles)
}

pub(crate) async fn get_active_profile_id(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"value": settings.active_calibration_id}))
}

#[derive(Deserialize)]
pub(crate) struct SetActiveRequest {
    pub(crate) id: String,
}

pub(crate) async fn set_active_profile(
    State(state): State<AppState>,
    Json(req): Json<SetActiveRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.active_calibration_id = req.id;
    save_user_settings(&state, &settings);
    state.broadcast("calibration-changed", serde_json::json!({"action": "set-active"}));
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn create_profile(
    State(state): State<AppState>,
    Json(mut profile): Json<CalibrationProfile>,
) -> Json<CalibrationProfile> {
    profile.id = new_profile_id();
    profile.last_calibration_utc = None;
    let ret = profile.clone();

    let mut settings = load_user_settings(&state);
    settings.calibration_profiles.push(profile);
    save_user_settings(&state, &settings);
    state.broadcast("calibration-changed", serde_json::json!({"action": "created"}));
    Json(ret)
}

pub(crate) async fn update_profile(
    State(state): State<AppState>,
    Json(profile): Json<CalibrationProfile>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    if let Some(entry) = settings.calibration_profiles.iter_mut().find(|p| p.id == profile.id) {
        *entry = profile;
        save_user_settings(&state, &settings);
        state.broadcast("calibration-changed", serde_json::json!({"action": "updated"}));
        Json(serde_json::json!({"ok": true}))
    } else {
        Json(serde_json::json!({"ok": false, "error": "profile not found"}))
    }
}

#[derive(Deserialize)]
pub(crate) struct DeleteProfileRequest {
    pub(crate) id: String,
}

/// Returns the profile ID to auto-start calibration for, if any.
/// Called once by Tauri at startup to check if the active profile has auto_start enabled.
pub(crate) async fn auto_start_pending(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    let active_id = &settings.active_calibration_id;
    let profile_id = settings
        .calibration_profiles
        .iter()
        .find(|p| p.id == *active_id && p.auto_start)
        .map(|p| p.id.clone());
    Json(serde_json::json!({"profile_id": profile_id}))
}

pub(crate) async fn delete_profile(
    State(state): State<AppState>,
    Json(req): Json<DeleteProfileRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    if settings.calibration_profiles.len() <= 1 {
        return Json(serde_json::json!({"ok": false, "error": "Cannot delete the last calibration profile"}));
    }
    settings.calibration_profiles.retain(|p| p.id != req.id);
    if settings.active_calibration_id == req.id {
        settings.active_calibration_id = settings
            .calibration_profiles
            .first()
            .map(|p| p.id.clone())
            .unwrap_or_default();
    }
    save_user_settings(&state, &settings);
    state.broadcast("calibration-changed", serde_json::json!({"action": "deleted"}));
    Json(serde_json::json!({"ok": true}))
}

// ── Session control routes ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct StartSessionRequest {
    pub(crate) profile_id: String,
}

pub(crate) async fn start_session(
    State(state): State<AppState>,
    Json(req): Json<StartSessionRequest>,
) -> Json<serde_json::Value> {
    match crate::calibration_runner::spawn_session(&state, &req.profile_id) {
        Ok(()) => Json(serde_json::json!({"ok": true})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": e})),
    }
}

pub(crate) async fn cancel_session(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cancelled = crate::calibration_runner::cancel_session(&state);
    Json(serde_json::json!({"ok": true, "was_running": cancelled}))
}

pub(crate) async fn session_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let snap = state.calibration_phase.lock().unwrap().clone();
    Json(serde_json::to_value(&snap).unwrap_or_default())
}

#[derive(Deserialize)]
pub(crate) struct RecordCompletedRequest {
    pub(crate) profile_id: Option<String>,
}

pub(crate) async fn record_completed(
    State(state): State<AppState>,
    Json(req): Json<RecordCompletedRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    let target_id = req.profile_id.unwrap_or_else(|| settings.active_calibration_id.clone());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Some(p) = settings.calibration_profiles.iter_mut().find(|p| p.id == target_id) {
        p.last_calibration_utc = Some(now);
        save_user_settings(&state, &settings);
        state.broadcast("calibration-changed", serde_json::json!({"action": "completed"}));
        Json(serde_json::json!({"ok": true}))
    } else {
        Json(serde_json::json!({"ok": false, "error": "profile not found"}))
    }
}

/// Returns the full active CalibrationProfile (not just the ID).
pub(crate) async fn get_active_profile(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    let active_id = &settings.active_calibration_id;
    let profile = settings
        .calibration_profiles
        .iter()
        .find(|p| p.id == *active_id)
        .or_else(|| settings.calibration_profiles.first())
        .cloned();
    Json(serde_json::to_value(&profile).unwrap_or(serde_json::Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use tempfile::TempDir;

    fn mk_state() -> (TempDir, AppState) {
        let td = TempDir::new().unwrap();
        // Write default settings with a default calibration profile.
        let mut settings = skill_settings::UserSettings::default();
        settings.calibration_profiles = vec![CalibrationProfile::default()];
        let path = skill_settings::settings_path(td.path());
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        std::fs::write(&path, serde_json::to_string_pretty(&settings).unwrap()).unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        (td, state)
    }

    #[tokio::test]
    async fn list_profiles_returns_defaults() {
        let (_td, state) = mk_state();
        let res = list_profiles(State(state)).await.0;
        assert!(!res.is_empty());
    }

    #[tokio::test]
    async fn create_and_delete_profile() {
        let (_td, state) = mk_state();
        let profile = CalibrationProfile {
            id: String::new(),
            name: "Test".into(),
            ..CalibrationProfile::default()
        };
        let created = create_profile(State(state.clone()), Json(profile)).await.0;
        assert!(!created.id.is_empty());

        // Now there are 2 profiles (default + created), so we can delete.
        let del_req = DeleteProfileRequest { id: created.id.clone() };
        let res = delete_profile(State(state.clone()), Json(del_req)).await.0;
        assert_eq!(res["ok"], true);
    }
}
