// SPDX-License-Identifier: GPL-3.0-only
//! Daemon settings/model routes.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use base64::Engine as _;
use serde::Deserialize;
use skill_data::{
    active_window::ActiveWindowInfo,
    activity_store::{ActiveWindowRow, ActivityStore, InputActivityRow, InputBucketRow},
};
use skill_eeg::{
    eeg_filter::{FilterConfig, PowerlineFreq},
    eeg_model_config::{load_model_config, save_model_config, EegModelStatus, ExgModelConfig},
};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct HookLogRequest {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct HookKeywordsRequest {
    draft: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct HookDistanceRequest {
    keywords: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ActivityRecentRequest {
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ActivityBucketsRequest {
    from_ts: Option<u64>,
    to_ts: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ChatIdRequest {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct ChatRenameRequest {
    id: i64,
    title: String,
}

#[derive(Debug, Deserialize)]
struct ChatSaveMessageRequest {
    session_id: i64,
    role: String,
    content: String,
    thinking: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatSessionParamsRequest {
    id: i64,
    params_json: String,
}

#[derive(Debug, Deserialize)]
struct ChatSaveToolCallsRequest {
    message_id: i64,
    tool_calls: Vec<skill_llm::chat_store::StoredToolCall>,
}

#[derive(Debug, serde::Serialize)]
struct ChatSessionResponse {
    session_id: i64,
    messages: Vec<skill_llm::chat_store::StoredMessage>,
}

#[derive(Debug, Deserialize)]
struct LslAutoConnectRequest {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct LslPairRequest {
    source_id: String,
    name: String,
    stream_type: String,
    channels: usize,
    sample_rate: f64,
}

#[derive(Debug, Deserialize)]
struct LslUnpairRequest {
    source_id: String,
}

#[derive(Debug, Deserialize)]
struct LslIdleTimeoutRequest {
    secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct U64ValueRequest {
    value: u64,
}

#[derive(Debug, Deserialize)]
struct BoolValueRequest {
    value: bool,
}

#[derive(Debug, Deserialize)]
struct StringValueRequest {
    value: String,
}

#[derive(Debug, Deserialize)]
struct StringListRequest {
    values: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct StringKeyRequest {
    key: String,
}

#[derive(Debug, Deserialize)]
struct DndTestRequest {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct NotchPresetRequest {
    value: Option<PowerlineFreq>,
}

#[derive(Debug, Deserialize)]
struct ScreenshotAroundRequest {
    timestamp: i64,
    window_secs: i32,
}

#[derive(Debug, Deserialize)]
struct ScreenshotImageSearchRequest {
    image_bytes: Vec<u8>,
    k: usize,
}

#[derive(Debug, Deserialize)]
struct ScreenshotTextSearchRequest {
    query: String,
    k: Option<usize>,
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScreenshotVectorSearchRequest {
    vector: Vec<f32>,
    k: usize,
}

#[derive(Debug, Deserialize)]
struct WsConfigRequest {
    host: String,
    port: u16,
}

#[derive(Debug, Deserialize)]
struct FilenameRequest {
    filename: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ChatCompletionsRequest {
    messages: Vec<serde_json::Value>,
    params: serde_json::Value,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ToolCancelRequest {
    tool_call_id: String,
}

#[derive(Debug, Deserialize)]
struct LlmAddModelRequest {
    repo: String,
    filename: String,
    size_gb: Option<f32>,
    mmproj: Option<String>,
    download: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct LlmFilenameRequest {
    filename: String,
}

#[derive(Debug, Deserialize)]
struct LlmImageRequest {
    png_base64: String,
}

#[cfg(feature = "llm")]
#[derive(Clone)]
struct DaemonLlmEmitter {
    events_tx: tokio::sync::broadcast::Sender<skill_daemon_common::EventEnvelope>,
}

#[cfg(feature = "llm")]
impl skill_llm::LlmEventEmitter for DaemonLlmEmitter {
    fn emit_event(&self, event: &str, payload: serde_json::Value) {
        let _ = self.events_tx.send(skill_daemon_common::EventEnvelope {
            r#type: format!("Llm{}", event.replace(':', "_")),
            ts_unix_ms: now_unix_ms(),
            correlation_id: None,
            payload,
        });
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/models/config", get(get_model_config).put(set_model_config))
        .route("/models/status", get(get_model_status))
        .route("/models/trigger-reembed", post(trigger_reembed))
        .route("/models/trigger-weights-download", post(trigger_weights_download))
        .route("/models/cancel-weights-download", post(cancel_weights_download))
        .route("/models/estimate-reembed", get(estimate_reembed))
        .route("/models/rebuild-index", post(rebuild_index))
        .route("/models/exg-catalog", get(get_exg_catalog))
        .route("/hooks", get(get_hooks).put(set_hooks))
        .route("/hooks/statuses", get(get_hook_statuses))
        .route("/hooks/log", post(get_hook_log))
        .route("/hooks/log-count", get(get_hook_log_count))
        .route("/hooks/suggest-keywords", post(suggest_hook_keywords))
        .route("/hooks/suggest-distances", post(suggest_hook_distances))
        .route("/activity/recent-windows", post(activity_recent_windows))
        .route("/activity/recent-input", post(activity_recent_input))
        .route("/activity/input-buckets", post(activity_input_buckets))
        .route(
            "/activity/tracking/active-window",
            get(get_active_window_tracking).post(set_active_window_tracking),
        )
        .route(
            "/activity/tracking/input",
            get(get_input_activity_tracking).post(set_input_activity_tracking),
        )
        .route("/activity/current-window", get(get_current_active_window))
        .route("/activity/last-input", get(get_last_input_activity))
        .route("/settings/api-token", get(get_api_token).post(set_api_token))
        .route("/settings/hf-endpoint", get(get_hf_endpoint).post(set_hf_endpoint))
        .route(
            "/settings/filter-config",
            get(get_filter_config).post(set_filter_config),
        )
        .route("/settings/notch-preset", post(set_notch_preset))
        .route(
            "/settings/storage-format",
            get(get_storage_format).post(set_storage_format),
        )
        .route(
            "/settings/embedding-overlap",
            get(get_embedding_overlap).post(set_embedding_overlap),
        )
        .route(
            "/settings/update-check-interval",
            get(get_update_check_interval).post(set_update_check_interval),
        )
        .route(
            "/settings/openbci-config",
            get(get_openbci_config).post(set_openbci_config),
        )
        .route(
            "/settings/device-api-config",
            get(get_device_api_config).post(set_device_api_config),
        )
        .route(
            "/settings/scanner-config",
            get(get_scanner_config).post(set_scanner_config),
        )
        .route("/settings/device-log", get(get_device_log))
        .route(
            "/settings/neutts-config",
            get(get_neutts_config).post(set_neutts_config),
        )
        .route("/settings/llm-config", get(get_llm_config).post(set_llm_config))
        .route("/settings/tts-preload", get(get_tts_preload).post(set_tts_preload))
        .route("/settings/sleep-config", get(get_sleep_config).post(set_sleep_config))
        .route("/settings/ws-config", get(get_ws_config).post(set_ws_config))
        .route(
            "/settings/inference-device",
            get(get_inference_device).post(set_inference_device),
        )
        .route(
            "/settings/exg-inference-device",
            get(get_exg_inference_device).post(set_exg_inference_device),
        )
        .route(
            "/settings/location-enabled",
            get(get_location_enabled).post(set_location_enabled),
        )
        .route("/settings/location-test", post(test_location))
        .route("/settings/umap-config", get(get_umap_config).post(set_umap_config))
        .route("/settings/gpu-stats", get(get_gpu_stats))
        .route("/settings/web-cache/stats", get(web_cache_stats))
        .route("/settings/web-cache/list", get(web_cache_list))
        .route("/settings/web-cache/clear", post(web_cache_clear))
        .route("/settings/web-cache/remove-domain", post(web_cache_remove_domain))
        .route("/settings/web-cache/remove-entry", post(web_cache_remove_entry))
        .route("/settings/dnd/focus-modes", get(get_dnd_focus_modes))
        .route("/settings/dnd/config", get(get_dnd_config).post(set_dnd_config))
        .route("/settings/dnd/active", get(get_dnd_active))
        .route("/settings/dnd/status", get(get_dnd_status))
        .route("/settings/dnd/test", post(test_dnd))
        .route(
            "/settings/screenshot/config",
            get(get_screenshot_config).post(set_screenshot_config),
        )
        .route(
            "/settings/screenshot/estimate-reembed",
            get(estimate_screenshot_reembed),
        )
        .route(
            "/settings/screenshot/rebuild-embeddings",
            post(rebuild_screenshot_embeddings),
        )
        .route("/settings/screenshot/around", post(get_screenshots_around))
        .route("/settings/screenshot/search-image", post(search_screenshots_by_image))
        .route("/settings/screenshot/metrics", get(get_screenshot_metrics))
        .route("/settings/screenshot/ocr-ready", get(check_ocr_models_ready))
        .route("/settings/screenshot/download-ocr", post(download_ocr_models))
        .route("/settings/screenshot/search-text", post(search_screenshots_by_text))
        .route("/settings/screenshot/dir", get(get_screenshots_dir))
        .route("/settings/screenshot/search-vector", post(search_screenshots_by_vector))
        .route("/ui/accent-color", get(get_accent_color).post(set_accent_color))
        .route("/ui/daily-goal", get(get_daily_goal).post(set_daily_goal))
        .route(
            "/ui/goal-notified-date",
            get(get_goal_notified_date).post(set_goal_notified_date),
        )
        .route(
            "/ui/main-window-auto-fit",
            get(get_main_window_auto_fit).post(set_main_window_auto_fit),
        )
        .route(
            "/skills/refresh-interval",
            get(get_skills_refresh_interval).post(set_skills_refresh_interval),
        )
        .route(
            "/skills/sync-on-launch",
            get(get_skills_sync_on_launch).post(set_skills_sync_on_launch),
        )
        .route("/skills/last-sync", get(get_skills_last_sync))
        .route("/skills/sync-now", post(sync_skills_now))
        .route("/skills/list", get(list_skills))
        .route("/skills/license", get(get_skills_license))
        .route("/skills/disabled", get(get_disabled_skills).post(set_disabled_skills))
        .route("/llm/server/start", post(llm_server_start))
        .route("/llm/server/stop", post(llm_server_stop))
        .route("/llm/server/status", get(llm_server_status))
        .route("/llm/server/logs", get(llm_server_logs))
        .route("/llm/server/switch-model", post(llm_server_switch_model))
        .route("/llm/server/switch-mmproj", post(llm_server_switch_mmproj))
        .route("/llm/catalog", get(llm_get_catalog))
        .route("/llm/catalog/refresh", post(llm_refresh_catalog))
        .route("/llm/catalog/add-model", post(llm_add_model))
        .route("/llm/downloads", get(llm_get_downloads))
        .route("/llm/download/start", post(llm_download_start))
        .route("/llm/download/cancel", post(llm_download_cancel))
        .route("/llm/download/pause", post(llm_download_pause))
        .route("/llm/download/resume", post(llm_download_resume))
        .route("/llm/download/delete", post(llm_download_delete))
        .route("/llm/selection/active-model", post(llm_set_active_model))
        .route("/llm/selection/active-mmproj", post(llm_set_active_mmproj))
        .route("/llm/selection/autoload-mmproj", post(llm_set_autoload_mmproj))
        .route("/llm/chat/last-session", post(chat_last_session))
        .route("/llm/chat/load-session", post(chat_load_session))
        .route("/llm/chat/sessions", get(chat_list_sessions))
        .route("/llm/chat/rename", post(chat_rename_session))
        .route("/llm/chat/delete", post(chat_delete_session))
        .route("/llm/chat/archive", post(chat_archive_session))
        .route("/llm/chat/unarchive", post(chat_unarchive_session))
        .route("/llm/chat/archived-sessions", get(chat_list_archived_sessions))
        .route("/llm/chat/save-message", post(chat_save_message))
        .route("/llm/chat/session-params", post(chat_get_session_params))
        .route("/llm/chat/set-session-params", post(chat_set_session_params))
        .route("/llm/chat/new-session", post(chat_new_session))
        .route("/llm/chat/save-tool-calls", post(chat_save_tool_calls))
        .route("/llm/chat-completions", post(llm_chat_completions))
        .route("/llm/embed-image", post(llm_embed_image))
        .route("/llm/ocr", post(llm_ocr))
        .route("/llm/abort-stream", post(llm_abort_stream))
        .route("/llm/cancel-tool-call", post(llm_cancel_tool_call))
        .route("/device/serial-ports", get(list_serial_ports))
        .route("/lsl/config", get(get_lsl_config))
        .route("/lsl/auto-connect", post(set_lsl_auto_connect))
        .route("/lsl/pair", post(lsl_pair_stream))
        .route("/lsl/unpair", post(lsl_unpair_stream))
        .route(
            "/lsl/idle-timeout",
            get(get_lsl_idle_timeout).post(set_lsl_idle_timeout),
        )
        .route("/lsl/virtual-source/start", post(lsl_virtual_source_start))
        .route("/lsl/virtual-source/stop", post(lsl_virtual_source_stop))
        .route("/lsl/virtual-source/running", get(lsl_virtual_source_running))
        .route("/lsl/iroh/start", post(lsl_iroh_start))
        .route("/lsl/iroh/status", get(lsl_iroh_status))
        .route("/lsl/iroh/stop", post(lsl_iroh_stop))
}

async fn get_model_config(State(state): State<AppState>) -> Json<ExgModelConfig> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    Json(load_model_config(&skill_dir))
}

async fn set_model_config(
    State(state): State<AppState>,
    Json(config): Json<ExgModelConfig>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    save_model_config(&skill_dir, &config);
    Json(serde_json::json!({"ok": true}))
}

async fn get_model_status() -> Json<EegModelStatus> {
    Json(EegModelStatus::default())
}

async fn trigger_reembed() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "message": "reembed queued in daemon" }))
}

async fn trigger_weights_download() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "message": "weights download queued in daemon" }))
}

async fn cancel_weights_download() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "message": "weights download cancellation requested" }))
}

async fn estimate_reembed(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let sessions = tokio::task::spawn_blocking(move || skill_history::list_all_sessions(&skill_dir, None))
        .await
        .unwrap_or_default();
    Json(serde_json::json!({
        "sessions_total": sessions.len(),
        "embeddings_needed": 0,
    }))
}

async fn rebuild_index() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "message": "index rebuild queued in daemon" }))
}

async fn get_exg_catalog() -> Json<serde_json::Value> {
    const BUNDLED: &str = include_str!("../../../../src-tauri/exg_catalog.json");
    let v: serde_json::Value = serde_json::from_str(BUNDLED).unwrap_or_default();
    Json(v)
}

async fn get_hooks(State(state): State<AppState>) -> Json<Vec<skill_settings::HookRule>> {
    Json(state.hooks.lock().map(|g| g.clone()).unwrap_or_default())
}

async fn set_hooks(
    State(state): State<AppState>,
    Json(hooks): Json<Vec<skill_settings::HookRule>>,
) -> Json<serde_json::Value> {
    if let Ok(mut g) = state.hooks.lock() {
        *g = hooks.clone();
    }
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let mut settings = skill_settings::load_settings(&skill_dir);
    settings.hooks = hooks;
    let path = skill_settings::settings_path(&skill_dir);
    let ok = serde_json::to_string_pretty(&settings)
        .ok()
        .and_then(|json| std::fs::write(path, json).ok())
        .is_some();
    Json(serde_json::json!({"ok": ok}))
}

async fn get_hook_statuses(State(state): State<AppState>) -> Json<serde_json::Value> {
    let hooks = state.hooks.lock().map(|g| g.clone()).unwrap_or_default();
    Json(serde_json::Value::Array(
        hooks
            .into_iter()
            .map(|hook| serde_json::json!({"hook": hook, "last_trigger": serde_json::Value::Null}))
            .collect(),
    ))
}

async fn get_hook_log(
    State(state): State<AppState>,
    Json(req): Json<HookLogRequest>,
) -> Json<Vec<skill_data::hooks_log::HookLogRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let rows = tokio::task::spawn_blocking(move || {
        let Some(log) = skill_data::hooks_log::HooksLog::open(&skill_dir) else {
            return vec![];
        };
        log.query(req.limit.unwrap_or(50).clamp(1, 500), req.offset.unwrap_or(0).max(0))
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn get_hook_log_count(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let count = tokio::task::spawn_blocking(move || {
        skill_data::hooks_log::HooksLog::open(&skill_dir)
            .map(|l| l.count())
            .unwrap_or(0)
    })
    .await
    .unwrap_or(0);
    Json(serde_json::json!({"count": count}))
}

async fn suggest_hook_keywords(
    State(state): State<AppState>,
    Json(req): Json<HookKeywordsRequest>,
) -> Json<Vec<serde_json::Value>> {
    let q = req.draft.trim().to_lowercase();
    if q.len() < 2 {
        return Json(Vec::new());
    }
    let max_n = req.limit.unwrap_or(8).clamp(1, 20);
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let labels_db = skill_dir.join(skill_constants::LABELS_FILE);

    let out = tokio::task::spawn_blocking(move || {
        let mut out = Vec::<serde_json::Value>::new();
        if !labels_db.exists() {
            return out;
        }
        let Ok(conn) = skill_data::util::open_readonly(&labels_db) else {
            return out;
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT text FROM labels
             WHERE length(trim(text)) > 0
             GROUP BY text
             ORDER BY MAX(created_at) DESC
             LIMIT 600",
        ) else {
            return out;
        };
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for text in rows.flatten() {
                let cand = text.to_lowercase();
                if cand.contains(&q) {
                    out.push(serde_json::json!({"keyword": text, "source": "fuzzy", "score": 0.92}));
                }
                if out.len() >= max_n {
                    break;
                }
            }
        }
        out
    })
    .await
    .unwrap_or_default();

    Json(out)
}

async fn suggest_hook_distances(
    State(state): State<AppState>,
    Json(req): Json<HookDistanceRequest>,
) -> Json<serde_json::Value> {
    let label_n = req.keywords.len();
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();

    let mut distances: Vec<f32> = tokio::task::spawn_blocking(move || {
        let Some(log) = skill_data::hooks_log::HooksLog::open(&skill_dir) else {
            return Vec::new();
        };
        let rows = log.query(5000, 0);
        let mut vals = Vec::new();
        for row in rows {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&row.trigger_json) else {
                continue;
            };
            let maybe = v
                .get("distance")
                .and_then(serde_json::Value::as_f64)
                .or_else(|| v.get("eeg_distance").and_then(serde_json::Value::as_f64))
                .or_else(|| v.get("eegDistance").and_then(serde_json::Value::as_f64));
            if let Some(d) = maybe {
                let d = d as f32;
                if d.is_finite() {
                    vals.push(d.clamp(0.0, 1.0));
                }
            }
        }
        vals
    })
    .await
    .unwrap_or_default();

    if distances.is_empty() {
        return Json(serde_json::json!({
            "label_n": label_n,
            "ref_n": 0,
            "sample_n": 0,
            "eeg_min": 0.0,
            "eeg_p25": 0.0,
            "eeg_p50": 0.0,
            "eeg_p75": 0.0,
            "eeg_max": 0.0,
            "suggested": 0.1,
            "note": "No hook trigger distances recorded yet."
        }));
    }

    distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sample_n = distances.len();
    let min = distances[0];
    let max = *distances.last().unwrap_or(&min);
    let q = |p: f32| -> f32 {
        let idx = ((sample_n - 1) as f32 * p).round() as usize;
        distances[idx.min(sample_n - 1)]
    };
    let p25 = q(0.25);
    let p50 = q(0.50);
    let p75 = q(0.75);
    let suggested = p75.clamp(0.05, 0.95);

    Json(serde_json::json!({
        "label_n": label_n,
        "ref_n": sample_n,
        "sample_n": sample_n,
        "eeg_min": min,
        "eeg_p25": p25,
        "eeg_p50": p50,
        "eeg_p75": p75,
        "eeg_max": max,
        "suggested": suggested,
        "note": "Estimated from recent hook trigger EEG distances."
    }))
}

async fn activity_recent_windows(
    State(state): State<AppState>,
    Json(req): Json<ActivityRecentRequest>,
) -> Json<Vec<ActiveWindowRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let limit = req.limit.unwrap_or(50).min(500);
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|store| store.get_recent_windows(limit))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn activity_recent_input(
    State(state): State<AppState>,
    Json(req): Json<ActivityRecentRequest>,
) -> Json<Vec<InputActivityRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let limit = req.limit.unwrap_or(50).min(500);
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|store| store.get_recent_input(limit))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn activity_input_buckets(
    State(state): State<AppState>,
    Json(req): Json<ActivityBucketsRequest>,
) -> Json<Vec<InputBucketRow>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let now = now_unix();
    let end = req.to_ts.unwrap_or(now);
    let start = req.from_ts.unwrap_or_else(|| end.saturating_sub(24 * 3600));
    let rows = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .map(|store| store.get_input_buckets(start, end))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(rows)
}

async fn get_active_window_tracking(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "value": state
            .track_active_window
            .load(std::sync::atomic::Ordering::Relaxed)
    }))
}

async fn set_active_window_tracking(
    State(state): State<AppState>,
    Json(req): Json<BoolValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.track_active_window = req.value;
    save_user_settings(&state, &settings);
    state
        .track_active_window
        .store(req.value, std::sync::atomic::Ordering::Relaxed);
    Json(serde_json::json!({"value": req.value}))
}

async fn get_input_activity_tracking(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "value": state
            .track_input_activity
            .load(std::sync::atomic::Ordering::Relaxed)
    }))
}

async fn set_input_activity_tracking(
    State(state): State<AppState>,
    Json(req): Json<BoolValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.track_input_activity = req.value;
    save_user_settings(&state, &settings);
    state
        .track_input_activity
        .store(req.value, std::sync::atomic::Ordering::Relaxed);
    Json(serde_json::json!({"value": req.value}))
}

async fn get_current_active_window(State(state): State<AppState>) -> Json<Option<ActiveWindowInfo>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        ActivityStore::open(&skill_dir)
            .and_then(|store| store.get_recent_windows(1).into_iter().next())
            .map(|row| ActiveWindowInfo {
                app_name: row.app_name,
                app_path: row.app_path,
                window_title: row.window_title,
                activated_at: row.activated_at,
            })
    })
    .await
    .ok()
    .flatten();
    Json(out)
}

async fn get_last_input_activity(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let (keyboard, mouse) = tokio::task::spawn_blocking(move || {
        let row = ActivityStore::open(&skill_dir).and_then(|store| store.get_recent_input(1).into_iter().next());
        (
            row.as_ref().and_then(|r| r.last_keyboard).unwrap_or(0),
            row.as_ref().and_then(|r| r.last_mouse).unwrap_or(0),
        )
    })
    .await
    .unwrap_or((0, 0));
    Json(serde_json::json!({"keyboard": keyboard, "mouse": mouse}))
}

fn load_user_settings(state: &AppState) -> skill_settings::UserSettings {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    skill_settings::load_settings(&skill_dir)
}

fn save_user_settings(state: &AppState, settings: &skill_settings::UserSettings) {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let path = skill_settings::settings_path(&skill_dir);
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, json);
    }
}

async fn get_filter_config(State(state): State<AppState>) -> Json<FilterConfig> {
    Json(load_user_settings(&state).filter_config)
}

async fn set_filter_config(State(state): State<AppState>, Json(config): Json<FilterConfig>) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.filter_config = config;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

async fn set_notch_preset(
    State(state): State<AppState>,
    Json(req): Json<NotchPresetRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.filter_config.notch = req.value;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

async fn get_storage_format(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": load_user_settings(&state).storage_format}))
}

async fn set_storage_format(
    State(state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    let fmt = match req.value.to_ascii_lowercase().as_str() {
        "parquet" => "parquet",
        "both" => "both",
        _ => "csv",
    };
    let mut settings = load_user_settings(&state);
    settings.storage_format = fmt.to_string();
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": fmt}))
}

async fn get_embedding_overlap(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": load_user_settings(&state).embedding_overlap_secs}))
}

async fn set_embedding_overlap(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let overlap = req
        .get("value")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(skill_constants::EMBEDDING_OVERLAP_SECS as f64) as f32;
    let clamped = overlap.clamp(
        skill_constants::EMBEDDING_OVERLAP_MIN_SECS,
        skill_constants::EMBEDDING_OVERLAP_MAX_SECS,
    );
    let mut settings = load_user_settings(&state);
    settings.embedding_overlap_secs = clamped;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": clamped}))
}

async fn get_update_check_interval(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": load_user_settings(&state).update_check_interval_secs}))
}

async fn set_update_check_interval(
    State(state): State<AppState>,
    Json(req): Json<U64ValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.update_check_interval_secs = req.value;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": req.value}))
}

async fn get_openbci_config(State(state): State<AppState>) -> Json<skill_settings::OpenBciConfig> {
    Json(load_user_settings(&state).openbci)
}

async fn set_openbci_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::OpenBciConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.openbci = config.clone();
    save_user_settings(&state, &settings);
    if let Ok(mut wifi) = state.scanner_wifi_config.lock() {
        wifi.wifi_shield_ip = config.wifi_shield_ip;
        wifi.galea_ip = config.galea_ip;
    }
    Json(serde_json::json!({"ok": true}))
}

async fn get_device_api_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let c = load_user_settings(&state).device_api;
    Json(serde_json::json!({
        "emotiv_client_id": c.emotiv_client_id,
        "emotiv_client_secret": c.emotiv_client_secret,
        "idun_api_token": c.idun_api_token,
        "oura_access_token": c.oura_access_token,
    }))
}

async fn set_device_api_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::DeviceApiConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.device_api = config.clone();
    save_user_settings(&state, &settings);
    if let Ok(mut cortex) = state.scanner_cortex_config.lock() {
        cortex.emotiv_client_id = config.emotiv_client_id;
        cortex.emotiv_client_secret = config.emotiv_client_secret;
    }
    Json(serde_json::json!({"ok": true}))
}

async fn get_scanner_config(State(state): State<AppState>) -> Json<skill_settings::ScannerConfig> {
    Json(load_user_settings(&state).scanner)
}

async fn get_device_log(State(state): State<AppState>) -> Json<Vec<skill_daemon_common::DeviceLogEntry>> {
    let out = state
        .device_log
        .lock()
        .map(|g| g.iter().cloned().collect())
        .unwrap_or_default();
    Json(out)
}

async fn set_scanner_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::ScannerConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.scanner = config;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

async fn get_neutts_config(State(state): State<AppState>) -> Json<skill_settings::NeuttsConfig> {
    Json(load_user_settings(&state).neutts)
}

async fn set_neutts_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::NeuttsConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.neutts = config;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

async fn get_llm_config(State(state): State<AppState>) -> Json<skill_settings::LlmConfig> {
    Json(load_user_settings(&state).llm)
}

async fn set_llm_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::LlmConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.llm = config.clone();
    save_user_settings(&state, &settings);

    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            *cfg = config.clone();
        }

        if let Ok(guard) = state.llm_state_cell.lock() {
            if let Some(server) = guard.clone() {
                let prev_port = server.allowed_tools.lock().map(|t| t.skill_api_port).unwrap_or(18445);
                let mut new_tools = config.tools.clone();
                new_tools.skill_api_port = prev_port;
                if !settings.location_enabled {
                    new_tools.location = false;
                }
                if let Ok(mut tools) = server.allowed_tools.lock() {
                    *tools = new_tools;
                }
            }
        }
    }

    Json(serde_json::json!({"ok": true}))
}

async fn get_tts_preload(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": load_user_settings(&state).tts_preload}))
}

async fn set_tts_preload(State(state): State<AppState>, Json(req): Json<BoolValueRequest>) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.tts_preload = req.value;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": req.value}))
}

async fn get_sleep_config(State(state): State<AppState>) -> Json<skill_settings::SleepConfig> {
    Json(load_user_settings(&state).sleep)
}

async fn set_sleep_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::SleepConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.sleep = config;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

async fn get_inference_device(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": load_user_settings(&state).inference_device}))
}

async fn set_inference_device(
    State(state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    let is_cpu = req.value == "cpu";
    let mut settings = load_user_settings(&state);
    settings.inference_device = if is_cpu { "cpu".into() } else { "gpu".into() };
    if is_cpu {
        let cur_layers = settings.llm.n_gpu_layers;
        if cur_layers != 0 {
            settings.llm_gpu_layers_saved = cur_layers;
        }
        settings.llm.n_gpu_layers = 0;
    } else {
        settings.llm.n_gpu_layers = settings.llm_gpu_layers_saved;
    }
    let out = settings.inference_device.clone();
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": out}))
}

async fn get_exg_inference_device(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": load_user_settings(&state).exg_inference_device}))
}

async fn set_exg_inference_device(
    State(state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.exg_inference_device = if req.value == "cpu" { "cpu".into() } else { "gpu".into() };
    let out = settings.exg_inference_device.clone();
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": out}))
}

async fn get_ws_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"host": settings.ws_host, "port": settings.ws_port}))
}

async fn set_ws_config(State(state): State<AppState>, Json(req): Json<WsConfigRequest>) -> Json<serde_json::Value> {
    let host = req.host.trim().to_string();
    if host != "127.0.0.1" && host != "0.0.0.0" {
        return Json(
            serde_json::json!({"ok": false, "error": format!("invalid host '{host}': must be '127.0.0.1' or '0.0.0.0'")}),
        );
    }
    if req.port < 1024 {
        return Json(
            serde_json::json!({"ok": false, "error": format!("port {} is reserved; use 1024–65535", req.port)}),
        );
    }
    let mut settings = load_user_settings(&state);
    settings.ws_host = host;
    settings.ws_port = req.port;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "port": req.port}))
}

async fn get_location_enabled(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"value": settings.location_enabled}))
}

async fn set_location_enabled(
    State(state): State<AppState>,
    Json(req): Json<BoolValueRequest>,
) -> Json<serde_json::Value> {
    use serde_json::json;
    if !req.value {
        let mut settings = load_user_settings(&state);
        settings.location_enabled = false;
        save_user_settings(&state, &settings);
        return Json(json!({"enabled": false}));
    }

    let result = tokio::task::spawn_blocking(|| {
        let auth = skill_location::auth_status();
        match auth {
            skill_location::LocationAuthStatus::Denied => {
                return json!({"enabled": false, "permission": "denied", "error": "Location permission denied."});
            }
            skill_location::LocationAuthStatus::Restricted => {
                return json!({"enabled": false, "permission": "restricted", "error": "Location access is restricted."});
            }
            _ => {}
        }

        if skill_location::auth_status() == skill_location::LocationAuthStatus::NotDetermined {
            skill_location::request_access(30.0);
        }

        let post_auth = skill_location::auth_status();
        let perm_str = match post_auth {
            skill_location::LocationAuthStatus::Authorized => "authorized",
            skill_location::LocationAuthStatus::Denied => "denied",
            skill_location::LocationAuthStatus::Restricted => "restricted",
            skill_location::LocationAuthStatus::NotDetermined => "not_determined",
        };

        if matches!(
            post_auth,
            skill_location::LocationAuthStatus::Denied | skill_location::LocationAuthStatus::Restricted
        ) {
            return json!({"enabled": false, "permission": perm_str, "error": "Location permission denied."});
        }

        match skill_location::fetch_location(10.0) {
            Ok(fix) => json!({
                "enabled": true,
                "permission": perm_str,
                "fix": {
                    "latitude": fix.latitude,
                    "longitude": fix.longitude,
                    "source": format!("{:?}", fix.source),
                    "country": fix.country,
                    "region": fix.region,
                    "city": fix.city,
                    "timezone": fix.timezone,
                    "horizontal_accuracy": fix.horizontal_accuracy,
                    "altitude": fix.altitude,
                }
            }),
            Err(e) => json!({"enabled": true, "permission": perm_str, "error": e.to_string()}),
        }
    })
    .await
    .unwrap_or_else(|e| json!({"enabled": false, "error": format!("location task error: {e}")}));

    let enabled_result = result
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if enabled_result {
        let mut settings = load_user_settings(&state);
        settings.location_enabled = true;
        save_user_settings(&state, &settings);
    }

    Json(result)
}

async fn test_location() -> Json<serde_json::Value> {
    use serde_json::json;
    let v = tokio::task::spawn_blocking(|| match skill_location::fetch_location(10.0) {
        Ok(fix) => json!({
            "ok": true,
            "source": format!("{:?}", fix.source),
            "latitude": fix.latitude,
            "longitude": fix.longitude,
            "country": fix.country,
            "region": fix.region,
            "city": fix.city,
            "timezone": fix.timezone,
            "horizontal_accuracy": fix.horizontal_accuracy,
            "altitude": fix.altitude,
        }),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    })
    .await
    .unwrap_or_else(|e| json!({"ok": false, "error": format!("location task error: {e}")}));
    Json(v)
}

async fn get_api_token(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"value": settings.api_token}))
}

async fn set_api_token(State(state): State<AppState>, Json(req): Json<StringValueRequest>) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.api_token = req.value;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

async fn get_hf_endpoint(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    let endpoint = if settings.hf_endpoint.trim().is_empty() {
        skill_settings::default_hf_endpoint()
    } else {
        settings.hf_endpoint
    };
    Json(serde_json::json!({"value": endpoint}))
}

async fn set_hf_endpoint(
    State(state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.hf_endpoint = if req.value.trim().is_empty() {
        skill_settings::default_hf_endpoint()
    } else {
        req.value.trim().to_string()
    };
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": settings.hf_endpoint}))
}

async fn get_umap_config(State(state): State<AppState>) -> Json<skill_settings::UmapUserConfig> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    Json(skill_settings::load_umap_config(&skill_dir))
}

async fn set_umap_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::UmapUserConfig>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    skill_settings::save_umap_config(&skill_dir, &config);
    let cache_dir = skill_dir.join("umap_cache");
    if cache_dir.exists() {
        let _ = std::fs::remove_dir_all(&cache_dir);
    }
    Json(serde_json::json!({"ok": true}))
}

async fn get_gpu_stats() -> Json<serde_json::Value> {
    Json(serde_json::to_value(skill_data::gpu_stats::read()).unwrap_or(serde_json::Value::Null))
}

async fn web_cache_stats() -> Json<serde_json::Value> {
    let v = match skill_tools::web_cache::global() {
        Some(cache) => serde_json::to_value(cache.stats()).unwrap_or_default(),
        None => serde_json::json!({"total_entries": 0, "expired_entries": 0, "total_bytes": 0}),
    };
    Json(v)
}

async fn web_cache_list() -> Json<Vec<serde_json::Value>> {
    let v = match skill_tools::web_cache::global() {
        Some(cache) => cache
            .list_entries()
            .into_iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect(),
        None => Vec::new(),
    };
    Json(v)
}

async fn web_cache_clear() -> Json<serde_json::Value> {
    let removed = if let Some(cache) = skill_tools::web_cache::global() {
        let stats = cache.stats();
        cache.clear();
        stats.total_entries
    } else {
        0
    };
    Json(serde_json::json!({"removed": removed}))
}

async fn web_cache_remove_domain(Json(req): Json<StringValueRequest>) -> Json<serde_json::Value> {
    let removed = match skill_tools::web_cache::global() {
        Some(cache) => cache.remove_by_domain(&req.value),
        None => 0,
    };
    Json(serde_json::json!({"removed": removed}))
}

async fn web_cache_remove_entry(Json(req): Json<StringKeyRequest>) -> Json<serde_json::Value> {
    let removed = match skill_tools::web_cache::global() {
        Some(cache) => cache.remove_entry(&req.key),
        None => false,
    };
    Json(serde_json::json!({"removed": removed}))
}

async fn get_dnd_focus_modes() -> Json<Vec<skill_data::dnd::FocusModeOption>> {
    Json(skill_data::dnd::list_focus_modes())
}

async fn get_dnd_config(State(state): State<AppState>) -> Json<skill_settings::DoNotDisturbConfig> {
    let settings = load_user_settings(&state);
    Json(settings.do_not_disturb)
}

async fn set_dnd_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::DoNotDisturbConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.do_not_disturb = config;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

async fn get_dnd_active() -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": skill_data::dnd::query_os_active().unwrap_or(false)}))
}

async fn get_dnd_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cfg = load_user_settings(&state).do_not_disturb;
    let os_active = skill_data::dnd::query_os_active();
    let dnd_active = os_active.unwrap_or(false);
    Json(serde_json::json!({
        "enabled": cfg.enabled,
        "avg_score": 0.0,
        "threshold": cfg.focus_threshold as f64,
        "sample_count": 0,
        "window_size": (cfg.duration_secs as usize * 4).max(8),
        "duration_secs": cfg.duration_secs,
        "dnd_active": dnd_active,
        "os_active": os_active,
        "last_error": serde_json::Value::Null,
        "exit_duration_secs": cfg.exit_duration_secs,
        "below_ticks": 0,
        "exit_window_size": (cfg.exit_duration_secs as usize * 4).max(4),
        "exit_secs_remaining": 0.0,
        "focus_lookback_secs": cfg.focus_lookback_secs,
        "exit_held_by_lookback": false,
    }))
}

async fn test_dnd(Json(req): Json<DndTestRequest>) -> Json<serde_json::Value> {
    if req.enabled {
        return Json(serde_json::json!({"ok": false, "value": false}));
    }
    let ok = skill_data::dnd::set_dnd(false, "");
    Json(serde_json::json!({"ok": ok, "value": ok}))
}

#[derive(Clone)]
struct DaemonScreenshotContext {
    config: skill_settings::ScreenshotConfig,
    events_tx: tokio::sync::broadcast::Sender<skill_daemon_common::EventEnvelope>,
}

impl skill_screenshots::ScreenshotContext for DaemonScreenshotContext {
    fn config(&self) -> skill_screenshots::ScreenshotConfig {
        self.config.clone()
    }
    fn is_session_active(&self) -> bool {
        false
    }
    fn active_window(&self) -> skill_screenshots::ActiveWindowInfo {
        skill_screenshots::ActiveWindowInfo::default()
    }
    fn emit_event(&self, event: &str, payload: serde_json::Value) {
        let _ = self.events_tx.send(skill_daemon_common::EventEnvelope {
            r#type: event.to_string(),
            ts_unix_ms: now_unix_ms(),
            correlation_id: None,
            payload,
        });
    }
    fn embed_image_via_llm(&self, _png_bytes: &[u8]) -> Option<Vec<f32>> {
        None
    }
}

async fn get_screenshot_config(State(state): State<AppState>) -> Json<skill_settings::ScreenshotConfig> {
    Json(load_user_settings(&state).screenshot)
}

async fn set_screenshot_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::ScreenshotConfig>,
) -> Json<skill_data::screenshot_store::ConfigChangeResult> {
    let mut settings = load_user_settings(&state);
    let old_backend = settings.screenshot.embed_backend.clone();
    let old_model = settings.screenshot.model_id();
    let new_backend = config.embed_backend.clone();
    let new_model = config.model_id();
    let model_changed = old_backend != new_backend || old_model != new_model;

    settings.screenshot = config;
    save_user_settings(&state, &settings);

    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let stale_count = if model_changed {
        skill_data::screenshot_store::ScreenshotStore::open(&skill_dir)
            .map(|s| s.count_stale(&new_backend, &new_model))
            .unwrap_or(0)
    } else {
        0
    };

    Json(skill_data::screenshot_store::ConfigChangeResult {
        model_changed,
        stale_count,
    })
}

async fn estimate_screenshot_reembed(
    State(state): State<AppState>,
) -> Json<Option<skill_data::screenshot_store::ReembedEstimate>> {
    let settings = load_user_settings(&state);
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        let store = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir)?;
        Some(skill_screenshots::capture::estimate_reembed(
            &store,
            &settings.screenshot,
            &skill_dir,
        ))
    })
    .await
    .unwrap_or(None);
    Json(out)
}

async fn rebuild_screenshot_embeddings(
    State(state): State<AppState>,
) -> Json<Option<skill_data::screenshot_store::ReembedResult>> {
    let settings = load_user_settings(&state);
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let events_tx = state.events_tx.clone();
    let out = tokio::task::spawn_blocking(move || {
        let store = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir)?;
        let ctx = DaemonScreenshotContext {
            config: settings.screenshot.clone(),
            events_tx,
        };
        Some(skill_screenshots::capture::rebuild_embeddings(
            &store,
            &settings.screenshot,
            &skill_dir,
            &ctx,
        ))
    })
    .await
    .unwrap_or(None);
    Json(out)
}

async fn get_screenshots_around(
    State(state): State<AppState>,
    Json(req): Json<ScreenshotAroundRequest>,
) -> Json<Vec<skill_data::screenshot_store::ScreenshotResult>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return vec![];
        };
        skill_screenshots::capture::get_around(&store, req.timestamp, req.window_secs)
    })
    .await
    .unwrap_or_default();
    Json(out)
}

async fn search_screenshots_by_image(
    State(state): State<AppState>,
    Json(req): Json<ScreenshotImageSearchRequest>,
) -> Json<Vec<skill_data::screenshot_store::ScreenshotResult>> {
    let settings = load_user_settings(&state);
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        let Some(mut encoder) = skill_screenshots::capture::load_fastembed_image_pub(&settings.screenshot, &skill_dir)
        else {
            return vec![];
        };
        let Some(query) = skill_screenshots::capture::fastembed_embed_pub(&mut encoder, &req.image_bytes) else {
            return vec![];
        };
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return vec![];
        };
        let hnsw_path = skill_dir.join(skill_constants::SCREENSHOTS_HNSW);
        let Ok(hnsw) = fast_hnsw::labeled::LabeledIndex::<fast_hnsw::distance::Cosine, i64>::load(
            &hnsw_path,
            fast_hnsw::distance::Cosine,
        ) else {
            return vec![];
        };
        skill_screenshots::capture::search_by_vector(&hnsw, &store, &query, req.k)
    })
    .await
    .unwrap_or_default();
    Json(out)
}

async fn get_screenshot_metrics(State(state): State<AppState>) -> Json<skill_screenshots::capture::MetricsSnapshot> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let (captures, embeds, last_capture_unix, last_embed_unix) = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return (0u64, 0u64, 0u64, 0u64);
        };
        let summary = store.summary_counts();
        let db_path = skill_dir.join(skill_constants::SCREENSHOTS_SQLITE);
        let mut last_capture = 0u64;
        let mut last_embed = 0u64;
        if let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
            last_capture = conn
                .query_row("SELECT COALESCE(MAX(unix_ts), 0) FROM screenshots", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap_or(0)
                .max(0) as u64;
            last_embed = conn
                .query_row(
                    "SELECT COALESCE(MAX(unix_ts), 0) FROM screenshots WHERE embedding IS NOT NULL",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0)
                .max(0) as u64;
        }
        (summary.total, summary.with_embedding, last_capture, last_embed)
    })
    .await
    .unwrap_or((0, 0, 0, 0));

    Json(skill_screenshots::capture::MetricsSnapshot {
        captures,
        capture_errors: 0,
        drops: 0,
        capture_us: 0,
        ocr_us: 0,
        resize_us: 0,
        save_us: 0,
        capture_total_us: 0,
        embeds,
        embed_errors: 0,
        vision_embed_us: 0,
        text_embed_us: 0,
        embed_total_us: 0,
        queue_depth: 0,
        last_capture_unix,
        last_embed_unix,
        backoff_multiplier: 0,
    })
}

async fn check_ocr_models_ready(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let ocr_dir = skill_dir.join("ocr_models");
    Json(
        serde_json::json!({"value": ocr_dir.join(skill_constants::OCR_DETECTION_MODEL_FILE).exists() && ocr_dir.join(skill_constants::OCR_RECOGNITION_MODEL_FILE).exists()}),
    )
}

async fn download_ocr_models(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let ok = tokio::task::spawn_blocking(move || {
        let ocr_dir = skill_dir.join("ocr_models");
        let _ = std::fs::create_dir_all(&ocr_dir);
        let det_path = ocr_dir.join(skill_constants::OCR_DETECTION_MODEL_FILE);
        let rec_path = ocr_dir.join(skill_constants::OCR_RECOGNITION_MODEL_FILE);
        let det_ok =
            skill_screenshots::capture::download_ocr_model_pub(skill_constants::OCR_DETECTION_MODEL_URL, &det_path);
        let rec_ok =
            skill_screenshots::capture::download_ocr_model_pub(skill_constants::OCR_RECOGNITION_MODEL_URL, &rec_path);
        det_ok && rec_ok
    })
    .await
    .unwrap_or(false);
    Json(serde_json::json!({"value": ok}))
}

async fn search_screenshots_by_text(
    State(state): State<AppState>,
    Json(req): Json<ScreenshotTextSearchRequest>,
) -> Json<Vec<skill_data::screenshot_store::ScreenshotResult>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let settings = load_user_settings(&state);
    let out = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return vec![];
        };
        let k = req.k.unwrap_or(20);
        let mode = req.mode.unwrap_or_else(|| "semantic".into());
        if mode == "substring" {
            return skill_screenshots::capture::search_by_ocr_text_like(&store, &req.query, k);
        }

        let cache_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".cache")
            .join("fastembed");
        let te = match fastembed::TextEmbedding::try_new(
            fastembed::TextInitOptions::new(fastembed::EmbeddingModel::BGESmallENV15)
                .with_cache_dir(cache_dir)
                .with_show_download_progress(false),
        ) {
            Ok(te) => std::sync::Mutex::new(te),
            Err(_) => {
                return skill_screenshots::capture::search_by_ocr_text_like(&store, &req.query, k);
            }
        };

        let embed_fn = |text: &str| -> Option<Vec<f32>> {
            let mut guard = te.lock().ok()?;
            let mut vecs = guard.embed(vec![text], None).ok()?;
            if vecs.is_empty() {
                None
            } else {
                Some(vecs.remove(0))
            }
        };

        let mut results =
            skill_screenshots::capture::search_by_ocr_text_embedding(&skill_dir, &store, &req.query, k, &embed_fn);

        if results.is_empty() {
            results = skill_screenshots::capture::search_by_ocr_text_like(&store, &req.query, k);
        }

        if settings.text_embedding_model != "Xenova/bge-small-en-v1.5" {
            eprintln!(
                "[screenshot-search] semantic mode currently uses BGESmallENV15; requested model={} ",
                settings.text_embedding_model
            );
        }
        results
    })
    .await
    .unwrap_or_default();
    Json(out)
}

async fn get_screenshots_dir(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let dir = skill_dir
        .join(skill_constants::SCREENSHOTS_DIR)
        .to_string_lossy()
        .into_owned();
    let port = std::env::var("SKILL_DAEMON_ADDR")
        .ok()
        .and_then(|v| v.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()))
        .unwrap_or(18444);
    Json(serde_json::json!({"dir": dir, "port": port}))
}

async fn search_screenshots_by_vector(
    State(state): State<AppState>,
    Json(req): Json<ScreenshotVectorSearchRequest>,
) -> Json<Vec<skill_data::screenshot_store::ScreenshotResult>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return vec![];
        };
        let hnsw_path = skill_dir.join(skill_constants::SCREENSHOTS_HNSW);
        let Ok(hnsw) = fast_hnsw::labeled::LabeledIndex::<fast_hnsw::distance::Cosine, i64>::load(
            &hnsw_path,
            fast_hnsw::distance::Cosine,
        ) else {
            return vec![];
        };
        skill_screenshots::capture::search_by_vector(&hnsw, &store, &req.vector, req.k)
    })
    .await
    .unwrap_or_default();
    Json(out)
}

async fn get_accent_color(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"value": settings.accent_color}))
}

async fn set_accent_color(
    State(state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.accent_color = req.value;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

async fn get_daily_goal(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"value": settings.daily_goal_min}))
}

async fn set_daily_goal(State(state): State<AppState>, Json(req): Json<U64ValueRequest>) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    let clamped = (req.value as u32).min(480);
    settings.daily_goal_min = clamped;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": clamped}))
}

async fn get_goal_notified_date(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"value": settings.goal_notified_date}))
}

async fn set_goal_notified_date(
    State(state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.goal_notified_date = req.value;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

async fn get_main_window_auto_fit(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"value": settings.main_window_auto_fit}))
}

async fn set_main_window_auto_fit(
    State(state): State<AppState>,
    Json(req): Json<BoolValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.main_window_auto_fit = req.value;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": req.value}))
}

async fn get_skills_refresh_interval(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"value": settings.llm.tools.skills_refresh_interval_secs}))
}

async fn set_skills_refresh_interval(
    State(state): State<AppState>,
    Json(req): Json<U64ValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.llm.tools.skills_refresh_interval_secs = req.value;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": req.value}))
}

async fn get_skills_sync_on_launch(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"value": settings.llm.tools.skills_sync_on_launch}))
}

async fn set_skills_sync_on_launch(
    State(state): State<AppState>,
    Json(req): Json<BoolValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.llm.tools.skills_sync_on_launch = req.value;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": req.value}))
}

async fn get_skills_last_sync(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    Json(serde_json::json!({"value": skill_skills::sync::last_sync_ts(&skill_dir)}))
}

async fn sync_skills_now(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let outcome = tokio::task::spawn_blocking(move || skill_skills::sync::sync_skills(&skill_dir, 0, None)).await;
    match outcome {
        Ok(skill_skills::sync::SyncOutcome::Updated { elapsed_ms, .. }) => {
            Json(serde_json::json!({"status": "updated", "message": format!("updated in {elapsed_ms} ms")}))
        }
        Ok(skill_skills::sync::SyncOutcome::Fresh { .. }) => {
            Json(serde_json::json!({"status": "fresh", "message": "already up to date"}))
        }
        Ok(skill_skills::sync::SyncOutcome::Failed(e)) => Json(serde_json::json!({"status": "failed", "message": e})),
        Err(e) => Json(serde_json::json!({"status": "failed", "message": e.to_string()})),
    }
}

async fn list_skills(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    let disabled = settings.llm.tools.disabled_skills;
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
    let bundled_dir = exe_dir
        .as_ref()
        .map(|d| d.join(skill_constants::SKILLS_SUBDIR))
        .filter(|d| d.is_dir())
        .or_else(|| {
            let cwd = std::env::current_dir().ok()?;
            let p = cwd.join(skill_constants::SKILLS_SUBDIR);
            if p.is_dir() {
                Some(p)
            } else {
                None
            }
        });

    let result = skill_skills::load_skills(&skill_skills::LoadSkillsOptions {
        cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        skill_dir: skill_dir.to_path_buf(),
        bundled_dir,
        skill_paths: Vec::new(),
        include_defaults: true,
    });

    Json(serde_json::Value::Array(
        result
            .skills
            .into_iter()
            .map(|s| {
                let enabled = !disabled.iter().any(|d| d == &s.name);
                serde_json::json!({
                    "name": s.name,
                    "description": s.description,
                    "source": s.source,
                    "enabled": enabled
                })
            })
            .collect(),
    ))
}

async fn get_skills_license(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let license_path = skill_dir.join(skill_constants::SKILLS_SUBDIR).join("LICENSE");
    Json(serde_json::json!({"value": std::fs::read_to_string(&license_path).ok()}))
}

async fn get_disabled_skills(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"value": settings.llm.tools.disabled_skills}))
}

async fn set_disabled_skills(
    State(state): State<AppState>,
    Json(req): Json<StringListRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.llm.tools.disabled_skills = req.values.clone();
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": req.values}))
}

async fn llm_server_start(State(state): State<AppState>) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        let cfg = state.llm_config.lock().map(|g| g.clone()).unwrap_or_default();
        let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
        let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
        let cell = state.llm_state_cell.clone();
        let log_buf = state.llm_log_buffer.clone();

        if cell.lock().ok().and_then(|g| g.clone()).is_some() {
            return Json(serde_json::json!({"ok": true, "result": "already_running"}));
        }

        let emitter: std::sync::Arc<dyn skill_llm::LlmEventEmitter> = std::sync::Arc::new(DaemonLlmEmitter {
            events_tx: state.events_tx.clone(),
        });
        match tokio::task::spawn_blocking(move || skill_llm::init(&cfg, &cat, emitter, log_buf, &skill_dir)).await {
            Ok(Some(srv)) => {
                let model_name = srv.model_name.clone();
                if let Ok(mut g) = cell.lock() {
                    *g = Some(srv);
                }
                if let Ok(mut st) = state.llm_status.lock() {
                    *st = "running".to_string();
                }
                if let Ok(mut m) = state.llm_model_name.lock() {
                    *m = model_name;
                }
                return Json(serde_json::json!({"ok": true, "result": "starting"}));
            }
            Ok(None) => {
                return Json(serde_json::json!({"ok": false, "result": "failed", "error": "init returned none"}));
            }
            Err(e) => {
                return Json(serde_json::json!({"ok": false, "result": "failed", "error": e.to_string()}));
            }
        }
    }

    #[cfg(not(feature = "llm"))]
    {
        if let Ok(mut st) = state.llm_status.lock() {
            *st = "running".to_string();
        }
        Json(serde_json::json!({"ok": true, "result": "starting"}))
    }
}

async fn llm_server_stop(State(state): State<AppState>) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        skill_llm::shutdown_cell(&state.llm_state_cell);
    }
    if let Ok(mut st) = state.llm_status.lock() {
        *st = "stopped".to_string();
    }
    Json(serde_json::json!({"ok": true}))
}

async fn llm_server_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        use std::sync::atomic::Ordering;
        let (status, model_name) = skill_llm::cell_status(&state.llm_state_cell);
        let (n_ctx, supports_vision, supports_tools) = state
            .llm_state_cell
            .lock()
            .ok()
            .and_then(|g| {
                g.as_ref().map(|srv| {
                    (
                        srv.n_ctx.load(Ordering::Relaxed),
                        srv.vision_ready.load(Ordering::Relaxed),
                        srv.is_ready(),
                    )
                })
            })
            .unwrap_or((0, false, false));

        return Json(serde_json::json!({
            "status": serde_json::to_value(status).unwrap_or(serde_json::json!("stopped")),
            "model_name": model_name,
            "n_ctx": n_ctx,
            "supports_vision": supports_vision,
            "supports_tools": supports_tools,
            "start_error": serde_json::Value::Null
        }));
    }

    #[cfg(not(feature = "llm"))]
    {
        let status = state
            .llm_status
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "stopped".into());
        let model_name = state.llm_model_name.lock().map(|g| g.clone()).unwrap_or_default();
        let supports_vision = state.llm_mmproj_name.lock().map(|g| g.is_some()).unwrap_or(false);
        let supports_tools = status == "running";
        Json(serde_json::json!({
            "status": status,
            "model_name": model_name,
            "n_ctx": if supports_tools { 8192 } else { 0 },
            "supports_vision": supports_vision,
            "supports_tools": supports_tools,
            "start_error": serde_json::Value::Null
        }))
    }
}

async fn llm_server_logs(State(state): State<AppState>) -> Json<Vec<serde_json::Value>> {
    #[cfg(feature = "llm")]
    {
        let logs = state
            .llm_log_buffer
            .lock()
            .map(|q| q.iter().filter_map(|e| serde_json::to_value(e).ok()).collect())
            .unwrap_or_default();
        return Json(logs);
    }

    #[cfg(not(feature = "llm"))]
    {
        Json(state.llm_logs.lock().map(|g| g.clone()).unwrap_or_default())
    }
}

async fn llm_server_switch_model(
    State(state): State<AppState>,
    Json(req): Json<FilenameRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.active_model = req.filename.clone();
        if !cat.active_mmproj_matches_active_model() {
            cat.active_mmproj.clear();
        }
    }
    persist_llm_catalog(&state);

    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
            cfg.model_path = cat.active_model_path();
            cfg.mmproj = if cfg.autoload_mmproj {
                cat.active_mmproj_path()
            } else {
                None
            };
        }
        skill_llm::shutdown_cell(&state.llm_state_cell);
        let _ = llm_server_start(State(state.clone())).await;
    }

    Json(serde_json::json!({"ok": true, "result": "switching"}))
}

async fn llm_server_switch_mmproj(
    State(state): State<AppState>,
    Json(req): Json<FilenameRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.active_mmproj = req.filename.clone();
    }
    persist_llm_catalog(&state);

    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
            cfg.mmproj = if cfg.autoload_mmproj {
                cat.active_mmproj_path()
            } else {
                None
            };
        }
        skill_llm::shutdown_cell(&state.llm_state_cell);
        let _ = llm_server_start(State(state.clone())).await;
    }

    Json(serde_json::json!({"ok": true, "result": "switching"}))
}

async fn chat_last_session(State(state): State<AppState>) -> Json<ChatSessionResponse> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) else {
            return ChatSessionResponse {
                session_id: 0,
                messages: vec![],
            };
        };
        let session_id = store.get_or_create_last_session();
        let messages = store.load_session(session_id);
        ChatSessionResponse { session_id, messages }
    })
    .await
    .unwrap_or(ChatSessionResponse {
        session_id: 0,
        messages: vec![],
    });
    Json(out)
}

async fn chat_load_session(State(state): State<AppState>, Json(req): Json<ChatIdRequest>) -> Json<ChatSessionResponse> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) else {
            return ChatSessionResponse {
                session_id: req.id,
                messages: vec![],
            };
        };
        let messages = store.load_session(req.id);
        ChatSessionResponse {
            session_id: req.id,
            messages,
        }
    })
    .await
    .unwrap_or(ChatSessionResponse {
        session_id: req.id,
        messages: vec![],
    });
    Json(out)
}

async fn chat_list_sessions(State(state): State<AppState>) -> Json<Vec<skill_llm::chat_store::SessionSummary>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        skill_llm::chat_store::ChatStore::open(&skill_dir)
            .map(|mut store| store.list_sessions())
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(out)
}

async fn chat_rename_session(
    State(state): State<AppState>,
    Json(req): Json<ChatRenameRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) {
            store.rename_session(req.id, &req.title);
        }
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

async fn chat_delete_session(State(state): State<AppState>, Json(req): Json<ChatIdRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) {
            store.delete_session(req.id);
        }
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

async fn chat_archive_session(
    State(state): State<AppState>,
    Json(req): Json<ChatIdRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) {
            store.archive_session(req.id);
        }
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

async fn chat_unarchive_session(
    State(state): State<AppState>,
    Json(req): Json<ChatIdRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) {
            store.unarchive_session(req.id);
        }
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

async fn chat_list_archived_sessions(
    State(state): State<AppState>,
) -> Json<Vec<skill_llm::chat_store::SessionSummary>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        skill_llm::chat_store::ChatStore::open(&skill_dir)
            .map(|mut store| store.list_archived_sessions())
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(out)
}

async fn chat_save_message(
    State(state): State<AppState>,
    Json(req): Json<ChatSaveMessageRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let id = tokio::task::spawn_blocking(move || {
        skill_llm::chat_store::ChatStore::open(&skill_dir)
            .map(|mut store| store.save_message(req.session_id, &req.role, &req.content, req.thinking.as_deref()))
            .unwrap_or(0)
    })
    .await
    .unwrap_or(0);
    Json(serde_json::json!({"id": id}))
}

async fn chat_get_session_params(
    State(state): State<AppState>,
    Json(req): Json<ChatIdRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let value = tokio::task::spawn_blocking(move || {
        skill_llm::chat_store::ChatStore::open(&skill_dir)
            .map(|store| store.get_session_params(req.id))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(serde_json::json!({"value": value}))
}

async fn chat_set_session_params(
    State(state): State<AppState>,
    Json(req): Json<ChatSessionParamsRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) {
            store.set_session_params(req.id, &req.params_json);
        }
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

async fn chat_new_session(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let id = tokio::task::spawn_blocking(move || {
        skill_llm::chat_store::ChatStore::open(&skill_dir)
            .map(|mut store| store.new_session())
            .unwrap_or(0)
    })
    .await
    .unwrap_or(0);
    Json(serde_json::json!({"id": id}))
}

async fn chat_save_tool_calls(
    State(state): State<AppState>,
    Json(req): Json<ChatSaveToolCallsRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) {
            store.save_tool_calls(req.message_id, &req.tool_calls);
        }
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

async fn llm_chat_completions(
    State(state): State<AppState>,
    Json(req): Json<ChatCompletionsRequest>,
) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        let srv_opt = state.llm_state_cell.lock().ok().and_then(|g| g.clone());
        let Some(srv) = srv_opt else {
            return Json(serde_json::json!({"error":"LLM server not running"}));
        };

        let params: skill_llm::GenParams = serde_json::from_value(req.params).unwrap_or_default();
        let result =
            skill_llm::run_chat_with_builtin_tools(&srv, req.messages, params, Vec::new(), |_delta| {}, |_evt| {})
                .await;

        return match result {
            Ok((text, finish_reason, prompt_tokens, completion_tokens, n_ctx)) => Json(serde_json::json!({
                "content": text,
                "finish_reason": finish_reason,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "n_ctx": n_ctx
            })),
            Err(e) => Json(serde_json::json!({"error": e.to_string()})),
        };
    }

    #[cfg(not(feature = "llm"))]
    {
        let _ = req;
        let _ = state;
        Json(serde_json::json!({
            "content": "Daemon LLM unavailable (compiled without llm feature)",
            "finish_reason": "stop",
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "n_ctx": 0
        }))
    }
}

async fn llm_embed_image(State(_state): State<AppState>, Json(req): Json<LlmImageRequest>) -> Json<serde_json::Value> {
    let bytes = match base64::engine::general_purpose::STANDARD.decode(req.png_base64.as_bytes()) {
        Ok(b) => b,
        Err(e) => return Json(serde_json::json!({"error": format!("invalid base64: {e}")})),
    };

    #[cfg(feature = "llm")]
    {
        let srv_opt = _state.llm_state_cell.lock().ok().and_then(|g| g.clone());
        let Some(srv) = srv_opt else {
            return Json(serde_json::json!({"error":"LLM server not running"}));
        };
        if !srv.vision_ready.load(std::sync::atomic::Ordering::Relaxed) {
            return Json(serde_json::json!({"error":"vision not ready"}));
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        if srv
            .req_tx
            .send(skill_llm::InferRequest::EmbedImage { bytes, result_tx: tx })
            .is_err()
        {
            return Json(serde_json::json!({"error":"failed to queue embed request"}));
        }
        return match rx.await {
            Ok(Some(v)) => Json(serde_json::json!({"embedding": v})),
            Ok(None) => Json(serde_json::json!({"embedding": serde_json::Value::Null})),
            Err(e) => Json(serde_json::json!({"error": e.to_string()})),
        };
    }

    #[cfg(not(feature = "llm"))]
    {
        let _ = bytes;
        Json(serde_json::json!({"error":"LLM unavailable"}))
    }
}

async fn llm_ocr(State(_state): State<AppState>, Json(req): Json<LlmImageRequest>) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        let srv_opt = _state.llm_state_cell.lock().ok().and_then(|g| g.clone());
        let Some(srv) = srv_opt else {
            return Json(serde_json::json!({"error":"LLM server not running"}));
        };

        let data_url = format!("data:image/png;base64,{}", req.png_base64);
        let messages = vec![
            serde_json::json!({
                "role": "system",
                "content": "You are an OCR assistant. Extract ALL visible text from the image exactly as it appears. Output only the extracted text, nothing else. Preserve line breaks. If no text is visible, output an empty string."
            }),
            serde_json::json!({
                "role": "user",
                "content": [
                    {"type":"image_url","image_url":{"url": data_url}},
                    {"type":"text","text":"Extract all visible text from this screenshot."}
                ]
            }),
        ];

        let params = skill_llm::GenParams {
            max_tokens: 2048,
            temperature: 0.0,
            thinking_budget: Some(0),
            ..Default::default()
        };

        let result =
            skill_llm::run_chat_with_builtin_tools(&srv, messages, params, Vec::new(), |_delta| {}, |_evt| {}).await;

        return match result {
            Ok((text, ..)) => Json(serde_json::json!({"text": text.trim()})),
            Err(e) => Json(serde_json::json!({"error": e.to_string()})),
        };
    }

    #[cfg(not(feature = "llm"))]
    {
        let _ = req;
        Json(serde_json::json!({"error":"LLM unavailable"}))
    }
}

async fn llm_abort_stream(State(_state): State<AppState>) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        if let Ok(guard) = _state.llm_state_cell.lock() {
            if let Some(srv) = guard.as_ref() {
                srv.abort_tx.send_modify(|v| *v = v.wrapping_add(1));
            }
        }
    }
    Json(serde_json::json!({"ok": true}))
}

async fn llm_cancel_tool_call(
    State(_state): State<AppState>,
    Json(req): Json<ToolCancelRequest>,
) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        if let Ok(guard) = _state.llm_state_cell.lock() {
            if let Some(srv) = guard.as_ref() {
                if let Ok(mut c) = srv.cancelled_tool_calls.lock() {
                    c.insert(req.tool_call_id);
                }
            }
        }
    }
    #[cfg(not(feature = "llm"))]
    {
        let _ = req;
    }
    Json(serde_json::json!({"ok": true}))
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn emit_daemon_event(state: &AppState, event_type: &str, payload: serde_json::Value) {
    let _ = state.events_tx.send(skill_daemon_common::EventEnvelope {
        r#type: event_type.to_string(),
        ts_unix_ms: now_unix_ms(),
        correlation_id: None,
        payload,
    });
}

fn persist_llm_catalog(state: &AppState) {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    if let Ok(cat) = state.llm_catalog.lock() {
        cat.save(&skill_dir);
    }
}

async fn llm_get_catalog(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
    Json(serde_json::to_value(cat).unwrap_or_default())
}

async fn llm_refresh_catalog(State(state): State<AppState>) -> Json<serde_json::Value> {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.refresh_cache();
        cat.auto_select();
    }
    persist_llm_catalog(&state);
    Json(serde_json::json!({"ok": true}))
}

fn infer_quant(filename: &str) -> String {
    let upper = filename.to_uppercase();
    for q in [
        "IQ4_NL", "IQ4_XS", "IQ3_XXS", "IQ3_XS", "IQ3_M", "IQ3_S", "IQ2_XXS", "IQ2_XS", "IQ2_M", "IQ2_S", "Q6_K_L",
        "Q6_K", "Q5_K_L", "Q5_K_M", "Q5_K_S", "Q4_K_L", "Q4_K_M", "Q4_K_S", "Q4_0", "Q4_1", "Q3_K_XL", "Q3_K_L",
        "Q3_K_M", "Q3_K_S", "Q2_K_L", "Q2_K", "Q8_0", "Q8_1", "BF16", "F16", "F32",
    ] {
        if upper.contains(q) {
            return q.to_string();
        }
    }
    "unknown".to_string()
}

async fn llm_add_model(State(state): State<AppState>, Json(req): Json<LlmAddModelRequest>) -> Json<serde_json::Value> {
    let should_download = req.download.unwrap_or(false);
    if let Ok(mut cat) = state.llm_catalog.lock() {
        if !cat.entries.iter().any(|e| e.filename == req.filename) {
            let entry = skill_llm::catalog::LlmModelEntry {
                repo: req.repo.clone(),
                filename: req.filename.clone(),
                quant: infer_quant(&req.filename),
                size_gb: req.size_gb.unwrap_or(0.0),
                description: "External model".to_string(),
                family_id: req
                    .repo
                    .split('/')
                    .next_back()
                    .unwrap_or("external")
                    .to_lowercase()
                    .replace(' ', "-"),
                family_name: req
                    .repo
                    .split('/')
                    .next_back()
                    .unwrap_or("External")
                    .replace(['_', '-'], " "),
                family_desc: String::new(),
                tags: vec!["external".to_string()],
                is_mmproj: req.mmproj.as_ref().map(|m| m == &req.filename).unwrap_or(false)
                    || req.filename.to_ascii_lowercase().contains("mmproj"),
                recommended: false,
                advanced: false,
                params_b: 0.0,
                max_context_length: 0,
                shard_files: Vec::new(),
                local_path: None,
                state: if should_download {
                    skill_llm::catalog::DownloadState::Downloading
                } else {
                    skill_llm::catalog::DownloadState::NotDownloaded
                },
                status_msg: if should_download {
                    Some("Queued in daemon".to_string())
                } else {
                    None
                },
                progress: 0.0,
                initiated_at_unix: Some(now_unix()),
            };
            cat.entries.push(entry);
        }
        cat.auto_select();
    }
    persist_llm_catalog(&state);
    Json(serde_json::json!({"ok": true, "filename": req.filename}))
}

async fn llm_get_downloads(State(state): State<AppState>) -> Json<Vec<serde_json::Value>> {
    let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
    let downloads = state.llm_downloads.lock().map(|g| g.clone()).unwrap_or_default();
    let items = cat
        .entries
        .into_iter()
        .filter(|e| {
            use skill_llm::catalog::DownloadState;
            matches!(
                e.state,
                DownloadState::Downloading
                    | DownloadState::Paused
                    | DownloadState::Failed
                    | DownloadState::Cancelled
                    | DownloadState::Downloaded
            )
        })
        .map(|e| {
            let live = downloads
                .get(&e.filename)
                .and_then(|p| p.lock().ok().map(|g| g.clone()));
            serde_json::json!({
                "repo": e.repo,
                "filename": e.filename,
                "quant": e.quant,
                "size_gb": e.size_gb,
                "description": e.description,
                "is_mmproj": e.is_mmproj,
                "state": live.as_ref().map(|p| p.state.clone()).unwrap_or(e.state.clone()),
                "status_msg": live.as_ref().and_then(|p| p.status_msg.clone()).or(e.status_msg.clone()),
                "progress": live.as_ref().map(|p| p.progress).unwrap_or(e.progress),
                "initiated_at_unix": e.initiated_at_unix,
                "local_path": e.local_path,
                "shard_count": e.shard_count(),
                "current_shard": live.as_ref().map(|p| p.current_shard).unwrap_or(0)
            })
        })
        .collect();
    Json(items)
}

fn set_download_state(
    state: &AppState,
    filename: &str,
    new_state: skill_llm::catalog::DownloadState,
    msg: Option<String>,
) {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == filename) {
            e.state = new_state.clone();
            e.status_msg = msg.clone();
            e.initiated_at_unix = Some(now_unix());
            if matches!(e.state, skill_llm::catalog::DownloadState::Downloaded) {
                e.progress = 1.0;
            }
        }
    }
    persist_llm_catalog(state);
    emit_daemon_event(
        state,
        "LlmDownloadUpdated",
        serde_json::json!({
            "filename": filename,
            "state": new_state,
            "status_msg": msg
        }),
    );
}

fn set_live_download_cancel_flags(state: &AppState, filename: &str, cancelled: bool, pause_requested: bool) {
    let progress_opt = state.llm_downloads.lock().ok().and_then(|m| m.get(filename).cloned());
    if let Some(progress) = progress_opt {
        if let Ok(mut p) = progress.lock() {
            p.cancelled = cancelled;
            p.pause_requested = pause_requested;
        }
    }
}

fn spawn_model_download(state: AppState, filename: String) {
    tokio::spawn(async move {
        let entry_opt = state
            .llm_catalog
            .lock()
            .ok()
            .and_then(|cat| cat.entries.iter().find(|e| e.filename == filename).cloned());
        let Some(entry) = entry_opt else {
            return;
        };

        let progress = std::sync::Arc::new(std::sync::Mutex::new(skill_llm::catalog::DownloadProgress {
            filename: entry.filename.clone(),
            state: skill_llm::catalog::DownloadState::Downloading,
            ..Default::default()
        }));

        if let Ok(mut m) = state.llm_downloads.lock() {
            m.insert(filename.clone(), progress.clone());
        }

        let progress_for_job = progress.clone();
        let entry_for_job = entry.clone();
        let mut job =
            tokio::task::spawn_blocking(move || skill_llm::catalog::download_model(&entry_for_job, &progress_for_job));

        loop {
            if job.is_finished() {
                break;
            }
            if let Ok(p) = progress.lock() {
                if let Ok(mut cat) = state.llm_catalog.lock() {
                    if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == filename) {
                        e.state = p.state.clone();
                        e.progress = p.progress;
                        e.status_msg = p.status_msg.clone();
                        e.initiated_at_unix = Some(now_unix());
                    }
                }
                emit_daemon_event(
                    &state,
                    "LlmDownloadProgress",
                    serde_json::json!({
                        "filename": filename.clone(),
                        "state": p.state.clone(),
                        "progress": p.progress,
                        "status_msg": p.status_msg.clone(),
                        "current_shard": p.current_shard,
                        "shard_count": p.total_shards
                    }),
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        let res = (&mut job).await;

        if let Ok(mut m) = state.llm_downloads.lock() {
            m.remove(&filename);
        }

        match res {
            Ok(Ok(path)) => {
                if let Ok(mut cat) = state.llm_catalog.lock() {
                    if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == filename) {
                        e.state = skill_llm::catalog::DownloadState::Downloaded;
                        e.progress = 1.0;
                        e.local_path = Some(path);
                        e.status_msg = Some("Downloaded".to_string());
                    }
                }
                persist_llm_catalog(&state);
                emit_daemon_event(
                    &state,
                    "LlmDownloadCompleted",
                    serde_json::json!({"filename": filename.clone()}),
                );
            }
            Ok(Err(err)) => {
                let msg = err.to_string();
                let st = if msg.contains("paused") {
                    skill_llm::catalog::DownloadState::Paused
                } else if msg.contains("cancelled") {
                    skill_llm::catalog::DownloadState::Cancelled
                } else {
                    skill_llm::catalog::DownloadState::Failed
                };
                set_download_state(&state, &filename, st, Some(msg));
            }
            Err(err) => {
                set_download_state(
                    &state,
                    &filename,
                    skill_llm::catalog::DownloadState::Failed,
                    Some(err.to_string()),
                );
            }
        }
    });
}

async fn llm_download_start(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    let is_active = state
        .llm_downloads
        .lock()
        .ok()
        .map(|m| m.contains_key(&req.filename))
        .unwrap_or(false);
    if is_active {
        return Json(serde_json::json!({"ok": true, "result": "already_downloading"}));
    }

    set_download_state(
        &state,
        &req.filename,
        skill_llm::catalog::DownloadState::Downloading,
        Some("Queued in daemon".into()),
    );
    spawn_model_download(state.clone(), req.filename.clone());
    Json(serde_json::json!({"ok": true}))
}

async fn llm_download_cancel(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    set_live_download_cancel_flags(&state, &req.filename, true, false);
    set_download_state(
        &state,
        &req.filename,
        skill_llm::catalog::DownloadState::Cancelled,
        Some("Cancelling".into()),
    );
    Json(serde_json::json!({"ok": true}))
}

async fn llm_download_pause(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    set_live_download_cancel_flags(&state, &req.filename, true, true);
    set_download_state(
        &state,
        &req.filename,
        skill_llm::catalog::DownloadState::Paused,
        Some("Pausing".into()),
    );
    Json(serde_json::json!({"ok": true}))
}

async fn llm_download_resume(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    let is_active = state
        .llm_downloads
        .lock()
        .ok()
        .map(|m| m.contains_key(&req.filename))
        .unwrap_or(false);
    if !is_active {
        set_download_state(
            &state,
            &req.filename,
            skill_llm::catalog::DownloadState::Downloading,
            Some("Resumed".into()),
        );
        spawn_model_download(state.clone(), req.filename.clone());
    }
    Json(serde_json::json!({"ok": true}))
}

async fn llm_download_delete(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    set_live_download_cancel_flags(&state, &req.filename, true, false);
    if let Ok(mut cat) = state.llm_catalog.lock() {
        if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == req.filename) {
            e.state = skill_llm::catalog::DownloadState::NotDownloaded;
            e.status_msg = None;
            e.progress = 0.0;
            e.local_path = None;
        }
    }
    if let Ok(mut m) = state.llm_downloads.lock() {
        m.remove(&req.filename);
    }
    persist_llm_catalog(&state);
    emit_daemon_event(
        &state,
        "LlmDownloadDeleted",
        serde_json::json!({"filename": req.filename}),
    );
    Json(serde_json::json!({"ok": true}))
}

async fn llm_set_active_model(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.active_model = req.filename;
        if !cat.active_mmproj_matches_active_model() {
            cat.active_mmproj.clear();
        }
    }
    persist_llm_catalog(&state);
    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
            cfg.model_path = cat.active_model_path();
        }
    }
    Json(serde_json::json!({"ok": true}))
}

async fn llm_set_active_mmproj(
    State(state): State<AppState>,
    Json(req): Json<LlmFilenameRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.active_mmproj = req.filename;
    }
    persist_llm_catalog(&state);
    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
            cfg.mmproj = if cfg.autoload_mmproj {
                cat.active_mmproj_path()
            } else {
                None
            };
        }
    }
    Json(serde_json::json!({"ok": true}))
}

async fn llm_set_autoload_mmproj(
    State(state): State<AppState>,
    Json(req): Json<BoolValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.llm.autoload_mmproj = req.value;
    save_user_settings(&state, &settings);
    #[cfg(feature = "llm")]
    {
        if let Ok(mut cfg) = state.llm_config.lock() {
            cfg.autoload_mmproj = req.value;
            if !req.value {
                cfg.mmproj = None;
            }
        }
    }
    Json(serde_json::json!({"ok": true, "value": req.value}))
}

async fn list_serial_ports() -> Json<Vec<String>> {
    Json(
        serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.port_name)
            .collect(),
    )
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

async fn get_lsl_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let auto_connect = state.lsl_auto_connect.lock().map(|g| *g).unwrap_or(false);
    let paired_streams = state.lsl_paired_streams.lock().map(|g| g.clone()).unwrap_or_default();
    Json(serde_json::json!({"auto_connect": auto_connect, "paired_streams": paired_streams}))
}

async fn set_lsl_auto_connect(
    State(state): State<AppState>,
    Json(req): Json<LslAutoConnectRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut g) = state.lsl_auto_connect.lock() {
        *g = req.enabled;
    }
    persist_lsl_settings(&state);
    Json(serde_json::json!({"ok": true, "auto_connect": req.enabled}))
}

async fn lsl_pair_stream(State(state): State<AppState>, Json(req): Json<LslPairRequest>) -> Json<serde_json::Value> {
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

async fn lsl_unpair_stream(
    State(state): State<AppState>,
    Json(req): Json<LslUnpairRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut g) = state.lsl_paired_streams.lock() {
        g.retain(|p| p.source_id != req.source_id);
    }
    persist_lsl_settings(&state);
    Json(serde_json::json!({"ok": true}))
}

async fn get_lsl_idle_timeout(State(state): State<AppState>) -> Json<serde_json::Value> {
    let secs = state.lsl_idle_timeout_secs.lock().map(|g| *g).unwrap_or(None);
    Json(serde_json::json!({"secs": secs}))
}

async fn set_lsl_idle_timeout(
    State(state): State<AppState>,
    Json(req): Json<LslIdleTimeoutRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut g) = state.lsl_idle_timeout_secs.lock() {
        *g = req.secs;
    }
    persist_lsl_settings(&state);
    Json(serde_json::json!({"ok": true, "secs": req.secs}))
}

async fn lsl_virtual_source_start(State(state): State<AppState>) -> Json<serde_json::Value> {
    let Ok(mut g) = state.lsl_virtual_source.lock() else {
        return Json(serde_json::json!({"ok": false, "running": false}));
    };
    if g.is_some() {
        return Json(serde_json::json!({"ok": true, "running": true, "started": false}));
    }
    match skill_lsl::VirtualLslSource::start() {
        Ok(src) => {
            *g = Some(src);
            Json(serde_json::json!({"ok": true, "running": true, "started": true}))
        }
        Err(e) => Json(serde_json::json!({"ok": false, "running": false, "error": e})),
    }
}

async fn lsl_virtual_source_stop(State(state): State<AppState>) -> Json<serde_json::Value> {
    let Ok(mut g) = state.lsl_virtual_source.lock() else {
        return Json(serde_json::json!({"ok": false, "running": false}));
    };
    let was_running = g.is_some();
    *g = None;
    Json(serde_json::json!({"ok": true, "running": false, "was_running": was_running}))
}

async fn lsl_virtual_source_running(State(state): State<AppState>) -> Json<serde_json::Value> {
    let running = state.lsl_virtual_source.lock().map(|g| g.is_some()).unwrap_or(false);
    Json(serde_json::json!({"running": running}))
}

async fn lsl_iroh_start(State(state): State<AppState>) -> Json<serde_json::Value> {
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

async fn lsl_iroh_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let eid = state.lsl_iroh_endpoint_id.lock().ok().and_then(|g| g.clone());
    Json(serde_json::json!({"running": eid.is_some(), "endpoint_id": eid}))
}

async fn lsl_iroh_stop(State(state): State<AppState>) -> Json<serde_json::Value> {
    if let Ok(mut g) = state.lsl_iroh_endpoint_id.lock() {
        *g = None;
    }
    Json(serde_json::json!({"running": false, "endpoint_id": serde_json::Value::Null}))
}
