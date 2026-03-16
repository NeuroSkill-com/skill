// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Device, filter, EEG model, app-settings, autostart, and update-interval Tauri commands.

use std::{collections::HashMap, sync::Mutex};
use crate::MutexExt;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    AppState, DeviceStatus, DiscoveredDevice, EegPacket, PpgPacket, ImuPacket,
    emit_status, emit_devices, save_settings, skill_dir, mutate_and_save,
    start_session, cancel_session,
    constants::{EMBEDDING_OVERLAP_MIN_SECS, EMBEDDING_OVERLAP_MAX_SECS, LOG_CONFIG_FILE},
};
use crate::tray::refresh_tray;
use crate::eeg_filter::{FilterConfig, PowerlineFreq};
use crate::settings::{OpenBciConfig, NeuttsConfig, DoNotDisturbConfig, HookRule, HookStatus};
use crate::active_window::ActiveWindowInfo;
use crate::activity_store::{ActiveWindowRow, InputActivityRow, InputBucketRow};
use crate::eeg_bands::BandSnapshot;
use crate::eeg_model_config::{EegModelConfig, EegModelStatus, save_model_config};
use crate::eeg_embeddings::download_hf_weights;
use crate::settings::{UmapUserConfig, save_umap_config};
use crate::autostart;

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
    if crate::eeg_embeddings::fuzzy_match(&q, &c) {
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
            if let Ok(conn) = rusqlite::Connection::open_with_flags(
                &labels_db,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            ) {
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
pub fn set_preferred_device(id: String, app: AppHandle) -> Vec<DiscoveredDevice> {
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        s.preferred_id = if id.is_empty() { None } else { Some(id.clone()) };
        let pref = s.preferred_id.clone();
        for d in s.discovered.iter_mut() { d.is_preferred = pref.as_deref() == Some(&d.id); }
    }
    save_settings(&app);
    emit_devices(&app);
    app.state::<Mutex<Box<AppState>>>().lock_or_recover().discovered.clone()
}

/// Explicitly pair a discovered device so it is trusted for future connections.
///
/// Adds the device to `paired_devices`, marks it as `is_paired` in the
/// discovered list, persists settings, and broadcasts updated state.
#[tauri::command]
pub fn pair_device(id: String, app: AppHandle) -> Vec<DiscoveredDevice> {
    {
        let r = app.state::<Mutex<Box<AppState>>>();
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
    app.state::<Mutex<Box<AppState>>>().lock_or_recover().discovered.clone()
}

#[tauri::command]
pub fn forget_device(id: String, app: AppHandle) -> DeviceStatus {
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        s.status.paired_devices.retain(|d| d.id != id);
        for d in s.discovered.iter_mut() { if d.id == id { d.is_paired = false; } }
        drop(s);
        save_settings(&app);
    }
    refresh_tray(&app); emit_status(&app); emit_devices(&app);
    app.state::<Mutex<Box<AppState>>>().lock_or_recover().status.clone()
}

#[tauri::command]
pub fn cancel_retry(app: AppHandle) {
    let r = app.state::<Mutex<Box<AppState>>>();
    let mut s = r.lock_or_recover();
    s.pending_reconnect           = false;
    s.retry_attempt               = 0;
    s.status.retry_attempt        = 0;
    s.status.retry_countdown_secs = 0;
    s.status.state                = "disconnected".into();
    s.status.bt_error             = None;
    drop(s);
    cancel_session(&app);
    emit_status(&app);
}

#[tauri::command]
pub fn retry_connect(app: AppHandle) {
    let preferred = {
        let r = app.state::<Mutex<Box<AppState>>>();
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
        let r = app.state::<Mutex<Box<AppState>>>();
        r.lock_or_recover().status.filter_config = config;
    }
    save_settings(&app);
    emit_status(&app);
}

#[tauri::command]
pub fn set_notch_preset(preset: Option<PowerlineFreq>, app: AppHandle) {
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        r.lock_or_recover().status.filter_config.notch = preset;
    }
    save_settings(&app);
    emit_status(&app);
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
        let r = app.state::<Mutex<Box<AppState>>>();
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
pub fn get_gpu_stats() -> Option<crate::gpu_stats::GpuStats> {
    crate::gpu_stats::read()
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
            if !fname.starts_with("muse_") || !fname.ends_with(".json") { continue; }
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

// ── Do Not Disturb automation ─────────────────────────────────────────────────

/// Return all Focus modes configured on this Mac as `{ identifier, name }` pairs.
///
/// On macOS 12+ this reads `ModeConfigurations.json`; falls back to the
/// well-known first-party list if the file is unavailable.  Returns an empty
/// array on non-macOS platforms.
#[tauri::command]
pub fn list_focus_modes() -> Vec<crate::dnd::FocusModeOption> {
    crate::dnd::list_focus_modes()
}

/// Force-disable the active Focus mode.
///
/// This is a one-direction safety escape hatch: it can only **deactivate**
/// Focus mode, never activate it.  Activation is exclusively controlled by the
/// EEG scoring pipeline in the session loop.
///
/// `enabled` is accepted as a parameter for API symmetry, but any call with
/// `enabled = true` is rejected immediately (returns `false`) so that no code
/// path other than live EEG data can turn on Focus mode.
///
/// Returns `true` if the OS call succeeded.
#[tauri::command]
pub fn test_dnd(
    enabled: bool,
    app:     AppHandle,
    state:   tauri::State<'_, Mutex<Box<AppState>>>,
) -> bool {
    // Guard: only allow disabling, never enabling.
    if enabled { return false; }

    let ok = crate::dnd::set_dnd(false, "");
    if ok {
        state.lock_or_recover().dnd_active = false;
        let _ = app.emit("dnd-state-changed", false);
        app.state::<crate::ws_server::WsBroadcaster>()
            .send("dnd-state-changed", &false);
    }
    ok
}

/// Return whether DND is currently active (i.e. the app has enabled it).
#[tauri::command]
pub fn get_dnd_active(state: tauri::State<'_, Mutex<Box<AppState>>>) -> bool {
    state.lock_or_recover().dnd_active
}

/// Return the current Do Not Disturb automation configuration.
#[tauri::command]
pub fn get_dnd_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> DoNotDisturbConfig {
    state.lock_or_recover().dnd_config.clone()
}

/// Persist new Do Not Disturb automation configuration.
///
/// If the feature is disabled and DND is currently active, DND is cleared
/// immediately so the user is not left in an unintended DND state.
#[tauri::command]
pub fn set_dnd_config(
    config: DoNotDisturbConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let was_active = {
        let mut s = state.lock_or_recover();
        let active = s.dnd_active;
        s.dnd_config           = config.clone();
        s.dnd_focus_samples.clear();   // reset activation window on any config change
        s.dnd_below_ticks      = 0;    // reset exit counter on any config change
        s.dnd_score_history.clear();   // reset lookback history on any config change
        s.dnd_snr_low_ticks    = 0;    // reset SNR low counter on any config change
        if !config.enabled && s.dnd_active {
            s.dnd_active = false;
        }
        active && !config.enabled
    };

    // If we just disabled the feature while DND was on, clear it:
    // (1) Exit system Focus first, (2) then notify the user if configured.
    if was_active {
        let ok = crate::dnd::set_dnd(false, "");
        let payload = false;
        let _ = app.emit("dnd-state-changed", payload);
        app.state::<crate::ws_server::WsBroadcaster>()
            .send("dnd-state-changed", &payload);
        if ok && config.exit_notification {
            crate::send_toast(
                &app,
                crate::ToastLevel::Info,
                "Focus mode exited",
                "Do Not Disturb automation was disabled. Focus mode deactivated.",
            );
        }
    }

    crate::save_settings(&app);
}

/// Live snapshot of the Do Not Disturb automation pipeline.
///
/// Returned by [`get_dnd_status`] and mirrored by the `dnd-eligibility`
/// broadcast event (emitted ~4 Hz).
#[derive(serde::Serialize, Clone, Debug)]
pub struct DndStatus {
    /// Whether the DND automation feature is enabled in settings.
    pub enabled: bool,
    /// Rolling-average focus score over the current sample window (0–100).
    /// The live per-tick value is only available via the `dnd-eligibility` event.
    pub avg_score: f64,
    /// Score (0–100) that the rolling average must reach to activate DND.
    pub threshold: f64,
    /// Number of samples currently in the rolling window.
    pub sample_count: usize,
    /// Target window size in samples (≈ duration_secs × 4 Hz).
    pub window_size: usize,
    /// Duration (seconds) that defines the rolling window length.
    pub duration_secs: u32,
    /// Whether the app has currently activated DND.
    pub dnd_active: bool,
    /// Whether the OS reports DND / Focus as active right now (`null` on non-macOS).
    pub os_active: Option<bool>,
    /// Seconds the score must remain below the threshold before DND clears.
    pub exit_duration_secs: u32,
    /// Consecutive ticks for which the score has been below threshold while
    /// DND is active.  Used to show the exit countdown in the UI.
    pub below_ticks: u32,
    /// Total ticks required for the exit window (≈ exit_duration_secs × 4 Hz).
    pub exit_window_size: usize,
    /// Approximate seconds remaining until DND exits (0 if not counting down).
    pub exit_secs_remaining: f64,
    /// Lookback window in seconds: if any tick in this window was above the
    /// threshold the exit counter resets (recent focus delays deactivation).
    pub focus_lookback_secs: u32,
    /// `true` when DND is active, score is below threshold, but the lookback
    /// window still contains a focus peak so exit is being delayed.
    pub exit_held_by_lookback: bool,
}

/// Return a snapshot of the DND automation pipeline state.
#[tauri::command]
pub fn get_dnd_status(state: tauri::State<'_, Mutex<Box<AppState>>>) -> DndStatus {
    let s                    = state.lock_or_recover();
    let enabled              = s.dnd_config.enabled;
    let threshold            = s.dnd_config.focus_threshold as f64;
    let duration_secs        = s.dnd_config.duration_secs;
    let exit_duration_secs   = s.dnd_config.exit_duration_secs;
    let focus_lookback_secs  = s.dnd_config.focus_lookback_secs;
    let window_size          = (duration_secs as usize * 4).max(8);
    let exit_window_size     = (exit_duration_secs as usize * 4).max(4);
    let sample_count         = s.dnd_focus_samples.len();
    let avg_score            = if sample_count > 0 {
        s.dnd_focus_samples.iter().sum::<f64>() / sample_count as f64
    } else { 0.0 };
    let dnd_active           = s.dnd_active;
    let below_ticks          = s.dnd_below_ticks;
    let exit_held_by_lookback = dnd_active
        && avg_score < threshold
        && s.dnd_score_history.iter().any(|&v| v >= threshold);
    // Use the cached OS state (refreshed every 5 s by the background poll)
    // rather than reading the file on every UI request.
    let os_active            = s.dnd_os_active;
    drop(s);

    let exit_secs_remaining =
        if dnd_active && avg_score < threshold && !exit_held_by_lookback {
            let remaining = exit_window_size.saturating_sub(below_ticks as usize);
            remaining as f64 / 4.0
        } else { 0.0 };

    DndStatus {
        enabled, avg_score, threshold, sample_count, window_size,
        duration_secs, dnd_active, os_active,
        exit_duration_secs, below_ticks, exit_window_size, exit_secs_remaining,
        focus_lookback_secs, exit_held_by_lookback,
    }
}

/// Open a native file-picker dialog and return the selected WAV file path.
///
/// Returns `None` if the user cancels.  The dialog is opened on a blocking
/// thread so it does not hold the Tauri async executor.
#[tauri::command]
pub async fn pick_ref_wav_file() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("WAV audio", &["wav"])
            .set_title("Select reference WAV for voice cloning")
            .pick_file()
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .ok()
    .flatten()
}

// ── LLM server configuration ──────────────────────────────────────────────────

/// Return the current LLM server configuration.
///
/// The LLM endpoints (`/v1/*`) are only active when `enabled = true` **and**
/// the binary was compiled with `--features llm`.  Changes take effect on the
/// next app restart.
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
    if let Some(server) = cell.lock().unwrap().clone() {
        *server.allowed_tools.lock().unwrap() = config.tools.clone();
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

// ── Hook distance suggestion ──────────────────────────────────────────────────

/// Percentile distribution of EEG distances, used to suggest a threshold.
#[derive(serde::Serialize)]
pub struct HookDistanceSuggestion {
    /// Number of labels that text-matched at least one keyword.
    pub label_n:    usize,
    /// Number of label EEG reference embeddings with real EEG data.
    pub ref_n:      usize,
    /// Number of recent EEG samples used for the distribution.
    pub sample_n:   usize,
    pub eeg_min:    f32,
    pub eeg_p25:    f32,
    pub eeg_p50:    f32,
    pub eeg_p75:    f32,
    pub eeg_max:    f32,
    /// Suggested `distance_threshold` value (p25 of the distribution).
    pub suggested:  f32,
    /// Human-readable explanation of the suggestion.
    pub note:       String,
}

/// Suggest a `distance_threshold` value by analysing real HNSW and SQLite data.
///
/// Steps:
/// 1. Query `labels.sqlite` for labels that fuzzy-match any of the supplied keywords.
/// 2. Compute the mean EEG embedding for each matched label's time window.
/// 3. Sample up to 300 recent EEG embeddings from `eeg.sqlite` daily files.
/// 4. Compute cosine distance from every sample to every label reference.
/// 5. Return a percentile breakdown + suggested threshold.
/// Suggest a `distance_threshold` value by analysing real HNSW and SQLite data.
///
/// Runs on a blocking thread — involves SQLite queries, EEG embedding sampling,
/// and pairwise distance computation which can take several seconds.
#[tauri::command]
pub async fn suggest_hook_distances(
    keywords: Vec<String>,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<HookDistanceSuggestion, String> {
    let skill_dir = skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        suggest_hook_distances_sync(keywords, &skill_dir)
    })
    .await
    .map_err(|e| e.to_string())
}

fn suggest_hook_distances_sync(
    keywords: Vec<String>,
    skill_dir: &std::path::Path,
) -> HookDistanceSuggestion {
    let empty = HookDistanceSuggestion {
        label_n: 0, ref_n: 0, sample_n: 0,
        eeg_min: 0.0, eeg_p25: 0.0, eeg_p50: 0.0, eeg_p75: 0.0, eeg_max: 0.0,
        suggested: 0.1,
        note: "No label data found. Keep the default 0.1 and adjust after recording sessions with labels."
            .to_owned(),
    };

    let kws: Vec<String> = keywords.iter()
        .map(|k| k.trim().to_owned())
        .filter(|k| !k.is_empty())
        .collect();
    if kws.is_empty() {
        return empty;
    }

    // ── Step 1: find matching labels ─────────────────────────────────────────
    let labels_db = skill_dir.join(crate::constants::LABELS_FILE);
    if !labels_db.exists() {
        return empty;
    }
    let Ok(conn) = rusqlite::Connection::open_with_flags(
        &labels_db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) else {
        return empty;
    };

    let all_labels: Vec<(i64, String, u64, u64)> = {
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, text, eeg_start, eeg_end FROM labels WHERE length(trim(text)) > 0",
        ) else { return empty; };
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    };

    let matched: Vec<(i64, String, u64, u64)> = all_labels
        .into_iter()
        .filter(|(_, text, _, _)| kws.iter().any(|k| crate::eeg_embeddings::fuzzy_match(k, text)))
        .collect();

    let label_n = matched.len();
    if label_n == 0 {
        return HookDistanceSuggestion {
            note: format!(
                "No labels matched your keywords ({kws_fmt}). Add labels to your sessions first.",
                kws_fmt = kws.join(", ")
            ),
            suggested: 0.1,
            ..empty
        };
    }

    // ── Step 2: get mean EEG embeddings for matched labels ───────────────────
    let refs: Vec<Vec<f32>> = matched.iter()
        .filter_map(|(_, _, eeg_start, eeg_end)| {
            crate::label_index::mean_eeg_for_window(skill_dir, *eeg_start, *eeg_end)
        })
        .collect();

    let ref_n = refs.len();
    if ref_n == 0 {
        return HookDistanceSuggestion {
            label_n,
            note: format!(
                "{label_n} label(s) matched but no EEG recordings cover their time windows yet.",
            ),
            suggested: 0.1,
            ..empty
        };
    }

    // ── Step 3: sample recent EEG embeddings ─────────────────────────────────
    let samples = sample_recent_eeg_embeddings(skill_dir, 300);
    let sample_n = samples.len();
    if sample_n == 0 {
        return HookDistanceSuggestion {
            label_n,
            ref_n,
            note: "No recent EEG embeddings found. Record a session first.".to_owned(),
            suggested: 0.1,
            ..empty
        };
    }

    // ── Step 4: compute all pairwise distances ────────────────────────────────
    let mut distances: Vec<f32> = Vec::with_capacity(samples.len() * refs.len());
    for sample in &samples {
        for r in &refs {
            let d = crate::eeg_embeddings::cosine_distance(sample, r);
            if d < 2.0 {
                distances.push(d);
            }
        }
    }
    if distances.is_empty() {
        return HookDistanceSuggestion {
            label_n, ref_n, sample_n,
            note: "Could not compute distances (dimension mismatch).".to_owned(),
            suggested: 0.1,
            ..empty
        };
    }

    // ── Step 5: percentiles ───────────────────────────────────────────────────
    distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = distances.len();
    let percentile = |p: f32| -> f32 {
        let idx = ((p / 100.0) * (n as f32 - 1.0)).round() as usize;
        distances[idx.min(n - 1)]
    };
    let eeg_min = distances[0];
    let eeg_p25 = percentile(25.0);
    let eeg_p50 = percentile(50.0);
    let eeg_p75 = percentile(75.0);
    let eeg_max = *distances.last().unwrap();
    // Suggest p25 rounded to 2 decimal places — catches the closest quarter of hits.
    let suggested = (eeg_p25 * 100.0).round() / 100.0;
    let suggested = suggested.clamp(0.01, 0.99);

    let note = format!(
        "{label_n} label(s) matched ({ref_n} with EEG data). Distribution of {n} \
         distances — min {eeg_min:.3}, p25 {eeg_p25:.3}, median {eeg_p50:.3}, \
         p75 {eeg_p75:.3}, max {eeg_max:.3}. \
         Suggested threshold {suggested:.2} (p25 = fairly strict match).",
    );

    HookDistanceSuggestion { label_n, ref_n, sample_n, eeg_min, eeg_p25, eeg_p50, eeg_p75, eeg_max, suggested, note }
}

/// Read up to `max` EEG embedding blobs from the most-recent daily `eeg.sqlite` files.
fn sample_recent_eeg_embeddings(skill_dir: &std::path::Path, max: usize) -> Vec<Vec<f32>> {
    let mut date_dirs: Vec<std::path::PathBuf> = std::fs::read_dir(skill_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.len() == 8 && name.chars().all(|c| c.is_ascii_digit()) {
                Some(e.path())
            } else {
                None
            }
        })
        .collect();
    date_dirs.sort_by(|a, b| b.cmp(a)); // newest first

    let mut out: Vec<Vec<f32>> = Vec::new();
    let per_day = (max / date_dirs.len().max(1)).max(20);

    for dir in &date_dirs {
        let db = dir.join(crate::constants::SQLITE_FILE);
        if !db.exists() { continue; }
        let Ok(conn) = rusqlite::Connection::open_with_flags(
            &db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) else { continue };

        let Ok(mut stmt) = conn.prepare(
            "SELECT eeg_embedding FROM embeddings ORDER BY timestamp DESC LIMIT ?1",
        ) else { continue };

        let blobs: Vec<Vec<f32>> = stmt
            .query_map(rusqlite::params![per_day as i64], |r| r.get::<_, Vec<u8>>(0))
            .map(|rows| {
                rows.flatten()
                    .map(|b| b.chunks_exact(4)
                        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                        .collect())
                    .collect()
            })
            .unwrap_or_default();

        out.extend(blobs);
        if out.len() >= max { break; }
    }
    out
}

// ── Hook audit log ────────────────────────────────────────────────────────────

/// Return the most-recent hook-fire events from `hooks.sqlite`.
///
/// Runs on a blocking thread — opens and queries a SQLite database.
#[tauri::command]
pub async fn get_hook_log(
    limit:  Option<i64>,
    offset: Option<i64>,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<crate::hooks_log::HookLogRow>, String> {
    let skill_dir = skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        let Some(log) = crate::hooks_log::HooksLog::open(&skill_dir) else {
            return vec![];
        };
        log.query(limit.unwrap_or(50).clamp(1, 500), offset.unwrap_or(0).max(0))
    })
    .await
    .map_err(|e| e.to_string())
}

/// Return the total number of hook-fire events in the audit log.
///
/// Runs on a blocking thread — opens and queries a SQLite database.
#[tauri::command]
pub async fn get_hook_log_count(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Result<i64, String> {
    let skill_dir = skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        crate::hooks_log::HooksLog::open(&skill_dir)
            .map(|l| l.count())
            .unwrap_or(0)
    })
    .await
    .map_err(|e| e.to_string())
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
) -> crate::screenshot_store::ConfigChangeResult {
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
        crate::screenshot_store::ScreenshotStore::open(&skill_dir)
            .map(|s| s.count_stale(&new_backend, &new_model))
            .unwrap_or(0)
    } else {
        0
    };

    crate::screenshot_store::ConfigChangeResult { model_changed, stale_count }
}

/// Count screenshots needing (re-)embedding and estimate wall-clock time.
/// Runs on a background thread to avoid blocking the UI.
#[tauri::command]
pub async fn estimate_screenshot_reembed(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Option<crate::screenshot_store::ReembedEstimate>, String> {
    let (config, skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.screenshot_config.clone(), g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let store = store.or_else(|| crate::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))?;
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
) -> Result<Option<crate::screenshot_store::ReembedResult>, String> {
    let (config, skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.screenshot_config.clone(), g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let store = store.or_else(|| crate::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))?;
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
) -> Result<Vec<crate::screenshot_store::ScreenshotResult>, String> {
    let (skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let store = match store.or_else(|| crate::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new)) {
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
) -> Result<Vec<crate::screenshot_store::ScreenshotResult>, String> {
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
        let store = match store.or_else(|| crate::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new)) {
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
) -> Result<Vec<crate::screenshot_store::ScreenshotResult>, String> {
    let (skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.skill_dir.clone(), g.screenshot_store.clone())
    };
    let embedder = std::sync::Arc::clone(&embedder);
    Ok(tokio::task::spawn_blocking(move || {
        let store = match store.or_else(|| crate::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new)) {
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
) -> Result<Vec<crate::screenshot_store::ScreenshotResult>, String> {
    let (skill_dir, store) = {
        let g = state.lock_or_recover();
        (g.skill_dir.clone(), g.screenshot_store.clone())
    };
    Ok(tokio::task::spawn_blocking(move || {
        let store = match store.or_else(|| crate::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new)) {
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
