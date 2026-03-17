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

mod hooks;
mod search;
#[cfg(feature = "llm")]
mod llm_cmds;

use crate::AppStateExt;
use crate::MutexExt;
use crate::skill_dir;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::constants::SQLITE_FILE;
use skill_eeg::eeg_model_config::LatestEpochMetrics;
use crate::unix_secs;

// ── Re-exports from skill-router ──────────────────────────────────────────────

pub use skill_router::{
    r1, r2, r3, r1d, r2d,
    RoundedBands, RoundedScores,
    umap_compute_inner,
};


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

    // ── Hooks — most recent trigger across all hooks ─────────────────────────
    let hooks_summary = {
        let runtime = guard.hook_runtime.lock_or_recover();
        let total_hooks   = guard.hooks.len();
        let enabled_hooks = guard.hooks.iter().filter(|h| h.enabled).count();

        // Find the most recent trigger across all hooks.
        let latest: Option<(&String, &crate::settings::HookLastTrigger)> = runtime.iter()
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

    let s = app.app_state();
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
        let conn = match skill_data::util::open_readonly(&db_path)
        { Ok(c) => c, Err(_) => continue };
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

    let st = app.app_state();
    let skill_dir = skill_dir(&st);

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

// umap_compute_inner, cache helpers, analyze_umap_points, load_embeddings_range,
// load_labels_range, find_label_for_epoch — all re-exported from skill_router above.

// ── calibration profile commands ──────────────────────────────────────────────

/// `list_calibrations` — return all saved calibration profiles.
pub fn list_calibrations(app: &AppHandle) -> Result<Value, String> {
    let profiles = crate::calibration_service::list_profiles(app);
    Ok(serde_json::json!({ "profiles": profiles }))
}

/// `get_calibration { "id": "…" }` — return a single profile by ID.
pub fn get_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let id = msg.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"id\" (string)".to_string())?;
    crate::calibration_service::get_profile(app, id)
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
        id:                   String::new(), // overwritten by create_profile
        name,
        actions,
        break_duration_secs:  break_secs,
        loop_count,
        auto_start,
        last_calibration_utc: None,
    };

    let created = crate::calibration_service::create_profile(app, profile);
    Ok(serde_json::json!({ "profile": created }))
}

/// `update_calibration { "id": "…", …fields… }` — partial-update an existing profile.
pub fn update_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let id = msg.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"id\"".to_string())?;

    // Fetch the existing profile, apply partial updates from msg, then save.
    let mut profile = crate::calibration_service::get_profile(app, id)
        .ok_or_else(|| format!("profile not found: {id}"))?;

    if let Some(name) = msg.get("name").and_then(|v| v.as_str()) {
        profile.name = name.to_owned();
    }
    if let Some(actions) = msg.get("actions").and_then(|v| serde_json::from_value::<Vec<crate::CalibrationAction>>(v.clone()).ok()) {
        if !actions.is_empty() { profile.actions = actions; }
    }
    if let Some(b) = msg.get("break_duration_secs").and_then(|v| v.as_u64()) {
        profile.break_duration_secs = b as u32;
    }
    if let Some(n) = msg.get("loop_count").and_then(|v| v.as_u64()) {
        profile.loop_count = n as u32;
    }
    if let Some(a) = msg.get("auto_start").and_then(|v| v.as_bool()) {
        profile.auto_start = a;
    }

    let updated = crate::calibration_service::update_profile(app, profile)?;
    Ok(serde_json::json!({ "profile": updated }))
}

/// `delete_calibration { "id": "…" }` — remove a profile.
pub fn delete_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let id = msg.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"id\"".to_string())?;
    crate::calibration_service::delete_profile(app, id)?;
    Ok(serde_json::json!({}))
}

/// `run_calibration { "id"?: "…" }` — open the calibration window and start
/// the specified (or active) profile immediately.
pub async fn run_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let profile_id = msg.get("id").and_then(|v| v.as_str()).map(str::to_owned);
    crate::open_calibration_window_inner(app, profile_id, true).await?;
    Ok(serde_json::json!({}))
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
    let s = app.app_state();
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
        let s = app.app_state();
        let g = s.lock_or_recover();
        g.dnd_config.focus_mode_identifier.clone()
    };

    let ok = skill_data::dnd::set_dnd(enabled, &mode_id);
    if ok {
        let s = app.app_state();
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

// ── sleep schedule ────────────────────────────────────────────────────────────

/// `sleep_schedule` — return the current sleep schedule configuration.
///
/// ```json
/// { "command": "sleep_schedule" }
/// ```
///
/// Response:
/// ```json
/// { "bedtime": "23:00", "wake_time": "07:00", "preset": "default",
///   "duration_minutes": 480 }
/// ```
pub fn sleep_schedule(app: &AppHandle) -> Result<Value, String> {
    let s = app.app_state();
    let guard = s.lock_or_recover();
    let cfg = &guard.sleep_config;
    let dur = cfg.duration_minutes();
    Ok(serde_json::json!({
        "bedtime":          cfg.bedtime,
        "wake_time":        cfg.wake_time,
        "preset":           cfg.preset,
        "duration_minutes": dur,
    }))
}

/// `sleep_schedule_set` — update the sleep schedule.
///
/// ```json
/// { "command": "sleep_schedule_set", "bedtime": "23:00", "wake_time": "07:00", "preset": "default" }
/// ```
///
/// All fields are optional — only the fields present are updated; omitted
/// fields keep their current value.
pub fn sleep_schedule_set(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    use crate::settings::SleepPreset;

    let s = app.app_state();
    let mut guard = s.lock_or_recover();

    if let Some(v) = msg.get("bedtime").and_then(|v| v.as_str()) {
        guard.sleep_config.bedtime = v.to_string();
    }
    if let Some(v) = msg.get("wake_time").and_then(|v| v.as_str()) {
        guard.sleep_config.wake_time = v.to_string();
    }
    if let Some(v) = msg.get("preset").and_then(|v| v.as_str()) {
        guard.sleep_config.preset = match v {
            "default"       => SleepPreset::Default,
            "early_bird"    => SleepPreset::EarlyBird,
            "night_owl"     => SleepPreset::NightOwl,
            "short_sleeper" => SleepPreset::ShortSleeper,
            "long_sleeper"  => SleepPreset::LongSleeper,
            _               => SleepPreset::Custom,
        };
    }

    let cfg = guard.sleep_config.clone();
    let dur = cfg.duration_minutes();
    drop(guard);

    crate::save_settings(app);

    Ok(serde_json::json!({
        "ok":               true,
        "bedtime":          cfg.bedtime,
        "wake_time":        cfg.wake_time,
        "preset":           cfg.preset,
        "duration_minutes": dur,
    }))
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
    tokio::spawn(async move { skill_tts::tts_speak(text, voice).await });

    let mut resp = serde_json::json!({ "spoken": spoken });
    if let Some(v) = voice_echo {
        resp["voice"] = serde_json::Value::String(v);
    }
    Ok(resp)
}

// ── Screenshot search commands ────────────────────────────────────────────────

/// `search_screenshots` — search screenshots by OCR text (semantic or substring).
///
/// Accepts:
/// ```json
/// { "command": "search_screenshots", "query": "some text", "k": 20, "mode": "semantic" }
/// ```
/// - `query` (required): the search text.
/// - `k` (optional, default 20): max results.
/// - `mode` (optional, default "semantic"): "semantic" or "substring".
fn search_screenshots(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let query = msg.get("query")
        .and_then(|v| v.as_str())
        .ok_or("missing \"query\" field")?
        .to_owned();
    let k    = msg.get("k").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let mode = msg.get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("semantic")
        .to_owned();

    let (skill_dir, store) = {
        let st = app.app_state();
        let s  = st.lock_or_recover();
        (s.skill_dir.clone(), s.screenshot_store.clone())
    };

    let embedder = std::sync::Arc::clone(&*app.state::<std::sync::Arc<crate::EmbedderState>>());

    let store = store
        .or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))
        .ok_or("screenshot store not available")?;

    let results = match mode.as_str() {
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
    };

    Ok(serde_json::json!({
        "query":   query,
        "mode":    mode,
        "k":       k,
        "count":   results.len(),
        "results": results,
    }))
}

/// `screenshots_around` — find screenshots near a given unix timestamp.
///
/// Accepts:
/// ```json
/// { "command": "screenshots_around", "timestamp": 1740412800, "window_secs": 60 }
/// ```
fn screenshots_around(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let timestamp   = msg.get("timestamp")
        .and_then(|v| v.as_i64())
        .ok_or("missing \"timestamp\" field")?;
    let window_secs = msg.get("window_secs")
        .and_then(|v| v.as_i64())
        .unwrap_or(60) as i32;

    let (skill_dir, store) = {
        let st = app.app_state();
        let s  = st.lock_or_recover();
        (s.skill_dir.clone(), s.screenshot_store.clone())
    };

    let store = store
        .or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new))
        .ok_or("screenshot store not available")?;

    let results = crate::screenshot::get_around(&store, timestamp, window_secs);

    Ok(serde_json::json!({
        "timestamp":   timestamp,
        "window_secs": window_secs,
        "count":       results.len(),
        "results":     results,
    }))
}

// ── HealthKit commands ─────────────────────────────────────────────────────────

/// `health_sync` — upsert Apple HealthKit data from the iOS companion app.
///
/// Accepts a batch payload with typed sample arrays.  Idempotent — sending
/// the same samples again is safe (deduplication by source + timestamps).
///
/// ```json
/// { "command": "health_sync",
///   "sleep": [{ "source_id": "watch", "start_utc": 1740000000, "end_utc": 1740028800, "value": "REM" }],
///   "workouts": [{ "workout_type": "Running", "start_utc": 1740030000, "end_utc": 1740033600,
///                   "duration_secs": 3600, "active_calories": 450, "distance_meters": 8000 }],
///   "heart_rate": [{ "timestamp": 1740030000, "bpm": 72.0, "context": "sedentary" }],
///   "steps": [{ "start_utc": 1740000000, "end_utc": 1740086400, "count": 9500 }],
///   "mindfulness": [{ "start_utc": 1740040000, "end_utc": 1740041200 }],
///   "metrics": [{ "metric_type": "restingHeartRate", "timestamp": 1740000000, "value": 58.0, "unit": "bpm" }]
/// }
/// ```
fn health_sync(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let payload: skill_data::health_store::HealthSyncPayload =
        serde_json::from_value(msg.clone()).map_err(|e| format!("invalid health_sync payload: {e}"))?;

    let st = app.app_state();
    let store = {
        let s = st.lock_or_recover();
        s.health_store.clone()
    };
    let store = store.ok_or_else(|| "health store not available".to_string())?;
    let result = store.sync(&payload);
    eprintln!(
        "[health] sync: sleep={} workouts={} hr={} steps={} mindful={} metrics={}",
        result.sleep_upserted, result.workouts_upserted, result.heart_rate_upserted,
        result.steps_upserted, result.mindfulness_upserted, result.metrics_upserted,
    );
    serde_json::to_value(&result).map_err(|e| e.to_string())
}

/// `health_query` — query stored HealthKit data by type and time range.
///
/// ```json
/// { "command": "health_query", "type": "sleep", "start_utc": 1740000000, "end_utc": 1740086400, "limit": 100 }
/// ```
///
/// Valid `type` values: `sleep`, `workouts`, `heart_rate`, `steps`, `metrics`.
/// For `metrics`, an additional `metric_type` field is required (e.g. `"restingHeartRate"`).
fn health_query(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let data_type = msg.get("type").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"type\" (sleep|workouts|heart_rate|steps|metrics)".to_string())?;
    let start_utc = msg.get("start_utc").and_then(|v| v.as_i64()).unwrap_or(0);
    let end_utc   = msg.get("end_utc").and_then(|v| v.as_i64())
        .unwrap_or_else(|| std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64);
    let limit     = msg.get("limit").and_then(|v| v.as_i64()).unwrap_or(500).clamp(1, 10_000);

    let st = app.app_state();
    let store = {
        let s = st.lock_or_recover();
        s.health_store.clone()
    };
    let store = store.ok_or_else(|| "health store not available".to_string())?;

    match data_type {
        "sleep" => {
            let rows = store.query_sleep(start_utc, end_utc, limit);
            Ok(serde_json::json!({ "type": "sleep", "count": rows.len(), "results": rows }))
        }
        "workouts" => {
            let rows = store.query_workouts(start_utc, end_utc, limit);
            Ok(serde_json::json!({ "type": "workouts", "count": rows.len(), "results": rows }))
        }
        "heart_rate" => {
            let rows = store.query_heart_rate(start_utc, end_utc, limit);
            Ok(serde_json::json!({ "type": "heart_rate", "count": rows.len(), "results": rows }))
        }
        "steps" => {
            let rows = store.query_steps(start_utc, end_utc, limit);
            Ok(serde_json::json!({ "type": "steps", "count": rows.len(), "results": rows }))
        }
        "metrics" => {
            let metric_type = msg.get("metric_type").and_then(|v| v.as_str())
                .ok_or_else(|| "\"metric_type\" required when type=\"metrics\" (e.g. \"restingHeartRate\")".to_string())?;
            let rows = store.query_metrics(metric_type, start_utc, end_utc, limit);
            Ok(serde_json::json!({ "type": "metrics", "metric_type": metric_type, "count": rows.len(), "results": rows }))
        }
        other => Err(format!("invalid health data type: \"{other}\" — must be sleep|workouts|heart_rate|steps|metrics")),
    }
}

/// `health_summary` — aggregate counts for a time range.
///
/// ```json
/// { "command": "health_summary", "start_utc": 1740000000, "end_utc": 1740086400 }
/// ```
fn health_summary(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let start_utc = msg.get("start_utc").and_then(|v| v.as_i64()).unwrap_or(0);
    let end_utc   = msg.get("end_utc").and_then(|v| v.as_i64())
        .unwrap_or_else(|| std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64);

    let st = app.app_state();
    let store = {
        let s = st.lock_or_recover();
        s.health_store.clone()
    };
    let store = store.ok_or_else(|| "health store not available".to_string())?;
    Ok(store.summary(start_utc, end_utc))
}

/// `health_metric_types` — list all distinct metric types in the database.
fn health_metric_types(app: &AppHandle) -> Result<Value, String> {
    let st = app.app_state();
    let store = {
        let s = st.lock_or_recover();
        s.health_store.clone()
    };
    let store = store.ok_or_else(|| "health store not available".to_string())?;
    let types = store.list_metric_types();
    Ok(serde_json::json!({ "metric_types": types }))
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
        "search_labels"       => search::search_labels(app, msg),
        "interactive_search"  => search::interactive_search(app, msg),
        "search"              => search::search(app, msg),
        "compare"             => search::compare(app, msg),
        "session_metrics"     => search::session_metrics(app, msg),
        "sessions"            => sessions(app),
        "sleep"               => sleep(app, msg),
        "umap"                => umap(app, msg),
        "hooks_get"           => hooks::hooks_get(app),
        "hooks_set"           => hooks::hooks_set(app, msg),
        "hooks_status"        => hooks::hooks_status(app),
        "hooks_suggest"       => hooks::hooks_suggest(app, msg),
        "hooks_log"           => hooks::hooks_log(app, msg),
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
        "sleep_schedule"      => sleep_schedule(app),
        "sleep_schedule_set"  => sleep_schedule_set(app, msg),
        // ── Screenshot search ─────────────────────────────────────────────
        "search_screenshots"  => search_screenshots(app, msg),
        "screenshots_around"  => screenshots_around(app, msg),
        // ── HealthKit ─────────────────────────────────────────────────────
        "health_sync"         => health_sync(app, msg),
        "health_query"        => health_query(app, msg),
        "health_summary"      => health_summary(app, msg),
        "health_metric_types" => health_metric_types(app),
        // ── LLM commands (llm_chat is handled before dispatch — see api.rs) ──
        #[cfg(feature = "llm")]
        "llm_status"          => llm_cmds::llm_status(app),
        #[cfg(feature = "llm")]
        "llm_start"           => llm_cmds::llm_start(app).await,
        #[cfg(feature = "llm")]
        "llm_stop"            => llm_cmds::llm_stop(app),
        #[cfg(feature = "llm")]
        "llm_catalog"         => llm_cmds::llm_catalog(app),
        #[cfg(feature = "llm")]
        "llm_download"        => llm_cmds::llm_download(app, msg),
        #[cfg(feature = "llm")]
        "llm_cancel_download" => llm_cmds::llm_cancel_download(app, msg),
        #[cfg(feature = "llm")]
        "llm_delete"          => llm_cmds::llm_delete(app, msg),
        #[cfg(feature = "llm")]
        "llm_logs"            => llm_cmds::llm_logs(app),
        #[cfg(feature = "llm")]
        "llm_select_model"    => llm_cmds::llm_select_model(app, msg),
        #[cfg(feature = "llm")]
        "llm_select_mmproj"   => llm_cmds::llm_select_mmproj(app, msg),
        #[cfg(feature = "llm")]
        "llm_pause_download"  => llm_cmds::llm_pause_download(app, msg),
        #[cfg(feature = "llm")]
        "llm_resume_download" => llm_cmds::llm_resume_download(app, msg),
        #[cfg(feature = "llm")]
        "llm_refresh_catalog" => llm_cmds::llm_refresh_catalog(app),
        #[cfg(feature = "llm")]
        "llm_downloads"       => llm_cmds::llm_downloads(app),
        #[cfg(feature = "llm")]
        "llm_set_autoload_mmproj" => llm_cmds::llm_set_autoload_mmproj(app, msg),
        #[cfg(feature = "llm")]
        "llm_add_model"       => llm_cmds::llm_add_model(app, msg),
        #[cfg(feature = "llm")]
        "llm_hardware_fit"    => llm_cmds::llm_hardware_fit(app, msg),
        other                 => Err(format!("unknown command: \"{other}\"")),
    }
}
