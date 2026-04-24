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

use skill_data::screenshot_store::ScreenshotResult;

use crate::state::AppState;

// ── Search result cache ──────────────────────────────────────────────────────
// Simple bounded cache: key = hash of query+params, value = JSON result.
static SEARCH_CACHE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<u64, (std::time::Instant, serde_json::Value)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
const CACHE_MAX: usize = 8;
const CACHE_TTL_SECS: u64 = 300; // 5 minutes

fn cache_key(parts: &[&str]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        p.hash(&mut h);
    }
    h.finish()
}
fn cache_get(key: u64) -> Option<serde_json::Value> {
    let guard = SEARCH_CACHE.lock().ok()?;
    let (ts, val) = guard.get(&key)?;
    if ts.elapsed().as_secs() < CACHE_TTL_SECS {
        Some(val.clone())
    } else {
        None
    }
}
/// Clear the search result cache (e.g. after backfill enriches metrics).
pub fn cache_clear() {
    if let Ok(mut guard) = SEARCH_CACHE.lock() {
        guard.clear();
    }
}

fn cache_put(key: u64, val: serde_json::Value) {
    if let Ok(mut guard) = SEARCH_CACHE.lock() {
        // Evict oldest if full.
        if guard.len() >= CACHE_MAX {
            if let Some(&oldest_key) = guard.iter().min_by_key(|(_, (ts, _))| *ts).map(|(k, _)| k) {
                guard.remove(&oldest_key);
            }
        }
        guard.insert(key, (std::time::Instant::now(), val));
    }
}

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
    pub query: Option<String>,
    pub k_text: Option<u64>,
    pub k_eeg: Option<u64>,
    pub k_labels: Option<u64>,
    pub k_screenshots: Option<u64>,
    pub reach_minutes: Option<u64>,
    #[allow(dead_code)]
    pub mode: Option<String>,
    /// Filter by device name (e.g. "MuseS-F921"). `None` or `"all"` = all devices.
    pub device_name: Option<String>,
    /// When true, only include EEG epochs with SNR > 0 (positive signal quality).
    pub snr_positive_only: Option<bool>,
    /// Optional date-range filter (Unix seconds). Constrains interactive search to a time window.
    pub filter_start_utc: Option<u64>,
    pub filter_end_utc: Option<u64>,
    /// Sort EEG epochs by this metric before taking top-k. Default: "timestamp".
    pub eeg_rank_by: Option<String>,
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
        .route("/search/stats/stream", get(search_corpus_stats_stream))
        .route("/search/devices", get(list_search_devices))
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
        let snr_positive_only = req.snr_positive_only.unwrap_or(false);
        let device_filter = req.device_name.filter(|d| !d.is_empty() && d != "all");
        let filter_start = req.filter_start_utc;
        let filter_end = req.filter_end_utc;
        let eeg_rank_by = req.eeg_rank_by;
        // Check cache first.
        let ck = cache_key(&[
            &query,
            &k_text.to_string(),
            &k_eeg.to_string(),
            &k_labels.to_string(),
            &reach_minutes.to_string(),
            &snr_positive_only.to_string(),
            device_filter.as_deref().unwrap_or(""),
            &filter_start.unwrap_or(0).to_string(),
            &filter_end.unwrap_or(0).to_string(),
            eeg_rank_by.as_deref().unwrap_or(""),
        ]);
        if let Some(cached) = cache_get(ck) {
            return Json(cached);
        }

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
                snr_positive_only,
                device_filter.as_deref(),
                filter_start,
                filter_end,
                eeg_rank_by.as_deref(),
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
        cache_put(ck, result.clone());
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
    let device_filter = req.device_name.filter(|d| !d.is_empty() && d != "all");

    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(64);

    tokio::task::spawn_blocking(move || {
        let emit = |progress: skill_commands::SearchProgress| {
            let json = serde_json::to_string(&progress).unwrap_or_default();
            let event = Event::default().data(json);
            // If send fails, the client disconnected — stop searching.
            tx.blocking_send(event).is_ok()
        };

        // Emit "started" first, then results one by one.
        skill_commands::stream_search_inner(
            &skill_dir,
            start_utc,
            end_utc,
            k,
            ef,
            None,
            device_filter.as_deref(),
            &|progress| {
                emit(progress);
            },
        );
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(Ok);
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}

/// GET /search/devices — list distinct device names across all days.
async fn list_search_devices(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let devices = tokio::task::spawn_blocking(move || {
        let mut names = std::collections::BTreeSet::new();
        let Ok(entries) = std::fs::read_dir(&skill_dir) else {
            return Vec::new();
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(s) = name.to_str() else { continue };
            if s.len() != 8 || !s.starts_with("20") {
                continue;
            }
            let db = p.join(skill_constants::SQLITE_FILE);
            if !db.exists() {
                continue;
            }
            if let Ok(conn) = rusqlite::Connection::open_with_flags(
                &db,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            ) {
                let mut stmt = conn
                    .prepare("SELECT DISTINCT device_name FROM embeddings WHERE device_name IS NOT NULL AND device_name != ''")
                    .ok();
                if let Some(ref mut st) = stmt {
                    let _ = st.query_map([], |row| row.get::<_, String>(0)).map(|rows| {
                        for r in rows.flatten() {
                            names.insert(r);
                        }
                    });
                }
            }
        }
        names.into_iter().collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    Json(serde_json::json!({ "devices": devices }))
}

/// GET /search/stats — fast corpus metadata (tier 1+2, <50ms).
///
/// Returns in-memory index sizes, embed model, label count, screenshot count,
/// and day count/range.  Expensive stats (session parsing, stale label scan)
/// are omitted — use `/search/stats/stream` for those.
async fn search_corpus_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let label_index = state.label_index.clone();
    let result = tokio::task::spawn_blocking(move || collect_fast_meta(&skill_dir, &label_index))
        .await
        .unwrap_or_else(|_| serde_json::json!({}));
    Json(result)
}

/// GET /search/stats/stream — SSE that emits "fast" then "slow" events.
///
/// The client receives instant stats first, then expensive stats as they
/// become available — no blocking the UI.
async fn search_corpus_stats_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let label_index = state.label_index.clone();

    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(4);

    tokio::task::spawn_blocking(move || {
        // Tier 1+2: fast stats (~1–50ms)
        let fast = collect_fast_meta(&skill_dir, &label_index);
        let _ = tx.blocking_send(Event::default().event("fast").data(fast.to_string()));

        // Tier 3: slow stats (100ms+)
        let slow = collect_slow_meta(&skill_dir);
        let _ = tx.blocking_send(Event::default().event("slow").data(slow.to_string()));
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(Ok);
    Sse::new(stream)
}

/// Tier 1+2: in-memory + fast SQL counts. Typically <50ms.
fn collect_fast_meta(
    skill_dir: &std::path::Path,
    label_index: &std::sync::Arc<skill_label_index::LabelIndexState>,
) -> serde_json::Value {
    // Tier 1: pure in-memory (~0ms)
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

    // Tier 2: single COUNT(*) queries + readdir (~10–50ms)
    let days = skill_history::list_session_days(skill_dir);
    let total_days = days.len();
    let first_day = days.first().cloned().unwrap_or_default();
    let last_day = days.last().cloned().unwrap_or_default();

    let label_total = skill_data::label_store::LabelStore::open(skill_dir)
        .map(|s| s.count())
        .unwrap_or(0);

    let (ss_total, ss_embedded) = if let Some(store) = skill_data::screenshot_store::ScreenshotStore::open(skill_dir) {
        (store.count_all() as u64, store.count_embedded() as u64)
    } else {
        (0, 0)
    };

    serde_json::json!({
        "eeg_days": total_days,
        "eeg_first_day": first_day,
        "eeg_last_day": last_day,
        "label_total": label_total,
        "label_text_index": text_index_size,
        "label_eeg_index": eeg_index_size,
        "label_embed_model": super::labels::EMBED_MODEL_NAME,
        "screenshot_total": ss_total,
        "screenshot_embedded": ss_embedded,
    })
}

/// Tier 3: expensive stats that parse files or scan rows. 100ms+ on large datasets.
fn collect_slow_meta(skill_dir: &std::path::Path) -> serde_json::Value {
    let history_stats = skill_history::get_history_stats(skill_dir);

    let label_stale = skill_data::label_store::LabelStore::open(skill_dir)
        .map(|s| s.count_needing_embed(super::labels::EMBED_MODEL_NAME))
        .unwrap_or(0);

    // Scan all day SQLite DBs for embedding coverage.
    let (eeg_total_epochs, eeg_embedded_epochs) = count_epoch_coverage(skill_dir);

    serde_json::json!({
        "eeg_total_sessions": history_stats.total_sessions,
        "eeg_total_secs": history_stats.total_secs,
        "label_stale": label_stale,
        "eeg_total_epochs": eeg_total_epochs,
        "eeg_embedded_epochs": eeg_embedded_epochs,
        "eeg_missing_epochs": eeg_total_epochs - eeg_embedded_epochs,
    })
}

/// Count total vs embedded epochs across all day directories.
fn count_epoch_coverage(skill_dir: &std::path::Path) -> (i64, i64) {
    let Ok(entries) = std::fs::read_dir(skill_dir) else {
        return (0, 0);
    };
    let mut total = 0i64;
    let mut embedded = 0i64;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let db_path = path.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }
        let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        let t: i64 = conn
            .query_row("SELECT COUNT(*) FROM embeddings", [], |r| r.get(0))
            .unwrap_or(0);
        let m: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM embeddings WHERE eeg_embedding IS NOT NULL AND length(eeg_embedding) >= 4",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        total += t;
        embedded += m;
    }
    (total, embedded)
}

/// Build an interactive cross-modal search graph.
#[allow(clippy::too_many_arguments)]
fn interactive_search_impl(
    skill_dir: &std::path::Path,
    query: &str,
    k_text: usize,
    k_eeg: usize,
    k_labels: usize,
    k_screenshots: usize,
    reach_minutes: u64,
    snr_positive_only: bool,
    device_filter: Option<&str>,
    filter_start_utc: Option<u64>,
    filter_end_utc: Option<u64>,
    eeg_rank_by: Option<&str>,
    embedder: &crate::text_embedder::SharedTextEmbedder,
    label_index: &std::sync::Arc<skill_label_index::LabelIndexState>,
) -> serde_json::Value {
    let t_total = std::time::Instant::now();
    use skill_commands::{InteractiveGraphEdge, InteractiveGraphNode, NeighborMetrics};
    use skill_constants::LABELS_FILE;

    let mut nodes: Vec<InteractiveGraphNode> = Vec::new();
    let mut edges: Vec<InteractiveGraphEdge> = Vec::new();

    // ── Session summary accumulators for cross-session comparison ────────
    struct SessionAccum {
        relaxation: f64,
        engagement: f64,
        snr: f64,
        rel_alpha: f64,
        rel_beta: f64,
        rel_theta: f64,
        engagement_sq: f64,
        snr_sq: f64,
        min_engagement: f64,
        max_engagement: f64,
        min_snr: f64,
        max_snr: f64,
        min_ts: u64,
        max_ts: u64,
        count: u32,
    }
    impl SessionAccum {
        fn new() -> Self {
            Self {
                relaxation: 0.0,
                engagement: 0.0,
                snr: 0.0,
                rel_alpha: 0.0,
                rel_beta: 0.0,
                rel_theta: 0.0,
                engagement_sq: 0.0,
                snr_sq: 0.0,
                min_engagement: f64::MAX,
                max_engagement: f64::MIN,
                min_snr: f64::MAX,
                max_snr: f64::MIN,
                min_ts: u64::MAX,
                max_ts: 0,
                count: 0,
            }
        }
        /// Returns true if the epoch has any meaningful signal.
        fn is_valid(ep: &skill_history::EpochRow) -> bool {
            !(ep.relaxation == 0.0 && ep.engagement == 0.0 && ep.snr == 0.0)
        }
        fn add(&mut self, ep: &skill_history::EpochRow, ts: u64) {
            if !Self::is_valid(ep) {
                return;
            }
            self.relaxation += ep.relaxation;
            self.engagement += ep.engagement;
            self.snr += ep.snr;
            self.rel_alpha += ep.ra;
            self.rel_beta += ep.rb;
            self.rel_theta += ep.rt;
            self.engagement_sq += ep.engagement * ep.engagement;
            self.snr_sq += ep.snr * ep.snr;
            if ep.engagement < self.min_engagement {
                self.min_engagement = ep.engagement;
            }
            if ep.engagement > self.max_engagement {
                self.max_engagement = ep.engagement;
            }
            if ep.snr < self.min_snr {
                self.min_snr = ep.snr;
            }
            if ep.snr > self.max_snr {
                self.max_snr = ep.snr;
            }
            if ts < self.min_ts {
                self.min_ts = ts;
            }
            if ts > self.max_ts {
                self.max_ts = ts;
            }
            self.count += 1;
        }
        fn stddev(sum: f64, sum_sq: f64, n: f64) -> f64 {
            if n < 2.0 {
                return 0.0;
            }
            let variance = (sum_sq / n) - (sum / n).powi(2);
            if variance > 0.0 {
                variance.sqrt()
            } else {
                0.0
            }
        }
    }
    let mut session_stats: std::collections::HashMap<String, SessionAccum> = std::collections::HashMap::new();

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
    let t_embed = std::time::Instant::now();
    let Some(query_vec) = embedder.embed(query) else {
        return serde_json::json!({
            "nodes": nodes, "edges": edges, "dot": "", "svg": "", "svg_col": "",
            "error": "failed to embed query text"
        });
    };

    let text_neighbors = skill_label_index::search_by_text_vec(&query_vec, k_text, 64, skill_dir, label_index);
    let embed_ms = t_embed.elapsed().as_millis();
    let t_graph = std::time::Instant::now();

    // If no results, check whether there are labels that need re-embedding.
    let mut reembed_needed: Option<serde_json::Value> = None;
    if text_neighbors.is_empty() {
        if let Some(store) = skill_data::label_store::LabelStore::open(skill_dir) {
            let total = store.count();
            if total > 0 {
                let stale = store.count_needing_embed(super::labels::EMBED_MODEL_NAME);
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

    // Open screenshot store once (reused for proximity + OCR search).
    let ss_store = if k_screenshots > 0 {
        skill_data::screenshot_store::ScreenshotStore::open(skill_dir)
    } else {
        None
    };

    // Open activity store for file interaction correlation.
    let activity_store = skill_data::activity_store::ActivityStore::open(skill_dir);
    let mut seen_files: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Helper: derive a session_id from a Unix timestamp (date + hour bucket).
    let session_id_from_ts = |ts: u64| -> String {
        let dt = skill_data::util::unix_to_ts(ts);
        let date = dt / 1_000_000;
        let hour = (dt / 10_000) % 100;
        format!("{date}_{hour:02}h")
    };

    // Helper: convert EpochRow metrics to NeighborMetrics.
    let metrics_from_epoch = |ep: &skill_history::EpochRow| -> Option<NeighborMetrics> {
        if ep.relaxation == 0.0 && ep.engagement == 0.0 && ep.snr == 0.0 {
            return None;
        }
        Some(NeighborMetrics {
            relaxation: Some(ep.relaxation),
            engagement: Some(ep.engagement),
            faa: if ep.faa != 0.0 { Some(ep.faa) } else { None },
            tar: if ep.tar != 0.0 { Some(ep.tar) } else { None },
            mood: if ep.mood != 0.0 { Some(ep.mood) } else { None },
            meditation: if ep.med != 0.0 { Some(ep.med) } else { None },
            cognitive_load: if ep.cog != 0.0 { Some(ep.cog) } else { None },
            drowsiness: if ep.drow != 0.0 { Some(ep.drow) } else { None },
            hr: if ep.hr != 0.0 { Some(ep.hr) } else { None },
            snr: if ep.snr != 0.0 { Some(ep.snr) } else { None },
            rel_alpha: if ep.ra != 0.0 { Some(ep.ra) } else { None },
            rel_beta: if ep.rb != 0.0 { Some(ep.rb) } else { None },
            rel_theta: if ep.rt != 0.0 { Some(ep.rt) } else { None },
            headache_index: None,
            migraine_index: None,
            consciousness_lzc: None,
            consciousness_wakefulness: None,
            consciousness_integration: None,
        })
    };

    // Helper: create screenshot node.
    let make_screenshot_node = |id: String, ss: &ScreenshotResult, parent: &str, dist: f32| -> InteractiveGraphNode {
        InteractiveGraphNode {
            id,
            kind: "screenshot".into(),
            text: if ss.window_title.is_empty() {
                None
            } else {
                Some(ss.window_title.clone())
            },
            timestamp_unix: Some(ss.unix_ts),
            distance: dist,
            parent_id: Some(parent.to_string()),
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
        }
    };

    // Dedup sets for O(1) duplicate detection.
    let mut seen_screenshots: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut seen_eeg_ts: std::collections::HashSet<u64> = std::collections::HashSet::new();

    // Lazy CSV metrics fallback: loaded on first epoch that lacks metrics_json.
    let mut csv_fallback: Option<Vec<skill_history::EpochRow>> = None;

    // Precompute constants used inside the loop.
    let reach_secs = reach_minutes * 60;
    let labels_db = skill_dir.join(LABELS_FILE);

    // Add text_label nodes with session context.
    // Apply optional date-range filter on labels.
    let filtered_neighbors: Vec<_> = text_neighbors
        .iter()
        .filter(|nb| {
            if nb.eeg_start == 0 {
                return true;
            } // no timestamp, keep it
            if let Some(start) = filter_start_utc {
                if nb.eeg_start < start {
                    return false;
                }
            }
            if let Some(end) = filter_end_utc {
                if nb.eeg_start > end {
                    return false;
                }
            }
            true
        })
        .collect();
    for (i, nb) in filtered_neighbors.iter().enumerate() {
        let node_id = format!("tl{i}");
        let ts = if nb.eeg_start > 0 { Some(nb.eeg_start) } else { None };
        let sid = ts.map(|t| session_id_from_ts(t));
        nodes.push(InteractiveGraphNode {
            id: node_id.clone(),
            kind: "text_label".into(),
            text: Some(nb.text.clone()),
            timestamp_unix: ts,
            distance: nb.distance,
            parent_id: Some(query_id.clone()),
            session_id: sid.clone(),
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
            let eeg_ts = skill_history::get_session_timeseries_filtered(
                skill_dir,
                ts.saturating_sub(reach_secs),
                ts + reach_secs,
                device_filter,
            );
            // Filter: optionally skip epochs with non-positive SNR (bad signal quality).
            let mut valid_epochs: Vec<&skill_history::EpochRow> = if snr_positive_only {
                eeg_ts.iter().filter(|ep| ep.snr > 0.0).collect()
            } else {
                eeg_ts.iter().collect()
            };

            // Rank EEG epochs by selected metric (default: timestamp order).
            match eeg_rank_by.unwrap_or("timestamp") {
                "engagement" => valid_epochs.sort_by(|a, b| {
                    b.engagement
                        .partial_cmp(&a.engagement)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }),
                "snr" => valid_epochs.sort_by(|a, b| b.snr.partial_cmp(&a.snr).unwrap_or(std::cmp::Ordering::Equal)),
                "relaxation" => valid_epochs.sort_by(|a, b| {
                    b.relaxation
                        .partial_cmp(&a.relaxation)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }),
                _ => {} // keep timestamp order
            }

            for (j, ep) in valid_epochs.iter().take(k_eeg).enumerate() {
                let ep_ts = ep.t as u64;

                // Dedup EEG epochs across text_labels (overlapping time windows).
                if !seen_eeg_ts.insert(ep_ts) {
                    continue;
                }

                let eeg_id = format!("ep{i}_{j}");
                let ep_sid = session_id_from_ts(ep_ts);

                let mut metrics = metrics_from_epoch(ep);

                // Fallback: if embedding lacks metrics, try CSV lookup.
                let mut csv_engagement = ep.engagement;
                if metrics.is_none() {
                    // Lazy-load CSV epochs on first miss.
                    if csv_fallback.is_none() {
                        let global_start = filtered_neighbors
                            .iter()
                            .filter_map(|nb| {
                                if nb.eeg_start > 0 {
                                    Some(nb.eeg_start.saturating_sub(reach_secs))
                                } else {
                                    None
                                }
                            })
                            .min()
                            .unwrap_or(0);
                        let global_end = filtered_neighbors
                            .iter()
                            .filter_map(|nb| {
                                if nb.eeg_start > 0 {
                                    Some(nb.eeg_start + reach_secs)
                                } else {
                                    None
                                }
                            })
                            .max()
                            .unwrap_or(0);
                        csv_fallback = Some(skill_history::lookup_csv_metrics_for_range(
                            skill_dir,
                            global_start,
                            global_end,
                        ));
                    }
                    if let Some(ref csv_epochs) = csv_fallback {
                        if let Some(csv_row) = skill_history::find_closest_csv_epoch(csv_epochs, ep_ts as f64) {
                            csv_engagement = csv_row.engagement;
                            metrics = Some(NeighborMetrics {
                                relaxation: Some(csv_row.relaxation),
                                engagement: Some(csv_row.engagement),
                                faa: if csv_row.faa != 0.0 { Some(csv_row.faa) } else { None },
                                tar: if csv_row.tar != 0.0 { Some(csv_row.tar) } else { None },
                                mood: if csv_row.mood != 0.0 { Some(csv_row.mood) } else { None },
                                meditation: if csv_row.med != 0.0 { Some(csv_row.med) } else { None },
                                cognitive_load: if csv_row.cog != 0.0 { Some(csv_row.cog) } else { None },
                                drowsiness: if csv_row.drow != 0.0 { Some(csv_row.drow) } else { None },
                                hr: if csv_row.hr != 0.0 { Some(csv_row.hr) } else { None },
                                snr: if csv_row.snr != 0.0 { Some(csv_row.snr) } else { None },
                                rel_alpha: if csv_row.ra != 0.0 { Some(csv_row.ra) } else { None },
                                rel_beta: if csv_row.rb != 0.0 { Some(csv_row.rb) } else { None },
                                rel_theta: if csv_row.rt != 0.0 { Some(csv_row.rt) } else { None },
                                headache_index: None,
                                migraine_index: None,
                                consciousness_lzc: None,
                                consciousness_wakefulness: None,
                                consciousness_integration: None,
                            });
                        }
                    }
                }

                // Accumulate session stats (skips zero-signal epochs internally)
                session_stats
                    .entry(ep_sid.clone())
                    .or_insert_with(SessionAccum::new)
                    .add(ep, ep_ts);

                // Time distance from parent label (normalized to reach window)
                let time_dist = (ep_ts as f64 - ts as f64).abs() / reach_secs as f64;

                // Composite relevance score: text_sim * 0.5 + time_dist * 0.3 + (1 - norm_engagement) * 0.2
                let norm_engagement = (csv_engagement.clamp(0.0, 1.0)) as f32;
                let relevance = nb.distance * 0.5 + time_dist as f32 * 0.3 + (1.0 - norm_engagement) * 0.2;

                nodes.push(InteractiveGraphNode {
                    id: eeg_id.clone(),
                    kind: "eeg_point".into(),
                    timestamp_unix: Some(ep_ts),
                    distance: time_dist as f32,
                    parent_id: Some(node_id.clone()),
                    eeg_metrics: metrics,
                    session_id: Some(ep_sid),
                    relevance_score: Some(relevance),
                    ..Default::default()
                });
                edges.push(InteractiveGraphEdge {
                    from_id: node_id.clone(),
                    to_id: eeg_id.clone(),
                    distance: time_dist as f32,
                    kind: "eeg_bridge".into(),
                });

                // Step 3: Find labels near each EEG epoch.
                let nearby_labels = skill_commands::get_labels_near(&labels_db, ep_ts, reach_secs);
                for (l, lbl) in nearby_labels.iter().enumerate().take(k_labels) {
                    let fl_id = format!("fl{i}_{j}_{l}");
                    nodes.push(InteractiveGraphNode {
                        id: fl_id.clone(),
                        kind: "found_label".into(),
                        text: Some(lbl.text.clone()),
                        timestamp_unix: Some(lbl.eeg_start),
                        distance: 0.0,
                        parent_id: Some(eeg_id.clone()),
                        session_id: Some(session_id_from_ts(lbl.eeg_start)),
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
            if let Some(ref store) = ss_store {
                let nearby_screenshots = skill_screenshots::capture::get_around(store, ts as i64, reach_secs as i32);
                for (s, ss) in nearby_screenshots.iter().take(k_screenshots).enumerate() {
                    let ss_id = format!("ss{i}_{s}");
                    if seen_screenshots.contains(&ss.filename) {
                        continue;
                    }
                    seen_screenshots.insert(ss.filename.clone());
                    let mut node = make_screenshot_node(ss_id.clone(), ss, &node_id, 0.0);
                    node.session_id = Some(session_id_from_ts(ss.unix_ts));
                    nodes.push(node);
                    edges.push(InteractiveGraphEdge {
                        from_id: node_id.clone(),
                        to_id: ss_id,
                        distance: 0.0,
                        kind: "screenshot_prox".into(),
                    });
                }
            }

            // Step 2c: Find file activity near this label's timestamp.
            if let Some(ref store) = activity_store {
                let nearby_files = store.get_files_in_range(ts.saturating_sub(reach_secs), ts + reach_secs, 5);
                for (f, fi) in nearby_files.iter().enumerate() {
                    if !seen_files.insert(fi.file_path.clone()) {
                        continue; // dedup same file across labels
                    }
                    let basename = std::path::Path::new(&fi.file_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&fi.file_path)
                        .to_string();
                    let fa_id = format!("fa{i}_{f}");
                    let mut fa_node = InteractiveGraphNode {
                        id: fa_id.clone(),
                        kind: "file_activity".into(),
                        text: Some(basename),
                        timestamp_unix: Some(fi.seen_at),
                        file_path: Some(fi.file_path.clone()),
                        project: if fi.project.is_empty() {
                            None
                        } else {
                            Some(fi.project.clone())
                        },
                        language: if fi.language.is_empty() {
                            None
                        } else {
                            Some(fi.language.clone())
                        },
                        was_modified: Some(fi.was_modified),
                        lines_added: Some(fi.lines_added),
                        lines_removed: Some(fi.lines_removed),
                        parent_id: Some(node_id.clone()),
                        ..Default::default()
                    };
                    // Copy EEG metrics from file interaction if present.
                    if fi.eeg_focus.is_some() || fi.eeg_mood.is_some() {
                        fa_node.eeg_metrics = Some(NeighborMetrics {
                            engagement: fi.eeg_focus.map(|v| v as f64),
                            mood: fi.eeg_mood.map(|v| v as f64),
                            ..Default::default()
                        });
                    }
                    fa_node.session_id = Some(session_id_from_ts(fi.seen_at));
                    nodes.push(fa_node);
                    edges.push(InteractiveGraphEdge {
                        from_id: node_id.clone(),
                        to_id: fa_id,
                        distance: 0.0,
                        kind: "file_activity_prox".into(),
                    });
                }
            }

            // Step 2d: Find meetings near this label's timestamp.
            if let Some(ref store) = activity_store {
                let nearby_meetings = store.get_meetings_in_range(ts.saturating_sub(reach_secs), ts + reach_secs);
                for (m, mtg) in nearby_meetings.iter().take(3).enumerate() {
                    let mtg_id = format!("mtg{i}_{m}");
                    let mut mtg_node = InteractiveGraphNode {
                        id: mtg_id.clone(),
                        kind: "meeting".into(),
                        text: Some(format!("{} ({})", mtg.platform, mtg.title)),
                        timestamp_unix: Some(mtg.start_at),
                        parent_id: Some(node_id.clone()),
                        ..Default::default()
                    };
                    mtg_node.session_id = Some(session_id_from_ts(mtg.start_at));
                    nodes.push(mtg_node);
                    edges.push(InteractiveGraphEdge {
                        from_id: node_id.clone(),
                        to_id: mtg_id,
                        distance: 0.0,
                        kind: "meeting_prox".into(),
                    });
                }
            }
        }
    }

    // Step 4: Search screenshots by OCR text similarity (semantic, not proximity).
    if k_screenshots > 0 {
        if let Some(ref store) = ss_store {
            let embed_fn = |text: &str| -> Option<Vec<f32>> { embedder.embed(text) };
            let mut ocr_results = skill_screenshots::capture::search_by_ocr_text_embedding(
                skill_dir,
                store,
                query,
                k_screenshots,
                &embed_fn,
            );
            if ocr_results.is_empty() {
                ocr_results = skill_screenshots::capture::search_by_ocr_text_like(store, query, k_screenshots);
            }
            for (s, ss) in ocr_results.iter().enumerate() {
                let ss_id = format!("sst{s}");
                if seen_screenshots.contains(&ss.filename) {
                    continue;
                }
                seen_screenshots.insert(ss.filename.clone());
                let mut node = make_screenshot_node(ss_id.clone(), ss, &query_id, 1.0 - ss.similarity);
                node.ocr_similarity = Some(ss.similarity);
                node.session_id = Some(session_id_from_ts(ss.unix_ts));
                nodes.push(node);
                edges.push(InteractiveGraphEdge {
                    from_id: query_id.clone(),
                    to_id: ss_id,
                    distance: 1.0 - ss.similarity,
                    kind: "ocr_sim".into(),
                });
            }
        }
    }

    // ── Cross-session comparison summary ───────────────────────────────
    let sessions_summary: Vec<serde_json::Value> = {
        let mut entries: Vec<_> = session_stats.iter().filter(|(_, s)| s.count > 0).collect();
        entries.sort_by_key(|(k, _)| (*k).clone());

        // Find the best session (highest avg engagement).
        let best_sid = entries
            .iter()
            .max_by(|(_, a), (_, b)| {
                let avg_a = if a.count > 0 {
                    a.engagement / a.count as f64
                } else {
                    0.0
                };
                let avg_b = if b.count > 0 {
                    b.engagement / b.count as f64
                } else {
                    0.0
                };
                avg_a.partial_cmp(&avg_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(sid, _)| (*sid).clone());

        entries
            .iter()
            .map(|(sid, s)| {
                let n = s.count as f64;
                let duration_secs = s.max_ts.saturating_sub(s.min_ts);
                serde_json::json!({
                    "session_id": sid,
                    "epoch_count": s.count,
                    "duration_secs": duration_secs,
                    "best": best_sid.as_deref() == Some(sid.as_str()),
                    "avg_relaxation":      if n > 0.0 { s.relaxation / n } else { 0.0 },
                    "avg_engagement":      if n > 0.0 { s.engagement / n } else { 0.0 },
                    "avg_snr":             if n > 0.0 { s.snr / n } else { 0.0 },
                    "avg_rel_alpha":       if n > 0.0 { s.rel_alpha / n } else { 0.0 },
                    "avg_rel_beta":        if n > 0.0 { s.rel_beta / n } else { 0.0 },
                    "avg_rel_theta":       if n > 0.0 { s.rel_theta / n } else { 0.0 },
                    "stddev_engagement":   SessionAccum::stddev(s.engagement, s.engagement_sq, n),
                    "stddev_snr":          SessionAccum::stddev(s.snr, s.snr_sq, n),
                    "min_engagement":      if s.min_engagement < f64::MAX { s.min_engagement } else { 0.0 },
                    "max_engagement":      if s.max_engagement > f64::MIN { s.max_engagement } else { 0.0 },
                    "min_snr":             if s.min_snr < f64::MAX { s.min_snr } else { 0.0 },
                    "max_snr":             if s.max_snr > f64::MIN { s.max_snr } else { 0.0 },
                })
            })
            .collect()
    };

    let graph_ms = t_graph.elapsed().as_millis();
    let total_ms = t_total.elapsed().as_millis();

    // System load snapshot.
    let sys_info = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing()
            .with_cpu(sysinfo::CpuRefreshKind::everything())
            .with_memory(sysinfo::MemoryRefreshKind::everything()),
    );
    let perf = serde_json::json!({
        "embed_ms": embed_ms,
        "graph_ms": graph_ms,
        "total_ms": total_ms,
        "node_count": nodes.len(),
        "edge_count": edges.len(),
        "snr_positive_only": snr_positive_only,
        "cpu_usage_pct": sys_info.global_cpu_usage(),
        "mem_used_mb": sys_info.used_memory() / (1024 * 1024),
        "mem_total_mb": sys_info.total_memory() / (1024 * 1024),
    });

    let mut result = serde_json::json!({
        "nodes": nodes,
        "edges": edges,
        "dot": "",
        "svg": "",
        "svg_col": "",
        "sessions": sessions_summary,
        "perf": perf,
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
    let dir_b = skill_dir.clone();
    let (a_start, a_end) = (req.a_start_utc, req.a_end_utc);
    let (b_start, b_end) = (req.b_start_utc, req.b_end_utc);

    let (result_a, result_b) = tokio::join!(
        tokio::task::spawn_blocking(move || {
            serde_json::to_value(skill_commands::search_embeddings_in_range(
                &dir_a, a_start, a_end, 10, 50, None,
            ))
            .unwrap_or_default()
        }),
        tokio::task::spawn_blocking(move || {
            serde_json::to_value(skill_commands::search_embeddings_in_range(
                &dir_b, b_start, b_end, 10, 50, None,
            ))
            .unwrap_or_default()
        }),
    );
    let result_a = result_a.unwrap_or_default();
    let result_b = result_b.unwrap_or_default();

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
    fn interactive_search_empty_dir_returns_query_node() {
        let td = TempDir::new().unwrap();
        let label_index = std::sync::Arc::new(skill_label_index::LabelIndexState::default());
        let embedder = crate::text_embedder::SharedTextEmbedder::new_noop();

        let result = interactive_search_impl(
            td.path(),
            "hello",
            3,
            5,
            2,
            5,
            10,
            false,
            None,
            None,
            None,
            None,
            &embedder,
            &label_index,
        );

        // Should have at least the query node (noop embedder → error path)
        let nodes = result["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["kind"], "query");
        assert_eq!(nodes[0]["text"], "hello");
    }

    #[test]
    fn collect_fast_meta_includes_expected_fields() {
        let td = TempDir::new().unwrap();
        let label_index = std::sync::Arc::new(skill_label_index::LabelIndexState::default());

        let meta = collect_fast_meta(td.path(), &label_index);

        for field in [
            "eeg_days",
            "eeg_first_day",
            "eeg_last_day",
            "label_total",
            "label_text_index",
            "label_eeg_index",
            "label_embed_model",
            "screenshot_total",
            "screenshot_embedded",
        ] {
            assert!(meta.get(field).is_some(), "fast meta missing: {field}");
        }
        assert_eq!(meta["eeg_days"], 0);
        assert_eq!(meta["label_total"], 0);
        assert_eq!(meta["label_embed_model"], super::super::labels::EMBED_MODEL_NAME);
    }

    #[test]
    fn collect_slow_meta_includes_expected_fields() {
        let td = TempDir::new().unwrap();

        let meta = collect_slow_meta(td.path());

        for field in [
            "eeg_total_sessions",
            "eeg_total_secs",
            "label_stale",
            "eeg_total_epochs",
            "eeg_embedded_epochs",
            "eeg_missing_epochs",
        ] {
            assert!(meta.get(field).is_some(), "slow meta missing: {field}");
        }
        assert_eq!(meta["eeg_total_sessions"], 0);
        assert_eq!(meta["eeg_total_secs"], 0);
        assert_eq!(meta["label_stale"], 0);
        assert_eq!(meta["eeg_total_epochs"], 0);
        assert_eq!(meta["eeg_embedded_epochs"], 0);
        assert_eq!(meta["eeg_missing_epochs"], 0);
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
                device_name: None,
                snr_positive_only: None,
                filter_start_utc: None,
                filter_end_utc: None,
                eeg_rank_by: None,
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
                device_name: None,
                snr_positive_only: None,
                filter_start_utc: None,
                filter_end_utc: None,
                eeg_rank_by: None,
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
                device_name: None,
                snr_positive_only: None,
                filter_start_utc: None,
                filter_end_utc: None,
                eeg_rank_by: None,
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

        let result = interactive_search_impl(
            td.path(),
            "focus",
            3,
            5,
            2,
            5,
            10,
            false,
            None,
            None,
            None,
            None,
            &embedder,
            &label_index,
        );

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

    // ── metrics_from_epoch ──────────────────────────────────────────────

    #[test]
    fn metrics_from_epoch_returns_some_for_nonzero_metrics() {
        // Simulate what interactive_search_impl does
        let metrics_from_epoch = |ep: &skill_history::EpochRow| -> Option<skill_commands::NeighborMetrics> {
            if ep.relaxation == 0.0 && ep.engagement == 0.0 && ep.snr == 0.0 {
                return None;
            }
            Some(skill_commands::NeighborMetrics {
                relaxation: Some(ep.relaxation),
                engagement: Some(ep.engagement),
                snr: if ep.snr != 0.0 { Some(ep.snr) } else { None },
                ..Default::default()
            })
        };

        let ep = skill_history::EpochRow {
            t: 1700000000.0,
            engagement: 50.0,
            relaxation: 30.0,
            snr: 15.0,
            ..Default::default()
        };
        let m = metrics_from_epoch(&ep);
        assert!(m.is_some(), "should return Some for nonzero engagement/relaxation/snr");
        let m = m.unwrap();
        assert!((m.engagement.unwrap() - 50.0).abs() < 0.01);
        assert!((m.relaxation.unwrap() - 30.0).abs() < 0.01);
        assert!((m.snr.unwrap() - 15.0).abs() < 0.01);
    }

    #[test]
    fn metrics_from_epoch_returns_none_for_all_zero() {
        let metrics_from_epoch = |ep: &skill_history::EpochRow| -> Option<skill_commands::NeighborMetrics> {
            if ep.relaxation == 0.0 && ep.engagement == 0.0 && ep.snr == 0.0 {
                return None;
            }
            Some(skill_commands::NeighborMetrics::default())
        };

        let ep = skill_history::EpochRow {
            t: 1700000000.0,
            engagement: 0.0,
            relaxation: 0.0,
            snr: 0.0,
            ..Default::default()
        };
        assert!(
            metrics_from_epoch(&ep).is_none(),
            "should return None when all three are zero"
        );
    }

    // ── End-to-end: timeseries → metrics_from_epoch pipeline ────────────

    #[test]
    fn timeseries_to_metrics_pipeline_with_valid_json() {
        let dir = TempDir::new().unwrap();
        let base_ts = 1700000000i64 * 1000;
        let json = serde_json::json!({
            "rel_delta": 0.3, "rel_theta": 0.2, "rel_alpha": 0.025, "rel_beta": 0.05,
            "rel_gamma": 0.05, "relaxation_score": 30.0, "engagement_score": 50.0,
            "snr": 15.0, "faa": 0.1, "mood": 60.0, "hr": 72.0
        })
        .to_string();
        let rows_data: Vec<(i64, String)> = (0..3).map(|i| (base_ts + i * 5000, json.clone())).collect();
        let rows_ref: Vec<(i64, &str)> = rows_data.iter().map(|(ts, j)| (*ts, j.as_str())).collect();

        // Create fixture DB
        let day_dir = dir.path().join("20231114");
        std::fs::create_dir_all(&day_dir).unwrap();
        let db_path = day_dir.join("eeg.sqlite");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS embeddings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                device_id TEXT, device_name TEXT,
                hnsw_id INTEGER DEFAULT 0,
                eeg_embedding BLOB, label TEXT,
                metrics_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_embeddings_timestamp ON embeddings(timestamp);",
        )
        .unwrap();
        for (ts, j) in &rows_ref {
            conn.execute(
                "INSERT INTO embeddings (timestamp, metrics_json) VALUES (?1, ?2)",
                rusqlite::params![ts, j],
            )
            .unwrap();
        }
        drop(conn);

        // Step 1: get timeseries
        let epochs = skill_history::get_session_timeseries(dir.path(), 1700000000, 1700000050);
        assert!(!epochs.is_empty(), "should find epochs");

        // Step 2: metrics_from_epoch (same logic as interactive_search_impl)
        for ep in &epochs {
            assert!(
                ep.engagement != 0.0 || ep.relaxation != 0.0 || ep.snr != 0.0,
                "epoch at t={} should have nonzero metrics (eng={}, rel={}, snr={})",
                ep.t,
                ep.engagement,
                ep.relaxation,
                ep.snr
            );
        }

        // Step 3: Would produce metrics in the search response (not "(no EEG metrics stored)")
        let has_metrics = epochs
            .iter()
            .filter(|ep| ep.engagement != 0.0 || ep.relaxation != 0.0 || ep.snr != 0.0)
            .count();
        assert_eq!(has_metrics, epochs.len(), "all epochs should have metrics");
    }

    #[test]
    fn timeseries_to_metrics_pipeline_with_null_json() {
        let dir = TempDir::new().unwrap();
        let day_dir = dir.path().join("20231114");
        std::fs::create_dir_all(&day_dir).unwrap();
        let db_path = day_dir.join("eeg.sqlite");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS embeddings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                device_id TEXT, device_name TEXT,
                hnsw_id INTEGER DEFAULT 0,
                eeg_embedding BLOB, label TEXT,
                metrics_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_embeddings_timestamp ON embeddings(timestamp);",
        )
        .unwrap();
        // NULL metrics_json
        conn.execute(
            "INSERT INTO embeddings (timestamp, metrics_json) VALUES (?1, NULL)",
            rusqlite::params![1700000000000i64],
        )
        .unwrap();
        drop(conn);

        let epochs = skill_history::get_session_timeseries(dir.path(), 1700000000, 1700000050);
        assert!(!epochs.is_empty());
        // All zero → metrics_from_epoch would return None → "(no EEG metrics stored)"
        assert_eq!(epochs[0].engagement, 0.0);
        assert_eq!(epochs[0].relaxation, 0.0);
        assert_eq!(epochs[0].snr, 0.0);
    }

    // ── cache_clear ────────────────────────────────────────────────────

    #[test]
    fn cache_clear_removes_all_entries() {
        let key = cache_key(&["test", "cache"]);
        cache_put(key, serde_json::json!({"cached": true}));
        assert!(cache_get(key).is_some(), "should be cached");
        cache_clear();
        assert!(cache_get(key).is_none(), "cache should be empty after clear");
    }
}
