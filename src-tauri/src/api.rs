// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! HTTP REST API + WebSocket server — both served on the same TCP port.
//!
//! The router returned by [`router`] mounts:
//!
//! - `GET  /`                  → WebSocket upgrade *or* JSON API info page
//! - `POST /`                  → Universal command tunnel (same JSON as WS)
//! - `GET  /status`            → `status` command
//! - `GET  /sessions`          → `sessions` command
//! - `POST /label`             → `label` command
//! - `POST /notify`            → `notify` command
//! - `POST /calibrate`         → `run_calibration` command (auto-start)
//! - `POST /timer`             → `timer` command (open & auto-start focus timer)
//! - `POST /search`            → `search` command (EEG ANN)
//! - `POST /search_labels`     → `search_labels` command (text/context/both)
//! - `POST /compare`           → `compare` command
//! - `POST /sleep`             → `sleep` command
//! - `POST /umap`              → `umap` command (enqueue job)
//! - `GET  /umap/{job_id}`     → `umap_poll` command
//! - `GET  /calibrations`      → `list_calibrations`
//! - `POST /calibrations`      → `create_calibration`
//! - `GET  /calibrations/{id}` → `get_calibration`
//! - `PATCH /calibrations/{id}` → `update_calibration`
//! - `DELETE /calibrations/{id}`→ `delete_calibration`
//!
//! All endpoints return `{ "command": "…", "ok": true/false, …payload }`.
//! HTTP status is 200 on success and 400 on error.
//!
//! CORS is wide-open (`*`) so browser scripts and Jupyter notebooks can call
//! the API directly without a proxy.
//!
//! ## Universal tunnel
//!
//! `POST /` with body `{ "command": "status" }` is identical to sending the
//! same JSON over WebSocket.  Every command, including those with nested
//! parameters, is available this way.
//!
//! ## REST shortcuts
//!
//! Individual endpoints accept only the *payload* fields (no `"command"` key
//! needed).  For example:
//! ```bash
//! curl -X POST http://localhost:8375/label \
//!      -H 'Content-Type: application/json' \
//!      -d '{"text":"eyes closed","context":"morning session"}'
//! ```
//!
//! ## WebSocket (unchanged)
//!
//! Existing WS clients continue to connect to `ws://host:port/` as before.

use std::net::SocketAddr;
use std::sync::Mutex;

use axum::{
    extract::{ConnectInfo, FromRequestParts, Path, Request, State, WebSocketUpgrade},
    http::{StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

use crate::ws_server::SharedTracker;
use crate::MutexExt;

// ── Shared state ──────────────────────────────────────────────────────────────

/// State passed to every HTTP and WebSocket handler.
#[derive(Clone)]
pub struct SharedState {
    /// Tauri app handle — used to call `ws_commands::dispatch`.
    pub app:     AppHandle,
    /// Broadcast sender — WS handler subscribes to this for push events.
    pub tx:      broadcast::Sender<String>,
    /// Connected-client list and request log — shared with the Tauri UI.
    pub tracker: SharedTracker,
}

// ── Router ────────────────────────────────────────────────────────────────────

/// Build the combined HTTP + WebSocket axum router.
///
/// Serve with:
/// ```ignore
/// axum::serve(listener, router(state).into_make_service_with_connect_info::<SocketAddr>()).await?;
/// ```
pub fn router(state: SharedState) -> Router {
    Router::new()
        // ── Root: WS upgrade OR GET info / POST command tunnel ────────────
        .route("/", get(root_get).post(command_post))
        // ── REST shortcuts ────────────────────────────────────────────────
        .route("/status",         get(status_get))
        .route("/sessions",       get(sessions_get))
        .route("/label",          post(label_post))
        .route("/notify",         post(notify_post))
        .route("/calibrate",      post(calibrate_post))
        .route("/timer",          post(timer_post))
        .route("/search",         post(search_post))
        .route("/search_labels",  post(search_labels_post))
        .route("/compare",        post(compare_post))
        .route("/sleep",          post(sleep_post))
        .route("/umap",           post(umap_post))
        .route("/umap/{job_id}",   get(umap_poll_get))
        .route("/calibrations",
            get(list_calibrations_get).post(create_calibration_post))
        .route("/say",            post(say_post))
        .route("/calibrations/{id}",
            get(get_calibration_get)
            .patch(update_calibration_patch)
            .delete(delete_calibration_delete))
        .route("/dnd",            get(dnd_get).post(dnd_post))
        // ── LLM REST shortcuts ─────────────────────────────────────────────
        .route("/llm/status",           get(llm_status_get))
        .route("/llm/start",            post(llm_start_post))
        .route("/llm/stop",             post(llm_stop_post))
        .route("/llm/catalog",          get(llm_catalog_get))
        .route("/llm/download",         post(llm_download_post))
        .route("/llm/cancel_download",  post(llm_cancel_download_post))
        .route("/llm/delete",           post(llm_delete_post))
        .route("/llm/logs",             get(llm_logs_get))
        .route("/llm/chat",             post(llm_chat_post))
        // ── CORS: allow all origins so browsers / notebooks can call freely
        .layer(CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any))
        .with_state(state)
}

// ── Dispatch helper ───────────────────────────────────────────────────────────

/// Run one command via [`crate::ws_commands::dispatch`], log it in the tracker,
/// and return an HTTP [`Response`] with the standard envelope JSON.
async fn cmd(
    state:   &SharedState,
    peer:    &str,
    command: &str,
    msg:     Value,
) -> Response {
    eprintln!("[http] {peer} → {command}");
    let result = crate::ws_commands::dispatch(&state.app, command, &msg).await;
    let ok = result.is_ok();
    state.tracker.lock_or_recover().log_request(peer, command, ok);
    match result {
        Ok(mut payload) => {
            payload["command"] = command.into();
            payload["ok"]      = true.into();
            (StatusCode::OK, Json(payload)).into_response()
        }
        Err(e) => {
            let body = json!({ "command": command, "ok": false, "error": e });
            (StatusCode::BAD_REQUEST, Json(body)).into_response()
        }
    }
}

/// Extract the remote peer address from [`ConnectInfo`], falling back to
/// `"http-unknown"` when the address is not available.
fn peer_str(addr: ConnectInfo<SocketAddr>) -> String {
    format!("http-{}", addr.0)
}

/// Merge an optional JSON body with a base object.
/// The body fields overwrite base fields on collision.
fn merge(base: Value, body: Option<Json<Value>>) -> Value {
    match body {
        None => base,
        Some(Json(mut extra)) => {
            if let (Some(m), Some(b)) = (base.as_object(), extra.as_object_mut()) {
                for (k, v) in m {
                    b.entry(k).or_insert_with(|| v.clone());
                }
            }
            extra
        }
    }
}

// ── Root handler (WS upgrade + GET info + POST tunnel) ───────────────────────

/// `GET /` — WebSocket upgrade if the client sent `Upgrade: websocket`,
/// otherwise a JSON API info/health document.
async fn root_get(
    State(state): State<SharedState>,
    addr:  ConnectInfo<SocketAddr>,
    req:   Request,
) -> Response {
    // axum 0.8: Option<WebSocketUpgrade> no longer works as an extractor
    // (requires OptionalFromRequestParts which WebSocketUpgrade does not impl).
    // Instead inspect the Upgrade header ourselves, then extract manually.
    let is_ws = req.headers()
        .get(axum::http::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    if is_ws {
        let (mut parts, _body) = req.into_parts();
        let ws = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
            Ok(ws)   => ws,
            Err(rej) => return rej.into_response(),
        };
        let peer = addr.0.to_string();
        ws.on_upgrade(move |socket| ws_client_task(socket, peer, state))
    } else {
        let info = json!({
            "name":    "Skill API",
            "version": 1,
            "docs":    "POST / with {\"command\":\"...\", …params} or use REST shortcuts below",
            "commands": [
                "status","sessions","label","notify","say","calibrate","timer",
                "search","search_labels","compare","sleep",
                "umap","umap_poll",
                "list_calibrations","get_calibration",
                "create_calibration","update_calibration","delete_calibration",
                "run_calibration",
                "dnd","dnd_set",
                "llm_status","llm_start","llm_stop","llm_catalog",
                "llm_download","llm_cancel_download","llm_delete","llm_logs",
                "llm_chat (WebSocket streaming + POST /llm/chat non-streaming)"
            ],
            "rest": {
                "GET /status":                  "status snapshot",
                "GET /sessions":                "list sessions",
                "POST /label":                  "create label",
                "POST /notify":                 "OS notification",
                "POST /say":                    "speak text via TTS (fire-and-forget)",
                "POST /calibrate":              "open calibration + auto-start",
                "POST /timer":                  "open focus timer + auto-start",
                "POST /search":                 "EEG ANN search",
                "POST /search_labels":          "text/context label search",
                "POST /compare":                "A/B comparison",
                "POST /sleep":                  "sleep staging",
                "POST /umap":                   "enqueue UMAP job",
                "GET  /umap/{job_id}":          "poll UMAP job",
                "GET  /calibrations":           "list profiles",
                "POST /calibrations":           "create profile",
                "GET  /calibrations/{id}":      "get profile",
                "PATCH /calibrations/{id}":     "update profile",
                "DELETE /calibrations/{id}":    "delete profile",
                "GET  /dnd":                    "DND automation status (config + live eligibility)",
                "POST /dnd":                    "force-enable/disable DND: { \"enabled\": bool }",
                "GET  /llm/status":             "LLM server status (stopped/loading/running)",
                "POST /llm/start":              "start LLM inference server (loads model)",
                "POST /llm/stop":               "stop LLM inference server (frees GPU memory)",
                "GET  /llm/catalog":            "model catalog with download states",
                "POST /llm/download":           "start model download: { \"filename\": \"...\" }",
                "POST /llm/cancel_download":    "cancel download: { \"filename\": \"...\" }",
                "POST /llm/delete":             "delete cached model: { \"filename\": \"...\" }",
                "GET  /llm/logs":               "last 500 LLM server log lines",
                "POST /llm/chat":               "non-streaming chat: { message, images?, system?, temperature?, max_tokens? } → { text, finish_reason, tokens }"
            }
        });
        (StatusCode::OK, Json(info)).into_response()
    }
}

/// `POST /` — Universal command tunnel: body must be `{ "command": "…", …params }`.
async fn command_post(
    State(state): State<SharedState>,
    addr:  ConnectInfo<SocketAddr>,
    body:  Option<Json<Value>>,
) -> Response {
    let msg     = body.map(|b| b.0).unwrap_or_else(|| json!({}));
    let command = match msg.get("command").and_then(|v| v.as_str()) {
        Some(c) if !c.is_empty() => c.to_owned(),
        _ => {
            let err = json!({ "ok": false, "error": "missing \"command\" field" });
            return (StatusCode::BAD_REQUEST, Json(err)).into_response();
        }
    };
    cmd(&state, &peer_str(addr), &command, msg).await
}

// ── REST shortcut handlers ────────────────────────────────────────────────────

async fn status_get(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd(&s, &peer_str(addr), "status", json!({})).await
}

async fn sessions_get(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd(&s, &peer_str(addr), "sessions", json!({})).await
}

async fn label_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "label", merge(json!({}), body)).await
}

async fn notify_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "notify", merge(json!({}), body)).await
}

/// `POST /say` — speak text via on-device TTS (fire-and-forget).
/// Body: `{ "text": "Eyes closed. Relax." }`
async fn say_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "say", merge(json!({}), body)).await
}

/// `POST /calibrate` — open the calibration window and auto-start.
/// Optional body: `{ "id": "<profile-uuid>" }`.
async fn calibrate_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "run_calibration", merge(json!({}), body)).await
}

async fn timer_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd(&s, &peer_str(addr), "timer", json!({})).await
}

async fn search_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "search", merge(json!({}), body)).await
}

async fn search_labels_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "search_labels", merge(json!({}), body)).await
}

async fn compare_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "compare", merge(json!({}), body)).await
}

async fn sleep_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "sleep", merge(json!({}), body)).await
}

async fn umap_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "umap", merge(json!({}), body)).await
}

async fn umap_poll_get(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    Path(job_id): Path<u64>,
) -> Response {
    cmd(&s, &peer_str(addr), "umap_poll", json!({ "job_id": job_id })).await
}

// ── Calibration profile CRUD ─────────────────────────────────────────────────

async fn list_calibrations_get(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd(&s, &peer_str(addr), "list_calibrations", json!({})).await
}

async fn create_calibration_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "create_calibration", merge(json!({}), body)).await
}

async fn get_calibration_get(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
) -> Response {
    cmd(&s, &peer_str(addr), "get_calibration", json!({ "id": id })).await
}

async fn update_calibration_patch(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    body: Option<Json<Value>>,
) -> Response {
    let mut msg = merge(json!({}), body);
    msg["id"] = id.into();
    cmd(&s, &peer_str(addr), "update_calibration", msg).await
}

/// `GET /dnd` — return the full DND automation status snapshot.
///
/// Equivalent to `{ "command": "dnd" }` via WebSocket or the universal tunnel.
/// Returns config (enabled, threshold, duration, mode), live timer progress
/// (`elapsed_secs`), app-side `dnd_active`, and the real OS Focus state.
async fn dnd_get(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd(&s, &peer_str(addr), "dnd", json!({})).await
}

/// `POST /dnd` — force-enable or disable DND, bypassing the EEG threshold.
///
/// Body: `{ "enabled": true | false }` — required.
///
/// Equivalent to `{ "command": "dnd_set", "enabled": true }` via WebSocket.
/// Useful for automation scripts, shell scripts, and CI/CD pipelines that need
/// to control Focus mode without waiting for the EEG threshold to be met.
async fn dnd_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "dnd_set", merge(json!({}), body)).await
}

async fn delete_calibration_delete(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
) -> Response {
    cmd(&s, &peer_str(addr), "delete_calibration", json!({ "id": id })).await
}

// ── LLM REST shortcut handlers ────────────────────────────────────────────────

async fn llm_status_get(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd(&s, &peer_str(addr), "llm_status", json!({})).await
}

async fn llm_start_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd(&s, &peer_str(addr), "llm_start", json!({})).await
}

async fn llm_stop_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd(&s, &peer_str(addr), "llm_stop", json!({})).await
}

async fn llm_catalog_get(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd(&s, &peer_str(addr), "llm_catalog", json!({})).await
}

async fn llm_download_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "llm_download", merge(json!({}), body)).await
}

async fn llm_cancel_download_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "llm_cancel_download", merge(json!({}), body)).await
}

async fn llm_delete_post(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd(&s, &peer_str(addr), "llm_delete", merge(json!({}), body)).await
}

async fn llm_logs_get(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd(&s, &peer_str(addr), "llm_logs", json!({})).await
}

/// `POST /llm/chat` — non-streaming LLM chat over HTTP.
///
/// Accepts a JSON body in either of two formats:
///
/// **Simple format** (plain text or text + base64 images):
/// ```json
/// {
///   "message":     "What's in this image?",
///   "images":      ["data:image/jpeg;base64,…", "data:image/png;base64,…"],
///   "system":      "You are a concise assistant.",
///   "temperature": 0.7,
///   "max_tokens":  512
/// }
/// ```
///
/// **Full OpenAI messages format** (for multi-turn or vision content parts):
/// ```json
/// {
///   "messages": [
///     {"role": "system",    "content": "You are helpful."},
///     {"role": "user",      "content": [
///       {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,…"}},
///       {"type": "text",      "text": "Describe this EEG headset."}
///     ]}
///   ],
///   "temperature": 0.8
/// }
/// ```
///
/// Response (always complete, never streamed):
/// ```json
/// {
///   "command": "llm_chat",
///   "ok": true,
///   "text": "The image shows…",
///   "finish_reason": "stop",
///   "prompt_tokens": 42,
///   "completion_tokens": 87,
///   "n_ctx": 4096
/// }
/// ```
#[cfg(feature = "llm")]
async fn llm_chat_post(
    State(state): State<SharedState>,
    addr:  ConnectInfo<SocketAddr>,
    body:  Option<Json<Value>>,
) -> Response {
    use tauri::Manager as _;

    let peer = peer_str(addr);
    let msg  = body.map(|b| b.0).unwrap_or_else(|| json!({}));

    // ── Build messages array ──────────────────────────────────────────────────
    let messages: Vec<Value> = if let Some(arr) = msg.get("messages").and_then(|v| v.as_array()) {
        // Full OpenAI messages array — pass through as-is.
        arr.clone()
    } else {
        // Simple format: optional system + user message + top-level images list.
        let mut msgs: Vec<Value> = Vec::new();

        if let Some(sys) = msg.get("system").and_then(|v| v.as_str()) {
            msgs.push(json!({ "role": "system", "content": sys }));
        }

        // Build user content: mix of image_url parts and text.
        let mut parts: Vec<Value> = Vec::new();

        // top-level "images": ["data:image/jpeg;base64,...", ...]
        if let Some(imgs) = msg.get("images").and_then(|v| v.as_array()) {
            for url in imgs {
                if let Some(u) = url.as_str() {
                    parts.push(json!({
                        "type": "image_url",
                        "image_url": { "url": u }
                    }));
                }
            }
        }

        let text = msg.get("message").and_then(|v| v.as_str()).unwrap_or("");
        if text.is_empty() && parts.is_empty() {
            let body = json!({ "command": "llm_chat", "ok": false,
                "error": "'message' or 'messages' field required" });
            return (StatusCode::BAD_REQUEST, Json(body)).into_response();
        }

        // If images present, use a parts array; otherwise a plain string.
        if parts.is_empty() {
            msgs.push(json!({ "role": "user", "content": text }));
        } else {
            if !text.is_empty() {
                parts.push(json!({ "type": "text", "text": text }));
            }
            msgs.push(json!({ "role": "user", "content": parts }));
        }
        msgs
    };

    // ── GenParams ─────────────────────────────────────────────────────────────
    let params = serde_json::from_value::<crate::llm::GenParams>(msg.clone())
        .unwrap_or_default();

    // ── Get server ────────────────────────────────────────────────────────────
    let cell = state.app.state::<Mutex<crate::AppState>>().lock_or_recover().llm_state_cell.clone();
    let server = { cell.lock().unwrap().as_ref().cloned() };

    let Some(server) = server else {
        let body = json!({ "command": "llm_chat", "ok": false,
            "error": "LLM server not running — POST /llm/start first" });
        state.tracker.lock_or_recover().log_request(&peer, "llm_chat", false);
        return (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response();
    };

    // ── Collect response ──────────────────────────────────────────────────────
    let images = crate::llm::extract_images_from_messages(&messages);
    let mut tok_rx = match server.chat(messages, images, params) {
        Ok(rx)  => rx,
        Err(e)  => {
            let body = json!({ "command": "llm_chat", "ok": false, "error": e });
            state.tracker.lock_or_recover().log_request(&peer, "llm_chat", false);
            return (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response();
        }
    };

    let mut text              = String::new();
    let mut finish_reason     = "stop".to_string();
    let mut prompt_tokens     = 0usize;
    let mut completion_tokens = 0usize;
    let mut n_ctx             = 0usize;

    while let Some(tok) = tok_rx.recv().await {
        match tok {
            crate::llm::InferToken::Delta(t) => text.push_str(&t),
            crate::llm::InferToken::Done { finish_reason: fr, prompt_tokens: pt,
                                           completion_tokens: ct, n_ctx: nc } => {
                finish_reason = fr; prompt_tokens = pt; completion_tokens = ct; n_ctx = nc;
                break;
            }
            crate::llm::InferToken::Error(e) => {
                let body = json!({ "command": "llm_chat", "ok": false, "error": e });
                state.tracker.lock_or_recover().log_request(&peer, "llm_chat", false);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response();
            }
        }
    }

    state.tracker.lock_or_recover().log_request(&peer, "llm_chat", true);
    let body = json!({
        "command":           "llm_chat",
        "ok":                true,
        "text":              text,
        "finish_reason":     finish_reason,
        "prompt_tokens":     prompt_tokens,
        "completion_tokens": completion_tokens,
        "n_ctx":             n_ctx,
    });
    (StatusCode::OK, Json(body)).into_response()
}

// Stub when llm feature is disabled.
#[cfg(not(feature = "llm"))]
async fn llm_chat_post(
    State(_): State<SharedState>, _addr: ConnectInfo<SocketAddr>,
    _body: Option<Json<Value>>,
) -> Response {
    let body = json!({ "command": "llm_chat", "ok": false,
        "error": "LLM feature not compiled in this build" });
    (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response()
}

// ── LLM chat — streaming WebSocket handler ────────────────────────────────────

/// Handle `llm_chat` over WebSocket.  Streams `delta` tokens back to the
/// client as individual frames; sends a final `done` or `error` frame.
///
/// # Wire protocol (client → server)
/// ```json
/// {
///   "command": "llm_chat",
///   "messages": [{"role":"user","content":"Hello!"}],
///   "temperature": 0.8,
///   "max_tokens": 2048
/// }
/// ```
/// Short-hand (single user message):
/// ```json
/// { "command": "llm_chat", "message": "What is EEG coherence?" }
/// ```
///
/// # Wire protocol (server → client, multiple frames)
/// ```json
/// { "command": "llm_chat", "type": "delta", "text": "Hello" }
/// { "command": "llm_chat", "type": "delta", "text": "!" }
/// { "command": "llm_chat", "ok": true,  "type": "done",
///   "finish_reason": "stop", "prompt_tokens": 12, "completion_tokens": 1, "n_ctx": 4096 }
/// ```
/// Or on error:
/// ```json
/// { "command": "llm_chat", "ok": false, "type": "error", "error": "..." }
/// ```
#[cfg(feature = "llm")]
async fn handle_llm_chat_ws(
    state: &SharedState,
    peer:  &str,
    text:  &str,
    sink:  &mut futures_util::stream::SplitSink<
        axum::extract::ws::WebSocket,
        axum::extract::ws::Message,
    >,
) -> Result<(), ()> {
    use axum::extract::ws::Message;

    /// Send a JSON object as a WebSocket text frame.
    macro_rules! ws_send {
        ($sink:expr, $val:expr) => {{
            let s = serde_json::to_string(&$val).unwrap_or_default();
            $sink.send(Message::Text(s.into())).await.map_err(|_| ())?;
        }};
    }

    let msg: Value = match serde_json::from_str(text) {
        Ok(v)  => v,
        Err(_) => {
            ws_send!(sink, json!({"command":"llm_chat","ok":false,"type":"error","error":"invalid JSON"}));
            state.tracker.lock_or_recover().log_request(peer, "llm_chat", false);
            return Ok(());
        }
    };

    eprintln!("[ws] {peer} → llm_chat");

    // ── Resolve messages ──────────────────────────────────────────────────────
    let messages: Vec<Value> = if let Some(arr) = msg.get("messages").and_then(|v| v.as_array()) {
        arr.clone()
    } else if let Some(s) = msg.get("message").and_then(|v| v.as_str()) {
        vec![json!({"role":"user","content":s})]
    } else {
        ws_send!(sink, json!({
            "command":"llm_chat","ok":false,"type":"error",
            "error":"'messages' array required (or 'message' shorthand string)"
        }));
        state.tracker.lock_or_recover().log_request(peer, "llm_chat", false);
        return Ok(());
    };

    // ── Resolve GenParams ─────────────────────────────────────────────────────
    let params = serde_json::from_value::<crate::llm::GenParams>(msg.clone())
        .unwrap_or_default();

    // ── Get the running server ────────────────────────────────────────────────
    use tauri::Manager as _;
    let app_state = state.app.state::<Mutex<crate::AppState>>();
    let cell = app_state.lock_or_recover().llm_state_cell.clone();
    let server = { cell.lock().unwrap().as_ref().cloned() };

    let Some(server) = server else {
        ws_send!(sink, json!({
            "command":"llm_chat","ok":false,"type":"error",
            "error":"LLM server not running — send { \"command\": \"llm_start\" } first"
        }));
        state.tracker.lock_or_recover().log_request(peer, "llm_chat", false);
        return Ok(());
    };

    // ── Extract images embedded in messages (base64 data-URLs only) ──────────
    let images = crate::llm::extract_images_from_messages(&messages);

    // ── Send to actor and stream tokens ───────────────────────────────────────
    let mut tok_rx = match server.chat(messages, images, params) {
        Ok(rx)   => rx,
        Err(e)   => {
            ws_send!(sink, json!({"command":"llm_chat","ok":false,"type":"error","error":e}));
            state.tracker.lock_or_recover().log_request(peer, "llm_chat", false);
            return Ok(());
        }
    };

    while let Some(tok) = tok_rx.recv().await {
        match tok {
            crate::llm::InferToken::Delta(text) => {
                ws_send!(sink, json!({"command":"llm_chat","type":"delta","text":text}));
            }
            crate::llm::InferToken::Done {
                finish_reason, prompt_tokens, completion_tokens, n_ctx,
            } => {
                ws_send!(sink, json!({
                    "command":          "llm_chat",
                    "ok":               true,
                    "type":             "done",
                    "finish_reason":    finish_reason,
                    "prompt_tokens":    prompt_tokens,
                    "completion_tokens":completion_tokens,
                    "n_ctx":            n_ctx,
                }));
                state.tracker.lock_or_recover().log_request(peer, "llm_chat", true);
                return Ok(());
            }
            crate::llm::InferToken::Error(e) => {
                ws_send!(sink, json!({"command":"llm_chat","ok":false,"type":"error","error":e}));
                state.tracker.lock_or_recover().log_request(peer, "llm_chat", false);
                return Ok(());
            }
        }
    }

    // Channel closed without a Done/Error — actor exited mid-generation.
    ws_send!(sink, json!({
        "command":"llm_chat","ok":false,"type":"error",
        "error":"LLM actor exited unexpectedly"
    }));
    state.tracker.lock_or_recover().log_request(peer, "llm_chat", false);
    Ok(())
}

// ── WebSocket client task ─────────────────────────────────────────────────────

/// One connected WebSocket client.
/// Fans out broadcast messages and handles inbound command frames.
async fn ws_client_task(
    socket: axum::extract::ws::WebSocket,
    peer:   String,
    state:  SharedState,
) {
    use axum::extract::ws::Message;

    state.tracker.lock_or_recover().add_client(&peer);
    eprintln!("[ws] + {peer}");

    let (mut sink, mut stream) = socket.split();
    let mut rx = state.tx.subscribe();

    loop {
        tokio::select! {
            // ── Broadcast → this client ───────────────────────────────────
            result = rx.recv() => match result {
                Ok(text) => {
                    if sink.send(Message::Text(text.into())).await.is_err() { break; }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("[ws] {peer} lagged {n} messages — slow consumer");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },

            // ── Client → command ──────────────────────────────────────────
            frame = stream.next() => match frame {
                None | Some(Err(_))            => break,
                Some(Ok(Message::Close(_)))    => break,
                Some(Ok(Message::Text(text)))  => {
                    let text_str: &str = &text;
                    // llm_chat is handled specially: it streams multiple
                    // frames back over the socket rather than a single response.
                    let is_llm_chat = serde_json::from_str::<Value>(text_str)
                        .ok()
                        .and_then(|v| v.get("command").and_then(|c| c.as_str()).map(|c| c == "llm_chat"))
                        .unwrap_or(false);

                    if is_llm_chat {
                        #[cfg(feature = "llm")]
                        {
                            if handle_llm_chat_ws(&state, &peer, text_str, &mut sink).await.is_err() {
                                break;
                            }
                        }
                        #[cfg(not(feature = "llm"))]
                        {
                            let resp = json!({"command":"llm_chat","ok":false,"error":"LLM feature not compiled in this build"});
                            let s = serde_json::to_string(&resp).unwrap_or_default();
                            if sink.send(Message::Text(s.into())).await.is_err() { break; }
                        }
                    } else if let Some(resp) = handle_ws_text(&state, &peer, text_str).await {
                        if sink.send(Message::Text(resp.into())).await.is_err() { break; }
                    }
                }
                Some(Ok(_)) => {} // ping / pong / binary — ignore
            },
        }
    }

    state.tracker.lock_or_recover().remove_client(&peer);
    eprintln!("[ws] - {peer}");
}

/// Parse one WS text frame as a JSON command and return the response string.
/// Returns `None` for unparseable frames (no reply sent).
async fn handle_ws_text(state: &SharedState, peer: &str, text: &str) -> Option<String> {
    let msg: Value = serde_json::from_str(text).ok()?;
    let command    = msg.get("command")?.as_str()?;
    eprintln!("[ws] {peer} → {command}");

    let result = crate::ws_commands::dispatch(&state.app, command, &msg).await;
    let ok = result.is_ok();
    state.tracker.lock_or_recover().log_request(peer, command, ok);

    let response = match result {
        Ok(mut payload) => {
            payload["command"] = command.into();
            payload["ok"]      = true.into();
            payload
        }
        Err(e) => json!({ "command": command, "ok": false, "error": e }),
    };

    serde_json::to_string(&response).ok()
}
