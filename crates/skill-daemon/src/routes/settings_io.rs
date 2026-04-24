// SPDX-License-Identifier: GPL-3.0-only
//! Shared settings load/save helpers for daemon routes.

use crate::state::AppState;

/// Global lock for settings read-modify-write cycles.
/// Prevents TOCTOU races when multiple handlers modify settings concurrently.
static SETTINGS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    // Bump the generation counter so background threads can detect the change.
    state
        .settings_generation
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Async-safe wrapper: atomically load, modify, and save settings under a
/// lock on the blocking thread pool.  Safe to call from async axum handlers.
/// The closure can return a value to communicate results back to the caller.
pub(crate) async fn modify_settings_blocking<F, R>(state: &AppState, f: F) -> R
where
    F: FnOnce(&mut skill_settings::UserSettings) -> R + Send + 'static,
    R: Send + Default + 'static,
{
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let gen = state.settings_generation.clone();
    tokio::task::spawn_blocking(move || {
        let _guard = SETTINGS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut settings = skill_settings::load_settings(&skill_dir);
        let result = f(&mut settings);
        let path = skill_settings::settings_path(&skill_dir);
        if let Ok(json) = serde_json::to_string_pretty(&settings) {
            let _ = std::fs::write(path, json);
        }
        gen.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        result
    })
    .await
    .unwrap_or_default()
}
