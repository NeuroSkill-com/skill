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
    activity_store::{
        ActiveWindowRow, ActivityStore, BuildEventRow, CoEditRow, DailySummaryRow, EditChunkRow, FileInteractionRow,
        FileUsageRow, FocusSessionRow, HourlyEditRow, InputActivityRow, InputBucketRow, LanguageBreakdownRow,
        ProjectUsageRow,
    },
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
pub(crate) struct ActivityFilesRequest {
    pub(crate) limit: Option<u32>,
    pub(crate) since: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditChunksRequest {
    /// If set, return chunks for this specific interaction.
    pub(crate) interaction_id: Option<i64>,
    /// If set (with to_ts), return chunks in a time range.
    pub(crate) from_ts: Option<u64>,
    pub(crate) to_ts: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CoEditRequest {
    pub(crate) window_secs: Option<u64>,
    pub(crate) limit: Option<u32>,
    pub(crate) since: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DaySummaryRequest {
    pub(crate) day_start: u64,
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
pub(crate) struct HfSearchParams {
    pub(crate) q: String,
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HfFilesParams {
    pub(crate) repo: String,
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
        .route("/activity/recent-files", post(activity_recent_files))
        .route("/activity/top-files", post(activity_top_files))
        .route("/activity/top-projects", post(activity_top_projects))
        .route("/activity/edit-chunks", post(activity_edit_chunks))
        .route("/activity/language-breakdown", post(activity_language_breakdown))
        .route("/activity/context-switch-rate", post(activity_context_switch_rate))
        .route("/activity/coedited-files", post(activity_coedited_files))
        .route("/activity/daily-summary", post(activity_daily_summary))
        .route("/activity/hourly-heatmap", post(activity_hourly_heatmap))
        .route("/activity/focus-sessions", post(activity_focus_sessions))
        .route("/activity/forgotten-files", post(activity_forgotten_files))
        .route("/activity/recent-builds", get(activity_recent_builds))
        .route("/activity/productivity-score", post(activity_productivity_score))
        .route("/activity/weekly-digest", post(activity_weekly_digest))
        .route("/activity/stale-files", post(activity_stale_files))
        .route("/activity/vscode-events", post(activity_vscode_events))
        .route("/activity/shell-command", post(activity_shell_command))
        .route("/activity/shell-hook", get(get_shell_hook))
        .route("/activity/install-shell-hook", post(install_shell_hook))
        .route("/activity/uninstall-shell-hook", post(uninstall_shell_hook))
        .route("/activity/shell-hook-status", post(shell_hook_status))
        .route("/activity/files-in-range", post(activity_files_in_range))
        .route("/activity/meetings-in-range", post(activity_meetings_in_range))
        .route("/activity/recent-clipboard", post(activity_recent_clipboard))
        .route(
            "/activity/file-patterns",
            get(get_file_patterns).post(set_file_patterns),
        )
        .route(
            "/activity/tracking/active-window",
            get(get_active_window_tracking).post(set_active_window_tracking),
        )
        .route(
            "/activity/tracking/input",
            get(get_input_activity_tracking).post(set_input_activity_tracking),
        )
        .route(
            "/activity/tracking/files",
            get(get_file_activity_tracking).post(set_file_activity_tracking),
        )
        .route(
            "/activity/tracking/clipboard",
            get(get_clipboard_tracking).post(set_clipboard_tracking),
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
            "/settings/reembed-config",
            get(get_reembed_config).post(set_reembed_config),
        )
        .route(
            "/settings/daemon-watchdog",
            get(get_daemon_watchdog).post(set_daemon_watchdog),
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
        .route("/settings/umap-backends", get(settings_ui::get_umap_backends))
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
            axum::routing::put(super::settings_calibration::update_profile)
                .post(super::settings_calibration::update_profile),
        )
        .route(
            "/calibration/profiles/delete",
            axum::routing::post(super::settings_calibration::delete_profile),
        )
        .route(
            "/calibration/active",
            get(super::settings_calibration::get_active_profile_id)
                .put(super::settings_calibration::set_active_profile)
                .post(super::settings_calibration::set_active_profile),
        )
        .route(
            "/calibration/auto-start-pending",
            get(super::settings_calibration::auto_start_pending),
        )
        .route(
            "/calibration/session/start",
            post(super::settings_calibration::start_session),
        )
        .route(
            "/calibration/session/cancel",
            post(super::settings_calibration::cancel_session),
        )
        .route(
            "/calibration/session/status",
            get(super::settings_calibration::session_status),
        )
        .route(
            "/calibration/record-completed",
            post(super::settings_calibration::record_completed),
        )
        .route(
            "/calibration/active-profile",
            get(super::settings_calibration::get_active_profile),
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
        .route("/models/trigger-reembed/stream", post(trigger_reembed_stream))
        .route("/models/trigger-weights-download", post(trigger_weights_download))
        .route("/models/cancel-weights-download", post(cancel_weights_download))
        .route("/models/estimate-reembed", get(estimate_reembed))
        .route("/models/rebuild-index", post(rebuild_index))
        .route("/models/exg-catalog", get(get_exg_catalog))
        .route(
            "/models/text-embedding",
            get(get_text_embedding_model).post(set_text_embedding_model),
        )
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
        .route("/llm/catalog/search", get(llm_search_hf))
        .route("/llm/catalog/search/files", get(llm_search_hf_files))
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

async fn get_reembed_config(state: State<AppState>) -> Json<skill_settings::ReembedConfig> {
    Json(load_user_settings(&state).reembed)
}

async fn set_reembed_config(
    state: State<AppState>,
    Json(config): Json<skill_settings::ReembedConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.reembed = config;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct DaemonWatchdogConfig {
    enabled: bool,
    timeout_secs: u64,
}

async fn get_daemon_watchdog(state: State<AppState>) -> Json<DaemonWatchdogConfig> {
    let s = load_user_settings(&state);
    Json(DaemonWatchdogConfig {
        enabled: s.daemon_auto_restart,
        timeout_secs: s.daemon_restart_timeout_secs,
    })
}

async fn set_daemon_watchdog(
    state: State<AppState>,
    Json(config): Json<DaemonWatchdogConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.daemon_auto_restart = config.enabled;
    settings.daemon_restart_timeout_secs = config.timeout_secs;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({ "ok": true }))
}

async fn trigger_reembed(state: State<AppState>) -> Json<serde_json::Value> {
    settings_exg::trigger_reembed_impl(state).await
}

async fn trigger_reembed_stream(
    State(state): State<AppState>,
) -> axum::response::sse::Sse<
    impl futures::stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
> {
    use axum::response::sse::Event;

    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let events_tx = state.events_tx.clone();
    let label_index = state.label_index.clone();
    let model_status = state.exg_model_status.clone();
    let cancel = state.idle_reembed_cancel.clone();

    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(64);

    // Subscribe to the broadcast channel BEFORE spawning the reembed task
    // so we don't miss the first events.
    let mut broadcast_rx = events_tx.subscribe();
    let tx_fwd = tx.clone();

    // Forward reembed-progress broadcast events → SSE channel.
    tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok(envelope) => {
                    if envelope.r#type != "reembed-progress" {
                        continue;
                    }
                    let json = serde_json::to_string(&envelope.payload).unwrap_or_default();
                    let event = Event::default().data(json);
                    if tx_fwd.send(event).await.is_err() {
                        break; // client disconnected
                    }
                    // Stop forwarding once done/error.
                    let status = envelope.payload.get("status").and_then(|v| v.as_str()).unwrap_or("");
                    if status == "done" || status == "error" || status == "idle_done" {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    });

    // Spawn the actual reembed work.
    tokio::task::spawn_blocking(move || {
        // Guard: reject if weights download is in progress.
        {
            let st = model_status.lock().unwrap_or_else(|e| e.into_inner());
            if st.downloading_weights {
                let _ = events_tx.send(skill_daemon_common::EventEnvelope {
                    r#type: "reembed-progress".into(),
                    ts_unix_ms: settings_exg::now_unix_ms(),
                    correlation_id: None,
                    payload: serde_json::json!({ "status": "error", "message": "weights download in progress" }),
                });
                return;
            }
        }

        cancel.store(false, std::sync::atomic::Ordering::Relaxed);

        if let Err(e) = settings_exg::run_batch_reembed_with_cancel(&skill_dir, &events_tx, &cancel, true, 10, 50) {
            tracing::error!("batch reembed failed: {e}");
            let _ = events_tx.send(skill_daemon_common::EventEnvelope {
                r#type: "reembed-progress".into(),
                ts_unix_ms: settings_exg::now_unix_ms(),
                correlation_id: None,
                payload: serde_json::json!({ "status": "error", "message": e.to_string() }),
            });
        }
        let stats = skill_label_index::rebuild(&skill_dir, &label_index);
        tracing::info!(
            "[reembed] label index rebuilt: {} text, {} eeg ({} skipped)",
            stats.text_nodes,
            stats.eeg_nodes,
            stats.eeg_skipped
        );
    });

    let stream = futures::stream::StreamExt::map(tokio_stream::wrappers::ReceiverStream::new(rx), Ok);
    axum::response::sse::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)))
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

async fn get_text_embedding_model(State(state): State<AppState>) -> Json<serde_json::Value> {
    let code = state.text_embedder.model_code();
    Json(serde_json::json!({ "model": code }))
}

async fn set_text_embedding_model(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let Some(code) = body.get("model").or(body.get("modelCode")).and_then(|v| v.as_str()) else {
        return Json(serde_json::json!({ "ok": false, "error": "missing 'model' field" }));
    };
    let code = code.to_string();
    let embedder = state.text_embedder.clone();
    let state_clone = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        embedder.set_model_code(&code);
        let ok = embedder.reload();
        if ok {
            let mut settings = load_user_settings(&state_clone);
            settings.text_embedding_model = code.clone();
            save_user_settings(&state_clone, &settings);
        }
        (ok, code)
    })
    .await;

    match result {
        Ok((true, code)) => Json(serde_json::json!({ "ok": true, "model": code })),
        Ok((false, code)) => {
            Json(serde_json::json!({ "ok": false, "error": format!("failed to load model '{code}'") }))
        }
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
    }
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

async fn activity_recent_files(
    state: State<AppState>,
    req: Json<ActivityFilesRequest>,
) -> Json<Vec<FileInteractionRow>> {
    settings_hooks_activity::activity_recent_files_impl(state, req).await
}

async fn activity_top_files(state: State<AppState>, req: Json<ActivityFilesRequest>) -> Json<Vec<FileUsageRow>> {
    settings_hooks_activity::activity_top_files_impl(state, req).await
}

async fn activity_top_projects(state: State<AppState>, req: Json<ActivityFilesRequest>) -> Json<Vec<ProjectUsageRow>> {
    settings_hooks_activity::activity_top_projects_impl(state, req).await
}

async fn activity_edit_chunks(state: State<AppState>, req: Json<EditChunksRequest>) -> Json<Vec<EditChunkRow>> {
    settings_hooks_activity::activity_edit_chunks_impl(state, req).await
}

async fn activity_language_breakdown(
    state: State<AppState>,
    req: Json<ActivityFilesRequest>,
) -> Json<Vec<LanguageBreakdownRow>> {
    settings_hooks_activity::activity_language_breakdown_impl(state, req).await
}

async fn activity_context_switch_rate(
    state: State<AppState>,
    req: Json<ActivityBucketsRequest>,
) -> Json<serde_json::Value> {
    settings_hooks_activity::activity_context_switch_rate_impl(state, req).await
}

async fn activity_coedited_files(state: State<AppState>, req: Json<CoEditRequest>) -> Json<Vec<CoEditRow>> {
    settings_hooks_activity::activity_coedited_files_impl(state, req).await
}

async fn activity_daily_summary(state: State<AppState>, req: Json<DaySummaryRequest>) -> Json<DailySummaryRow> {
    settings_hooks_activity::activity_daily_summary_impl(state, req).await
}

async fn activity_hourly_heatmap(state: State<AppState>, req: Json<ActivityFilesRequest>) -> Json<Vec<HourlyEditRow>> {
    settings_hooks_activity::activity_hourly_heatmap_impl(state, req).await
}

async fn activity_focus_sessions(
    state: State<AppState>,
    req: Json<ActivityFilesRequest>,
) -> Json<Vec<FocusSessionRow>> {
    settings_hooks_activity::activity_focus_sessions_impl(state, req).await
}

async fn activity_forgotten_files(state: State<AppState>, req: Json<ActivityFilesRequest>) -> Json<Vec<String>> {
    settings_hooks_activity::activity_forgotten_files_impl(state, req).await
}

async fn activity_recent_builds(state: State<AppState>) -> Json<Vec<BuildEventRow>> {
    settings_hooks_activity::activity_recent_builds_impl(state).await
}

async fn activity_productivity_score(
    state: State<AppState>,
    req: Json<DaySummaryRequest>,
) -> Json<skill_data::activity_store::ProductivityScore> {
    settings_hooks_activity::activity_productivity_score_impl(state, req).await
}

async fn activity_weekly_digest(
    state: State<AppState>,
    req: Json<DaySummaryRequest>,
) -> Json<skill_data::activity_store::WeeklyDigest> {
    settings_hooks_activity::activity_weekly_digest_impl(state, req).await
}

async fn activity_stale_files(
    state: State<AppState>,
    req: Json<ActivityFilesRequest>,
) -> Json<Vec<skill_data::activity_store::StaleFileRow>> {
    settings_hooks_activity::activity_stale_files_impl(state, req).await
}

async fn activity_vscode_events(
    state: State<AppState>,
    Json(events): Json<Vec<serde_json::Value>>,
) -> Json<serde_json::Value> {
    settings_hooks_activity::activity_vscode_events_impl(state, events).await
}

/// Receive a single shell command from the OS-wide shell hook (preexec).
/// Expects JSON: {"command":"...", "cwd":"...", "shell":"zsh", "exit_code": null}
async fn activity_shell_command(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let embedder = state.text_embedder.clone();
    let label_index = state.label_index.clone();
    let result = tokio::task::spawn_blocking(move || {
        let Some(store) = ActivityStore::open(&skill_dir) else { return 0u64; };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let command = body.get("command").and_then(|v| v.as_str()).unwrap_or("");
        let cwd = body.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
        let shell = body.get("shell").and_then(|v| v.as_str()).unwrap_or("shell");
        let exit_code = body.get("exit_code").and_then(|v| v.as_i64());
        if command.is_empty() { return 0; }

        if exit_code.is_none() {
            // Command start
            store.insert_terminal_command_start(shell, command, cwd, now, None, None);
        } else {
            // Command end (with exit code)
            store.update_terminal_command_end(command, shell, exit_code, now, None);
        }

        // Auto-label for EEG
        let labels_db = skill_dir.join(skill_constants::LABELS_FILE);
        if let Ok(conn) = rusqlite::Connection::open(&labels_db) {
            skill_data::util::init_wal_pragmas(&conn);
            let label = if exit_code.is_none() {
                let short = if command.len() > 40 { &command[..40] } else { command };
                format!("running: {short}")
            } else {
                let status = match exit_code {
                    Some(0) => "passed".to_string(),
                    Some(c) => format!("failed (exit {c})"),
                    None => "ended".to_string(),
                };
                let short = if command.len() > 30 { &command[..30] } else { command };
                format!("{short} {status}")
            };
            let text_emb = embedder.embed(&label);
            let text_blob = text_emb.as_ref().map(|v| skill_data::util::f32_to_blob(v));
            let model_name = if text_blob.is_some() { Some("nomic-embed-text-v1.5") } else { None };
            let label_id = conn.execute(
                "INSERT INTO labels (text, context, eeg_start, eeg_end, wall_start, wall_end, created_at, text_embedding, embedding_model)
                 VALUES (?1, ?2, ?3, ?3, ?3, ?3, ?3, ?4, ?5)",
                rusqlite::params![label, cwd, now as i64, text_blob, model_name],
            ).ok().map(|_| conn.last_insert_rowid());
            if let (Some(id), Some(ref te)) = (label_id, &text_emb) {
                skill_label_index::insert_label(&skill_dir, id, te, &[], now, now, &label_index);
            }
        }
        1u64
    })
    .await
    .unwrap_or(0);
    Json(serde_json::json!({"ok": true, "processed": result}))
}

/// Return the shell hook script for the requested shell.
/// GET /v1/activity/shell-hook?shell=zsh
async fn get_shell_hook(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    State(state): State<AppState>,
) -> axum::response::Response {
    let shell = params.get("shell").map(|s| s.as_str()).unwrap_or("zsh");
    let hook = generate_shell_hook(shell);
    axum::response::Response::builder()
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(axum::body::Body::from(hook))
        .unwrap_or_default()
}

/// Install the shell hook into the user's shell rc file.
/// POST /v1/activity/install-shell-hook  {"shell": "zsh"}
async fn install_shell_hook(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let shell: String = body.get("shell").and_then(|v| v.as_str()).unwrap_or("zsh").to_string();
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let result = tokio::task::spawn_blocking(move || {
        let shell = shell.as_str();
        let hook_dir = skill_dir.join("shell-hooks");
        std::fs::create_dir_all(&hook_dir).ok();

        // Determine file extension and rc file path
        let (ext, rc_paths) = match shell {
            "zsh" => ("zsh", vec![dirs::home_dir().map(|h| h.join(".zshrc"))]),
            "bash" => (
                "bash",
                vec![
                    dirs::home_dir().map(|h| h.join(".bashrc")),
                    dirs::home_dir().map(|h| h.join(".bash_profile")),
                ],
            ),
            "fish" => (
                "fish",
                vec![dirs::config_dir().map(|c| c.join("fish").join("config.fish"))],
            ),
            "powershell" | "pwsh" => ("psm1", vec![]), // PowerShell uses Import-Module
            _ => return serde_json::json!({"ok": false, "error": format!("unsupported shell: {shell}")}),
        };

        // Write hook script to skill config dir
        let hook_content = generate_shell_hook(shell);
        let hook_file = hook_dir.join(format!("neuroskill.{ext}"));
        if let Err(e) = std::fs::write(&hook_file, &hook_content) {
            return serde_json::json!({"ok": false, "error": format!("failed to write hook: {e}")});
        }

        // For PowerShell, write the module and return instructions
        if shell == "powershell" || shell == "pwsh" {
            let profile_hint = if cfg!(windows) {
                "$PROFILE (e.g. Documents\\PowerShell\\Microsoft.PowerShell_profile.ps1)"
            } else {
                "$PROFILE (e.g. ~/.config/powershell/Microsoft.PowerShell_profile.ps1)"
            };
            return serde_json::json!({
                "ok": true,
                "hook_path": hook_file.to_string_lossy(),
                "instructions": format!("Add to {profile_hint}:\n  Import-Module '{}'", hook_file.to_string_lossy()),
            });
        }

        // For POSIX shells, add source line to rc file
        let hook_path_str = hook_file.to_string_lossy().to_string();
        let source_line = if shell == "fish" {
            format!("source '{hook_path_str}'")
        } else {
            format!("[ -f '{hook_path_str}' ] && source '{hook_path_str}'")
        };
        let marker = "# neuroskill shell hook";

        for rc_opt in &rc_paths {
            let Some(rc) = rc_opt else { continue };
            // Check if already installed
            if let Ok(content) = std::fs::read_to_string(rc) {
                if content.contains("neuroskill") {
                    return serde_json::json!({
                        "ok": true,
                        "already_installed": true,
                        "rc_file": rc.to_string_lossy(),
                        "hook_path": hook_path_str,
                    });
                }
            }
            // Append to rc file
            let line = format!("\n{marker}\n{source_line}\n");
            if let Err(e) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(rc)
                .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()))
            {
                return serde_json::json!({"ok": false, "error": format!("failed to update {}: {e}", rc.display())});
            }
            return serde_json::json!({
                "ok": true,
                "installed": true,
                "rc_file": rc.to_string_lossy(),
                "hook_path": hook_path_str,
                "note": "Open a new terminal for the hook to take effect.",
            });
        }

        serde_json::json!({"ok": false, "error": "could not find rc file to install into"})
    })
    .await
    .unwrap_or_else(|e| serde_json::json!({"ok": false, "error": format!("{e}")}));

    Json(result)
}

/// Check the installation status of a shell hook.
async fn shell_hook_status(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let shell: String = body.get("shell").and_then(|v| v.as_str()).unwrap_or("zsh").to_string();
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let result = tokio::task::spawn_blocking(move || {
        let shell = shell.as_str();
        let hook_dir = skill_dir.join("shell-hooks");
        let (ext, rc_path) = shell_rc_info(shell);
        let hook_file = hook_dir.join(format!("neuroskill.{ext}"));

        let hook_exists = hook_file.exists();
        let rc_has_line = rc_path.as_ref().map_or(false, |p| {
            std::fs::read_to_string(p).map_or(false, |c| c.contains("neuroskill"))
        });
        let available = match shell {
            "zsh" | "bash" => true,
            "fish" => dirs::config_dir().map_or(false, |_| true),
            "powershell" | "pwsh" => true,
            _ => false,
        };

        serde_json::json!({
            "shell": shell,
            "installed": hook_exists && rc_has_line,
            "hook_exists": hook_exists,
            "rc_has_line": rc_has_line,
            "hook_path": hook_file.to_string_lossy(),
            "rc_file": rc_path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            "available": available,
        })
    })
    .await
    .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
    Json(result)
}

/// Remove the shell hook from the user's rc file and delete the hook script.
async fn uninstall_shell_hook(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let shell: String = body.get("shell").and_then(|v| v.as_str()).unwrap_or("zsh").to_string();
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let result = tokio::task::spawn_blocking(move || {
        let shell = shell.as_str();
        let hook_dir = skill_dir.join("shell-hooks");
        let (ext, rc_path) = shell_rc_info(shell);
        let hook_file = hook_dir.join(format!("neuroskill.{ext}"));

        // Delete the hook script
        let _ = std::fs::remove_file(&hook_file);

        // Remove the source line from the rc file
        if let Some(ref rc) = rc_path {
            if let Ok(content) = std::fs::read_to_string(rc) {
                let cleaned: String = content
                    .lines()
                    .filter(|line| !line.contains("neuroskill"))
                    .collect::<Vec<_>>()
                    .join("\n");
                // Preserve trailing newline
                let cleaned = if content.ends_with('\n') && !cleaned.ends_with('\n') {
                    cleaned + "\n"
                } else {
                    cleaned
                };
                if let Err(e) = std::fs::write(rc, &cleaned) {
                    return serde_json::json!({"ok": false, "error": format!("failed to update {}: {e}", rc.display())});
                }
            }
        }

        serde_json::json!({
            "ok": true,
            "removed": true,
            "rc_file": rc_path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        })
    })
    .await
    .unwrap_or_else(|e| serde_json::json!({"ok": false, "error": e.to_string()}));
    Json(result)
}

/// Get the rc file path for a given shell.
fn shell_rc_info(shell: &str) -> (&'static str, Option<std::path::PathBuf>) {
    match shell {
        "zsh" => ("zsh", dirs::home_dir().map(|h| h.join(".zshrc"))),
        "bash" => ("bash", dirs::home_dir().map(|h| h.join(".bashrc"))),
        "fish" => ("fish", dirs::config_dir().map(|c| c.join("fish").join("config.fish"))),
        "powershell" | "pwsh" => ("psm1", None),
        _ => ("sh", None),
    }
}

/// Generate the hook script content for a given shell. Self-contained — no external file deps.
fn generate_shell_hook(shell: &str) -> String {
    let port = 18444;
    match shell {
        "zsh" => format!(
            r#"# NeuroSkill terminal tracking hook (zsh)
_NEUROSKILL_PORT="${{NEUROSKILL_DAEMON_PORT:-{port}}}"
_NEUROSKILL_HOST="${{NEUROSKILL_DAEMON_HOST:-127.0.0.1}}"
_neuroskill_read_token() {{
  local f
  case "$(uname -s)" in
    Darwin) f="$HOME/Library/Application Support/skill/daemon/auth.token" ;;
    *)      f="${{XDG_CONFIG_HOME:-$HOME/.config}}/skill/daemon/auth.token" ;;
  esac
  [[ -f "$f" ]] && cat "$f" 2>/dev/null
}}
_NEUROSKILL_TOKEN="$(_neuroskill_read_token)"
_neuroskill_escape() {{ local s="$1"; s="${{s//\\/\\\\}}"; s="${{s//\"/\\\"}}"; s="${{s//$'\n'/\\n}}"; printf '%s' "$s"; }}
_neuroskill_post() {{
  local auth=(); [[ -n "$_NEUROSKILL_TOKEN" ]] && auth=(-H "Authorization: Bearer $_NEUROSKILL_TOKEN")
  curl -sf -X POST "http://${{_NEUROSKILL_HOST}}:${{_NEUROSKILL_PORT}}/v1/activity/shell-command" \
    -H "Content-Type: application/json" "${{auth[@]}}" -d "$1" --connect-timeout 1 -m 2 >/dev/null 2>&1 &!
}}
_neuroskill_preexec() {{
  _NEUROSKILL_LAST_CMD="$1"
  _neuroskill_post "{{\"command\":\"$(_neuroskill_escape "$1")\",\"cwd\":\"$(_neuroskill_escape "$PWD")\",\"shell\":\"zsh\"}}"
}}
_neuroskill_precmd() {{
  local ec=$?
  [[ -n "$_NEUROSKILL_LAST_CMD" ]] && _neuroskill_post "{{\"command\":\"$(_neuroskill_escape "$_NEUROSKILL_LAST_CMD")\",\"cwd\":\"$(_neuroskill_escape "$PWD")\",\"shell\":\"zsh\",\"exit_code\":$ec}}"
  _NEUROSKILL_LAST_CMD=""
}}
autoload -Uz add-zsh-hook
add-zsh-hook preexec _neuroskill_preexec
add-zsh-hook precmd _neuroskill_precmd
"#
        ),

        "bash" => format!(
            r#"# NeuroSkill terminal tracking hook (bash)
_NEUROSKILL_PORT="${{NEUROSKILL_DAEMON_PORT:-{port}}}"
_NEUROSKILL_HOST="${{NEUROSKILL_DAEMON_HOST:-127.0.0.1}}"
_neuroskill_read_token() {{
  local f
  case "$(uname -s)" in
    Darwin) f="$HOME/Library/Application Support/skill/daemon/auth.token" ;;
    MINGW*|MSYS*|CYGWIN*) f="$APPDATA/skill/daemon/auth.token" ;;
    *)      f="${{XDG_CONFIG_HOME:-$HOME/.config}}/skill/daemon/auth.token" ;;
  esac
  [[ -f "$f" ]] && cat "$f" 2>/dev/null
}}
_NEUROSKILL_TOKEN="$(_neuroskill_read_token)"
_NEUROSKILL_LAST_CMD=""
_neuroskill_escape() {{ local s="$1"; s="${{s//\\/\\\\}}"; s="${{s//\"/\\\"}}"; s="${{s//$'\n'/\\n}}"; printf '%s' "$s"; }}
_neuroskill_post() {{
  local auth_header=""
  [[ -n "$_NEUROSKILL_TOKEN" ]] && auth_header="-H 'Authorization: Bearer $_NEUROSKILL_TOKEN'"
  eval curl -sf -X POST "http://${{_NEUROSKILL_HOST}}:${{_NEUROSKILL_PORT}}/v1/activity/shell-command" \
    -H "'Content-Type: application/json'" $auth_header -d "'$1'" --connect-timeout 1 -m 2 '>/dev/null' '2>&1' '&'
}}
_neuroskill_debug_trap() {{
  [[ -n "$COMP_LINE" ]] && return
  [[ "$BASH_COMMAND" == "$PROMPT_COMMAND" ]] && return
  [[ "$BASH_COMMAND" == _neuroskill_* ]] && return
  _NEUROSKILL_LAST_CMD="$BASH_COMMAND"
  _neuroskill_post "{{\"command\":\"$(_neuroskill_escape "$BASH_COMMAND")\",\"cwd\":\"$(_neuroskill_escape "$PWD")\",\"shell\":\"bash\"}}"
}}
_neuroskill_prompt_cmd() {{
  local ec=$?
  [[ -n "$_NEUROSKILL_LAST_CMD" ]] && _neuroskill_post "{{\"command\":\"$(_neuroskill_escape "$_NEUROSKILL_LAST_CMD")\",\"cwd\":\"$(_neuroskill_escape "$PWD")\",\"shell\":\"bash\",\"exit_code\":$ec}}"
  _NEUROSKILL_LAST_CMD=""
}}
trap '_neuroskill_debug_trap' DEBUG
PROMPT_COMMAND="_neuroskill_prompt_cmd${{PROMPT_COMMAND:+;$PROMPT_COMMAND}}"
"#
        ),

        "fish" => format!(
            r#"# NeuroSkill terminal tracking hook (fish)
set -gx _NEUROSKILL_PORT (test -n "$NEUROSKILL_DAEMON_PORT"; and echo $NEUROSKILL_DAEMON_PORT; or echo {port})
set -gx _NEUROSKILL_HOST (test -n "$NEUROSKILL_DAEMON_HOST"; and echo $NEUROSKILL_DAEMON_HOST; or echo 127.0.0.1)
function _neuroskill_read_token
  switch (uname -s)
    case Darwin; set -l f "$HOME/Library/Application Support/skill/daemon/auth.token"
    case '*';    set -l f (test -n "$XDG_CONFIG_HOME"; and echo "$XDG_CONFIG_HOME"; or echo "$HOME/.config")"/skill/daemon/auth.token"
  end
  test -f "$f"; and cat "$f" 2>/dev/null
end
set -g _NEUROSKILL_TOKEN (_neuroskill_read_token)
function _neuroskill_escape; string replace -a '\\' '\\\\' -- $argv[1] | string replace -a '"' '\\"' | string replace -a \n '\\n'; end
function _neuroskill_post
  set -l auth; test -n "$_NEUROSKILL_TOKEN"; and set auth -H "Authorization: Bearer $_NEUROSKILL_TOKEN"
  curl -sf -X POST "http://$_NEUROSKILL_HOST:$_NEUROSKILL_PORT/v1/activity/shell-command" \
    -H "Content-Type: application/json" $auth -d "$argv[1]" --connect-timeout 1 -m 2 >/dev/null 2>&1 &; disown 2>/dev/null
end
function _neuroskill_preexec --on-event fish_preexec
  set -g _NEUROSKILL_LAST_CMD $argv[1]
  _neuroskill_post '{{"command":"'(_neuroskill_escape "$argv[1]")'","cwd":"'(_neuroskill_escape "$PWD")'","shell":"fish"}}'
end
function _neuroskill_postexec --on-event fish_postexec
  set -l ec $status
  if test -n "$_NEUROSKILL_LAST_CMD"
    _neuroskill_post '{{"command":"'(_neuroskill_escape "$_NEUROSKILL_LAST_CMD")'","cwd":"'(_neuroskill_escape "$PWD")'","shell":"fish","exit_code":'$ec'}}'
    set -e _NEUROSKILL_LAST_CMD
  end
end
"#
        ),

        "powershell" | "pwsh" => format!(
            r#"# NeuroSkill terminal tracking hook (PowerShell)
$script:NsPort = if ($env:NEUROSKILL_DAEMON_PORT) {{ $env:NEUROSKILL_DAEMON_PORT }} else {{ "{port}" }}
$script:NsHost = if ($env:NEUROSKILL_DAEMON_HOST) {{ $env:NEUROSKILL_DAEMON_HOST }} else {{ "127.0.0.1" }}
$script:NsUrl = "http://$($script:NsHost):$($script:NsPort)/v1/activity/shell-command"
$script:NsLastCmd = ""
function Get-NsToken {{
  $f = if ($IsWindows -or $env:OS -match "Windows") {{ Join-Path $env:APPDATA "skill\daemon\auth.token" }}
       elseif ($IsMacOS) {{ Join-Path $HOME "Library/Application Support/skill/daemon/auth.token" }}
       else {{ $xdg = if ($env:XDG_CONFIG_HOME) {{ $env:XDG_CONFIG_HOME }} else {{ Join-Path $HOME ".config" }}; Join-Path $xdg "skill/daemon/auth.token" }}
  if (Test-Path $f) {{ (Get-Content $f -Raw).Trim() }} else {{ $null }}
}}
$script:NsToken = Get-NsToken
function Send-NsCommand {{ param([string]$Command,[string]$Cwd,[object]$ExitCode=$null)
  if (-not $Command) {{ return }}
  $body = @{{ command=$Command; cwd=$Cwd; shell="powershell"; exit_code=$ExitCode }} | ConvertTo-Json -Compress
  $headers = @{{ "Content-Type"="application/json" }}
  if ($script:NsToken) {{ $headers["Authorization"] = "Bearer $($script:NsToken)" }}
  try {{ $null = Start-Job -ScriptBlock {{ param($u,$b,$h); try {{ Invoke-RestMethod -Uri $u -Method Post -Body $b -Headers $h -TimeoutSec 2 -EA SilentlyContinue | Out-Null }} catch {{}} }} -ArgumentList $script:NsUrl,$body,$headers }} catch {{}}
}}
$script:OriginalPrompt = $function:prompt
function prompt {{
  $lastEc = $LASTEXITCODE; $ok = $?
  $hist = Get-History -Count 1 -EA SilentlyContinue
  if ($hist -and $hist.CommandLine -ne $script:NsLastCmd) {{
    $cmd = $hist.CommandLine; $cwd = (Get-Location).Path
    Send-NsCommand -Command $cmd -Cwd $cwd
    $ec = if ($ok) {{ 0 }} else {{ if ($lastEc) {{ $lastEc }} else {{ 1 }} }}
    Send-NsCommand -Command $cmd -Cwd $cwd -ExitCode $ec
    $script:NsLastCmd = $cmd
  }}
  if ($script:OriginalPrompt) {{ & $script:OriginalPrompt }} else {{ "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) " }}
}}
Register-EngineEvent -SourceIdentifier PowerShell.Exiting -Action {{ Get-Job | Where-Object {{ $_.State -eq 'Completed' }} | Remove-Job -Force -EA SilentlyContinue }} | Out-Null
"#
        ),

        _ => format!("# Unsupported shell: {shell}\n# Supported: zsh, bash, fish, powershell\n"),
    }
}

async fn activity_files_in_range(state: State<AppState>, req: Json<ActivityBucketsRequest>) -> Json<serde_json::Value> {
    settings_hooks_activity::activity_files_in_range_impl(state, req).await
}

async fn activity_meetings_in_range(
    state: State<AppState>,
    req: Json<ActivityBucketsRequest>,
) -> Json<Vec<skill_data::activity_store::MeetingEventRow>> {
    settings_hooks_activity::activity_meetings_in_range_impl(state, req).await
}

async fn activity_recent_clipboard(
    state: State<AppState>,
    req: Json<ActivityFilesRequest>,
) -> Json<Vec<skill_data::activity_store::ClipboardEventRow>> {
    settings_hooks_activity::activity_recent_clipboard_impl(state, req).await
}

async fn get_file_patterns(State(state): State<AppState>) -> Json<Vec<skill_settings::FilePatternRule>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let settings = skill_settings::load_settings(&skill_dir);
    let patterns = if settings.file_patterns.is_empty() {
        skill_settings::default_file_patterns()
    } else {
        settings.file_patterns
    };
    Json(patterns)
}

async fn set_file_patterns(
    State(state): State<AppState>,
    Json(patterns): Json<Vec<skill_settings::FilePatternRule>>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.file_patterns = patterns;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

async fn get_file_activity_tracking(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "value": state
            .track_file_activity
            .load(std::sync::atomic::Ordering::Relaxed)
    }))
}

async fn set_file_activity_tracking(
    State(state): State<AppState>,
    Json(req): Json<BoolValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.track_file_activity = req.value;
    save_user_settings(&state, &settings);
    state
        .track_file_activity
        .store(req.value, std::sync::atomic::Ordering::Relaxed);
    Json(serde_json::json!({"value": req.value}))
}

async fn get_clipboard_tracking(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"value": settings.track_clipboard}))
}

async fn set_clipboard_tracking(
    State(state): State<AppState>,
    Json(req): Json<BoolValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.track_clipboard = req.value;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"value": req.value}))
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
        ActivityStore::open_readonly(&skill_dir)
            .and_then(|store| store.get_recent_windows(1).into_iter().next())
            .map(|row| ActiveWindowInfo {
                app_name: row.app_name,
                app_path: row.app_path,
                window_title: row.window_title,
                document_path: None,
                activated_at: row.activated_at,
                browser_title: row.browser_title,
                monitor_id: row.monitor_id,
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
        let row =
            ActivityStore::open_readonly(&skill_dir).and_then(|store| store.get_recent_input(1).into_iter().next());
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

async fn llm_search_hf(state: State<AppState>, query: axum::extract::Query<HfSearchParams>) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_search_hf_impl(state, query).await
}

async fn llm_search_hf_files(
    state: State<AppState>,
    query: axum::extract::Query<HfFilesParams>,
) -> Json<serde_json::Value> {
    settings_llm_runtime::llm_search_hf_files_impl(state, query).await
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

        let Json(r1) = trigger_reembed(State(st.clone())).await;
        assert_eq!(r1["ok"], true);

        let Json(r2) = rebuild_index().await;
        assert_eq!(r2["ok"], true);

        let Json(est) = estimate_reembed(State(st)).await;
        assert!(est.get("total_epochs").is_some());
        assert!(est.get("missing").is_some());
        assert!(est.get("embedded").is_some());
        assert!(est.get("coverage_pct").is_some());
        assert!(est.get("per_day").is_some());
        assert!(est.get("idle_reembed").is_some());
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

    // ── Text embedding model tests ────────────────────────────────────────

    #[tokio::test]
    async fn get_text_embedding_model_returns_current() {
        let (_td, st) = mk_state();
        let Json(v) = get_text_embedding_model(State(st)).await;
        assert!(v.get("model").is_some());
        assert!(v["model"].as_str().unwrap().contains('/'));
    }

    #[tokio::test]
    async fn set_text_embedding_model_valid_persists() {
        let (td, st) = mk_state();
        // Set to bge-small
        let Json(_) = set_text_embedding_model(
            State(st.clone()),
            Json(serde_json::json!({ "model": "BAAI/bge-small-en-v1.5" })),
        )
        .await;
        // Model load may fail in test env (no weights), but the code should be set.
        let Json(cur) = get_text_embedding_model(State(st.clone())).await;
        assert_eq!(cur["model"], "BAAI/bge-small-en-v1.5");

        // Verify it was persisted to settings file.
        let settings = skill_settings::load_settings(td.path());
        assert_eq!(settings.text_embedding_model, "BAAI/bge-small-en-v1.5");
    }

    #[tokio::test]
    async fn set_text_embedding_model_unknown_rejected() {
        let (_td, st) = mk_state();
        let Json(v) = set_text_embedding_model(
            State(st),
            Json(serde_json::json!({ "model": "nonexistent/fake-model" })),
        )
        .await;
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("failed to load"));
    }

    #[tokio::test]
    async fn set_text_embedding_model_missing_field_rejected() {
        let (_td, st) = mk_state();
        let Json(v) = set_text_embedding_model(State(st), Json(serde_json::json!({}))).await;
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("missing"));
    }

    // ── Streaming reembed tests ────────────────────────────────────────────

    #[tokio::test]
    async fn trigger_reembed_stream_returns_sse() {
        use axum::body::Body;
        use tower::ServiceExt;

        let (_td, st) = mk_state();
        let app = router().with_state(st);

        let req = axum::http::Request::builder()
            .method(axum::http::Method::POST)
            .uri("/models/trigger-reembed/stream")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        assert_eq!(resp.headers().get("content-type").unwrap(), "text/event-stream",);
    }

    #[tokio::test]
    async fn trigger_reembed_stream_emits_events() {
        let (_td, st) = mk_state();

        // Subscribe to broadcast before triggering.
        let mut rx = st.events_tx.subscribe();

        // Trigger reembed (empty skill_dir = fast, emits loading_encoder → done).
        let Json(v) = trigger_reembed(State(st.clone())).await;
        assert_eq!(v["ok"], true);

        // Collect reembed-progress events with a timeout.
        let mut statuses = Vec::new();
        let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                match rx.recv().await {
                    Ok(env) if env.r#type == "reembed-progress" => {
                        let s = env
                            .payload
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let done = s == "done" || s == "error" || s == "idle_done";
                        statuses.push(s);
                        if done {
                            break;
                        }
                    }
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
        })
        .await;

        assert!(result.is_ok(), "reembed should complete within 30s");
        assert!(!statuses.is_empty(), "should receive at least one status event");
        assert!(
            statuses.contains(&"loading_encoder".to_string())
                || statuses.contains(&"done".to_string())
                || statuses.contains(&"error".to_string()),
            "should see loading_encoder or done/error, got: {statuses:?}"
        );
    }

    #[tokio::test]
    async fn trigger_reembed_non_stream_still_works() {
        let (_td, st) = mk_state();
        let Json(v) = trigger_reembed(State(st)).await;
        assert_eq!(v["ok"], true);
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
            (axum::http::Method::POST, "/models/trigger-reembed/stream"),
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
