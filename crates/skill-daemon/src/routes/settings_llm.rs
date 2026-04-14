// SPDX-License-Identifier: GPL-3.0-only
//! LLM-related settings handlers.

use axum::{extract::State, Json};

use crate::{
    routes::{
        settings::StringValueRequest,
        settings_io::{load_user_settings, save_user_settings},
    },
    state::AppState,
};

pub(crate) async fn get_llm_config(State(state): State<AppState>) -> Json<skill_settings::LlmConfig> {
    Json(load_user_settings(&state).llm)
}

pub(crate) async fn set_llm_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::LlmConfig>,
) -> Json<serde_json::Value> {
    // Detect whether n_gpu_layers or ctx_size changed (requires server restart).
    #[cfg(feature = "llm")]
    let prev_gpu_layers = state.llm_config.lock().map(|g| g.n_gpu_layers).unwrap_or(0);
    #[cfg(feature = "llm")]
    let prev_ctx_size = state.llm_config.lock().map(|g| g.ctx_size).ok();

    let mut settings = load_user_settings(&state);
    settings.llm = config.clone();
    save_user_settings(&state, &settings);

    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            *cfg = config.clone();
        }

        let is_running = state.llm_state_cell.lock().ok().and_then(|g| g.clone()).is_some();

        if is_running {
            // Hot-patch allowed tools on the running server.
            if let Ok(guard) = state.llm_state_cell.lock() {
                if let Some(server) = guard.clone() {
                    let prev_port = server.allowed_tools.lock().map(|t| t.skill_api_port).unwrap_or(18445);
                    let mut new_tools = config.tools.clone();
                    new_tools.skill_api_port = prev_port;
                    if !settings.location_enabled {
                        new_tools.location = false;
                    }
                    if let Ok(mut tools) = server.allowed_tools.lock() {
                        *tools = new_tools;
                    }
                }
            }

            // If n_gpu_layers or ctx_size changed, the model must be reloaded.
            let ctx_changed = prev_ctx_size.map_or(false, |prev| prev != config.ctx_size);
            if config.n_gpu_layers != prev_gpu_layers || ctx_changed {
                skill_llm::shutdown_cell(&state.llm_state_cell);
                if let Ok(mut st) = state.llm_status.lock() {
                    *st = "stopped".to_string();
                }
                state.broadcast("llm:status", serde_json::json!({"status": "loading"}));
                let _ = super::settings_llm_runtime::llm_server_start_impl(State(state.clone())).await;
            }
        }
    }

    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn get_inference_device(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": load_user_settings(&state).inference_device}))
}

pub(crate) async fn set_inference_device(
    State(state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    let is_cpu = req.value == "cpu";
    let mut settings = load_user_settings(&state);
    settings.inference_device = if is_cpu { "cpu".into() } else { "gpu".into() };
    if is_cpu {
        let cur_layers = settings.llm.n_gpu_layers;
        if cur_layers != 0 {
            settings.llm_gpu_layers_saved = cur_layers;
        }
        settings.llm.n_gpu_layers = 0;
    } else {
        settings.llm.n_gpu_layers = settings.llm_gpu_layers_saved;
    }
    let out = settings.inference_device.clone();

    // Sync to in-memory llm_config so the next server start picks it up.
    #[cfg(feature = "llm")]
    if let Ok(mut cfg) = state.llm_config.lock() {
        cfg.n_gpu_layers = settings.llm.n_gpu_layers;
    }

    save_user_settings(&state, &settings);

    // If the LLM server is already running, restart it so the device
    // change takes effect immediately.
    #[cfg(feature = "llm")]
    {
        let is_running = state.llm_state_cell.lock().ok().and_then(|g| g.clone()).is_some();
        if is_running {
            // Shut down without persisting enabled=false (this is a restart,
            // not a user-initiated stop).
            skill_llm::shutdown_cell(&state.llm_state_cell);
            if let Ok(mut st) = state.llm_status.lock() {
                *st = "stopped".to_string();
            }
            let _ = super::settings_llm_runtime::llm_server_start_impl(State(state.clone())).await;
        }
    }

    Json(serde_json::json!({"ok": true, "value": out}))
}

pub(crate) async fn get_exg_inference_device(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": load_user_settings(&state).exg_inference_device}))
}

pub(crate) async fn set_exg_inference_device(
    State(state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.exg_inference_device = if req.value == "cpu" { "cpu".into() } else { "gpu".into() };
    let out = settings.exg_inference_device.clone();
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": out}))
}

pub(crate) async fn get_hf_endpoint(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    let endpoint = if settings.hf_endpoint.trim().is_empty() {
        skill_settings::default_hf_endpoint()
    } else {
        settings.hf_endpoint
    };
    Json(serde_json::json!({"value": endpoint}))
}

pub(crate) async fn set_hf_endpoint(
    State(state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.hf_endpoint = if req.value.trim().is_empty() {
        skill_settings::default_hf_endpoint()
    } else {
        req.value.trim().to_string()
    };
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": settings.hf_endpoint}))
}
