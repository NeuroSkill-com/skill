// SPDX-License-Identifier: GPL-3.0-only
//! Validation HTTP endpoints — `/v1/validation/*`.
//!
//! All inputs/outputs are JSON.  The daemon is the single source of truth
//! for validation config and prompt scheduling; the VS Code extension and
//! the Tauri preferences pane are dumb clients that POST/GET against this
//! module.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Local, TimeZone, Timelike};
use serde::Deserialize;
use skill_daemon_common::ApiError;
use skill_data::activity_store::ActivityStore;
use skill_data::validation_store::{
    self, decide_prompt, eeg_fatigue_index, KssRecord, PromptDecision, PvtRecord, SchedulerCtx, TlxRecord,
    ValidationConfig, ValidationStore,
};

use crate::state::AppState;

type R<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

fn err500(code: &'static str, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            code,
            message: msg.into(),
        }),
    )
}

fn err503(code: &'static str, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiError {
            code,
            message: msg.into(),
        }),
    )
}

fn open_store(state: &AppState) -> Result<ValidationStore, (StatusCode, Json<ApiError>)> {
    let dir = state
        .skill_dir
        .lock()
        .map(|g| g.clone())
        .map_err(|_| err500("lock", "skill_dir poisoned"))?;
    ValidationStore::open(&dir).ok_or_else(|| err503("db_unavailable", "validation store offline"))
}

fn next_local_midnight(now: DateTime<Local>) -> i64 {
    let tomorrow = (now + chrono::Duration::days(1))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|n| Local.from_local_datetime(&n).single())
        .unwrap_or(now);
    tomorrow.timestamp()
}

// ── Config ───────────────────────────────────────────────────────────────────

async fn get_config(State(state): State<AppState>) -> R<ValidationConfig> {
    let store = open_store(&state)?;
    Ok(Json(store.load_config()))
}

#[derive(Deserialize)]
struct ConfigPatch(serde_json::Value);

async fn patch_config(State(state): State<AppState>, Json(patch): Json<ConfigPatch>) -> R<ValidationConfig> {
    let store = open_store(&state)?;
    let mut current = serde_json::to_value(store.load_config()).map_err(|e| err500("serde", e.to_string()))?;
    merge_json(&mut current, patch.0);
    let new_cfg: ValidationConfig =
        serde_json::from_value(current).map_err(|e| err500("invalid_patch", e.to_string()))?;
    store
        .save_config(&new_cfg)
        .map_err(|e| err500("persist", e.to_string()))?;
    Ok(Json(new_cfg))
}

/// Recursive object merge: `dst.field = src.field` for each key in `src`.
fn merge_json(dst: &mut serde_json::Value, src: serde_json::Value) {
    match (dst, src) {
        (serde_json::Value::Object(d), serde_json::Value::Object(s)) => {
            for (k, v) in s {
                merge_json(d.entry(k).or_insert(serde_json::Value::Null), v);
            }
        }
        (slot, src) => *slot = src,
    }
}

// ── Runtime: snooze / disable today ─────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct SnoozeReq {
    channel: String,
    duration_secs: i64,
}

async fn snooze(State(state): State<AppState>, Json(req): Json<SnoozeReq>) -> R<serde_json::Value> {
    let mut rt = state
        .validation_runtime
        .lock()
        .map_err(|_| err500("lock", "validation_runtime poisoned"))?;
    rt.snooze(&req.channel, req.duration_secs.max(0));
    Ok(Json(
        serde_json::json!({ "ok": true, "channel": req.channel, "duration_secs": req.duration_secs }),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct DisableTodayReq {
    channel: String,
}

async fn disable_today(State(state): State<AppState>, Json(req): Json<DisableTodayReq>) -> R<serde_json::Value> {
    let midnight = next_local_midnight(Local::now());
    let mut rt = state
        .validation_runtime
        .lock()
        .map_err(|_| err500("lock", "validation_runtime poisoned"))?;
    rt.disable_today(&req.channel, midnight);
    Ok(Json(serde_json::json!({
        "ok": true, "channel": req.channel, "until_unix": midnight,
    })))
}

// ── Scheduler decision ──────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct ShouldPromptQuery {
    /// VS Code or Tauri tells the daemon what *just* finished, so the
    /// scheduler can decide whether a TLX prompt is appropriate.  Optional —
    /// callers that don't know just omit it.
    #[serde(default)]
    last_task_kind: Option<String>,
    #[serde(default)]
    last_task_duration_secs: Option<i64>,
    /// Surface that's asking; logged with the prompt so we can study which
    /// surface produces higher response rates.
    #[serde(default)]
    surface: Option<String>,
}

async fn should_prompt(State(state): State<AppState>, Query(q): Query<ShouldPromptQuery>) -> R<serde_json::Value> {
    let store = open_store(&state)?;
    let cfg = store.load_config();

    let in_flow = read_in_flow(&state).unwrap_or(false);
    let break_coach_active = read_break_coach_active(&state).unwrap_or(false);

    let now_local = Local::now();
    let local_hour = now_local.hour() as u8;
    let now_unix = now_local.timestamp();
    let midnight = next_local_midnight(now_local);

    let runtime_snapshot = {
        let rt = state
            .validation_runtime
            .lock()
            .map_err(|_| err500("lock", "validation_runtime poisoned"))?;
        rt.clone()
    };

    let ctx = SchedulerCtx {
        config: &cfg,
        runtime: &runtime_snapshot,
        store: &store,
        now_unix,
        local_hour,
        local_midnight_next: midnight,
        in_flow,
    };

    let decision = decide_prompt(
        &ctx,
        break_coach_active,
        q.last_task_kind.as_deref(),
        q.last_task_duration_secs,
    );

    // Log the prompt fire so the rate-limiter sees it.  We don't log a
    // 'None' decision — there's no prompt to track in that case.
    let surface = q.surface.unwrap_or_else(|| "unknown".into());
    let prompt_id = match &decision {
        PromptDecision::Kss { triggered_by } => store.log_prompt("kss", triggered_by, &surface).ok(),
        PromptDecision::Tlx { triggered_by, .. } => store.log_prompt("tlx", triggered_by, &surface).ok(),
        PromptDecision::Pvt { triggered_by } => store.log_prompt("pvt", triggered_by, &surface).ok(),
        PromptDecision::None { .. } => None,
    };

    let mut payload = serde_json::to_value(&decision).map_err(|e| err500("serde", e.to_string()))?;
    if let Some(id) = prompt_id {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("prompt_id".into(), serde_json::json!(id));
        }
    }
    Ok(Json(payload))
}

/// Open the activity store read-only and ask whether the user is currently
/// in a flow block (5-minute window).  Returns `None` if the store is
/// unavailable — the scheduler then conservatively treats it as not-in-flow,
/// which only costs a missed prompt (false negatives are cheaper than
/// false positives here).
fn read_in_flow(state: &AppState) -> Option<bool> {
    let dir = state.skill_dir.lock().ok()?.clone();
    let store = ActivityStore::open_readonly(&dir)?;
    Some(store.flow_state_now(300).in_flow)
}

/// True when the user is currently fatigued per the daemon's brain module
/// (the "Break Coach" trigger).  Same fall-through as `read_in_flow`: if
/// the store can't be opened we return `None` and let the scheduler skip
/// the break-coach branch.
fn read_break_coach_active(state: &AppState) -> Option<bool> {
    let dir = state.skill_dir.lock().ok()?.clone();
    let store = ActivityStore::open_readonly(&dir)?;
    Some(store.fatigue_check().fatigued)
}

// ── Recording answers ───────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct KssAnswer {
    score: u8,
    triggered_by: String,
    surface: String,
    in_flow: bool,
    focus_score: Option<f64>,
    fatigue_idx: Option<f64>,
    /// If the prompt came from the scheduler, the client echoes back the
    /// id we returned — we use it to mark the prompt 'answered'.
    #[serde(default)]
    prompt_id: Option<i64>,
}

async fn record_kss(State(state): State<AppState>, Json(ans): Json<KssAnswer>) -> R<serde_json::Value> {
    if !(1..=9).contains(&ans.score) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "out_of_range",
                message: "kss score must be 1..=9".into(),
            }),
        ));
    }
    let store = open_store(&state)?;
    let id = store
        .record_kss(&KssRecord {
            score: ans.score,
            triggered_by: ans.triggered_by,
            surface: ans.surface,
            in_flow: ans.in_flow,
            focus_score: ans.focus_score,
            fatigue_idx: ans.fatigue_idx,
        })
        .map_err(|e| err500("persist", e.to_string()))?;
    if let Some(pid) = ans.prompt_id {
        let _ = store.close_prompt(pid, "answered");
    }
    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct TlxAnswer {
    mental: u8,
    physical: u8,
    temporal: u8,
    performance: u8,
    effort: u8,
    frustration: u8,
    task_kind: String,
    task_duration_secs: Option<i64>,
    surface: String,
    #[serde(default)]
    prompt_id: Option<i64>,
}

async fn record_tlx(State(state): State<AppState>, Json(ans): Json<TlxAnswer>) -> R<serde_json::Value> {
    for (name, v) in [
        ("mental", ans.mental),
        ("physical", ans.physical),
        ("temporal", ans.temporal),
        ("performance", ans.performance),
        ("effort", ans.effort),
        ("frustration", ans.frustration),
    ] {
        if v > 100 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    code: "out_of_range",
                    message: format!("{name} must be 0..=100"),
                }),
            ));
        }
    }
    let store = open_store(&state)?;
    let id = store
        .record_tlx(&TlxRecord {
            mental: ans.mental,
            physical: ans.physical,
            temporal: ans.temporal,
            performance: ans.performance,
            effort: ans.effort,
            frustration: ans.frustration,
            task_kind: ans.task_kind,
            task_duration_secs: ans.task_duration_secs,
            surface: ans.surface,
        })
        .map_err(|e| err500("persist", e.to_string()))?;
    if let Some(pid) = ans.prompt_id {
        let _ = store.close_prompt(pid, "answered");
    }
    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct PvtRun(PvtRecord);

async fn record_pvt(State(state): State<AppState>, Json(run): Json<PvtRun>) -> R<serde_json::Value> {
    let store = open_store(&state)?;
    let id = store.record_pvt(&run.0).map_err(|e| err500("persist", e.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

// ── Prompt outcome (snooze/dismiss without answering) ───────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct PromptOutcome {
    prompt_id: i64,
    /// 'snoozed' | 'dismissed' | 'disabled_today' | 'disabled_perm'
    outcome: String,
}

async fn close_prompt(State(state): State<AppState>, Json(req): Json<PromptOutcome>) -> R<serde_json::Value> {
    let store = open_store(&state)?;
    store
        .close_prompt(req.prompt_id, &req.outcome)
        .map_err(|e| err500("persist", e.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Read-back: results + correlations summary ───────────────────────────────

#[derive(Deserialize)]
struct ResultsQuery {
    #[serde(default)]
    since: Option<i64>,
}

async fn results(State(state): State<AppState>, Query(q): Query<ResultsQuery>) -> R<serde_json::Value> {
    let store = open_store(&state)?;
    let since = q.since.unwrap_or(0);
    let kss = store.recent_kss(since);
    let kss_json: Vec<_> = kss
        .into_iter()
        .map(|(id, score, trigger, ts)| {
            serde_json::json!({
                "id": id, "score": score, "triggered_by": trigger, "ts": ts,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "kss": kss_json,
        // tlx + pvt readers can be added similarly when the UIs need them.
    })))
}

// ── EEG fatigue index (derived, on demand) ──────────────────────────────────

async fn fatigue_index(State(state): State<AppState>) -> R<serde_json::Value> {
    // The session runner publishes `latest_bands` as a JSON snapshot at ~4 Hz.
    let bands = state
        .latest_bands
        .lock()
        .map_err(|_| err500("lock", "latest_bands poisoned"))?
        .clone();
    let value = bands.as_ref().and_then(eeg_fatigue_index);
    Ok(Json(serde_json::json!({
        "fatigue_idx": value,
        "formula": "(alpha + theta) / beta",
        "reference": "Jap et al. 2009; doi:10.1016/j.eswa.2007.12.043",
    })))
}

// ── Router ──────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/validation/config", get(get_config).patch(patch_config))
        .route("/validation/snooze", post(snooze))
        .route("/validation/disable-today", post(disable_today))
        .route("/validation/should-prompt", get(should_prompt))
        .route("/validation/kss", post(record_kss))
        .route("/validation/tlx", post(record_tlx))
        .route("/validation/pvt", post(record_pvt))
        .route("/validation/close-prompt", post(close_prompt))
        .route("/validation/results", get(results))
        .route("/validation/fatigue-index", get(fatigue_index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tempfile::TempDir;
    use tower::ServiceExt;

    #[test]
    fn merge_json_overwrites_leaves() {
        let mut dst = serde_json::json!({ "kss": { "enabled": false, "max_per_day": 4 } });
        let src = serde_json::json!({ "kss": { "enabled": true } });
        merge_json(&mut dst, src);
        assert_eq!(dst["kss"]["enabled"], serde_json::json!(true));
        // Untouched leaves preserved.
        assert_eq!(dst["kss"]["max_per_day"], serde_json::json!(4));
    }

    #[test]
    fn merge_json_adds_new_keys() {
        let mut dst = serde_json::json!({});
        let src = serde_json::json!({ "respect_flow": false });
        merge_json(&mut dst, src);
        assert_eq!(dst["respect_flow"], serde_json::json!(false));
    }

    #[test]
    fn merge_json_deep_nested() {
        let mut dst = serde_json::json!({
            "kss": { "enabled": false, "max_per_day": 4, "trigger_random": true }
        });
        let src = serde_json::json!({ "kss": { "max_per_day": 8 } });
        merge_json(&mut dst, src);
        assert_eq!(dst["kss"]["max_per_day"], serde_json::json!(8));
        assert_eq!(dst["kss"]["enabled"], serde_json::json!(false));
        assert_eq!(dst["kss"]["trigger_random"], serde_json::json!(true));
    }

    fn test_state() -> (TempDir, AppState) {
        let td = TempDir::new().unwrap();
        let state = AppState::new("test-token".to_string(), td.path().to_path_buf());
        (td, state)
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn http_get_config_returns_defaults() {
        let (_td, state) = test_state();
        let app = router().with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/validation/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = body_json(resp).await;
        assert_eq!(body["respect_flow"], serde_json::json!(true));
        assert_eq!(body["kss"]["enabled"], serde_json::json!(false));
        assert_eq!(body["eeg_fatigue"]["enabled"], serde_json::json!(true));
    }

    #[tokio::test]
    async fn http_patch_config_persists_partial_update() {
        let (_td, state) = test_state();
        let app = router().with_state(state.clone());

        // Patch: enable KSS + set max_per_day = 6.
        let patch = serde_json::json!({ "kss": { "enabled": true, "max_per_day": 6 } });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/validation/config")
                    .header("content-type", "application/json")
                    .body(Body::from(patch.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = body_json(resp).await;
        assert_eq!(body["kss"]["enabled"], serde_json::json!(true));
        assert_eq!(body["kss"]["max_per_day"], serde_json::json!(6));
        // Untouched fields preserved.
        assert_eq!(body["respect_flow"], serde_json::json!(true));

        // Round-trip: subsequent GET sees the change.
        let resp2 = app
            .oneshot(
                Request::builder()
                    .uri("/validation/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body2 = body_json(resp2).await;
        assert_eq!(body2["kss"]["enabled"], serde_json::json!(true));
        assert_eq!(body2["kss"]["max_per_day"], serde_json::json!(6));
    }

    #[tokio::test]
    async fn http_record_kss_persists() {
        let (_td, state) = test_state();
        let app = router().with_state(state);
        let payload = serde_json::json!({
            "score": 7,
            "triggered_by": "manual",
            "surface": "vscode",
            "in_flow": false,
            "focus_score": null,
            "fatigue_idx": null,
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/validation/kss")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = body_json(resp).await;
        assert_eq!(body["ok"], serde_json::json!(true));
        assert!(body["id"].as_i64().unwrap() > 0);
    }

    #[tokio::test]
    async fn http_record_kss_rejects_out_of_range_score() {
        let (_td, state) = test_state();
        let app = router().with_state(state);
        // Score 10 is invalid — KSS only goes 1..=9.
        let payload = serde_json::json!({
            "score": 10,
            "triggered_by": "manual",
            "surface": "vscode",
            "in_flow": false,
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/validation/kss")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn http_record_tlx_rejects_out_of_range_subscale() {
        let (_td, state) = test_state();
        let app = router().with_state(state);
        let payload = serde_json::json!({
            "mental": 50,
            "physical": 50,
            "temporal": 50,
            "performance": 50,
            "effort": 200,    // > 100, invalid
            "frustration": 50,
            "task_kind": "manual",
            "surface": "tauri",
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/validation/tlx")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn http_should_prompt_returns_none_with_default_config() {
        // Defaults: KSS/TLX/PVT all disabled → should always return "none".
        let (_td, state) = test_state();
        let app = router().with_state(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/validation/should-prompt?surface=vscode")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = body_json(resp).await;
        assert_eq!(body["kind"], serde_json::json!("none"));
    }

    #[tokio::test]
    async fn http_snooze_returns_ok_envelope() {
        let (_td, state) = test_state();
        let app = router().with_state(state);
        let payload = serde_json::json!({ "channel": "kss", "duration_secs": 1800 });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/validation/snooze")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = body_json(resp).await;
        assert_eq!(body["ok"], serde_json::json!(true));
        assert_eq!(body["channel"], serde_json::json!("kss"));
    }
}

// suppress dead-code on the `_ = validation_store::ValidationConfig::default();` only used in tests
#[allow(dead_code)]
fn _force_link() {
    let _ = validation_store::ValidationConfig::default();
}
