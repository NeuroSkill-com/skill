// SPDX-License-Identifier: GPL-3.0-only
//! Download lifecycle commands — daemon-backed.

use std::sync::Mutex;
use tauri::AppHandle;

use crate::AppState;

#[tauri::command]
pub fn download_llm_model(
    filename: String,
    _app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let _ = crate::daemon_cmds::llm_download_action("/v1/llm/download/start", filename);
}

#[tauri::command]
pub fn cancel_llm_download(filename: String, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let _ = crate::daemon_cmds::llm_download_action("/v1/llm/download/cancel", filename);
}

#[tauri::command]
pub fn pause_llm_download(filename: String, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let _ = crate::daemon_cmds::llm_download_action("/v1/llm/download/pause", filename);
}

#[tauri::command]
pub fn resume_llm_download(
    filename: String,
    _app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let _ = crate::daemon_cmds::llm_download_action("/v1/llm/download/resume", filename);
}

#[tauri::command]
pub fn delete_llm_model(
    filename: String,
    _app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    crate::daemon_cmds::llm_download_action("/v1/llm/download/delete", filename)
}

#[tauri::command]
pub async fn open_downloads_window(app: AppHandle) -> Result<(), String> {
    crate::window_cmds::focus_or_create(
        &app,
        crate::window_cmds::WindowSpec {
            label: "downloads",
            route: "downloads",
            title: "NeuroSkill™ – Downloads",
            inner_size: (760.0, 620.0),
            min_inner_size: Some((560.0, 420.0)),
            ..Default::default()
        },
    )
}
