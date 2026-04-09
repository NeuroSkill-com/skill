// SPDX-License-Identifier: GPL-3.0-only
//! HTTP routes for remote-access iroh tunnel management.
//!
//! All routes are mounted under `/v1/iroh/` by the daemon router.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/iroh/info", get(iroh_info))
        .route("/iroh/phone-invite", post(iroh_phone_invite))
        .route("/iroh/totp", get(iroh_totp_list).post(iroh_totp_create))
        .route("/iroh/totp/:id/qr", get(iroh_totp_qr))
        .route("/iroh/totp/:id/revoke", post(iroh_totp_revoke))
        .route("/iroh/clients", get(iroh_clients_list))
        .route("/iroh/clients/register", post(iroh_client_register))
        .route("/iroh/clients/:id/revoke", post(iroh_client_revoke))
        .route("/iroh/clients/:id/scope", post(iroh_client_set_scope))
        .route("/iroh/clients/:id/permissions", get(iroh_client_permissions))
        .route("/iroh/scope-groups", get(iroh_scope_groups))
}

fn ok(v: anyhow::Result<Value>) -> Json<Value> {
    match v {
        Ok(mut val) => {
            val["ok"] = serde_json::json!(true);
            Json(val)
        }
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
    }
}

async fn iroh_info(State(state): State<AppState>) -> Json<Value> {
    ok(skill_iroh::commands::iroh_info(&state.iroh_auth, &state.iroh_runtime))
}

async fn iroh_phone_invite(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    ok(skill_iroh::commands::iroh_phone_invite(
        &state.iroh_auth,
        &state.iroh_runtime,
        &body,
    ))
}

async fn iroh_totp_list(State(state): State<AppState>) -> Json<Value> {
    ok(skill_iroh::commands::iroh_totp_list(&state.iroh_auth))
}

async fn iroh_totp_create(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    ok(skill_iroh::commands::iroh_totp_create(&state.iroh_auth, &body))
}

async fn iroh_totp_qr(State(state): State<AppState>, Path(id): Path<String>) -> Json<Value> {
    ok(skill_iroh::commands::iroh_totp_qr(
        &state.iroh_auth,
        &serde_json::json!({ "id": id }),
    ))
}

async fn iroh_totp_revoke(State(state): State<AppState>, Path(id): Path<String>) -> Json<Value> {
    ok(skill_iroh::commands::iroh_totp_revoke(
        &state.iroh_auth,
        &serde_json::json!({ "id": id }),
    ))
}

async fn iroh_clients_list(State(state): State<AppState>) -> Json<Value> {
    ok(skill_iroh::commands::iroh_clients_list(&state.iroh_auth))
}

async fn iroh_client_register(State(state): State<AppState>, Json(body): Json<Value>) -> Json<Value> {
    ok(skill_iroh::commands::iroh_client_register(&state.iroh_auth, &body))
}

async fn iroh_client_revoke(State(state): State<AppState>, Path(id): Path<String>) -> Json<Value> {
    ok(skill_iroh::commands::iroh_client_revoke(
        &state.iroh_auth,
        &serde_json::json!({ "id": id }),
    ))
}

async fn iroh_client_set_scope(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(mut body): Json<Value>,
) -> Json<Value> {
    body["id"] = serde_json::json!(id);
    ok(skill_iroh::commands::iroh_client_set_scope(&state.iroh_auth, &body))
}

async fn iroh_client_permissions(State(state): State<AppState>, Path(id): Path<String>) -> Json<Value> {
    ok(skill_iroh::commands::iroh_client_permissions(
        &state.iroh_auth,
        &serde_json::json!({ "id": id }),
    ))
}

async fn iroh_scope_groups(State(state): State<AppState>) -> Json<Value> {
    ok(skill_iroh::commands::iroh_scope_groups(&state.iroh_auth))
}
