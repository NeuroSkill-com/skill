// SPDX-License-Identifier: GPL-3.0-only
//! LLM-related settings handlers.

use axum::{extract::State, Json};

use crate::{
    routes::{
        settings::StringValueRequest,
        settings_io::{load_user_settings, modify_settings_blocking},
    },
    state::AppState,
};

crate::settings_struct_get!(skill_settings::LlmConfig, get_llm_config => llm);

/// Points the LLM's `skill` tool at this daemon.
///
/// Port and token are runtime-only (never persisted), so every path that hands
/// a tool config to the LLM must call this. Without it the port stays 0 and the
/// tool answers "tool disabled in settings"; without the token the daemon's
/// command endpoint answers 401. The token is shared by handle, so a default
/// token refresh reaches a running LLM.
#[cfg(feature = "llm")]
pub(crate) fn wire_skill_api(tools: &mut skill_tools::LlmToolConfig, state: &AppState) {
    tools.skill_api_port = crate::handlers::daemon_addr().port();
    tools.skill_api_token = skill_tools::SkillApiToken::shared(state.auth_token.clone());
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
    // The vision-token floor is read when the runner is built, so a change needs
    // a model reload to take effect.
    #[cfg(feature = "llm")]
    let prev_vision_min_tokens = state.llm_config.lock().map(|g| g.vision_min_tokens).ok();

    // Read by the `llm`-gated prompt assembly below.
    #[cfg_attr(not(feature = "llm"), allow(unused_variables))]
    let location_enabled = load_user_settings(&state).location_enabled;
    let config = config.clone();
    let disk_llm = config.clone();
    modify_settings_blocking(&state, move |s| {
        s.llm = disk_llm;
    })
    .await;

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
                    let mut new_tools = config.tools.clone();
                    wire_skill_api(&mut new_tools, &state);
                    if !location_enabled {
                        new_tools.location = false;
                    }
                    if let Ok(mut tools) = server.allowed_tools.lock() {
                        *tools = new_tools;
                    }
                }
            }

            // If n_gpu_layers, ctx_size, or the vision-token floor changed, the
            // model must be reloaded.
            let ctx_changed = prev_ctx_size.is_some_and(|prev| prev != config.ctx_size);
            let vision_changed = prev_vision_min_tokens.is_some_and(|prev| prev != config.vision_min_tokens);
            if config.n_gpu_layers != prev_gpu_layers || ctx_changed || vision_changed {
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

crate::settings_get_value!(get_inference_device => inference_device);

pub(crate) async fn set_inference_device(
    State(state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    let is_cpu = req.value == "cpu";
    // Both runtimes have to be told, because they read different fields:
    // llama.cpp offloads per layer (`n_gpu_layers`), rlx picks one device for
    // the whole graph (`rlx_device`). Writing only the layer count left the rlx
    // runtime on whatever `rlx_device` already said — which is how 0.1.0-rc.3
    // shipped with the default model on CPU while every device setting read
    // "metal". Saving the previous value of each mirrors the long-standing
    // `llm_gpu_layers_saved` behaviour, so a trip through CPU and back does not
    // quietly downgrade someone who had chosen `mlx` or `cuda`.
    let out = modify_settings_blocking(&state, move |s| {
        s.inference_device = if is_cpu { "cpu".into() } else { "gpu".into() };
        if is_cpu {
            let cur_layers = s.llm.n_gpu_layers;
            if cur_layers != 0 {
                s.llm_gpu_layers_saved = cur_layers;
            }
            s.llm.n_gpu_layers = 0;

            if !s.llm.rlx_device.eq_ignore_ascii_case("cpu") {
                s.llm_rlx_device_saved = std::mem::take(&mut s.llm.rlx_device);
            }
            s.llm.rlx_device = "cpu".into();
        } else {
            s.llm.n_gpu_layers = s.llm_gpu_layers_saved;

            // An empty saved value means this profile has never had a device
            // other than CPU recorded; fall back to the per-platform default
            // rather than writing an empty string nobody can resolve.
            s.llm.rlx_device = if s.llm_rlx_device_saved.is_empty() {
                skill_llm::config::default_rlx_device_tag()
            } else {
                s.llm_rlx_device_saved.clone()
            };
        }
        s.inference_device.clone()
    })
    .await;

    // Sync to in-memory llm_config so the next server start picks it up.
    #[cfg(feature = "llm")]
    if let Ok(mut cfg) = state.llm_config.lock() {
        let s = load_user_settings(&state);
        cfg.n_gpu_layers = if is_cpu { 0 } else { s.llm_gpu_layers_saved };
        cfg.rlx_device = s.llm.rlx_device.clone();
    }

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

crate::settings_get_value!(get_exg_inference_device => exg_inference_device);

/// `GET /settings/eeg-fft-backend` → the saved choice, the backend FFTs run on
/// now, the backends this build can try here, and — under `auto` — which GPU
/// (if any) takes batches from what size (`null` while calibrating / no win).
pub(crate) async fn get_eeg_fft_backend(State(state): State<AppState>) -> Json<serde_json::Value> {
    use skill_eeg::fft;
    let auto = fft::is_auto().then(|| {
        let plan = fft::auto_plan();
        serde_json::json!({
            "calibrating": fft::calibrating(),
            "gpu": plan.map(|p| p.gpu.name()),
            "gpu_min_samples": plan.map(|p| p.min_samples),
        })
    });
    Json(serde_json::json!({
        "value": load_user_settings(&state).eeg_fft_backend,
        "active": fft::backend().name(),
        "available": fft::known(),
        "auto": auto,
    }))
}

/// `POST /settings/eeg-fft-backend` `{"value": "accelerate"}` — applies it now
/// (after a parity check against rustfft) and saves it only if it was accepted.
pub(crate) async fn set_eeg_fft_backend(
    State(state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    let name = req.value.trim().to_string();
    // The probe may compile a GPU graph; keep it off the async runtime.
    let n2 = name.clone();
    let res = tokio::task::spawn_blocking(move || skill_eeg::fft::set_backend(&n2))
        .await
        .unwrap_or_else(|e| Err(format!("probe task failed: {e}")));
    match res {
        Ok(b) => {
            let saved = name.to_ascii_lowercase();
            modify_settings_blocking(&state, move |s| s.eeg_fft_backend = saved).await;
            Json(serde_json::json!({"ok": true, "value": name.to_ascii_lowercase(), "active": b.name()}))
        }
        Err(e) => Json(serde_json::json!({
            "ok": false,
            "error": e,
            "active": skill_eeg::fft::backend().name(),
        })),
    }
}

// Opt-in: fetch EXG encoder weights automatically when a session finds them
// missing. Off by default — see `Settings::exg_auto_download_weights`.
crate::settings_bool!(get_exg_auto_download_weights, set_exg_auto_download_weights => exg_auto_download_weights);

pub(crate) async fn set_exg_inference_device(
    State(state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    let out: String = match req.value.as_str() {
        "mlx" => "mlx".into(),
        "gpu" => "gpu".into(),
        "cpu" => "cpu".into(),
        _ => "auto".into(),
    };
    let out_clone = out.clone();
    modify_settings_blocking(&state, move |s| {
        s.exg_inference_device = out;
    })
    .await;
    Json(serde_json::json!({"ok": true, "value": out_clone}))
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
    let endpoint = if req.value.trim().is_empty() {
        skill_settings::default_hf_endpoint()
    } else {
        req.value.trim().to_string()
    };
    let endpoint_out = endpoint.clone();
    modify_settings_blocking(&state, move |s| {
        s.hf_endpoint = endpoint;
    })
    .await;
    Json(serde_json::json!({"ok": true, "value": endpoint_out}))
}
