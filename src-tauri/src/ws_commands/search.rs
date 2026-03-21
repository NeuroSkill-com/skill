// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! WebSocket search command handlers (search_labels, search, compare, interactive_search).

use serde_json::Value;
use tauri::AppHandle;

use tauri::Manager;

use crate::AppStateExt;
use crate::MutexExt;
use crate::skill_dir;
use super::umap_compute_inner;

// ── search_labels ─────────────────────────────────────────────────────────────

/// Search labels by free-text query using the fastembed HNSW index.
///
/// **Required**
/// - `query` — the text to embed and search for
///
/// **Optional**
/// - `k` — number of results to return (default 10, max 100)
/// - `ef` — HNSW ef parameter (default max(k×4, 64))
/// - `mode` — which field(s) to search against (default `"text"`):
///     - `"text"` — match against the label's short text embedding
///     - `"context"` — match against the label's long context embedding
///     - `"both"` — run both searches and merge by best distance
///
/// **Response**
/// ```json
/// {
///   "ok": true,
///   "command": "search_labels",
///   "query": "...",
///   "mode": "text",
///   "results": [
///     {
///       "label_id": 42,
///       "text": "...",
///       "context": "...",
///       "distance": 0.12,
///       "similarity": 0.88,
///       "eeg_start": 1700000000,
///       "eeg_end":   1700000300,
///       "created_at": 1700000010,
///       "embedding_model": "Xenova/bge-small-en-v1.5"
///     }
///   ]
/// }
/// ```
pub(super) fn search_labels(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let query = msg.get("query")
        .and_then(|v| v.as_str())
        .ok_or("missing required field: \"query\" (string)")?
        .trim()
        .to_owned();
    if query.is_empty() {
        return Err("\"query\" must not be empty".into());
    }

    let k    = msg.get("k") .and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(10).clamp(1, 100);
    let ef   = msg.get("ef").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or_else(|| (k * 4).max(64));
    let mode = msg.get("mode").and_then(|v| v.as_str()).unwrap_or("text");

    match mode {
        "text" | "context" | "both" => {}
        other => return Err(format!("invalid mode \"{other}\": must be \"text\", \"context\", or \"both\"")),
    }

    let (skill_dir, model_code) = crate::read_state(
        &app.app_state(),
        |s| (s.skill_dir.clone(), s.ui.text_embedding_model.clone()),
    );

    // Embed the query synchronously — ws_commands are called from a blocking
    // context (tokio spawn_blocking) so this is safe.
    let embedder_arc = std::sync::Arc::clone(
        &*app.state::<std::sync::Arc<crate::label_cmds::EmbedderState>>()
    );
    let label_idx_arc = std::sync::Arc::clone(
        &*app.state::<std::sync::Arc<crate::label_index::LabelIndexState>>()
    );

    let query_vec: Vec<f32> = {
        crate::label_cmds::ensure_embedder(&embedder_arc, &model_code, &skill_dir)?;
        let mut guard = embedder_arc.0.lock_or_recover();
        let te = guard.as_mut().ok_or("text embedder not initialised — model may still be downloading")?;
        let mut vecs = te.embed(vec![query.as_str()], None).map_err(|e| e.to_string())?;
        vecs.remove(0)
    };

    // Run whichever index searches are required.
    let merged: Vec<crate::label_index::LabelNeighbor> = match mode {
        "text" => crate::label_index::search_by_text_vec(&query_vec, k, ef, &skill_dir, &label_idx_arc),
        "context" => crate::label_index::search_by_context_vec(&query_vec, k, ef, &skill_dir, &label_idx_arc),
        _ => {
            let mut text_hits    = crate::label_index::search_by_text_vec   (&query_vec, k, ef, &skill_dir, &label_idx_arc);
            let mut context_hits = crate::label_index::search_by_context_vec(&query_vec, k, ef, &skill_dir, &label_idx_arc);
            // Merge: keep best distance per label_id, then sort and cap at k.
            text_hits.append(&mut context_hits);
            text_hits.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
            text_hits.dedup_by_key(|n| n.label_id);
            text_hits.truncate(k);
            text_hits
        }
    };

    // Enrich: add similarity (= 1 − distance for cosine) to each result.
    let results: Vec<Value> = merged.iter().map(|n| serde_json::json!({
        "label_id":        n.label_id,
        "text":            n.text,
        "context":         n.context,
        "distance":        n.distance,
        "similarity":      (1.0 - n.distance).clamp(0.0, 1.0),
        "eeg_start":       n.eeg_start,
        "eeg_end":         n.eeg_end,
        "created_at":      n.created_at,
        "embedding_model": n.embedding_model,
        "eeg_metrics":     n.eeg_metrics,
    })).collect();

    Ok(serde_json::json!({
        "query":  query,
        "mode":   mode,
        "model":  model_code,
        "k":      k,
        "count":  results.len(),
        "results": results,
    }))
}

// ── search ────────────────────────────────────────────────────────────────────

pub(super) fn search(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let start_utc = msg.get("start_utc")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing required field: \"start_utc\" (u64, unix seconds)".to_string())?;

    let end_utc = msg.get("end_utc")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "missing required field: \"end_utc\" (u64, unix seconds)".to_string())?;

    if end_utc < start_utc {
        return Err("\"end_utc\" must be >= \"start_utc\"".into());
    }

    let k  = msg.get("k").and_then(|v| v.as_u64()).map(|v| v as usize);
    let ef = msg.get("ef").and_then(|v| v.as_u64()).map(|v| v as usize);

    let skill_dir = skill_dir(&app.app_state());

    let k  = k.unwrap_or(10).clamp(1, 100);
    let ef = ef.unwrap_or(k.max(50));

    // Pull the global cross-day HNSW index from Tauri managed state so the
    // WebSocket search path gets the same accelerated backend as the UI.
    let global_arc = {
        use tauri::Manager as _;
        app.state::<std::sync::Arc<crate::global_eeg_index::GlobalEegIndex>>()
           .inner()
           .arc()
    };
    let result = crate::commands::search_embeddings_in_range(
        &skill_dir, start_utc, end_utc, k, ef, Some(global_arc),
    );

    let analysis = crate::analyze_search_results(&result);
    let mut result_json = serde_json::to_value(&result)
        .map_err(|e| format!("serialization error: {e}"))?;
    if let Some(obj) = result_json.as_object_mut() {
        obj.insert("analysis".into(), analysis);
    }
    Ok(serde_json::json!({ "result": result_json }))
}

// ── compare ───────────────────────────────────────────────────────────────────

/// Compare two time ranges by returning aggregated band-power metrics for each.
/// Return aggregated metrics for a single time range plus first/second-half
/// trend directions — lightweight alternative to `compare` that does not
/// enqueue a UMAP job.
///
/// Required parameters: `start_utc`, `end_utc` (unix seconds).
///
/// Returns `{ "metrics": SessionMetrics, "first": SessionMetrics,
///            "second": SessionMetrics, "trends": { metric: "up"|"down"|"flat" } }`.
pub(super) fn session_metrics(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let start = msg.get("start_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"start_utc\" (u64)")?;
    let end = msg.get("end_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"end_utc\" (u64)")?;
    if end < start { return Err("\"end_utc\" must be >= \"start_utc\"".into()); }

    let skill_dir = skill_dir(&app.app_state());
    let mid = (start + end) / 2;

    let full   = crate::get_session_metrics_impl(&skill_dir, start, end);
    let first  = crate::get_session_metrics_impl(&skill_dir, start, mid);
    let second = crate::get_session_metrics_impl(&skill_dir, mid,   end);

    // Direction: "up" if second-half > first-half by >5%, "down" if <-5%, else "flat".
    let dir = |a: f64, b: f64| -> &'static str {
        if a == 0.0 && b == 0.0 { return "flat"; }
        let rel = if a.abs() > 1e-9 { (b - a) / a.abs() } else { 0.0 };
        if rel >  0.05 { "up" } else if rel < -0.05 { "down" } else { "flat" }
    };

    // Compute trend direction for every numeric field dynamically so that
    // new SessionMetrics fields are covered automatically.
    let first_json  = serde_json::to_value(&first).unwrap_or_default();
    let second_json = serde_json::to_value(&second).unwrap_or_default();
    let mut trends  = serde_json::Map::new();
    if let (Some(fo), Some(so)) = (first_json.as_object(), second_json.as_object()) {
        for (key, fv) in fo {
            if key == "n_epochs" { continue; }
            if let (Some(f), Some(s)) = (fv.as_f64(), so.get(key).and_then(|v| v.as_f64())) {
                trends.insert(key.clone(), serde_json::json!(dir(f, s)));
            }
        }
    }

    Ok(serde_json::json!({
        "ok":      true,
        "command": "session_metrics",
        "metrics": serde_json::to_value(&full).unwrap_or(serde_json::Value::Null),
        "first":   first_json,
        "second":  second_json,
        "trends":  trends,
    }))
}

///
/// Required parameters:
///   `a_start_utc`, `a_end_utc` — unix seconds for session A
///   `b_start_utc`, `b_end_utc` — unix seconds for session B
///
/// Returns `{ "a": SessionMetrics, "b": SessionMetrics }`.
pub(super) fn compare(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let a_start = msg.get("a_start_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"a_start_utc\" (u64)")?;
    let a_end = msg.get("a_end_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"a_end_utc\" (u64)")?;
    let b_start = msg.get("b_start_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"b_start_utc\" (u64)")?;
    let b_end = msg.get("b_end_utc").and_then(|v| v.as_u64())
        .ok_or("missing required field: \"b_end_utc\" (u64)")?;

    if a_end < a_start { return Err("\"a_end_utc\" must be >= \"a_start_utc\"".into()); }
    if b_end < b_start { return Err("\"b_end_utc\" must be >= \"b_start_utc\"".into()); }

    let st = app.app_state();
    let skill_dir = skill_dir(&st);

    // ── Session metrics (all bands + derived scores + ratios + PPG) ──────────
    let metrics_a = crate::get_session_metrics_impl(&skill_dir, a_start, a_end);
    let metrics_b = crate::get_session_metrics_impl(&skill_dir, b_start, b_end);

    // ── Compare insights (timeseries stats, deltas, trends) ────────────────
    let insights = crate::compute_compare_insights(
        &skill_dir, a_start, a_end, b_start, b_end, &metrics_a, &metrics_b,
    );

    // ── Sleep staging ────────────────────────────────────────────────────────
    let sleep_a = crate::get_sleep_stages_impl(&skill_dir, a_start, a_end);
    let sleep_b = crate::get_sleep_stages_impl(&skill_dir, b_start, b_end);

    // ── UMAP — enqueue via job queue (non-blocking) ─────────────────────────
    let queue = app.state::<std::sync::Arc<crate::job_queue::JobQueue>>();

    let n_a = crate::load_embeddings_range(&skill_dir, a_start, a_end).len();
    let n_b = crate::load_embeddings_range(&skill_dir, b_start, b_end).len();
    let n = n_a + n_b;
    // Time estimate: KNN is O(n²) on GPU, training is O(epochs × edges).
    // Rough empirical formula: 3s base + n²/20000 ms + n×epochs/2000 ms.
    let ucfg_est = crate::load_umap_config(&skill_dir);
    let est_epochs = ucfg_est.n_epochs.clamp(50, 2000) as u64;
    let estimated_ms = 3000u64
        + (n as u64) * (n as u64) / 20_000
        + (n as u64) * est_epochs / 2000;

    let sd = skill_dir.clone();
    let (as2, ae2, bs2, be2) = (a_start, a_end, b_start, b_end);
    let prog_map = queue.progress_map();
    let umap_ticket = queue.submit_with_id(estimated_ms, move |job_id| {
        let pm = prog_map;
        let cb: Box<dyn Fn(fast_umap::EpochProgress) + Send> = Box::new(move |ep| {
            let mut map = pm.lock_or_recover();
            map.insert(job_id, crate::job_queue::JobProgress {
                epoch:        ep.epoch,
                total_epochs: ep.total_epochs,
                loss:         ep.loss,
                best_loss:    ep.best_loss,
                elapsed_secs: ep.elapsed_secs,
                epoch_ms:     ep.epoch_ms,
            });
        });
        umap_compute_inner(&sd, as2, ae2, bs2, be2, Some(cb))
    });

    Ok(serde_json::json!({
        "a": metrics_a,
        "b": metrics_b,
        "sleep_a": sleep_a,
        "sleep_b": sleep_b,
        "insights": insights,
        "umap": {
            "queued":              true,
            "job_id":              umap_ticket.job_id,
            "estimated_ready_utc": umap_ticket.estimated_ready_utc,
            "queue_position":      umap_ticket.queue_position,
            "estimated_secs":      umap_ticket.estimated_secs,
            "n_a":                 n_a,
            "n_b":                 n_b,
        },
    }))
}


// ── interactive_search ────────────────────────────────────────────────────────

/// `interactive_search { "query": "…", "k_text"?: 5, "k_eeg"?: 5, "k_labels"?: 3, "reach_minutes"?: 10 }`
///
/// Cross-modal 4-layer graph search:
///   Layer 0  query        — center node (the embedded query text)
///   Layer 1  text_label   — semantically similar label annotations
///   Layer 2  eeg_point    — raw EEG neighbors of label time windows
///   Layer 3  found_label  — labels near the EEG neighbors in time
///
/// Returns `{ nodes, edges, dot }` — same structure as the Tauri `interactive_search`
/// command used by the desktop UI, minus the SVG (not generated for the WS API).
pub(super) fn interactive_search(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    use crate::commands::{
        InteractiveGraphNode, InteractiveGraphEdge,
        list_date_dirs, load_day_index_for, get_labels_near, generate_dot, ts_to_unix,
    };
    use crate::constants::LABELS_FILE;

    let query = msg.get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required field: \"query\" (string)".to_string())?
        .to_owned();

    let k_text        = msg.get("k_text")       .and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(5).clamp(1, 20);
    let k_eeg         = msg.get("k_eeg")        .and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(5).clamp(1, 20);
    let k_labels      = msg.get("k_labels")     .and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(3).clamp(1, 10);
    let reach_minutes = msg.get("reach_minutes").and_then(|v| v.as_u64()).unwrap_or(10).clamp(1, 60);
    let reach_seconds = reach_minutes * 60;

    let (skill_dir, model_code, eeg_model_backend, screenshot_store) = crate::read_state(
        &app.app_state(),
        |s| (s.skill_dir.clone(), s.ui.text_embedding_model.clone(),
             s.embedding.model_config.model_backend.as_str().to_string(),
             s.screenshot_store.clone()),
    );

    let embedder_arc = std::sync::Arc::clone(
        &*app.state::<std::sync::Arc<crate::label_cmds::EmbedderState>>()
    );
    let label_idx_arc = std::sync::Arc::clone(
        &*app.state::<std::sync::Arc<crate::label_index::LabelIndexState>>()
    );

    // Embed the query (ws_commands run inside spawn_blocking — safe to block)
    let query_vec: Vec<f32> = {
        crate::label_cmds::ensure_embedder(&embedder_arc, &model_code, &skill_dir)?;
        let mut guard = embedder_arc.0.lock_or_recover();
        let te = guard.as_mut().ok_or("text embedder not initialised — model may still be downloading")?;
        let mut vecs = te.embed(vec![query.as_str()], None).map_err(|e| e.to_string())?;
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
        ..InteractiveGraphNode::default()
    });

    // Step 2: search the label text-HNSW for semantically similar labels.
    let ef_text = (k_text * 4).max(64);
    let text_labels = crate::label_index::search_by_text_vec(
        &query_vec, k_text, ef_text, &skill_dir, &label_idx_arc,
    );

    // Load all daily EEG HNSW indices once (re-used for every text label).
    let day_indices: Vec<_> = list_date_dirs(&skill_dir)
        .into_iter()
        .filter_map(|(date, dir)| load_day_index_for(date, dir, &eeg_model_backend))
        .collect();

    let ef_eeg    = (k_eeg * 4).max(64);
    let labels_db = skill_dir.join(LABELS_FILE);

    // Open screenshot store for step 6
    let ss_store = screenshot_store
        .or_else(|| skill_data::screenshot_store::ScreenshotStore::open(&skill_dir).map(std::sync::Arc::new));

    let mut seen_eeg: std::collections::HashSet<u64>  = std::collections::HashSet::new();
    let mut seen_labels: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut seen_screenshots: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Steps 3–6: per text label → EEG neighbors → nearby labels → screenshots.
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
            ..InteractiveGraphNode::default()
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
                ..InteractiveGraphNode::default()
            });
            edges.push(InteractiveGraphEdge {
                from_id:  tl_id.clone(),
                to_id:    ep_id.clone(),
                distance: *ep_dist,
                kind:     "eeg_bridge".into(),
            });

            // Step 5: find nearest labels around this EEG timestamp (±reach_minutes).
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
                        ..InteractiveGraphNode::default()
                    });
                    edges.push(InteractiveGraphEdge {
                        from_id:  ep_id.clone(),
                        to_id:    fl_id,
                        distance: t_dist,
                        kind:     "label_prox".into(),
                    });
                }
            }

            // Step 6: find screenshots near this EEG timestamp.
            if let Some(ref store) = ss_store {
                let screenshots = store.around_timestamp(
                    *ep_unix as i64,
                    reach_seconds as i32,
                );

                let query_lower = query.to_lowercase();
                let query_words: Vec<&str> = query_lower.split_whitespace().collect();

                for ss in screenshots.iter().take(k_labels.max(3)) {
                    if !seen_screenshots.insert(ss.filename.clone()) { continue; }

                    let time_dist = (ss.unix_ts as f32 - *ep_unix as f32).abs()
                        / (reach_seconds as f32);

                    let title_lower = ss.window_title.to_lowercase();
                    let app_lower   = ss.app_name.to_lowercase();
                    let title_match = if !query_words.is_empty() {
                        let hits = query_words.iter()
                            .filter(|w| title_lower.contains(*w) || app_lower.contains(*w))
                            .count();
                        hits as f32 / query_words.len() as f32
                    } else { 0.0 };

                    let ocr_lower = ss.ocr_text.to_lowercase();
                    let ocr_substr_match = if !query_words.is_empty() {
                        let hits = query_words.iter()
                            .filter(|w| ocr_lower.contains(*w))
                            .count();
                        hits as f32 / query_words.len() as f32
                    } else { 0.0 };

                    let relevance = title_match * 0.4
                        + ocr_substr_match * 0.4
                        + (1.0 - time_dist.min(1.0)) * 0.2;
                    if relevance < 0.05 && time_dist > 0.5 { continue; }

                    let ss_id = format!("ss_{}", ss.unix_ts);
                    let (edge_kind, edge_dist) = if title_match > 0.3 || ocr_substr_match > 0.3 {
                        ("ocr_sim", 1.0 - relevance)
                    } else {
                        ("screenshot_prox", time_dist)
                    };

                    nodes.push(InteractiveGraphNode {
                        id:             ss_id.clone(),
                        kind:           "screenshot".into(),
                        text:           if !ss.ocr_text.is_empty() {
                            Some(ss.ocr_text.chars().take(80).collect())
                        } else { None },
                        timestamp_unix: Some(ss.unix_ts),
                        distance:       edge_dist,
                        parent_id:      Some(ep_id.clone()),
                        filename:       Some(ss.filename.clone()),
                        app_name:       Some(ss.app_name.clone()),
                        window_title:   Some(ss.window_title.clone()),
                        ocr_text:       if !ss.ocr_text.is_empty() {
                            Some(ss.ocr_text.clone())
                        } else { None },
                        ocr_similarity: if ocr_substr_match > 0.0 {
                            Some(ocr_substr_match)
                        } else { None },
                        ..InteractiveGraphNode::default()
                    });
                    edges.push(InteractiveGraphEdge {
                        from_id:  ep_id.clone(),
                        to_id:    ss_id,
                        distance: edge_dist,
                        kind:     edge_kind.into(),
                    });
                }
            }
        }
    }

    let dot = generate_dot(&nodes, &edges);

    let nodes_json: Vec<serde_json::Value> = nodes.iter().map(|n| serde_json::json!({
        "id":             n.id,
        "kind":           n.kind,
        "text":           n.text,
        "timestamp_unix": n.timestamp_unix,
        "distance":       n.distance,
        "eeg_metrics":    n.eeg_metrics,
        "parent_id":      n.parent_id,
        "filename":       n.filename,
        "app_name":       n.app_name,
        "window_title":   n.window_title,
        "ocr_text":       n.ocr_text,
        "ocr_similarity": n.ocr_similarity,
    })).collect();

    let edges_json: Vec<serde_json::Value> = edges.iter().map(|e| serde_json::json!({
        "from_id":  e.from_id,
        "to_id":    e.to_id,
        "distance": e.distance,
        "kind":     e.kind,
    })).collect();

    Ok(serde_json::json!({
        "query":   query,
        "k_text":  k_text,
        "k_eeg":   k_eeg,
        "k_labels": k_labels,
        "reach_minutes": reach_minutes,
        "nodes":   nodes_json,
        "edges":   edges_json,
        "dot":     dot,
    }))
}

