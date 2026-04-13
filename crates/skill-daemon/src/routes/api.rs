// SPDX-License-Identifier: GPL-3.0-only
//! Daemon WS command routes — external API (REST equivalents of WS commands).

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};

use crate::state::AppState;
use skill_data;
use skill_settings;

/// Maximum number of commands allowed in a single batch request.
const BATCH_MAX_COMMANDS: usize = 20;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/status", get(api_status))
        .route("/api/sessions", get(api_sessions))
        .route("/api/label", post(api_create_label))
        // REST shortcuts used by test.ts and external integrations
        .route("/say", post(api_say))
        .route("/dnd", get(api_dnd_get).post(api_dnd_set))
        .route("/batch", post(api_batch))
}

async fn api_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let status = state.status.lock().ok().map(|g| g.clone()).unwrap_or_default();
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

async fn api_say(Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let text = req.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if text.is_empty() {
        return Json(serde_json::json!({ "command": "say", "ok": false, "error": "missing text" }));
    }
    let voice = req.get("voice").and_then(|v| v.as_str()).map(String::from);
    let spoken = text.clone();
    skill_tts::tts_speak(text, voice).await;
    Json(serde_json::json!({ "command": "say", "ok": true, "spoken": spoken }))
}

async fn api_dnd_get(State(state): State<AppState>) -> Json<serde_json::Value> {
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

async fn api_dnd_set(Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let enabled = req.get("enabled").and_then(serde_json::Value::as_bool).unwrap_or(false);
    let mode = req
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("com.apple.donotdisturb.mode.default");
    let ok = skill_data::dnd::set_dnd(enabled, mode);
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
    let db_path = skill_dir.join(skill_constants::LABELS_FILE);
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let result = rusqlite::Connection::open(&db_path).and_then(|conn| {
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
        )?;
        conn.execute(
            "INSERT INTO labels (text, context, eeg_start, eeg_end, wall_start, wall_end, created_at)
             VALUES (?1, '', ?2, ?2, ?2, ?2, ?2)",
            rusqlite::params![text, now_secs],
        )?;
        Ok(conn.last_insert_rowid())
    });
    match result {
        Ok(id) => Json(serde_json::json!({ "command": "label", "ok": true, "label_id": id })),
        Err(e) => Json(serde_json::json!({ "command": "label", "ok": false, "error": e.to_string() })),
    }
}

async fn api_batch(State(state): State<AppState>, Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let commands = match req.get("commands").and_then(|v| v.as_array()) {
        Some(arr) => arr.clone(),
        None => {
            return Json(serde_json::json!({
                "ok": false,
                "error": "missing or invalid \"commands\" array"
            }));
        }
    };

    if commands.len() > BATCH_MAX_COMMANDS {
        return Json(serde_json::json!({
            "ok": false,
            "error": format!("too many commands: {} (max {})", commands.len(), BATCH_MAX_COMMANDS)
        }));
    }

    let futures: Vec<_> = commands
        .into_iter()
        .map(|cmd| {
            let st = state.clone();
            tokio::spawn(async move { crate::cmd_dispatch::dispatch(st, cmd).await })
        })
        .collect();

    let mut results = Vec::with_capacity(futures.len());
    for f in futures {
        results.push(
            f.await
                .unwrap_or_else(|e| serde_json::json!({ "ok": false, "error": format!("task panic: {e}") })),
        );
    }

    Json(serde_json::json!({ "ok": true, "results": results }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn api_status_shape_is_stable() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let Json(v) = api_status(State(state)).await;
        assert_eq!(v["command"], "status");
        assert_eq!(v["ok"], true);
        assert!(v.get("state").is_some());
        assert!(v.get("sample_count").is_some());
    }

    #[tokio::test]
    async fn api_say_rejects_missing_text() {
        let Json(v) = api_say(Json(serde_json::json!({}))).await;
        assert_eq!(v["command"], "say");
        assert_eq!(v["ok"], false);
    }

    #[tokio::test]
    async fn api_dnd_set_false_is_accepted() {
        let Json(v) = api_dnd_set(Json(serde_json::json!({"enabled": false}))).await;
        assert_eq!(v["command"], "dnd_set");
        assert_eq!(v["ok"], true);
        assert_eq!(v["enabled"], false);
    }

    #[tokio::test]
    async fn api_create_label_persists_row() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let Json(v) = api_create_label(State(state.clone()), Json(serde_json::json!({"text": "focus marker"}))).await;
        assert_eq!(v["ok"], true);
        assert!(v["label_id"].as_i64().unwrap_or(0) > 0);

        let db = td.path().join(skill_constants::LABELS_FILE);
        let conn = rusqlite::Connection::open(db).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM labels WHERE text='focus marker'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn api_sessions_empty_dir_returns_empty_array() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let Json(v) = api_sessions(State(state)).await;
        assert_eq!(v["command"], "sessions");
        assert_eq!(v["ok"], true);
        assert!(v["sessions"].as_array().map(|a| a.is_empty()).unwrap_or(false));
    }
}
