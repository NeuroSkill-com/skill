// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Thin Tauri IPC wrappers for session metrics, time-series, sleep staging,
// UMAP comparison, and cross-session analysis.  All heavy computation is
// delegated to the `skill-history` crate and runs on Tokio blocking threads.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::settings::load_umap_config;
use crate::{job_queue, ws_commands};
use crate::{unix_secs, AppState, MutexExt};

// Re-export types from skill-history for backward compatibility with callers.
pub(crate) use skill_history::{CsvMetricsResult, EpochRow, SessionMetrics};

// Re-export pure functions for ws_commands and other internal callers.
pub(crate) use skill_history::{
    analyze_search_results, analyze_sleep_stages, compute_compare_insights,
    get_session_metrics as get_session_metrics_impl, get_sleep_stages as get_sleep_stages_impl,
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
    x: f32,
    y: f32,
    z: f32,
    session: u8,
    utc: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub(crate) struct UmapResult {
    points: Vec<UmapPoint>,
    n_a: usize,
    n_b: usize,
    dim: usize,
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn get_sleep_stages(
    start_utc: u64,
    end_utc: u64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<skill_history::SleepStages, String> {
    let skill_dir = crate::skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        skill_history::get_sleep_stages(&skill_dir, start_utc, end_utc)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn get_session_metrics(
    start_utc: u64,
    end_utc: u64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<SessionMetrics, String> {
    let skill_dir = crate::skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        skill_history::get_session_metrics(&skill_dir, start_utc, end_utc)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn get_session_timeseries(
    start_utc: u64,
    end_utc: u64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<EpochRow>, String> {
    let skill_dir = crate::skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        skill_history::get_session_timeseries(&skill_dir, start_utc, end_utc)
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn get_csv_metrics(csv_path: String) -> Result<Option<CsvMetricsResult>, String> {
    tokio::task::spawn_blocking(move || {
        skill_history::load_csv_metrics_cached(std::path::Path::new(&csv_path))
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn get_day_metrics_batch(
    csv_paths: Vec<String>,
    max_ts_points: Option<usize>,
) -> Result<std::collections::HashMap<String, CsvMetricsResult>, String> {
    tokio::task::spawn_blocking(move || {
        skill_history::get_day_metrics_batch(&csv_paths, max_ts_points.unwrap_or(360))
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn compute_umap_compare(
    a_start_utc: u64,
    a_end_utc: u64,
    b_start_utc: u64,
    b_end_utc: u64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> UmapResult {
    let skill_dir = crate::skill_dir(&state);
    match ws_commands::umap_compute_inner(
        &skill_dir,
        a_start_utc,
        a_end_utc,
        b_start_utc,
        b_end_utc,
        None,
    ) {
        Ok(val) => serde_json::from_value(val).unwrap_or_default(),
        Err(e) => {
            eprintln!("[umap] compute error: {e}");
            UmapResult::default()
        }
    }
}

#[tauri::command]
pub(crate) fn enqueue_umap_compare(
    a_start_utc: u64,
    a_end_utc: u64,
    b_start_utc: u64,
    b_end_utc: u64,
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
            map.insert(
                job_id,
                job_queue::JobProgress {
                    epoch: ep.epoch,
                    total_epochs: ep.total_epochs,
                    loss: ep.loss,
                    best_loss: ep.best_loss,
                    elapsed_secs: ep.elapsed_secs,
                    epoch_ms: ep.epoch_ms,
                },
            );
        });
        ws_commands::umap_compute_inner(
            &sd,
            a_start_utc,
            a_end_utc,
            b_start_utc,
            b_end_utc,
            Some(cb),
        )
        .map_err(|e| e.to_string())
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
    crate::window_cmds::focus_or_create(
        &app,
        crate::window_cmds::WindowSpec {
            label: "compare",
            route: "compare",
            title: "NeuroSkill™ – Compare",
            inner_size: (780.0, 640.0),
            min_inner_size: Some((600.0, 440.0)),
            ..Default::default()
        },
    )
}

#[tauri::command]
pub(crate) async fn open_compare_window_with_sessions(
    app: AppHandle,
    start_a: i64,
    end_a: i64,
    start_b: i64,
    end_b: i64,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("compare") {
        let _ = win.close();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let url_path = format!(
        "compare?startA={}&endA={}&startB={}&endB={}",
        start_a, end_a, start_b, end_b
    );
    tauri::WebviewWindowBuilder::new(&app, "compare", tauri::WebviewUrl::App(url_path.into()))
        .title("NeuroSkill™ – Compare")
        .inner_size(780.0, 640.0)
        .min_inner_size(600.0, 440.0)
        .resizable(true)
        .center()
        .decorations(false)
        .transparent(true)
        .build()
        .map(|w| {
            let _ = w.set_focus();
        })
        .map_err(|e| e.to_string())
}

// ── GPS track for a session ───────────────────────────────────────────────────

/// A single GPS fix — latitude/longitude in WGS-84 degrees, altitude in metres,
/// speed in m/s, and Unix-second timestamp.
#[derive(Serialize, Clone, Debug)]
pub(crate) struct GpsPoint {
    pub ts: f64,
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub accuracy: f64,
    pub speed: f64,
}

/// Return GPS track points for a session.
///
/// Sources (merged, sorted by timestamp, deduplicated):
/// 1. `*.meta.jsonl` sidecar alongside the session CSV — written in real time
///    by the Iroh remote phone proxy for every CoreLocation fix received.
/// 2. `location_samples` table in `~/.skill/skill_health.sqlite` — populated
///    by the iOS HealthKit / CoreLocation sync push (feature-gated: `gps`).
///
/// Returns an empty vec if neither source has data for the given window, so the
/// frontend can safely skip rendering the map card.
#[tauri::command]
pub(crate) async fn get_session_location(
    csv_path: String,
    start_utc: u64,
    end_utc: u64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<GpsPoint>, String> {
    let skill_dir = crate::skill_dir(&state);

    tokio::task::spawn_blocking(move || {
        let mut points: Vec<GpsPoint> = Vec::new();

        // ── Source 1: *.meta.jsonl sidecar ───────────────────────────────
        let sidecar = std::path::Path::new(&csv_path).with_extension("meta.jsonl");
        if let Ok(text) = std::fs::read_to_string(&sidecar) {
            for line in text.lines() {
                let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                if val.get("type").and_then(|v| v.as_str()) != Some("location") {
                    continue;
                }
                let ts = val["timestamp"].as_f64().unwrap_or(0.0);
                if ts < start_utc as f64 || ts > end_utc as f64 {
                    continue;
                }
                let lat = val["latitude"].as_f64().unwrap_or(f64::NAN);
                let lon = val["longitude"].as_f64().unwrap_or(f64::NAN);
                if !lat.is_finite() || !lon.is_finite() {
                    continue;
                }
                if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                    continue;
                }
                points.push(GpsPoint {
                    ts,
                    lat,
                    lon,
                    alt: val["altitude"].as_f64().unwrap_or(0.0),
                    accuracy: val["accuracy"].as_f64().unwrap_or(0.0),
                    speed: val["speed"].as_f64().unwrap_or(0.0),
                });
            }
        }

        // ── Source 2: health SQLite location_samples (gps feature) ───────
        #[cfg(feature = "gps")]
        {
            let db_path = skill_dir.join("skill_health.sqlite");
            if let Ok(conn) = rusqlite::Connection::open_with_flags(
                &db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            ) {
                let sql = "SELECT timestamp, latitude, longitude, altitude, accuracy, speed \
                           FROM location_samples \
                           WHERE timestamp >= ?1 AND timestamp <= ?2 \
                           ORDER BY timestamp ASC";
                if let Ok(mut stmt) = conn.prepare(sql) {
                    let rows =
                        stmt.query_map(rusqlite::params![start_utc as i64, end_utc as i64], |r| {
                            Ok(GpsPoint {
                                ts: r.get::<_, i64>(0)? as f64,
                                lat: r.get(1)?,
                                lon: r.get(2)?,
                                alt: r.get::<_, f64>(3).unwrap_or(0.0),
                                accuracy: r.get::<_, f64>(4).unwrap_or(0.0),
                                speed: r.get::<_, f64>(5).unwrap_or(0.0),
                            })
                        });
                    if let Ok(iter) = rows {
                        for row in iter.flatten() {
                            // Skip duplicates already in the sidecar (same ts ±1 s)
                            let already = points.iter().any(|p| (p.ts - row.ts).abs() < 1.0);
                            if !already {
                                points.push(row);
                            }
                        }
                    }
                }
            }
        }
        let _ = skill_dir; // keep alive for cfg(not(feature="gps")) builds

        points.sort_by(|a, b| a.ts.partial_cmp(&b.ts).unwrap_or(std::cmp::Ordering::Equal));
        Ok(points)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── HNSW / SQLite embedding count for a session ───────────────────────────────

/// Count EEG embeddings stored in the per-day SQLite database for the
/// given session time window.  Includes both full-embedding rows (hnsw_id > 0)
/// and metrics-only rows (hnsw_id = 0), so the number reflects every epoch
/// that reached the embed worker regardless of GPU availability.
#[tauri::command]
pub(crate) async fn get_session_embedding_count(
    start_utc: u64,
    end_utc: u64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<u64, String> {
    let skill_dir = crate::skill_dir(&state);

    tokio::task::spawn_blocking(move || {
        let start_ms = start_utc as i64 * 1000;
        let end_ms = end_utc as i64 * 1000;

        // The embed worker writes into the day directory whose name matches
        // the UTC date of each epoch.  A session straddling midnight spans
        // two day dirs — query both and sum.
        // Convert a Unix timestamp to a UTC YYYYMMDD string matching the
        // day-directory names written by the embed worker.
        fn utc_dir(ts: u64) -> String {
            let days = ts / 86400;
            // Gregorian calendar date from Julian Day Number (JDN = days + 2440588)
            let jdn = days as i64 + 2_440_588;
            let a = jdn + 32_044;
            let b = (4 * a + 3) / 146_097;
            let c = a - (146_097 * b) / 4;
            let d = (4 * c + 3) / 1_461;
            let e = c - (1_461 * d) / 4;
            let m = (5 * e + 2) / 153;
            let day = e - (153 * m + 2) / 5 + 1;
            let month = m + 3 - 12 * (m / 10);
            let year = 100 * b + d - 4_800 + m / 10;
            format!("{year:04}{month:02}{day:02}")
        }
        let dir1 = utc_dir(start_utc);
        let dir2 = utc_dir(end_utc);

        let mut total: u64 = 0;
        for dir_name in std::collections::HashSet::from([dir1, dir2]) {
            let db = skill_dir.join(&dir_name).join("eeg.sqlite");
            if !db.exists() {
                continue;
            }
            if let Ok(conn) = rusqlite::Connection::open_with_flags(
                &db,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            ) {
                let n: u64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM embeddings \
                         WHERE timestamp >= ?1 AND timestamp <= ?2",
                        rusqlite::params![start_ms, end_ms],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                total += n;
            }
        }
        Ok(total)
    })
    .await
    .map_err(|e| e.to_string())?
}
