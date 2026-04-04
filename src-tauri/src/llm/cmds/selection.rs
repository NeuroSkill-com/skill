// SPDX-License-Identifier: GPL-3.0-only
//! Active model/mmproj selection — daemon-backed.

use std::sync::Mutex;
use tauri::AppHandle;

use crate::AppState;

#[tauri::command]
pub fn set_llm_active_model(
    filename: String,
    _app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let _ = crate::daemon_cmds::llm_set_active_model(filename);
}

#[tauri::command]
pub fn set_llm_autoload_mmproj(
    enabled: bool,
    _app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let _ = crate::daemon_cmds::llm_set_autoload_mmproj(enabled);
}

#[tauri::command]
pub fn set_llm_active_mmproj(
    filename: String,
    _app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let _ = crate::daemon_cmds::llm_set_active_mmproj(filename);
}
