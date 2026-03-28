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

mod calendar;
mod calibration;
pub(crate) mod dnd_sleep;
mod health;
mod hooks;
#[cfg(feature = "llm")]
mod llm_cmds;
mod screenshots;
mod search;

use crate::skill_dir;
use crate::AppStateExt;
use crate::MutexExt;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::constants::SQLITE_FILE;
use crate::unix_secs;
use skill_eeg::eeg_model_config::LatestEpochMetrics;

// ── Re-exports from skill-router ──────────────────────────────────────────────

pub use skill_router::{r1, r1d, r2, r2d, r3, umap_compute_inner, RoundedBands, RoundedScores};

fn rounded_scores_from(m: &LatestEpochMetrics) -> RoundedScores {
    RoundedScores {
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

// ── status ────────────────────────────────────────────────────────────────────

pub fn status(app: &AppHandle) -> Result<Value, String> {
    let st = app.app_state();
    let guard = st.lock_or_recover();

    // ── Device / connection ──────────────────────────────────────────────────
    let status = &guard.status;
    let connected = status.state == "connected";
    let streaming = connected && guard.stream.is_some();
    let sample_count = status.sample_count;
    let ppg_sample_count = status.ppg_sample_count;
    let ppg = status.ppg.clone();

    // ── Session ──────────────────────────────────────────────────────────────
    let session_start_utc = guard.session_start_utc;
    let session_duration_secs = session_start_utc.map(|s| unix_secs().saturating_sub(s));

    // ── Embeddings (today + all-time) ────────────────────────────────────────
    let model_status = guard.embedding.model_status.lock_or_recover().clone();
    let embeddings_today = model_status.embeddings_today;
    let encoder_loaded = model_status.encoder_loaded;
    let latest_metrics = model_status.latest_metrics.clone();

    let skill_dir = guard.skill_dir.clone();

    // Label count, recent labels, and most frequent label texts from the database.
    let label_count = guard
        .label_store
        .as_ref()
        .map(skill_data::label_store::LabelStore::count)
        .unwrap_or(0);
    let label_embedded_count = guard
        .label_store
        .as_ref()
        .map(skill_data::label_store::LabelStore::count_embedded)
        .unwrap_or(0);
    let recent_labels: Vec<serde_json::Value> = guard
        .label_store
        .as_ref()
        .map(|ls| {
            ls.recent(5)
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                        "id":         r.id,
                        "text":       r.text,
                        "created_at": r.created_at,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let now_for_labels = unix_secs();
    let top_labels_all = guard
        .label_store
        .as_ref()
        .map(|ls| ls.top_texts(10, None))
        .unwrap_or_default();
    let top_labels_24h = guard
        .label_store
        .as_ref()
        .map(|ls| ls.top_texts(10, Some(now_for_labels.saturating_sub(24 * 3600))))
        .unwrap_or_default();
    let top_labels_7d = guard
        .label_store
        .as_ref()
        .map(|ls| ls.top_texts(10, Some(now_for_labels.saturating_sub(7 * 24 * 3600))))
        .unwrap_or_default();

    // ── Most used apps ───────────────────────────────────────────────────────
    let activity_store = guard.input.activity_store.clone();
    let top_apps_all = activity_store
        .as_ref()
        .map(|s| s.top_apps(10, None))
        .unwrap_or_default();
    let top_apps_24h = activity_store
        .as_ref()
        .map(|s| s.top_apps(10, Some(now_for_labels.saturating_sub(24 * 3600))))
        .unwrap_or_default();
    let top_apps_7d = activity_store
        .as_ref()
        .map(|s| s.top_apps(10, Some(now_for_labels.saturating_sub(7 * 24 * 3600))))
        .unwrap_or_default();

    // ── Screenshot / OCR summary ─────────────────────────────────────────────
    let ss_store = guard.screenshot_store.clone();
    let ss_summary = ss_store.as_ref().map(|s| s.summary_counts());
    let ss_top_apps_all = ss_store
        .as_ref()
        .map(|s| s.top_screenshot_apps(10, None))
        .unwrap_or_default();
    let ss_top_apps_24h = ss_store
        .as_ref()
        .map(|s| s.top_screenshot_apps(10, Some(now_for_labels.saturating_sub(24 * 3600))))
        .unwrap_or_default();

    // ── Calibration ──────────────────────────────────────────────────────────
    let last_calibration_utc = {
        let active_id = &guard.active_calibration_id;
        guard
            .calibration_profiles
            .iter()
            .find(|p| &p.id == active_id)
            .or_else(|| guard.calibration_profiles.first())
            .and_then(|p| p.last_calibration_utc)
    };

    // ── Signal quality ───────────────────────────────────────────────────────
    // Only return entries for electrodes that actually exist on the device.
    let n_ch = status.eeg_channel_count;
    let channel_quality: Vec<_> = status.channel_quality.iter().take(n_ch).cloned().collect();

    // Snapshot scalars before dropping the lock.
    let state_str = status.state.clone();
    let battery = status.battery;
    let device_name = status.device_name.clone();
    let device_id = status.device_id.clone();
    let serial_number = status.serial_number.clone();
    let firmware_version = status.firmware_version.clone();
    let hardware_version = status.hardware_version.clone();
    let bootloader_version = status.bootloader_version.clone();
    let headset_preset = status.headset_preset.clone();
    let mac_address = status.mac_address.clone();
    let embedding_overlap_secs = status.embedding_overlap_secs;
    let retry_attempt = status.retry_attempt;
    let retry_countdown_secs = status.retry_countdown_secs;
    let accel = status.accel;
    let gyro = status.gyro;
    let fuel_gauge_mv = status.fuel_gauge_mv;
    let temperature_raw = status.temperature_raw;

    // ── Hooks — most recent trigger across all hooks ─────────────────────────
    let hooks_summary = {
        let runtime = guard.hook_runtime.lock_or_recover();
        let total_hooks = guard.hooks.len();
        let enabled_hooks = guard.hooks.iter().filter(|h| h.enabled).count();

        // Find the most recent trigger across all hooks.
        let latest: Option<(&String, &crate::settings::HookLastTrigger)> = runtime
            .iter()
            .filter(|(_, t)| t.triggered_at_utc > 0)
            .max_by_key(|(_, t)| t.triggered_at_utc);

        serde_json::json!({
            "total":   total_hooks,
            "enabled": enabled_hooks,
            "latest_trigger": latest.map(|(name, t)| serde_json::json!({
                "hook":             name,
                "triggered_at_utc": t.triggered_at_utc,
                "distance":         t.distance,
                "label_id":         t.label_id,
                "label_text":       t.label_text,
            })),
        })
    };

    drop(guard);

    // ── Embedding totals (filesystem scan, outside lock) ─────────────────────
    let mut total_embeddings: u64 = 0;
    let mut recording_days: u64 = 0;
    if let Ok(rd) = std::fs::read_dir(&skill_dir) {
        for entry in rd.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.len() == 8 && name.bytes().all(|b| b.is_ascii_digit()) && entry.path().is_dir()
            {
                let db_path = entry.path().join(SQLITE_FILE);
                if db_path.exists() {
                    recording_days += 1;
                    // Count embeddings via SQLite (lightweight, no HNSW load).
                    if let Ok(conn) = skill_data::util::open_readonly(&db_path) {
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
    let sleep_48h =
        crate::get_sleep_stages_impl(&skill_dir, now_utc.saturating_sub(48 * 3600), now_utc);
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
            "total":    label_count,
            "embedded": label_embedded_count,
            "recent":   recent_labels,
            "top_all_time": top_labels_all,
            "top_24h":      top_labels_24h,
            "top_7d":       top_labels_7d,
        },
        "apps": {
            "top_all_time": top_apps_all,
            "top_24h":      top_apps_24h,
            "top_7d":       top_apps_7d,
        },
        "screenshots": {
            "total":              ss_summary.as_ref().map(|s| s.total).unwrap_or(0),
            "with_embedding":     ss_summary.as_ref().map(|s| s.with_embedding).unwrap_or(0),
            "with_ocr":           ss_summary.as_ref().map(|s| s.with_ocr).unwrap_or(0),
            "with_ocr_embedding": ss_summary.as_ref().map(|s| s.with_ocr_embedding).unwrap_or(0),
            "top_apps_all_time":  ss_top_apps_all,
            "top_apps_24h":       ss_top_apps_24h,
        },
        "calibration": {
            "last_calibration_utc": last_calibration_utc,
        },
        "signal_quality": channel_quality,
        "sleep": sleep_json,
        "scores": match latest_metrics {
            Some(ref m) => serde_json::to_value(rounded_scores_from(m))
                .unwrap_or(Value::Null),
            None => Value::Null,
        },
        "hooks": hooks_summary,
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

    let title = msg
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"title\" (string)".to_string())?;
    let body = msg.get("body").and_then(|v| v.as_str()).unwrap_or("");

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
    let text = msg
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"text\" (string)".to_string())?;

    let text = text.trim().to_owned();
    if text.is_empty() {
        return Err("\"text\" must not be empty".into());
    }

    // label_start_utc: optional — defaults to "now".
    let label_start_utc = msg
        .get("label_start_utc")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_else(unix_secs);

    let context = msg
        .get("context")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_owned();

    let s = app.app_state();
    let guard = s.lock_or_recover();
    let now = unix_secs();
    match &guard.label_store {
        Some(store) => {
            let id = store
                .insert(
                    label_start_utc,
                    now,
                    label_start_utc,
                    now,
                    &text,
                    &context,
                    now,
                )
                .ok_or_else(|| "database insert failed".to_string())?;
            drop(guard);
            let _ = app.emit(
                "label-created",
                serde_json::json!({
                    "text": text, "context": context, "label_id": id,
                }),
            );
            Ok(serde_json::json!({ "label_id": id }))
        }
        None => Err("label store not available".into()),
    }
}

// ── sessions ──────────────────────────────────────────────────────────────────

/// List all embedding sessions (contiguous recording ranges from the
/// daily `eeg.sqlite` databases).  No parameters.
pub fn sessions(app: &AppHandle) -> Result<Value, String> {
    let st = app.app_state();
    // We can't call the #[tauri::command] directly, but we can replicate
    // the same logic.  Use the state's skill_dir.
    let skill_dir = skill_dir(&st);

    const GAP_SECS: u64 = skill_constants::SESSION_GAP_SECS;

    let mut all_ts: Vec<(u64, String)> = Vec::new();

    let Ok(entries) = std::fs::read_dir(&skill_dir) else {
        return Ok(serde_json::json!({ "sessions": [] }));
    };
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let day_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if day_name.len() != 8 || !day_name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let db_path = path.join(crate::constants::SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }
        let Ok(conn) = skill_data::util::open_readonly(&db_path) else {
            continue;
        };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        let Ok(mut stmt) = conn.prepare("SELECT timestamp FROM embeddings ORDER BY timestamp")
        else {
            continue;
        };
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0));
        if let Ok(rows) = rows {
            for row in rows.filter_map(std::result::Result::ok) {
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
    let mut end = start;
    let mut count: u64 = 1;
    let mut day = all_ts[0].1.clone();

    for &(ts, ref d) in &all_ts[1..] {
        if ts.saturating_sub(end) > GAP_SECS {
            out.push(serde_json::json!({
                "start_utc": start, "end_utc": end,
                "n_epochs": count,  "day": day,
            }));
            start = ts;
            end = ts;
            count = 1;
            day = d.clone();
        } else {
            end = ts;
            count += 1;
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
    let start = msg
        .get("start_utc")
        .and_then(serde_json::Value::as_u64)
        .ok_or("missing required field: \"start_utc\" (u64)")?;
    let end = msg
        .get("end_utc")
        .and_then(serde_json::Value::as_u64)
        .ok_or("missing required field: \"end_utc\" (u64)")?;
    if end < start {
        return Err("\"end_utc\" must be >= \"start_utc\"".into());
    }

    let st = app.app_state();
    let skill_dir = skill_dir(&st);

    let result = crate::get_sleep_stages_impl(&skill_dir, start, end);
    let analysis = crate::analyze_sleep_stages(&result);
    let mut val = serde_json::to_value(&result).map_err(|e| format!("serialization error: {e}"))?;
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
    let a_start = msg
        .get("a_start_utc")
        .and_then(serde_json::Value::as_u64)
        .ok_or("missing required field: \"a_start_utc\" (u64)")?;
    let a_end = msg
        .get("a_end_utc")
        .and_then(serde_json::Value::as_u64)
        .ok_or("missing required field: \"a_end_utc\" (u64)")?;
    let b_start = msg
        .get("b_start_utc")
        .and_then(serde_json::Value::as_u64)
        .ok_or("missing required field: \"b_start_utc\" (u64)")?;
    let b_end = msg
        .get("b_end_utc")
        .and_then(serde_json::Value::as_u64)
        .ok_or("missing required field: \"b_end_utc\" (u64)")?;

    if a_end < a_start {
        return Err("\"a_end_utc\" must be >= \"a_start_utc\"".into());
    }
    if b_end < b_start {
        return Err("\"b_end_utc\" must be >= \"b_start_utc\"".into());
    }

    let st = app.app_state();
    let skill_dir = skill_dir(&st);

    let queue = app.state::<std::sync::Arc<crate::job_queue::JobQueue>>();

    // Quick count for time estimation (cheap read-only scan).
    let n_a = crate::load_embeddings_range(&skill_dir, a_start, a_end).len();
    let n_b = crate::load_embeddings_range(&skill_dir, b_start, b_end).len();
    let n = n_a + n_b;
    // Time estimate: KNN is O(n²) on GPU, training is O(epochs × edges).
    let ucfg_est = crate::load_umap_config(&skill_dir);
    let est_epochs = ucfg_est.n_epochs.clamp(50, 2000) as u64;
    let estimated_ms = 3000u64 + (n as u64) * (n as u64) / 20_000 + (n as u64) * est_epochs / 2000;

    let sd = skill_dir.clone();
    let prog_map = queue.progress_map();
    let ticket = queue.submit_with_id(estimated_ms, move |job_id| {
        let pm = prog_map;
        let cb: Box<dyn Fn(fast_umap::EpochProgress) + Send> = Box::new(move |ep| {
            let mut map = pm.lock_or_recover();
            map.insert(
                job_id,
                crate::job_queue::JobProgress {
                    epoch: ep.epoch,
                    total_epochs: ep.total_epochs,
                    loss: ep.loss,
                    best_loss: ep.best_loss,
                    elapsed_secs: ep.elapsed_secs,
                    epoch_ms: ep.epoch_ms,
                },
            );
        });
        umap_compute_inner(&sd, a_start, a_end, b_start, b_end, Some(cb)).map_err(|e| e.to_string())
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
    let job_id = msg
        .get("job_id")
        .and_then(serde_json::Value::as_u64)
        .ok_or("missing required field: \"job_id\" (u64)")?;

    let queue = app.state::<std::sync::Arc<crate::job_queue::JobQueue>>();
    let result = queue.poll(job_id);
    serde_json::to_value(&result).map_err(|e| e.to_string())
}

// umap_compute_inner, cache helpers, analyze_umap_points, load_embeddings_range,
// load_labels_range, find_label_for_epoch — all re-exported from skill_router above.

// Calibration commands — delegated to calibration.rs sub-module.

// DND + sleep schedule commands — delegated to dnd_sleep.rs sub-module.

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
    tokio::spawn(async move { skill_tts::tts_speak(text, voice).await });

    let mut resp = serde_json::json!({ "spoken": spoken });
    if let Some(v) = voice_echo {
        resp["voice"] = serde_json::Value::String(v);
    }
    Ok(resp)
}

// Screenshot + HealthKit commands — delegated to sub-modules.

// ── Central dispatcher ────────────────────────────────────────────────────────

/// Dispatch a named command to the appropriate handler function.
///
/// This is the single source of truth for command routing, used by both the
/// WebSocket server (`ws_server.rs`) and the HTTP API server (`api.rs`).
/// Adding a new command only requires updating this match arm.
pub async fn dispatch(app: &AppHandle, command: &str, msg: &Value) -> Result<Value, String> {
    match command {
        "status" => status(app),
        "calibrate" => calibrate(app).await,
        "timer" => timer(app).await,
        "notify" => notify(app, msg),
        "label" => label(app, msg),
        "search_labels" => search::search_labels(app, msg),
        "interactive_search" => search::interactive_search(app, msg),
        "search" => search::search(app, msg),
        "compare" => search::compare(app, msg),
        "session_metrics" => search::session_metrics(app, msg),
        "sessions" => sessions(app),
        "sleep" => sleep(app, msg),
        "umap" => umap(app, msg),
        "hooks_get" => hooks::hooks_get(app),
        "hooks_set" => hooks::hooks_set(app, msg),
        "hooks_status" => hooks::hooks_status(app),
        "hooks_suggest" => hooks::hooks_suggest(app, msg),
        "hooks_log" => hooks::hooks_log(app, msg),
        "umap_poll" => umap_poll(app, msg),
        // ── iroh NAT-traversing API tunnel auth ─────────────────────────
        "iroh_info" => {
            let rt = app.state::<skill_iroh::SharedIrohRuntime>();
            let auth = app.state::<skill_iroh::SharedIrohAuth>();
            skill_iroh::commands::iroh_info(&auth, &rt)
        }
        "iroh_totp_list" => {
            let auth = app.state::<skill_iroh::SharedIrohAuth>();
            skill_iroh::commands::iroh_totp_list(&auth)
        }
        "iroh_totp_create" => {
            let auth = app.state::<skill_iroh::SharedIrohAuth>();
            skill_iroh::commands::iroh_totp_create(&auth, msg)
        }
        "iroh_totp_qr" => {
            let auth = app.state::<skill_iroh::SharedIrohAuth>();
            skill_iroh::commands::iroh_totp_qr(&auth, msg)
        }
        "iroh_totp_revoke" => {
            let auth = app.state::<skill_iroh::SharedIrohAuth>();
            skill_iroh::commands::iroh_totp_revoke(&auth, msg)
        }
        "iroh_clients_list" => {
            let auth = app.state::<skill_iroh::SharedIrohAuth>();
            skill_iroh::commands::iroh_clients_list(&auth)
        }
        "iroh_client_register" => {
            let auth = app.state::<skill_iroh::SharedIrohAuth>();
            skill_iroh::commands::iroh_client_register(&auth, msg)
        }
        "iroh_client_revoke" => {
            let auth = app.state::<skill_iroh::SharedIrohAuth>();
            skill_iroh::commands::iroh_client_revoke(&auth, msg)
        }
        "iroh_client_set_scope" => {
            let auth = app.state::<skill_iroh::SharedIrohAuth>();
            skill_iroh::commands::iroh_client_set_scope(&auth, msg)
        }
        "iroh_phone_invite" => {
            let auth = app.state::<skill_iroh::SharedIrohAuth>();
            let rt = app.state::<skill_iroh::SharedIrohRuntime>();
            skill_iroh::commands::iroh_phone_invite(&auth, &rt, msg)
        }
        "iroh_scope_groups" => {
            let auth = app.state::<skill_iroh::SharedIrohAuth>();
            skill_iroh::commands::iroh_scope_groups(&auth)
        }
        "iroh_client_permissions" => {
            let auth = app.state::<skill_iroh::SharedIrohAuth>();
            skill_iroh::commands::iroh_client_permissions(&auth, msg)
        }
        "list_calibrations" => calibration::list_calibrations(app),
        "get_calibration" => calibration::get_calibration(app, msg),
        "create_calibration" => calibration::create_calibration(app, msg),
        "update_calibration" => calibration::update_calibration(app, msg),
        "delete_calibration" => calibration::delete_calibration(app, msg),
        "run_calibration" => calibration::run_calibration(app, msg).await,
        "say" => say(app, msg).await,
        "dnd" => dnd_sleep::dnd_status(app),
        "dnd_set" => dnd_sleep::dnd_set(app, msg),
        "sleep_schedule" => dnd_sleep::sleep_schedule(app),
        "sleep_schedule_set" => dnd_sleep::sleep_schedule_set(app, msg),
        "alarm_config" => dnd_sleep::alarm_config(app, msg),
        // ── Pipeline config ────────────────────────────────────────────────
        "get_max_pipeline_channels" => {
            let r = app.app_state();
            let s = r.lock_or_recover();
            Ok(serde_json::json!({
                "ok": true,
                "max_pipeline_channels": s.max_pipeline_channels,
                "eeg_channels_limit": skill_constants::EEG_CHANNELS,
            }))
        }
        "set_max_pipeline_channels" => {
            let val = msg.get("value").and_then(|v| v.as_u64()).unwrap_or(24) as usize;
            let clamped = val.clamp(2, 1024);
            {
                let r = app.app_state();
                let mut s = r.lock_or_recover();
                s.max_pipeline_channels = clamped;
            }
            crate::save_settings(app);
            Ok(serde_json::json!({
                "ok": true,
                "max_pipeline_channels": clamped,
                "dsp_limit": clamped.min(skill_constants::EEG_CHANNELS),
            }))
        }
        // ── LSL stream sink ───────────────────────────────────────────────
        "lsl_discover" => lsl_discover(app),
        "lsl_connect" => lsl_connect(app, msg),
        "lsl_iroh_start" => lsl_iroh_start(app).await,
        "lsl_iroh_status" => lsl_iroh_status(app),
        // ── Screenshot search ─────────────────────────────────────────────
        "search_screenshots" => screenshots::search_screenshots(app, msg),
        "screenshots_around" => screenshots::screenshots_around(app, msg),
        "search_screenshots_vision" => screenshots::search_screenshots_vision(app, msg),
        "search_screenshots_by_image_b64" => screenshots::search_screenshots_by_image_b64(app, msg),
        "screenshots_for_eeg" => screenshots::screenshots_for_eeg(app, msg),
        "eeg_for_screenshots" => screenshots::eeg_for_screenshots(app, msg),
        // ── Calendar ──────────────────────────────────────────────────────
        "calendar_events" => calendar::calendar_events(app, msg).await,
        "calendar_status" => calendar::calendar_status(app),
        "calendar_request_permission" => calendar::calendar_request_permission(app).await,
        // ── HealthKit ─────────────────────────────────────────────────────
        "health_sync" => health::health_sync(app, msg),
        "health_query" => health::health_query(app, msg),
        "health_summary" => health::health_summary(app, msg),
        "health_metric_types" => health::health_metric_types(app),
        // ── LLM commands (llm_chat is handled before dispatch — see api.rs) ──
        #[cfg(feature = "llm")]
        "llm_status" => llm_cmds::llm_status(app),
        #[cfg(feature = "llm")]
        "llm_start" => llm_cmds::llm_start(app).await,
        #[cfg(feature = "llm")]
        "llm_stop" => llm_cmds::llm_stop(app),
        #[cfg(feature = "llm")]
        "llm_catalog" => llm_cmds::llm_catalog(app),
        #[cfg(feature = "llm")]
        "llm_download" => llm_cmds::llm_download(app, msg),
        #[cfg(feature = "llm")]
        "llm_cancel_download" => llm_cmds::llm_cancel_download(app, msg),
        #[cfg(feature = "llm")]
        "llm_delete" => llm_cmds::llm_delete(app, msg),
        #[cfg(feature = "llm")]
        "llm_logs" => llm_cmds::llm_logs(app),
        #[cfg(feature = "llm")]
        "llm_select_model" => llm_cmds::llm_select_model(app, msg),
        #[cfg(feature = "llm")]
        "llm_select_mmproj" => llm_cmds::llm_select_mmproj(app, msg),
        #[cfg(feature = "llm")]
        "llm_pause_download" => llm_cmds::llm_pause_download(app, msg),
        #[cfg(feature = "llm")]
        "llm_resume_download" => llm_cmds::llm_resume_download(app, msg),
        #[cfg(feature = "llm")]
        "llm_refresh_catalog" => llm_cmds::llm_refresh_catalog(app),
        #[cfg(feature = "llm")]
        "llm_downloads" => llm_cmds::llm_downloads(app),
        #[cfg(feature = "llm")]
        "llm_set_autoload_mmproj" => llm_cmds::llm_set_autoload_mmproj(app, msg),
        #[cfg(feature = "llm")]
        "llm_add_model" => llm_cmds::llm_add_model(app, msg),
        #[cfg(feature = "llm")]
        "llm_hardware_fit" => llm_cmds::llm_hardware_fit(app, msg),
        other => Err(format!("unknown command: \"{other}\"")),
    }
}

// ── LSL stream commands ───────────────────────────────────────────────────────

/// `lsl_discover` — scan for LSL streams on the local network.
fn lsl_discover(_app: &AppHandle) -> Result<Value, String> {
    let streams = std::thread::spawn(|| skill_lsl::discover_streams(3.0))
        .join()
        .map_err(|_| "LSL discovery thread panicked".to_string())?;

    let list: Vec<Value> = streams.iter().map(|s| {
        serde_json::json!({
            "name": s.name,
            "type": s.stream_type,
            "channels": s.channel_count,
            "sample_rate": s.sample_rate,
            "source_id": s.source_id,
            "hostname": s.hostname,
        })
    }).collect();

    Ok(serde_json::json!({
        "ok": true,
        "command": "lsl_discover",
        "streams": list,
        "count": list.len(),
    }))
}

/// `lsl_connect` — connect to a specific LSL stream by name and start a recording session.
fn lsl_connect(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let name = msg.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());

    // Start session with "lsl" device kind and the stream name as target
    let target = name.unwrap_or_default();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::lifecycle::start_session(&app2, Some(format!("lsl:{target}")));
    });

    Ok(serde_json::json!({ "ok": true, "command": "lsl_connect" }))
}

/// `lsl_iroh_start` — start the rlsl-iroh sink to accept remote LSL streams.
async fn lsl_iroh_start(app: &AppHandle) -> Result<Value, String> {
    // Check if already running
    {
        let r = app.app_state();
        let s = r.lock_or_recover();
        if s.lsl_iroh_endpoint_id.is_some() {
            return Ok(serde_json::json!({
                "ok": true,
                "command": "lsl_iroh_start",
                "endpoint_id": s.lsl_iroh_endpoint_id,
                "already_running": true,
            }));
        }
    }

    let (adapter, endpoint_id) = skill_lsl::IrohLslAdapter::start_sink().await
        .map_err(|e| format!("rlsl-iroh sink failed: {e}"))?;

    // Store endpoint ID
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.lsl_iroh_endpoint_id = Some(endpoint_id.clone());
    }

    // Start session with this adapter
    let csv = crate::session_csv::new_csv_path(app);
    let cancel = tokio_util::sync::CancellationToken::new();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::session_runner::run_device_session(app2, cancel, csv, Box::new(adapter)).await;
    });

    eprintln!("[lsl-iroh] sink started, endpoint_id={endpoint_id}");

    Ok(serde_json::json!({
        "ok": true,
        "command": "lsl_iroh_start",
        "endpoint_id": endpoint_id,
    }))
}

/// `lsl_iroh_status` — return the rlsl-iroh sink endpoint ID.
fn lsl_iroh_status(app: &AppHandle) -> Result<Value, String> {
    let r = app.app_state();
    let s = r.lock_or_recover();
    Ok(serde_json::json!({
        "ok": true,
        "command": "lsl_iroh_status",
        "endpoint_id": s.lsl_iroh_endpoint_id,
        "running": s.lsl_iroh_endpoint_id.is_some(),
    }))
}
