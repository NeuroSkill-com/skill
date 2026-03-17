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

/// `hooks_get` — return raw hook rules (no runtime trigger state).
fn hooks_get(app: &AppHandle) -> Result<Value, String> {
    let st = app.app_state();
    let s = st.lock_or_recover();
    Ok(serde_json::json!({ "hooks": s.hooks }))
}

/// `hooks_set` — replace all hooks with the provided list.
///
/// Accepts `{ "hooks": [ { name, enabled, keywords, scenario, command, text,
///   distance_threshold, recent_limit }, … ] }`.
/// Each rule is sanitised identically to the Tauri `set_hooks` command.
fn hooks_set(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let raw: Vec<crate::settings::HookRule> = msg
        .get("hooks")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let clean: Vec<crate::settings::HookRule> = raw
        .into_iter()
        .filter_map(crate::settings_cmds::sanitize_hook)
        .take(100)
        .collect();

    {
        let st = app.app_state();
        let mut s = st.lock_or_recover();
        s.hooks = clean;
        let keep: std::collections::HashSet<String> =
            s.hooks.iter().map(|h| h.name.clone()).collect();
        s.hook_runtime.lock_or_recover().retain(|name, _| keep.contains(name));
    }
    crate::save_settings_handle(app);

    // Return the saved hooks so callers can verify sanitisation.
    let st = app.app_state();
    let s = st.lock_or_recover();
    Ok(serde_json::json!({ "hooks": s.hooks }))
}

/// `hooks_status` — return all hooks with last-trigger metadata.
fn hooks_status(app: &AppHandle) -> Result<Value, String> {
    let st = app.app_state();
    let s = st.lock_or_recover();
    let runtime = s.hook_runtime.lock_or_recover();
    let statuses: Vec<crate::settings::HookStatus> = s.hooks
        .iter()
        .cloned()
        .map(|hook| crate::settings::HookStatus {
            last_trigger: runtime.get(&hook.name).cloned(),
            hook,
        })
        .collect();
    Ok(serde_json::json!({ "hooks": statuses }))
}

/// `hooks_suggest` — suggest a hook distance threshold from existing labels + EEG embeddings.
fn hooks_suggest(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let keywords: Vec<String> = if let Some(arr) = msg.get("keywords").and_then(|v| v.as_array()) {
        arr.iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect()
    } else if let Some(s) = msg.get("keywords").and_then(|v| v.as_str()) {
        s.split(',')
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .collect()
    } else {
        Vec::new()
    };

    let skill_dir = {
        let st = app.app_state();
        skill_dir(&st)
    };

    // Keep this implementation parallel to settings_cmds::suggest_hook_distances.
    let empty = crate::settings_cmds::HookDistanceSuggestion {
        label_n: 0,
        ref_n: 0,
        sample_n: 0,
        eeg_min: 0.0,
        eeg_p25: 0.0,
        eeg_p50: 0.0,
        eeg_p75: 0.0,
        eeg_max: 0.0,
        suggested: 0.1,
        note: "No label data found. Keep the default 0.1 and adjust after recording sessions with labels.".to_owned(),
    };

    if keywords.is_empty() {
        return Ok(serde_json::json!({ "suggestion": empty }));
    }

    let labels_db = skill_dir.join(crate::constants::LABELS_FILE);
    if !labels_db.exists() {
        return Ok(serde_json::json!({ "suggestion": empty }));
    }
    let Ok(conn) = rusqlite::Connection::open_with_flags(
        &labels_db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) else {
        return Ok(serde_json::json!({ "suggestion": empty }));
    };

    let all_labels: Vec<(i64, String, u64, u64)> = {
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, text, eeg_start, eeg_end FROM labels WHERE length(trim(text)) > 0",
        ) else {
            return Ok(serde_json::json!({ "suggestion": empty }));
        };
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    };

    let matched: Vec<(i64, String, u64, u64)> = all_labels
        .into_iter()
        .filter(|(_, text, _, _)| keywords.iter().any(|k| crate::eeg_embeddings::fuzzy_match(k, text)))
        .collect();
    let label_n = matched.len();
    if label_n == 0 {
        let out = crate::settings_cmds::HookDistanceSuggestion {
            note: format!(
                "No labels matched your keywords ({}). Add labels to your sessions first.",
                keywords.join(", ")
            ),
            ..empty
        };
        return Ok(serde_json::json!({ "suggestion": out }));
    }

    let refs: Vec<Vec<f32>> = matched
        .iter()
        .filter_map(|(_, _, eeg_start, eeg_end)| crate::label_index::mean_eeg_for_window(&skill_dir, *eeg_start, *eeg_end))
        .collect();
    let ref_n = refs.len();
    if ref_n == 0 {
        let out = crate::settings_cmds::HookDistanceSuggestion {
            label_n,
            note: format!(
                "{label_n} label(s) matched but no EEG recordings cover their time windows yet.",
            ),
            ..empty
        };
        return Ok(serde_json::json!({ "suggestion": out }));
    }

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
        date_dirs.sort_by(|a, b| b.cmp(a));

        let mut out: Vec<Vec<f32>> = Vec::new();
        let per_day = (max / date_dirs.len().max(1)).max(20);

        for dir in &date_dirs {
            let db = dir.join(crate::constants::SQLITE_FILE);
            if !db.exists() {
                continue;
            }
            let Ok(conn) = rusqlite::Connection::open_with_flags(
                &db,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            ) else {
                continue;
            };

            let Ok(mut stmt) = conn.prepare(
                "SELECT eeg_embedding FROM embeddings ORDER BY timestamp DESC LIMIT ?1",
            ) else {
                continue;
            };

            let blobs: Vec<Vec<f32>> = stmt
                .query_map(rusqlite::params![per_day as i64], |r| r.get::<_, Vec<u8>>(0))
                .map(|rows| {
                    rows.flatten()
                        .map(|b| {
                            b.chunks_exact(4)
                                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                                .collect()
                        })
                        .collect()
                })
                .unwrap_or_default();

            out.extend(blobs);
            if out.len() >= max {
                break;
            }
        }
        out
    }

    let samples = sample_recent_eeg_embeddings(&skill_dir, 300);
    let sample_n = samples.len();
    if sample_n == 0 {
        let out = crate::settings_cmds::HookDistanceSuggestion {
            label_n,
            ref_n,
            note: "No recent EEG embeddings found. Record a session first.".to_owned(),
            ..empty
        };
        return Ok(serde_json::json!({ "suggestion": out }));
    }

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
        let out = crate::settings_cmds::HookDistanceSuggestion {
            label_n,
            ref_n,
            sample_n,
            note: "Could not compute distances (dimension mismatch).".to_owned(),
            ..empty
        };
        return Ok(serde_json::json!({ "suggestion": out }));
    }

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
    let eeg_max = *distances.last().unwrap_or(&0.0);
    let suggested = ((eeg_p25 * 100.0).round() / 100.0).clamp(0.01, 0.99);

    let out = crate::settings_cmds::HookDistanceSuggestion {
        label_n,
        ref_n,
        sample_n,
        eeg_min,
        eeg_p25,
        eeg_p50,
        eeg_p75,
        eeg_max,
        suggested,
        note: format!(
            "{label_n} label(s) matched ({ref_n} with EEG data). Distribution of {n} distances — min {eeg_min:.3}, p25 {eeg_p25:.3}, median {eeg_p50:.3}, p75 {eeg_p75:.3}, max {eeg_max:.3}. Suggested threshold {suggested:.2} (p25 = fairly strict match)."
        ),
    };

    Ok(serde_json::json!({ "suggestion": out }))
}

/// `hooks_log` — fetch paginated hook trigger audit rows from hooks.sqlite.
fn hooks_log(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let limit = msg
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as i64)
        .unwrap_or(50)
        .clamp(1, 500);
    let offset = msg
        .get("offset")
        .and_then(|v| v.as_u64())
        .map(|v| v as i64)
        .unwrap_or(0)
        .max(0);

    let skill_dir = {
        let st = app.app_state();
        skill_dir(&st)
    };
    let Some(log) = skill_data::hooks_log::HooksLog::open(&skill_dir) else {
        return Ok(serde_json::json!({ "rows": [], "total": 0, "limit": limit, "offset": offset }));
    };

    let rows = log.query(limit, offset);
    let total = log.count();
    Ok(serde_json::json!({
        "rows": rows,
        "total": total,
        "limit": limit,
        "offset": offset
    }))
}

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

    let (skill_dir, model_code) = crate::read_state(
        &app.app_state(),
        |s| (s.skill_dir.clone(), s.text_embedding_model.clone()),
    );

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

    let skill_dir = skill_dir(&app.app_state());

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

    let skill_dir = skill_dir(&app.app_state());
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

    let st = app.app_state();
    let skill_dir = skill_dir(&st);

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
    let s = app.app_state();
    let guard = s.lock_or_recover();
    Ok(serde_json::json!({ "profiles": guard.calibration_profiles }))
}

/// `get_calibration { "id": "…" }` — return a single profile by ID.
pub fn get_calibration(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let id = msg.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"id\" (string)".to_string())?;
    let s = app.app_state();
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

    let st = app.app_state();
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

    let st = app.app_state();
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
    let st = app.app_state();
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

    let (skill_dir, _model_code) = crate::read_state(
        &app.app_state(),
        |s| (s.skill_dir.clone(), s.text_embedding_model.clone()),
    );

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
    let state = app.app_state();
    let s = state.lock_or_recover();
    let (status, model_name) = crate::llm::cell_status(&s.llm.state_cell);
    let (n_ctx, supports_vision) = s.llm.state_cell.lock().unwrap()
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
        let st = app.app_state();
        let s = st.lock_or_recover();
        (
            s.llm.config.clone(),
            s.llm.catalog.clone(),
            s.llm.logs.clone(),
            s.llm.state_cell.clone(),
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

    let emitter = crate::llm::TauriEmitter(app.clone());
    crate::llm::push_log(&emitter, &log_buf, "info", "llm_start command received via WebSocket");

    let emitter_arc: std::sync::Arc<dyn crate::llm::LlmEventEmitter> = std::sync::Arc::new(emitter);
    let new_state = tokio::task::spawn_blocking(move || {
        crate::llm::init(&config, &catalog, emitter_arc, log_buf, &skill_dir)
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
        let st = app.app_state();
        let s = st.lock_or_recover();
        (s.llm.state_cell.clone(), s.llm.logs.clone())
    };
    let server_state = { cell.lock().unwrap().take() };
    if let Some(server_state) = server_state {
        let emitter = crate::llm::TauriEmitter(app.clone());
        crate::llm::push_log(&emitter, &log_buf, "info", "llm_stop command received via WebSocket");
        match std::sync::Arc::try_unwrap(server_state) {
            Ok(owned) => owned.shutdown(),
            Err(arc)  => drop(arc),
        }
        crate::llm::push_log(&emitter, &log_buf, "info", "LLM server stopped");
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
    let state = app.app_state();
    let mut s = state.lock_or_recover();
    // Sync in-flight downloads into the catalog so callers see live progress.
    let downloads = s.llm.downloads.clone();
    for (filename, prog_arc) in &downloads {
        if let Ok(prog) = prog_arc.lock() {
            if let Some(entry) = s.llm.catalog.entries
                .iter_mut()
                .find(|e| &e.filename == filename)
            {
                entry.state      = prog.state.clone();
                entry.status_msg = prog.status_msg.clone();
                entry.progress   = prog.progress;
            }
        }
    }
    serde_json::to_value(&s.llm.catalog).map_err(|e| e.to_string())
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
        app.app_state(),
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
    crate::llm::cmds::cancel_llm_download_with_app(filename.clone(), app, app.app_state());
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
        app.app_state(),
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
    let state = app.app_state();
    let s = state.lock_or_recover();
    let log = s.llm.logs.lock().unwrap();
    let logs: Vec<&crate::llm::LlmLogEntry> = log.iter().collect();
    Ok(serde_json::json!({ "logs": logs, "count": logs.len() }))
}

/// `llm_select_model` — set the active text model by filename.
///
/// ```json
/// { "command": "llm_select_model", "filename": "Qwen_Qwen3.5-4B-Q4_K_M.gguf" }
/// → { "command": "llm_select_model", "ok": true, "filename": "...", "active_model": "..." }
/// ```
#[cfg(feature = "llm")]
fn llm_select_model(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_select_model: 'filename' field required (string)".to_string())?
        .to_string();
    crate::llm::cmds::set_llm_active_model(
        filename.clone(),
        app.clone(),
        app.app_state(),
    );
    let state = app.app_state();
    let s = state.lock_or_recover();
    Ok(serde_json::json!({
        "filename": filename,
        "active_model": s.llm.catalog.active_model,
        "active_mmproj": s.llm.catalog.active_mmproj,
    }))
}

/// `llm_select_mmproj` — set the active vision projector by filename (empty to disable).
///
/// ```json
/// { "command": "llm_select_mmproj", "filename": "mmproj-Qwen_Qwen3.5-4B-BF16.gguf" }
/// → { "command": "llm_select_mmproj", "ok": true, "filename": "...", "active_mmproj": "..." }
/// ```
#[cfg(feature = "llm")]
fn llm_select_mmproj(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .unwrap_or("")
        .to_string();
    crate::llm::cmds::set_llm_active_mmproj(
        filename.clone(),
        app.clone(),
        app.app_state(),
    );
    let state = app.app_state();
    let s = state.lock_or_recover();
    Ok(serde_json::json!({
        "filename": filename,
        "active_model": s.llm.catalog.active_model,
        "active_mmproj": s.llm.catalog.active_mmproj,
    }))
}

/// `llm_pause_download` — pause an in-progress model download.
///
/// ```json
/// { "command": "llm_pause_download", "filename": "Qwen3-1.7B-Q4_K_M.gguf" }
/// → { "command": "llm_pause_download", "ok": true, "filename": "..." }
/// ```
#[cfg(feature = "llm")]
fn llm_pause_download(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_pause_download: 'filename' field required".to_string())?
        .to_string();
    crate::llm::cmds::pause_llm_download(
        filename.clone(),
        app.app_state(),
    );
    Ok(serde_json::json!({ "filename": filename }))
}

/// `llm_resume_download` — resume a paused model download.
///
/// ```json
/// { "command": "llm_resume_download", "filename": "Qwen3-1.7B-Q4_K_M.gguf" }
/// → { "command": "llm_resume_download", "ok": true, "filename": "..." }
/// ```
#[cfg(feature = "llm")]
fn llm_resume_download(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_resume_download: 'filename' field required".to_string())?
        .to_string();
    crate::llm::cmds::resume_llm_download(
        filename.clone(),
        app.clone(),
        app.app_state(),
    );
    Ok(serde_json::json!({ "filename": filename }))
}

/// `llm_refresh_catalog` — re-probe the HF Hub cache and update download states.
///
/// ```json
/// { "command": "llm_refresh_catalog" }
/// → { "command": "llm_refresh_catalog", "ok": true }
/// ```
#[cfg(feature = "llm")]
fn llm_refresh_catalog(app: &AppHandle) -> Result<Value, String> {
    crate::llm::cmds::refresh_llm_catalog(
        app.clone(),
        app.app_state(),
    );
    Ok(serde_json::json!({}))
}

/// `llm_downloads` — list all downloads (active, paused, completed, failed).
///
/// ```json
/// { "command": "llm_downloads" }
/// → { "command": "llm_downloads", "ok": true, "downloads": [...] }
/// ```
#[cfg(feature = "llm")]
fn llm_downloads(app: &AppHandle) -> Result<Value, String> {
    let items = crate::llm::cmds::get_llm_downloads(
        app.app_state(),
    );
    Ok(serde_json::json!({ "downloads": items, "count": items.len() }))
}

/// `llm_set_autoload_mmproj` — toggle whether the vision projector auto-loads on start.
///
/// ```json
/// { "command": "llm_set_autoload_mmproj", "enabled": true }
/// → { "command": "llm_set_autoload_mmproj", "ok": true, "enabled": true }
/// ```
#[cfg(feature = "llm")]
fn llm_set_autoload_mmproj(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let enabled = msg["enabled"]
        .as_bool()
        .ok_or_else(|| "llm_set_autoload_mmproj: 'enabled' field required (bool)".to_string())?;
    crate::llm::cmds::set_llm_autoload_mmproj(
        enabled,
        app.clone(),
        app.app_state(),
    );
    Ok(serde_json::json!({ "enabled": enabled }))
}

/// `llm_add_model` — add an external HuggingFace model to the catalog and optionally download it.
///
/// Creates a new catalog entry from the repo and filename if it doesn't already exist.
/// Metadata (quant, mmproj, family) is inferred from the filename/repo.
///
/// ```json
/// { "command": "llm_add_model", "repo": "bartowski/Phi-4-mini-reasoning-GGUF",
///   "filename": "Phi-4-mini-reasoning-Q4_K_M.gguf", "download": true }
/// → { "command": "llm_add_model", "ok": true, "filename": "...", "repo": "..." }
/// ```
#[cfg(feature = "llm")]
fn llm_add_model(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let repo = msg["repo"]
        .as_str()
        .ok_or_else(|| "llm_add_model: 'repo' field required (string, e.g. \"bartowski/Phi-4-GGUF\")".to_string())?
        .to_string();
    let filename = msg["filename"]
        .as_str()
        .ok_or_else(|| "llm_add_model: 'filename' field required (string, e.g. \"Phi-4-Q4_K_M.gguf\")".to_string())?
        .to_string();
    let size_gb = msg["size_gb"].as_f64().map(|v| v as f32);
    let mmproj = msg["mmproj"].as_str().map(|s| s.to_string());
    let download = msg.get("download").and_then(|v| v.as_bool());

    let result = crate::llm::cmds::add_llm_model(
        repo.clone(),
        filename.clone(),
        size_gb,
        mmproj.clone(),
        download,
        app.clone(),
        app.app_state(),
    )?;
    Ok(serde_json::json!({ "filename": result, "repo": repo, "mmproj": mmproj }))
}

/// `llm_hardware_fit` — check which models fit in available memory.
///
/// ```json
/// { "command": "llm_hardware_fit" }
/// → { "command": "llm_hardware_fit", "ok": true,
///     "fits": [{ "filename": "...", "fit_level": "good", "run_mode": "gpu", ... }, …] }
/// ```
#[cfg(feature = "llm")]
fn llm_hardware_fit(app: &AppHandle, _msg: &Value) -> Result<Value, String> {
    let result = crate::llm::cmds::get_model_hardware_fit(
        app.app_state(),
    );
    Ok(serde_json::json!({ "fits": result }))
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
        "search_labels"       => search_labels(app, msg),
        "interactive_search"  => interactive_search(app, msg),
        "search"              => search(app, msg),
        "compare"             => compare(app, msg),
        "session_metrics"     => session_metrics(app, msg),
        "sessions"            => sessions(app),
        "sleep"               => sleep(app, msg),
        "umap"                => umap(app, msg),
        "hooks_get"           => hooks_get(app),
        "hooks_set"           => hooks_set(app, msg),
        "hooks_status"        => hooks_status(app),
        "hooks_suggest"       => hooks_suggest(app, msg),
        "hooks_log"           => hooks_log(app, msg),
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
        #[cfg(feature = "llm")]
        "llm_select_model"    => llm_select_model(app, msg),
        #[cfg(feature = "llm")]
        "llm_select_mmproj"   => llm_select_mmproj(app, msg),
        #[cfg(feature = "llm")]
        "llm_pause_download"  => llm_pause_download(app, msg),
        #[cfg(feature = "llm")]
        "llm_resume_download" => llm_resume_download(app, msg),
        #[cfg(feature = "llm")]
        "llm_refresh_catalog" => llm_refresh_catalog(app),
        #[cfg(feature = "llm")]
        "llm_downloads"       => llm_downloads(app),
        #[cfg(feature = "llm")]
        "llm_set_autoload_mmproj" => llm_set_autoload_mmproj(app, msg),
        #[cfg(feature = "llm")]
        "llm_add_model"       => llm_add_model(app, msg),
        #[cfg(feature = "llm")]
        "llm_hardware_fit"    => llm_hardware_fit(app, msg),
        other                 => Err(format!("unknown command: \"{other}\"")),
    }
}
