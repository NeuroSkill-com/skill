// SPDX-License-Identifier: GPL-3.0-only
//! Daemon search routes — EEG embedding search.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::state::AppState;

/// Unified request for `/v1/search/eeg`.
///
/// Multiple frontend commands route here with different payloads:
///   - stream_search_embeddings: { startUtc, endUtc, k }
///   - search_labels_by_text:    { query, k }
///   - interactive_search:       { query, kText, kEeg }
///   - regenerate_interactive_svg/dot, save_dot_file, save_svg_file
///
/// All fields are optional so every variant deserializes.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub start_utc: Option<u64>,
    pub end_utc: Option<u64>,
    pub k: Option<u64>,
    pub ef: Option<u64>,
    #[allow(dead_code)]
    pub query: Option<String>,
    #[allow(dead_code)]
    pub k_text: Option<u64>,
    #[allow(dead_code)]
    pub k_eeg: Option<u64>,
    #[allow(dead_code)]
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareSearchRequest {
    pub a_start_utc: u64,
    pub a_end_utc: u64,
    pub b_start_utc: u64,
    pub b_end_utc: u64,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/search/eeg", post(search_eeg))
        .route("/search/compare", post(compare_search))
        .route("/search/global-index/stats", get(global_index_stats))
        .route("/search/global-index/rebuild", post(global_index_rebuild))
}

async fn search_eeg(State(state): State<AppState>, Json(req): Json<SearchRequest>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();

    // Dispatch based on which fields are present.
    if let (Some(start), Some(end)) = (req.start_utc, req.end_utc) {
        // EEG embedding search (stream_search_embeddings)
        let k = req.k.unwrap_or(5) as usize;
        let ef = req.ef.unwrap_or(50) as usize;
        let result = tokio::task::spawn_blocking(move || {
            serde_json::to_value(skill_commands::search_embeddings_in_range(
                &skill_dir, start, end, k, ef, None,
            ))
            .unwrap_or_default()
        })
        .await
        .unwrap_or_default();
        Json(result)
    } else {
        // Text-based search or other commands that share this endpoint —
        // return empty results (text search is handled by /labels/search).
        Json(serde_json::json!({ "results": [] }))
    }
}

async fn compare_search(
    State(state): State<AppState>,
    Json(req): Json<CompareSearchRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let dir_a = skill_dir.clone();
    let result_a = tokio::task::spawn_blocking(move || {
        serde_json::to_value(skill_commands::search_embeddings_in_range(
            &dir_a,
            req.a_start_utc,
            req.a_end_utc,
            10,
            50,
            None,
        ))
        .unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    let dir_b = skill_dir.clone();
    let result_b = tokio::task::spawn_blocking(move || {
        serde_json::to_value(skill_commands::search_embeddings_in_range(
            &dir_b,
            req.b_start_utc,
            req.b_end_utc,
            10,
            50,
            None,
        ))
        .unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    Json(serde_json::json!({ "a": result_a, "b": result_b }))
}

async fn global_index_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let path = skill_dir.join(skill_constants::GLOBAL_HNSW_FILE);
    let file_size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    Json(serde_json::json!({
        "total_embeddings": 0,
        "file_size_bytes": file_size_bytes,
        "path": path.display().to_string(),
        "ready": true
    }))
}

async fn global_index_rebuild(State(state): State<AppState>) -> Json<serde_json::Value> {
    // Placeholder daemon-owned endpoint; full global-index lifecycle moved out of Tauri.
    global_index_stats(State(state)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn global_index_stats_reports_path_and_ready() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let Json(v) = global_index_stats(State(state)).await;
        assert_eq!(v["ready"], true);
        assert!(v["path"].as_str().unwrap_or("").contains("global"));
    }

    #[tokio::test]
    async fn compare_search_returns_a_and_b_keys() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let Json(v) = compare_search(
            State(state),
            Json(CompareSearchRequest {
                a_start_utc: 1,
                a_end_utc: 2,
                b_start_utc: 3,
                b_end_utc: 4,
            }),
        )
        .await;
        assert!(v.get("a").is_some());
        assert!(v.get("b").is_some());
    }
}
