// SPDX-License-Identifier: GPL-3.0-only
//! Daemon search routes — EEG embedding search.

use axum::{
    extract::State,
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::{Stream, StreamExt};
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
        .route("/search/eeg/stream", post(search_eeg_stream))
        .route("/search/compare", post(compare_search))
        .route("/search/commands", post(search_commands))
        .route("/search/global-index/stats", get(global_index_stats))
        .route("/search/global-index/rebuild", post(global_index_rebuild))
}

// ── Cmd-K semantic command search ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CommandCandidate {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct CommandSearchRequest {
    pub query: String,
    pub candidates: Vec<CommandCandidate>,
}

/// Cosine similarity between two vectors.
fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// Semantic search over Cmd-K command candidates using text embeddings.
/// Embeds the query and all candidate texts, returns top-5 by cosine similarity.
async fn search_commands(
    State(state): State<AppState>,
    Json(req): Json<CommandSearchRequest>,
) -> Json<serde_json::Value> {
    let embedder = state.text_embedder.clone();
    let query = req.query;
    let candidates = req.candidates;

    let result = tokio::task::spawn_blocking(move || {
        let Some(query_vec) = embedder.embed(&query) else {
            return serde_json::json!({ "results": [] });
        };

        // Batch-embed all candidates
        let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
        let Some(cand_vecs) = embedder.embed_batch(texts) else {
            return serde_json::json!({ "results": [] });
        };

        // Score and rank
        let mut scored: Vec<(usize, f32)> = cand_vecs
            .iter()
            .enumerate()
            .map(|(i, v)| (i, cosine_sim(&query_vec, v)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let results: Vec<serde_json::Value> = scored
            .iter()
            .take(5)
            .filter(|(_, s)| *s > 0.3) // threshold for relevance
            .map(|(i, s)| serde_json::json!({ "id": candidates[*i].id, "score": s }))
            .collect();

        serde_json::json!({ "results": results })
    })
    .await
    .unwrap_or_else(|_| serde_json::json!({ "results": [] }));

    Json(result)
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
    } else if let Some(query) = req.query.filter(|q| !q.trim().is_empty()) {
        // Interactive cross-modal search:
        // 1. Embed query → search text labels
        // 2. For each label → find nearby EEG epochs
        // 3. For each EEG epoch → find temporal neighbors
        let k_text = req.k_text.unwrap_or(3) as usize;
        let k_eeg = req.k_eeg.unwrap_or(5) as usize;
        let embedder = state.text_embedder.clone();
        let label_index = state.label_index.clone();

        let result = tokio::task::spawn_blocking(move || {
            interactive_search_impl(&skill_dir, &query, k_text, k_eeg, &embedder, &label_index)
        })
        .await
        .unwrap_or_else(|_| {
            serde_json::json!({
                "nodes": [], "edges": [], "dot": "", "svg": "", "svg_col": ""
            })
        });
        Json(result)
    } else {
        // No recognized parameters — return empty.
        Json(serde_json::json!({
            "nodes": [], "edges": [], "dot": "", "svg": "", "svg_col": "",
            "results": []
        }))
    }
}

/// SSE streaming EEG search — sends results as they're found.
/// The client can cancel by closing the connection.
async fn search_eeg_stream(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let start_utc = req.start_utc.unwrap_or(0);
    let end_utc = req.end_utc.unwrap_or(0);
    let k = req.k.unwrap_or(5) as usize;
    let ef = req.ef.unwrap_or(50) as usize;

    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(64);

    tokio::task::spawn_blocking(move || {
        let emit = |progress: skill_commands::SearchProgress| {
            let json = serde_json::to_string(&progress).unwrap_or_default();
            let event = Event::default().data(json);
            // If send fails, the client disconnected — stop searching.
            tx.blocking_send(event).is_ok()
        };

        // Emit "started" first, then results one by one.
        skill_commands::stream_search_inner(&skill_dir, start_utc, end_utc, k, ef, None, &|progress| {
            emit(progress);
        });
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(Ok);
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}

/// Build an interactive cross-modal search graph.
fn interactive_search_impl(
    skill_dir: &std::path::Path,
    query: &str,
    k_text: usize,
    k_eeg: usize,
    embedder: &crate::text_embedder::SharedTextEmbedder,
    label_index: &std::sync::Arc<skill_label_index::LabelIndexState>,
) -> serde_json::Value {
    use skill_commands::{InteractiveGraphEdge, InteractiveGraphNode};
    use skill_constants::LABELS_FILE;

    let mut nodes: Vec<InteractiveGraphNode> = Vec::new();
    let mut edges: Vec<InteractiveGraphEdge> = Vec::new();

    // Query node
    let query_id = "q0".to_string();
    nodes.push(InteractiveGraphNode {
        id: query_id.clone(),
        kind: "query".into(),
        text: Some(query.to_string()),
        distance: 0.0,
        ..Default::default()
    });

    // Step 1: Embed query text → search labels by text similarity.
    let Some(query_vec) = embedder.embed(query) else {
        return serde_json::json!({
            "nodes": nodes, "edges": edges, "dot": "", "svg": "", "svg_col": "",
            "error": "failed to embed query text"
        });
    };

    let text_neighbors = skill_label_index::search_by_text_vec(&query_vec, k_text, 64, skill_dir, label_index);

    // If no results, check whether there are labels that need re-embedding.
    let mut warning: Option<String> = None;
    if text_neighbors.is_empty() {
        if let Some(store) = skill_data::label_store::LabelStore::open(skill_dir) {
            let total = store.count();
            if total > 0 {
                let stale = store.rows_needing_embed(super::labels::EMBED_MODEL_NAME).len() as u64;
                if stale > 0 {
                    warning = Some(format!(
                        "{stale} of {total} labels need re-embedding. POST /v1/labels/reembed to fix."
                    ));
                }
            }
        }
    }

    // Add text_label nodes.
    for (i, nb) in text_neighbors.iter().enumerate() {
        let node_id = format!("tl{i}");
        let ts = if nb.eeg_start > 0 { Some(nb.eeg_start) } else { None };
        nodes.push(InteractiveGraphNode {
            id: node_id.clone(),
            kind: "text_label".into(),
            text: Some(nb.text.clone()),
            timestamp_unix: ts,
            distance: nb.distance,
            parent_id: Some(query_id.clone()),
            ..Default::default()
        });
        edges.push(InteractiveGraphEdge {
            from_id: query_id.clone(),
            to_id: node_id.clone(),
            distance: nb.distance,
            kind: "text_sim".into(),
        });

        // Step 2: For each text label with a timestamp, find nearby EEG epochs.
        if let Some(ts) = ts {
            // Get EEG epochs from the session range around this label.
            let eeg_ts = skill_history::get_session_timeseries(skill_dir, ts.saturating_sub(60), ts + 60);
            for (j, ep) in eeg_ts.iter().take(k_eeg).enumerate() {
                let eeg_id = format!("ep{i}_{j}");
                nodes.push(InteractiveGraphNode {
                    id: eeg_id.clone(),
                    kind: "eeg_point".into(),
                    timestamp_unix: Some(ep.t as u64),
                    distance: 0.0,
                    parent_id: Some(node_id.clone()),
                    ..Default::default()
                });
                edges.push(InteractiveGraphEdge {
                    from_id: node_id.clone(),
                    to_id: eeg_id.clone(),
                    distance: 0.0,
                    kind: "eeg_bridge".into(),
                });

                // Step 3: Find labels near each EEG epoch.
                let labels_db = skill_dir.join(LABELS_FILE);
                let nearby_labels = skill_commands::get_labels_near(&labels_db, ep.t as u64, 60);
                for (l, lbl) in nearby_labels.iter().enumerate().take(2) {
                    let fl_id = format!("fl{i}_{j}_{l}");
                    nodes.push(InteractiveGraphNode {
                        id: fl_id.clone(),
                        kind: "found_label".into(),
                        text: Some(lbl.text.clone()),
                        timestamp_unix: Some(lbl.eeg_start),
                        distance: 0.0,
                        parent_id: Some(eeg_id.clone()),
                        ..Default::default()
                    });
                    edges.push(InteractiveGraphEdge {
                        from_id: eeg_id.clone(),
                        to_id: fl_id.clone(),
                        distance: 0.0,
                        kind: "label_prox".into(),
                    });
                }
            }
        }
    }

    let mut result = serde_json::json!({
        "nodes": nodes,
        "edges": edges,
        "dot": "",
        "svg": "",
        "svg_col": "",
    });
    if let Some(w) = warning {
        result
            .as_object_mut()
            .unwrap()
            .insert("warning".into(), serde_json::json!(w));
    }
    result
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
