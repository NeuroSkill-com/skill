// SPDX-License-Identifier: GPL-3.0-only
//! Server lifecycle commands — thin proxies to the daemon.
//!
//! The daemon owns the LLM server.  These Tauri commands exist only as
//! fallbacks when the `daemonInvoke` HTTP path fails (invoke-proxy.ts).
//! They forward to the daemon and return the response as-is.

use std::sync::Mutex;
use tauri::AppHandle;

use crate::llm::LlmStatus;
use crate::AppState;

#[derive(serde::Serialize)]
pub struct LlmServerStatusResponse {
    pub status: LlmStatus,
    pub model_name: String,
    pub n_ctx: usize,
    pub supports_vision: bool,
    pub supports_tools: bool,
    pub start_error: Option<String>,
}

#[tauri::command]
pub fn get_llm_server_status(
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> LlmServerStatusResponse {
    let v = crate::daemon_cmds::llm_server_status().unwrap_or_default();
    LlmServerStatusResponse {
        status: match v
            .get("status")
            .and_then(|x| x.as_str())
            .unwrap_or("stopped")
            .to_ascii_lowercase()
            .as_str()
        {
            "running" => LlmStatus::Running,
            "loading" => LlmStatus::Loading,
            _ => LlmStatus::Stopped,
        },
        model_name: v
            .get("model_name")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        n_ctx: v
            .get("n_ctx")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize,
        supports_vision: v
            .get("supports_vision")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        supports_tools: v
            .get("supports_tools")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        start_error: v
            .get("start_error")
            .and_then(|x| x.as_str())
            .map(std::string::ToString::to_string),
    }
}

#[tauri::command]
pub fn get_llm_logs(_state: tauri::State<'_, Mutex<Box<AppState>>>) -> serde_json::Value {
    crate::daemon_cmds::llm_server_logs()
        .map(|v| serde_json::Value::Array(v))
        .unwrap_or(serde_json::Value::Array(Vec::new()))
}

#[tauri::command]
pub fn start_llm_server(
    _app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<String, String> {
    crate::daemon_cmds::llm_server_start()
}

#[tauri::command]
pub fn stop_llm_server(_app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let _ = crate::daemon_cmds::llm_server_stop();
}

#[tauri::command]
pub fn switch_llm_model(
    filename: String,
    _app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<String, String> {
    crate::daemon_cmds::llm_server_switch_model(filename)
}

#[tauri::command]
pub fn switch_llm_mmproj(
    filename: String,
    _app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<String, String> {
    crate::daemon_cmds::llm_server_switch_mmproj(filename)
}
