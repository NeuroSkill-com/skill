#![allow(clippy::manual_let_else)]
// SPDX-License-Identifier: GPL-3.0-only
//! Brain awareness HTTP endpoints — `/v1/brain/*`.
//!
//! All computation lives in `ActivityStore` methods; these handlers just
//! open the store, call the method, and return JSON.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use skill_data::activity_store::ActivityStore;

use crate::state::AppState;

// ── Request types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlowRequest {
    window_secs: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CognitiveLoadRequest {
    since: Option<u64>,
    group_by: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SinceRequest {
    since: Option<u64>,
    limit: Option<u32>,
    top_n: Option<usize>,
    min_deep_work_mins: Option<u32>,
    threshold: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DayRequest {
    day_start: u64,
}

// ── Router ───────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/brain/flow-state", post(flow_state))
        .route("/brain/cognitive-load", post(cognitive_load))
        .route("/brain/meeting-recovery", post(meeting_recovery))
        .route("/brain/optimal-hours", post(optimal_hours))
        .route("/brain/fatigue", get(fatigue))
        .route("/brain/struggle", post(struggle))
        .route("/brain/daily-report", post(daily_report))
        .route("/brain/break-timing", post(break_timing))
        .route("/brain/streak", post(streak))
        .route("/brain/task-type", post(task_type))
        .route("/brain/struggle-predict", post(struggle_predict))
        .route("/brain/interruption-recovery", post(interruption_recovery))
        .route("/brain/code-eeg", post(code_eeg))
        .route("/activity/timeline", post(timeline))
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn flow_state(State(state): State<AppState>, Json(req): Json<FlowRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let window = req.window_secs.unwrap_or(300);
    let result = tokio::task::spawn_blocking(move || ActivityStore::open(&skill_dir).map(|s| s.flow_state_now(window)))
        .await
        .ok()
        .flatten();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn cognitive_load(
    State(state): State<AppState>,
    Json(req): Json<CognitiveLoadRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let since = req.since.unwrap_or(0);
    let by_lang = req.group_by.as_deref() != Some("file");
    let result = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir).map(|s| s.cognitive_load_by(since, by_lang))
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn meeting_recovery(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let since = req.since.unwrap_or(0);
    let limit = req.limit.unwrap_or(20);
    let result = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir).map(|s| s.meeting_recovery_times(since, limit))
    })
    .await
    .ok()
    .flatten();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn optimal_hours(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let since = req.since.unwrap_or(0);
    let top_n = req.top_n.unwrap_or(5);
    let result =
        tokio::task::spawn_blocking(move || ActivityStore::open(&skill_dir).map(|s| s.optimal_hours(since, top_n)))
            .await
            .ok()
            .flatten();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn fatigue(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let result = tokio::task::spawn_blocking(move || ActivityStore::open(&skill_dir).map(|s| s.fatigue_check()))
        .await
        .ok()
        .flatten();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn struggle(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let since = req.since.unwrap_or(0);
    let threshold = req.threshold.unwrap_or(5);
    let result =
        tokio::task::spawn_blocking(move || ActivityStore::open(&skill_dir).map(|s| s.undo_struggle(since, threshold)))
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn daily_report(State(state): State<AppState>, Json(req): Json<DayRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let day = req.day_start;
    let result =
        tokio::task::spawn_blocking(move || ActivityStore::open(&skill_dir).map(|s| s.daily_brain_report(day)))
            .await
            .ok()
            .flatten();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn break_timing(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let since = req.since.unwrap_or(0);
    let result = tokio::task::spawn_blocking(move || ActivityStore::open(&skill_dir).map(|s| s.break_timing(since)))
        .await
        .ok()
        .flatten();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn streak(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let mins = req.min_deep_work_mins.unwrap_or(60);
    let result = tokio::task::spawn_blocking(move || ActivityStore::open(&skill_dir).map(|s| s.deep_work_streak(mins)))
        .await
        .ok()
        .flatten();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn task_type(State(state): State<AppState>, Json(req): Json<FlowRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let window = req.window_secs.unwrap_or(300);
    let result =
        tokio::task::spawn_blocking(move || ActivityStore::open(&skill_dir).map(|s| s.detect_task_type(window)))
            .await
            .ok()
            .flatten();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn struggle_predict(State(state): State<AppState>, Json(req): Json<FlowRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let window = req.window_secs.unwrap_or(600);
    let result =
        tokio::task::spawn_blocking(move || ActivityStore::open(&skill_dir).map(|s| s.predict_struggle(window)))
            .await
            .ok()
            .flatten();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn interruption_recovery(
    State(state): State<AppState>,
    Json(req): Json<SinceRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let since = req.since.unwrap_or(0);
    let limit = req.limit.unwrap_or(20);
    let result = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir).map(|s| s.interruption_recovery(since, limit))
    })
    .await
    .ok()
    .flatten();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn code_eeg(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let since = req.since.unwrap_or(0);
    let result =
        tokio::task::spawn_blocking(move || ActivityStore::open(&skill_dir).map(|s| s.code_eeg_correlation(since)))
            .await
            .ok()
            .flatten();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn timeline(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let since = req.since.unwrap_or(now.saturating_sub(86400));
    let limit = req.limit.unwrap_or(100);
    let result = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir).map(|s| s.activity_timeline(since, now, limit))
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default();
    Json(serde_json::to_value(result).unwrap_or_default())
}
