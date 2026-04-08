// SPDX-License-Identifier: GPL-3.0-only
//! Daemon settings/model routes.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use skill_data::{
    active_window::ActiveWindowInfo,
    activity_store::{ActiveWindowRow, ActivityStore, InputActivityRow, InputBucketRow},
};
use skill_eeg::{
    eeg_filter::{FilterConfig, PowerlineFreq},
    eeg_model_config::{EegModelStatus, ExgModelConfig},
};

use crate::{
    routes::{
        settings_exg, settings_hooks_activity,
        settings_io::{load_user_settings, save_user_settings},
        settings_llm::{
            get_exg_inference_device, get_hf_endpoint, get_inference_device, get_llm_config, set_exg_inference_device,
            set_hf_endpoint, set_inference_device, set_llm_config,
        },
        settings_llm_chat, settings_llm_runtime,
        settings_lsl::{
            get_lsl_config, get_lsl_idle_timeout, lsl_iroh_start, lsl_iroh_status, lsl_iroh_stop, lsl_pair_stream,
            lsl_unpair_stream, lsl_virtual_source_running, lsl_virtual_source_start, lsl_virtual_source_stop,
            set_lsl_auto_connect, set_lsl_idle_timeout,
        },
    },
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub(crate) struct HookLogRequest {
    pub(crate) limit: Option<i64>,
    pub(crate) offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HookKeywordsRequest {
    pub(crate) draft: String,
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HookDistanceRequest {
    pub(crate) keywords: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ActivityRecentRequest {
    pub(crate) limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ActivityBucketsRequest {
    pub(crate) from_ts: Option<u64>,
    pub(crate) to_ts: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatIdRequest {
    pub(crate) id: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatRenameRequest {
    pub(crate) id: i64,
    pub(crate) title: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatSaveMessageRequest {
    pub(crate) session_id: i64,
    pub(crate) role: String,
    pub(crate) content: String,
    pub(crate) thinking: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatSessionParamsRequest {
    pub(crate) id: i64,
    pub(crate) params_json: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatSaveToolCallsRequest {
    pub(crate) message_id: i64,
    pub(crate) tool_calls: Vec<skill_llm::chat_store::StoredToolCall>,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct ChatSessionResponse {
    pub(crate) session_id: i64,
    pub(crate) messages: Vec<skill_llm::chat_store::StoredMessage>,
}

#[derive(Debug, Deserialize)]
struct U64ValueRequest {
    value: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BoolValueRequest {
    pub(crate) value: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StringValueRequest {
    pub(crate) value: String,
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
pub(crate) struct FilenameRequest {
    pub(crate) filename: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub(crate) struct ChatCompletionsRequest {
    pub(crate) messages: Vec<serde_json::Value>,
    pub(crate) params: serde_json::Value,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub(crate) struct ToolCancelRequest {
    pub(crate) tool_call_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LlmAddModelRequest {
    pub(crate) repo: String,
    pub(crate) filename: String,
    pub(crate) size_gb: Option<f32>,
    pub(crate) mmproj: Option<String>,
    pub(crate) download: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LlmFilenameRequest {
    pub(crate) filename: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LlmImageRequest {
    pub(crate) png_base64: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(exg_routes())
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
        .route("/activity/latest-bands", get(get_latest_bands))
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
        .route("/device/serial-ports", get(list_serial_ports))
        .merge(llm_routes())
        .merge(lsl_routes())
}

fn exg_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/models/config",
            get(get_model_config).put(set_model_config).post(set_model_config),
        )
        .route("/models/status", get(get_model_status))
        .route("/models/trigger-reembed", post(trigger_reembed))
        .route("/models/trigger-weights-download", post(trigger_weights_download))
        .route("/models/cancel-weights-download", post(cancel_weights_download))
        .route("/models/estimate-reembed", get(estimate_reembed))
        .route("/models/rebuild-index", post(rebuild_index))
        .route("/models/exg-catalog", get(get_exg_catalog))
}

fn llm_routes() -> Router<AppState> {
    Router::new()
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
}

fn lsl_routes() -> Router<AppState> {
    Router::new()
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

async fn get_model_config(state: State<AppState>) -> Json<ExgModelConfig> {
    settings_exg::get_model_config_impl(state).await
}

async fn set_model_config(state: State<AppState>, config: Json<ExgModelConfig>) -> Json<serde_json::Value> {
    settings_exg::set_model_config_impl(state, config).await
}

async fn get_model_status(state: State<AppState>) -> Json<EegModelStatus> {
    settings_exg::get_model_status_impl(state).await
}

/// Public so `main.rs` can call it during daemon startup.
pub fn probe_weights_for_config(config: &ExgModelConfig) -> Option<(String, String)> {
    settings_exg::probe_weights_for_config(config)
}

async fn trigger_reembed() -> Json<serde_json::Value> {
    settings_exg::trigger_reembed_impl().await
}

async fn trigger_weights_download(state: State<AppState>) -> Json<serde_json::Value> {
    settings_exg::trigger_weights_download_impl(state).await
}

async fn cancel_weights_download(state: State<AppState>) -> Json<serde_json::Value> {
    settings_exg::cancel_weights_download_impl(state).await
}

async fn estimate_reembed(state: State<AppState>) -> Json<serde_json::Value> {
    settings_exg::estimate_reembed_impl(state).await
}

async fn rebuild_index() -> Json<serde_json::Value> {
    settings_exg::rebuild_index_impl().await
}

async fn get_exg_catalog(state: State<AppState>) -> Json<serde_json::Value> {
    settings_exg::get_exg_catalog_impl(state).await
}

async fn get_hooks(state: State<AppState>) -> Json<Vec<skill_settings::HookRule>> {
    settings_hooks_activity::get_hooks_impl(state).await
}

async fn set_hooks(state: State<AppState>, hooks: Json<Vec<skill_settings::HookRule>>) -> Json<serde_json::Value> {
    settings_hooks_activity::set_hooks_impl(state, hooks).await
}

async fn get_hook_statuses(state: State<AppState>) -> Json<serde_json::Value> {
    settings_hooks_activity::get_hook_statuses_impl(state).await
}

async fn get_hook_log(
    state: State<AppState>,
    req: Json<HookLogRequest>,
) -> Json<Vec<skill_data::hooks_log::HookLogRow>> {
    settings_hooks_activity::get_hook_log_impl(state, req).await
}

async fn get_hook_log_count(state: State<AppState>) -> Json<serde_json::Value> {
    settings_hooks_activity::get_hook_log_count_impl(state).await
}

async fn suggest_hook_keywords(state: State<AppState>, req: Json<HookKeywordsRequest>) -> Json<Vec<serde_json::Value>> {
    settings_hooks_activity::suggest_hook_keywords_impl(state, req).await
}

async fn suggest_hook_distances(state: State<AppState>, req: Json<HookDistanceRequest>) -> Json<serde_json::Value> {
    settings_hooks_activity::suggest_hook_distances_impl(state, req).await
}

async fn activity_recent_windows(
    state: State<AppState>,
    req: Json<ActivityRecentRequest>,
) -> Json<Vec<ActiveWindowRow>> {
    settings_hooks_activity::activity_recent_windows_impl(state, req).await
}

async fn activity_recent_input(
    state: State<AppState>,
    req: Json<ActivityRecentRequest>,
) -> Json<Vec<InputActivityRow>> {
    settings_hooks_activity::activity_recent_input_impl(state, req).await
}

async fn activity_input_buckets(
    state: State<AppState>,
    req: Json<ActivityBucketsRequest>,
) -> Json<Vec<InputBucketRow>> {
    settings_hooks_activity::activity_input_buckets_impl(state, req).await
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

async fn get_latest_bands(State(state): State<AppState>) -> Json<serde_json::Value> {
    let bands = state.latest_bands.lock().map(|g| g.clone()).unwrap_or(None);
    match bands {
        Some(b) => Json(serde_json::to_value(b).unwrap_or(serde_json::Value::Null)),
        None => Json(serde_json::Value::Null),
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
        "neurosity_email": c.neurosity_email,
        "neurosity_password": c.neurosity_password,
        "neurosity_device_id": c.neurosity_device_id,
        "brainmaster_model": c.brainmaster_model,
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

async fn llm_server_start(state: State<AppState>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_server_start_impl(state).await
}

async fn llm_server_stop(state: State<AppState>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_server_stop_impl(state).await
}

async fn llm_server_status(state: State<AppState>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_server_status_impl(state).await
}

async fn llm_server_logs(state: State<AppState>) -> Json<Vec<serde_json::Value>> {
    settings_llm_runtime::llm_server_logs_impl(state).await
}

async fn llm_server_switch_model(state: State<AppState>, req: Json<FilenameRequest>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_server_switch_model_impl(state, req).await
}

async fn llm_server_switch_mmproj(state: State<AppState>, req: Json<FilenameRequest>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_server_switch_mmproj_impl(state, req).await
}

async fn chat_last_session(state: State<AppState>) -> Json<ChatSessionResponse> {
    settings_llm_chat::chat_last_session_impl(state).await
}

async fn chat_load_session(state: State<AppState>, req: Json<ChatIdRequest>) -> Json<ChatSessionResponse> {
    settings_llm_chat::chat_load_session_impl(state, req).await
}

async fn chat_list_sessions(state: State<AppState>) -> Json<Vec<skill_llm::chat_store::SessionSummary>> {
    settings_llm_chat::chat_list_sessions_impl(state).await
}

async fn chat_rename_session(state: State<AppState>, req: Json<ChatRenameRequest>) -> Json<serde_json::Value> {
    settings_llm_chat::chat_rename_session_impl(state, req).await
}

async fn chat_delete_session(state: State<AppState>, req: Json<ChatIdRequest>) -> Json<serde_json::Value> {
    settings_llm_chat::chat_delete_session_impl(state, req).await
}

async fn chat_archive_session(state: State<AppState>, req: Json<ChatIdRequest>) -> Json<serde_json::Value> {
    settings_llm_chat::chat_archive_session_impl(state, req).await
}

async fn chat_unarchive_session(state: State<AppState>, req: Json<ChatIdRequest>) -> Json<serde_json::Value> {
    settings_llm_chat::chat_unarchive_session_impl(state, req).await
}

async fn chat_list_archived_sessions(state: State<AppState>) -> Json<Vec<skill_llm::chat_store::SessionSummary>> {
    settings_llm_chat::chat_list_archived_sessions_impl(state).await
}

async fn chat_save_message(state: State<AppState>, req: Json<ChatSaveMessageRequest>) -> Json<serde_json::Value> {
    settings_llm_chat::chat_save_message_impl(state, req).await
}

async fn chat_get_session_params(state: State<AppState>, req: Json<ChatIdRequest>) -> Json<serde_json::Value> {
    settings_llm_chat::chat_get_session_params_impl(state, req).await
}

async fn chat_set_session_params(
    state: State<AppState>,
    req: Json<ChatSessionParamsRequest>,
) -> Json<serde_json::Value> {
    settings_llm_chat::chat_set_session_params_impl(state, req).await
}

async fn chat_new_session(state: State<AppState>) -> Json<serde_json::Value> {
    settings_llm_chat::chat_new_session_impl(state).await
}

async fn chat_save_tool_calls(state: State<AppState>, req: Json<ChatSaveToolCallsRequest>) -> Json<serde_json::Value> {
    settings_llm_chat::chat_save_tool_calls_impl(state, req).await
}

async fn llm_chat_completions(state: State<AppState>, req: Json<ChatCompletionsRequest>) -> Json<serde_json::Value> {
    settings_llm_chat::llm_chat_completions_impl(state, req).await
}

async fn llm_embed_image(state: State<AppState>, req: Json<LlmImageRequest>) -> Json<serde_json::Value> {
    settings_llm_chat::llm_embed_image_impl(state, req).await
}

async fn llm_ocr(state: State<AppState>, req: Json<LlmImageRequest>) -> Json<serde_json::Value> {
    settings_llm_chat::llm_ocr_impl(state, req).await
}

async fn llm_abort_stream(state: State<AppState>) -> Json<serde_json::Value> {
    settings_llm_chat::llm_abort_stream_impl(state).await
}

async fn llm_cancel_tool_call(state: State<AppState>, req: Json<ToolCancelRequest>) -> Json<serde_json::Value> {
    settings_llm_chat::llm_cancel_tool_call_impl(state, req).await
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

async fn llm_get_catalog(state: State<AppState>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_get_catalog_impl(state).await
}

async fn llm_refresh_catalog(state: State<AppState>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_refresh_catalog_impl(state).await
}

async fn llm_add_model(state: State<AppState>, req: Json<LlmAddModelRequest>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_add_model_impl(state, req).await
}

async fn llm_get_downloads(state: State<AppState>) -> Json<Vec<serde_json::Value>> {
    settings_llm_runtime::llm_get_downloads_impl(state).await
}

async fn llm_download_start(state: State<AppState>, req: Json<LlmFilenameRequest>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_download_start_impl(state, req).await
}

async fn llm_download_cancel(state: State<AppState>, req: Json<LlmFilenameRequest>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_download_cancel_impl(state, req).await
}

async fn llm_download_pause(state: State<AppState>, req: Json<LlmFilenameRequest>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_download_pause_impl(state, req).await
}

async fn llm_download_resume(state: State<AppState>, req: Json<LlmFilenameRequest>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_download_resume_impl(state, req).await
}

async fn llm_download_delete(state: State<AppState>, req: Json<LlmFilenameRequest>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_download_delete_impl(state, req).await
}

async fn llm_set_active_model(state: State<AppState>, req: Json<LlmFilenameRequest>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_set_active_model_impl(state, req).await
}

async fn llm_set_active_mmproj(state: State<AppState>, req: Json<LlmFilenameRequest>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_set_active_mmproj_impl(state, req).await
}

async fn llm_set_autoload_mmproj(state: State<AppState>, req: Json<BoolValueRequest>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_set_autoload_mmproj_impl(state, req).await
}

async fn list_serial_ports() -> Json<Vec<String>> {
    // `serialport::available_ports()` performs blocking I/O (Windows registry
    // queries / Linux sysfs reads) and can stall for several seconds if a
    // USB driver is misbehaving.  Run it off the async runtime with a timeout.
    let ports = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::task::spawn_blocking(|| {
            serialport::available_ports()
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.port_name)
                .collect::<Vec<String>>()
        }),
    )
    .await
    .ok()
    .and_then(std::result::Result::ok)
    .unwrap_or_default();

    Json(ports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::settings_lsl::{LslAutoConnectRequest, LslIdleTimeoutRequest, LslPairRequest, LslUnpairRequest};
    use std::sync::atomic::Ordering;
    use tempfile::TempDir;

    fn mk_state() -> (TempDir, AppState) {
        let td = TempDir::new().unwrap();
        let st = AppState::new("t".into(), td.path().to_path_buf());
        (td, st)
    }

    #[tokio::test]
    async fn api_token_roundtrip() {
        let (_td, st) = mk_state();
        let Json(v) = set_api_token(State(st.clone()), Json(StringValueRequest { value: "abc123".into() })).await;
        assert_eq!(v["ok"], true);
        let Json(v2) = get_api_token(State(st)).await;
        assert_eq!(v2["value"], "abc123");
    }

    #[tokio::test]
    async fn hf_endpoint_empty_defaults_and_trimmed_custom() {
        let (_td, st) = mk_state();
        let Json(v0) = get_hf_endpoint(State(st.clone())).await;
        assert!(!v0["value"].as_str().unwrap_or("").is_empty());

        let Json(v1) = set_hf_endpoint(
            State(st.clone()),
            Json(StringValueRequest {
                value: "  https://example.test  ".into(),
            }),
        )
        .await;
        assert_eq!(v1["ok"], true);
        assert_eq!(v1["value"], "https://example.test");

        let Json(v2) = set_hf_endpoint(State(st), Json(StringValueRequest { value: " ".into() })).await;
        assert_eq!(v2["ok"], true);
        assert!(!v2["value"].as_str().unwrap_or("").is_empty());
    }

    #[tokio::test]
    async fn storage_format_normalizes_values() {
        let (_td, st) = mk_state();
        let Json(v_csv) =
            set_storage_format(State(st.clone()), Json(StringValueRequest { value: "weird".into() })).await;
        assert_eq!(v_csv["value"], "csv");

        let Json(v_parquet) = set_storage_format(
            State(st.clone()),
            Json(StringValueRequest {
                value: "PARQUET".into(),
            }),
        )
        .await;
        assert_eq!(v_parquet["value"], "parquet");

        let Json(v_both) = set_storage_format(State(st), Json(StringValueRequest { value: "both".into() })).await;
        assert_eq!(v_both["value"], "both");
    }

    #[tokio::test]
    async fn embedding_overlap_is_clamped() {
        let (_td, st) = mk_state();
        let Json(v_hi) = set_embedding_overlap(State(st.clone()), Json(serde_json::json!({"value": 99999.0}))).await;
        let hi = v_hi["value"].as_f64().unwrap_or(0.0) as f32;
        assert_eq!(hi, skill_constants::EMBEDDING_OVERLAP_MAX_SECS);

        let Json(v_lo) = set_embedding_overlap(State(st), Json(serde_json::json!({"value": -1.0}))).await;
        let lo = v_lo["value"].as_f64().unwrap_or(0.0) as f32;
        assert_eq!(lo, skill_constants::EMBEDDING_OVERLAP_MIN_SECS);
    }

    #[tokio::test]
    async fn ws_config_validates_host_and_port() {
        let (_td, st) = mk_state();
        let Json(bad_host) = set_ws_config(
            State(st.clone()),
            Json(WsConfigRequest {
                host: "localhost".into(),
                port: 18444,
            }),
        )
        .await;
        assert_eq!(bad_host["ok"], false);

        let Json(bad_port) = set_ws_config(
            State(st.clone()),
            Json(WsConfigRequest {
                host: "127.0.0.1".into(),
                port: 80,
            }),
        )
        .await;
        assert_eq!(bad_port["ok"], false);

        let Json(ok) = set_ws_config(
            State(st),
            Json(WsConfigRequest {
                host: "0.0.0.0".into(),
                port: 18445,
            }),
        )
        .await;
        assert_eq!(ok["ok"], true);
        assert_eq!(ok["port"], 18445);
    }

    #[tokio::test]
    async fn activity_tracking_toggles_update_state() {
        let (_td, st) = mk_state();
        let Json(v1) = set_active_window_tracking(State(st.clone()), Json(BoolValueRequest { value: true })).await;
        assert_eq!(v1["value"], true);
        assert!(st.track_active_window.load(Ordering::Relaxed));

        let Json(v2) = set_input_activity_tracking(State(st.clone()), Json(BoolValueRequest { value: false })).await;
        assert_eq!(v2["value"], false);
        assert!(!st.track_input_activity.load(Ordering::Relaxed));

        let Json(g1) = get_active_window_tracking(State(st.clone())).await;
        let Json(g2) = get_input_activity_tracking(State(st)).await;
        assert_eq!(g1["value"], true);
        assert_eq!(g2["value"], false);
    }

    #[tokio::test]
    async fn lsl_config_pair_unpair_and_idle_timeout_roundtrip() {
        let (_td, st) = mk_state();

        let Json(v1) = set_lsl_auto_connect(State(st.clone()), Json(LslAutoConnectRequest { enabled: true })).await;
        assert_eq!(v1["ok"], true);
        assert_eq!(v1["auto_connect"], true);

        let Json(v2) = lsl_pair_stream(
            State(st.clone()),
            Json(LslPairRequest {
                source_id: "src-1".into(),
                name: "My EEG".into(),
                stream_type: "EEG".into(),
                channels: 8,
                sample_rate: 256.0,
            }),
        )
        .await;
        assert_eq!(v2["ok"], true);

        let Json(cfg) = get_lsl_config(State(st.clone())).await;
        assert_eq!(cfg["auto_connect"], true);
        assert_eq!(cfg["paired_streams"].as_array().map(|a| a.len()).unwrap_or(0), 1);

        let Json(v3) = set_lsl_idle_timeout(State(st.clone()), Json(LslIdleTimeoutRequest { secs: Some(77) })).await;
        assert_eq!(v3["ok"], true);
        let Json(timeout) = get_lsl_idle_timeout(State(st.clone())).await;
        assert_eq!(timeout["secs"], 77);

        let Json(v4) = lsl_unpair_stream(
            State(st),
            Json(LslUnpairRequest {
                source_id: "src-1".into(),
            }),
        )
        .await;
        assert_eq!(v4["ok"], true);
    }

    #[tokio::test]
    async fn lsl_iroh_lifecycle_is_consistent() {
        let (_td, st) = mk_state();
        let Json(start) = lsl_iroh_start(State(st.clone())).await;
        assert_eq!(start["running"], true);
        let id = start["endpoint_id"].as_str().unwrap_or("").to_string();
        assert_eq!(id.len(), 16);

        let Json(status) = lsl_iroh_status(State(st.clone())).await;
        assert_eq!(status["running"], true);
        assert_eq!(status["endpoint_id"], id);

        let Json(stop) = lsl_iroh_stop(State(st.clone())).await;
        assert_eq!(stop["running"], false);

        let Json(status2) = lsl_iroh_status(State(st)).await;
        assert_eq!(status2["running"], false);
    }

    #[tokio::test]
    async fn lsl_virtual_source_running_and_stop_when_not_started() {
        let (_td, st) = mk_state();
        let Json(r0) = lsl_virtual_source_running(State(st.clone())).await;
        assert_eq!(r0["running"], false);

        let Json(stop) = lsl_virtual_source_stop(State(st.clone())).await;
        assert_eq!(stop["ok"], true);
        assert_eq!(stop["was_running"], false);

        let Json(r1) = lsl_virtual_source_running(State(st)).await;
        assert_eq!(r1["running"], false);
    }

    #[tokio::test]
    async fn latest_bands_null_then_value() {
        let (_td, st) = mk_state();
        let Json(v0) = get_latest_bands(State(st.clone())).await;
        assert!(v0.is_null());

        if let Ok(mut g) = st.latest_bands.lock() {
            *g = Some(serde_json::json!({"alpha": 1.23}));
        }
        let Json(v1) = get_latest_bands(State(st)).await;
        assert_eq!(v1["alpha"], 1.23);
    }

    #[tokio::test]
    async fn hooks_roundtrip_status_and_log_queries() {
        let (td, st) = mk_state();

        let hook = skill_settings::HookRule {
            name: "focus".into(),
            enabled: true,
            keywords: vec!["focus".into()],
            scenario: "any".into(),
            command: "say".into(),
            text: "yo".into(),
            distance_threshold: 0.2,
            recent_limit: 10,
        };
        let Json(v) = set_hooks(State(st.clone()), Json(vec![hook.clone()])).await;
        assert_eq!(v["ok"], true);

        let Json(hooks) = get_hooks(State(st.clone())).await;
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].name, "focus");

        let Json(statuses) = get_hook_statuses(State(st.clone())).await;
        let arr = statuses.as_array().cloned().unwrap_or_default();
        assert_eq!(arr.len(), 1);
        assert!(arr[0].get("last_trigger").map(|v| v.is_null()).unwrap_or(false));

        let log = skill_data::hooks_log::HooksLog::open(td.path()).expect("open hooks log");
        log.record(&skill_data::hooks_log::HookFireEntry {
            triggered_at_utc: 100,
            hook_json: "{}",
            trigger_json: "{\"distance\":0.3}",
            payload_json: "{}",
        });
        log.record(&skill_data::hooks_log::HookFireEntry {
            triggered_at_utc: 101,
            hook_json: "{}",
            trigger_json: "{\"eegDistance\":0.8}",
            payload_json: "{}",
        });

        let Json(count) = get_hook_log_count(State(st.clone())).await;
        assert_eq!(count["count"], 2);

        let Json(rows) = get_hook_log(
            State(st.clone()),
            Json(HookLogRequest {
                limit: Some(1),
                offset: Some(0),
            }),
        )
        .await;
        assert_eq!(rows.len(), 1);

        let Json(d) = suggest_hook_distances(
            State(st),
            Json(HookDistanceRequest {
                keywords: vec!["focus".into()],
            }),
        )
        .await;
        assert_eq!(d["sample_n"], 2);
        assert!(d["suggested"].as_f64().unwrap_or(0.0) > 0.0);
    }

    #[tokio::test]
    async fn suggest_hook_keywords_finds_matching_labels() {
        let (td, st) = mk_state();
        let db = td.path().join(skill_constants::LABELS_FILE);
        let conn = rusqlite::Connection::open(db).unwrap();
        conn.execute_batch("CREATE TABLE labels (text TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT 0);")
            .unwrap();
        conn.execute(
            "INSERT INTO labels (text, created_at) VALUES (?1, ?2)",
            rusqlite::params!["Deep Focus", 1_i64],
        )
        .unwrap();

        let Json(items) = suggest_hook_keywords(
            State(st),
            Json(HookKeywordsRequest {
                draft: "focu".into(),
                limit: Some(8),
            }),
        )
        .await;
        assert!(!items.is_empty());
        assert!(items[0]["keyword"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("focu"));
    }

    #[tokio::test]
    async fn web_cache_endpoints_smoke() {
        let Json(stats) = web_cache_stats().await;
        assert!(stats.get("total_entries").is_some());

        let Json(list) = web_cache_list().await;
        let _ = list.len();

        let Json(cleared) = web_cache_clear().await;
        assert!(cleared.get("removed").is_some());

        let Json(rm_domain) = web_cache_remove_domain(Json(StringValueRequest {
            value: "example.com".into(),
        }))
        .await;
        assert!(rm_domain.get("removed").is_some());

        let Json(rm_key) = web_cache_remove_entry(Json(StringKeyRequest { key: "k".into() })).await;
        assert!(rm_key.get("removed").is_some());
    }

    #[tokio::test]
    async fn chat_session_lifecycle_roundtrip() {
        let (_td, st) = mk_state();

        let Json(new_s) = chat_new_session(State(st.clone())).await;
        let sid = new_s["id"].as_i64().unwrap_or(0);
        assert!(sid > 0);

        let Json(saved) = chat_save_message(
            State(st.clone()),
            Json(ChatSaveMessageRequest {
                session_id: sid,
                role: "user".into(),
                content: "hello".into(),
                thinking: None,
            }),
        )
        .await;
        assert!(saved["id"].as_i64().unwrap_or(0) > 0);

        let Json(loaded) = chat_load_session(State(st.clone()), Json(ChatIdRequest { id: sid })).await;
        assert_eq!(loaded.session_id, sid);
        assert!(!loaded.messages.is_empty());

        let Json(_ok) = chat_rename_session(
            State(st.clone()),
            Json(ChatRenameRequest {
                id: sid,
                title: "renamed".into(),
            }),
        )
        .await;

        let Json(active) = chat_list_sessions(State(st.clone())).await;
        assert!(active.iter().any(|s| s.id == sid));

        let Json(_arch) = chat_archive_session(State(st.clone()), Json(ChatIdRequest { id: sid })).await;
        let Json(archived) = chat_list_archived_sessions(State(st.clone())).await;
        assert!(archived.iter().any(|s| s.id == sid));

        let Json(_unarch) = chat_unarchive_session(State(st.clone()), Json(ChatIdRequest { id: sid })).await;
        let Json(_del) = chat_delete_session(State(st.clone()), Json(ChatIdRequest { id: sid })).await;
        let Json(active2) = chat_list_sessions(State(st)).await;
        assert!(!active2.iter().any(|s| s.id == sid));
    }

    #[tokio::test]
    async fn llm_download_state_and_active_selection_paths() {
        let (_td, st) = mk_state();

        let mut cat = st.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
        cat.entries.push(skill_llm::catalog::LlmModelEntry {
            repo: "x/y".into(),
            filename: "model.gguf".into(),
            quant: "Q4".into(),
            size_gb: 1.0,
            description: "m".into(),
            family_id: "fam".into(),
            family_name: "Fam".into(),
            family_desc: String::new(),
            tags: vec![],
            is_mmproj: false,
            recommended: false,
            advanced: false,
            params_b: 1.0,
            max_context_length: 2048,
            shard_files: vec![],
            local_path: None,
            state: skill_llm::catalog::DownloadState::NotDownloaded,
            status_msg: None,
            progress: 0.0,
            initiated_at_unix: None,
        });
        cat.entries.push(skill_llm::catalog::LlmModelEntry {
            repo: "x/y".into(),
            filename: "model-mmproj-f16.gguf".into(),
            quant: "F16".into(),
            size_gb: 0.2,
            description: "mm".into(),
            family_id: "fam".into(),
            family_name: "Fam".into(),
            family_desc: String::new(),
            tags: vec![],
            is_mmproj: true,
            recommended: false,
            advanced: false,
            params_b: 0.0,
            max_context_length: 0,
            shard_files: vec![],
            local_path: None,
            state: skill_llm::catalog::DownloadState::NotDownloaded,
            status_msg: None,
            progress: 0.0,
            initiated_at_unix: None,
        });
        if let Ok(mut g) = st.llm_catalog.lock() {
            *g = cat;
        }

        let Json(c) = llm_download_cancel(
            State(st.clone()),
            Json(LlmFilenameRequest {
                filename: "model.gguf".into(),
            }),
        )
        .await;
        assert_eq!(c["ok"], true);

        let Json(p) = llm_download_pause(
            State(st.clone()),
            Json(LlmFilenameRequest {
                filename: "model.gguf".into(),
            }),
        )
        .await;
        assert_eq!(p["ok"], true);

        let Json(sel_model) = llm_set_active_model(
            State(st.clone()),
            Json(LlmFilenameRequest {
                filename: "model.gguf".into(),
            }),
        )
        .await;
        assert_eq!(sel_model["ok"], true);

        let Json(sel_mmproj) = llm_set_active_mmproj(
            State(st.clone()),
            Json(LlmFilenameRequest {
                filename: "model-mmproj-f16.gguf".into(),
            }),
        )
        .await;
        assert_eq!(sel_mmproj["ok"], true);

        let Json(del) = llm_download_delete(
            State(st.clone()),
            Json(LlmFilenameRequest {
                filename: "model.gguf".into(),
            }),
        )
        .await;
        assert_eq!(del["ok"], true);

        let cat_after = st.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
        let e = cat_after.entries.iter().find(|e| e.filename == "model.gguf").unwrap();
        assert!(matches!(e.state, skill_llm::catalog::DownloadState::NotDownloaded));
    }

    #[tokio::test]
    async fn set_lsl_idle_timeout_accepts_none() {
        let (_td, st) = mk_state();
        let Json(v) = set_lsl_idle_timeout(State(st.clone()), Json(LslIdleTimeoutRequest { secs: None })).await;
        assert_eq!(v["ok"], true);
        assert!(v["secs"].is_null());

        let Json(got) = get_lsl_idle_timeout(State(st)).await;
        assert!(got["secs"].is_null());
    }

    #[tokio::test]
    async fn lsl_pair_stream_updates_existing_source() {
        let (_td, st) = mk_state();
        let _ = lsl_pair_stream(
            State(st.clone()),
            Json(LslPairRequest {
                source_id: "src".into(),
                name: "A".into(),
                stream_type: "EEG".into(),
                channels: 4,
                sample_rate: 256.0,
            }),
        )
        .await;
        let _ = lsl_pair_stream(
            State(st.clone()),
            Json(LslPairRequest {
                source_id: "src".into(),
                name: "B".into(),
                stream_type: "EEG".into(),
                channels: 8,
                sample_rate: 512.0,
            }),
        )
        .await;

        let Json(cfg) = get_lsl_config(State(st)).await;
        let arr = cfg["paired_streams"].as_array().cloned().unwrap_or_default();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "B");
        assert_eq!(arr[0]["channels"], 8);
    }

    #[tokio::test]
    async fn lsl_iroh_start_is_idempotent_when_running() {
        let (_td, st) = mk_state();
        let Json(a) = lsl_iroh_start(State(st.clone())).await;
        let Json(b) = lsl_iroh_start(State(st)).await;
        assert_eq!(a["running"], true);
        assert_eq!(b["running"], true);
        assert_eq!(a["endpoint_id"], b["endpoint_id"]);
    }

    #[tokio::test]
    async fn inference_device_roundtrip_cpu_then_gpu() {
        let (_td, st) = mk_state();

        let Json(cpu) = set_inference_device(State(st.clone()), Json(StringValueRequest { value: "cpu".into() })).await;
        assert_eq!(cpu["ok"], true);
        assert_eq!(cpu["value"], "cpu");

        let Json(gpu) = set_inference_device(State(st.clone()), Json(StringValueRequest { value: "gpu".into() })).await;
        assert_eq!(gpu["ok"], true);
        assert_eq!(gpu["value"], "gpu");

        let Json(cur) = get_inference_device(State(st)).await;
        assert_eq!(cur["value"], "gpu");
    }

    #[tokio::test]
    async fn exg_routes_smoke_config_status_and_catalog() {
        let (_td, st) = mk_state();

        let Json(cfg) = get_model_config(State(st.clone())).await;
        let Json(set_ok) = set_model_config(State(st.clone()), Json(cfg.clone())).await;
        assert_eq!(set_ok["ok"], true);

        let Json(status) = get_model_status(State(st.clone())).await;
        let _ = status.weights_found;

        let Json(catalog) = get_exg_catalog(State(st.clone())).await;
        assert!(catalog.get("families").is_some());

        let Json(r1) = trigger_reembed().await;
        assert_eq!(r1["ok"], true);

        let Json(r2) = rebuild_index().await;
        assert_eq!(r2["ok"], true);

        let Json(est) = estimate_reembed(State(st)).await;
        assert!(est.get("sessions_total").is_some());
    }

    #[tokio::test]
    async fn llm_download_start_already_downloading_short_circuits() {
        let (_td, st) = mk_state();
        if let Ok(mut m) = st.llm_downloads.lock() {
            m.insert(
                "model.gguf".into(),
                std::sync::Arc::new(std::sync::Mutex::new(skill_llm::catalog::DownloadProgress::default())),
            );
        }

        let Json(v) = llm_download_start(
            State(st),
            Json(LlmFilenameRequest {
                filename: "model.gguf".into(),
            }),
        )
        .await;
        assert_eq!(v["ok"], true);
        assert_eq!(v["result"], "already_downloading");
    }

    #[tokio::test]
    async fn llm_pause_cancel_update_live_flags() {
        let (_td, st) = mk_state();
        let progress = std::sync::Arc::new(std::sync::Mutex::new(skill_llm::catalog::DownloadProgress::default()));
        if let Ok(mut m) = st.llm_downloads.lock() {
            m.insert("model.gguf".into(), progress.clone());
        }

        let _ = llm_download_pause(
            State(st.clone()),
            Json(LlmFilenameRequest {
                filename: "model.gguf".into(),
            }),
        )
        .await;
        {
            let p = progress.lock().unwrap();
            assert!(p.cancelled);
            assert!(p.pause_requested);
        }

        let _ = llm_download_cancel(
            State(st),
            Json(LlmFilenameRequest {
                filename: "model.gguf".into(),
            }),
        )
        .await;
        let p = progress.lock().unwrap();
        assert!(p.cancelled);
        assert!(!p.pause_requested);
    }

    #[tokio::test]
    async fn settings_router_contract_core_paths_exist() {
        use axum::body::Body;
        use tower::ServiceExt;

        let (_td, st) = mk_state();
        let app = router().with_state(st);

        let cases = [
            (axum::http::Method::GET, "/models/status"),
            (axum::http::Method::POST, "/models/trigger-reembed"),
            (axum::http::Method::GET, "/llm/catalog"),
            (axum::http::Method::POST, "/llm/download/start"),
            (axum::http::Method::GET, "/lsl/config"),
            (axum::http::Method::POST, "/lsl/pair"),
        ];

        for (method, uri) in cases {
            let req = axum::http::Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_ne!(resp.status(), axum::http::StatusCode::NOT_FOUND, "missing route {uri}");
        }
    }
}
