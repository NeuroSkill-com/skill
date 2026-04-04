// SPDX-License-Identifier: GPL-3.0-only
//! History commands — delegated to daemon.

use tauri::AppHandle;

#[tauri::command]
pub(crate) async fn open_history_window(app: AppHandle) -> Result<(), String> {
    crate::window_cmds::focus_or_create(
        &app,
        crate::window_cmds::WindowSpec {
            label: "history",
            route: "history",
            title: "NeuroSkill™ – History",
            inner_size: (920.0, 780.0),
            min_inner_size: Some((700.0, 560.0)),
            ..Default::default()
        },
    )
}

#[tauri::command]
pub(crate) async fn list_session_days() -> Result<Vec<serde_json::Value>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub(crate) async fn list_sessions_for_day(_day: String) -> Result<Vec<serde_json::Value>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub(crate) async fn stream_sessions() -> Result<Vec<serde_json::Value>, String> {
    crate::daemon_cmds::fetch_history_sessions()
}
