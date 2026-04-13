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
use skill_eeg::eeg_model_config::{EegModelStatus, ExgModelConfig};

use crate::{
    routes::{
        settings_device, settings_exg, settings_hooks_activity,
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
        settings_screenshots, settings_ui,
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatSaveMessageRequest {
    pub(crate) session_id: i64,
    pub(crate) role: String,
    pub(crate) content: String,
    pub(crate) thinking: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatSessionParamsRequest {
    pub(crate) id: i64,
    pub(crate) params_json: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
pub(crate) struct BoolValueRequest {
    pub(crate) value: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StringValueRequest {
    pub(crate) value: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FilenameRequest {
    pub(crate) filename: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub(crate) struct ChatCompletionsRequest {
    pub(crate) messages: Vec<serde_json::Value>,
    /// Custom params object (Skill UI sends this).
    #[serde(default)]
    pub(crate) params: serde_json::Value,
    /// OpenAI-compatible fields — forwarded as params when `params` is absent.
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) max_tokens: Option<u32>,
    #[serde(default)]
    pub(crate) temperature: Option<f64>,
    #[serde(default)]
    pub(crate) stream: Option<bool>,
    #[serde(default)]
    pub(crate) stop: Option<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolCancelRequest {
    pub(crate) tool_call_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
        .route(
            "/settings/api-token",
            get(settings_ui::get_api_token).post(settings_ui::set_api_token),
        )
        .route("/settings/hf-endpoint", get(get_hf_endpoint).post(set_hf_endpoint))
        .route(
            "/settings/filter-config",
            get(settings_device::get_filter_config).post(settings_device::set_filter_config),
        )
        .route("/settings/notch-preset", post(settings_device::set_notch_preset))
        .route(
            "/settings/storage-format",
            get(settings_device::get_storage_format).post(settings_device::set_storage_format),
        )
        .route(
            "/settings/embedding-overlap",
            get(settings_device::get_embedding_overlap).post(settings_device::set_embedding_overlap),
        )
        .route(
            "/settings/update-check-interval",
            get(settings_device::get_update_check_interval).post(settings_device::set_update_check_interval),
        )
        .route(
            "/settings/openbci-config",
            get(settings_device::get_openbci_config).post(settings_device::set_openbci_config),
        )
        .route(
            "/settings/device-api-config",
            get(settings_device::get_device_api_config).post(settings_device::set_device_api_config),
        )
        .route(
            "/settings/scanner-config",
            get(settings_device::get_scanner_config).post(settings_device::set_scanner_config),
        )
        .route("/settings/device-log", get(settings_device::get_device_log))
        .route(
            "/settings/neutts-config",
            get(settings_ui::get_neutts_config).post(settings_ui::set_neutts_config),
        )
        .route("/settings/llm-config", get(get_llm_config).post(set_llm_config))
        .route(
            "/settings/tts-preload",
            get(settings_ui::get_tts_preload).post(settings_ui::set_tts_preload),
        )
        .route(
            "/settings/sleep-config",
            get(settings_ui::get_sleep_config).post(settings_ui::set_sleep_config),
        )
        .route(
            "/settings/ws-config",
            get(settings_ui::get_ws_config).post(settings_ui::set_ws_config),
        )
        .route(
            "/settings/inference-device",
            get(get_inference_device).post(set_inference_device),
        )
        .route(
            "/settings/exg-inference-device",
            get(get_exg_inference_device).post(set_exg_inference_device),
        )
        .route(
            "/settings/iroh-logs",
            get(settings_ui::get_iroh_logs).post(settings_ui::set_iroh_logs),
        )
        .route(
            "/settings/location-enabled",
            get(settings_ui::get_location_enabled).post(settings_ui::set_location_enabled),
        )
        .route("/settings/location-test", post(settings_ui::test_location))
        .route(
            "/settings/umap-config",
            get(settings_ui::get_umap_config).post(settings_ui::set_umap_config),
        )
        .route("/settings/gpu-stats", get(settings_ui::get_gpu_stats))
        .route("/settings/web-cache/stats", get(settings_ui::web_cache_stats))
        .route("/settings/web-cache/list", get(settings_ui::web_cache_list))
        .route("/settings/web-cache/clear", post(settings_ui::web_cache_clear))
        .route(
            "/settings/web-cache/remove-domain",
            post(settings_ui::web_cache_remove_domain),
        )
        .route(
            "/settings/web-cache/remove-entry",
            post(settings_ui::web_cache_remove_entry),
        )
        .route("/settings/dnd/focus-modes", get(settings_ui::get_dnd_focus_modes))
        .route(
            "/settings/dnd/config",
            get(settings_ui::get_dnd_config).post(settings_ui::set_dnd_config),
        )
        .route("/settings/dnd/active", get(settings_ui::get_dnd_active))
        .route("/settings/dnd/status", get(settings_ui::get_dnd_status))
        .route("/settings/dnd/test", post(settings_ui::test_dnd))
        .route(
            "/settings/dnd/open-full-disk-access",
            post(settings_ui::open_full_disk_access),
        )
        .route(
            "/settings/screenshot/config",
            get(settings_screenshots::get_screenshot_config).post(settings_screenshots::set_screenshot_config),
        )
        .route(
            "/settings/screenshot/estimate-reembed",
            get(settings_screenshots::estimate_screenshot_reembed),
        )
        .route(
            "/settings/screenshot/rebuild-embeddings",
            post(settings_screenshots::rebuild_screenshot_embeddings),
        )
        .route(
            "/settings/screenshot/around",
            post(settings_screenshots::get_screenshots_around),
        )
        .route(
            "/settings/screenshot/search-image",
            post(settings_screenshots::search_screenshots_by_image),
        )
        .route(
            "/settings/screenshot/metrics",
            get(settings_screenshots::get_screenshot_metrics),
        )
        .route(
            "/settings/screenshot/ocr-ready",
            get(settings_screenshots::check_ocr_models_ready),
        )
        .route(
            "/settings/screenshot/download-ocr",
            post(settings_screenshots::download_ocr_models),
        )
        .route(
            "/settings/screenshot/search-text",
            post(settings_screenshots::search_screenshots_by_text),
        )
        .route(
            "/settings/screenshot/dir",
            get(settings_screenshots::get_screenshots_dir),
        )
        .route(
            "/settings/screenshot/search-vector",
            post(settings_screenshots::search_screenshots_by_vector),
        )
        .route(
            "/ui/accent-color",
            get(settings_ui::get_accent_color).post(settings_ui::set_accent_color),
        )
        .route(
            "/ui/daily-goal",
            get(settings_ui::get_daily_goal).post(settings_ui::set_daily_goal),
        )
        .route(
            "/ui/goal-notified-date",
            get(settings_ui::get_goal_notified_date).post(settings_ui::set_goal_notified_date),
        )
        .route(
            "/ui/main-window-auto-fit",
            get(settings_ui::get_main_window_auto_fit).post(settings_ui::set_main_window_auto_fit),
        )
        .route(
            "/skills/refresh-interval",
            get(settings_ui::get_skills_refresh_interval).post(settings_ui::set_skills_refresh_interval),
        )
        .route(
            "/skills/sync-on-launch",
            get(settings_ui::get_skills_sync_on_launch).post(settings_ui::set_skills_sync_on_launch),
        )
        .route("/skills/last-sync", get(settings_ui::get_skills_last_sync))
        .route("/skills/sync-now", post(settings_ui::sync_skills_now))
        .route("/skills/list", get(settings_ui::list_skills))
        .route("/skills/license", get(settings_ui::get_skills_license))
        .route(
            "/skills/disabled",
            get(settings_ui::get_disabled_skills).post(settings_ui::set_disabled_skills),
        )
        .route("/device/serial-ports", get(settings_device::list_serial_ports))
        .route(
            "/calibration/profiles",
            get(super::settings_calibration::list_profiles).post(super::settings_calibration::create_profile),
        )
        .route(
            "/calibration/profiles/update",
            axum::routing::put(super::settings_calibration::update_profile),
        )
        .route(
            "/calibration/profiles/delete",
            axum::routing::post(super::settings_calibration::delete_profile),
        )
        .route(
            "/calibration/active",
            get(super::settings_calibration::get_active_profile_id)
                .put(super::settings_calibration::set_active_profile),
        )
        .route(
            "/calibration/auto-start-pending",
            get(super::settings_calibration::auto_start_pending),
        )
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
        // OpenAI-compatible alias — SDKs send to /v1/chat/completions
        .route("/chat/completions", post(llm_chat_completions))
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

async fn llm_chat_completions(state: State<AppState>, req: Json<ChatCompletionsRequest>) -> axum::response::Response {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::settings_device::*;
    use crate::routes::settings_lsl::{LslAutoConnectRequest, LslIdleTimeoutRequest, LslPairRequest, LslUnpairRequest};
    use crate::routes::settings_ui::*;
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
