// SPDX-License-Identifier: GPL-3.0-only
//! Universal JSON command dispatcher.
//!
//! Maps CLI command names (e.g. `{ "command": "status" }`) to daemon REST
//! handler logic.  Used by both the WS command handler and the `POST /v1/cmd`
//! HTTP tunnel.

use serde_json::{json, Value};

use crate::state::AppState;

/// Dispatch a JSON command envelope to the appropriate handler.
///
/// The envelope must have a `"command"` string field.  Additional fields are
/// forwarded as parameters.  Returns a JSON response that always contains
/// `"command"` and `"ok"` fields matching the CLI protocol.
pub async fn dispatch(state: AppState, msg: Value) -> Value {
    let cmd = msg.get("command").and_then(Value::as_str).unwrap_or("").to_string();

    if cmd.is_empty() {
        return json!({ "command": "", "ok": false, "error": "missing command field" });
    }

    let result = if let Some(result) = dispatch_family_commands(&state, &msg, &cmd).await {
        result
    } else {
        match cmd.as_str() {
            // ── Core status / sessions ──────────────────────────────────────
            "status" => cmd_status(&state).await,
            "sessions" => cmd_sessions(&state).await,
            "session_metrics" => cmd_session_metrics(&state, &msg).await,

            // ── Device / session control ─────────────────────────────────────
            "devices" => cmd_devices(&state).await,
            "start_session" => cmd_start_session(&state, &msg).await,
            "cancel_session" => cmd_cancel_session(&state).await,

            // ── Labels ──────────────────────────────────────────────────────
            "label" => cmd_label(&state, &msg).await,
            "search_labels" => cmd_search_labels(&state, &msg).await,

            // ── Screenshots ─────────────────────────────────────────────────
            "search_screenshots" => cmd_search_screenshots(&state, &msg).await,
            "screenshots_around" => cmd_screenshots_around(&state, &msg).await,
            "screenshots_for_eeg" => cmd_screenshots_for_eeg(&state, &msg).await,
            "eeg_for_screenshots" => cmd_eeg_for_screenshots(&state, &msg).await,

            // ── EEG search / compare / sleep / umap ─────────────────────────
            "search" => cmd_search(&state, &msg).await,
            "compare" => cmd_compare(&state, &msg).await,
            "sleep" => cmd_sleep(&state, &msg).await,
            "interactive_search" => cmd_interactive_search(&state, &msg).await,
            "umap" => cmd_umap(&state, &msg).await,
            "umap_poll" => cmd_umap_poll(&state, &msg).await,

            // ── Calibrations ────────────────────────────────────────────────
            "list_calibrations" => cmd_list_calibrations(&state).await,
            "get_calibration" => cmd_get_calibration(&state, &msg).await,
            "create_calibration" => cmd_create_calibration(&state, &msg).await,
            "update_calibration" => cmd_update_calibration(&state, &msg).await,
            "delete_calibration" => cmd_delete_calibration(&state, &msg).await,
            "run_calibration" => cmd_stub(&cmd, "calibration requires GUI").await,

            // ── TTS / Notify / Timer ────────────────────────────────────────
            "say" => cmd_say(&state, &msg).await,
            "notify" => cmd_notify(&msg).await,
            "timer" => cmd_stub(&cmd, "timer requires GUI").await,

            // ── Health / Oura / Calendar ────────────────────────────────────
            "health_query" => cmd_health_query(&state, &msg).await,
            "health_summary" => cmd_health_summary(&state, &msg).await,
            "health_metric_types" => cmd_health_metric_types(&state).await,
            "oura_status" => cmd_oura_status(&state).await,
            "oura_sync" => cmd_oura_sync(&state, &msg).await,
            "calendar_status" => cmd_calendar_status().await,
            "calendar_request_permission" => cmd_calendar_permission().await,
            "calendar_events" => cmd_calendar_events(&msg).await,

            // ── Subscribe (WS-only, acknowledge) ────────────────────────────
            "subscribe" => Ok(json!({ "subscribed": true })),

            _ => Err(format!("unknown command: {cmd}")),
        }
    };

    match result {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("command".into(), json!(cmd));
                if !obj.contains_key("ok") {
                    obj.insert("ok".into(), json!(true));
                }
            }
            v
        }
        Err(e) => json!({ "command": cmd, "ok": false, "error": e }),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn skill_dir(state: &AppState) -> std::path::PathBuf {
    state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default()
}

fn str_field(msg: &Value, key: &str) -> Option<String> {
    msg.get(key).and_then(Value::as_str).map(String::from)
}

fn u64_field(msg: &Value, key: &str) -> Option<u64> {
    msg.get(key).and_then(Value::as_u64)
}

fn f64_field(msg: &Value, key: &str) -> Option<f64> {
    msg.get(key).and_then(Value::as_f64)
}

fn i64_field(msg: &Value, key: &str) -> Option<i64> {
    msg.get(key).and_then(Value::as_i64)
}

fn bool_field(msg: &Value, key: &str) -> Option<bool> {
    msg.get(key).and_then(Value::as_bool)
}

async fn cmd_stub(cmd: &str, note: &str) -> Result<Value, String> {
    Ok(json!({ "command": cmd, "ok": false, "error": note }))
}

async fn dispatch_family_commands(state: &AppState, msg: &Value, cmd: &str) -> Option<Result<Value, String>> {
    match cmd {
        // Hooks
        "hooks_status" => Some(cmd_hooks_status(state).await),
        "hooks_get" => Some(cmd_hooks_get(state).await),
        "hooks_set" => Some(cmd_hooks_set(state, msg).await),
        "hooks_suggest" => Some(cmd_hooks_suggest(state, msg).await),
        "hooks_log" => Some(cmd_hooks_log(state, msg).await),

        // Sleep schedule
        "sleep_schedule" => Some(cmd_sleep_schedule(state).await),
        "sleep_schedule_set" => Some(cmd_sleep_schedule_set(state, msg).await),

        // DND
        "dnd" => Some(cmd_dnd(state).await),
        "dnd_set" => Some(cmd_dnd_set(msg).await),

        // Iroh
        "iroh_info" => Some(cmd_iroh_info(state).await),
        "iroh_totp_list" => Some(cmd_iroh(skill_iroh::commands::iroh_totp_list(&state.iroh_auth))),
        "iroh_totp_create" => Some(cmd_iroh(skill_iroh::commands::iroh_totp_create(&state.iroh_auth, msg))),
        "iroh_totp_qr" => Some(cmd_iroh(skill_iroh::commands::iroh_totp_qr(&state.iroh_auth, msg))),
        "iroh_totp_revoke" => Some(cmd_iroh(skill_iroh::commands::iroh_totp_revoke(&state.iroh_auth, msg))),
        "iroh_clients_list" => Some(cmd_iroh(skill_iroh::commands::iroh_clients_list(&state.iroh_auth))),
        "iroh_client_register" => Some(cmd_iroh(skill_iroh::commands::iroh_client_register(
            &state.iroh_auth,
            msg,
        ))),
        "iroh_client_revoke" => Some(cmd_iroh(skill_iroh::commands::iroh_client_revoke(
            &state.iroh_auth,
            msg,
        ))),
        "iroh_client_set_scope" => Some(cmd_iroh(skill_iroh::commands::iroh_client_set_scope(
            &state.iroh_auth,
            msg,
        ))),
        "iroh_scope_groups" => Some(cmd_iroh(skill_iroh::commands::iroh_scope_groups(&state.iroh_auth))),
        "iroh_client_permissions" => Some(cmd_iroh(skill_iroh::commands::iroh_client_permissions(
            &state.iroh_auth,
            msg,
        ))),
        "iroh_phone_invite" => Some(cmd_iroh(skill_iroh::commands::iroh_phone_invite(
            &state.iroh_auth,
            &state.iroh_runtime,
            msg,
        ))),

        // LLM
        "llm_status" => Some(cmd_llm_status(state).await),
        "llm_start" => Some(cmd_llm_start(state).await),
        "llm_stop" => Some(cmd_llm_stop(state).await),
        "llm_catalog" => Some(cmd_llm_catalog(state).await),
        "llm_add_model" => Some(cmd_llm_add_model(state, msg).await),
        "llm_select_model" => Some(cmd_llm_select_model(state, msg).await),
        "llm_select_mmproj" => Some(cmd_llm_select_mmproj(state, msg).await),
        "llm_set_autoload_mmproj" => Some(cmd_llm_set_autoload_mmproj(state, msg).await),
        "llm_download" => Some(cmd_llm_download(state, msg).await),
        "llm_pause_download" => Some(cmd_llm_pause_download(state, msg).await),
        "llm_resume_download" => Some(cmd_llm_resume_download(state, msg).await),
        "llm_cancel_download" => Some(cmd_llm_cancel_download(state, msg).await),
        "llm_delete" => Some(cmd_llm_delete(state, msg).await),
        "llm_downloads" => Some(cmd_llm_downloads(state).await),
        "llm_refresh_catalog" => Some(cmd_llm_refresh(state).await),
        "llm_hardware_fit" => Some(cmd_llm_hardware_fit(state).await),
        "llm_logs" => Some(cmd_llm_logs(state).await),
        "llm_chat" => Some(cmd_llm_chat(state, msg).await),

        _ => None,
    }
}

// ── Status ───────────────────────────────────────────────────────────────────

async fn cmd_status(state: &AppState) -> Result<Value, String> {
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
        "battery": status.battery,
        "eeg_samples": status.sample_count,
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

async fn cmd_sessions(state: &AppState) -> Result<Value, String> {
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

async fn cmd_session_metrics(state: &AppState, msg: &Value) -> Result<Value, String> {
    let start = u64_field(msg, "start_utc").ok_or("missing start_utc")?;
    let end = u64_field(msg, "end_utc").ok_or("missing end_utc")?;
    let skill_dir = skill_dir(state);
    let result = tokio::task::spawn_blocking(move || skill_history::get_session_metrics(&skill_dir, start, end))
        .await
        .unwrap_or_default();
    Ok(serde_json::to_value(result).unwrap_or_default())
}

// ── Devices ──────────────────────────────────────────────────────────────────

async fn cmd_devices(state: &AppState) -> Result<Value, String> {
    let devices = state.devices.lock().map(|g| g.clone()).unwrap_or_default();
    Ok(json!({ "devices": devices }))
}

// ── Session control ──────────────────────────────────────────────────────────

/// Start (or restart) a device session.  The `target` field selects the device:
/// e.g. `"peer:<endpoint_id>"` for an iroh-remote phone stream.
/// Mirrors `POST /v1/control/start-session` for WS / cmd clients.
async fn cmd_start_session(state: &AppState, msg: &Value) -> Result<Value, String> {
    let target = str_field(msg, "target");

    // Reject unpaired hardware targets (same guard as the HTTP route).
    if let Some(ref t) = target {
        if crate::target_requires_pairing(t) && !crate::is_paired_target(state, t) {
            return Err("Target device is not paired. Pair it first in Settings → Devices.".into());
        }
    }

    crate::spawn_session_for_target(state, target.as_deref());

    let state_str = if target.is_some() { "connecting" } else { "disconnected" };
    Ok(json!({ "state": state_str, "target": target }))
}

/// Cancel the running device session (if any).
async fn cmd_cancel_session(state: &AppState) -> Result<Value, String> {
    if let Ok(mut slot) = state.session_handle.lock() {
        if let Some(handle) = slot.take() {
            let _ = handle.cancel_tx.send(());
        }
    }
    Ok(json!({ "state": "disconnected" }))
}

// ── Labels ───────────────────────────────────────────────────────────────────

async fn cmd_label(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_search_labels(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_search_screenshots(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_screenshots_around(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_screenshots_for_eeg(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_eeg_for_screenshots(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_search(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_compare(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_sleep(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_interactive_search(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_umap(state: &AppState, msg: &Value) -> Result<Value, String> {
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
        "job_id": 0,
        "status": "done",
        "result": result,
    }))
}

async fn cmd_umap_poll(_state: &AppState, msg: &Value) -> Result<Value, String> {
    // Since we compute UMAP synchronously in cmd_umap, polling always returns done
    let _job_id = u64_field(msg, "job_id").unwrap_or(0);
    Ok(json!({ "status": "done" }))
}

// ── Hooks ────────────────────────────────────────────────────────────────────

async fn cmd_hooks_status(state: &AppState) -> Result<Value, String> {
    let hooks = state.hooks.lock().map(|g| g.clone()).unwrap_or_default();
    let items: Vec<Value> = hooks
        .into_iter()
        .map(|h| json!({ "hook": h, "last_trigger": Value::Null }))
        .collect();
    Ok(json!({ "hooks": items }))
}

async fn cmd_hooks_get(state: &AppState) -> Result<Value, String> {
    let hooks = state.hooks.lock().map(|g| g.clone()).unwrap_or_default();
    Ok(json!({ "hooks": hooks }))
}

async fn cmd_hooks_set(state: &AppState, msg: &Value) -> Result<Value, String> {
    let hooks_val = msg.get("hooks").ok_or("missing hooks")?;
    let hooks: Vec<skill_settings::HookRule> = serde_json::from_value(hooks_val.clone()).map_err(|e| e.to_string())?;
    if let Ok(mut g) = state.hooks.lock() {
        *g = hooks.clone();
    }
    let skill_dir = skill_dir(state);
    let mut settings = skill_settings::load_settings(&skill_dir);
    settings.hooks = hooks;
    let path = skill_settings::settings_path(&skill_dir);
    let _ = serde_json::to_string_pretty(&settings)
        .ok()
        .and_then(|json| std::fs::write(path, json).ok());
    Ok(json!({ "hooks": settings.hooks }))
}

async fn cmd_hooks_suggest(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_hooks_log(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_list_calibrations(state: &AppState) -> Result<Value, String> {
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

async fn cmd_get_calibration(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_create_calibration(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_update_calibration(state: &AppState, msg: &Value) -> Result<Value, String> {
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

async fn cmd_delete_calibration(state: &AppState, msg: &Value) -> Result<Value, String> {
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

// ── TTS / Notify ─────────────────────────────────────────────────────────────

async fn cmd_say(_state: &AppState, msg: &Value) -> Result<Value, String> {
    let text = str_field(msg, "text").ok_or("missing text")?;
    let voice = str_field(msg, "voice");

    let spoken = text.clone();
    skill_tts::tts_speak(text, voice).await;

    Ok(json!({ "spoken": spoken }))
}

async fn cmd_notify(msg: &Value) -> Result<Value, String> {
    let title = str_field(msg, "title").ok_or("missing title")?;
    let body = str_field(msg, "body").unwrap_or_default();

    tokio::task::spawn_blocking(move || {
        let _ = notify_rust::Notification::new().summary(&title).body(&body).show();
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!({}))
}

// ── Sleep schedule ───────────────────────────────────────────────────────────

async fn cmd_sleep_schedule(state: &AppState) -> Result<Value, String> {
    let skill_dir = skill_dir(state);
    let settings = skill_settings::load_settings(&skill_dir);
    Ok(json!({
        "bedtime": settings.sleep.bedtime,
        "wake_time": settings.sleep.wake_time,
        "preset": settings.sleep.preset,
    }))
}

async fn cmd_sleep_schedule_set(state: &AppState, msg: &Value) -> Result<Value, String> {
    let skill_dir = skill_dir(state);
    let mut settings = skill_settings::load_settings(&skill_dir);
    if let Some(bt) = str_field(msg, "bedtime") {
        settings.sleep.bedtime = bt;
    }
    if let Some(wt) = str_field(msg, "wake_time") {
        settings.sleep.wake_time = wt;
    }
    if let Some(p) = str_field(msg, "preset") {
        if let Ok(preset) = serde_json::from_value::<skill_settings::SleepPreset>(json!(p)) {
            settings.sleep.preset = preset;
        }
    }
    let path = skill_settings::settings_path(&skill_dir);
    let _ = serde_json::to_string_pretty(&settings)
        .ok()
        .and_then(|json| std::fs::write(path, json).ok());
    Ok(json!({
        "bedtime": settings.sleep.bedtime,
        "wake_time": settings.sleep.wake_time,
        "preset": settings.sleep.preset,
    }))
}

// ── Health / Oura / Calendar ─────────────────────────────────────────────────

async fn cmd_health_query(state: &AppState, msg: &Value) -> Result<Value, String> {
    let query_type = str_field(msg, "type").unwrap_or_else(|| "summary".into());
    let start_utc = u64_field(msg, "start_utc");
    let end_utc = u64_field(msg, "end_utc");
    let metric_type = str_field(msg, "metric_type");
    let limit = u64_field(msg, "limit");
    let skill_dir = skill_dir(state);

    let result = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_health::HealthStore::open(&skill_dir) else {
            return json!({ "error": "health store not available" });
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let end = end_utc.unwrap_or(now) as i64;
        let start = start_utc.map(|v| v as i64).unwrap_or_else(|| end - 86400);
        let lim = limit.unwrap_or(500) as i64;

        match query_type.as_str() {
            "sleep" => {
                let rows = store.query_sleep(start, end, lim);
                json!({ "type": "sleep", "data": rows })
            }
            "workouts" => {
                let rows = store.query_workouts(start, end, lim);
                json!({ "type": "workouts", "data": rows })
            }
            "hr" => {
                let rows = store.query_heart_rate(start, end, lim);
                json!({ "type": "hr", "data": rows })
            }
            "steps" => {
                let rows = store.query_steps(start, end, lim);
                json!({ "type": "steps", "data": rows })
            }
            "metrics" => {
                let mt = metric_type.unwrap_or_default();
                let rows = store.query_metrics(&mt, start, end, lim);
                json!({ "type": "metrics", "metric_type": mt, "data": rows })
            }
            "location" => {
                // Location query not directly available from health store.
                json!({ "type": "location", "data": [] })
            }
            _ => {
                let summary = store.summary(start, end);
                json!({ "type": "summary", "data": summary })
            }
        }
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));

    Ok(result)
}

async fn cmd_health_summary(state: &AppState, msg: &Value) -> Result<Value, String> {
    let start_utc = u64_field(msg, "start_utc");
    let end_utc = u64_field(msg, "end_utc");
    let skill_dir = skill_dir(state);

    let result = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_health::HealthStore::open(&skill_dir) else {
            return json!({ "error": "health store not available" });
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let end = end_utc.unwrap_or(now) as i64;
        let start = start_utc.map(|v| v as i64).unwrap_or_else(|| end - 86400);
        store.summary(start, end)
    })
    .await
    .unwrap_or_default();

    Ok(serde_json::to_value(result).unwrap_or_default())
}

async fn cmd_health_metric_types(state: &AppState) -> Result<Value, String> {
    let skill_dir = skill_dir(state);
    let types = tokio::task::spawn_blocking(move || {
        skill_health::HealthStore::open(&skill_dir)
            .map(|store| store.list_metric_types())
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    Ok(json!({ "types": types }))
}

async fn cmd_oura_status(state: &AppState) -> Result<Value, String> {
    let skill_dir = skill_dir(state);
    let settings = skill_settings::load_settings(&skill_dir);
    let has_token = !settings.device_api.oura_access_token.is_empty();
    Ok(json!({
        "connected": has_token,
        "has_token": has_token,
    }))
}

async fn cmd_oura_sync(state: &AppState, msg: &Value) -> Result<Value, String> {
    let start_date = str_field(msg, "start_date");
    let end_date = str_field(msg, "end_date");
    let skill_dir = skill_dir(state);
    let settings = skill_settings::load_settings(&skill_dir);
    let token = settings.device_api.oura_access_token.clone();

    if token.is_empty() {
        return Err("Oura access token not configured".into());
    }

    let result = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_health::HealthStore::open(&skill_dir) else {
            return json!({ "error": "health store not available" });
        };
        let oura = skill_oura::OuraSync::new(&token);
        let now = chrono::Utc::now();
        let end = end_date
            .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
            .unwrap_or(now.date_naive());
        let start = start_date
            .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
            .unwrap_or(end - chrono::Duration::days(30));

        match oura.fetch(&start.to_string(), &end.to_string()) {
            Ok(payload) => {
                let result = store.sync(&payload);
                json!({
                    "synced": true,
                    "start": start.to_string(),
                    "end": end.to_string(),
                    "sleep_upserted": result.sleep_upserted,
                    "workouts_upserted": result.workouts_upserted,
                    "heart_rate_upserted": result.heart_rate_upserted,
                    "steps_upserted": result.steps_upserted,
                })
            }
            Err(e) => json!({ "error": e.to_string() }),
        }
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));

    Ok(result)
}

async fn cmd_calendar_status() -> Result<Value, String> {
    let result = tokio::task::spawn_blocking(|| {
        let status = skill_calendar::auth_status();
        json!({
            "platform": std::env::consts::OS,
            "permission": format!("{:?}", status),
        })
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));
    Ok(result)
}

async fn cmd_calendar_permission() -> Result<Value, String> {
    let result = tokio::task::spawn_blocking(|| {
        let granted = skill_calendar::request_access();
        let status = skill_calendar::auth_status();
        json!({
            "permission": format!("{:?}", status),
            "granted": granted,
        })
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));
    Ok(result)
}

async fn cmd_calendar_events(msg: &Value) -> Result<Value, String> {
    let start_utc = u64_field(msg, "start_utc");
    let end_utc = u64_field(msg, "end_utc");

    let result = tokio::task::spawn_blocking(move || {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let start = start_utc.unwrap_or(now) as i64;
        let end = end_utc.unwrap_or(now + 7 * 86400) as i64;
        match skill_calendar::fetch_events(start, end) {
            Ok(events) => json!({ "events": events }),
            Err(e) => json!({ "events": [], "error": e.to_string() }),
        }
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));

    Ok(result)
}

// ── DND ──────────────────────────────────────────────────────────────────────

async fn cmd_dnd(state: &AppState) -> Result<Value, String> {
    let skill_dir = skill_dir(state);
    let settings = skill_settings::load_settings(&skill_dir);
    let cfg = settings.do_not_disturb;
    let os_active = skill_data::dnd::query_os_active();
    Ok(json!({
        "enabled": cfg.enabled,
        "threshold": cfg.focus_threshold,
        "duration_secs": cfg.duration_secs,
        "dnd_active": os_active.unwrap_or(false),
        "os_active": os_active,
    }))
}

async fn cmd_dnd_set(msg: &Value) -> Result<Value, String> {
    let enabled = bool_field(msg, "enabled").ok_or("missing enabled")?;
    let ok = if enabled {
        // Can't programmatically enable DND easily - platform specific
        false
    } else {
        skill_data::dnd::set_dnd(false, "")
    };
    Ok(json!({ "enabled": enabled, "applied": ok }))
}

// ── Iroh ─────────────────────────────────────────────────────────────────────

fn cmd_iroh(result: anyhow::Result<Value>) -> Result<Value, String> {
    result.map_err(|e| e.to_string())
}

async fn cmd_iroh_info(state: &AppState) -> Result<Value, String> {
    skill_iroh::commands::iroh_info(&state.iroh_auth, &state.iroh_runtime).map_err(|e| e.to_string())
}

// ── LLM ──────────────────────────────────────────────────────────────────────

#[cfg(feature = "llm")]
#[derive(Clone)]
struct CmdLlmEmitter {
    events_tx: tokio::sync::broadcast::Sender<skill_daemon_common::EventEnvelope>,
}

#[cfg(feature = "llm")]
impl skill_llm::LlmEventEmitter for CmdLlmEmitter {
    fn emit_event(&self, event: &str, payload: serde_json::Value) {
        let _ = self.events_tx.send(skill_daemon_common::EventEnvelope {
            r#type: format!("Llm{}", event.replace(':', "_")),
            ts_unix_ms: now_unix() * 1000,
            correlation_id: None,
            payload,
        });
    }
}

async fn cmd_llm_status(state: &AppState) -> Result<Value, String> {
    #[cfg(feature = "llm")]
    {
        use std::sync::atomic::Ordering;
        let (status, model_name) = skill_llm::cell_status(&state.llm_state_cell);
        let (n_ctx, supports_vision, supports_tools) = state
            .llm_state_cell
            .lock()
            .ok()
            .and_then(|g| {
                g.as_ref().map(|srv| {
                    (
                        srv.n_ctx.load(Ordering::Relaxed),
                        srv.vision_ready.load(Ordering::Relaxed),
                        srv.is_ready(),
                    )
                })
            })
            .unwrap_or((0, false, false));

        return Ok(json!({
            "status": serde_json::to_value(status).unwrap_or(json!("stopped")),
            "model_name": model_name,
            "n_ctx": n_ctx,
            "supports_vision": supports_vision,
            "supports_tools": supports_tools,
        }));
    }

    #[cfg(not(feature = "llm"))]
    {
        let status = state
            .llm_status
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "stopped".into());
        let model_name = state.llm_model_name.lock().map(|g| g.clone()).unwrap_or_default();
        Ok(json!({
            "status": status,
            "model_name": model_name,
        }))
    }
}

async fn cmd_llm_start(state: &AppState) -> Result<Value, String> {
    // Delegate to the REST handler logic
    #[cfg(feature = "llm")]
    {
        let cfg = state.llm_config.lock().map(|g| g.clone()).unwrap_or_default();
        let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
        let skill_dir = skill_dir(state);
        let cell = state.llm_state_cell.clone();
        let log_buf = state.llm_log_buffer.clone();

        if cell.lock().ok().and_then(|g| g.clone()).is_some() {
            return Ok(json!({"result": "already_running"}));
        }

        let emitter: std::sync::Arc<dyn skill_llm::LlmEventEmitter> = std::sync::Arc::new(CmdLlmEmitter {
            events_tx: state.events_tx.clone(),
        });
        match tokio::task::spawn_blocking(move || skill_llm::init(&cfg, &cat, emitter, log_buf, &skill_dir)).await {
            Ok(Some(srv)) => {
                let model_name = srv.model_name.clone();
                if let Ok(mut g) = cell.lock() {
                    *g = Some(srv);
                }
                if let Ok(mut st) = state.llm_status.lock() {
                    *st = "running".into();
                }
                if let Ok(mut m) = state.llm_model_name.lock() {
                    *m = model_name;
                }
                return Ok(json!({"result": "starting"}));
            }
            Ok(None) => return Err("LLM init returned none".into()),
            Err(e) => return Err(e.to_string()),
        }
    }

    #[cfg(not(feature = "llm"))]
    {
        if let Ok(mut st) = state.llm_status.lock() {
            *st = "running".into();
        }
        Ok(json!({"result": "starting"}))
    }
}

async fn cmd_llm_stop(state: &AppState) -> Result<Value, String> {
    #[cfg(feature = "llm")]
    {
        skill_llm::shutdown_cell(&state.llm_state_cell);
    }
    if let Ok(mut st) = state.llm_status.lock() {
        *st = "stopped".into();
    }
    Ok(json!({}))
}

async fn cmd_llm_catalog(state: &AppState) -> Result<Value, String> {
    let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
    Ok(serde_json::to_value(cat).unwrap_or_default())
}

async fn cmd_llm_add_model(state: &AppState, msg: &Value) -> Result<Value, String> {
    let repo = str_field(msg, "repo").ok_or("missing repo")?;
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    let size_gb = f64_field(msg, "size_gb").map(|v| v as f32);
    let mmproj = str_field(msg, "mmproj");
    let download = bool_field(msg, "download").unwrap_or(false);

    if let Ok(mut cat) = state.llm_catalog.lock() {
        if !cat.entries.iter().any(|e| e.filename == filename) {
            let is_mmproj = mmproj.as_ref().map(|m| m == &filename).unwrap_or(false)
                || filename.to_ascii_lowercase().contains("mmproj");
            cat.entries.push(skill_llm::catalog::LlmModelEntry {
                repo: repo.clone(),
                filename: filename.clone(),
                quant: infer_quant(&filename),
                size_gb: size_gb.unwrap_or(0.0),
                description: "External model".to_string(),
                family_id: repo
                    .split('/')
                    .next_back()
                    .unwrap_or("external")
                    .to_lowercase()
                    .replace(' ', "-"),
                family_name: repo
                    .split('/')
                    .next_back()
                    .unwrap_or("External")
                    .replace(['_', '-'], " "),
                family_desc: String::new(),
                tags: vec!["external".to_string()],
                is_mmproj,
                recommended: false,
                advanced: false,
                params_b: 0.0,
                max_context_length: 0,
                shard_files: Vec::new(),
                local_path: None,
                state: if download {
                    skill_llm::catalog::DownloadState::Downloading
                } else {
                    skill_llm::catalog::DownloadState::NotDownloaded
                },
                status_msg: if download { Some("Queued".into()) } else { None },
                progress: 0.0,
                initiated_at_unix: Some(now_unix()),
            });
        }
        cat.auto_select();
    }
    persist_llm_catalog(state);
    Ok(json!({ "filename": filename }))
}

async fn cmd_llm_select_model(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.active_model = filename.clone();
        if !cat.active_mmproj_matches_active_model() {
            cat.active_mmproj.clear();
        }
    }
    persist_llm_catalog(state);
    Ok(json!({ "active_model": filename }))
}

async fn cmd_llm_select_mmproj(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.active_mmproj = filename.clone();
    }
    persist_llm_catalog(state);
    Ok(json!({ "active_mmproj": filename }))
}

async fn cmd_llm_set_autoload_mmproj(state: &AppState, msg: &Value) -> Result<Value, String> {
    let enabled = bool_field(msg, "enabled").ok_or("missing enabled")?;
    let skill_dir = skill_dir(state);
    let mut settings = skill_settings::load_settings(&skill_dir);
    settings.llm.autoload_mmproj = enabled;
    let path = skill_settings::settings_path(&skill_dir);
    let _ = serde_json::to_string_pretty(&settings)
        .ok()
        .and_then(|json| std::fs::write(path, json).ok());
    Ok(json!({ "value": enabled }))
}

async fn cmd_llm_download(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    set_download_state_cmd(
        state,
        &filename,
        skill_llm::catalog::DownloadState::Downloading,
        Some("Queued".into()),
    );
    spawn_model_download_cmd(state.clone(), filename.clone());
    Ok(json!({}))
}

async fn cmd_llm_pause_download(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    set_live_cancel_flags(state, &filename, true, true);
    set_download_state_cmd(
        state,
        &filename,
        skill_llm::catalog::DownloadState::Paused,
        Some("Pausing".into()),
    );
    Ok(json!({}))
}

async fn cmd_llm_resume_download(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    let is_active = state
        .llm_downloads
        .lock()
        .ok()
        .map(|m| m.contains_key(&filename))
        .unwrap_or(false);
    if !is_active {
        set_download_state_cmd(
            state,
            &filename,
            skill_llm::catalog::DownloadState::Downloading,
            Some("Resumed".into()),
        );
        spawn_model_download_cmd(state.clone(), filename);
    }
    Ok(json!({}))
}

async fn cmd_llm_cancel_download(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    set_live_cancel_flags(state, &filename, true, false);
    set_download_state_cmd(
        state,
        &filename,
        skill_llm::catalog::DownloadState::Cancelled,
        Some("Cancelling".into()),
    );
    Ok(json!({}))
}

async fn cmd_llm_delete(state: &AppState, msg: &Value) -> Result<Value, String> {
    let filename = str_field(msg, "filename").ok_or("missing filename")?;
    set_live_cancel_flags(state, &filename, true, false);
    if let Ok(mut cat) = state.llm_catalog.lock() {
        if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == filename) {
            e.state = skill_llm::catalog::DownloadState::NotDownloaded;
            e.status_msg = None;
            e.progress = 0.0;
            e.local_path = None;
        }
    }
    if let Ok(mut m) = state.llm_downloads.lock() {
        m.remove(&filename);
    }
    persist_llm_catalog(state);
    Ok(json!({}))
}

async fn cmd_llm_downloads(state: &AppState) -> Result<Value, String> {
    let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
    let downloads = state.llm_downloads.lock().map(|g| g.clone()).unwrap_or_default();
    let items: Vec<Value> = cat
        .entries
        .into_iter()
        .filter(|e| {
            use skill_llm::catalog::DownloadState;
            matches!(
                e.state,
                DownloadState::Downloading
                    | DownloadState::Paused
                    | DownloadState::Failed
                    | DownloadState::Cancelled
                    | DownloadState::Downloaded
            )
        })
        .map(|e| {
            let live = downloads
                .get(&e.filename)
                .and_then(|p| p.lock().ok().map(|g| g.clone()));
            json!({
                "repo": e.repo,
                "filename": e.filename,
                "quant": e.quant,
                "size_gb": e.size_gb,
                "state": live.as_ref().map(|p| p.state.clone()).unwrap_or(e.state),
                "status_msg": live.as_ref().and_then(|p| p.status_msg.clone()).or(e.status_msg),
                "progress": live.as_ref().map(|p| p.progress).unwrap_or(e.progress),
            })
        })
        .collect();
    Ok(json!({ "downloads": items }))
}

async fn cmd_llm_refresh(state: &AppState) -> Result<Value, String> {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        cat.refresh_cache();
        cat.auto_select();
    }
    persist_llm_catalog(state);
    Ok(json!({}))
}

async fn cmd_llm_hardware_fit(state: &AppState) -> Result<Value, String> {
    let cat = state.llm_catalog.lock().map(|g| g.clone()).unwrap_or_default();
    let gpu = skill_data::gpu_stats::read();
    let sys_ram_gb = sysinfo::System::new_all().total_memory() as f64 / 1_073_741_824.0;
    let vram_gb = gpu
        .as_ref()
        .and_then(|g| g.total_memory_bytes.map(|b| b as f64 / 1_073_741_824.0))
        .unwrap_or(0.0);

    let fits: Vec<Value> = cat
        .entries
        .iter()
        .filter(|e| e.size_gb > 0.0)
        .map(|e| {
            let size = e.size_gb as f64;
            let (fit_level, run_mode) = if vram_gb > 0.0 && size * 1.2 <= vram_gb {
                ("perfect", "gpu")
            } else if size * 1.2 <= sys_ram_gb {
                ("good", "cpu")
            } else if size <= sys_ram_gb {
                ("marginal", "cpu")
            } else {
                ("too_large", "none")
            };
            json!({
                "filename": e.filename,
                "size_gb": e.size_gb,
                "fit_level": fit_level,
                "run_mode": run_mode,
                "memory_required_gb": size * 1.2,
            })
        })
        .collect();

    Ok(json!({ "fits": fits }))
}

async fn cmd_llm_logs(state: &AppState) -> Result<Value, String> {
    #[cfg(feature = "llm")]
    {
        let logs: Vec<Value> = state
            .llm_log_buffer
            .lock()
            .map(|q| q.iter().filter_map(|e| serde_json::to_value(e).ok()).collect())
            .unwrap_or_default();
        return Ok(json!({ "logs": logs }));
    }

    #[cfg(not(feature = "llm"))]
    {
        let logs = state.llm_logs.lock().map(|g| g.clone()).unwrap_or_default();
        Ok(json!({ "logs": logs }))
    }
}

/// Non-streaming LLM chat (for HTTP mode).  For WS streaming see `dispatch_llm_chat_streaming`.
#[allow(unused_variables)]
async fn cmd_llm_chat(state: &AppState, msg: &Value) -> Result<Value, String> {
    let messages = msg.get("messages").cloned().unwrap_or(Value::Array(Vec::new()));
    let params = msg.get("params").cloned().unwrap_or(Value::Object(Default::default()));

    #[cfg(feature = "llm")]
    {
        let srv_opt = state.llm_state_cell.lock().ok().and_then(|g| g.clone());
        let Some(srv) = srv_opt else {
            return Err("LLM server not running".into());
        };
        let messages_vec: Vec<Value> = serde_json::from_value(messages).unwrap_or_default();
        let gen_params: skill_llm::GenParams = serde_json::from_value(params).unwrap_or_default();

        let result =
            skill_llm::run_chat_with_builtin_tools(&srv, messages_vec, gen_params, Vec::new(), |_delta| {}, |_evt| {})
                .await;

        return match result {
            Ok((text, finish_reason, prompt_tokens, completion_tokens, n_ctx)) => Ok(json!({
                "content": text,
                "finish_reason": finish_reason,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "n_ctx": n_ctx,
            })),
            Err(e) => Err(e.to_string()),
        };
    }

    #[cfg(not(feature = "llm"))]
    {
        let _ = (messages, params);
        Err("LLM unavailable (compiled without llm feature)".into())
    }
}

/// Streaming LLM chat over WebSocket.  Sends incremental `delta` messages
/// followed by a `done` message.  Returns `None` to indicate the caller
/// should NOT send a normal response (we already sent the streaming messages).
#[cfg(feature = "llm")]
pub async fn dispatch_llm_chat_streaming(
    state: AppState,
    msg: Value,
    ws_tx: &mut tokio::sync::mpsc::Sender<String>,
) -> bool {
    let messages = msg.get("messages").cloned().unwrap_or(Value::Array(Vec::new()));
    let params = msg.get("params").cloned().unwrap_or(Value::Object(Default::default()));

    let srv_opt = state.llm_state_cell.lock().ok().and_then(|g| g.clone());
    let Some(srv) = srv_opt else {
        let err = json!({ "command": "llm_chat", "ok": false, "type": "error", "error": "LLM server not running" });
        let _ = ws_tx.send(serde_json::to_string(&err).unwrap_or_default()).await;
        return true;
    };

    let messages_vec: Vec<Value> = serde_json::from_value(messages).unwrap_or_default();
    let gen_params: skill_llm::GenParams = serde_json::from_value(params).unwrap_or_default();

    // Send session message.
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let session_id = skill_llm::chat_store::ChatStore::open(&skill_dir)
        .map(|mut store| store.get_or_create_last_session())
        .unwrap_or(0);
    let session_msg = json!({ "command": "llm_chat", "type": "session", "session_id": session_id });
    let _ = ws_tx
        .send(serde_json::to_string(&session_msg).unwrap_or_default())
        .await;

    // Set up streaming callback.
    let tx_for_delta = ws_tx.clone();
    let delta_callback = move |delta: &str| {
        let msg = json!({ "command": "llm_chat", "type": "delta", "text": delta });
        // Use blocking send since the callback is sync.
        let _ = tx_for_delta.blocking_send(serde_json::to_string(&msg).unwrap_or_default());
    };

    let result =
        skill_llm::run_chat_with_builtin_tools(&srv, messages_vec, gen_params, Vec::new(), delta_callback, |_evt| {})
            .await;

    match result {
        Ok((text, finish_reason, prompt_tokens, completion_tokens, n_ctx)) => {
            let done = json!({
                "command": "llm_chat",
                "ok": true,
                "type": "done",
                "content": text,
                "finish_reason": finish_reason,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "n_ctx": n_ctx,
                "session_id": session_id,
            });
            let _ = ws_tx.send(serde_json::to_string(&done).unwrap_or_default()).await;
        }
        Err(e) => {
            let err = json!({
                "command": "llm_chat",
                "ok": false,
                "type": "error",
                "error": e.to_string(),
            });
            let _ = ws_tx.send(serde_json::to_string(&err).unwrap_or_default()).await;
        }
    }
    true
}

// ── LLM helpers ──────────────────────────────────────────────────────────────

fn infer_quant(filename: &str) -> String {
    let upper = filename.to_uppercase();
    for q in [
        "IQ4_NL", "IQ4_XS", "IQ3_XXS", "IQ3_XS", "IQ3_M", "IQ3_S", "Q6_K", "Q5_K_M", "Q5_K_S", "Q4_K_M", "Q4_K_S",
        "Q4_0", "Q3_K_M", "Q3_K_S", "Q2_K", "Q8_0", "BF16", "F16", "F32",
    ] {
        if upper.contains(q) {
            return q.to_string();
        }
    }
    "unknown".to_string()
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn persist_llm_catalog(state: &AppState) {
    let skill_dir = skill_dir(state);
    if let Ok(cat) = state.llm_catalog.lock() {
        cat.save(&skill_dir);
    }
}

fn set_download_state_cmd(
    state: &AppState,
    filename: &str,
    new_state: skill_llm::catalog::DownloadState,
    msg: Option<String>,
) {
    if let Ok(mut cat) = state.llm_catalog.lock() {
        if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == filename) {
            e.state = new_state;
            e.status_msg = msg;
            e.initiated_at_unix = Some(now_unix());
        }
    }
    persist_llm_catalog(state);
}

fn set_live_cancel_flags(state: &AppState, filename: &str, cancelled: bool, pause_requested: bool) {
    let progress_opt = state.llm_downloads.lock().ok().and_then(|m| m.get(filename).cloned());
    if let Some(progress) = progress_opt {
        if let Ok(mut p) = progress.lock() {
            p.cancelled = cancelled;
            p.pause_requested = pause_requested;
        }
    }
}

fn spawn_model_download_cmd(state: AppState, filename: String) {
    tokio::spawn(async move {
        let entry_opt = state
            .llm_catalog
            .lock()
            .ok()
            .and_then(|cat| cat.entries.iter().find(|e| e.filename == filename).cloned());
        let Some(entry) = entry_opt else { return };

        let progress = std::sync::Arc::new(std::sync::Mutex::new(skill_llm::catalog::DownloadProgress {
            filename: entry.filename.clone(),
            state: skill_llm::catalog::DownloadState::Downloading,
            ..Default::default()
        }));

        if let Ok(mut m) = state.llm_downloads.lock() {
            m.insert(filename.clone(), progress.clone());
        }

        let progress_for_job = progress.clone();
        let entry_for_job = entry.clone();
        let job =
            tokio::task::spawn_blocking(move || skill_llm::catalog::download_model(&entry_for_job, &progress_for_job));

        let res = job.await;

        if let Ok(mut m) = state.llm_downloads.lock() {
            m.remove(&filename);
        }

        match res {
            Ok(Ok(path)) => {
                if let Ok(mut cat) = state.llm_catalog.lock() {
                    if let Some(e) = cat.entries.iter_mut().find(|e| e.filename == filename) {
                        e.state = skill_llm::catalog::DownloadState::Downloaded;
                        e.progress = 1.0;
                        e.local_path = Some(path);
                        e.status_msg = Some("Downloaded".into());
                    }
                }
                persist_llm_catalog(&state);
            }
            Ok(Err(err)) => {
                let msg = err.to_string();
                let st = if msg.contains("paused") {
                    skill_llm::catalog::DownloadState::Paused
                } else if msg.contains("cancelled") {
                    skill_llm::catalog::DownloadState::Cancelled
                } else {
                    skill_llm::catalog::DownloadState::Failed
                };
                set_download_state_cmd(&state, &filename, st, Some(msg));
            }
            Err(err) => {
                set_download_state_cmd(
                    &state,
                    &filename,
                    skill_llm::catalog::DownloadState::Failed,
                    Some(err.to_string()),
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn helpers_extract_typed_fields() {
        let msg = json!({"s":"x","u":7,"f":1.25,"i":-2,"b":true});
        assert_eq!(str_field(&msg, "s").as_deref(), Some("x"));
        assert_eq!(u64_field(&msg, "u"), Some(7));
        assert_eq!(f64_field(&msg, "f"), Some(1.25));
        assert_eq!(i64_field(&msg, "i"), Some(-2));
        assert_eq!(bool_field(&msg, "b"), Some(true));
    }

    #[tokio::test]
    async fn dispatch_missing_command_fails() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let v = dispatch(state, json!({"foo":"bar"})).await;
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"], "missing command field");
    }

    #[tokio::test]
    async fn dispatch_unknown_command_fails() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let v = dispatch(state, json!({"command":"nope"})).await;
        assert_eq!(v["command"], "nope");
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap_or("").contains("unknown command"));
    }

    #[tokio::test]
    async fn dispatch_status_returns_command_and_ok() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let v = dispatch(state, json!({"command":"status"})).await;
        assert_eq!(v["command"], "status");
        assert_eq!(v["ok"], true);
        assert!(v.get("device").is_some());
    }

    #[tokio::test]
    async fn dispatch_devices_and_sessions_have_expected_shape() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let v1 = dispatch(state.clone(), json!({"command":"devices"})).await;
        assert_eq!(v1["ok"], true);
        assert!(v1["devices"].is_array());

        let v2 = dispatch(state, json!({"command":"sessions"})).await;
        assert_eq!(v2["ok"], true);
        assert!(v2["sessions"].is_array());
    }

    #[tokio::test]
    async fn dispatch_validation_errors_for_missing_required_fields() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let m1 = dispatch(state.clone(), json!({"command":"session_metrics"})).await;
        assert_eq!(m1["ok"], false);
        assert!(m1["error"].as_str().unwrap_or("").contains("start_utc"));

        let m2 = dispatch(state.clone(), json!({"command":"label"})).await;
        assert_eq!(m2["ok"], false);
        assert!(m2["error"].as_str().unwrap_or("").contains("text"));

        let m3 = dispatch(state.clone(), json!({"command":"search_labels"})).await;
        assert_eq!(m3["ok"], false);
        assert!(m3["error"].as_str().unwrap_or("").contains("query"));

        let m4 = dispatch(state.clone(), json!({"command":"dnd_set"})).await;
        assert_eq!(m4["ok"], false);
        assert!(m4["error"].as_str().unwrap_or("").contains("enabled"));

        let m5 = dispatch(state, json!({"command":"hooks_set"})).await;
        assert_eq!(m5["ok"], false);
        assert!(m5["error"].as_str().unwrap_or("").contains("hooks"));
    }

    #[tokio::test]
    async fn dispatch_hooks_and_iroh_info_paths() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let set = dispatch(
            state.clone(),
            json!({
                "command":"hooks_set",
                "hooks":[{
                    "name":"focus",
                    "enabled":true,
                    "keywords":["focus"],
                    "scenario":"any",
                    "command":"say",
                    "text":"hello",
                    "distance_threshold":0.2,
                    "recent_limit":8
                }]
            }),
        )
        .await;
        assert_eq!(set["ok"], true);
        assert!(set["hooks"].is_array());

        let get = dispatch(state.clone(), json!({"command":"hooks_get"})).await;
        assert_eq!(get["ok"], true);
        assert_eq!(get["hooks"].as_array().map(|a| a.len()).unwrap_or(0), 1);

        let status = dispatch(state.clone(), json!({"command":"hooks_status"})).await;
        assert_eq!(status["ok"], true);
        assert!(status["hooks"].is_array());

        let iroh = dispatch(state, json!({"command":"iroh_info"})).await;
        assert_eq!(iroh["ok"], true);
        // online=false because the tunnel hasn't started in tests
        assert_eq!(iroh["online"], false);
    }

    #[tokio::test]
    async fn dispatch_sleep_schedule_roundtrip_and_stub_command() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let before = dispatch(state.clone(), json!({"command":"sleep_schedule"})).await;
        assert_eq!(before["ok"], true);

        let set = dispatch(
            state.clone(),
            json!({
                "command":"sleep_schedule_set",
                "bedtime":"22:30",
                "wake_time":"06:45"
            }),
        )
        .await;
        assert_eq!(set["ok"], true);
        assert_eq!(set["bedtime"], "22:30");

        let timer = dispatch(state, json!({"command":"timer"})).await;
        assert_eq!(timer["ok"], false);
        assert!(timer["error"].as_str().unwrap_or("").contains("requires GUI"));
    }

    #[tokio::test]
    async fn dispatch_dnd_and_dnd_set_paths() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let dnd = dispatch(state.clone(), json!({"command":"dnd"})).await;
        assert_eq!(dnd["ok"], true);
        assert!(dnd.get("enabled").is_some());

        let off = dispatch(state.clone(), json!({"command":"dnd_set","enabled":false})).await;
        assert_eq!(off["ok"], true);
        assert_eq!(off["enabled"], false);

        let on = dispatch(state, json!({"command":"dnd_set","enabled":true})).await;
        assert_eq!(on["ok"], true);
        assert_eq!(on["enabled"], true);
        assert_eq!(on["applied"], false);
    }

    #[tokio::test]
    async fn dispatch_hooks_suggest_and_log_empty_paths() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let suggest = dispatch(state.clone(), json!({"command":"hooks_suggest","keywords":["focus"]})).await;
        assert_eq!(suggest["ok"], true);
        assert!(suggest.get("suggested").is_some());

        let log = dispatch(state, json!({"command":"hooks_log","limit":10,"offset":0})).await;
        assert_eq!(log["ok"], true);
        assert!(log["rows"].is_array());
        assert_eq!(log["count"], 0);
    }

    #[tokio::test]
    async fn dispatch_calibration_crud_roundtrip() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let created = dispatch(
            state.clone(),
            json!({"command":"create_calibration","name":"alpha","config":{"gain":2}}),
        )
        .await;
        assert_eq!(created["ok"], true);
        let id = created["id"].as_str().unwrap_or("").to_string();
        assert!(!id.is_empty());

        let listed = dispatch(state.clone(), json!({"command":"list_calibrations"})).await;
        assert_eq!(listed["ok"], true);
        assert!(listed["profiles"].as_array().map(|a| !a.is_empty()).unwrap_or(false));

        let got = dispatch(state.clone(), json!({"command":"get_calibration","id":id})).await;
        assert_eq!(got["ok"], true);
        assert!(got["profile"].get("id").is_some());

        let _updated = dispatch(
            state.clone(),
            json!({"command":"update_calibration","id":got["profile"]["id"],"name":"beta","config":{"gain":3}}),
        )
        .await;

        let deleted = dispatch(
            state.clone(),
            json!({"command":"delete_calibration","id":got["profile"]["id"]}),
        )
        .await;
        assert_eq!(deleted["ok"], true);

        let listed2 = dispatch(state, json!({"command":"list_calibrations"})).await;
        assert_eq!(listed2["ok"], true);
        assert_eq!(listed2["profiles"].as_array().map(|a| a.len()).unwrap_or(0), 0);
    }

    #[tokio::test]
    async fn dispatch_family_router_handles_known_groups() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let hooks = dispatch_family_commands(&state, &json!({}), "hooks_status").await;
        assert!(hooks.is_some());

        let dnd = dispatch_family_commands(&state, &json!({"enabled":false}), "dnd_set").await;
        assert!(dnd.is_some());

        let iroh_totp = dispatch_family_commands(&state, &json!({}), "iroh_totp_list").await;
        assert!(iroh_totp.is_some());

        let llm = dispatch_family_commands(&state, &json!({}), "llm_status").await;
        assert!(llm.is_some());

        let unknown = dispatch_family_commands(&state, &json!({}), "not-a-family-command").await;
        assert!(unknown.is_none());
    }

    #[tokio::test]
    async fn dispatch_command_matrix_has_no_unknowns() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let cases = vec![
            json!({"command":"status"}),
            json!({"command":"devices"}),
            json!({"command":"sessions"}),
            json!({"command":"hooks_status"}),
            json!({"command":"hooks_get"}),
            json!({"command":"hooks_log","limit":1,"offset":0}),
            json!({"command":"sleep_schedule"}),
            json!({"command":"dnd"}),
            json!({"command":"dnd_set","enabled":false}),
            json!({"command":"iroh_info"}),
            json!({"command":"iroh_totp_list"}),
            json!({"command":"llm_status"}),
            json!({"command":"llm_catalog"}),
            json!({"command":"llm_downloads"}),
            json!({"command":"llm_logs"}),
            json!({"command":"subscribe"}),
            json!({"command":"calendar_status"}),
            json!({"command":"health_metric_types"}),
            json!({"command":"timer"}),
        ];

        for msg in cases {
            let out = dispatch(state.clone(), msg).await;
            let err = out["error"].as_str().unwrap_or("");
            assert!(
                !err.contains("unknown command"),
                "dispatcher returned unknown command for payload: {out}"
            );
        }
    }
}
