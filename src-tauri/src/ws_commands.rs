// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! WebSocket command handlers.
//!
//! Each `pub` function here corresponds to a JSON `"command"` that a
//! WebSocket client can send.  The dispatcher in [`super::ws_server`] calls
//! into this module and forwards the `Result` back to the client.

use std::sync::Mutex;
use crate::MutexExt;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use serde::Serialize;

use crate::constants::SQLITE_FILE;
use crate::eeg_model_config::LatestEpochMetrics;
use crate::{AppState, unix_secs};

// ── rounded scores helper ─────────────────────────────────────────────────────

fn r1(v: f32) -> f32 { (v * 10.0).round() / 10.0 }
fn r2(v: f32) -> f32 { (v * 100.0).round() / 100.0 }
fn r3(v: f32) -> f32 { (v * 1000.0).round() / 1000.0 }
fn r1d(v: f64) -> f64 { (v * 10.0).round() / 10.0 }
fn r2d(v: f64) -> f64 { (v * 100.0).round() / 100.0 }

#[derive(Serialize)]
struct RoundedBands {
    rel_delta: f32,
    rel_theta: f32,
    rel_alpha: f32,
    rel_beta:  f32,
    rel_gamma: f32,
}

#[derive(Serialize)]
struct RoundedScores {
    relaxation: f32,
    engagement: f32,
    faa: f32,
    tar: f32,
    bar: f32,
    dtr: f32,
    pse: f32,
    apf: f32,
    bps: f32,
    snr: f32,
    coherence: f32,
    mu_suppression: f32,
    mood: f32,
    tbr: f32,
    sef95: f32,
    spectral_centroid: f32,
    hjorth_activity: f32,
    hjorth_mobility: f32,
    hjorth_complexity: f32,
    permutation_entropy: f32,
    higuchi_fd: f32,
    dfa_exponent: f32,
    sample_entropy: f32,
    pac_theta_gamma: f32,
    laterality_index: f32,
    hr: f64,
    rmssd: f64,
    sdnn: f64,
    pnn50: f64,
    lf_hf_ratio: f64,
    respiratory_rate: f64,
    spo2_estimate: f64,
    perfusion_index: f64,
    stress_index: f64,
    // Artifact detection
    blink_count: u64,
    blink_rate: f64,
    // Head pose
    head_pitch: f64,
    head_roll: f64,
    stillness: f64,
    nod_count: u64,
    shake_count: u64,
    // Composite scores
    meditation: f64,
    cognitive_load: f64,
    drowsiness: f64,
    bands: RoundedBands,
    epoch_timestamp: i64,
}

impl From<&LatestEpochMetrics> for RoundedScores {
    fn from(m: &LatestEpochMetrics) -> Self {
        Self {
            relaxation: r1(m.relaxation_score),
            engagement: r1(m.engagement_score),
            faa: r3(m.faa),
            tar: r3(m.tar),
            bar: r3(m.bar),
            dtr: r3(m.dtr),
            pse: r3(m.pse),
            apf: r2(m.apf),
            bps: r3(m.bps),
            snr: r1(m.snr),
            coherence: r3(m.coherence),
            mu_suppression: r3(m.mu_suppression),
            mood: r1(m.mood),
            tbr: r3(m.tbr),
            sef95: r2(m.sef95),
            spectral_centroid: r2(m.spectral_centroid),
            hjorth_activity: r3(m.hjorth_activity),
            hjorth_mobility: r3(m.hjorth_mobility),
            hjorth_complexity: r3(m.hjorth_complexity),
            permutation_entropy: r3(m.permutation_entropy),
            higuchi_fd: r3(m.higuchi_fd),
            dfa_exponent: r3(m.dfa_exponent),
            sample_entropy: r3(m.sample_entropy),
            pac_theta_gamma: r3(m.pac_theta_gamma),
            laterality_index: r3(m.laterality_index),
            hr: r1d(m.hr),
            rmssd: r1d(m.rmssd),
            sdnn: r1d(m.sdnn),
            pnn50: r1d(m.pnn50),
            lf_hf_ratio: r2d(m.lf_hf_ratio),
            respiratory_rate: r1d(m.respiratory_rate),
            spo2_estimate: r1d(m.spo2_estimate),
            perfusion_index: r2d(m.perfusion_index),
            stress_index: r1d(m.stress_index),
            blink_count: m.blink_count,
            blink_rate: r1d(m.blink_rate),
            head_pitch: r1d(m.head_pitch),
            head_roll: r1d(m.head_roll),
            stillness: r1d(m.stillness),
            nod_count: m.nod_count,
            shake_count: m.shake_count,
            meditation: r1d(m.meditation),
            cognitive_load: r1d(m.cognitive_load),
            drowsiness: r1d(m.drowsiness),
            bands: RoundedBands {
                rel_delta: r3(m.rel_delta),
                rel_theta: r3(m.rel_theta),
                rel_alpha: r3(m.rel_alpha),
                rel_beta: r3(m.rel_beta),
                rel_gamma: r3(m.rel_gamma),
            },
            epoch_timestamp: m.epoch_timestamp,
        }
    }
}

// ── status ────────────────────────────────────────────────────────────────────

pub fn status(app: &AppHandle) -> Result<Value, String> {
    let st = app.state::<Mutex<AppState>>();
    let guard = st.lock_or_recover();

    // ── Device / connection ──────────────────────────────────────────────────
    let status       = &guard.status;
    let connected    = status.state == "connected";
    let streaming    = connected && guard.stream.is_some();
    let sample_count     = status.sample_count;
    let ppg_sample_count = status.ppg_sample_count;
    let ppg              = status.ppg.clone();

    // ── Session ──────────────────────────────────────────────────────────────
    let session_start_utc = guard.session_start_utc;
    let session_duration_secs = session_start_utc.map(|s| unix_secs().saturating_sub(s));

    // ── Embeddings (today + all-time) ────────────────────────────────────────
    let model_status     = guard.model_status.lock_or_recover().clone();
    let embeddings_today = model_status.embeddings_today;
    let encoder_loaded   = model_status.encoder_loaded;
    let latest_metrics   = model_status.latest_metrics.clone();

    let skill_dir = guard.skill_dir.clone();

    // Label count and recent labels from the database.
    let label_count  = guard.label_store.as_ref().map(|ls| ls.count()).unwrap_or(0);
    let recent_labels: Vec<serde_json::Value> = guard.label_store.as_ref()
        .map(|ls| ls.recent(5).into_iter().map(|r| serde_json::json!({
            "id":         r.id,
            "text":       r.text,
            "created_at": r.created_at,
        })).collect())
        .unwrap_or_default();

    // ── Calibration ──────────────────────────────────────────────────────────
    let last_calibration_utc = {
        let active_id = &guard.active_calibration_id;
        guard.calibration_profiles.iter()
            .find(|p| &p.id == active_id)
            .or_else(|| guard.calibration_profiles.first())
            .and_then(|p| p.last_calibration_utc)
    };

    // ── Signal quality ───────────────────────────────────────────────────────
    let channel_quality = status.channel_quality.clone();

    // Snapshot scalars before dropping the lock.
    let state_str   = status.state.clone();
    let battery     = status.battery;
    let device_name = status.device_name.clone();
    let device_id   = status.device_id.clone();
    let serial_number          = status.serial_number.clone();
    let firmware_version       = status.firmware_version.clone();
    let hardware_version       = status.hardware_version.clone();
    let bootloader_version     = status.bootloader_version.clone();
    let headset_preset         = status.headset_preset.clone();
    let mac_address            = status.mac_address.clone();
    let embedding_overlap_secs = status.embedding_overlap_secs;
    let retry_attempt          = status.retry_attempt;
    let retry_countdown_secs   = status.retry_countdown_secs;
    let accel                  = status.accel;
    let gyro                   = status.gyro;
    let fuel_gauge_mv          = status.fuel_gauge_mv;
    let temperature_raw        = status.temperature_raw;
    drop(guard);

    // ── Embedding totals (filesystem scan, outside lock) ─────────────────────
    let mut total_embeddings: u64 = 0;
    let mut recording_days:   u64 = 0;
    if let Ok(rd) = std::fs::read_dir(&skill_dir) {
        for entry in rd.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.len() == 8 && name.bytes().all(|b| b.is_ascii_digit()) && entry.path().is_dir() {
                let db_path = entry.path().join(SQLITE_FILE);
                if db_path.exists() {
                    recording_days += 1;
                    // Count embeddings via SQLite (lightweight, no HNSW load).
                    if let Ok(conn) = rusqlite::Connection::open_with_flags(
                        &db_path,
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
                    ) {
                        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
                        let n: i64 = conn
                            .query_row("SELECT COUNT(*) FROM embeddings", [], |row| row.get(0))
                            .unwrap_or(0);
                        total_embeddings += n as u64;
                    }
                }
            }
        }
    }

    // ── Sleep staging (past 48 h) ───────────────────────────────────────────
    let now_utc = unix_secs();
    let sleep_48h = crate::get_sleep_stages_impl(
        &skill_dir,
        now_utc.saturating_sub(48 * 3600),
        now_utc,
    );
    let sleep_json = serde_json::json!({
        "window_hours": 48,
        "total_epochs":  sleep_48h.summary.total_epochs,
        "epoch_secs":    sleep_48h.summary.epoch_secs,
        "wake_epochs":   sleep_48h.summary.wake_epochs,
        "n1_epochs":     sleep_48h.summary.n1_epochs,
        "n2_epochs":     sleep_48h.summary.n2_epochs,
        "n3_epochs":     sleep_48h.summary.n3_epochs,
        "rem_epochs":    sleep_48h.summary.rem_epochs,
    });

    // ── Recording history (totals, streak, today vs 7-day avg) ───────────
    let history_json = {
        let sess = sessions(app).unwrap_or(serde_json::json!({"sessions": []}));
        let arr = sess["sessions"].as_array().cloned().unwrap_or_default();
        crate::compute_status_history(&skill_dir, &arr)
    };

    Ok(serde_json::json!({
        "device": {
            "state":              state_str,
            "connected":          connected,
            "streaming":          streaming,
            "name":               device_name,
            "id":                 device_id,
            "serial_number":      serial_number,
            "mac_address":        mac_address,
            "firmware_version":   firmware_version,
            "hardware_version":   hardware_version,
            "bootloader_version": bootloader_version,
            "preset":             headset_preset,
            "battery":              battery,
            "sample_count":        sample_count,
            "ppg_sample_count":    ppg_sample_count,
            "ppg":                 ppg,
            "retry_attempt":       retry_attempt,
            "retry_countdown_secs": retry_countdown_secs,
            "accel":               accel,
            "gyro":                gyro,
            "fuel_gauge_mv":       fuel_gauge_mv,
            "temperature_raw":     temperature_raw,
        },
        "session": {
            "start_utc":     session_start_utc,
            "duration_secs": session_duration_secs,
        },
        "embeddings": {
            "today":            embeddings_today,
            "total":            total_embeddings,
            "recording_days":   recording_days,
            "encoder_loaded":   encoder_loaded,
            "overlap_secs":     embedding_overlap_secs,
        },
        "labels": {
            "total":  label_count,
            "recent": recent_labels,
        },
        "calibration": {
            "last_calibration_utc": last_calibration_utc,
        },
        "signal_quality": channel_quality,
        "sleep": sleep_json,
        "scores": match latest_metrics {
            Some(ref m) => serde_json::to_value(RoundedScores::from(m))
                .unwrap_or(Value::Null),
            None => Value::Null,
        },
        "history": history_json,
    }))
}

// ── calibrate ─────────────────────────────────────────────────────────────────

pub async fn calibrate(app: &AppHandle) -> Result<Value, String> {
    crate::open_calibration_window_inner(app, None, false).await?;
    Ok(serde_json::json!({}))
}

// ── timer ─────────────────────────────────────────────────────────────────────

/// `timer` — open the focus-timer window and auto-start the work phase.
///
/// If the window is already open it is brought to the front and a
/// `focus-timer-start` Tauri event is emitted so the running page starts
/// immediately without reloading.
pub async fn timer(app: &AppHandle) -> Result<Value, String> {
    crate::window_cmds::open_focus_timer_window_inner(app, true).await?;
    Ok(serde_json::json!({}))
}

// ── notify ────────────────────────────────────────────────────────────────────

/// `notify { "title": "…", "body": "…" }` — show a native OS notification.
/// `body` is optional; `title` is required.
pub fn notify(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    use tauri_plugin_notification::NotificationExt;

    let title = msg.get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"title\" (string)".to_string())?;
    let body = msg.get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({}))
}

// ── label ─────────────────────────────────────────────────────────────────────

pub fn label(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let text = msg.get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"text\" (string)".to_string())?;

    let text = text.trim().to_owned();
    if text.is_empty() {
        return Err("\"text\" must not be empty".into());
    }

    // label_start_utc: optional — defaults to "now".
    let label_start_utc = msg.get("label_start_utc")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(unix_secs);

    let context = msg.get("context")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_owned();

    let s = app.state::<Mutex<AppState>>();
    let guard = s.lock_or_recover();
    let now = unix_secs();
    match &guard.label_store {
        Some(store) => {
            let id = store
                .insert(label_start_utc, now, label_start_utc, now, &text, &context, now)
                .ok_or_else(|| "database insert failed".to_string())?;
            drop(guard);
            let _ = app.emit("label-created", serde_json::json!({
                "text": text, "context": context, "label_id": id,
            }));
            Ok(serde_json::json!({ "label_id": id }))
        }
        None => Err("label store not available".into()),
    }
}

// ── search_labels ─────────────────────────────────────────────────────────────

/// Search labels by free-text query using the fastembed HNSW index.
///
/// **Required**
/// - `query` — the text to embed and search for
///
/// **Optional**
/// - `k` — number of results to return (default 10, max 100)
/// - `ef` — HNSW ef parameter (default max(k×4, 64))
/// - `mode` — which field(s) to search against (default `"text"`):
///     - `"text"` — match against the label's short text embedding
///     - `"context"` — match against the label's long context embedding
///     - `"both"` — run both searches and merge by best distance
///
/// **Response**
/// ```json
/// {
///   "ok": true,
///   "command": "search_labels",
///   "query": "...",
///   "mode": "text",
///   "results": [
///     {
///       "label_id": 42,
///       "text": "...",
///       "context": "...",
///       "distance": 0.12,
///       "similarity": 0.88,
///       "eeg_start": 1700000000,
///       "eeg_end":   1700000300,
///       "created_at": 1700000010,
///       "embedding_model": "Xenova/bge-small-en-v1.5"
///     }
///   ]
/// }
/// ```
pub fn search_labels(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let query = msg.get("query")
        .and_then(|v| v.as_str())
        .ok_or("missing required field: \"query\" (string)")?
        .trim()
        .to_owned();
    if query.is_empty() {
        return Err("\"query\" must not be empty".into());
    }

    let k    = msg.get("k") .and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(10).clamp(1, 100);
    let ef   = msg.get("ef").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or_else(|| (k * 4).max(64));
    let mode = msg.get("mode").and_then(|v| v.as_str()).unwrap_or("text");

    match mode {
        "text" | "context" | "both" => {}
        other => return Err(format!("invalid mode \"{other}\": must be \"text\", \"context\", or \"both\"")),
    }

    let (skill_dir, model_code) = {
        let state = app.state::<Mutex<AppState>>();
        let g = state.lock_or_recover();
        (g.skill_dir.clone(), g.text_embedding_model.clone())
    };

    // Embed the query synchronously — ws_commands are called from a blocking
    // context (tokio spawn_blocking) so this is safe.
    let embedder_arc = std::sync::Arc::clone(
        &*app.state::<std::sync::Arc<crate::label_cmds::EmbedderState>>()
    );
    let label_idx_arc = std::sync::Arc::clone(
        &*app.state::<std::sync::Arc<crate::label_index::LabelIndexState>>()
    );

    let query_vec: Vec<f32> = {
        let mut guard = embedder_arc.0.lock_or_recover();
        let te = guard.as_mut().ok_or("text embedder not initialised — model may still be downloading")?;
        let mut vecs = te.embed(vec![query.as_str()], None).map_err(|e| e.to_string())?;
        vecs.remove(0)
    };

    // Run whichever index searches are required.
    let merged: Vec<crate::label_index::LabelNeighbor> = match mode {
        "text" => crate::label_index::search_by_text_vec(&query_vec, k, ef, &skill_dir, &label_idx_arc),
        "context" => crate::label_index::search_by_context_vec(&query_vec, k, ef, &skill_dir, &label_idx_arc),
        _ => {
            let mut text_hits    = crate::label_index::search_by_text_vec   (&query_vec, k, ef, &skill_dir, &label_idx_arc);
            let mut context_hits = crate::label_index::search_by_context_vec(&query_vec, k, ef, &skill_dir, &label_idx_arc);
            // Merge: keep best distance per label_id, then sort and cap at k.
            text_hits.append(&mut context_hits);
            text_hits.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
            text_hits.dedup_by_key(|n| n.label_id);
            text_hits.truncate(k);
            text_hits
        }
    };

    // Enrich: add similarity (= 1 − distance for cosine) to each result.
    let results: Vec<Value> = merged.iter().map(|n| serde_json::json!({
        "label_id":        n.label_id,
        "text":            n.text,
        "context":         n.context,
        "distance":        n.distance,
        "similarity":      (1.0 - n.distance).clamp(0.0, 1.0),
        "eeg_start":       n.eeg_start,
        "eeg_end":         n.eeg_end,
        "created_at":      n.created_at,
        "embedding_model": n.embedding_model,
        "eeg_metrics":     n.eeg_metrics,
    })).collect();

    Ok(serde_json::json!({
        "query":  query,
        "mode":   mode,
        "model":  model_code,
        "k":      k,
        "count":  results.len(),
        "results": results,
    }))
}

// ── search ────────────────────────────────────────────────────────────────────

pub fn search(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let start_utc = msg.get("start_utc")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing required field: \"start_utc\" (u64, unix seconds)".to_string())?;

    let end_utc = msg.get("end_utc")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing required field: \"end_utc\" (u64, unix seconds)".to_string())?;

    if end_utc < start_utc {
        return Err("\"end_utc\" must be >= \"start_utc\"".into());
    }

    let k  = msg.get("k").and_then(|v| v.as_u64()).map(|v| v as usize);
    let ef = msg.get("ef").and_then(|v| v.as_u64()).map(|v| v as usize);

    let skill_dir = {
        let s = app.state::<Mutex<AppState>>();
        let dir = s.lock_or_recover().skill_dir.clone();
        dir
    };

    let k  = k.unwrap_or(10).clamp(1, 100);
    let ef = ef.unwrap_or(k.max(50));

    // Pull the global cross-day HNSW index from Tauri managed state so the
    // WebSocket search path gets the same accelerated backend as the UI.
    let global_arc = {
        use tauri::Manager as _;
        app.state::<std::sync::Arc<crate::global_eeg_index::GlobalEegIndex>>()
           .inner()
           .arc()
    };
    let result = crate::commands::search_embeddings_in_range(
        &skill_dir, start_utc, end_utc, k, ef, Some(global_arc),
    );

    let analysis = crate::analyze_search_results(&result);
    let mut result_json = serde_json::to_value(&result)
        .map_err(|e| format!("serialization error: {e}"))?;
    if let Some(obj) = result_json.as_object_mut() {
        obj.insert("analysis".into(), analysis);
    }
    Ok(serde_json::json!({ "result": result_json }))
}

// ── compare ───────────────────────────────────────────────────────────────────

/// Compare two time ranges by returning aggregated band-power metrics for each.
/// Return aggregated metrics for a single time range plus first/second-half
/// trend directions — lightweight alternative to `compare` that does not
/// enqueue a UMAP job.
///
/// Required parameters: `start_utc`, `end_utc` (unix seconds).
///
/// Returns `{ "metrics": SessionMetrics, "first": SessionMetrics,
///            "second": SessionMetrics, "trends": { metric: "up"|"down"|"flat" } }`.
pub fn session_metrics(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let start = msg.get("start_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"start_utc\" (u64)")?;
    let end = msg.get("end_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"end_utc\" (u64)")?;
    if end < start { return Err("\"end_utc\" must be >= \"start_utc\"".into()); }

    let skill_dir = app.state::<Mutex<AppState>>().lock_or_recover().skill_dir.clone();
    let mid = (start + end) / 2;

    let full   = crate::get_session_metrics_impl(&skill_dir, start, end);
    let first  = crate::get_session_metrics_impl(&skill_dir, start, mid);
    let second = crate::get_session_metrics_impl(&skill_dir, mid,   end);

    // Direction: "up" if second-half > first-half by >5%, "down" if <-5%, else "flat".
    let dir = |a: f64, b: f64| -> &'static str {
        if a == 0.0 && b == 0.0 { return "flat"; }
        let rel = if a.abs() > 1e-9 { (b - a) / a.abs() } else { 0.0 };
        if rel >  0.05 { "up" } else if rel < -0.05 { "down" } else { "flat" }
    };

    // Compute trend direction for every numeric field dynamically so that
    // new SessionMetrics fields are covered automatically.
    let first_json  = serde_json::to_value(&first).unwrap_or_default();
    let second_json = serde_json::to_value(&second).unwrap_or_default();
    let mut trends  = serde_json::Map::new();
    if let (Some(fo), Some(so)) = (first_json.as_object(), second_json.as_object()) {
        for (key, fv) in fo {
            if key == "n_epochs" { continue; }
            if let (Some(f), Some(s)) = (fv.as_f64(), so.get(key).and_then(|v| v.as_f64())) {
                trends.insert(key.clone(), serde_json::json!(dir(f, s)));
            }
        }
    }

    Ok(serde_json::json!({
        "ok":      true,
        "command": "session_metrics",
        "metrics": serde_json::to_value(&full).unwrap_or(serde_json::Value::Null),
        "first":   first_json,
        "second":  second_json,
        "trends":  trends,
    }))
}

///
/// Required parameters:
///   `a_start_utc`, `a_end_utc` — unix seconds for session A
///   `b_start_utc`, `b_end_utc` — unix seconds for session B
///
/// Returns `{ "a": SessionMetrics, "b": SessionMetrics }`.
pub fn compare(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let a_start = msg.get("a_start_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"a_start_utc\" (u64)")?;
    let a_end = msg.get("a_end_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"a_end_utc\" (u64)")?;
    let b_start = msg.get("b_start_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"b_start_utc\" (u64)")?;
    let b_end = msg.get("b_end_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"b_end_utc\" (u64)")?;

    if a_end < a_start { return Err("\"a_end_utc\" must be >= \"a_start_utc\"".into()); }
    if b_end < b_start { return Err("\"b_end_utc\" must be >= \"b_start_utc\"".into()); }

    let st = app.state::<Mutex<AppState>>();
    let skill_dir = st.lock_or_recover().skill_dir.clone();

    // ── Session metrics (all bands + derived scores + ratios + PPG) ──────────
    let metrics_a = crate::get_session_metrics_impl(&skill_dir, a_start, a_end);
    let metrics_b = crate::get_session_metrics_impl(&skill_dir, b_start, b_end);

    // ── Compare insights (timeseries stats, deltas, trends) ────────────────
    let insights = crate::compute_compare_insights(
        &skill_dir, a_start, a_end, b_start, b_end, &metrics_a, &metrics_b,
    );

    // ── Sleep staging ────────────────────────────────────────────────────────
    let sleep_a = crate::get_sleep_stages_impl(&skill_dir, a_start, a_end);
    let sleep_b = crate::get_sleep_stages_impl(&skill_dir, b_start, b_end);

    // ── UMAP — enqueue via job queue (non-blocking) ─────────────────────────
    let queue = app.state::<std::sync::Arc<crate::job_queue::JobQueue>>();

    let n_a = crate::load_embeddings_range(&skill_dir, a_start, a_end).len();
    let n_b = crate::load_embeddings_range(&skill_dir, b_start, b_end).len();
    let n = n_a + n_b;
    // Time estimate: KNN is O(n²) on GPU, training is O(epochs × edges).
    // Rough empirical formula: 3s base + n²/20000 ms + n×epochs/2000 ms.
    let ucfg_est = crate::load_umap_config(&skill_dir);
    let est_epochs = ucfg_est.n_epochs.clamp(50, 2000) as u64;
    let estimated_ms = 3000u64
        + (n as u64) * (n as u64) / 20_000
        + (n as u64) * est_epochs / 2000;

    let sd = skill_dir.clone();
    let (as2, ae2, bs2, be2) = (a_start, a_end, b_start, b_end);
    let prog_map = queue.progress_map();
    let umap_ticket = queue.submit_with_id(estimated_ms, move |job_id| {
        let pm = prog_map;
        let cb: Box<dyn Fn(fast_umap::EpochProgress) + Send> = Box::new(move |ep| {
            let mut map = pm.lock_or_recover();
            map.insert(job_id, crate::job_queue::JobProgress {
                epoch:        ep.epoch,
                total_epochs: ep.total_epochs,
                loss:         ep.loss,
                best_loss:    ep.best_loss,
                elapsed_secs: ep.elapsed_secs,
                epoch_ms:     ep.epoch_ms,
            });
        });
        umap_compute_inner(&sd, as2, ae2, bs2, be2, Some(cb))
    });

    Ok(serde_json::json!({
        "a": metrics_a,
        "b": metrics_b,
        "sleep_a": sleep_a,
        "sleep_b": sleep_b,
        "insights": insights,
        "umap": {
            "queued":              true,
            "job_id":              umap_ticket.job_id,
            "estimated_ready_utc": umap_ticket.estimated_ready_utc,
            "queue_position":      umap_ticket.queue_position,
            "estimated_secs":      umap_ticket.estimated_secs,
            "n_a":                 n_a,
            "n_b":                 n_b,
        },
    }))
}

// ── sessions ──────────────────────────────────────────────────────────────────

/// List all embedding sessions (contiguous recording ranges from the
/// daily `eeg.sqlite` databases).  No parameters.
pub fn sessions(app: &AppHandle) -> Result<Value, String> {
    let st = app.state::<Mutex<AppState>>();
    // We can't call the #[tauri::command] directly, but we can replicate
    // the same logic.  Use the state's skill_dir.
    let skill_dir = st.lock_or_recover().skill_dir.clone();

    const GAP_SECS: u64 = 120;

    let mut all_ts: Vec<(u64, String)> = Vec::new();

    let entries = match std::fs::read_dir(&skill_dir) {
        Ok(e) => e,
        Err(_) => return Ok(serde_json::json!({ "sessions": [] })),
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let day_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if day_name.len() != 8 || !day_name.bytes().all(|b| b.is_ascii_digit()) { continue; }
        let db_path = path.join(crate::constants::SQLITE_FILE);
        if !db_path.exists() { continue; }
        let conn = match rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) { Ok(c) => c, Err(_) => continue };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        let mut stmt = match conn.prepare("SELECT timestamp FROM embeddings ORDER BY timestamp") {
            Ok(s) => s, Err(_) => continue,
        };
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0));
        if let Ok(rows) = rows {
            for row in rows.filter_map(|r| r.ok()) {
                let utc = crate::commands::ts_to_unix(row);
                all_ts.push((utc, day_name.clone()));
            }
        }
    }

    if all_ts.is_empty() {
        return Ok(serde_json::json!({ "sessions": [] }));
    }

    all_ts.sort_by_key(|(ts, _)| *ts);

    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut start = all_ts[0].0;
    let mut end   = start;
    let mut count: u64 = 1;
    let mut day   = all_ts[0].1.clone();

    for &(ts, ref d) in &all_ts[1..] {
        if ts.saturating_sub(end) > GAP_SECS {
            out.push(serde_json::json!({
                "start_utc": start, "end_utc": end,
                "n_epochs": count,  "day": day,
            }));
            start = ts; end = ts; count = 1; day = d.clone();
        } else {
            end = ts; count += 1;
        }
    }
    out.push(serde_json::json!({
        "start_utc": start, "end_utc": end,
        "n_epochs": count,  "day": day,
    }));

    out.reverse(); // newest first
    Ok(serde_json::json!({ "sessions": out }))
}

// ── sleep ─────────────────────────────────────────────────────────────────────

/// Classify sleep stages for a time range and return a hypnogram.
///
/// Required: `start_utc`, `end_utc` (u64).
pub fn sleep(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let start = msg.get("start_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"start_utc\" (u64)")?;
    let end = msg.get("end_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"end_utc\" (u64)")?;
    if end < start { return Err("\"end_utc\" must be >= \"start_utc\"".into()); }

    let st = app.state::<Mutex<AppState>>();
    let skill_dir = st.lock_or_recover().skill_dir.clone();

    let result = crate::get_sleep_stages_impl(&skill_dir, start, end);
    let analysis = crate::analyze_sleep_stages(&result);
    let mut val = serde_json::to_value(&result)
        .map_err(|e| format!("serialization error: {e}"))?;
    if let Some(obj) = val.as_object_mut() {
        obj.insert("analysis".into(), analysis);
    }
    Ok(val)
}

// ── umap (queue-based) ────────────────────────────────────────────────────────

/// Enqueue a UMAP 3D projection job.  Returns a `JobTicket` immediately.
///
/// Required: `a_start_utc`, `a_end_utc`, `b_start_utc`, `b_end_utc` (u64).
///
/// The `"umap"` WS command now uses the job queue so it never blocks the
/// WebSocket handler thread.  Clients should poll with `"umap_poll"`.
pub fn umap(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let a_start = msg.get("a_start_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"a_start_utc\" (u64)")?;
    let a_end = msg.get("a_end_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"a_end_utc\" (u64)")?;
    let b_start = msg.get("b_start_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"b_start_utc\" (u64)")?;
    let b_end = msg.get("b_end_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"b_end_utc\" (u64)")?;

    if a_end < a_start { return Err("\"a_end_utc\" must be >= \"a_start_utc\"".into()); }
    if b_end < b_start { return Err("\"b_end_utc\" must be >= \"b_start_utc\"".into()); }

    let st = app.state::<Mutex<AppState>>();
    let skill_dir = st.lock_or_recover().skill_dir.clone();

    let queue = app.state::<std::sync::Arc<crate::job_queue::JobQueue>>();

    // Quick count for time estimation (cheap read-only scan).
    let n_a = crate::load_embeddings_range(&skill_dir, a_start, a_end).len();
    let n_b = crate::load_embeddings_range(&skill_dir, b_start, b_end).len();
    let n = n_a + n_b;
    // Time estimate: KNN is O(n²) on GPU, training is O(epochs × edges).
    let ucfg_est = crate::load_umap_config(&skill_dir);
    let est_epochs = ucfg_est.n_epochs.clamp(50, 2000) as u64;
    let estimated_ms = 3000u64
        + (n as u64) * (n as u64) / 20_000
        + (n as u64) * est_epochs / 2000;

    let sd = skill_dir.clone();
    let prog_map = queue.progress_map();
    let ticket = queue.submit_with_id(estimated_ms, move |job_id| {
        let pm = prog_map;
        let cb: Box<dyn Fn(fast_umap::EpochProgress) + Send> = Box::new(move |ep| {
            let mut map = pm.lock_or_recover();
            map.insert(job_id, crate::job_queue::JobProgress {
                epoch:        ep.epoch,
                total_epochs: ep.total_epochs,
                loss:         ep.loss,
                best_loss:    ep.best_loss,
                elapsed_secs: ep.elapsed_secs,
                epoch_ms:     ep.epoch_ms,
            });
        });
        umap_compute_inner(&sd, a_start, a_end, b_start, b_end, Some(cb))
    });

    Ok(serde_json::json!({
        "queued":              true,
        "job_id":              ticket.job_id,
        "estimated_ready_utc": ticket.estimated_ready_utc,
        "queue_position":      ticket.queue_position,
        "estimated_secs":      ticket.estimated_secs,
        "n_a":                 n_a,
        "n_b":                 n_b,
    }))
}

/// Poll for a previously-enqueued job result.
///
/// Required: `job_id` (u64).
pub fn umap_poll(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let job_id = msg.get("job_id").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"job_id\" (u64)")?;

    let queue = app.state::<std::sync::Arc<crate::job_queue::JobQueue>>();
    let result = queue.poll(job_id);
    serde_json::to_value(&result).map_err(|e| e.to_string())
}

/// Backend type alias used by fast-umap (GPU-accelerated via wgpu / CubeCL).
///
/// We use the raw `CubeBackend` (without the `Fusion` wrapper) because
/// `fast_umap::backend::AutodiffBackend` is only implemented for
/// `Autodiff<CubeBackend<R, F, I, BT>>`, not for `Autodiff<Fusion<…>>`.
type FastUmapBackend = burn::backend::Autodiff<
    burn_cubecl::CubeBackend<cubecl::wgpu::WgpuRuntime, f32, i32, u32>,
>;

/// Return the path to the UMAP cache directory inside `~/.skill/umap_cache/`.
fn umap_cache_dir(skill_dir: &std::path::Path) -> std::path::PathBuf {
    skill_dir.join("umap_cache")
}

/// Build a deterministic cache filename for a session-pair UMAP result.
///
/// Format: `umap_{a_start}_{a_end}_{b_start}_{b_end}.json`
fn umap_cache_path(
    skill_dir: &std::path::Path,
    a_start: u64,
    a_end: u64,
    b_start: u64,
    b_end: u64,
) -> std::path::PathBuf {
    umap_cache_dir(skill_dir)
        .join(format!("umap_{a_start}_{a_end}_{b_start}_{b_end}.json"))
}

/// Try to load a cached UMAP result from disk.
fn umap_cache_load(path: &std::path::Path) -> Option<serde_json::Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Persist a UMAP result to the cache directory (best-effort, errors are logged).
fn umap_cache_store(path: &std::path::Path, value: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("[umap] failed to create cache dir: {e}");
            return;
        }
    }
    match serde_json::to_vec(value) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(path, bytes) {
                eprintln!("[umap] failed to write cache file: {e}");
            } else {
                eprintln!("[umap] cached result to {}", path.display());
            }
        }
        Err(e) => eprintln!("[umap] failed to serialise cache: {e}"),
    }
}

/// Inner UMAP compute — shared by both WS and Tauri IPC paths.
///
/// Uses `fast-umap` (parametric, GPU-accelerated) instead of `umap-rs` for
/// significantly faster projection on large embedding sets.
///
/// Results are cached to `~/.skill/umap_cache/umap_{a}_{b}_{c}_{d}.json` so
/// that repeated queries for the same session pair return instantly.
pub fn umap_compute_inner(
    skill_dir: &std::path::Path,
    a_start: u64,
    a_end: u64,
    b_start: u64,
    b_end: u64,
    on_progress: Option<Box<dyn Fn(fast_umap::EpochProgress) + Send>>,
) -> Result<serde_json::Value, String> {
    // ── Check cache first ────────────────────────────────────────────────
    let cache_path = umap_cache_path(skill_dir, a_start, a_end, b_start, b_end);
    if let Some(cached) = umap_cache_load(&cache_path) {
        eprintln!("[umap] cache hit: {}", cache_path.display());
        return Ok(cached);
    }

    let embs_a = crate::load_embeddings_range(skill_dir, a_start, a_end);
    let embs_b = crate::load_embeddings_range(skill_dir, b_start, b_end);
    let all_labels = crate::load_labels_range(
        skill_dir,
        a_start.min(b_start),
        a_end.max(b_end),
    );

    let n_a = embs_a.len();
    let n_b = embs_b.len();
    let n   = n_a + n_b;

    let umap_start = std::time::Instant::now();
    eprintln!("[umap] computing 3D projection for {} embeddings (A={}, B={})", n, n_a, n_b);

    if n < 5 {
        return Ok(serde_json::json!({ "points": [], "n_a": n_a, "n_b": n_b, "dim": 0 }));
    }

    let dim = embs_a.first().or(embs_b.first())
        .map(|e| e.1.len()).unwrap_or(0);
    if dim == 0 {
        return Ok(serde_json::json!({ "points": [], "n_a": n_a, "n_b": n_b, "dim": 0 }));
    }

    // ── Load user-configurable UMAP parameters ─────────────────────────────
    let ucfg = crate::load_umap_config(skill_dir);

    // All embeddings are used — no subsampling.
    let n_use = n;

    // Build Vec<Vec<f64>> input expected by fast-umap.
    let mut data: Vec<Vec<f64>> = Vec::with_capacity(n_use);
    let mut timestamps: Vec<u64> = Vec::with_capacity(n_use);
    let mut labels: Vec<u8> = Vec::with_capacity(n_use);
    for (ts, emb) in embs_a.iter().chain(embs_b.iter()) {
        data.push(emb.iter().map(|&v| v as f64).collect());
        timestamps.push(*ts);
        labels.push(if timestamps.len() <= n_a { 0 } else { 1 });
    }

    let k = ucfg.n_neighbors.clamp(2, 50).min(n_use - 1).min(n_use / 2).max(2);
    let n_epochs = ucfg.n_epochs.clamp(50, 2000);

    let config = fast_umap::UmapConfig {
        n_components: 3,
        graph: fast_umap::GraphParams {
            n_neighbors: k,
            ..Default::default()
        },
        optimization: fast_umap::OptimizationParams {
            n_epochs,
            verbose: false,
            repulsion_strength: ucfg.repulsion_strength.clamp(0.1, 10.0),
            neg_sample_rate: ucfg.neg_sample_rate.clamp(1, 30),
            timeout: Some(ucfg.timeout_secs.clamp(10, 600)),
            cooldown_ms: ucfg.cooldown_ms.clamp(0, 10_000),
            figures_dir: Some(skill_dir.join("tmp/figures")),
            ..Default::default()
        },
        ..Default::default()
    };

    // Build per-point label strings for fast-umap's training chart snapshots.
    // Also includes user-defined labels from the label store if present.
    let fit_labels: Vec<String> = (0..n_use).map(|i| {
        let session_tag = if labels[i] == 0 { "A" } else { "B" };
        if let Some(lbl) = crate::find_label_for_epoch(&all_labels, timestamps[i]) {
            format!("{session_tag}:{lbl}")
        } else {
            session_tag.to_string()
        }
    }).collect();

    // Run UMAP in a catch_unwind guard — GPU buffer readback can abort on some
    // drivers when VRAM pressure is high or tensor shapes are degenerate.
    let fit_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let umap = fast_umap::Umap::<FastUmapBackend>::new(config);
        let (_exit_tx, exit_rx) = crossbeam_channel::unbounded::<()>();
        let fitted = if let Some(cb) = on_progress {
            umap.fit_with_progress(data, Some(fit_labels), exit_rx, cb)
        } else {
            umap.fit_with_signal(data, Some(fit_labels), exit_rx)
        };
        fitted.into_embedding()
    }));

    let embedding = match fit_result {
        Ok(emb) => emb,
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "unknown panic".to_string()
            };
            eprintln!("[umap] UMAP fit panicked: {msg}");
            return Err(format!("UMAP projection failed: {msg}"));
        }
    };

    let points: Vec<serde_json::Value> = (0..n_use).map(|i| {
        let mut pt = serde_json::json!({
            "x": embedding[i][0],
            "y": embedding[i][1],
            "z": embedding[i][2],
            "session": labels[i],
            "utc": timestamps[i],
        });
        if let Some(lbl) = crate::find_label_for_epoch(&all_labels, timestamps[i]) {
            pt.as_object_mut().unwrap().insert("label".into(), serde_json::Value::String(lbl));
        }
        pt
    }).collect();

    let elapsed_ms = umap_start.elapsed().as_millis() as u64;
    eprintln!("[umap] projection done in {elapsed_ms} ms ({n_use} embeddings)");

    // Cluster analysis (centroids, separation score, outliers)
    let analysis = crate::analyze_umap_points(&embedding, &labels, &timestamps, n_a);

    let result = serde_json::json!({
        "points":     points,
        "n_a":        n_a,
        "n_b":        n_b,
        "dim":        dim,
        "elapsed_ms": elapsed_ms,
        "analysis":   analysis,
    });

    // ── Persist to cache ─────────────────────────────────────────────────
    umap_cache_store(&cache_path, &result);

    Ok(result)
}

// ── calibration profile commands ──────────────────────────────────────────────

/// `list_calibrations` — return all saved calibration profiles.
pub fn list_calibrations(app: &AppHandle) -> Result<Value, String> {
    let s = app.state::<Mutex<crate::AppState>>();
    let guard = s.lock_or_recover();
    Ok(serde_json::json!({ "profiles": guard.calibration_profiles }))
}

/// `get_calibration { "id": "…" }` — return a single profile by ID.
pub fn get_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let id = msg.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"id\" (string)".to_string())?;
    let s = app.state::<Mutex<crate::AppState>>();
    let guard = s.lock_or_recover();
    guard.calibration_profiles.iter().find(|p| p.id == id)
        .map(|p| serde_json::json!({ "profile": p }))
        .ok_or_else(|| format!("profile not found: {id}"))
}

/// `create_calibration { "name": "…", "actions": […], "break_duration_secs": n, "loop_count": n }`
pub fn create_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let name = msg.get("name").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"name\"".to_string())?
        .trim().to_owned();
    if name.is_empty() { return Err("\"name\" must not be empty".into()); }

    let actions: Vec<crate::CalibrationAction> = msg.get("actions")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .filter(|v: &Vec<_>| !v.is_empty())
        .ok_or_else(|| "\"actions\" must be a non-empty array of {label, duration_secs}".to_string())?;

    let break_secs  = msg.get("break_duration_secs").and_then(|v| v.as_u64()).unwrap_or(5) as u32;
    let loop_count  = msg.get("loop_count").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
    let auto_start  = msg.get("auto_start").and_then(|v| v.as_bool()).unwrap_or(false);

    let profile = crate::CalibrationProfile {
        id:                   crate::new_profile_id(),
        name,
        actions,
        break_duration_secs:  break_secs,
        loop_count,
        auto_start,
        last_calibration_utc: None,
    };

    let st = app.state::<Mutex<crate::AppState>>();
    {
        let mut s = st.lock_or_recover();
        s.calibration_profiles.push(profile.clone());
    }
    crate::save_settings_handle(app);
    Ok(serde_json::json!({ "profile": profile }))
}

/// `update_calibration { "id": "…", …fields… }` — update an existing profile.
pub fn update_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let id = msg.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"id\"".to_string())?;

    let st = app.state::<Mutex<crate::AppState>>();
    let mut s = st.lock_or_recover();
    let p = s.calibration_profiles.iter_mut().find(|p| p.id == id)
        .ok_or_else(|| format!("profile not found: {id}"))?;

    if let Some(name) = msg.get("name").and_then(|v| v.as_str()) {
        p.name = name.to_owned();
    }
    if let Some(actions) = msg.get("actions").and_then(|v| serde_json::from_value::<Vec<crate::CalibrationAction>>(v.clone()).ok()) {
        if !actions.is_empty() { p.actions = actions; }
    }
    if let Some(b) = msg.get("break_duration_secs").and_then(|v| v.as_u64()) {
        p.break_duration_secs = b as u32;
    }
    if let Some(n) = msg.get("loop_count").and_then(|v| v.as_u64()) {
        p.loop_count = n as u32;
    }
    if let Some(a) = msg.get("auto_start").and_then(|v| v.as_bool()) {
        p.auto_start = a;
    }
    let ret = p.clone();
    drop(s);
    crate::save_settings_handle(app);
    Ok(serde_json::json!({ "profile": ret }))
}

/// `delete_calibration { "id": "…" }` — remove a profile.
pub fn delete_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let id = msg.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"id\"".to_string())?;
    let st = app.state::<Mutex<crate::AppState>>();
    {
        let mut s = st.lock_or_recover();
        if s.calibration_profiles.len() <= 1 {
            return Err("Cannot delete the last calibration profile".into());
        }
        s.calibration_profiles.retain(|p| p.id != id);
        if s.active_calibration_id == id {
            s.active_calibration_id = s.calibration_profiles.first()
                .map(|p| p.id.clone()).unwrap_or_default();
        }
    }
    crate::save_settings_handle(app);
    Ok(serde_json::json!({}))
}

/// `run_calibration { "id"?: "…" }` — open the calibration window and start
/// the specified (or active) profile immediately.
pub async fn run_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let profile_id = msg.get("id").and_then(|v| v.as_str()).map(str::to_owned);
    crate::open_calibration_window_inner(app, profile_id, true).await?;
    Ok(serde_json::json!({}))
}

// ── interactive_search ────────────────────────────────────────────────────────

/// `interactive_search { "query": "…", "k_text"?: 5, "k_eeg"?: 5, "k_labels"?: 3, "reach_minutes"?: 10 }`
///
/// Cross-modal 4-layer graph search:
///   Layer 0  query        — center node (the embedded query text)
///   Layer 1  text_label   — semantically similar label annotations
///   Layer 2  eeg_point    — raw EEG neighbors of label time windows
///   Layer 3  found_label  — labels near the EEG neighbors in time
///
/// Returns `{ nodes, edges, dot }` — same structure as the Tauri `interactive_search`
/// command used by the desktop UI, minus the SVG (not generated for the WS API).
pub fn interactive_search(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    use crate::commands::{
        InteractiveGraphNode, InteractiveGraphEdge,
        list_date_dirs, load_day_index, get_labels_near, generate_dot, ts_to_unix,
    };
    use crate::constants::LABELS_FILE;

    let query = msg.get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"query\" (string)".to_string())?
        .to_owned();

    let k_text        = msg.get("k_text")       .and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(5).clamp(1, 20);
    let k_eeg         = msg.get("k_eeg")        .and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(5).clamp(1, 20);
    let k_labels      = msg.get("k_labels")     .and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(3).clamp(1, 10);
    let reach_minutes = msg.get("reach_minutes").and_then(|v| v.as_u64()).unwrap_or(10).clamp(1, 60);
    let reach_seconds = reach_minutes * 60;

    let (skill_dir, _model_code) = {
        let state = app.state::<Mutex<AppState>>();
        let g = state.lock_or_recover();
        (g.skill_dir.clone(), g.text_embedding_model.clone())
    };

    let embedder_arc = std::sync::Arc::clone(
        &*app.state::<std::sync::Arc<crate::label_cmds::EmbedderState>>()
    );
    let label_idx_arc = std::sync::Arc::clone(
        &*app.state::<std::sync::Arc<crate::label_index::LabelIndexState>>()
    );

    // Embed the query (ws_commands run inside spawn_blocking — safe to block)
    let query_vec: Vec<f32> = {
        let mut guard = embedder_arc.0.lock_or_recover();
        let te = guard.as_mut().ok_or("text embedder not initialised — model may still be downloading")?;
        let mut vecs = te.embed(vec![query.as_str()], None).map_err(|e| e.to_string())?;
        vecs.remove(0)
    };

    let mut nodes: Vec<InteractiveGraphNode> = Vec::new();
    let mut edges: Vec<InteractiveGraphEdge> = Vec::new();

    // Query node (center of the graph).
    nodes.push(InteractiveGraphNode {
        id:             "query".into(),
        kind:           "query".into(),
        text:           Some(query.clone()),
        timestamp_unix: None,
        distance:       0.0,
        eeg_metrics:    None,
        parent_id:      None,
        proj_x:         None,
        proj_y:         None,
    });

    // Step 2: search the label text-HNSW for semantically similar labels.
    let ef_text = (k_text * 4).max(64);
    let text_labels = crate::label_index::search_by_text_vec(
        &query_vec, k_text, ef_text, &skill_dir, &label_idx_arc,
    );

    // Load all daily EEG HNSW indices once (re-used for every text label).
    let day_indices: Vec<_> = list_date_dirs(&skill_dir)
        .into_iter()
        .filter_map(|(date, dir)| load_day_index(date, dir))
        .collect();

    let ef_eeg    = (k_eeg * 4).max(64);
    let labels_db = skill_dir.join(LABELS_FILE);

    let mut seen_eeg: std::collections::HashSet<u64>  = std::collections::HashSet::new();
    let mut seen_labels: std::collections::HashSet<i64> = std::collections::HashSet::new();

    // Steps 3–5: per text label → EEG neighbors → nearby labels.
    for (ti, tl) in text_labels.iter().enumerate() {
        let tl_id = format!("tl_{ti}");

        nodes.push(InteractiveGraphNode {
            id:             tl_id.clone(),
            kind:           "text_label".into(),
            text:           Some(tl.text.clone()),
            timestamp_unix: Some(tl.eeg_start),
            distance:       tl.distance,
            eeg_metrics:    tl.eeg_metrics.clone(),
            parent_id:      Some("query".into()),
            proj_x:         None,
            proj_y:         None,
        });
        edges.push(InteractiveGraphEdge {
            from_id:  "query".into(),
            to_id:    tl_id.clone(),
            distance: tl.distance,
            kind:     "text_sim".into(),
        });

        // Step 3: mean EEG embedding for this label's time window.
        let Some(mean_eeg) = crate::label_index::mean_eeg_for_window(
            &skill_dir, tl.eeg_start, tl.eeg_end,
        ) else { continue };

        // Step 4: search all daily HNSW indices with that EEG vector.
        let mut eeg_candidates: Vec<(u64, f32)> = Vec::new();
        for day in &day_indices {
            if day.index.is_empty() { continue; }
            for hit in day.index.search(&mean_eeg, k_eeg, ef_eeg.max(k_eeg)) {
                eeg_candidates.push((ts_to_unix(*hit.payload), hit.distance));
            }
        }
        eeg_candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        eeg_candidates.truncate(k_eeg);

        for (ep_unix, ep_dist) in &eeg_candidates {
            let ep_id = format!("ep_{ep_unix}");

            if seen_eeg.contains(ep_unix) {
                // Shared EEG point — just add a cross-edge (avoid duplicate).
                let already = edges.iter().any(|e| e.from_id == tl_id && e.to_id == ep_id);
                if !already {
                    edges.push(InteractiveGraphEdge {
                        from_id:  tl_id.clone(),
                        to_id:    ep_id,
                        distance: *ep_dist,
                        kind:     "eeg_bridge".into(),
                    });
                }
                continue;
            }
            seen_eeg.insert(*ep_unix);

            nodes.push(InteractiveGraphNode {
                id:             ep_id.clone(),
                kind:           "eeg_point".into(),
                text:           None,
                timestamp_unix: Some(*ep_unix),
                distance:       *ep_dist,
                eeg_metrics:    None,
                parent_id:      Some(tl_id.clone()),
                proj_x:         None,
                proj_y:         None,
            });
            edges.push(InteractiveGraphEdge {
                from_id:  tl_id.clone(),
                to_id:    ep_id.clone(),
                distance: *ep_dist,
                kind:     "eeg_bridge".into(),
            });

            // Step 5: find nearest labels around this EEG timestamp (±reach_minutes).
            if labels_db.exists() {
                let nearby = get_labels_near(&labels_db, *ep_unix, reach_seconds);
                for fl in nearby.iter().take(k_labels) {
                    if seen_labels.contains(&fl.id) { continue; }
                    seen_labels.insert(fl.id);

                    let fl_id  = format!("fl_{}", fl.id);
                    let t_dist = (fl.eeg_start as f32 - *ep_unix as f32).abs()
                        / (reach_seconds as f32);

                    nodes.push(InteractiveGraphNode {
                        id:             fl_id.clone(),
                        kind:           "found_label".into(),
                        text:           Some(fl.text.clone()),
                        timestamp_unix: Some(fl.eeg_start),
                        distance:       t_dist,
                        eeg_metrics:    None,
                        parent_id:      Some(ep_id.clone()),
                        proj_x:         None, // filled by PCA step in commands.rs
                        proj_y:         None,
                    });
                    edges.push(InteractiveGraphEdge {
                        from_id:  ep_id.clone(),
                        to_id:    fl_id,
                        distance: t_dist,
                        kind:     "label_prox".into(),
                    });
                }
            }
        }
    }

    let dot = generate_dot(&nodes, &edges);

    let nodes_json: Vec<serde_json::Value> = nodes.iter().map(|n| serde_json::json!({
        "id":             n.id,
        "kind":           n.kind,
        "text":           n.text,
        "timestamp_unix": n.timestamp_unix,
        "distance":       n.distance,
        "eeg_metrics":    n.eeg_metrics,
        "parent_id":      n.parent_id,
    })).collect();

    let edges_json: Vec<serde_json::Value> = edges.iter().map(|e| serde_json::json!({
        "from_id":  e.from_id,
        "to_id":    e.to_id,
        "distance": e.distance,
        "kind":     e.kind,
    })).collect();

    Ok(serde_json::json!({
        "query":   query,
        "k_text":  k_text,
        "k_eeg":   k_eeg,
        "k_labels": k_labels,
        "reach_minutes": reach_minutes,
        "nodes":   nodes_json,
        "edges":   edges_json,
        "dot":     dot,
    }))
}

// ── dnd ───────────────────────────────────────────────────────────────────────

/// `dnd` — return the current Do Not Disturb automation status.
///
/// **Response**
/// ```json
/// {
///   "enabled":         true,
///   "avg_score":       68.4,
///   "threshold":       60.0,
///   "sample_count":    120,
///   "window_size":     240,
///   "duration_secs":   60,
///   "mode_identifier": "com.apple.donotdisturb.mode.default",
///   "dnd_active":      false,
///   "os_active":       false
/// }
/// ```
///
/// | Field | Description |
/// |---|---|
/// | `enabled` | Whether DND automation is enabled in settings |
/// | `avg_score` | Rolling average focus score over the current sample window |
/// | `threshold` | Average must reach this (0–100) to activate DND |
/// | `sample_count` | Samples currently in the window |
/// | `window_size` | Target window size (≈ duration_secs × 4 Hz) |
/// | `duration_secs` | Seconds worth of samples in the rolling window |
/// | `mode_identifier` | macOS Focus mode identifier (ignored on non-macOS) |
/// | `dnd_active` | Whether the app has currently activated DND |
/// | `os_active` | Real OS Focus state (`null` on non-macOS) |
pub fn dnd_status(app: &AppHandle) -> Result<Value, String> {
    let s = app.state::<Mutex<AppState>>();
    let guard = s.lock_or_recover();
    let enabled       = guard.dnd_config.enabled;
    let threshold     = guard.dnd_config.focus_threshold;
    let duration_secs = guard.dnd_config.duration_secs;
    let mode_id       = guard.dnd_config.focus_mode_identifier.clone();
    let dnd_active    = guard.dnd_active;
    let window_size   = (duration_secs as usize * 4).max(8);
    let sample_count  = guard.dnd_focus_samples.len();
    let avg_score     = if sample_count > 0 {
        guard.dnd_focus_samples.iter().sum::<f64>() / sample_count as f64
    } else { 0.0 };
    // Use the cached OS state kept fresh by the 5-second background poll.
    let os_active     = guard.dnd_os_active;
    drop(guard);

    Ok(serde_json::json!({
        "enabled":          enabled,
        "avg_score":        avg_score,
        "threshold":        threshold,
        "sample_count":     sample_count,
        "window_size":      window_size,
        "duration_secs":    duration_secs,
        "mode_identifier":  mode_id,
        "dnd_active":       dnd_active,
        "os_active":        os_active,
    }))
}

/// `dnd_set { "enabled": bool }` — force-enable or disable DND immediately,
/// bypassing the EEG threshold entirely.
///
/// Useful for automation scripts, integrations, or testing.  The change is
/// reflected in `dnd_active` and a `dnd-state-changed` event is emitted to all
/// connected clients (including the desktop UI) so they can update their state.
///
/// **Required**
/// - `enabled` — `true` to activate DND, `false` to deactivate.
///
/// **Response**
/// ```json
/// { "enabled": true, "ok": true }
/// ```
///
/// When the OS call fails (e.g. macOS permissions not granted), `ok` is
/// `false` and no state change is applied.
pub fn dnd_set(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let enabled = msg.get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| "missing required field: \"enabled\" (boolean)".to_string())?;

    let mode_id = {
        let s = app.state::<Mutex<AppState>>();
        let g = s.lock_or_recover();
        g.dnd_config.focus_mode_identifier.clone()
    };

    let ok = crate::dnd::set_dnd(enabled, &mode_id);
    if ok {
        let s = app.state::<Mutex<AppState>>();
        let mut guard = s.lock_or_recover();
        guard.dnd_active = enabled;
        if !enabled {
            guard.dnd_focus_samples.clear();
        }
        drop(guard);
        let _ = app.emit("dnd-state-changed", enabled);
    }

    Ok(serde_json::json!({ "enabled": enabled, "ok": ok }))
}

// ── say ───────────────────────────────────────────────────────────────────────

/// `say` — synthesise `text` via TTS and play it on the default audio output.
///
/// The command is **fire-and-forget**: it spawns synthesis in a background
/// thread and returns immediately so the WebSocket / HTTP caller is not blocked.
///
/// ```json
/// { "command": "say", "text": "Eyes closed. Relax." }
/// { "command": "say", "text": "Eyes closed. Relax.", "voice": "Jasper" }
/// { "command": "say", "text": "Eyes closed. Relax.", "voice": "dave" }
/// ```
///
/// `voice` is **engine-specific** and optional:
/// - **KittenTTS**: voice name (e.g. `"Jasper"`).  Falls back to the globally
///   active voice (set via `tts_set_voice`; default `"Jasper"`).
/// - **NeuTTS**: preset name (`"jo"`, `"dave"`, `"greta"`, `"juliette"`,
///   `"mateo"`).  Overrides the configured reference voice for this single
///   utterance only.  Ignored if not a known preset name.
pub async fn say(_app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let text = msg["text"]
        .as_str()
        .ok_or_else(|| "say: 'text' field is required (string)".to_string())?
        .to_string();

    // Optional voice override — falls back to the globally active voice.
    let voice: Option<String> = msg["voice"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let spoken = text.clone();
    let voice_echo = voice.clone();
    tokio::spawn(async move { crate::tts::tts_speak(text, voice).await });

    let mut resp = serde_json::json!({ "spoken": spoken });
    if let Some(v) = voice_echo {
        resp["voice"] = serde_json::Value::String(v);
    }
    Ok(resp)
}

// ── LLM commands (feature = "llm") ───────────────────────────────────────────

/// `llm_status` — return the current LLM server state.
///
/// ```json
/// { "command": "llm_status" }
/// → { "command": "llm_status", "ok": true,
///     "status": "stopped"|"loading"|"running",
///     "model_name": "Qwen3-1.7B-Q4_K_M.gguf",
///     "n_ctx": 4096, "supports_vision": false }
/// ```
#[cfg(feature = "llm")]
fn llm_status(app: &AppHandle) -> Result<Value, String> {
    use std::sync::atomic::Ordering;
    let state = app.state::<Mutex<AppState>>();
    let s = state.lock_or_recover();
    let (status, model_name) = crate::llm::cell_status(&s.llm_state_cell);
    let (n_ctx, supports_vision) = s.llm_state_cell.lock().unwrap()
        .as_ref()
        .map(|srv| (
            srv.n_ctx.load(Ordering::Relaxed),
            srv.vision_ready.load(Ordering::Relaxed),
        ))
        .unwrap_or((0, false));
    Ok(serde_json::json!({
        "status":          status,
        "model_name":      model_name,
        "n_ctx":           n_ctx,
        "supports_vision": supports_vision,
    }))
}

/// `llm_start` — load the active model and start the LLM inference server.
///
/// Blocks until the model is fully loaded (which can take several seconds
/// depending on model size and hardware).  Returns `ok=false` on failure.
///
/// ```json
/// { "command": "llm_start" }
/// → { "command": "llm_start", "ok": true, "result": "started"|"already_running" }
/// ```
#[cfg(feature = "llm")]
async fn llm_start(app: &AppHandle) -> Result<Value, String> {
    let (mut config, catalog, log_buf, cell, skill_dir) = {
        let st = app.state::<Mutex<AppState>>();
        let s = st.lock_or_recover();
        (
            s.llm_config.clone(),
            s.llm_catalog.clone(),
            s.llm_logs.clone(),
            s.llm_state_cell.clone(),
            s.skill_dir.clone(),
        )
    };

    if cell.lock().unwrap().is_some() {
        return Ok(serde_json::json!({ "result": "already_running" }));
    }

    // Resolve mmproj if autoload is on and none is set.
    if config.mmproj.is_none() {
        config.mmproj = catalog.resolve_mmproj_path(config.autoload_mmproj);
    }

    crate::llm::push_log(app, &log_buf, "info", "llm_start command received via WebSocket");

    let app2 = app.clone();
    let new_state = tokio::task::spawn_blocking(move || {
        crate::llm::init(&config, &catalog, app2, log_buf, &skill_dir)
    }).await.map_err(|e| e.to_string())?;

    match new_state {
        Some(s) => {
            *cell.lock().unwrap() = Some(s);
            Ok(serde_json::json!({ "result": "started" }))
        }
        None => Err(
            "Failed to start LLM server. \
             Check that a model is downloaded and selected in Settings → LLM.".to_string()
        ),
    }
}

/// `llm_stop` — stop the LLM inference server and free all GPU/CPU resources.
///
/// ```json
/// { "command": "llm_stop" }
/// → { "command": "llm_stop", "ok": true, "result": "stopped"|"not_running" }
/// ```
#[cfg(feature = "llm")]
fn llm_stop(app: &AppHandle) -> Result<Value, String> {
    let (cell, log_buf) = {
        let st = app.state::<Mutex<AppState>>();
        let s = st.lock_or_recover();
        (s.llm_state_cell.clone(), s.llm_logs.clone())
    };
    let server_state = { cell.lock().unwrap().take() };
    if let Some(server_state) = server_state {
        crate::llm::push_log(app, &log_buf, "info", "llm_stop command received via WebSocket");
        match std::sync::Arc::try_unwrap(server_state) {
            Ok(owned) => owned.shutdown(),
            Err(arc)  => drop(arc),
        }
        crate::llm::push_log(app, &log_buf, "info", "LLM server stopped");
        Ok(serde_json::json!({ "result": "stopped" }))
    } else {
        Ok(serde_json::json!({ "result": "not_running" }))
    }
}

/// `llm_catalog` — return the model catalog with download states and selections.
///
/// ```json
/// { "command": "llm_catalog" }
/// → { "command": "llm_catalog", "ok": true,
///     "entries": [...], "active_model": "...", "active_mmproj": "..." }
/// ```
#[cfg(feature = "llm")]
fn llm_catalog(app: &AppHandle) -> Result<Value, String> {
    let state = app.state::<Mutex<AppState>>();
    let mut s = state.lock_or_recover();
    // Sync in-flight downloads into the catalog so callers see live progress.
    let downloads = s.llm_downloads.clone();
    for (filename, prog_arc) in &downloads {
        if let Ok(prog) = prog_arc.lock() {
            if let Some(entry) = s.llm_catalog.entries
                .iter_mut()
                .find(|e| &e.filename == filename)
            {
                entry.state      = prog.state.clone();
                entry.status_msg = prog.status_msg.clone();
                entry.progress   = prog.progress;
            }
        }
    }
    serde_json::to_value(&s.llm_catalog).map_err(|e| e.to_string())
}

/// `llm_download` — start downloading a GGUF model by filename (fire-and-forget).
///
/// Poll `llm_catalog` for progress updates.
///
/// ```json
/// { "command": "llm_download", "filename": "Qwen3-1.7B-Q4_K_M.gguf" }
/// → { "command": "llm_download", "ok": true, "result": "queued", "filename": "..." }
/// ```
#[cfg(feature = "llm")]
fn llm_download(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_download: 'filename' field required (string)".to_string())?
        .to_string();
    crate::llm::cmds::download_llm_model(
        filename.clone(),
        app.clone(),
        app.state::<Mutex<AppState>>(),
    );
    Ok(serde_json::json!({ "result": "queued", "filename": filename }))
}

/// `llm_cancel_download` — cancel an in-progress model download.
///
/// ```json
/// { "command": "llm_cancel_download", "filename": "Qwen3-1.7B-Q4_K_M.gguf" }
/// → { "command": "llm_cancel_download", "ok": true, "filename": "..." }
/// ```
#[cfg(feature = "llm")]
fn llm_cancel_download(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_cancel_download: 'filename' field required".to_string())?
        .to_string();
    crate::llm::cmds::cancel_llm_download(filename.clone(), app.state::<Mutex<AppState>>());
    Ok(serde_json::json!({ "filename": filename }))
}

/// `llm_delete` — delete a locally-cached model file.
///
/// ```json
/// { "command": "llm_delete", "filename": "Qwen3-1.7B-Q4_K_M.gguf" }
/// → { "command": "llm_delete", "ok": true, "filename": "..." }
/// ```
#[cfg(feature = "llm")]
fn llm_delete(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_delete: 'filename' field required".to_string())?
        .to_string();
    crate::llm::cmds::delete_llm_model(
        filename.clone(),
        app.clone(),
        app.state::<Mutex<AppState>>(),
    );
    Ok(serde_json::json!({ "filename": filename }))
}

/// `llm_logs` — return the last ≤500 LLM server log lines.
///
/// ```json
/// { "command": "llm_logs" }
/// → { "command": "llm_logs", "ok": true,
///     "logs": [{ "ts": 1740412800000, "level": "info", "message": "..." }, …] }
/// ```
#[cfg(feature = "llm")]
fn llm_logs(app: &AppHandle) -> Result<Value, String> {
    let state = app.state::<Mutex<AppState>>();
    let s = state.lock_or_recover();
    let log = s.llm_logs.lock().unwrap();
    let logs: Vec<&crate::llm::LlmLogEntry> = log.iter().collect();
    Ok(serde_json::json!({ "logs": logs, "count": logs.len() }))
}

// ── Central dispatcher ────────────────────────────────────────────────────────

/// Dispatch a named command to the appropriate handler function.
///
/// This is the single source of truth for command routing, used by both the
/// WebSocket server (`ws_server.rs`) and the HTTP API server (`api.rs`).
/// Adding a new command only requires updating this match arm.
pub async fn dispatch(
    app:     &AppHandle,
    command: &str,
    msg:     &Value,
) -> Result<Value, String> {
    match command {
        "status"              => status(app),
        "calibrate"           => calibrate(app).await,
        "timer"               => timer(app).await,
        "notify"              => notify(app, msg),
        "label"               => label(app, msg),
        "search_labels"       => search_labels(app, msg),
        "interactive_search"  => interactive_search(app, msg),
        "search"              => search(app, msg),
        "compare"             => compare(app, msg),
        "session_metrics"     => session_metrics(app, msg),
        "sessions"            => sessions(app),
        "sleep"               => sleep(app, msg),
        "umap"                => umap(app, msg),
        "umap_poll"           => umap_poll(app, msg),
        "list_calibrations"   => list_calibrations(app),
        "get_calibration"     => get_calibration(app, msg),
        "create_calibration"  => create_calibration(app, msg),
        "update_calibration"  => update_calibration(app, msg),
        "delete_calibration"  => delete_calibration(app, msg),
        "run_calibration"     => run_calibration(app, msg).await,
        "say"                 => say(app, msg).await,
        "dnd"                 => dnd_status(app),
        "dnd_set"             => dnd_set(app, msg),
        // ── LLM commands (llm_chat is handled before dispatch — see api.rs) ──
        #[cfg(feature = "llm")]
        "llm_status"          => llm_status(app),
        #[cfg(feature = "llm")]
        "llm_start"           => llm_start(app).await,
        #[cfg(feature = "llm")]
        "llm_stop"            => llm_stop(app),
        #[cfg(feature = "llm")]
        "llm_catalog"         => llm_catalog(app),
        #[cfg(feature = "llm")]
        "llm_download"        => llm_download(app, msg),
        #[cfg(feature = "llm")]
        "llm_cancel_download" => llm_cancel_download(app, msg),
        #[cfg(feature = "llm")]
        "llm_delete"          => llm_delete(app, msg),
        #[cfg(feature = "llm")]
        "llm_logs"            => llm_logs(app),
        other                 => Err(format!("unknown command: \"{other}\"")),
    }
}
