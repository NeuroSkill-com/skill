// SPDX-License-Identifier: GPL-3.0-only
//! LSL settings and virtual/iroh source handlers.

use axum::{extract::State, Json};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub(crate) struct LslAutoConnectRequest {
    pub(crate) enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LslPairRequest {
    pub(crate) source_id: String,
    pub(crate) name: String,
    pub(crate) stream_type: String,
    pub(crate) channels: usize,
    pub(crate) sample_rate: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LslUnpairRequest {
    pub(crate) source_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LslIdleTimeoutRequest {
    pub(crate) secs: Option<u64>,
}

fn persist_lsl_settings(state: &AppState) {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let mut settings = skill_settings::load_settings(&skill_dir);
    settings.lsl_auto_connect = state.lsl_auto_connect.lock().map(|g| *g).unwrap_or(false);
    settings.lsl_paired_streams = state.lsl_paired_streams.lock().map(|g| g.clone()).unwrap_or_default();
    settings.lsl_idle_timeout_secs = state
        .lsl_idle_timeout_secs
        .lock()
        .map(|g| *g)
        .unwrap_or(skill_settings::default_lsl_idle_timeout_secs());
    let path = skill_settings::settings_path(&skill_dir);
    if let Ok(json) = serde_json::to_string_pretty(&settings) {
        let _ = std::fs::write(path, json);
    }
}

pub(crate) async fn get_lsl_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let auto_connect = state.lsl_auto_connect.lock().map(|g| *g).unwrap_or(false);
    let paired_streams = state.lsl_paired_streams.lock().map(|g| g.clone()).unwrap_or_default();
    Json(serde_json::json!({"auto_connect": auto_connect, "paired_streams": paired_streams}))
}

pub(crate) async fn set_lsl_auto_connect(
    State(state): State<AppState>,
    Json(req): Json<LslAutoConnectRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut g) = state.lsl_auto_connect.lock() {
        *g = req.enabled;
    }
    persist_lsl_settings(&state);
    Json(serde_json::json!({"ok": true, "auto_connect": req.enabled}))
}

pub(crate) async fn lsl_pair_stream(
    State(state): State<AppState>,
    Json(req): Json<LslPairRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut g) = state.lsl_paired_streams.lock() {
        if let Some(existing) = g.iter_mut().find(|p| p.source_id == req.source_id) {
            existing.name = req.name;
            existing.stream_type = req.stream_type;
            existing.channels = req.channels;
            existing.sample_rate = req.sample_rate;
        } else {
            g.push(skill_settings::LslPairedStream {
                source_id: req.source_id,
                name: req.name,
                stream_type: req.stream_type,
                channels: req.channels,
                sample_rate: req.sample_rate,
            });
        }
    }
    persist_lsl_settings(&state);
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn lsl_unpair_stream(
    State(state): State<AppState>,
    Json(req): Json<LslUnpairRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut g) = state.lsl_paired_streams.lock() {
        g.retain(|p| p.source_id != req.source_id);
    }
    persist_lsl_settings(&state);
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn get_lsl_idle_timeout(State(state): State<AppState>) -> Json<serde_json::Value> {
    let secs = state.lsl_idle_timeout_secs.lock().map(|g| *g).unwrap_or(None);
    Json(serde_json::json!({"secs": secs}))
}

pub(crate) async fn set_lsl_idle_timeout(
    State(state): State<AppState>,
    Json(req): Json<LslIdleTimeoutRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut g) = state.lsl_idle_timeout_secs.lock() {
        *g = req.secs;
    }
    persist_lsl_settings(&state);
    Json(serde_json::json!({"ok": true, "secs": req.secs}))
}

pub(crate) async fn lsl_virtual_source_start(
    State(state): State<AppState>,
    body: Option<axum::extract::Json<serde_json::Value>>,
) -> Json<serde_json::Value> {
    let Ok(mut g) = state.lsl_virtual_source.lock() else {
        return Json(serde_json::json!({"ok": false, "running": false}));
    };
    if g.is_some() {
        return Json(serde_json::json!({"ok": true, "running": true, "started": false}));
    }
    // Parse config from the request body; fall back to defaults if absent / invalid.
    let config: skill_lsl::VirtualSourceConfig =
        body.and_then(|b| serde_json::from_value(b.0).ok()).unwrap_or_default();
    match skill_lsl::VirtualLslSource::start(config) {
        Ok(src) => {
            *g = Some(src);
            Json(serde_json::json!({"ok": true, "running": true, "started": true}))
        }
        Err(e) => Json(serde_json::json!({"ok": false, "running": false, "error": e.to_string()})),
    }
}

pub(crate) async fn lsl_virtual_source_stop(State(state): State<AppState>) -> Json<serde_json::Value> {
    let Ok(mut g) = state.lsl_virtual_source.lock() else {
        return Json(serde_json::json!({"ok": false, "running": false}));
    };
    let was_running = g.is_some();
    *g = None;
    Json(serde_json::json!({"ok": true, "running": false, "was_running": was_running}))
}

pub(crate) async fn lsl_virtual_source_running(State(state): State<AppState>) -> Json<serde_json::Value> {
    let running = state.lsl_virtual_source.lock().map(|g| g.is_some()).unwrap_or(false);
    Json(serde_json::json!({"running": running}))
}

pub(crate) async fn lsl_iroh_start(State(state): State<AppState>) -> Json<serde_json::Value> {
    let mut guard = state.lsl_iroh_endpoint_id.lock().ok();
    if let Some(ref mut g) = guard {
        if g.is_none() {
            let id: String = (0..16)
                .map(|_| {
                    const CH: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
                    let i = (rand::random::<u64>() as usize) % CH.len();
                    CH[i] as char
                })
                .collect();
            **g = Some(id);
        }
        return Json(serde_json::json!({"running": true, "endpoint_id": **g }));
    }
    Json(serde_json::json!({"running": false, "endpoint_id": serde_json::Value::Null}))
}

pub(crate) async fn lsl_iroh_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let eid = state.lsl_iroh_endpoint_id.lock().ok().and_then(|g| g.clone());
    Json(serde_json::json!({"running": eid.is_some(), "endpoint_id": eid}))
}

pub(crate) async fn lsl_iroh_stop(State(state): State<AppState>) -> Json<serde_json::Value> {
    if let Ok(mut g) = state.lsl_iroh_endpoint_id.lock() {
        *g = None;
    }
    Json(serde_json::json!({"running": false, "endpoint_id": serde_json::Value::Null}))
}
