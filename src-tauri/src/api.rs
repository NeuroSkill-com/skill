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
                "run_calibration"
            ],
            "rest": {
                "GET /status":             "status snapshot",
                "GET /sessions":           "list sessions",
                "POST /label":             "create label",
                "POST /notify":            "OS notification",
                "POST /say":               "speak text via TTS (fire-and-forget)",
                "POST /calibrate":         "open calibration + auto-start",
                "POST /timer":             "open focus timer + auto-start",
                "POST /search":            "EEG ANN search",
                "POST /search_labels":     "text/context label search",
                "POST /compare":           "A/B comparison",
                "POST /sleep":             "sleep staging",
                "POST /umap":              "enqueue UMAP job",
                "GET  /umap/{job_id}":     "poll UMAP job",
                "GET  /calibrations":      "list profiles",
                "POST /calibrations":      "create profile",
                "GET  /calibrations/{id}":  "get profile",
                "PATCH /calibrations/{id}": "update profile",
                "DELETE /calibrations/{id}":"delete profile"
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

async fn delete_calibration_delete(
    State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
) -> Response {
    cmd(&s, &peer_str(addr), "delete_calibration", json!({ "id": id })).await
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
                    let text: &str = &text;
                    if let Some(resp) = handle_ws_text(&state, &peer, text).await {
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
