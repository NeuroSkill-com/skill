// SPDX-License-Identifier: GPL-3.0-only
//! Catalog query and model registration — daemon-backed.

use serde::Serialize;
use std::sync::Mutex;
use tauri::AppHandle;

use crate::llm::catalog::{DownloadState, LlmCatalog};
use crate::AppState;

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct LlmDownloadItem {
    pub repo: String,
    pub filename: String,
    pub quant: String,
    pub size_gb: f32,
    pub description: String,
    pub is_mmproj: bool,
    pub state: DownloadState,
    pub status_msg: Option<String>,
    pub progress: f32,
    pub initiated_at_unix: Option<u64>,
    pub local_path: Option<std::path::PathBuf>,
    pub shard_count: u16,
    pub current_shard: u16,
}

#[tauri::command]
pub fn get_llm_catalog(_state: tauri::State<'_, Mutex<Box<AppState>>>) -> LlmCatalog {
    crate::daemon_cmds::llm_get_catalog().unwrap_or_default()
}

#[tauri::command]
pub fn get_llm_downloads(_state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<LlmDownloadItem> {
    crate::daemon_cmds::llm_get_downloads()
        .ok()
        .and_then(|v| serde_json::from_value(serde_json::Value::Array(v)).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn add_llm_model(
    repo: String,
    filename: String,
    size_gb: Option<f32>,
    mmproj: Option<String>,
    download: Option<bool>,
    _app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<String, String> {
    crate::daemon_cmds::llm_add_model(repo, filename, size_gb, mmproj, download)
}

#[tauri::command]
pub fn refresh_llm_catalog(_app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let _ = crate::daemon_cmds::llm_refresh_catalog();
}
