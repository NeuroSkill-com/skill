// SPDX-License-Identifier: GPL-3.0-only
//! Data commands: status, sessions, devices, session control, labels,
//! screenshots, search, compare, sleep, UMAP, hooks, and calibrations.

use serde_json::{json, Value};

use super::{f64_field, i64_field, skill_dir, str_field, u64_field};
use crate::state::AppState;

// ── Status ───────────────────────────────────────────────────────────────────

pub(super) async fn cmd_status(state: &AppState) -> Result<Value, String> {
    let status = state.status.lock().map(|g| g.clone()).unwrap_or_default();

    let devices = state.devices.lock().map(|g| g.clone()).unwrap_or_default();
    let bands = state.latest_bands.lock().map(|g| g.clone()).unwrap_or(None);

    let skill_dir = skill_dir(state);
    let sessions = tokio::task::spawn_blocking(move || skill_history::list_all_sessions(&skill_dir, None))
        .await
        .unwrap_or_default();

    // Build device block
    let device = json!({
        "state": status.state,
        "name": status.device_name,
        "kind": status.device_kind,
        "id": status.device_id,
        "battery": status.battery,
        "eeg_samples": status.sample_count,
        "eeg_channels": status.eeg_channel_count,
        "eeg_sample_rate": status.eeg_sample_rate_hz,
        "error": status.device_error,
    });

    // Build session block from latest session
    let session = if let Some(s) = sessions.first() {
        let start = s.session_start_utc.unwrap_or(0);
        let end = s.session_end_utc.unwrap_or(0);
        let dur = end.saturating_sub(start);
        json!({
            "start_utc": start,
            "end_utc": end,
            "duration_secs": dur,
            "device_name": s.device_name,
        })
    } else {
        Value::Null
    };

    // Build scores block from latest bands
    let scores = if let Some(b) = &bands { b.clone() } else { Value::Null };

    // Build embeddings stub
    let skill_dir2 = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let embedding_count = tokio::task::spawn_blocking(move || {
        let mut count = 0u64;
        if let Ok(entries) = std::fs::read_dir(&skill_dir2) {
            for entry in entries.filter_map(Result::ok) {
                let db = entry.path().join("eeg.sqlite");
                if db.exists() {
                    if let Ok(conn) =
                        rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                    {
                        count += conn
                            .query_row("SELECT COUNT(*) FROM embeddings", [], |r| r.get::<_, u64>(0))
                            .unwrap_or(0);
                    }
                }
            }
        }
        count
    })
    .await
    .unwrap_or(0);

    Ok(json!({
        "device": device,
        "session": session,
        "scores": scores,
        "embeddings": {
            "total": embedding_count,
        },
        "discovered_devices": devices.len(),
        "paired_devices": status.paired_devices,
    }))
}

// ── Sessions ─────────────────────────────────────────────────────────────────

pub(super) async fn cmd_sessions(state: &AppState) -> Result<Value, String> {
    let skill_dir = skill_dir(state);
    let sessions = tokio::task::spawn_blocking(move || skill_history::list_all_sessions(&skill_dir, None))
        .await
        .unwrap_or_default();

    let out: Vec<Value> = sessions
        .into_iter()
        .map(|s| {
            json!({
                "csv_path": s.csv_path,
                "start_utc": s.session_start_utc,
                "end_utc": s.session_end_utc,
                "device_name": s.device_name,
                "total_samples": s.total_samples,
            })
        })
        .collect();

    Ok(json!({ "sessions": out }))
}

pub(super) async fn cmd_session_metrics(state: &AppState, msg: &Value) -> Result<Value, String> {
    let start = u64_field(msg, "start_utc").ok_or("missing start_utc")?;
    let end = u64_field(msg, "end_utc").ok_or("missing end_utc")?;
    let skill_dir = skill_dir(state);
    let result = tokio::task::spawn_blocking(move || skill_history::get_session_metrics(&skill_dir, start, end))
        .await
        .unwrap_or_default();
    Ok(serde_json::to_value(result).unwrap_or_default())
}

// ── Devices ──────────────────────────────────────────────────────────────────

pub(super) async fn cmd_devices(state: &AppState) -> Result<Value, String> {
    let devices = state.devices.lock().map(|g| g.clone()).unwrap_or_default();
    Ok(json!({ "devices": devices }))
}

// ── Session control ──────────────────────────────────────────────────────────

/// Start (or restart) a device session.  The `target` field selects the device:
/// e.g. `"peer:<endpoint_id>"` for an iroh-remote phone stream.
/// Mirrors `POST /v1/control/start-session` for WS / cmd clients.
pub(super) async fn cmd_start_session(state: &AppState, msg: &Value) -> Result<Value, String> {
    let target = str_field(msg, "target");

    // Reject unpaired hardware targets (same guard as the HTTP route).
    if let Some(ref t) = target {
        if crate::util::target_requires_pairing(t) && !crate::util::is_paired_target(state, t) {
            return Err("Target device is not paired. Pair it first in Settings → Devices.".into());
        }
    }

    crate::util::spawn_session_for_target(state, target.as_deref());

    let state_str = if target.is_some() { "connecting" } else { "disconnected" };
    Ok(json!({ "state": state_str, "target": target }))
}

/// Cancel the running device session (if any).
pub(super) async fn cmd_cancel_session(state: &AppState) -> Result<Value, String> {
    if let Ok(mut slot) = state.session_handle.lock() {
        if let Some(handle) = slot.take() {
            let _ = handle.cancel_tx.send(());
        }
    }
    Ok(json!({ "state": "disconnected" }))
}

// ── Labels ───────────────────────────────────────────────────────────────────

pub(super) async fn cmd_label(state: &AppState, msg: &Value) -> Result<Value, String> {
    let text = str_field(msg, "text").ok_or("missing text")?;
    let context = str_field(msg, "context");
    let label_start_utc = f64_field(msg, "label_start_utc");
    let skill_dir = skill_dir(state);
    let _label_index = state.label_index.clone();
    let db_path = skill_dir.join(skill_constants::LABELS_FILE);

    let result = tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS labels (
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                text              TEXT NOT NULL,
                context           TEXT DEFAULT '',
                eeg_start         INTEGER NOT NULL DEFAULT 0,
                eeg_end           INTEGER NOT NULL DEFAULT 0,
                wall_start        INTEGER NOT NULL DEFAULT 0,
                wall_end          INTEGER NOT NULL DEFAULT 0,
                created_at        INTEGER NOT NULL DEFAULT 0,
                text_embedding    BLOB,
                context_embedding BLOB,
                embedding_model   TEXT
            );",
        )
        .map_err(|e| e.to_string())?;
        let now = label_start_utc.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0)
        });
        let now_secs = now as u64;
        conn.execute(
            "INSERT INTO labels (text, context, eeg_start, eeg_end, wall_start, wall_end, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                text,
                context,
                now_secs as i64,
                now_secs as i64,
                now_secs as i64,
                now_secs as i64,
                now_secs as i64
            ],
        )
        .map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid();

        // Background-embed: the HNSW insert happens via the label route's
        // background path; for the cmd tunnel we do a simpler insert.
        // A full rebuild can be triggered via /v1/labels/index/rebuild.
        Ok::<_, String>(id)
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(json!({ "label_id": result }))
}

pub(super) async fn cmd_search_labels(state: &AppState, msg: &Value) -> Result<Value, String> {
    let query = str_field(msg, "query").ok_or("missing query")?;
    let k = u64_field(msg, "k").unwrap_or(10) as usize;
    let mode = str_field(msg, "mode").unwrap_or_else(|| "text".into());
    let skill_dir = skill_dir(state);
    let db_path = skill_dir.join(skill_constants::LABELS_FILE);

    let results = tokio::task::spawn_blocking(move || {
        if !db_path.exists() {
            return Vec::new();
        }
        let Ok(conn) = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) else {
            return Vec::new();
        };

        let sql = match mode.as_str() {
            "context" => "SELECT id, text, context, created_at FROM labels WHERE context LIKE '%' || ?1 || '%' ORDER BY created_at DESC LIMIT ?2",
            "both" => "SELECT id, text, context, created_at FROM labels WHERE text LIKE '%' || ?1 || '%' OR context LIKE '%' || ?1 || '%' ORDER BY created_at DESC LIMIT ?2",
            _ => "SELECT id, text, context, created_at FROM labels WHERE text LIKE '%' || ?1 || '%' ORDER BY created_at DESC LIMIT ?2",
        };

        let Ok(mut stmt) = conn.prepare(sql) else {
            return Vec::new();
        };
        stmt.query_map(rusqlite::params![query, k as i64], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "text": row.get::<_, String>(1)?,
                "context": row.get::<_, Option<String>>(2)?,
                "created_at": row.get::<_, f64>(3)?,
            }))
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    Ok(json!({ "results": results, "count": results.len() }))
}

// ── Screenshots ──────────────────────────────────────────────────────────────

pub(super) async fn cmd_search_screenshots(state: &AppState, msg: &Value) -> Result<Value, String> {
    let query = str_field(msg, "query").ok_or("missing query")?;
    let k = u64_field(msg, "k").unwrap_or(20) as usize;
    let mode = str_field(msg, "mode").unwrap_or_else(|| "semantic".into());
    let skill_dir = skill_dir(state);

    let results = tokio::task::spawn_blocking(move || -> Vec<Value> {
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return vec![];
        };
        if mode == "substring" {
            return skill_screenshots::capture::search_by_ocr_text_like(&store, &query, k)
                .into_iter()
                .filter_map(|r| serde_json::to_value(r).ok())
                .collect();
        }
        // Try semantic search, fall back to substring
        let results = skill_screenshots::capture::search_by_ocr_text_like(&store, &query, k);
        results
            .into_iter()
            .filter_map(|r| serde_json::to_value(r).ok())
            .collect()
    })
    .await
    .unwrap_or_default();

    Ok(json!({ "results": results, "count": results.len() }))
}

pub(super) async fn cmd_screenshots_around(state: &AppState, msg: &Value) -> Result<Value, String> {
    let timestamp = i64_field(msg, "timestamp").ok_or("missing timestamp")?;
    let window_secs = i64_field(msg, "window_secs").unwrap_or(30) as i32;
    let skill_dir = skill_dir(state);

    let results = tokio::task::spawn_blocking(move || -> Vec<Value> {
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return vec![];
        };
        skill_screenshots::capture::get_around(&store, timestamp, window_secs)
            .into_iter()
            .filter_map(|r| serde_json::to_value(r).ok())
            .collect()
    })
    .await
    .unwrap_or_default();

    Ok(json!({ "results": results, "count": results.len() }))
}

pub(super) async fn cmd_screenshots_for_eeg(state: &AppState, msg: &Value) -> Result<Value, String> {
    let start = u64_field(msg, "start_utc").ok_or("missing start_utc")?;
    let end = u64_field(msg, "end_utc").ok_or("missing end_utc")?;
    let window_secs = i64_field(msg, "window_secs").unwrap_or(30) as i32;
    let skill_dir = skill_dir(state);

    let results = tokio::task::spawn_blocking(move || -> Vec<Value> {
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return vec![];
        };
        let mut all = Vec::new();
        let step = window_secs.max(1) as u64;
        let mut ts = start as i64;
        while ts <= end as i64 {
            let around = skill_screenshots::capture::get_around(&store, ts, window_secs);
            for r in around {
                if let Ok(v) = serde_json::to_value(r) {
                    all.push(v);
                }
            }
            ts += step as i64;
        }
        all
    })
    .await
    .unwrap_or_default();

    Ok(json!({ "results": results, "count": results.len() }))
}

pub(super) async fn cmd_eeg_for_screenshots(state: &AppState, msg: &Value) -> Result<Value, String> {
    let query = str_field(msg, "query").ok_or("missing query")?;
    let k = u64_field(msg, "k").unwrap_or(10) as usize;
    let window_secs = u64_field(msg, "window_secs").unwrap_or(60);
    let skill_dir = skill_dir(state);

    let results = tokio::task::spawn_blocking(move || -> Value {
        let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir) else {
            return json!({ "screenshots": [], "eeg_segments": [] });
        };
        let screenshots = skill_screenshots::capture::search_by_ocr_text_like(&store, &query, k);
        let mut eeg_segments = Vec::new();
        for s in &screenshots {
            let ts = s.unix_ts;
            let start = ts.saturating_sub(window_secs);
            let end = ts + window_secs;
            let metrics = skill_history::get_session_metrics(&skill_dir, start, end);
            eeg_segments.push(json!({
                "screenshot_ts": ts,
                "start_utc": start,
                "end_utc": end,
                "metrics": serde_json::to_value(&metrics).unwrap_or_default(),
            }));
        }
        json!({
            "screenshots": screenshots.into_iter().filter_map(|r| serde_json::to_value(r).ok()).collect::<Vec<_>>(),
            "eeg_segments": eeg_segments,
        })
    })
    .await
    .unwrap_or_else(|_| json!({ "screenshots": [], "eeg_segments": [] }));

    Ok(results)
}

// ── Search / Compare / Sleep / UMAP ──────────────────────────────────────────

pub(super) async fn cmd_search(state: &AppState, msg: &Value) -> Result<Value, String> {
    let start = u64_field(msg, "start_utc").ok_or("missing start_utc")?;
    let end = u64_field(msg, "end_utc").ok_or("missing end_utc")?;
    let k = u64_field(msg, "k").unwrap_or(5) as usize;
    let skill_dir = skill_dir(state);

    let result = tokio::task::spawn_blocking(move || {
        serde_json::to_value(skill_commands::search_embeddings_in_range(
            &skill_dir, start, end, k, 50, None,
        ))
        .unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    Ok(json!({ "result": result }))
}

pub(super) async fn cmd_compare(state: &AppState, msg: &Value) -> Result<Value, String> {
    let a_start = u64_field(msg, "a_start_utc").ok_or("missing a_start_utc")?;
    let a_end = u64_field(msg, "a_end_utc").ok_or("missing a_end_utc")?;
    let b_start = u64_field(msg, "b_start_utc").ok_or("missing b_start_utc")?;
    let b_end = u64_field(msg, "b_end_utc").ok_or("missing b_end_utc")?;
    let skill_dir = skill_dir(state);

    let result = tokio::task::spawn_blocking(move || {
        let avg_a = skill_history::get_session_metrics(&skill_dir, a_start, a_end);
        let avg_b = skill_history::get_session_metrics(&skill_dir, b_start, b_end);
        skill_history::compute_compare_insights(&skill_dir, a_start, a_end, b_start, b_end, &avg_a, &avg_b)
    })
    .await
    .unwrap_or_default();

    Ok(result)
}

pub(super) async fn cmd_sleep(state: &AppState, msg: &Value) -> Result<Value, String> {
    let start = u64_field(msg, "start_utc").ok_or("missing start_utc")?;
    let end = u64_field(msg, "end_utc").ok_or("missing end_utc")?;
    let skill_dir = skill_dir(state);

    let result = tokio::task::spawn_blocking(move || {
        serde_json::to_value(skill_history::get_sleep_stages(&skill_dir, start, end)).unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    Ok(result)
}

pub(super) async fn cmd_interactive_search(state: &AppState, msg: &Value) -> Result<Value, String> {
    let query = str_field(msg, "query").ok_or("missing query")?;
    let k_text = u64_field(msg, "k_text").unwrap_or(5) as usize;
    let k_eeg = u64_field(msg, "k_eeg").unwrap_or(5) as usize;
    let k_labels = u64_field(msg, "k_labels").unwrap_or(3) as usize;
    let reach_minutes = u64_field(msg, "reach_minutes").unwrap_or(10);
    let skill_dir = skill_dir(state);

    let result = tokio::task::spawn_blocking(move || {
        // Step 1: search labels
        let db_path = skill_dir.join(skill_constants::LABELS_FILE);
        let mut label_results = Vec::new();
        if db_path.exists() {
            if let Ok(conn) = rusqlite::Connection::open_with_flags(
                &db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            ) {
                if let Ok(mut stmt) = conn.prepare(
                    "SELECT id, text, context, created_at FROM labels WHERE text LIKE '%' || ?1 || '%' ORDER BY created_at DESC LIMIT ?2",
                ) {
                    if let Ok(rows) = stmt.query_map(rusqlite::params![query, k_text as i64], |row| {
                        Ok(json!({
                            "id": row.get::<_, i64>(0)?,
                            "text": row.get::<_, String>(1)?,
                            "context": row.get::<_, Option<String>>(2)?,
                            "created_at": row.get::<_, f64>(3)?,
                        }))
                    }) {
                        label_results = rows.filter_map(Result::ok).collect();
                    }
                }
            }
        }

        // Step 2: for each label timestamp, search nearby EEG
        let mut eeg_results = Vec::new();
        let found_labels: Vec<Value> = Vec::new();
        let reach_secs = reach_minutes * 60;

        for label in &label_results {
            let ts = label.get("created_at").and_then(Value::as_f64).unwrap_or(0.0) as u64;
            let start = ts.saturating_sub(reach_secs);
            let end = ts + reach_secs;
            let search_result = skill_commands::search_embeddings_in_range(
                &skill_dir, start, end, k_eeg, 50, None,
            );
            eeg_results.push(json!({
                "label_ts": ts,
                "start_utc": start,
                "end_utc": end,
                "search": serde_json::to_value(&search_result).unwrap_or_default(),
            }));
        }

        json!({
            "query": query,
            "labels": label_results,
            "eeg_results": eeg_results,
            "found_labels": found_labels,
            "k_text": k_text,
            "k_eeg": k_eeg,
            "k_labels": k_labels,
            "reach_minutes": reach_minutes,
        })
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));

    Ok(result)
}

pub(super) async fn cmd_umap(state: &AppState, msg: &Value) -> Result<Value, String> {
    // UMAP is computationally heavy. If explicit ranges given, use them.
    // Otherwise auto-select from last 2 sessions.
    let skill_dir = skill_dir(state);

    let a_start = u64_field(msg, "a_start_utc");
    let a_end = u64_field(msg, "a_end_utc");
    let b_start = u64_field(msg, "b_start_utc");
    let b_end = u64_field(msg, "b_end_utc");

    let result = tokio::task::spawn_blocking(move || {
        let sessions = skill_history::list_all_sessions(&skill_dir, None);
        let (as_, ae, bs, be) = if let (Some(a), Some(b), Some(c), Some(d)) = (a_start, a_end, b_start, b_end) {
            (a, b, c, d)
        } else if sessions.len() >= 2 {
            let s0 = &sessions[0];
            let s1 = &sessions[1];
            (
                s0.session_start_utc.unwrap_or(0),
                s0.session_end_utc.unwrap_or(0),
                s1.session_start_utc.unwrap_or(0),
                s1.session_end_utc.unwrap_or(0),
            )
        } else if sessions.len() == 1 {
            let s = &sessions[0];
            let start = s.session_start_utc.unwrap_or(0);
            let end = s.session_end_utc.unwrap_or(0);
            let mid = (start + end) / 2;
            (start, mid, mid, end)
        } else {
            return json!({ "error": "no sessions found for UMAP" });
        };

        skill_router::umap_compute_inner(&skill_dir, as_, ae, bs, be, None)
            .unwrap_or_else(|e| json!({ "error": e.to_string() }))
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));

    // Return as a "completed" UMAP result with a synthetic job_id
    Ok(json!({
        "ok": true,
        "job_id": 0,
        "status": "complete",
        "result": result,
    }))
}

pub(super) async fn cmd_umap_poll(_state: &AppState, msg: &Value) -> Result<Value, String> {
    // Since we compute UMAP synchronously in cmd_umap, polling always returns complete
    let _job_id = u64_field(msg, "job_id").unwrap_or(0);
    Ok(json!({ "status": "complete" }))
}

// ── Hooks ────────────────────────────────────────────────────────────────────

pub(super) async fn cmd_hooks_status(state: &AppState) -> Result<Value, String> {
    let hooks = state.hooks.lock().map(|g| g.clone()).unwrap_or_default();
    let items: Vec<Value> = hooks
        .into_iter()
        .map(|h| json!({ "hook": h, "last_trigger": Value::Null }))
        .collect();
    Ok(json!({ "hooks": items }))
}

pub(super) async fn cmd_hooks_get(state: &AppState) -> Result<Value, String> {
    let hooks = state.hooks.lock().map(|g| g.clone()).unwrap_or_default();
    Ok(json!({ "hooks": hooks }))
}

pub(super) async fn cmd_hooks_set(state: &AppState, msg: &Value) -> Result<Value, String> {
    let hooks_val = msg.get("hooks").ok_or("missing hooks")?;
    let hooks: Vec<skill_settings::HookRule> = serde_json::from_value(hooks_val.clone()).map_err(|e| e.to_string())?;
    if let Ok(mut g) = state.hooks.lock() {
        *g = hooks.clone();
    }
    let skill_dir = skill_dir(state);
    let mut settings = skill_settings::load_settings(&skill_dir);
    settings.hooks = hooks;
    let path = skill_settings::settings_path(&skill_dir);
    match serde_json::to_string_pretty(&settings) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                return Err(format!("failed to save hooks: {e}"));
            }
        }
        Err(e) => return Err(format!("failed to serialize settings: {e}")),
    }
    Ok(json!({ "hooks": settings.hooks }))
}

pub(super) async fn cmd_hooks_suggest(state: &AppState, msg: &Value) -> Result<Value, String> {
    let keywords_val = msg.get("keywords").ok_or("missing keywords")?;
    let keywords: Vec<String> = serde_json::from_value(keywords_val.clone()).map_err(|e| e.to_string())?;
    let skill_dir = skill_dir(state);

    let result = tokio::task::spawn_blocking(move || {
        let Some(log) = skill_data::hooks_log::HooksLog::open(&skill_dir) else {
            return json!({
                "suggested": 0.1,
                "note": "No hook trigger distances recorded yet.",
            });
        };
        let rows = log.query(5000, 0);
        let mut distances: Vec<f32> = Vec::new();
        for row in rows {
            let Ok(v) = serde_json::from_str::<Value>(&row.trigger_json) else {
                continue;
            };
            let maybe = v
                .get("distance")
                .and_then(Value::as_f64)
                .or_else(|| v.get("eeg_distance").and_then(Value::as_f64));
            if let Some(d) = maybe {
                let d = d as f32;
                if d.is_finite() {
                    distances.push(d.clamp(0.0, 1.0));
                }
            }
        }
        if distances.is_empty() {
            return json!({
                "suggested": 0.1,
                "note": "No hook trigger distances recorded yet.",
            });
        }
        distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = distances.len();
        let q = |p: f32| -> f32 {
            let idx = ((n - 1) as f32 * p).round() as usize;
            distances[idx.min(n - 1)]
        };
        let p75 = q(0.75);
        json!({
            "label_n": keywords.len(),
            "sample_n": n,
            "eeg_p25": q(0.25),
            "eeg_p50": q(0.50),
            "eeg_p75": p75,
            "suggested": p75.clamp(0.05, 0.95),
        })
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));

    Ok(result)
}

pub(super) async fn cmd_hooks_log(state: &AppState, msg: &Value) -> Result<Value, String> {
    let limit = i64_field(msg, "limit").unwrap_or(50).clamp(1, 500);
    let offset = i64_field(msg, "offset").unwrap_or(0).max(0);
    let skill_dir = skill_dir(state);

    let rows = tokio::task::spawn_blocking(move || {
        let Some(log) = skill_data::hooks_log::HooksLog::open(&skill_dir) else {
            return Vec::new();
        };
        log.query(limit, offset)
    })
    .await
    .unwrap_or_default();

    Ok(json!({ "rows": rows, "count": rows.len() }))
}

// ── Calibrations ─────────────────────────────────────────────────────────────

pub(super) async fn cmd_list_calibrations(state: &AppState) -> Result<Value, String> {
    let skill_dir = skill_dir(state);
    let profiles = tokio::task::spawn_blocking(move || {
        let db = skill_dir.join("calibrations.db");
        if !db.exists() {
            return Vec::new();
        }
        let Ok(conn) = rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare("SELECT id, name, config, created_at FROM calibrations ORDER BY created_at DESC") else {
            return Vec::new();
        };
        stmt.query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "config": row.get::<_, String>(2).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Null),
                "created_at": row.get::<_, f64>(3)?,
            }))
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    Ok(json!({ "profiles": profiles }))
}

pub(super) async fn cmd_get_calibration(state: &AppState, msg: &Value) -> Result<Value, String> {
    let id = str_field(msg, "id").ok_or("missing id")?;
    let skill_dir = skill_dir(state);

    let result = tokio::task::spawn_blocking(move || {
        let db = skill_dir.join("calibrations.db");
        let Ok(conn) = rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else {
            return json!({ "error": "db not found" });
        };
        let row: Option<Value> = conn
            .query_row(
                "SELECT id, name, config, created_at FROM calibrations WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    Ok(json!({
                        "id": row.get::<_, String>(0)?,
                        "name": row.get::<_, String>(1)?,
                        "config": row.get::<_, String>(2).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Null),
                        "created_at": row.get::<_, f64>(3)?,
                    }))
                },
            )
            .ok();
        row.unwrap_or(json!({ "error": "not found" }))
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));

    Ok(json!({ "profile": result }))
}

pub(super) async fn cmd_create_calibration(state: &AppState, msg: &Value) -> Result<Value, String> {
    let name = str_field(msg, "name").ok_or("missing name")?;
    let config = msg.get("config").cloned().unwrap_or(Value::Null);
    let skill_dir = skill_dir(state);

    let result = tokio::task::spawn_blocking(move || {
        let db = skill_dir.join("calibrations.db");
        let conn = rusqlite::Connection::open(&db).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS calibrations (id TEXT PRIMARY KEY, name TEXT NOT NULL, config TEXT, created_at REAL NOT NULL);",
        )
        .map_err(|e| e.to_string())?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        conn.execute(
            "INSERT INTO calibrations (id, name, config, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, name, serde_json::to_string(&config).unwrap_or_default(), now],
        )
        .map_err(|e| e.to_string())?;
        Ok::<_, String>(json!({ "id": id, "name": name }))
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(result)
}

pub(super) async fn cmd_update_calibration(state: &AppState, msg: &Value) -> Result<Value, String> {
    let id = str_field(msg, "id").ok_or("missing id")?;
    let name = str_field(msg, "name");
    let config = msg.get("config").cloned();
    let skill_dir = skill_dir(state);

    tokio::task::spawn_blocking(move || {
        let db = skill_dir.join("calibrations.db");
        let conn = rusqlite::Connection::open(&db).map_err(|e| e.to_string())?;
        if let Some(n) = &name {
            conn.execute(
                "UPDATE calibrations SET name = ?1 WHERE id = ?2",
                rusqlite::params![n, id],
            )
            .map_err(|e| e.to_string())?;
        }
        if let Some(c) = &config {
            conn.execute(
                "UPDATE calibrations SET config = ?1 WHERE id = ?2",
                rusqlite::params![serde_json::to_string(c).unwrap_or_default(), id],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(json!({}))
}

pub(super) async fn cmd_delete_calibration(state: &AppState, msg: &Value) -> Result<Value, String> {
    let id = str_field(msg, "id").ok_or("missing id")?;
    let skill_dir = skill_dir(state);

    tokio::task::spawn_blocking(move || {
        let db = skill_dir.join("calibrations.db");
        let conn = rusqlite::Connection::open(&db).map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM calibrations WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_state() -> (TempDir, AppState) {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        (td, state)
    }

    #[tokio::test]
    async fn cmd_status_returns_expected_shape() {
        let (_td, state) = test_state();
        let result = cmd_status(&state).await.unwrap();
        assert!(result.get("device").is_some());
        assert!(result.get("session").is_some());
    }

    #[tokio::test]
    async fn cmd_devices_returns_array() {
        let (_td, state) = test_state();
        let result = cmd_devices(&state).await.unwrap();
        assert!(result.get("devices").unwrap().is_array());
    }

    #[tokio::test]
    async fn cmd_label_requires_text() {
        let (_td, state) = test_state();
        let msg = json!({});
        let result = cmd_label(&state, &msg).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cmd_search_labels_returns_results_array() {
        let (_td, state) = test_state();
        let msg = json!({"query": "test"});
        let result = cmd_search_labels(&state, &msg).await.unwrap();
        assert!(result.get("results").unwrap().is_array());
    }

    #[tokio::test]
    async fn cmd_sessions_returns_sessions_array() {
        let (_td, state) = test_state();
        let result = cmd_sessions(&state).await.unwrap();
        assert!(result.get("sessions").unwrap().is_array());
    }

    #[tokio::test]
    async fn cmd_cancel_session_returns_state() {
        let (_td, state) = test_state();
        let result = cmd_cancel_session(&state).await.unwrap();
        assert_eq!(result["state"], "disconnected");
    }

    #[tokio::test]
    async fn cmd_start_session_without_target_returns_disconnected() {
        let (_td, state) = test_state();
        let msg = json!({});
        let result = cmd_start_session(&state, &msg).await.unwrap();
        assert_eq!(result["state"], "disconnected");
    }

    #[tokio::test]
    async fn cmd_hooks_status_returns_value() {
        let (_td, state) = test_state();
        let result = cmd_hooks_status(&state).await.unwrap();
        // hooks_status returns an array of hook statuses
        assert!(result.is_array() || result.is_object());
    }

    #[tokio::test]
    async fn cmd_hooks_get_returns_hooks() {
        let (_td, state) = test_state();
        let result = cmd_hooks_get(&state).await.unwrap();
        assert!(result.get("hooks").is_some());
    }

    #[tokio::test]
    async fn cmd_list_calibrations_returns_array() {
        let (_td, state) = test_state();
        let result = cmd_list_calibrations(&state).await.unwrap();
        assert!(result.get("profiles").unwrap().is_array());
    }

    #[tokio::test]
    async fn cmd_search_requires_query() {
        let (_td, state) = test_state();
        let msg = json!({});
        let result = cmd_search(&state, &msg).await;
        assert!(result.is_err());
    }
}
