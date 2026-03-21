// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Thin Tauri command wrappers around the pure logic in `skill-commands`.
//!
//! All types, helpers, and core search/SVG/DOT functions are re-exported from
//! the `skill_commands` crate so the rest of the Tauri app can keep using
//! `crate::commands::*` unchanged.

use std::sync::{Arc, Mutex};

use tauri::Manager as _;

use crate::MutexExt;
use crate::skill_dir;
use crate::global_eeg_index::GlobalEegIndex;

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use skill_commands::*;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Normalise the optional `k` and `ef` search parameters used by every
/// embedding-search command.
fn search_params(k: Option<usize>, ef: Option<usize>) -> (usize, usize) {
    let k  = k.unwrap_or(10).clamp(1, 100);
    let ef = ef.unwrap_or(k.max(50));
    (k, ef)
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// Search EEG embeddings in a Unix-second timestamp range.
///
/// ### Parameters
/// | name        | type   | default | description |
/// |-------------|--------|---------|-------------|
/// | `start_utc` | u64    | —       | range start (Unix seconds, inclusive) |
/// | `end_utc`   | u64    | —       | range end   (Unix seconds, inclusive) |
/// | `k`         | usize? | 10      | nearest neighbours per query embedding |
/// | `ef`        | usize? | max(k,50)| HNSW search-quality parameter |
#[tauri::command]
pub fn search_embeddings(
    start_utc: u64,
    end_utc:   u64,
    k:         Option<usize>,
    ef:        Option<usize>,
    state:     tauri::State<'_, Mutex<Box<crate::AppState>>>,
    global:    tauri::State<'_, Arc<GlobalEegIndex>>,
) -> SearchResult {
    let dir = skill_dir(&state);
    let (k, ef) = search_params(k, ef);
    search_embeddings_in_range(&dir, start_utc, end_utc, k, ef, Some(global.arc()))
}

/// Enqueue search_embeddings as a background job.  Returns a JobTicket.
#[tauri::command]
pub fn enqueue_search_embeddings(
    start_utc: u64,
    end_utc:   u64,
    k:         Option<usize>,
    ef:        Option<usize>,
    state:     tauri::State<'_, Mutex<Box<crate::AppState>>>,
    queue:     tauri::State<'_, std::sync::Arc<crate::job_queue::JobQueue>>,
    global:    tauri::State<'_, Arc<GlobalEegIndex>>,
) -> crate::job_queue::JobTicket {
    let skill_dir   = skill_dir(&state);
    let global_arc  = global.arc();
    let (k, ef) = search_params(k, ef);

    // Estimate: range in seconds × ~0.5ms per second searched
    let range_s = end_utc.saturating_sub(start_utc);
    let estimated_ms = (range_s * 2).max(2000); // minimum 2s

    queue.submit(estimated_ms, move || {
        let result = search_embeddings_in_range(
            &skill_dir, start_utc, end_utc, k, ef, Some(global_arc),
        );
        serde_json::to_value(&result).map_err(|e| e.to_string())
    })
}

/// Streaming version of `search_embeddings`.
/// Emits a `SearchProgress` event per query embedding so the UI can show
/// results incrementally rather than waiting for the full search.
#[tauri::command]
pub async fn stream_search_embeddings(
    start_utc:   u64,
    end_utc:     u64,
    k:           Option<usize>,
    ef:          Option<usize>,
    on_progress: tauri::ipc::Channel<SearchProgress>,
    state:       tauri::State<'_, Mutex<Box<crate::AppState>>>,
    global:      tauri::State<'_, Arc<GlobalEegIndex>>,
) -> Result<(), String> {
    let skill_dir  = skill_dir(&state);
    let global_arc = global.arc();
    let (k, ef) = search_params(k, ef);

    tokio::task::spawn_blocking(move || {
        stream_search_inner(
            &skill_dir,
            start_utc,
            end_utc,
            k,
            ef,
            Some(global_arc),
            &|progress| { let _ = on_progress.send(progress); },
        );
    }).await.map_err(|e| e.to_string())
}

/// Find which recording session (csv_path) a given timestamp belongs to.
#[tauri::command]
pub fn find_session_for_timestamp(
    timestamp_unix: u64,
    date: String,  // YYYYMMDD
    state: tauri::State<'_, Mutex<Box<crate::AppState>>>,
) -> Option<SessionRef> {
    find_session_for_timestamp_in(&skill_dir(&state), timestamp_unix, &date)
}

/// Interactive cross-modal search.
///
/// Pipeline:
/// 1. Embed `query` text → text vector.
/// 2. Search the label text-HNSW → `k_text` semantically similar labels.
/// 3. For each text label, compute the mean EEG embedding over its time window.
/// 4. Search all daily EEG HNSW indices with that vector → `k_eeg` raw EEG neighbors.
/// 5. For each EEG neighbor timestamp, find the nearest label(s) in time.
/// 6. Return a graph: nodes (4 kinds) + typed edges (3 kinds).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn interactive_search(
    query:         String,
    k_text:        usize,
    k_eeg:         usize,
    k_labels:      usize,
    reach_minutes: u64,
    use_pca:       bool,
    svg_labels:    SvgLabels,
    state:         tauri::State<'_, Mutex<Box<crate::AppState>>>,
    embedder:  tauri::State<'_, std::sync::Arc<crate::label_cmds::EmbedderState>>,
    label_idx: tauri::State<'_, std::sync::Arc<crate::label_index::LabelIndexState>>,
) -> Result<InteractiveSearchResult, String> {
    let (skill_dir, eeg_model_backend, model_code) = {
        let s = state.lock_or_recover();
        (s.skill_dir.clone(), s.embedding.model_config.model_backend.as_str().to_string(),
         s.ui.text_embedding_model.clone())
    };
    let embedder  = std::sync::Arc::clone(&embedder);
    let label_idx = std::sync::Arc::clone(&label_idx);

    let k_text        = k_text.clamp(1, 20);
    let k_eeg         = k_eeg.clamp(1, 20);
    let k_labels      = k_labels.clamp(1, 10);
    let reach_seconds = reach_minutes.clamp(1, 60) * 60;

    tokio::task::spawn_blocking(move || {
        // ── Step 1: embed the query ────────────────────────────────────────
        let query_vec = {
            crate::label_cmds::ensure_embedder(&embedder, &model_code, &skill_dir)?;
            let mut guard = embedder.0.lock_or_recover();
            let te = guard.as_mut().ok_or("embedder not initialized")?;
            let mut vecs = te.embed(vec![query.as_str()], None)
                .map_err(|e| e.to_string())?;
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

        // ── Step 2: search the label text-HNSW ────────────────────────────
        let ef_text   = (k_text * 4).max(64);
        let text_labels = crate::label_index::search_by_text_vec(
            &query_vec, k_text, ef_text, &skill_dir, &label_idx,
        );

        // ── Load all daily EEG HNSW indices once ──────────────────────────
        let day_indices: Vec<DayIndex> = list_date_dirs(&skill_dir)
            .into_iter()
            .filter_map(|(date, dir)| load_day_index_for(date, dir, &eeg_model_backend))
            .collect();

        let ef_eeg    = (k_eeg * 4).max(64);
        let labels_db = skill_dir.join(skill_constants::LABELS_FILE);

        // Deduplication sets
        let mut seen_eeg: std::collections::HashSet<u64> = std::collections::HashSet::new();
        let mut seen_labels: std::collections::HashSet<i64> = std::collections::HashSet::new();

        // ── Steps 3-5: per text label ──────────────────────────────────────
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

                // Step 5: find nearest labels around this EEG timestamp.
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
                            proj_x:         None,
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

        // ── Step 6: PCA projection for found_labels ───────────────────────
        if labels_db.exists() {
            let fl_info: Vec<(usize, i64)> = nodes.iter().enumerate()
                .filter(|(_, n)| n.kind == "found_label")
                .filter_map(|(i, n)| {
                    n.id.strip_prefix("fl_")
                       .and_then(|s| s.parse::<i64>().ok())
                       .map(|lid| (i, lid))
                })
                .collect();

            let emb_info: Vec<(usize, Vec<f32>)> = fl_info.iter()
                .filter_map(|&(idx, lid)| {
                    get_found_label_embedding(&labels_db, lid)
                        .filter(|e| !e.is_empty())
                        .map(|e| (idx, e))
                })
                .collect();

            if emb_info.len() >= 2 {
                let embs: Vec<Vec<f32>> = emb_info.iter().map(|(_, e)| e.clone()).collect();
                let projections = pca_2d(&embs);
                for ((node_idx, _), (px, py)) in emb_info.iter().zip(projections.iter()) {
                    nodes[*node_idx].proj_x = Some(*px);
                    nodes[*node_idx].proj_y = Some(*py);
                }
            } else if emb_info.len() == 1 {
                nodes[emb_info[0].0].proj_x = Some(0.0);
                nodes[emb_info[0].0].proj_y = Some(0.0);
            }
        }

        let dot     = generate_dot(&nodes, &edges);
        let svg     = generate_svg(&nodes, &edges, &svg_labels, use_pca);
        let svg_col = generate_svg(&nodes, &edges, &svg_labels, false);
        Ok(InteractiveSearchResult { nodes, edges, dot, svg, svg_col })
    }).await.map_err(|e| e.to_string())?
}

// ─────────────────────────────────────────────────────────────────────────────

/// Resolve the save directory (Downloads → temp fallback).
fn save_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().download_dir()
        .or_else(|_e: tauri::Error| app.path().temp_dir())
        .map_err(|e: tauri::Error| e.to_string())
}

/// Write the DOT source to the user's Downloads folder and return the full path.
#[tauri::command]
pub fn save_dot_file(dot: String, query: String, app: tauri::AppHandle) -> Result<String, String> {
    let now  = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let slug = query_slug(&query, 40);
    let name = if slug.is_empty() {
        format!("search_{}.dot", file_ts(now))
    } else {
        format!("search_{}_{}.dot", slug, file_ts(now))
    };
    let path = save_dir(&app)?.join(&name);
    std::fs::write(&path, dot.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Save pre-rendered SVG content to Downloads (no external binary required).
#[tauri::command]
pub fn save_svg_file(svg: String, query: String, app: tauri::AppHandle) -> Result<String, String> {
    let now  = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let slug = query_slug(&query, 40);
    let name = if slug.is_empty() {
        format!("search_{}.svg", file_ts(now))
    } else {
        format!("search_{}_{}.svg", slug, file_ts(now))
    };
    let path = save_dir(&app)?.join(&name);
    std::fs::write(&path, svg.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}
