// SPDX-License-Identifier: GPL-3.0-only
//! Test-mode endpoints — only available in debug builds.
//!
//! `POST /v1/test/begin` — pause background work for stable E2E testing.
//! `POST /v1/test/end`   — resume background work.
//! `GET  /v1/test/status` — check test mode state.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/test/begin", post(test_begin))
        .route("/test/end", post(test_end))
        .route("/test/status", get(test_status))
}

async fn test_begin(State(state): State<AppState>) -> Json<serde_json::Value> {
    use std::sync::atomic::Ordering::Relaxed;

    state.test_mode.store(true, Relaxed);

    // Pause background workers by signalling their cancel flags
    state.idle_reembed_cancel.store(true, Relaxed);

    // Stop the scanner if running
    if let Ok(mut tx) = state.scanner_stop_tx.lock() {
        if let Some(stop) = tx.take() {
            let _ = stop.send(());
        }
    }
    if let Ok(mut running) = state.scanner_running.lock() {
        *running = false;
    }

    tracing::info!("[test-mode] BEGIN — background work paused");
    Json(serde_json::json!({ "ok": true, "test_mode": true }))
}

async fn test_end(State(state): State<AppState>) -> Json<serde_json::Value> {
    use std::sync::atomic::Ordering::Relaxed;

    state.test_mode.store(false, Relaxed);

    // Resume idle re-embed
    state.idle_reembed_cancel.store(false, Relaxed);

    tracing::info!("[test-mode] END — background work resumed");
    Json(serde_json::json!({ "ok": true, "test_mode": false }))
}

async fn test_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    use std::sync::atomic::Ordering::Relaxed;
    Json(serde_json::json!({
        "test_mode": state.test_mode.load(Relaxed),
        "ready": state.ready.load(Relaxed),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_begin_end_toggles_mode() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        assert!(!state.test_mode.load(std::sync::atomic::Ordering::Relaxed));

        let Json(v) = test_begin(State(state.clone())).await;
        assert_eq!(v["test_mode"], true);
        assert!(state.test_mode.load(std::sync::atomic::Ordering::Relaxed));

        let Json(v) = test_end(State(state.clone())).await;
        assert_eq!(v["test_mode"], false);
        assert!(!state.test_mode.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_begin_cancels_reembed() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        state
            .idle_reembed_cancel
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let _ = test_begin(State(state.clone())).await;
        assert!(state.idle_reembed_cancel.load(std::sync::atomic::Ordering::Relaxed));
    }
}
