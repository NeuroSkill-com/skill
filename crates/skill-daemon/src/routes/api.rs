// SPDX-License-Identifier: GPL-3.0-only
//! Daemon WS command routes — external API (REST equivalents of WS commands).

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};

use crate::state::AppState;
use skill_settings;
use skill_data;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/status", get(api_status))
        .route("/api/sessions", get(api_sessions))
        .route("/api/label", post(api_create_label))
        // REST shortcuts used by test.ts and external integrations
        .route("/say", post(api_say))
        .route("/dnd", get(api_dnd_get).post(api_dnd_set))
}

async fn api_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let status = state
        .status
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_else(|| skill_daemon_common::StatusResponse {
            state: "disconnected".to_string(),
            device_name: None,
            sample_count: 0,
            battery: 0.0,
            device_error: None,
            target_name: None,
            retry_attempt: 0,
            retry_countdown_secs: 0,
            paired_devices: Vec::new(),
        });
    Json(serde_json::json!({
        "command": "status",
        "ok": true,
        "state": status.state,
        "device_name": status.device_name,
        "battery": status.battery,
        "sample_count": status.sample_count,
        "device_error": status.device_error,
    }))
}

async fn api_sessions(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let sessions = tokio::task::spawn_blocking(move || skill_history::list_all_sessions(&skill_dir, None))
        .await
        .unwrap_or_default();

    let out: Vec<_> = sessions
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "csv_path": s.csv_path,
                "session_start_utc": s.session_start_utc,
                "session_end_utc": s.session_end_utc,
                "device_name": s.device_name,
                "total_samples": s.total_samples,
            })
        })
        .collect();

    Json(serde_json::json!({ "command": "sessions", "ok": true, "sessions": out }))
}

async fn api_say(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let text = req.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if text.is_empty() {
        return Json(serde_json::json!({ "command": "say", "ok": false, "error": "missing text" }));
    }
    let voice = req.get("voice").and_then(|v| v.as_str()).map(String::from);
    let spoken = text.clone();
    skill_tts::tts_speak(text, voice).await;
    Json(serde_json::json!({ "command": "say", "ok": true, "spoken": spoken }))
}

async fn api_dnd_get(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let settings = skill_settings::load_settings(&skill_dir);
    let cfg = settings.do_not_disturb;
    let os_active = skill_data::dnd::query_os_active();
    Json(serde_json::json!({
        "command": "dnd",
        "ok": true,
        "enabled": cfg.enabled,
        "threshold": cfg.focus_threshold,
        "duration_secs": cfg.duration_secs,
        "dnd_active": os_active.unwrap_or(false),
        "os_active": os_active,
    }))
}

async fn api_dnd_set(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let enabled = req.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let ok = if enabled { false } else { skill_data::dnd::set_dnd(false, "") };
    Json(serde_json::json!({
        "command": "dnd_set",
        "ok": true,
        "enabled": enabled,
        "applied": ok,
    }))
}

async fn api_create_label(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let text = req.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let db_path = skill_dir.join("labels.db");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let result = rusqlite::Connection::open(&db_path).and_then(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS labels (id INTEGER PRIMARY KEY AUTOINCREMENT, \
             text TEXT NOT NULL, context TEXT, created_at REAL NOT NULL);",
        )?;
        conn.execute(
            "INSERT INTO labels (text, context, created_at) VALUES (?1, NULL, ?2)",
            rusqlite::params![text, now],
        )?;
        Ok(conn.last_insert_rowid())
    });
    match result {
        Ok(id) => Json(serde_json::json!({ "command": "label", "ok": true, "label_id": id })),
        Err(e) => Json(serde_json::json!({ "command": "label", "ok": false, "error": e.to_string() })),
    }
}
