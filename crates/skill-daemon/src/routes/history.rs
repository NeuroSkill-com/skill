// SPDX-License-Identifier: GPL-3.0-only
//! Daemon history routes — list/delete sessions, stats.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub csv_path: String,
    pub session_start_utc: Option<u64>,
    pub session_end_utc: Option<u64>,
    pub device_name: Option<String>,
    pub total_samples: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteSessionRequest {
    pub csv_path: String,
}

#[derive(Debug, Deserialize)]
pub struct FindSessionRequest {
    pub timestamp_utc: u64,
}

#[derive(Debug, Deserialize)]
pub struct DailyRecordingMinsRequest {
    pub days: Option<u32>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/history/sessions", get(list_sessions))
        .route("/history/sessions/delete", post(delete_session))
        .route("/history/stats", get(history_stats))
        .route("/history/find-session", post(find_session))
        .route("/history/daily-recording-mins", post(daily_recording_mins))
}

async fn list_sessions(State(state): State<AppState>) -> Json<Vec<SessionSummary>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let sessions = tokio::task::spawn_blocking(move || skill_history::list_all_sessions(&skill_dir, None))
        .await
        .unwrap_or_default();

    Json(
        sessions
            .into_iter()
            .map(|s| SessionSummary {
                csv_path: s.csv_path,
                session_start_utc: s.session_start_utc,
                session_end_utc: s.session_end_utc,
                device_name: s.device_name,
                total_samples: s.total_samples,
            })
            .collect(),
    )
}

async fn delete_session(
    State(_state): State<AppState>,
    Json(req): Json<DeleteSessionRequest>,
) -> Json<serde_json::Value> {
    let csv_path = req.csv_path.clone();
    let ok = tokio::task::spawn_blocking(move || skill_history::delete_session(&csv_path).is_ok())
        .await
        .unwrap_or(false);
    Json(serde_json::json!({ "ok": ok }))
}

async fn history_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let stats = tokio::task::spawn_blocking(move || skill_history::get_history_stats(&skill_dir))
        .await
        .unwrap_or(skill_history::HistoryStats {
            total_sessions: 0,
            total_secs: 0,
            this_week_secs: 0,
            last_week_secs: 0,
        });
    Json(serde_json::to_value(stats).unwrap_or_default())
}

async fn find_session(State(state): State<AppState>, Json(req): Json<FindSessionRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let found = tokio::task::spawn_blocking(move || {
        skill_history::find_session_csv_for_timestamp(&skill_dir, req.timestamp_utc)
    })
    .await
    .unwrap_or(None);
    Json(serde_json::json!({"csv_path": found}))
}

async fn daily_recording_mins(
    State(state): State<AppState>,
    Json(req): Json<DailyRecordingMinsRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let days = req.days;
    let out = tokio::task::spawn_blocking(move || {
        let n = days.unwrap_or(30).min(365) as i64;
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        fn unix_to_ymd(ts: u64) -> (u32, u32, u32) {
            let days = ts / 86400;
            let z = days + 719468;
            let era = z / 146097;
            let doe = z - era * 146097;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
            let y = yoe + era * 400;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let y = if m <= 2 { y + 1 } else { y };
            (y as u32, m as u32, d as u32)
        }

        let mut results: Vec<(String, u32)> = (0..n)
            .map(|i| {
                let day_secs = now_secs - i * 86400;
                let (y, mo, d) = unix_to_ymd(day_secs as u64);
                (format!("{y:04}{mo:02}{d:02}"), 0u32)
            })
            .collect();

        for (dir_date, total) in &mut results {
            let dir = skill_dir.join(dir_date.as_str());
            if !dir.is_dir() {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.filter_map(std::result::Result::ok) {
                let p = entry.path();
                let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !((fname.starts_with("exg_") || fname.starts_with("muse_")) && fname.ends_with(".json")) {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&p) else {
                    continue;
                };
                let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else {
                    continue;
                };
                let start = meta["session_start_utc"].as_u64().unwrap_or(0);
                let end = meta["session_end_utc"].as_u64().unwrap_or(start);
                *total += (end.saturating_sub(start) / 60) as u32;
            }
        }

        results.reverse();
        results
            .into_iter()
            .map(|(d, m)| serde_json::json!({"day": format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8]), "minutes": m}))
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    Json(serde_json::Value::Array(out))
}
