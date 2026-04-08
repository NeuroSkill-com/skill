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
    let mut settings = load_user_settings(&state);
    settings.llm = config.clone();
    save_user_settings(&state, &settings);

    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            *cfg = config.clone();
        }

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
    save_user_settings(&state, &settings);
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
