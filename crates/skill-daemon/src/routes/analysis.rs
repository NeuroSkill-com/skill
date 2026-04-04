// SPDX-License-Identifier: GPL-3.0-only
//! Daemon analysis routes — metrics, timeseries, sleep, compare, UMAP.

use axum::{extract::State, routing::post, Json, Router};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct TimeRangeRequest {
    pub start_utc: u64,
    pub end_utc: u64,
}

#[derive(Debug, Deserialize)]
struct CompareRequest {
    a_start_utc: u64,
    a_end_utc: u64,
    b_start_utc: u64,
    b_end_utc: u64,
}

#[derive(Debug, Deserialize)]
struct CsvMetricsRequest {
    csv_path: String,
}

#[derive(Debug, Deserialize)]
struct DayMetricsRequest {
    csv_paths: Vec<String>,
    max_ts_points: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SessionLocationRequest {
    csv_path: String,
    start_utc: u64,
    end_utc: u64,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/analysis/metrics", post(get_metrics))
        .route("/analysis/timeseries", post(get_timeseries))
        .route("/analysis/sleep", post(get_sleep))
        .route("/analysis/compare", post(compare))
        .route("/analysis/csv-metrics", post(csv_metrics))
        .route("/analysis/day-metrics", post(day_metrics))
        .route("/analysis/location", post(session_location))
        .route("/analysis/embedding-count", post(embedding_count))
        .route("/analysis/umap", post(umap_compare))
}

async fn get_metrics(State(state): State<AppState>, Json(req): Json<TimeRangeRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let result =
        tokio::task::spawn_blocking(move || skill_history::get_session_metrics(&skill_dir, req.start_utc, req.end_utc))
            .await
            .unwrap_or_default();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn get_timeseries(State(state): State<AppState>, Json(req): Json<TimeRangeRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let result = tokio::task::spawn_blocking(move || {
        skill_history::get_session_timeseries(&skill_dir, req.start_utc, req.end_utc)
    })
    .await
    .unwrap_or_default();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn get_sleep(State(state): State<AppState>, Json(req): Json<TimeRangeRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let result = tokio::task::spawn_blocking(move || {
        serde_json::to_value(skill_history::get_sleep_stages(&skill_dir, req.start_utc, req.end_utc))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(result)
}

async fn compare(State(state): State<AppState>, Json(req): Json<CompareRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let result = tokio::task::spawn_blocking(move || {
        let avg_a = skill_history::get_session_metrics(&skill_dir, req.a_start_utc, req.a_end_utc);
        let avg_b = skill_history::get_session_metrics(&skill_dir, req.b_start_utc, req.b_end_utc);
        skill_history::compute_compare_insights(
            &skill_dir,
            req.a_start_utc,
            req.a_end_utc,
            req.b_start_utc,
            req.b_end_utc,
            &avg_a,
            &avg_b,
        )
    })
    .await
    .unwrap_or_default();
    Json(result)
}

async fn csv_metrics(Json(req): Json<CsvMetricsRequest>) -> Json<serde_json::Value> {
    let result = tokio::task::spawn_blocking(move || {
        serde_json::to_value(skill_history::load_csv_metrics_cached(std::path::Path::new(
            &req.csv_path,
        )))
        .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(result)
}

async fn day_metrics(Json(req): Json<DayMetricsRequest>) -> Json<serde_json::Value> {
    let result = tokio::task::spawn_blocking(move || {
        skill_history::get_day_metrics_batch(&req.csv_paths, req.max_ts_points.unwrap_or(360))
    })
    .await
    .unwrap_or_default();
    Json(serde_json::to_value(result).unwrap_or_default())
}

async fn umap_compare(State(state): State<AppState>, Json(req): Json<CompareRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let result = tokio::task::spawn_blocking(move || {
        skill_router::umap_compute_inner(
            &skill_dir,
            req.a_start_utc,
            req.a_end_utc,
            req.b_start_utc,
            req.b_end_utc,
            None,
        )
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}))
    })
    .await
    .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
    Json(result)
}

async fn embedding_count(State(state): State<AppState>, Json(req): Json<TimeRangeRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let result = tokio::task::spawn_blocking(move || {
        let start_ms = req.start_utc as i64 * 1000;
        let end_ms = req.end_utc as i64 * 1000;

        fn utc_dir(ts: u64) -> String {
            let days = ts / 86400;
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

        let dir1 = utc_dir(req.start_utc);
        let dir2 = utc_dir(req.end_utc);
        let mut total: u64 = 0;
        for dir_name in std::collections::HashSet::from([dir1, dir2]) {
            let db = skill_dir.join(&dir_name).join("eeg.sqlite");
            if !db.exists() {
                continue;
            }
            if let Ok(conn) = rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
                let n: u64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM embeddings WHERE timestamp >= ?1 AND timestamp <= ?2",
                        rusqlite::params![start_ms, end_ms],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                total += n;
            }
        }
        serde_json::json!({"count": total})
    })
    .await
    .unwrap_or_else(|e| serde_json::json!({"error": e.to_string(), "count": 0u64}));
    Json(result)
}

async fn session_location(
    State(state): State<AppState>,
    Json(req): Json<SessionLocationRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let result = tokio::task::spawn_blocking(move || {
        let mut points: Vec<serde_json::Value> = Vec::new();
        let sidecar = std::path::Path::new(&req.csv_path).with_extension("meta.jsonl");
        if let Ok(text) = std::fs::read_to_string(&sidecar) {
            for line in text.lines() {
                let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                if val.get("type").and_then(|v| v.as_str()) != Some("location") {
                    continue;
                }
                let ts = val["timestamp"].as_f64().unwrap_or(0.0);
                if ts < req.start_utc as f64 || ts > req.end_utc as f64 {
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
                points.push(serde_json::json!({
                    "ts": ts,
                    "lat": lat,
                    "lon": lon,
                    "alt": val["altitude"].as_f64().unwrap_or(0.0),
                    "accuracy": val["accuracy"].as_f64().unwrap_or(0.0),
                    "speed": val["speed"].as_f64().unwrap_or(0.0)
                }));
            }
        }

        // NOTE: health-store GPS DB integration remains in client path today.
        // This daemon endpoint currently serves sidecar location points only.
        let _ = skill_dir;

        points.sort_by(|a, b| {
            let ta = a.get("ts").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
            let tb = b.get("ts").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
            ta.partial_cmp(&tb).unwrap_or(std::cmp::Ordering::Equal)
        });
        serde_json::Value::Array(points)
    })
    .await
    .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
    Json(result)
}
