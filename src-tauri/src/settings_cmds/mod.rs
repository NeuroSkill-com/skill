// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Device, filter, EEG model, app-settings, autostart, and update-interval Tauri commands.

pub mod dnd_cmds;
pub mod hook_cmds;

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

use std::{collections::HashMap, sync::Mutex};
use crate::MutexExt;
use tauri::{AppHandle, Emitter};

use crate::{
    AppState, DeviceStatus, DiscoveredDevice, EegPacket, PpgPacket, ImuPacket,
    emit_status, emit_devices, save_settings, skill_dir, mutate_and_save,
    start_session, cancel_session,
    constants::{EMBEDDING_OVERLAP_MIN_SECS, EMBEDDING_OVERLAP_MAX_SECS, LOG_CONFIG_FILE},
};
use crate::tray::refresh_tray;
use skill_eeg::eeg_filter::{FilterConfig, PowerlineFreq};
use crate::settings::{OpenBciConfig, DeviceApiConfig, NeuttsConfig, HookRule, HookStatus};
use crate::active_window::ActiveWindowInfo;
use skill_data::activity_store::{ActiveWindowRow, InputActivityRow, InputBucketRow};
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

// ── Device commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_status(state: tauri::State<'_, Mutex<Box<AppState>>>) -> DeviceStatus {
    state.lock_or_recover().status.clone()
}

#[tauri::command]
pub fn get_devices(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<DiscoveredDevice> {
    state.lock_or_recover().discovered.clone()
}

#[tauri::command]
pub fn get_supported_companies() -> Vec<skill_data::device::SupportedCompany> {
    skill_data::device::supported_companies()
}

#[tauri::command]
pub fn get_device_capabilities(device_name: Option<String>) -> skill_data::device::DeviceCapabilities {
    let kind = skill_data::device::DeviceKind::from_name(device_name.as_deref());
    kind.capabilities()
}

#[tauri::command]
pub fn set_preferred_device(id: String, app: AppHandle) -> Vec<DiscoveredDevice> {
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.preferred_id = if id.is_empty() { None } else { Some(id.clone()) };
        let pref = s.preferred_id.clone();
        for d in s.discovered.iter_mut() { d.is_preferred = pref.as_deref() == Some(&d.id); }
    }
    save_settings(&app);
    emit_devices(&app);
    app.app_state().lock_or_recover().discovered.clone()
}

/// Explicitly pair a discovered device so it is trusted for future connections.
///
/// Adds the device to `paired_devices`, marks it as `is_paired` in the
/// discovered list, persists settings, and broadcasts updated state.
#[tauri::command]
pub fn pair_device(id: String, app: AppHandle) -> Vec<DiscoveredDevice> {
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        // Look up the name from the discovered list.
        let name = s.discovered.iter()
            .find(|d| d.id == id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| id.clone());
        let now = crate::unix_secs();
        // Insert into paired list if not already there.
        if !s.status.paired_devices.iter().any(|d| d.id == id) {
            s.status.paired_devices.push(crate::PairedDevice {
                id:        id.clone(),
                name:      name.clone(),
                last_seen: now,
            });
        }
        // Mark as paired in the discovered/settings list.
        for d in s.discovered.iter_mut() {
            if d.id == id {
                d.is_paired = true;
                d.name      = name.clone();
            }
        }
    }
    save_settings(&app);
    refresh_tray(&app);
    emit_status(&app);
    emit_devices(&app);
    app.app_state().lock_or_recover().discovered.clone()
}

#[tauri::command]
pub fn forget_device(id: String, app: AppHandle) -> DeviceStatus {
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.status.paired_devices.retain(|d| d.id != id);
        for d in s.discovered.iter_mut() { if d.id == id { d.is_paired = false; } }
        drop(s);
        save_settings(&app);
    }
    refresh_tray(&app); emit_status(&app); emit_devices(&app);
    app.app_state().lock_or_recover().status.clone()
}

#[tauri::command]
pub fn cancel_retry(app: AppHandle) {
    let r = app.app_state();
    let mut s = r.lock_or_recover();
    s.pending_reconnect           = false;
    s.retry_attempt               = 0;
    s.status.retry_attempt        = 0;
    s.status.retry_countdown_secs = 0;
    s.status.state                = "disconnected".into();
    s.status.device_error             = None;
    drop(s);
    cancel_session(&app);
    emit_status(&app);
}

#[tauri::command]
pub fn retry_connect(app: AppHandle) {
    let preferred = {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.pending_reconnect = true;
        s.retry_attempt     = 0;
        s.status.retry_attempt        = 0;
        s.status.retry_countdown_secs = 0;
        s.preferred_id.clone()
            .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()))
    };
    start_session(&app, preferred);
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

// ── Activity tracking ──────────────────────────────────────────────────────────

/// Return whether active-window tracking is currently enabled.
#[tauri::command]
pub fn get_active_window_tracking(state: tauri::State<'_, Mutex<Box<AppState>>>) -> bool {
    state.lock_or_recover().track_active_window
}

/// Enable or disable active-window tracking and persist the change.
#[tauri::command]
pub fn set_active_window_tracking(
    enabled: bool,
    app:     AppHandle,
    state:   tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().track_active_window = enabled;
    crate::save_settings(&app);
}

/// Return the most recently detected active window, or `None` when tracking
/// is disabled or no window has been observed yet.
#[tauri::command]
pub fn get_active_window(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Option<ActiveWindowInfo> {
    state.lock_or_recover().current_active_window.clone()
}

/// Return whether keyboard/mouse input tracking is currently enabled.
#[tauri::command]
pub fn get_input_activity_tracking(state: tauri::State<'_, Mutex<Box<AppState>>>) -> bool {
    state.lock_or_recover().track_input_activity
}

/// Enable or disable keyboard/mouse input tracking and persist the change.
/// Also flips the `AtomicBool` read by the input-monitor loop so the change
/// takes effect immediately without a restart.
#[tauri::command]
pub fn set_input_activity_tracking(
    enabled: bool,
    app:     AppHandle,
    state:   tauri::State<'_, Mutex<Box<AppState>>>,
) {
    use std::sync::atomic::Ordering;
    let s = state.lock_or_recover();
    s.input_activity_enabled.store(enabled, Ordering::Relaxed);
    drop(s);
    state.lock_or_recover().track_input_activity = enabled;
    crate::save_settings(&app);
}

/// Return `(last_keyboard_unix_secs, last_mouse_unix_secs)`.
/// A value of `0` means the device type has not been seen since the app started.
#[tauri::command]
pub fn get_last_input_activity(state: tauri::State<'_, Mutex<Box<AppState>>>) -> (u64, u64) {
    use std::sync::atomic::Ordering;
    let s = state.lock_or_recover();
    (
        s.last_keyboard_ts.load(Ordering::Relaxed),
        s.last_mouse_ts.load(Ordering::Relaxed),
    )
}

/// Return up to `limit` recent active-window records from `activity.sqlite`,
/// newest first.
#[tauri::command]
pub fn get_recent_active_windows(
    limit: Option<u32>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<ActiveWindowRow> {
    let n = limit.unwrap_or(50).min(500);
    state
        .lock_or_recover()
        .activity_store
        .as_ref()
        .map(|s| s.get_recent_windows(n))
        .unwrap_or_default()
}

/// Return up to `limit` recent input-activity samples from `activity.sqlite`,
/// newest first.
#[tauri::command]
pub fn get_recent_input_activity(
    limit: Option<u32>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<InputActivityRow> {
    let n = limit.unwrap_or(50).min(500);
    state
        .lock_or_recover()
        .activity_store
        .as_ref()
        .map(|s| s.get_recent_input(n))
        .unwrap_or_default()
}

/// Return per-minute input-event buckets for `[from_ts, to_ts]` (Unix seconds).
///
/// Both timestamps must be rounded to 60-second boundaries by the caller,
/// though SQLite will accept any value.  Results are ordered oldest-first
/// (ascending `minute_ts`) which is what charting libraries expect.
///
/// A convenience default: if `from_ts` is `None` the query covers the last
/// 24 hours.  If `to_ts` is `None` it defaults to now.
#[tauri::command]
pub fn get_input_buckets(
    from_ts: Option<u64>,
    to_ts:   Option<u64>,
    state:   tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<InputBucketRow> {
    let now   = crate::unix_secs();
    let end   = to_ts.unwrap_or(now);
    let start = from_ts.unwrap_or_else(|| end.saturating_sub(24 * 3600));
    state
        .lock_or_recover()
        .activity_store
        .as_ref()
        .map(|s| s.get_input_buckets(start, end))
        .unwrap_or_default()
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
    state.lock_or_recover().llm.config.clone()
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
        let mut s = state.lock_or_recover();
        s.llm.config = config.clone();
        s.llm.state_cell.clone()
    };
    #[cfg(not(feature = "llm"))]
    {
        let mut s = state.lock_or_recover();
        s.llm.config = config.clone();
    }

    #[cfg(feature = "llm")]
    if let Some(server) = cell.lock().expect("lock poisoned").clone() {
        // Preserve the runtime-only skill_api_port when updating tools config.
        let prev_port = server.allowed_tools.lock().expect("lock poisoned").skill_api_port;
        let mut new_tools = config.tools.clone();
        new_tools.skill_api_port = prev_port;
        *server.allowed_tools.lock().expect("lock poisoned") = new_tools;
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

// ── Screenshot config ─────────────────────────────────────────────────────────

/// Get current screenshot configuration.
#[tauri::command]
pub fn get_screenshot_config(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> crate::settings::ScreenshotConfig {
    state.lock_or_recover().screenshot_config.clone()
}

/// Update screenshot configuration.  Returns whether the embedding model
/// changed (so the frontend can prompt re-embedding).
#[tauri::command]
pub fn set_screenshot_config(
    config: crate::settings::ScreenshotConfig,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> skill_data::screenshot_store::ConfigChangeResult {
    let (old_backend, old_model, skill_dir) = {
        let g = state.lock_or_recover();
        (g.screenshot_config.embed_backend.clone(),
         g.screenshot_config.model_id(),
         g.skill_dir.clone())
    };

    let new_backend = config.embed_backend.clone();
    let new_model = config.model_id();
    let model_changed = old_backend != new_backend || old_model != new_model;

    {
        let mut g = state.lock_or_recover();
        g.screenshot_config = config;
    }
    crate::save_settings(&app);

    let stale_count = if model_changed {
        skill_data::screenshot_store::ScreenshotStore::open(&skill_dir)
            .map(|s| s.count_stale(&new_backend, &new_model))
            .unwrap_or(0)
    } else {
        0
    };

    skill_data::screenshot_store::ConfigChangeResult { model_changed, stale_count }
}

/// Count screenshots needing (re-)embedding and estimate wall-clock time.
/// Runs on a background thread to avoid blocking the UI.
#[tauri::command]
pub async fn estimate_screenshot_reembed(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Option<skill_data::screenshot_store::ReembedEstimate>, String> {
    let (config, skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.screenshot_config.clone(), g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let store = store.or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))?;
        Some(crate::screenshot::estimate_reembed(&store, &config, &skill_dir))
    }).await.unwrap_or(None))
}

/// Re-embed all screenshots with the current model.
/// Emits `screenshot-reembed-progress` events.
/// Runs on a background thread to avoid blocking the UI.
#[tauri::command]
pub async fn rebuild_screenshot_embeddings(
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Option<skill_data::screenshot_store::ReembedResult>, String> {
    let (config, skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.screenshot_config.clone(), g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let store = store.or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))?;
        let ctx = crate::screenshot::TauriScreenshotContext { app };
        Some(crate::screenshot::rebuild_embeddings(&store, &config, &skill_dir, &ctx))
    }).await.unwrap_or(None))
}

/// Find screenshots by timestamp range (for EEG correlation).
#[tauri::command]
pub async fn get_screenshots_around(
    timestamp: i64,
    window_secs: i32,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<skill_data::screenshot_store::ScreenshotResult>, String> {
    let (skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let store = match store.or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new)) {
            Some(s) => s,
            None => return vec![],
        };
        crate::screenshot::get_around(&store, timestamp, window_secs)
    }).await.unwrap_or_default())
}

/// Find screenshots visually similar to a query image.
/// Embeds the query image with the current model, then searches HNSW.
/// Runs on a background thread (model loading + inference is heavy).
#[tauri::command]
pub async fn search_screenshots_by_image(
    image_bytes: Vec<u8>,
    k: usize,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<skill_data::screenshot_store::ScreenshotResult>, String> {
    let (config, skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.screenshot_config.clone(), g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let mut encoder = crate::screenshot::load_fastembed_image_pub(&config, &skill_dir);
        let query_emb = if let Some(ref mut fe) = encoder {
            crate::screenshot::fastembed_embed_pub(fe, &image_bytes)
        } else {
            None
        };
        let query = match query_emb {
            Some(v) => v,
            None => return vec![],
        };
        let store = match store.or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new)) {
            Some(s) => s,
            None => return vec![],
        };
        let hnsw_path = skill_dir.join(crate::constants::SCREENSHOTS_HNSW);
        let hnsw = match fast_hnsw::labeled::LabeledIndex::<fast_hnsw::distance::Cosine, i64>::load(&hnsw_path, fast_hnsw::distance::Cosine) {
            Ok(idx) => idx,
            Err(_) => return vec![],
        };
        crate::screenshot::search_by_vector(&hnsw, &store, &query, k)
    }).await.unwrap_or_default())
}

/// Get screenshot pipeline metrics (capture + embed thread performance).
/// Lightweight — just reads atomics, no spawn_blocking needed.
#[tauri::command]
pub fn get_screenshot_metrics(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> crate::screenshot::MetricsSnapshot {
    let metrics = state.lock_or_recover().screenshot_metrics.clone();
    metrics.snapshot()
}

/// Check whether OCR models are downloaded and ready.
#[tauri::command]
pub fn check_ocr_models_ready(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> bool {
    let skill_dir = skill_dir(&state);
    let ocr_dir = skill_dir.join("ocr_models");
    ocr_dir.join(crate::constants::OCR_DETECTION_MODEL_FILE).exists()
        && ocr_dir.join(crate::constants::OCR_RECOGNITION_MODEL_FILE).exists()
}

/// Download OCR models (text-detection.rten + text-recognition.rten).
/// Returns true if both models are now available.
/// Runs on a background thread (network download).
#[tauri::command]
pub async fn download_ocr_models(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<bool, String> {
    let skill_dir = skill_dir(&state);
    Ok(tokio::task::spawn_blocking(move || {
        let ocr_dir = skill_dir.join("ocr_models");
        let _ = std::fs::create_dir_all(&ocr_dir);
        let det_path = ocr_dir.join(crate::constants::OCR_DETECTION_MODEL_FILE);
        let rec_path = ocr_dir.join(crate::constants::OCR_RECOGNITION_MODEL_FILE);
        let det_ok = crate::screenshot::download_ocr_model_pub(
            crate::constants::OCR_DETECTION_MODEL_URL, &det_path,
        );
        let rec_ok = crate::screenshot::download_ocr_model_pub(
            crate::constants::OCR_RECOGNITION_MODEL_URL, &rec_path,
        );
        det_ok && rec_ok
    }).await.unwrap_or(false))
}

/// Search screenshots by OCR text — both semantic (embedding similarity)
/// and substring (SQL LIKE) modes.
/// `mode`: "semantic" (default) uses text embedding HNSW search,
///         "substring" uses SQL LIKE matching.
/// Runs on a background thread (semantic mode loads an embedding model).
#[tauri::command]
pub async fn search_screenshots_by_text(
    query: String,
    k: Option<usize>,
    mode: Option<String>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
    embedder: tauri::State<'_, std::sync::Arc<crate::EmbedderState>>,
) -> Result<Vec<skill_data::screenshot_store::ScreenshotResult>, String> {
    let (skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.skill_dir.clone(), g.screenshot_store.clone())
    };
    let embedder = std::sync::Arc::clone(&embedder);
    Ok(tokio::task::spawn_blocking(move || {
        let store = match store.or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new)) {
            Some(s) => s,
            None => return vec![],
        };
        let k = k.unwrap_or(20);
        let mode = mode.unwrap_or_else(|| "semantic".into());
        match mode.as_str() {
            "substring" => crate::screenshot::search_by_ocr_text_like(&store, &query, k),
            _ => {
                let embed_fn = |text: &str| -> Option<Vec<f32>> {
                    let mut guard = embedder.0.lock().ok()?;
                    let te = guard.as_mut()?;
                    let mut vecs = te.embed(vec![text], None).ok()?;
                    if vecs.is_empty() { None } else { Some(vecs.remove(0)) }
                };
                crate::screenshot::search_by_ocr_text_embedding(&skill_dir, &store, &query, k, &embed_fn)
            }
        }
    }).await.unwrap_or_default())
}

/// Return the screenshots directory path and the WebSocket server port.
/// The frontend constructs image URLs as `http://127.0.0.1:{port}/screenshots/{filename}`
/// which are served by the axum HTTP server — no asset protocol scope needed.
#[tauri::command]
pub fn get_screenshots_dir(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> (String, u16) {
    let g = state.lock_or_recover();
    let dir = g.skill_dir
        .join(crate::constants::SCREENSHOTS_DIR)
        .to_string_lossy()
        .into_owned();
    let port = g.ws_port;
    (dir, port)
}

/// Find screenshots visually similar to a query embedding vector.
/// Runs on a background thread (HNSW load + search).
#[tauri::command]
pub async fn search_screenshots_by_vector(
    vector: Vec<f32>,
    k: usize,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<skill_data::screenshot_store::ScreenshotResult>, String> {
    let (skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let store = match store.or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new)) {
            Some(s) => s,
            None => return vec![],
        };
        let hnsw_path = skill_dir.join(crate::constants::SCREENSHOTS_HNSW);
        let hnsw = match fast_hnsw::labeled::LabeledIndex::<fast_hnsw::distance::Cosine, i64>::load(&hnsw_path, fast_hnsw::distance::Cosine) {
            Ok(idx) => idx,
            Err(_) => return vec![],
        };
        crate::screenshot::search_by_vector(&hnsw, &store, &vector, k)
    }).await.unwrap_or_default())
}

// ── Skills refresh ─────────────────────────────────────────────────────────────

/// Return the current community-skills refresh interval in seconds (0 = disabled).
#[tauri::command]
pub fn get_skills_refresh_interval(state: tauri::State<'_, Mutex<Box<AppState>>>) -> u64 {
    state.lock_or_recover().llm.config.tools.skills_refresh_interval_secs
}

/// Persist a new skills refresh interval.  `secs` = 0 disables auto-refresh.
#[tauri::command]
pub fn set_skills_refresh_interval(
    secs:  u64,
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().llm.config.tools.skills_refresh_interval_secs = secs;
    crate::save_settings(&app);
}

/// Return the Unix timestamp of the last successful skills sync, or `null`.
#[tauri::command]
pub fn get_skills_last_sync(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Option<u64> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    skill_skills::sync::last_sync_ts(&skill_dir)
}

/// Trigger a manual skills sync (ignores the interval, forces re-download).
#[tauri::command]
pub async fn sync_skills_now(
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<String, String> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        skill_skills::sync::sync_skills(&skill_dir, 0, None)
    })
    .await
    .map_err(|e| format!("task panic: {e}"))?;

    match outcome {
        skill_skills::sync::SyncOutcome::Updated { elapsed_ms, .. } => {
            let _ = app.emit("skills-updated", ());
            Ok(format!("updated in {elapsed_ms} ms"))
        }
        skill_skills::sync::SyncOutcome::Fresh { .. } => {
            Ok("already up to date".into())
        }
        skill_skills::sync::SyncOutcome::Failed(e) => Err(e),
    }
}

// ── Discovered skills listing ──────────────────────────────────────────────────

/// A lightweight description of a discovered skill, sent to the frontend.
#[derive(serde::Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub source: String,
    pub enabled: bool,
}

/// List all discovered skills from the user's skill_dir (and bundled/project
/// directories).  Each entry includes whether it is currently enabled based on
/// the `disabled_skills` list in settings.
#[tauri::command]
pub fn list_skills(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<SkillInfo> {
    let (skill_dir, disabled) = {
        let g = state.lock_or_recover();
        (g.skill_dir.clone(), g.llm.config.tools.disabled_skills.clone())
    };

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let bundled_dir = exe_dir.as_ref()
        .map(|d| d.join(skill_constants::SKILLS_SUBDIR))
        .filter(|d| d.is_dir())
        .or_else(|| {
            let cwd = std::env::current_dir().ok()?;
            let p = cwd.join(skill_constants::SKILLS_SUBDIR);
            if p.is_dir() { Some(p) } else { None }
        });

    let result = skill_skills::load_skills(skill_skills::LoadSkillsOptions {
        cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        skill_dir: skill_dir.to_path_buf(),
        bundled_dir,
        skill_paths: Vec::new(),
        include_defaults: true,
    });

    result.skills.into_iter().map(|s| {
        let enabled = !disabled.iter().any(|d| d == &s.name);
        SkillInfo {
            name: s.name,
            description: s.description,
            source: s.source,
            enabled,
        }
    }).collect()
}

/// Read the LICENSE file from the user's skills directory, if present.
#[tauri::command]
pub fn get_skills_license(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Option<String> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    let license_path = skill_dir.join(skill_constants::SKILLS_SUBDIR).join("LICENSE");
    std::fs::read_to_string(&license_path).ok()
}

/// Return the list of disabled skill names.
#[tauri::command]
pub fn get_disabled_skills(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<String> {
    state.lock_or_recover().llm.config.tools.disabled_skills.clone()
}

/// Persist the disabled-skills list.
#[tauri::command]
pub fn set_disabled_skills(
    names: Vec<String>,
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().llm.config.tools.disabled_skills = names;
    crate::save_settings(&app);

    // Live-update the running LLM server's tool config so the change takes
    // effect without a restart.
    #[cfg(feature = "llm")]
    {
        let (cell, tools) = {
            let g = state.lock_or_recover();
            (g.llm.state_cell.clone(), g.llm.config.tools.clone())
        };
        let server = cell.lock().expect("lock poisoned").as_ref().cloned();
        if let Some(server) = server {
            server.set_allowed_tools(tools);
        }
    }
}
