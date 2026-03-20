// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Device, filter, EEG model, app-settings, autostart, and update-interval Tauri commands.

pub mod dnd_cmds;
pub mod hook_cmds;
pub mod device_cmds;
pub mod activity_cmds;
pub mod screenshot_cmds;
pub mod skills_cmds;

// Re-export extracted commands so `use settings_cmds::X` keeps working in lib.rs.
pub use dnd_cmds::{
    get_dnd_config, set_dnd_config, get_dnd_active, get_dnd_status,
    test_dnd, list_focus_modes,
    pick_ref_wav_file,
};
pub use hook_cmds::{
    HookDistanceSuggestion,
    suggest_hook_distances, get_hook_log, get_hook_log_count,
};
pub use device_cmds::{
    get_status, get_devices, get_supported_companies, get_device_capabilities,
    set_preferred_device, pair_device, forget_device, cancel_retry, retry_connect,
};
pub use activity_cmds::{
    get_active_window_tracking, set_active_window_tracking, get_active_window,
    get_input_activity_tracking, set_input_activity_tracking,
    get_last_input_activity,
    get_recent_active_windows, get_recent_input_activity,
    get_input_buckets,
};
pub use screenshot_cmds::{
    get_screenshot_config, set_screenshot_config,
    estimate_screenshot_reembed, rebuild_screenshot_embeddings,
    get_screenshots_around, search_screenshots_by_vector, search_screenshots_by_image,
    search_screenshots_by_text,
    check_ocr_models_ready, download_ocr_models,
    get_screenshot_metrics, get_screenshots_dir,
};
pub use skills_cmds::{
    get_skills_refresh_interval, set_skills_refresh_interval,
    get_skills_last_sync, sync_skills_now,
    list_skills, get_disabled_skills, set_disabled_skills, get_skills_license,
};

use std::{collections::HashMap, sync::Mutex};
use crate::MutexExt;
use tauri::{AppHandle, Emitter};

use crate::{
    AppState, EegPacket, PpgPacket, ImuPacket,
    emit_status, save_settings, skill_dir, mutate_and_save,
    constants::{EMBEDDING_OVERLAP_MIN_SECS, EMBEDDING_OVERLAP_MAX_SECS, LOG_CONFIG_FILE},
};
use skill_eeg::eeg_filter::{FilterConfig, PowerlineFreq};
use crate::settings::{OpenBciConfig, DeviceApiConfig, NeuttsConfig, HookRule, HookStatus};
use skill_eeg::eeg_bands::BandSnapshot;
use skill_eeg::eeg_model_config::{EegModelConfig, EegModelStatus, save_model_config};
use crate::eeg_embeddings::download_hf_weights;
use crate::settings::{UmapUserConfig, save_umap_config};
use crate::autostart;
use crate::AppStateExt;

#[derive(serde::Serialize)]
pub struct HookKeywordSuggestion {
    pub keyword: String,
    pub source:  String,
    pub score:   f32,
}

fn norm_keyword(s: &str) -> String {
    s.trim().to_lowercase()
}

fn fuzzy_score(query: &str, candidate: &str) -> f32 {
    let q = norm_keyword(query);
    let c = norm_keyword(candidate);
    if q.is_empty() || c.is_empty() {
        return 0.0;
    }
    if q == c {
        return 1.0;
    }
    if c.contains(&q) {
        return 0.92;
    }
    if q.contains(&c) {
        return 0.88;
    }
    if skill_exg::fuzzy_match(&q, &c) {
        return 0.75;
    }
    0.0
}

fn merge_suggestion(
    out: &mut HashMap<String, HookKeywordSuggestion>,
    keyword: &str,
    source: &str,
    score: f32,
) {
    let k = keyword.trim();
    if k.is_empty() || !score.is_finite() || score <= 0.0 {
        return;
    }
    let key = norm_keyword(k);
    if key.is_empty() {
        return;
    }
    if let Some(existing) = out.get_mut(&key) {
        existing.score = existing.score.max(score);
        if existing.source != source {
            existing.source = "both".to_owned();
        }
    } else {
        out.insert(
            key,
            HookKeywordSuggestion {
                keyword: k.to_owned(),
                source: source.to_owned(),
                score,
            },
        );
    }
}

/// Suggest hook keywords from previous labels using fuzzy + semantic search.
///
/// - Fuzzy suggestions come from `labels.sqlite` text matching.
/// - Embedding suggestions come from label text HNSW nearest-neighbor search.
#[tauri::command]
pub async fn suggest_hook_keywords(
    draft:     String,
    limit:     Option<usize>,
    state:     tauri::State<'_, Mutex<Box<AppState>>>,
    embedder:  tauri::State<'_, std::sync::Arc<crate::label_cmds::EmbedderState>>,
    label_idx: tauri::State<'_, std::sync::Arc<crate::label_index::LabelIndexState>>,
) -> Result<Vec<HookKeywordSuggestion>, String> {
    let q = draft.trim().to_owned();
    if q.len() < 2 {
        return Ok(vec![]);
    }

    let max_n = limit.unwrap_or(8).clamp(1, 20);
    let skill_dir = skill_dir(&state);
    let labels_db = skill_dir.join(crate::constants::LABELS_FILE);
    let embedder = std::sync::Arc::clone(&embedder);
    let label_idx = std::sync::Arc::clone(&label_idx);

    tokio::task::spawn_blocking(move || -> Result<Vec<HookKeywordSuggestion>, String> {
        let mut merged: HashMap<String, HookKeywordSuggestion> = HashMap::new();

        // ── Fuzzy suggestions from labels.sqlite ───────────────────────────
        if labels_db.exists() {
            if let Ok(conn) = skill_data::util::open_readonly(&labels_db) {
                if let Ok(mut stmt) = conn.prepare(
                    "SELECT text FROM labels
                     WHERE length(trim(text)) > 0
                     GROUP BY text
                     ORDER BY MAX(created_at) DESC
                     LIMIT 600",
                ) {
                    if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                        for text in rows.flatten() {
                            let score = fuzzy_score(&q, &text);
                            if score > 0.0 {
                                merge_suggestion(&mut merged, &text, "fuzzy", score);
                            }
                        }
                    }
                }
            }
        }

        // ── Semantic suggestions from label-text HNSW ──────────────────────
        {
            let mut guard = embedder.0.lock_or_recover();
            if let Some(te) = guard.as_mut() {
                let mut vecs = te.embed(vec![q.as_str()], None).map_err(|e| e.to_string())?;
                let query_vec = vecs.remove(0);
                let k = (max_n * 3).clamp(8, 48);
                let ef = (k * 4).max(64);
                let hits = crate::label_index::search_by_text_vec(&query_vec, k, ef, &skill_dir, &label_idx);
                for h in hits {
                    let score = (1.0 - (h.distance / 2.0)).clamp(0.0, 1.0);
                    merge_suggestion(&mut merged, &h.text, "embedding", score);
                }
            }
        }

        let mut out: Vec<HookKeywordSuggestion> = merged.into_values().collect();
        out.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.keyword.len().cmp(&b.keyword.len()))
        });
        out.truncate(max_n);
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── EEG / PPG / IMU subscriptions ────────────────────────────────────────────

#[tauri::command]
pub fn subscribe_eeg(on_event: tauri::ipc::Channel<EegPacket>, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    state.lock_or_recover().eeg_channel = Some(on_event);
}

#[tauri::command]
pub fn subscribe_ppg(on_event: tauri::ipc::Channel<PpgPacket>, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    state.lock_or_recover().ppg_channel = Some(on_event);
}

#[tauri::command]
pub fn subscribe_imu(on_event: tauri::ipc::Channel<ImuPacket>, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    state.lock_or_recover().imu_channel = Some(on_event);
}

// ── EEG filter commands ────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_filter_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> FilterConfig {
    state.lock_or_recover().status.filter_config
}

#[tauri::command]
pub fn set_filter_config(config: FilterConfig, app: AppHandle) {
    // Only write to status.filter_config.  The running SessionDsp picks up
    // the change via SessionDsp::sync_config() at the top of its next frame
    // (<250 ms latency), without ever holding the AppState lock during DSP.
    {
        let r = app.app_state();
        r.lock_or_recover().status.filter_config = config;
    }
    save_settings(&app);
    emit_status(&app);
}

#[tauri::command]
pub fn set_notch_preset(preset: Option<PowerlineFreq>, app: AppHandle) {
    {
        let r = app.app_state();
        r.lock_or_recover().status.filter_config.notch = preset;
    }
    save_settings(&app);
    emit_status(&app);
}

// ── Storage format ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_storage_format(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    state.lock_or_recover().settings_storage_format.clone()
}

#[tauri::command]
pub fn set_storage_format(format: String, app: AppHandle) {
    let fmt = match format.to_ascii_lowercase().as_str() {
        "parquet" => "parquet",
        "both"    => "both",
        _         => "csv",
    };
    {
        let r = app.app_state();
        r.lock_or_recover().settings_storage_format = fmt.to_string();
    }
    save_settings(&app);
}

// ── Band power ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_latest_bands(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Option<BandSnapshot> {
    // latest_bands is written back by the session task after each ~4 Hz
    // computation; reading it never blocks on DSP.
    state.lock_or_recover().latest_bands.clone()
}

// ── Embedding overlap ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_embedding_overlap(state: tauri::State<'_, Mutex<Box<AppState>>>) -> f32 {
    state.lock_or_recover().status.embedding_overlap_secs
}

#[tauri::command]
pub fn set_embedding_overlap(overlap_secs: f32, app: AppHandle) {
    let clamped = overlap_secs.clamp(EMBEDDING_OVERLAP_MIN_SECS, EMBEDDING_OVERLAP_MAX_SECS);
    {
        let r = app.app_state();
        r.lock_or_recover().status.embedding_overlap_secs = clamped;
        // SessionDsp::sync_config() picks up the change next frame.
    }
    save_settings(&app);
    emit_status(&app);
}

// ── GPU stats ──────────────────────────────────────────────────────────────────

/// Return current GPU statistics.
///
/// On macOS the utilisation fields (`render`, `tiler`, `overall`) are live
/// values from the IOKit EWMA sampler.  On Linux and Windows they are always
/// 0.0 — only memory figures are populated (via `llmfit-core`).
///
/// Returns `None` when no GPU can be detected on the current platform.
#[tauri::command]
pub fn get_gpu_stats() -> Option<skill_data::gpu_stats::GpuStats> {
    skill_data::gpu_stats::read()
}

// ── Logging config ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_log_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> crate::skill_log::LogConfig {
    state.lock_or_recover().logger.get_config()
}

#[tauri::command]
pub fn set_log_config(config: crate::skill_log::LogConfig, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let s = state.lock_or_recover();
    let config_path = s.skill_dir.join(LOG_CONFIG_FILE);
    // Propagate TTS, LLM, and tool logging flags to their crate-level runtime atomics.
    crate::tts::set_logging(config.tts);
    crate::llm::set_llm_logging(config.llm || config.chat_store);
    crate::llm::set_tool_logging(config.tools);
    s.logger.set_config(config, &config_path);
}

// ── EEG model config ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_eeg_model_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> EegModelConfig {
    state.lock_or_recover().model_config.clone()
}

#[tauri::command]
pub fn set_eeg_model_config(config: EegModelConfig, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let mut s = state.lock_or_recover();
    save_model_config(&s.skill_dir, &config);
    s.model_config = config;
}

#[tauri::command]
pub fn get_eeg_model_status(state: tauri::State<'_, Mutex<Box<AppState>>>) -> EegModelStatus {
    state.lock_or_recover().model_status.lock_or_recover().clone()
}

/// Spawn a background thread that downloads ZUNA weights from HuggingFace Hub.
///
/// Resets the cancel flag before starting so a previous cancellation does not
/// immediately abort the new attempt.  Progress and errors are reflected in
/// [`EegModelStatus`] which the UI polls every 2 s.
///
/// On success the `encoder_reload_requested` flag is set so the running embed
/// worker exits and respawns — loading the new weights in-process without
/// requiring an app restart.
#[tauri::command]
pub fn trigger_weights_download(state: tauri::State<'_, Mutex<Box<AppState>>>) {
    use std::sync::atomic::Ordering;

    let s = state.lock_or_recover();
    let hf_repo          = s.model_config.hf_repo.clone();
    let model_status     = s.model_status.clone();
    let cancel           = s.download_cancel.clone();
    let reload_requested = s.encoder_reload_requested.clone();
    let logger           = s.logger.clone();
    drop(s); // release AppState lock before spawning

    // Clear any previous cancellation so the new attempt actually runs.
    cancel.store(false, Ordering::Relaxed);

    std::thread::Builder::new()
        .name("hf-download".into())
        .spawn(move || {
            // mark_needs_restart=false: instead of prompting restart we signal
            // the embed worker to reload in-place via encoder_reload_requested.
            if download_hf_weights(&hf_repo, &model_status, &cancel, false, &logger).is_some() {
                // Signal the running embed worker (if any) to exit and respawn
                // so it picks up the freshly downloaded encoder immediately.
                reload_requested.store(true, Ordering::Relaxed);
                skill_log!(logger, "embedder",
                    "weights downloaded — signalling embed worker for in-place reload");
            }
        })
        .expect("[hf-download] failed to spawn download thread");
}

/// Set the download-cancel flag.
///
/// The background download thread checks this flag between the two file
/// downloads (config.json and the large safetensors file) and aborts if it
/// is `true`.
#[tauri::command]
pub fn cancel_weights_download(state: tauri::State<'_, Mutex<Box<AppState>>>) {
    use std::sync::atomic::Ordering;
    let s = state.lock_or_recover();
    s.download_cancel.store(true, Ordering::Relaxed);
    // Immediately reflect cancellation in the status so the UI updates before
    // the download thread has a chance to notice the flag.
    let mut st = s.model_status.lock_or_recover();
    if st.downloading_weights {
        st.download_status_msg = Some("Cancelling…".to_string());
    }
}

// ── UMAP config ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_umap_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> UmapUserConfig {
    state.lock_or_recover().umap_config.clone()
}

#[tauri::command]
pub fn set_umap_config(config: UmapUserConfig, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let mut s = state.lock_or_recover();
    save_umap_config(&s.skill_dir, &config);
    let cache_dir = s.skill_dir.join("umap_cache");
    if cache_dir.exists() {
        let _ = std::fs::remove_dir_all(&cache_dir);
        eprintln!("[umap] cleared cache after config change");
    }
    s.umap_config = config;
}

// ── Theme & language ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_theme_and_language(state: tauri::State<'_, Mutex<Box<AppState>>>) -> (String, String) {
    let s = state.lock_or_recover();
    (s.theme.clone(), s.language.clone())
}

#[tauri::command]
pub fn set_theme(theme: String, app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    mutate_and_save(&app, |s| s.theme = theme);
}

#[tauri::command]
pub fn set_language(language: String, app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    mutate_and_save(&app, |s| s.language = language);
}

#[tauri::command]
pub fn get_accent_color(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    state.lock_or_recover().accent_color.clone()
}

#[tauri::command]
pub fn set_accent_color(
    accent: String,
    app:    AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    mutate_and_save(&app, |s| s.accent_color = accent);
}

// ── Daily goal ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_daily_goal(state: tauri::State<'_, Mutex<Box<AppState>>>) -> u32 {
    state.lock_or_recover().daily_goal_min
}

#[tauri::command]
pub fn set_daily_goal(minutes: u32, app: AppHandle, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let clamped = minutes.min(480);
    state.lock_or_recover().daily_goal_min = clamped;
    save_settings(&app);
    let _ = app.emit("daily-goal-changed", clamped);
}

#[tauri::command]
pub fn get_goal_notified_date(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    state.lock_or_recover().goal_notified_date.clone()
}

#[tauri::command]
pub fn set_goal_notified_date(date: String, app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    mutate_and_save(&app, |s| s.goal_notified_date = date);
}

// ── Hooks ─────────────────────────────────────────────────────────────────────

pub fn sanitize_hook(mut h: HookRule) -> Option<HookRule> {
    h.name = h.name.trim().to_owned();
    if h.name.is_empty() { return None; }

    h.command = h.command.trim().to_owned();
    h.text = h.text.trim().to_owned();
    let scenario = h.scenario.trim().to_lowercase();
    h.scenario = match scenario.as_str() {
        "emotional" | "physical" | "cognitive" => scenario,
        _ => "any".to_owned(),
    };
    h.keywords = h.keywords
        .into_iter()
        .map(|k| k.trim().to_owned())
        .filter(|k| !k.is_empty())
        .take(20)
        .collect();

    h.distance_threshold = h.distance_threshold.clamp(0.01, 1.0);
    h.recent_limit = h.recent_limit.clamp(10, 20);
    Some(h)
}

#[tauri::command]
pub fn get_hooks(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<HookRule> {
    state.lock_or_recover().hooks.clone()
}

#[tauri::command]
pub fn set_hooks(
    hooks: Vec<HookRule>,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let clean: Vec<HookRule> = hooks
        .into_iter()
        .filter_map(sanitize_hook)
        .take(100)
        .collect();
    {
        let mut s = state.lock_or_recover();
        s.hooks = clean;
        let keep: std::collections::HashSet<String> = s.hooks.iter().map(|h| h.name.clone()).collect();
        s.hook_runtime.lock_or_recover().retain(|name, _| keep.contains(name));
    }
    save_settings(&app);
}

#[tauri::command]
pub fn get_hook_statuses(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<HookStatus> {
    let s = state.lock_or_recover();
    let runtime = s.hook_runtime.lock_or_recover();
    s.hooks
        .iter()
        .cloned()
        .map(|hook| HookStatus {
            last_trigger: runtime.get(&hook.name).cloned(),
            hook,
        })
        .collect()
}

#[tauri::command]
pub async fn open_session_for_timestamp(
    timestamp_utc: u64,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    let skill_dir = skill_dir(&state);
    let Some(csv_path) = crate::history_cmds::find_session_csv_for_timestamp(&skill_dir, timestamp_utc) else {
        return Err("no session found for timestamp".to_owned());
    };
    crate::window_cmds::open_session_window(app, csv_path).await
}

/// Return daily recording minutes for the last N days.
///
/// Runs on a blocking thread — reads up to 30 day-directories and parses
/// JSON sidecar files, which can be slow on spinning disks or large histories.
#[tauri::command]
pub async fn get_daily_recording_mins(
    days:  Option<u32>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<(String, u32)>, String> {
    let skill_dir = skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        get_daily_recording_mins_sync(&skill_dir, days)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Synchronous implementation extracted for `spawn_blocking`.
fn get_daily_recording_mins_sync(
    skill_dir: &std::path::Path,
    days: Option<u32>,
) -> Vec<(String, u32)> {
    let n = days.unwrap_or(30).min(365) as i64;
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut results: Vec<(String, u32)> = (0..n)
        .map(|i| {
            let day_secs = now_secs - i * 86400;
            let (y, mo, d) = unix_to_ymd(day_secs as u64);
            (format!("{y:04}{mo:02}{d:02}"), 0u32)
        })
        .collect();

    for (dir_date, total) in results.iter_mut() {
        let dir = skill_dir.join(dir_date.as_str());
        if !dir.is_dir() { continue; }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !((fname.starts_with("exg_") || fname.starts_with("muse_")) && fname.ends_with(".json")) { continue; }
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let start = meta["session_start_utc"].as_u64().unwrap_or(0);
            let end   = meta["session_end_utc"].as_u64().unwrap_or(start);
            *total += (end.saturating_sub(start) / 60) as u32;
        }
    }

    results.reverse();
    results.into_iter()
        .map(|(d, m)| (format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8]), m))
        .collect()
}

pub(crate) fn unix_to_ymd(ts: u64) -> (u32, u32, u32) {
    let days = ts / 86400;
    let z    = days + 719468;
    let era  = z / 146097;
    let doe  = z - era * 146097;
    let yoe  = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y    = yoe + era * 400;
    let doy  = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp   = (5 * doy + 2) / 153;
    let d    = doy - (153 * mp + 2) / 5 + 1;
    let m    = if mp < 10 { mp + 3 } else { mp - 9 };
    let y    = if m <= 2 { y + 1 } else { y };
    (y as u32, m as u32, d as u32)
}

// ── WebSocket server configuration ────────────────────────────────────────────

/// Return `(host, port)` — the persisted WebSocket bind config.
#[tauri::command]
pub fn get_ws_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> (String, u16) {
    let s = state.lock_or_recover();
    (s.ws_host.clone(), s.ws_port)
}

/// Persist a new WebSocket host/port.
///
/// `host` must be `"127.0.0.1"` or `"0.0.0.0"`.  Changes take effect after
/// the next app restart (the server binds once at startup).
#[tauri::command]
pub fn set_ws_config(
    host:  String,
    port:  u16,
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    if host != "127.0.0.1" && host != "0.0.0.0" {
        return Err(format!("invalid host '{host}': must be '127.0.0.1' or '0.0.0.0'"));
    }
    if port < 1024 {
        return Err(format!("port {port} is reserved; use 1024–65535"));
    }
    {
        let mut s = state.lock_or_recover();
        s.ws_host = host;
        s.ws_port = port;
    }
    crate::save_settings(&app);
    Ok(())
}

// ── Autostart (launch at login) ────────────────────────────────────────────────

/// Returns `true` if the app is registered to launch at login.
///
/// Reads the OS-level registration directly (plist / .desktop / registry).
#[tauri::command]
pub fn get_autostart_enabled(app: AppHandle) -> bool {
    let name = app.config()
        .product_name
        .as_deref()
        .unwrap_or("skill")
        .to_lowercase();
    autostart::is_enabled(&name)
}

/// Enable or disable launch-at-login.
///
/// On macOS this writes / removes a LaunchAgent plist.
/// On Linux this writes / removes an XDG `.desktop` file.
/// On Windows this writes / deletes the `HKCU\...\Run` registry value.
#[tauri::command]
pub fn set_autostart_enabled(
    app:     AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let name = app.config()
        .product_name
        .as_deref()
        .unwrap_or("skill")
        .to_lowercase();
    autostart::set_enabled(&name, enabled)
}

// ── Update-check interval ──────────────────────────────────────────────────────

/// Return the background update-check interval in seconds (0 = disabled).
#[tauri::command]
pub fn get_update_check_interval(state: tauri::State<'_, Mutex<Box<AppState>>>) -> u64 {
    state.lock_or_recover().update_check_interval_secs
}

/// Persist a new update-check interval.
///
/// `secs` = 0 disables automatic checking.
/// The background task re-reads this value each cycle, so the change takes
/// effect without a restart.
#[tauri::command]
pub fn set_update_check_interval(
    secs:  u64,
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().update_check_interval_secs = secs;
    crate::save_settings(&app);
}

// ── OpenBCI configuration ──────────────────────────────────────────────────────

/// Return the current OpenBCI configuration.
#[tauri::command]
pub fn get_openbci_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> OpenBciConfig {
    state.lock_or_recover().openbci_config.clone()
}

/// Persist new OpenBCI configuration.
///
/// Changes take effect on the next connection attempt — any active session
/// is not interrupted.
#[tauri::command]
pub fn set_openbci_config(
    config: OpenBciConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().openbci_config = config;
    crate::save_settings(&app);
}

/// Return the current device API credential configuration.
#[tauri::command]
pub fn get_device_api_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> DeviceApiConfig {
    state.lock_or_recover().device_api_config.clone()
}

/// Persist device API credential configuration.
#[tauri::command]
pub fn set_device_api_config(
    config: DeviceApiConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().device_api_config = config;
    crate::save_settings(&app);
}

/// Return the current scanner backend configuration.
#[tauri::command]
pub fn get_scanner_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> skill_settings::ScannerConfig {
    state.lock_or_recover().scanner_config.clone()
}

/// Return recent device/scanner log entries for the frontend log viewer.
#[tauri::command]
pub fn get_device_log() -> Vec<crate::device_scanner::DeviceLogEntry> {
    crate::device_scanner::DEVICE_LOG
        .lock()
        .map(|r| r.entries())
        .unwrap_or_default()
}

/// Persist scanner backend configuration.
#[tauri::command]
pub fn set_scanner_config(
    config: skill_settings::ScannerConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().scanner_config = config;
    crate::save_settings(&app);
}

/// List available serial ports on the host (for Cyton board selection).
///
/// Runs on a blocking thread — serial port enumeration can take hundreds of
/// milliseconds on some platforms (especially Windows with USB-serial drivers).
#[tauri::command]
pub async fn list_serial_ports() -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(|| {
        serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.port_name)
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

// ── NeuTTS configuration ───────────────────────────────────────────────────────

/// Return the current NeuTTS configuration.
#[tauri::command]
pub fn get_neutts_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> NeuttsConfig {
    state.lock_or_recover().neutts_config.clone()
}

/// Return whether TTS engine pre-warming at startup is enabled.
#[tauri::command]
pub fn get_tts_preload(state: tauri::State<'_, Mutex<Box<AppState>>>) -> bool {
    state.lock_or_recover().tts_preload
}

/// Enable or disable TTS engine pre-warming at startup, and persist the change.
#[tauri::command]
pub fn set_tts_preload(
    preload: bool,
    app:     AppHandle,
    state:   tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().tts_preload = preload;
    crate::save_settings(&app);
}

/// Persist new NeuTTS configuration.
///
/// If NeuTTS is enabled the backend is marked dirty so the next
/// `tts_init` call re-initialises with the updated backbone / ref-WAV.
#[tauri::command]
pub fn set_neutts_config(
    config: NeuttsConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    crate::tts::neutts_apply_config(&config);
    state.lock_or_recover().neutts_config = config;
    crate::save_settings(&app);
}

// ── LLM server configuration ──────────────────────────────────────────────────

/// Return the current LLM server configuration.
///
/// The LLM endpoints (`/v1/*`) are only active when `enabled = true` **and**
/// the binary was compiled with `--features llm`.  Changes take effect on the
/// next app restart.
// ── Sleep schedule ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_sleep_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> crate::settings::SleepConfig {
    state.lock_or_recover().sleep_config.clone()
}

#[tauri::command]
pub fn set_sleep_config(
    config: crate::settings::SleepConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().sleep_config = config;
    crate::save_settings(&app);
}

// ── LLM ───────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_llm_config(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> crate::settings::LlmConfig {
    { let __a = state.lock_or_recover().llm.clone(); let __r = __a.lock_or_recover().config.clone(); __r }
}

/// Update the LLM server configuration and persist it to `settings.json`.
///
/// Most model/runtime changes still require restart, but the built-in tool
/// allow-list is pushed into the running LLM chat state immediately.
#[tauri::command]
pub fn set_llm_config(
    config: crate::settings::LlmConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    #[cfg(feature = "llm")]
    let cell = {
        let s = state.lock_or_recover();
        let __llm_arc = s.llm.clone(); let mut llm = __llm_arc.lock_or_recover();
        llm.config = config.clone();
        llm.state_cell.clone()
    };
    #[cfg(not(feature = "llm"))]
    {
        let s = state.lock_or_recover();
        { let __a = s.llm.clone(); __a.lock_or_recover().config = config.clone(); }
    }

    #[cfg(feature = "llm")]
    if let Some(server) = cell.lock_or_recover().clone() {
        // Preserve the runtime-only skill_api_port when updating tools config.
        let prev_port = server.allowed_tools.lock_or_recover().skill_api_port;
        let mut new_tools = config.tools.clone();
        new_tools.skill_api_port = prev_port;
        *server.allowed_tools.lock_or_recover() = new_tools;
    }

    save_settings(&app);
}

/// Open a native file-picker dialog for selecting a GGUF model file.
///
/// Returns `None` if the user cancels.
#[tauri::command]
pub async fn pick_gguf_file() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("GGUF model", &["gguf"])
            .set_title("Select GGUF model file")
            .pick_file()
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .ok()
    .flatten()
}

