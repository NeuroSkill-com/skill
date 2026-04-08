// SPDX-License-Identifier: GPL-3.0-only
//! Shared settings load/save helpers for daemon routes.

use crate::state::AppState;

pub(crate) fn load_user_settings(state: &AppState) -> skill_settings::UserSettings {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    skill_settings::load_settings(&skill_dir)
}

pub(crate) fn save_user_settings(state: &AppState, settings: &skill_settings::UserSettings) {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let path = skill_settings::settings_path(&skill_dir);
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, json);
    }
}
