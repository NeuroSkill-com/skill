// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Thin Tauri IPC wrappers for session metrics, time-series, sleep staging,
// UMAP comparison, and cross-session analysis.  All heavy computation is
// delegated to the `skill-history` crate and runs on Tokio blocking threads.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{AppState, MutexExt, unix_secs};
use crate::settings::load_umap_config;
use crate::{job_queue, ws_commands};

// Re-export types from skill-history for backward compatibility with callers.
pub(crate) use skill_history::{SessionMetrics, EpochRow, CsvMetricsResult};

// Re-export pure functions for ws_commands and other internal callers.
pub(crate) use skill_history::{
    get_session_metrics as get_session_metrics_impl,
    get_sleep_stages as get_sleep_stages_impl,
    compute_compare_insights,
    analyze_sleep_stages,
    analyze_search_results,
};

/// Wrapper that supplies `now_utc` automatically for callers that used the old signature.
pub(crate) fn compute_status_history(
    skill_dir: &std::path::Path,
    sessions_json: &[serde_json::Value],
) -> serde_json::Value {
    skill_history::compute_status_history(skill_dir, unix_secs(), sessions_json)
}

// Re-export from skill_router (UMAP embeddings).
pub(crate) use skill_router::load_embeddings_range;

// ── UMAP types ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
struct UmapPoint {
    x: f32, y: f32, z: f32,
    session: u8,
    utc: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub(crate) struct UmapResult {
    points:  Vec<UmapPoint>,
    n_a:     usize,
    n_b:     usize,
    dim:     usize,
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn get_sleep_stages(
    start_utc: u64, end_utc: u64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<skill_history::SleepStages, String> {
    let skill_dir = crate::skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        skill_history::get_sleep_stages(&skill_dir, start_utc, end_utc)
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn get_session_metrics(
    start_utc: u64, end_utc: u64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<SessionMetrics, String> {
    let skill_dir = crate::skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        skill_history::get_session_metrics(&skill_dir, start_utc, end_utc)
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn get_session_timeseries(
    start_utc: u64, end_utc: u64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<EpochRow>, String> {
    let skill_dir = crate::skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        skill_history::get_session_timeseries(&skill_dir, start_utc, end_utc)
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn get_csv_metrics(csv_path: String) -> Result<Option<CsvMetricsResult>, String> {
    tokio::task::spawn_blocking(move || {
        skill_history::load_csv_metrics_cached(std::path::Path::new(&csv_path))
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn get_day_metrics_batch(
    csv_paths: Vec<String>,
    max_ts_points: Option<usize>,
) -> Result<std::collections::HashMap<String, CsvMetricsResult>, String> {
    tokio::task::spawn_blocking(move || {
        skill_history::get_day_metrics_batch(&csv_paths, max_ts_points.unwrap_or(360))
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn compute_umap_compare(
    a_start_utc: u64, a_end_utc: u64,
    b_start_utc: u64, b_end_utc: u64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> UmapResult {
    let skill_dir = crate::skill_dir(&state);
    match ws_commands::umap_compute_inner(&skill_dir, a_start_utc, a_end_utc, b_start_utc, b_end_utc, None) {
        Ok(val) => serde_json::from_value(val).unwrap_or_default(),
        Err(e) => { eprintln!("[umap] compute error: {e}"); UmapResult::default() }
    }
}

#[tauri::command]
pub(crate) fn enqueue_umap_compare(
    a_start_utc: u64, a_end_utc: u64,
    b_start_utc: u64, b_end_utc: u64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
    queue: tauri::State<'_, std::sync::Arc<job_queue::JobQueue>>,
) -> job_queue::JobTicket {
    let skill_dir = crate::skill_dir(&state);
    let n_a = load_embeddings_range(&skill_dir, a_start_utc, a_end_utc).len();
    let n_b = load_embeddings_range(&skill_dir, b_start_utc, b_end_utc).len();
    let n = n_a + n_b;
    let ucfg = load_umap_config(&skill_dir);
    let est_epochs = ucfg.n_epochs.clamp(50, 2000) as u64;
    let estimated_ms = 3000u64 + (n as u64) * (n as u64) / 20_000 + (n as u64) * est_epochs / 2000;

    let sd = skill_dir.clone();
    let prog_map = queue.progress_map();
    queue.submit_with_id(estimated_ms, move |job_id| {
        let pm = prog_map;
        let cb: Box<dyn Fn(fast_umap::EpochProgress) + Send> = Box::new(move |ep| {
            let mut map = pm.lock_or_recover();
            map.insert(job_id, job_queue::JobProgress {
                epoch: ep.epoch, total_epochs: ep.total_epochs,
                loss: ep.loss, best_loss: ep.best_loss,
                elapsed_secs: ep.elapsed_secs, epoch_ms: ep.epoch_ms,
            });
        });
        ws_commands::umap_compute_inner(&sd, a_start_utc, a_end_utc, b_start_utc, b_end_utc, Some(cb))
    })
}

#[tauri::command]
pub(crate) fn poll_job(
    job_id: u64,
    queue: tauri::State<'_, std::sync::Arc<job_queue::JobQueue>>,
) -> job_queue::JobPollResult {
    queue.poll(job_id)
}

#[tauri::command]
pub(crate) async fn open_compare_window(app: AppHandle) -> Result<(), String> {
    crate::window_cmds::focus_or_create(&app, crate::window_cmds::WindowSpec {
        label: "compare", route: "compare", title: "NeuroSkill™ – Compare",
        inner_size: (780.0, 640.0), min_inner_size: Some((600.0, 440.0)),
        ..Default::default()
    })
}

#[tauri::command]
pub(crate) async fn open_compare_window_with_sessions(
    app: AppHandle,
    start_a: i64, end_a: i64,
    start_b: i64, end_b: i64,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("compare") {
        let _ = win.close();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let url_path = format!("compare?startA={}&endA={}&startB={}&endB={}", start_a, end_a, start_b, end_b);
    tauri::WebviewWindowBuilder::new(&app, "compare", tauri::WebviewUrl::App(url_path.into()))
        .title("NeuroSkill™ – Compare")
        .inner_size(780.0, 640.0).min_inner_size(600.0, 440.0)
        .resizable(true).center()
        .decorations(false).transparent(true)
        .build()
        .map(|w| { let _ = w.set_focus(); })
        .map_err(|e| e.to_string())
}
