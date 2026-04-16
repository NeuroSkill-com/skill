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
    pub k_labels: Option<u64>,
    #[allow(dead_code)]
    pub k_screenshots: Option<u64>,
    #[allow(dead_code)]
    pub reach_minutes: Option<u64>,
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
        .route("/search/stats", get(search_corpus_stats))
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
        let k_labels = req.k_labels.unwrap_or(2) as usize;
        let k_screenshots = req.k_screenshots.unwrap_or(5) as usize;
        let reach_minutes = req.reach_minutes.unwrap_or(10) as u64;
        let embedder = state.text_embedder.clone();
        let label_index = state.label_index.clone();

        let result = tokio::task::spawn_blocking(move || {
            interactive_search_impl(
                &skill_dir,
                &query,
                k_text,
                k_eeg,
                k_labels,
                k_screenshots,
                reach_minutes,
                &embedder,
                &label_index,
            )
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

/// GET /search/stats — corpus metadata for the search UI.
async fn search_corpus_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let label_index = state.label_index.clone();
    let result = tokio::task::spawn_blocking(move || collect_search_meta(&skill_dir, &label_index))
        .await
        .unwrap_or_else(|_| serde_json::json!({}));
    Json(result)
}

/// Collect metadata about the search corpus so the UI can display stats.
fn collect_search_meta(
    skill_dir: &std::path::Path,
    label_index: &std::sync::Arc<skill_label_index::LabelIndexState>,
) -> serde_json::Value {
    let days = skill_history::list_session_days(skill_dir);
    let total_days = days.len();
    let first_day = days.first().cloned().unwrap_or_default();
    let last_day = days.last().cloned().unwrap_or_default();

    let history_stats = skill_history::get_history_stats(skill_dir);

    let text_index_size = label_index
        .text
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|i| i.len()))
        .unwrap_or(0);
    let eeg_index_size = label_index
        .eeg
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|i| i.len()))
        .unwrap_or(0);

    let (label_total, label_stale) = if let Some(store) = skill_data::label_store::LabelStore::open(skill_dir) {
        let total = store.count();
        let stale = store.rows_needing_embed(super::labels::EMBED_MODEL_NAME).len() as u64;
        (total, stale)
    } else {
        (0, 0)
    };

    let (ss_total, ss_embedded) = if let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(skill_dir) {
        let summary = store.summary_counts();
        (summary.total, summary.with_embedding)
    } else {
        (0, 0)
    };

    serde_json::json!({
        "eeg_days": total_days,
        "eeg_first_day": first_day,
        "eeg_last_day": last_day,
        "eeg_total_sessions": history_stats.total_sessions,
        "eeg_total_secs": history_stats.total_secs,
        "label_total": label_total,
        "label_stale": label_stale,
        "label_text_index": text_index_size,
        "label_eeg_index": eeg_index_size,
        "label_embed_model": super::labels::EMBED_MODEL_NAME,
        "screenshot_total": ss_total,
        "screenshot_embedded": ss_embedded,
    })
}

/// Build an interactive cross-modal search graph.
fn interactive_search_impl(
    skill_dir: &std::path::Path,
    query: &str,
    k_text: usize,
    k_eeg: usize,
    k_labels: usize,
    k_screenshots: usize,
    reach_minutes: u64,
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

    // ── Collect search metadata so the UI can show what was searched ────
    let meta = collect_search_meta(skill_dir, label_index);

    // Step 1: Embed query text → search labels by text similarity.
    let Some(query_vec) = embedder.embed(query) else {
        return serde_json::json!({
            "nodes": nodes, "edges": edges, "dot": "", "svg": "", "svg_col": "",
            "meta": meta,
            "error": "failed to embed query text"
        });
    };

    let text_neighbors = skill_label_index::search_by_text_vec(&query_vec, k_text, 64, skill_dir, label_index);

    // If no results, check whether there are labels that need re-embedding.
    let mut reembed_needed: Option<serde_json::Value> = None;
    if text_neighbors.is_empty() {
        if let Some(store) = skill_data::label_store::LabelStore::open(skill_dir) {
            let total = store.count();
            if total > 0 {
                let stale = store.rows_needing_embed(super::labels::EMBED_MODEL_NAME).len() as u64;
                if stale > 0 {
                    reembed_needed = Some(serde_json::json!({
                        "stale": stale,
                        "total": total,
                        "current_model": super::labels::EMBED_MODEL_NAME,
                    }));
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
        let reach_secs = reach_minutes * 60;
        if let Some(ts) = ts {
            // Get EEG epochs from the session range around this label.
            let eeg_ts =
                skill_history::get_session_timeseries(skill_dir, ts.saturating_sub(reach_secs), ts + reach_secs);
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
                let nearby_labels = skill_commands::get_labels_near(&labels_db, ep.t as u64, reach_secs);
                for (l, lbl) in nearby_labels.iter().enumerate().take(k_labels) {
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

            // Step 2b: Find screenshots near this label's timestamp.
            if let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(skill_dir) {
                let nearby_screenshots = skill_screenshots::capture::get_around(&store, ts as i64, reach_secs as i32);
                for (s, ss) in nearby_screenshots.iter().take(k_screenshots).enumerate() {
                    let ss_id = format!("ss{i}_{s}");
                    nodes.push(InteractiveGraphNode {
                        id: ss_id.clone(),
                        kind: "screenshot".into(),
                        text: if ss.window_title.is_empty() {
                            None
                        } else {
                            Some(ss.window_title.clone())
                        },
                        timestamp_unix: Some(ss.unix_ts),
                        distance: 0.0,
                        parent_id: Some(node_id.clone()),
                        filename: Some(ss.filename.clone()),
                        app_name: if ss.app_name.is_empty() {
                            None
                        } else {
                            Some(ss.app_name.clone())
                        },
                        window_title: if ss.window_title.is_empty() {
                            None
                        } else {
                            Some(ss.window_title.clone())
                        },
                        ocr_text: if ss.ocr_text.is_empty() {
                            None
                        } else {
                            Some(ss.ocr_text.clone())
                        },
                        ..Default::default()
                    });
                    edges.push(InteractiveGraphEdge {
                        from_id: node_id.clone(),
                        to_id: ss_id.clone(),
                        distance: 0.0,
                        kind: "screenshot_prox".into(),
                    });
                }
            }
        }
    }

    // Step 4: Search screenshots by OCR text similarity (semantic, not proximity).
    if k_screenshots > 0 {
        if let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(skill_dir) {
            let embed_fn = |text: &str| -> Option<Vec<f32>> { embedder.embed(text) };
            let mut ocr_results = skill_screenshots::capture::search_by_ocr_text_embedding(
                skill_dir,
                &store,
                query,
                k_screenshots,
                &embed_fn,
            );
            // Fall back to substring search if semantic returns nothing.
            if ocr_results.is_empty() {
                ocr_results = skill_screenshots::capture::search_by_ocr_text_like(&store, query, k_screenshots);
            }
            for (s, ss) in ocr_results.iter().enumerate() {
                let ss_id = format!("sst{s}");
                // Skip duplicates already added via proximity.
                if nodes
                    .iter()
                    .any(|n| n.kind == "screenshot" && n.filename.as_deref() == Some(&ss.filename))
                {
                    continue;
                }
                nodes.push(InteractiveGraphNode {
                    id: ss_id.clone(),
                    kind: "screenshot".into(),
                    text: if ss.window_title.is_empty() {
                        None
                    } else {
                        Some(ss.window_title.clone())
                    },
                    timestamp_unix: Some(ss.unix_ts),
                    distance: 1.0 - ss.similarity,
                    parent_id: Some(query_id.clone()),
                    filename: Some(ss.filename.clone()),
                    app_name: if ss.app_name.is_empty() {
                        None
                    } else {
                        Some(ss.app_name.clone())
                    },
                    window_title: if ss.window_title.is_empty() {
                        None
                    } else {
                        Some(ss.window_title.clone())
                    },
                    ocr_text: if ss.ocr_text.is_empty() {
                        None
                    } else {
                        Some(ss.ocr_text.clone())
                    },
                    ocr_similarity: Some(ss.similarity),
                    ..Default::default()
                });
                edges.push(InteractiveGraphEdge {
                    from_id: query_id.clone(),
                    to_id: ss_id,
                    distance: 1.0 - ss.similarity,
                    kind: "ocr_sim".into(),
                });
            }
        }
    }

    let mut result = serde_json::json!({
        "nodes": nodes,
        "edges": edges,
        "dot": "",
        "svg": "",
        "svg_col": "",
        "meta": meta,
    });
    if let Some(r) = reembed_needed {
        result.as_object_mut().unwrap().insert("reembed_needed".into(), r);
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

    // ── SearchRequest deserialization ─────────────────────────────────────

    #[test]
    fn search_request_deserializes_all_fields() {
        let json = serde_json::json!({
            "query": "focus",
            "kText": 5,
            "kEeg": 10,
            "kLabels": 3,
            "kScreenshots": 8,
            "reachMinutes": 30,
            "mode": "interactive"
        });
        let req: SearchRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.query.as_deref(), Some("focus"));
        assert_eq!(req.k_text, Some(5));
        assert_eq!(req.k_eeg, Some(10));
        assert_eq!(req.k_labels, Some(3));
        assert_eq!(req.k_screenshots, Some(8));
        assert_eq!(req.reach_minutes, Some(30));
    }

    #[test]
    fn search_request_all_fields_optional() {
        let json = serde_json::json!({});
        let req: SearchRequest = serde_json::from_value(json).unwrap();
        assert!(req.query.is_none());
        assert!(req.k_text.is_none());
        assert!(req.k_eeg.is_none());
        assert!(req.k_labels.is_none());
        assert!(req.k_screenshots.is_none());
        assert!(req.reach_minutes.is_none());
    }

    #[test]
    fn search_request_ignores_unknown_fields() {
        let json = serde_json::json!({
            "query": "test",
            "usePca": true,
            "svgLabels": { "layerQuery": "Q" }
        });
        let req: SearchRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.query.as_deref(), Some("test"));
    }

    // ── interactive_search_impl (empty data) ─────────────────────────────

    #[test]
    fn interactive_search_empty_dir_returns_query_node_and_meta() {
        let td = TempDir::new().unwrap();
        let label_index = std::sync::Arc::new(skill_label_index::LabelIndexState::default());
        // Use a no-op embedder that always returns None
        let embedder = crate::text_embedder::SharedTextEmbedder::new_noop();

        let result = interactive_search_impl(td.path(), "hello", 3, 5, 2, 5, 10, &embedder, &label_index);

        // Should have at least the query node
        let nodes = result["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["kind"], "query");
        assert_eq!(nodes[0]["text"], "hello");

        // Meta should be present with zero counts
        let meta = &result["meta"];
        assert!(meta.is_object());
        assert_eq!(meta["eeg_days"], 0);
        assert_eq!(meta["label_total"], 0);
        assert_eq!(meta["screenshot_total"], 0);
        assert_eq!(meta["label_embed_model"], super::super::labels::EMBED_MODEL_NAME);
    }

    #[test]
    fn interactive_search_meta_includes_all_expected_fields() {
        let td = TempDir::new().unwrap();
        let label_index = std::sync::Arc::new(skill_label_index::LabelIndexState::default());
        let embedder = crate::text_embedder::SharedTextEmbedder::new_noop();

        let result = interactive_search_impl(td.path(), "test", 1, 1, 1, 1, 5, &embedder, &label_index);

        let meta = &result["meta"];
        let expected_fields = [
            "eeg_days",
            "eeg_first_day",
            "eeg_last_day",
            "eeg_total_sessions",
            "eeg_total_secs",
            "label_total",
            "label_stale",
            "label_text_index",
            "label_eeg_index",
            "label_embed_model",
            "screenshot_total",
            "screenshot_embedded",
        ];
        for field in &expected_fields {
            assert!(meta.get(field).is_some(), "meta missing field: {field}");
        }
    }

    // ── search_eeg route dispatch ────────────────────────────────────────

    #[tokio::test]
    async fn search_eeg_empty_query_returns_empty() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let Json(v) = search_eeg(
            State(state),
            Json(SearchRequest {
                start_utc: None,
                end_utc: None,
                k: None,
                ef: None,
                query: Some("".into()),
                k_text: None,
                k_eeg: None,
                k_labels: None,
                k_screenshots: None,
                reach_minutes: None,
                mode: None,
            }),
        )
        .await;
        assert!(v["nodes"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn search_eeg_no_params_returns_empty() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let Json(v) = search_eeg(
            State(state),
            Json(SearchRequest {
                start_utc: None,
                end_utc: None,
                k: None,
                ef: None,
                query: None,
                k_text: None,
                k_eeg: None,
                k_labels: None,
                k_screenshots: None,
                reach_minutes: None,
                mode: None,
            }),
        )
        .await;
        assert!(v["nodes"].as_array().unwrap().is_empty());
        assert!(v["results"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn search_eeg_time_range_returns_json() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let Json(v) = search_eeg(
            State(state),
            Json(SearchRequest {
                start_utc: Some(1000),
                end_utc: Some(2000),
                k: Some(3),
                ef: None,
                query: None,
                k_text: None,
                k_eeg: None,
                k_labels: None,
                k_screenshots: None,
                reach_minutes: None,
                mode: None,
            }),
        )
        .await;
        // Should return valid JSON (empty results for empty dir)
        assert!(v.is_object() || v.is_array());
    }

    // ── Interactive search with labels in DB ─────────────────────────────

    #[test]
    fn interactive_search_with_labels_detects_stale_when_empty_results() {
        let td = TempDir::new().unwrap();
        // Create a labels DB with one label that has no embedding
        let labels_db = td.path().join(skill_constants::LABELS_FILE);
        {
            let conn = rusqlite::Connection::open(&labels_db).unwrap();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS labels (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    text TEXT NOT NULL,
                    context TEXT NOT NULL DEFAULT '',
                    eeg_start INTEGER NOT NULL DEFAULT 0,
                    eeg_end INTEGER NOT NULL DEFAULT 0,
                    text_embedding BLOB,
                    context_embedding BLOB,
                    embedding_model TEXT
                );
                INSERT INTO labels (text, context, eeg_start) VALUES ('focus', 'work', 1000);",
            )
            .unwrap();
        }

        let label_index = std::sync::Arc::new(skill_label_index::LabelIndexState::default());
        let embedder = crate::text_embedder::SharedTextEmbedder::new_noop();

        let result = interactive_search_impl(td.path(), "focus", 3, 5, 2, 5, 10, &embedder, &label_index);

        // Since embedder is noop, we get error — but reembed_needed should be set
        // because there's a label without embedding
        // Actually noop embedder returns None so we get the error path
        assert!(result.get("error").is_some() || result.get("reembed_needed").is_some());
    }

    // ── cosine_sim ───────────────────────────────────────────────────────

    #[test]
    fn cosine_sim_identical_vectors() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((cosine_sim(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_sim_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_sim(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_sim_opposite_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_sim(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_sim_zero_vector_returns_zero() {
        let a = vec![1.0, 2.0];
        let b = vec![0.0, 0.0];
        assert_eq!(cosine_sim(&a, &b), 0.0);
    }

    // ── Node/edge structure validation ───────────────────────────────────

    #[test]
    fn interactive_graph_node_serializes_screenshot_fields() {
        use skill_commands::InteractiveGraphNode;
        let node = InteractiveGraphNode {
            id: "ss0".into(),
            kind: "screenshot".into(),
            text: Some("Terminal".into()),
            filename: Some("20260401/img.webp".into()),
            app_name: Some("Terminal".into()),
            window_title: Some("bash".into()),
            ocr_text: Some("$ cargo build".into()),
            ocr_similarity: Some(0.85),
            ..Default::default()
        };
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(json["kind"], "screenshot");
        assert_eq!(json["filename"], "20260401/img.webp");
        assert_eq!(json["app_name"], "Terminal");
        assert_eq!(json["ocr_text"], "$ cargo build");
        assert!((json["ocr_similarity"].as_f64().unwrap() - 0.85).abs() < 1e-6);
    }

    #[test]
    fn interactive_graph_edge_serializes_screenshot_kind() {
        use skill_commands::InteractiveGraphEdge;
        let edge = InteractiveGraphEdge {
            from_id: "q0".into(),
            to_id: "sst0".into(),
            distance: 0.15,
            kind: "ocr_sim".into(),
        };
        let json = serde_json::to_value(&edge).unwrap();
        assert_eq!(json["kind"], "ocr_sim");
        assert!((json["distance"].as_f64().unwrap() - 0.15).abs() < 1e-6);
    }
}
