#![allow(clippy::manual_let_else)]
// SPDX-License-Identifier: GPL-3.0-only
//! Brain awareness HTTP endpoints — `/v1/brain/*`.
//!
//! All computation lives in `ActivityStore` methods; these handlers just
//! open the store, call the method, and return JSON.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use skill_daemon_common::ApiError;
use skill_data::activity_store::ActivityStore;

use crate::state::AppState;

type BrainResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Run a read-only query against the activity store, returning a proper error
/// response if the store is unavailable or the task panics.
async fn run_query<T, F>(state: &AppState, f: F) -> BrainResult<T>
where
    T: Send + 'static,
    F: FnOnce(&ActivityStore) -> T + Send + 'static,
{
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    tokio::task::spawn_blocking(move || {
        let store = ActivityStore::open_readonly(&skill_dir).ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiError {
                    code: "db_unavailable",
                    message: "activity store offline".into(),
                }),
            )
        })?;
        Ok(Json(f(&store)))
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "task_error",
                message: e.to_string(),
            }),
        )
    })?
}

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
        .route("/brain/terminal-impact", post(terminal_impact))
        .route("/brain/context-cost", post(context_cost))
        .route("/brain/terminal-commands", post(terminal_commands))
        .route("/brain/terminal-input", post(terminal_input))
        .route("/brain/dev-loops", post(dev_loops))
        .route("/brain/ai-usage", post(ai_usage))
        .route("/brain/search-conversations", post(search_conversations))
        .route("/brain/eeg-at", post(eeg_at))
        .route("/brain/eeg-range", post(eeg_range))
        .route("/activity/timeline", post(timeline))
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn flow_state(State(state): State<AppState>, Json(req): Json<FlowRequest>) -> BrainResult<serde_json::Value> {
    let window = req.window_secs.unwrap_or(300);
    run_query(&state, move |s| {
        serde_json::to_value(s.flow_state_now(window)).unwrap_or_default()
    })
    .await
}

async fn cognitive_load(
    State(state): State<AppState>,
    Json(req): Json<CognitiveLoadRequest>,
) -> BrainResult<serde_json::Value> {
    let since = req.since.unwrap_or(0);
    let by_lang = req.group_by.as_deref() != Some("file");
    run_query(&state, move |s| {
        serde_json::to_value(s.cognitive_load_by(since, by_lang)).unwrap_or_default()
    })
    .await
}

async fn meeting_recovery(
    State(state): State<AppState>,
    Json(req): Json<SinceRequest>,
) -> BrainResult<serde_json::Value> {
    let since = req.since.unwrap_or(0);
    let limit = req.limit.unwrap_or(20);
    run_query(&state, move |s| {
        serde_json::to_value(s.meeting_recovery_times(since, limit)).unwrap_or_default()
    })
    .await
}

async fn optimal_hours(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> BrainResult<serde_json::Value> {
    let since = req.since.unwrap_or(0);
    let top_n = req.top_n.unwrap_or(5);
    let tz = chrono::Local::now().offset().local_minus_utc();
    run_query(&state, move |s| {
        serde_json::to_value(s.optimal_hours(since, top_n, tz)).unwrap_or_default()
    })
    .await
}

async fn fatigue(State(state): State<AppState>) -> BrainResult<serde_json::Value> {
    run_query(&state, |s| serde_json::to_value(s.fatigue_check()).unwrap_or_default()).await
}

async fn struggle(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> BrainResult<serde_json::Value> {
    let since = req.since.unwrap_or(0);
    let threshold = req.threshold.unwrap_or(5);
    run_query(&state, move |s| {
        serde_json::to_value(s.undo_struggle(since, threshold)).unwrap_or_default()
    })
    .await
}

async fn daily_report(State(state): State<AppState>, Json(req): Json<DayRequest>) -> BrainResult<serde_json::Value> {
    let day = req.day_start;
    run_query(&state, move |s| {
        serde_json::to_value(s.daily_brain_report(day)).unwrap_or_default()
    })
    .await
}

async fn break_timing(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> BrainResult<serde_json::Value> {
    let since = req.since.unwrap_or(0);
    run_query(&state, move |s| {
        serde_json::to_value(s.break_timing(since)).unwrap_or_default()
    })
    .await
}

async fn streak(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> BrainResult<serde_json::Value> {
    let mins = req.min_deep_work_mins.unwrap_or(60);
    run_query(&state, move |s| {
        serde_json::to_value(s.deep_work_streak(mins)).unwrap_or_default()
    })
    .await
}

async fn task_type(State(state): State<AppState>, Json(req): Json<FlowRequest>) -> BrainResult<serde_json::Value> {
    let window = req.window_secs.unwrap_or(300);
    run_query(&state, move |s| {
        serde_json::to_value(s.detect_task_type(window)).unwrap_or_default()
    })
    .await
}

async fn struggle_predict(
    State(state): State<AppState>,
    Json(req): Json<FlowRequest>,
) -> BrainResult<serde_json::Value> {
    let window = req.window_secs.unwrap_or(600);
    run_query(&state, move |s| {
        serde_json::to_value(s.predict_struggle(window)).unwrap_or_default()
    })
    .await
}

async fn interruption_recovery(
    State(state): State<AppState>,
    Json(req): Json<SinceRequest>,
) -> BrainResult<serde_json::Value> {
    let since = req.since.unwrap_or(0);
    let limit = req.limit.unwrap_or(20);
    run_query(&state, move |s| {
        serde_json::to_value(s.interruption_recovery(since, limit)).unwrap_or_default()
    })
    .await
}

async fn code_eeg(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> BrainResult<serde_json::Value> {
    let since = req.since.unwrap_or(0);
    run_query(&state, move |s| {
        serde_json::to_value(s.code_eeg_correlation(since)).unwrap_or_default()
    })
    .await
}

async fn terminal_impact(
    State(state): State<AppState>,
    Json(req): Json<SinceRequest>,
) -> BrainResult<serde_json::Value> {
    let since = req.since.unwrap_or(0);
    run_query(&state, move |s| {
        serde_json::to_value(s.terminal_focus_impact(since)).unwrap_or_default()
    })
    .await
}

async fn context_cost(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> BrainResult<serde_json::Value> {
    let since = req.since.unwrap_or(0);
    run_query(&state, move |s| {
        serde_json::to_value(s.zone_switch_cost(since)).unwrap_or_default()
    })
    .await
}

async fn terminal_input(
    State(state): State<AppState>,
    Json(req): Json<SinceRequest>,
) -> BrainResult<serde_json::Value> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let since = req.since.unwrap_or(now.saturating_sub(86400));
    run_query(&state, move |s| {
        serde_json::to_value(s.terminal_input_activity(since)).unwrap_or_default()
    })
    .await
}

async fn ai_usage(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> BrainResult<serde_json::Value> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let since = req.since.unwrap_or(now.saturating_sub(86400));
    run_query(&state, move |s| {
        let events = s.get_recent_ai_events(500);
        let filtered: Vec<_> = events.iter().filter(|e| e.at >= since).collect();
        let shown = filtered
            .iter()
            .filter(|e| e.event_type == "ai_suggestion_shown")
            .count();
        let accepted = filtered
            .iter()
            .filter(|e| e.event_type == "ai_suggestion_accepted")
            .count();
        let rejected = filtered
            .iter()
            .filter(|e| e.event_type == "ai_suggestion_rejected")
            .count();
        let chats = filtered.iter().filter(|e| e.event_type == "ai_chat_start").count();
        let rate = if shown > 0 { accepted as f64 / shown as f64 } else { 0.0 };
        let mut by_source: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for e in &filtered {
            if !e.source.is_empty() {
                *by_source.entry(e.source.clone()).or_default() += 1;
            }
        }
        let sources: Vec<_> = by_source
            .into_iter()
            .map(|(s, c)| serde_json::json!({"source": s, "count": c}))
            .collect();
        serde_json::json!({
            "suggestions_shown": shown,
            "accepted": accepted,
            "rejected": rejected,
            "acceptance_rate": rate,
            "chat_sessions": chats,
            "by_source": sources,
        })
    })
    .await
}

/// Search conversations: mode = "fts" (full-text), "fuzzy" (LIKE), or "structured" (filters).
async fn search_conversations(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> BrainResult<serde_json::Value> {
    let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mode = body.get("mode").and_then(|v| v.as_str()).unwrap_or("fts").to_string();
    let app = body.get("app").and_then(|v| v.as_str()).map(|s| s.to_string());
    let role = body.get("role").and_then(|v| v.as_str()).map(|s| s.to_string());
    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as u32;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let since = body.get("since").and_then(|v| v.as_u64()).unwrap_or(0);
    let until = body.get("until").and_then(|v| v.as_u64()).unwrap_or(now);

    run_query(&state, move |s| {
        let results = match mode.as_str() {
            "fts" => s.search_conversations_fts(&query, limit),
            "fuzzy" => s.search_conversations_fuzzy(&query, limit),
            "structured" => s.search_conversations_structured(app.as_deref(), role.as_deref(), since, until, limit),
            _ => s.search_conversations_fts(&query, limit),
        };
        serde_json::to_value(results).unwrap_or_default()
    })
    .await
}

/// Get EEG metrics at a specific timestamp (nearest sample).
async fn eeg_at(State(state): State<AppState>, Json(body): Json<serde_json::Value>) -> BrainResult<serde_json::Value> {
    let ts = body.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);
    run_query(&state, move |s| s.eeg_at(ts).unwrap_or(serde_json::json!(null))).await
}

/// Get EEG time-series in a range (for charts, correlation).
async fn eeg_range(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> BrainResult<serde_json::Value> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let from = body
        .get("from")
        .and_then(|v| v.as_u64())
        .unwrap_or(now.saturating_sub(3600));
    let to = body.get("to").and_then(|v| v.as_u64()).unwrap_or(now);
    let max_points = body.get("maxPoints").and_then(|v| v.as_u64()).unwrap_or(500) as u32;
    run_query(&state, move |s| {
        let points = s.eeg_range(from, to, max_points);
        let arr: Vec<serde_json::Value> = points
            .into_iter()
            .map(|(ts, m)| serde_json::json!({"ts": ts, "metrics": m}))
            .collect();
        serde_json::to_value(arr).unwrap_or_default()
    })
    .await
}

async fn dev_loops(State(state): State<AppState>, Json(req): Json<FlowRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let window = req.window_secs.unwrap_or(3600);
    let result = tokio::task::spawn_blocking(move || {
        ActivityStore::open_readonly(&skill_dir).map(|s| s.detect_dev_loops(window))
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn terminal_commands(
    State(state): State<AppState>,
    Json(req): Json<SinceRequest>,
) -> BrainResult<serde_json::Value> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let since = req.since.unwrap_or(now.saturating_sub(86400));
    let limit = req.limit.unwrap_or(50);
    run_query(&state, move |s| {
        serde_json::to_value(s.get_recent_terminal_commands(limit, since)).unwrap_or_default()
    })
    .await
}

async fn timeline(State(state): State<AppState>, Json(req): Json<SinceRequest>) -> BrainResult<serde_json::Value> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let since = req.since.unwrap_or(now.saturating_sub(86400));
    let limit = req.limit.unwrap_or(100);
    run_query(&state, move |s| {
        serde_json::to_value(s.activity_timeline(since, now, limit)).unwrap_or_default()
    })
    .await
}
