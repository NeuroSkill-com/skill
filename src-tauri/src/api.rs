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
//!
//! ## Unversioned shortcuts (legacy / convenience)
//!
//! - `GET  /status`            → `status` command
//! - `GET  /sessions`          → `sessions` command
//! - `POST /label`             → `label` command
//! - `POST /notify`            → `notify` command
//! - `POST /say`               → TTS (fire-and-forget)
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
//! - `GET  /dnd`               → DND automation status
//! - `POST /dnd`               → force-enable/disable DND
//!
//! ## Versioned `/v1/` endpoints (stateless, one-shot)
//!
//! Every unversioned shortcut above is mirrored at a `/v1/` prefix so that
//! clients can target a stable, version-tagged namespace.  These are
//! **identical** to the unversioned variants — same request / response bodies.
//!
//! The `/v1/` path space is shared with the embedded LLM server (when
//! compiled with `--features llm`): LLM owns `/v1/models`,
//! `/v1/chat/completions`, `/v1/completions`, and `/v1/embeddings`; all
//! other `/v1/` paths belong to the Skill API.
//!
//! | Method | Path                          | Description                          |
//! |--------|-------------------------------|--------------------------------------|
//! | GET    | `/v1/status`                  | Full system status snapshot          |
//! | GET    | `/v1/sessions`                | List recording sessions              |
//! | POST   | `/v1/label`                   | Create a timestamped EEG label       |
//! | POST   | `/v1/notify`                  | Send a native OS notification        |
//! | POST   | `/v1/say`                     | Speak text via on-device TTS         |
//! | POST   | `/v1/calibrate`               | Open calibration window + auto-start |
//! | POST   | `/v1/timer`                   | Open focus timer + auto-start        |
//! | POST   | `/v1/search`                  | EEG approximate-nearest-neighbour    |
//! | POST   | `/v1/search_labels`           | Text / semantic label search         |
//! | POST   | `/v1/compare`                 | A/B band-power comparison            |
//! | POST   | `/v1/sleep`                   | Sleep-stage classification           |
//! | POST   | `/v1/umap`                    | Enqueue a UMAP dimensionality job    |
//! | GET    | `/v1/umap/{job_id}`           | Poll UMAP job result                 |
//! | GET    | `/v1/calibrations`            | List calibration profiles            |
//! | POST   | `/v1/calibrations`            | Create calibration profile           |
//! | GET    | `/v1/calibrations/{id}`       | Get calibration profile by UUID      |
//! | PATCH  | `/v1/calibrations/{id}`       | Update calibration profile           |
//! | DELETE | `/v1/calibrations/{id}`       | Delete calibration profile           |
//! | GET    | `/v1/dnd`                     | DND automation status                |
//! | POST   | `/v1/dnd`                     | Force-enable/disable DND             |
//!
//! ## Quick-start examples
//!
//! ```bash
//! PORT=8375
//!
//! # System status
//! curl http://localhost:$PORT/v1/status | jq .
//!
//! # Create a label
//! curl -X POST http://localhost:$PORT/v1/label \
//!      -H 'Content-Type: application/json' \
//!      -d '{"text":"eyes closed meditation","context":"morning session"}'
//!
//! # Search the last 10 minutes of EEG
//! NOW=$(date +%s)
//! curl -X POST http://localhost:$PORT/v1/search \
//!      -H 'Content-Type: application/json' \
//!      -d "{\"start_utc\":$((NOW-600)),\"end_utc\":$NOW,\"k\":5}"
//!
//! # Speak a message
//! curl -X POST http://localhost:$PORT/v1/say \
//!      -H 'Content-Type: application/json' \
//!      -d '{"text":"Focus session starting now."}'
//!
//! # Enable DND
//! curl -X POST http://localhost:$PORT/v1/dnd \
//!      -H 'Content-Type: application/json' \
//!      -d '{"enabled":true}'
//! ```
//!
//! ## Universal tunnel (any command via POST /)
//!
//! `POST /` with body `{ "command": "status" }` is identical to sending the
//! same JSON over WebSocket.  Every command, including those with nested
//! parameters, is available this way.
//!
//! ## WebSocket (unchanged)
//!
//! Existing WS clients continue to connect to `ws://host:port/` as before.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, FromRequestParts, Path, Request, State, WebSocketUpgrade},
    http::StatusCode,
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
use crate::AppStateExt;
use crate::MutexExt;

// ── Bearer token middleware ───────────────────────────────────────────────────

/// Axum middleware that rejects requests when a non-empty `api_token` is
/// configured but the `Authorization: Bearer <token>` header is missing or
/// does not match.
///
/// When `api_token` is empty (the default), all requests pass through.
async fn bearer_auth(
    State(state): State<SharedState>,
    req: Request,
    next: axum::middleware::Next,
) -> Response {
    let expected = {
        let r = state.app.app_state();
        let g = r.lock_or_recover();
        g.api_token.clone()
    };

    // Empty token → auth disabled.
    if expected.is_empty() {
        return next.run(req).await;
    }

    // Extract the `Authorization: Bearer <token>` header.
    let provided = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if provided == expected {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "ok": false, "error": "invalid or missing Bearer token" })),
        )
            .into_response()
    }
}

// ── Shared state ──────────────────────────────────────────────────────────────

/// State passed to every HTTP and WebSocket handler.
#[derive(Clone)]
pub struct SharedState {
    /// Tauri app handle — used to call `ws_commands::dispatch`.
    pub app: AppHandle,
    /// Broadcast sender — WS handler subscribes to this for push events.
    pub tx: broadcast::Sender<String>,
    /// Connected-client list and request log — shared with the Tauri UI.
    pub tracker: SharedTracker,
    /// Kept for backward compat with `serve_with_mode`. Unused when peer_map is set.
    #[allow(dead_code)]
    pub readonly: bool,
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

        // ── Unversioned REST shortcuts (legacy / convenience) ─────────────
        .route("/status",         get(status_get))
        .route("/sessions",       get(sessions_get))
        .route("/label",          post(label_post))
        .route("/notify",         post(notify_post))
        .route("/say",            post(say_post))
        .route("/calibrate",      post(calibrate_post))
        .route("/timer",          post(timer_post))
        .route("/search",         post(search_post))
        .route("/search_labels",  post(search_labels_post))
        .route("/compare",        post(compare_post))
        .route("/sleep",          post(sleep_post))
        .route("/umap",           post(umap_post))
        .route("/umap/{job_id}",  get(umap_poll_get))
        .route("/calibrations",
            get(list_calibrations_get).post(create_calibration_post))
        .route("/calibrations/{id}",
            get(get_calibration_get)
            .patch(update_calibration_patch)
            .delete(delete_calibration_delete))
        .route("/dnd",            get(dnd_get).post(dnd_post))

        // ── iroh tunnel auth endpoints ───────────────────────────────────
        .route("/iroh/info",            get(iroh_info_get))
        .route("/iroh/totp",            get(iroh_totp_list_get).post(iroh_totp_create_post))
        .route("/iroh/totp/qr",         post(iroh_totp_qr_post))
        .route("/iroh/totp/revoke",     post(iroh_totp_revoke_post))
        .route("/iroh/clients",         get(iroh_clients_list_get))
        .route("/iroh/clients/register",post(iroh_client_register_post))
        .route("/iroh/clients/revoke",  post(iroh_client_revoke_post))
        .route("/iroh/clients/scope",   post(iroh_client_set_scope_post))
        .route("/iroh/phone-invite",   post(iroh_phone_invite_post))
        .route("/iroh/scope-groups",   get(iroh_scope_groups_get))
        .route("/iroh/clients/permissions", post(iroh_client_permissions_post))

        // ── Versioned /v1/ REST endpoints (stateless, one-shot) ──────────
        // Mirrors every unversioned shortcut above.  Shares the /v1/ namespace
        // with the LLM sub-router (/v1/models, /v1/chat/completions, …) —
        // those paths are disjoint and are registered via .merge() in ws_server.
        .route("/v1/status",         get(status_get))
        .route("/v1/sessions",       get(sessions_get))
        .route("/v1/label",          post(label_post))
        .route("/v1/notify",         post(notify_post))
        .route("/v1/say",            post(say_post))
        .route("/v1/calibrate",      post(calibrate_post))
        .route("/v1/timer",          post(timer_post))
        .route("/v1/search",         post(search_post))
        .route("/v1/search_labels",  post(search_labels_post))
        .route("/v1/compare",        post(compare_post))
        .route("/v1/sleep",          post(sleep_post))
        .route("/v1/umap",           post(umap_post))
        .route("/v1/umap/{job_id}",  get(umap_poll_get))
        .route("/v1/calibrations",
            get(list_calibrations_get).post(create_calibration_post))
        .route("/v1/calibrations/{id}",
            get(get_calibration_get)
            .patch(update_calibration_patch)
            .delete(delete_calibration_delete))
        .route("/v1/dnd",            get(dnd_get).post(dnd_post))

        // ── Versioned iroh tunnel auth endpoints ─────────────────────────
        .route("/v1/iroh/info",             get(iroh_info_get))
        .route("/v1/iroh/totp",             get(iroh_totp_list_get).post(iroh_totp_create_post))
        .route("/v1/iroh/totp/qr",          post(iroh_totp_qr_post))
        .route("/v1/iroh/totp/revoke",      post(iroh_totp_revoke_post))
        .route("/v1/iroh/clients",          get(iroh_clients_list_get))
        .route("/v1/iroh/clients/register", post(iroh_client_register_post))
        .route("/v1/iroh/clients/revoke",   post(iroh_client_revoke_post))
        .route("/v1/iroh/clients/scope",    post(iroh_client_set_scope_post))
        .route("/v1/iroh/phone-invite",     post(iroh_phone_invite_post))
        .route("/v1/iroh/scope-groups",      get(iroh_scope_groups_get))
        .route("/v1/iroh/clients/permissions", post(iroh_client_permissions_post))

        // ── LLM REST shortcuts (non-/v1/ — /v1/ routes are in llm::router)
        .route("/llm/start",            post(llm_start_post))
        .route("/llm/stop",             post(llm_stop_post))
        .route("/llm/catalog",          get(llm_catalog_get))
        .route("/llm/download",         post(llm_download_post))
        .route("/llm/cancel_download",  post(llm_cancel_download_post))
        .route("/llm/delete",           post(llm_delete_post))
        .route("/llm/logs",             get(llm_logs_get))
        .route("/llm/chat",             post(llm_chat_post))

        // ── HealthKit endpoints (iOS companion app sync + query) ────────
        .route("/health/sync",         post(health_sync_post))
        .route("/health/query",        post(health_query_post))
        .route("/health/summary",      get(health_summary_get).post(health_summary_post))
        .route("/health/metric_types", get(health_metric_types_get))
        .route("/v1/health/sync",         post(health_sync_post))
        .route("/v1/health/query",        post(health_query_post))
        .route("/v1/health/summary",      get(health_summary_get).post(health_summary_post))
        .route("/v1/health/metric_types", get(health_metric_types_get))

        // ── Calendar endpoints ──────────────────────────────────────────
        .route("/calendar/events",     post(calendar_events_post))
        .route("/calendar/status",     get(calendar_status_get))
        .route("/calendar/permission", post(calendar_permission_post))
        .route("/v1/calendar/events",     post(calendar_events_post))
        .route("/v1/calendar/status",     get(calendar_status_get))
        .route("/v1/calendar/permission", post(calendar_permission_post))

        // ── CORS: allow all origins so browsers / notebooks can call freely
        // ── Static file serving for screenshot images ────────────────
        .route("/screenshots/{*path}", get(screenshot_file_get))

        .layer(CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any))
        .layer(axum::middleware::from_fn_with_state(state.clone(), bearer_auth))
        .with_state(state)
}

// ── Dispatch helper ───────────────────────────────────────────────────────────

/// Run one command via [`crate::ws_commands::dispatch`], log it in the tracker,
/// and return an HTTP [`Response`] with the standard envelope JSON.
/// Look up the iroh peer endpoint ID for a given TCP source port.
/// Returns `None` for local (non-iroh) connections.
fn iroh_peer_for_addr(state: &SharedState, addr: &SocketAddr) -> Option<String> {
    use tauri::Manager;
    let peer_map = state.app.try_state::<skill_iroh::IrohPeerMap>()?;
    let map = skill_iroh::lock_or_recover(&peer_map);
    map.get(&addr.port()).cloned()
}

/// Check whether a command is allowed for a given connection.
/// Local (non-iroh) connections always have full access.
fn check_permission(state: &SharedState, addr: &SocketAddr, command: &str) -> Result<(), Value> {
    use tauri::Manager;

    let Some(peer_endpoint_id) = iroh_peer_for_addr(state, addr) else {
        return Ok(()); // local connection — always allowed
    };

    let Some(auth) = state.app.try_state::<skill_iroh::SharedIrohAuth>() else {
        return Ok(()); // no auth store — allow (shouldn't happen)
    };

    let auth_g: std::sync::MutexGuard<'_, skill_iroh::IrohAuthStore> =
        skill_iroh::lock_or_recover(&auth);
    if auth_g.is_command_allowed(&peer_endpoint_id, command) {
        return Ok(());
    }

    // Debug: log what we're looking up vs what's registered
    let registered_ids: Vec<String> = auth_g
        .list_clients()
        .iter()
        .filter(|c| c.revoked_at.is_none())
        .map(|c| format!("{}({})", c.endpoint_id, c.scope))
        .collect();
    eprintln!(
        "[iroh-auth] DENIED {command} for peer={peer_endpoint_id} — registered: [{}]",
        registered_ids.join(", ")
    );

    // Build a helpful error
    let hint_group = skill_iroh::scope::group_for_command(command)
        .map(|g| g.id)
        .unwrap_or("unknown");
    let scope = auth_g
        .scope_for_endpoint(&peer_endpoint_id)
        .unwrap_or_else(|| "none".into());

    let error_msg = if scope == "none" {
        format!(
            "forbidden: this device is not recognized. \
             Please re-pair by scanning the QR code again in Skill's Remote Access settings. \
             (peer: {}…)",
            &peer_endpoint_id[..peer_endpoint_id.len().min(16)]
        )
    } else {
        format!(
            "forbidden: your scope ({scope}) does not permit '{command}'. \
             Required: group '{hint_group}' or explicit grant."
        )
    };

    Err(json!({
        "command": command,
        "ok": false,
        "error": error_msg,
        "scope": scope,
        "hint_group": hint_group,
    }))
}

#[cfg(test)]
mod permission_tests {
    use super::*;

    #[test]
    fn read_scope_allows_basic_commands() {
        let cs = skill_iroh::ClientScope::read();
        assert!(skill_iroh::scope::is_allowed(&cs, "status"));
        assert!(skill_iroh::scope::is_allowed(&cs, "search"));
        assert!(skill_iroh::scope::is_allowed(&cs, "iroh_info"));
        assert!(!skill_iroh::scope::is_allowed(&cs, "label"));
        assert!(!skill_iroh::scope::is_allowed(&cs, "notify"));
        assert!(!skill_iroh::scope::is_allowed(&cs, "iroh_client_set_scope"));
    }
}

#[allow(dead_code)]
async fn cmd(state: &SharedState, peer: &str, command: &str, msg: Value) -> Response {
    cmd_with_addr(state, peer, command, msg, None).await
}

async fn cmd_with_addr(
    state: &SharedState,
    peer: &str,
    command: &str,
    msg: Value,
    addr: Option<&SocketAddr>,
) -> Response {
    eprintln!("[http] {peer} → {command}");

    // Granular per-command permission check for iroh clients
    if let Some(a) = addr {
        if let Err(body) = check_permission(state, a, command) {
            return (StatusCode::FORBIDDEN, Json(body)).into_response();
        }
    }

    let result = crate::ws_commands::dispatch(&state.app, command, &msg).await;
    let ok = result.is_ok();
    state
        .tracker
        .lock_or_recover()
        .log_request(peer, command, ok);
    match result {
        Ok(mut payload) => {
            payload["command"] = command.into();
            payload["ok"] = true.into();
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
    addr: ConnectInfo<SocketAddr>,
    req: Request,
) -> Response {
    // axum 0.8: Option<WebSocketUpgrade> no longer works as an extractor
    // (requires OptionalFromRequestParts which WebSocketUpgrade does not impl).
    // Instead inspect the Upgrade header ourselves, then extract manually.
    let is_ws = req
        .headers()
        .get(axum::http::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    if is_ws {
        let (mut parts, _body) = req.into_parts();
        let ws = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
            Ok(ws) => ws,
            Err(rej) => return rej.into_response(),
        };
        let peer = addr.0.to_string();
        let sock_addr = addr.0;
        ws.on_upgrade(move |socket| ws_client_task(socket, peer, state, sock_addr))
    } else {
        let info = json!({
            "name":    "Skill API",
            "version": 1,
            "docs":    "POST / with {\"command\":\"...\", …params} or use /v1/ REST endpoints below",
            "websocket": "ws://host:port/  — connect then send JSON commands; receive broadcast events",
            "commands": [
                "status","sessions","label","notify","say","calibrate","timer",
                "search","search_labels","compare","sleep",
                "umap","umap_poll",
                "list_calibrations","get_calibration",
                "create_calibration","update_calibration","delete_calibration",
                "run_calibration",
                "dnd","dnd_set",
                "sleep_schedule","sleep_schedule_set",
                "llm_status","llm_start","llm_stop","llm_catalog",
                "llm_download","llm_cancel_download","llm_delete","llm_logs",
                "llm_chat (WebSocket streaming + POST /llm/chat non-streaming, persisted to chat history)"
            ],
            "v1": {
                "GET  /v1/status":                  "full system status snapshot",
                "GET  /v1/sessions":                "list recording sessions",
                "POST /v1/label":                   "create timestamped EEG label: { text, context? }",
                "POST /v1/notify":                  "native OS notification: { title, body? }",
                "POST /v1/say":                     "speak text via TTS: { text, voice? }",
                "POST /v1/calibrate":               "open calibration window + auto-start: { id? }",
                "POST /v1/timer":                   "open focus timer + auto-start",
                "POST /v1/search":                  "EEG ANN search: { start_utc, end_utc, k?, ef? }",
                "POST /v1/search_labels":           "label search: { query, k?, mode? }",
                "POST /v1/compare":                 "A/B comparison: { a_start_utc, a_end_utc, b_start_utc, b_end_utc }",
                "POST /v1/sleep":                   "sleep staging: { start_utc, end_utc }",
                "POST /v1/umap":                    "enqueue UMAP job: { start_utc, end_utc, … }",
                "GET  /v1/umap/{job_id}":           "poll UMAP job result",
                "GET  /v1/calibrations":            "list calibration profiles",
                "POST /v1/calibrations":            "create calibration profile",
                "GET  /v1/calibrations/{id}":       "get calibration profile by UUID",
                "PATCH /v1/calibrations/{id}":      "update calibration profile",
                "DELETE /v1/calibrations/{id}":     "delete calibration profile",
                "GET  /v1/dnd":                     "DND automation status",
                "POST /v1/dnd":                     "force-enable/disable DND: { enabled: bool }",
                "POST /v1/health/sync":             "upsert HealthKit data from iOS: { sleep?, workouts?, heart_rate?, steps?, mindfulness?, metrics? }",
                "POST /v1/health/query":            "query health data: { type, start_utc?, end_utc?, limit?, metric_type? }",
                "GET  /v1/health/summary":          "aggregate health counts (last 24h default)",
                "POST /v1/health/summary":          "aggregate health counts: { start_utc, end_utc }",
                "GET  /v1/health/metric_types":     "list all stored metric types",
                "POST /v1/calendar/events":         "calendar events in range: { start_utc, end_utc }",
                "GET  /v1/calendar/status":         "calendar access status + platform",
                "POST /v1/calendar/permission":     "request calendar access (macOS: system dialog)",
                "note":                             "/v1/models /v1/chat/completions /v1/completions /v1/embeddings are served by the LLM sub-router"
            },
            "rest_legacy": {
                "note":                         "Unversioned aliases (e.g. /status, /label) remain for backwards compatibility",
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
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    let msg = body.map(|b| b.0).unwrap_or_else(|| json!({}));
    let command = match msg.get("command").and_then(|v| v.as_str()) {
        Some(c) if !c.is_empty() => c.to_owned(),
        _ => {
            let err = json!({ "ok": false, "error": "missing \"command\" field" });
            return (StatusCode::BAD_REQUEST, Json(err)).into_response();
        }
    };
    cmd_with_addr(&state, &peer_str(addr.clone()), &command, msg, Some(&addr.0)).await
}

// ── REST shortcut handlers ────────────────────────────────────────────────────

async fn status_get(State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "status", json!({}), Some(&addr.0)).await
}

async fn sessions_get(State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "sessions", json!({}), Some(&addr.0)).await
}

async fn label_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "label", merge(json!({}), body), Some(&addr.0)).await
}

async fn notify_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "notify", merge(json!({}), body), Some(&addr.0)).await
}

/// `POST /say` — speak text via on-device TTS (fire-and-forget).
/// Body: `{ "text": "Eyes closed. Relax." }`
async fn say_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "say", merge(json!({}), body), Some(&addr.0)).await
}

/// `POST /calibrate` — open the calibration window and auto-start.
/// Optional body: `{ "id": "<profile-uuid>" }`.
async fn calibrate_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "run_calibration",
        merge(json!({}), body),
        Some(&addr.0),
    )
    .await
}

async fn timer_post(State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "timer", json!({}), Some(&addr.0)).await
}

async fn search_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "search", merge(json!({}), body), Some(&addr.0)).await
}

async fn search_labels_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "search_labels", merge(json!({}), body), Some(&addr.0)).await
}

async fn compare_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "compare", merge(json!({}), body), Some(&addr.0)).await
}

async fn sleep_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "sleep", merge(json!({}), body), Some(&addr.0)).await
}

async fn umap_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "umap", merge(json!({}), body), Some(&addr.0)).await
}

async fn umap_poll_get(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    Path(job_id): Path<u64>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "umap_poll",
        json!({ "job_id": job_id }),
        Some(&addr.0),
    )
    .await
}

// ── Calibration profile CRUD ─────────────────────────────────────────────────

async fn list_calibrations_get(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "list_calibrations", json!({}), Some(&addr.0)).await
}

async fn create_calibration_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "create_calibration",
        merge(json!({}), body),
        Some(&addr.0),
    )
    .await
}

async fn get_calibration_get(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "get_calibration", json!({ "id": id }), Some(&addr.0)).await
}

async fn update_calibration_patch(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    body: Option<Json<Value>>,
) -> Response {
    let mut msg = merge(json!({}), body);
    msg["id"] = id.into();
    cmd_with_addr(&s, &peer_str(addr.clone()), "update_calibration", msg, Some(&addr.0)).await
}

/// `GET /dnd` — return the full DND automation status snapshot.
///
/// Equivalent to `{ "command": "dnd" }` via WebSocket or the universal tunnel.
/// Returns config (enabled, threshold, duration, mode), live timer progress
/// (`elapsed_secs`), app-side `dnd_active`, and the real OS Focus state.
async fn dnd_get(State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "dnd", json!({}), Some(&addr.0)).await
}

/// `POST /dnd` — force-enable or disable DND, bypassing the EEG threshold.
///
/// Body: `{ "enabled": true | false }` — required.
///
/// Equivalent to `{ "command": "dnd_set", "enabled": true }` via WebSocket.
/// Useful for automation scripts, shell scripts, and CI/CD pipelines that need
/// to control Focus mode without waiting for the EEG threshold to be met.
async fn dnd_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "dnd_set", merge(json!({}), body), Some(&addr.0)).await
}

async fn delete_calibration_delete(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "delete_calibration",
        json!({ "id": id }),
        Some(&addr.0),
    )
    .await
}

// ── iroh REST handlers ───────────────────────────────────────────────────────

async fn iroh_info_get(State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "iroh_info", json!({}), Some(&addr.0)).await
}

async fn iroh_totp_list_get(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "iroh_totp_list", json!({}), Some(&addr.0)).await
}

async fn iroh_totp_create_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "iroh_totp_create",
        merge(json!({}), body),
        Some(&addr.0),
    )
    .await
}

async fn iroh_totp_qr_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "iroh_totp_qr", merge(json!({}), body), Some(&addr.0)).await
}

async fn iroh_totp_revoke_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "iroh_totp_revoke",
        merge(json!({}), body),
        Some(&addr.0),
    )
    .await
}

async fn iroh_clients_list_get(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "iroh_clients_list", json!({}), Some(&addr.0)).await
}

async fn iroh_client_register_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "iroh_client_register",
        merge(json!({}), body),
        Some(&addr.0),
    )
    .await
}

async fn iroh_client_revoke_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "iroh_client_revoke",
        merge(json!({}), body),
        Some(&addr.0),
    )
    .await
}

async fn iroh_client_set_scope_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "iroh_client_set_scope",
        merge(json!({}), body),
        Some(&addr.0),
    )
    .await
}

async fn iroh_phone_invite_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "iroh_phone_invite",
        merge(json!({}), body),
        Some(&addr.0),
    )
    .await
}

async fn iroh_scope_groups_get(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "iroh_scope_groups", json!({}), Some(&addr.0)).await
}

async fn iroh_client_permissions_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "iroh_client_permissions",
        merge(json!({}), body),
        Some(&addr.0),
    )
    .await
}

// ── LLM REST shortcut handlers ────────────────────────────────────────────────

async fn llm_start_post(State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "llm_start", json!({}), Some(&addr.0)).await
}

async fn llm_stop_post(State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "llm_stop", json!({}), Some(&addr.0)).await
}

async fn llm_catalog_get(State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "llm_catalog", json!({}), Some(&addr.0)).await
}

async fn llm_download_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "llm_download", merge(json!({}), body), Some(&addr.0)).await
}

async fn llm_cancel_download_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "llm_cancel_download",
        merge(json!({}), body),
        Some(&addr.0),
    )
    .await
}

async fn llm_delete_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "llm_delete", merge(json!({}), body), Some(&addr.0)).await
}

async fn llm_logs_get(State(s): State<SharedState>, addr: ConnectInfo<SocketAddr>) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "llm_logs", json!({}), Some(&addr.0)).await
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
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    let peer = peer_str(addr);
    let msg = body.map(|b| b.0).unwrap_or_else(|| json!({}));

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
    let params = serde_json::from_value::<crate::llm::GenParams>(msg.clone()).unwrap_or_default();

    // ── Get server ────────────────────────────────────────────────────────────
    let app_state = state.app.app_state();
    let cell = {
        let __a = app_state.lock_or_recover().llm.clone();
        let __r = __a.lock_or_recover().state_cell.clone();
        __r
    };
    let server = { cell.lock_or_recover().as_ref().cloned() };

    let Some(server) = server else {
        let body = json!({ "command": "llm_chat", "ok": false,
            "error": "LLM server not running — POST /llm/start first" });
        state
            .tracker
            .lock_or_recover()
            .log_request(&peer, "llm_chat", false);
        return (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response();
    };

    // ── Persist: resolve or create a chat session ─────────────────────────────
    let req_sid = msg
        .get("session_id")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let is_new_session = req_sid <= 0;
    let session_id: i64 = {
        let s = app_state.lock_or_recover();
        let __llm_arc = s.llm.clone();
        let mut llm = __llm_arc.lock_or_recover();
        if let Some(store) = llm.chat_store.as_mut() {
            if req_sid > 0 {
                req_sid
            } else {
                store.new_session()
            }
        } else {
            0
        }
    };

    // ── Persist: save the last user message ───────────────────────────────────
    if session_id > 0 {
        let last_user = messages
            .iter()
            .rev()
            .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"));
        if let Some(um) = last_user {
            let content = um.get("content").and_then(|c| c.as_str()).unwrap_or("");
            if !content.is_empty() {
                let s = app_state.lock_or_recover();
                let __llm_arc = s.llm.clone();
                let mut llm = __llm_arc.lock_or_recover();
                if let Some(store) = llm.chat_store.as_mut() {
                    store.save_message(session_id, "user", content, None);
                    if is_new_session {
                        let title: String = content
                            .chars()
                            .take(60)
                            .collect::<String>()
                            .replace('\n', " ");
                        store.rename_session(session_id, title.trim());
                    }
                }
            }
        }
    }

    // ── Collect response ──────────────────────────────────────────────────────
    let images = crate::llm::extract_images_from_messages(&messages);
    let mut tok_rx = match server.chat(messages, images, params) {
        Ok(rx) => rx,
        Err(e) => {
            let body = json!({ "command": "llm_chat", "ok": false, "error": e.to_string() });
            state
                .tracker
                .lock_or_recover()
                .log_request(&peer, "llm_chat", false);
            return (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response();
        }
    };

    let mut text = String::new();
    let mut finish_reason = "stop".to_string();
    let mut prompt_tokens = 0usize;
    let mut completion_tokens = 0usize;
    let mut n_ctx = 0usize;

    while let Some(tok) = tok_rx.recv().await {
        match tok {
            crate::llm::InferToken::Delta(t) => text.push_str(&t),
            crate::llm::InferToken::Done {
                finish_reason: fr,
                prompt_tokens: pt,
                completion_tokens: ct,
                n_ctx: nc,
            } => {
                finish_reason = fr;
                prompt_tokens = pt;
                completion_tokens = ct;
                n_ctx = nc;
                break;
            }
            crate::llm::InferToken::Error(e) => {
                let body = json!({ "command": "llm_chat", "ok": false, "error": e });
                state
                    .tracker
                    .lock_or_recover()
                    .log_request(&peer, "llm_chat", false);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response();
            }
        }
    }

    // ── Persist the assistant response ────────────────────────────────────────
    if session_id > 0 && !text.is_empty() {
        let s = app_state.lock_or_recover();
        let __llm_arc = s.llm.clone();
        let mut llm = __llm_arc.lock_or_recover();
        if let Some(store) = llm.chat_store.as_mut() {
            let msg_id = store.save_message(session_id, "assistant", &text, None);
            eprintln!(
                "[http] persisted assistant message id={msg_id} session={session_id} len={}",
                text.len()
            );
        }
    }

    state
        .tracker
        .lock_or_recover()
        .log_request(&peer, "llm_chat", true);
    let body = json!({
        "command":           "llm_chat",
        "ok":                true,
        "text":              text,
        "finish_reason":     finish_reason,
        "prompt_tokens":     prompt_tokens,
        "completion_tokens": completion_tokens,
        "n_ctx":             n_ctx,
        "session_id":        session_id,
    });
    (StatusCode::OK, Json(body)).into_response()
}

// Stub when llm feature is disabled.
#[cfg(not(feature = "llm"))]
async fn llm_chat_post(
    State(_): State<SharedState>,
    _addr: ConnectInfo<SocketAddr>,
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
///   "max_tokens": 2048,
///   "session_id": 42
/// }
/// ```
/// Short-hand (single user message):
/// ```json
/// { "command": "llm_chat", "message": "What is EEG coherence?" }
/// ```
///
/// If `session_id` is provided, the conversation continues that session.
/// Otherwise a new session is created automatically. Both user and assistant
/// messages are persisted to the same SQLite chat store used by the Chat window.
///
/// # Wire protocol (server → client, multiple frames)
/// ```json
/// { "command": "llm_chat", "type": "session", "session_id": 42 }
/// { "command": "llm_chat", "type": "delta", "text": "Hello" }
/// { "command": "llm_chat", "type": "delta", "text": "!" }
/// { "command": "llm_chat", "ok": true,  "type": "done",
///   "finish_reason": "stop", "prompt_tokens": 12, "completion_tokens": 1,
///   "n_ctx": 4096, "session_id": 42 }
/// ```
/// Or on error:
/// ```json
/// { "command": "llm_chat", "ok": false, "type": "error", "error": "..." }
/// ```
#[cfg(feature = "llm")]
async fn handle_llm_chat_ws(
    state: &SharedState,
    peer: &str,
    text: &str,
    sink: &mut futures_util::stream::SplitSink<
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
        Ok(v) => v,
        Err(_) => {
            ws_send!(
                sink,
                json!({"command":"llm_chat","ok":false,"type":"error","error":"invalid JSON"})
            );
            state
                .tracker
                .lock_or_recover()
                .log_request(peer, "llm_chat", false);
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
        ws_send!(
            sink,
            json!({
                "command":"llm_chat","ok":false,"type":"error",
                "error":"'messages' array required (or 'message' shorthand string)"
            })
        );
        state
            .tracker
            .lock_or_recover()
            .log_request(peer, "llm_chat", false);
        return Ok(());
    };

    // ── Resolve GenParams ─────────────────────────────────────────────────────
    let params = serde_json::from_value::<crate::llm::GenParams>(msg.clone()).unwrap_or_default();

    // ── Get the running server ────────────────────────────────────────────────
    let app_state = state.app.app_state();

    // ── Persist: resolve or create a chat session ─────────────────────────────
    // Callers may pass `"session_id": <i64>` to continue an existing session;
    // otherwise a new session is created automatically.
    let req_sid = msg
        .get("session_id")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let is_new_session = req_sid <= 0;
    let session_id: i64 = {
        let s = app_state.lock_or_recover();
        let __llm_arc = s.llm.clone();
        let mut llm = __llm_arc.lock_or_recover();
        if let Some(store) = llm.chat_store.as_mut() {
            if req_sid > 0 {
                req_sid
            } else {
                store.new_session()
            }
        } else {
            0
        }
    };

    // ── Persist: save the last user message (the new turn) ────────────────────
    if session_id > 0 {
        let last_user = messages
            .iter()
            .rev()
            .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"));
        if let Some(um) = last_user {
            let content = um.get("content").and_then(|c| c.as_str()).unwrap_or("");
            if !content.is_empty() {
                let s = app_state.lock_or_recover();
                let __llm_arc = s.llm.clone();
                let mut llm = __llm_arc.lock_or_recover();
                if let Some(store) = llm.chat_store.as_mut() {
                    store.save_message(session_id, "user", content, None);
                    // Auto-title new sessions with the first user message (up to 60 chars).
                    if is_new_session {
                        let title: String = content
                            .chars()
                            .take(60)
                            .collect::<String>()
                            .replace('\n', " ");
                        store.rename_session(session_id, title.trim());
                    }
                }
            }
        }
    }

    // Send session_id to client so it can reference this session later.
    ws_send!(
        sink,
        json!({
            "command": "llm_chat", "type": "session", "session_id": session_id
        })
    );

    let cell = {
        let __a = app_state.lock_or_recover().llm.clone();
        let __r = __a.lock_or_recover().state_cell.clone();
        __r
    };
    let server = { cell.lock_or_recover().as_ref().cloned() };

    let Some(server) = server else {
        ws_send!(
            sink,
            json!({
                "command":"llm_chat","ok":false,"type":"error",
                "error":"LLM server not running — send { \"command\": \"llm_start\" } first"
            })
        );
        state
            .tracker
            .lock_or_recover()
            .log_request(peer, "llm_chat", false);
        return Ok(());
    };

    // ── Run chat with built-in tool orchestration ──────────────────────────
    // Uses the same multi-round tool loop as the Tauri Chat window:
    // model generates → parse tool calls → execute → inject results → continue.
    // Visible deltas (with tool-call blocks stripped) are streamed to the client.
    // Tool execution events are sent as typed WS messages for client progress UI.
    use crate::llm::{run_chat_with_builtin_tools, ToolEvent};

    // Channel to shuttle WS frames from sync callbacks to our async send loop.
    let (frame_tx, mut frame_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();

    let delta_tx = frame_tx.clone();
    let tool_tx = frame_tx.clone();
    drop(frame_tx); // drop original so channel closes when both clones are done

    // Collect tool events for persistence after generation completes.
    let tool_calls_collected = std::sync::Arc::new(std::sync::Mutex::new(Vec::<
        crate::llm::chat_store::NewToolCall,
    >::new()));
    let tool_calls_for_cb = tool_calls_collected.clone();

    // Spawn the orchestration on a task so we can concurrently drain the frame channel.
    let server_clone = server.clone();
    let gen_handle = tokio::spawn(async move {
        run_chat_with_builtin_tools(
            &server_clone,
            messages,
            params,
            Vec::new(),
            // on_visible_delta
            move |delta: &str| {
                let _ = delta_tx.send(json!({
                    "command": "llm_chat", "type": "delta", "text": delta
                }));
            },
            // on_tool_event
            move |event: ToolEvent| {
                let msg = match &event {
                    ToolEvent::ExecutionStart { tool_call_id, tool_name, args } => {
                        // Record tool call start (we'll update with result on End).
                        tool_calls_for_cb.lock_or_recover().push(
                            crate::llm::chat_store::NewToolCall {
                                tool:         tool_name.clone(),
                                status:       "running".to_string(),
                                detail:       None,
                                tool_call_id: Some(tool_call_id.clone()),
                                args:         Some(args.clone()),
                                result:       None,
                            },
                        );
                        json!({
                            "command": "llm_chat", "type": "tool_start",
                            "tool_call_id": tool_call_id, "tool_name": tool_name, "arguments": args,
                        })
                    }
                    ToolEvent::ExecutionEnd { tool_call_id, tool_name, result, is_error } => {
                        // Update the matching tool call with the result.
                        let mut tcs = tool_calls_for_cb.lock_or_recover();
                        if let Some(tc) = tcs.iter_mut().rev().find(|tc|
                            tc.tool_call_id.as_deref() == Some(tool_call_id)
                        ) {
                            tc.status = if *is_error { "error".to_string() } else { "done".to_string() };
                            tc.result = Some(result.clone());
                        }
                        json!({
                            "command": "llm_chat", "type": "tool_end",
                            "tool_call_id": tool_call_id, "tool_name": tool_name,
                            "result": result, "is_error": is_error,
                        })
                    }
                    ToolEvent::Status { tool_name, status, detail } => json!({
                        "command": "llm_chat", "type": "tool_status",
                        "tool_name": tool_name, "status": status, "detail": detail,
                    }),
                    ToolEvent::RoundComplete { round, prompt_tokens, completion_tokens, tool_calls_count } => json!({
                        "command": "llm_chat", "type": "tool_round_complete",
                        "round": round, "prompt_tokens": prompt_tokens,
                        "completion_tokens": completion_tokens, "tool_calls_count": tool_calls_count,
                    }),
                };
                let _ = tool_tx.send(msg);
            },
        ).await
    });

    // Drain the frame channel, forwarding each JSON value as a WS text frame.
    while let Some(val) = frame_rx.recv().await {
        let s = serde_json::to_string(&val).unwrap_or_default();
        if sink.send(Message::Text(s.into())).await.is_err() {
            // Client disconnected — abort generation.
            gen_handle.abort();
            state
                .tracker
                .lock_or_recover()
                .log_request(peer, "llm_chat", false);
            return Err(());
        }
    }

    // Generation finished — collect the result.
    match gen_handle.await {
        Ok(Ok((text, finish_reason, prompt_tokens, completion_tokens, n_ctx))) => {
            // ── Persist the assistant response ────────────────────────────
            if session_id > 0 && !text.is_empty() {
                let tool_calls = tool_calls_collected.lock_or_recover().clone();
                let s = app_state.lock_or_recover();
                let __llm_arc = s.llm.clone();
                let mut llm = __llm_arc.lock_or_recover();
                if let Some(store) = llm.chat_store.as_mut() {
                    let msg_id = store.save_message_with_tools(
                        session_id,
                        "assistant",
                        &text,
                        None,
                        &tool_calls,
                    );
                    eprintln!(
                        "[ws] persisted assistant message id={msg_id} session={session_id} len={}",
                        text.len()
                    );
                }
            }

            ws_send!(
                sink,
                json!({
                    "command":           "llm_chat",
                    "ok":                true,
                    "type":              "done",
                    "finish_reason":     finish_reason,
                    "prompt_tokens":     prompt_tokens,
                    "completion_tokens": completion_tokens,
                    "n_ctx":             n_ctx,
                    "text":              text,
                    "session_id":        session_id,
                })
            );
            state
                .tracker
                .lock_or_recover()
                .log_request(peer, "llm_chat", true);
        }
        Ok(Err(e)) => {
            ws_send!(
                sink,
                json!({"command":"llm_chat","ok":false,"type":"error","error":e.to_string()})
            );
            state
                .tracker
                .lock_or_recover()
                .log_request(peer, "llm_chat", false);
        }
        Err(e) => {
            ws_send!(
                sink,
                json!({"command":"llm_chat","ok":false,"type":"error","error":format!("generation task panicked: {e}")})
            );
            state
                .tracker
                .lock_or_recover()
                .log_request(peer, "llm_chat", false);
        }
    }
    Ok(())
}

// ── WebSocket client task ─────────────────────────────────────────────────────

/// One connected WebSocket client.
/// Fans out broadcast messages and handles inbound command frames.
async fn ws_client_task(socket: axum::extract::ws::WebSocket, peer: String, state: SharedState, sock_addr: SocketAddr) {
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
                        // Check permission for llm_chat
                        if let Err(body) = check_permission(&state, &sock_addr, "llm_chat") {
                            let s = serde_json::to_string(&body).unwrap_or_default();
                            if sink.send(Message::Text(s.into())).await.is_err() { break; }
                            continue;
                        }
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
                    } else if let Some(resp) = handle_ws_text(&state, &peer, text_str, &sock_addr).await {
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
async fn handle_ws_text(state: &SharedState, peer: &str, text: &str, sock_addr: &SocketAddr) -> Option<String> {
    let msg: Value = serde_json::from_str(text).ok()?;
    let command = msg.get("command")?.as_str()?;
    eprintln!("[ws] {peer} → {command}");

    // Per-command permission check for iroh clients
    if let Err(body) = check_permission(state, sock_addr, command) {
        return serde_json::to_string(&body).ok();
    }

    let result = crate::ws_commands::dispatch(&state.app, command, &msg).await;
    let ok = result.is_ok();
    state
        .tracker
        .lock_or_recover()
        .log_request(peer, command, ok);

    let response = match result {
        Ok(mut payload) => {
            payload["command"] = command.into();
            payload["ok"] = true.into();
            payload
        }
        Err(e) => json!({ "command": command, "ok": false, "error": e }),
    };

    serde_json::to_string(&response).ok()
}

// ── HealthKit REST handlers ──────────────────────────────────────────────────

/// `POST /v1/health/sync` — upsert Apple HealthKit data from iOS companion.
///
/// This is the primary endpoint your iOS app calls to push HealthKit data.
/// The payload is a JSON object with typed sample arrays:
///
/// ```json
/// {
///   "sleep": [{ "source_id": "watch", "start_utc": 1740000000, "end_utc": 1740028800, "value": "REM" }],
///   "workouts": [{ "workout_type": "Running", "start_utc": ..., "end_utc": ..., "duration_secs": 3600,
///                   "active_calories": 450, "distance_meters": 8000, "avg_heart_rate": 145 }],
///   "heart_rate": [{ "timestamp": 1740030000, "bpm": 72.0, "context": "sedentary" }],
///   "steps": [{ "start_utc": 1740000000, "end_utc": 1740086400, "count": 9500 }],
///   "mindfulness": [{ "start_utc": 1740040000, "end_utc": 1740041200 }],
///   "metrics": [{ "metric_type": "restingHeartRate", "timestamp": 1740000000, "value": 58.0, "unit": "bpm" }]
/// }
/// ```
///
/// All arrays are optional.  Duplicates are ignored (idempotent upsert by
/// source_id + timestamps).
async fn health_sync_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "health_sync", merge(json!({}), body), Some(&addr.0)).await
}

/// `POST /v1/health/query` — query stored HealthKit data by type and range.
///
/// ```json
/// { "type": "sleep", "start_utc": 1740000000, "end_utc": 1740086400, "limit": 100 }
/// ```
///
/// Valid types: `sleep`, `workouts`, `heart_rate`, `steps`, `metrics`.
/// For `metrics`, also provide `"metric_type": "restingHeartRate"`.
async fn health_query_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "health_query", merge(json!({}), body), Some(&addr.0)).await
}

/// `GET /v1/health/summary` — aggregate counts (default: last 24h).
async fn health_summary_get(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "health_summary", json!({}), Some(&addr.0)).await
}

/// `POST /v1/health/summary` — aggregate counts for a custom range.
async fn health_summary_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "health_summary",
        merge(json!({}), body),
        Some(&addr.0),
    )
    .await
}

/// `GET /v1/health/metric_types` — list all distinct metric types.
async fn health_metric_types_get(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "health_metric_types", json!({}), Some(&addr.0)).await
}

// ── Calendar HTTP handlers ────────────────────────────────────────────────────

/// `POST /v1/calendar/events` — fetch calendar events in a time range.
///
/// ```json
/// { "start_utc": 1774396800, "end_utc": 1774483200 }
/// ```
async fn calendar_events_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
    body: Option<Json<Value>>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "calendar_events",
        merge(json!({}), body),
        Some(&addr.0),
    )
    .await
}

/// `GET /v1/calendar/status` — return calendar access status and platform.
async fn calendar_status_get(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd_with_addr(&s, &peer_str(addr.clone()), "calendar_status", json!({}), Some(&addr.0)).await
}

/// `POST /v1/calendar/permission` — request calendar access (macOS: system dialog).
async fn calendar_permission_post(
    State(s): State<SharedState>,
    addr: ConnectInfo<SocketAddr>,
) -> Response {
    cmd_with_addr(
        &s,
        &peer_str(addr.clone()),
        "calendar_request_permission",
        json!({}),
        Some(&addr.0),
    )
    .await
}

// ── Screenshot static file serving ───────────────────────────────────────────

/// Serve screenshot image files from `~/.skill/screenshots/`.
/// URL: `GET /screenshots/20260315/20260315143025.webp`
async fn screenshot_file_get(
    State(state): State<SharedState>,
    Path(path): Path<String>,
) -> Response {
    let skill_dir = {
        let r = state.app.app_state();
        let g = r.lock_or_recover();
        g.skill_dir.clone()
    };
    let file_path = skill_dir
        .join(crate::constants::SCREENSHOTS_DIR)
        .join(&path);

    // Security: ensure the resolved path is still under the screenshots dir
    let screenshots_dir = skill_dir.join(crate::constants::SCREENSHOTS_DIR);
    let Ok(canonical) = file_path.canonicalize() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(canonical_base) = screenshots_dir.canonicalize() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !canonical.starts_with(&canonical_base) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let Ok(bytes) = tokio::fs::read(&canonical).await else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let content_type = if path.ends_with(".webp") {
        "image/webp"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    };

    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, content_type),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=31536000, immutable",
            ),
        ],
        bytes,
    )
        .into_response()
}
