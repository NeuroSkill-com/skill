// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Search commands — timestamp-range EEG embedding search with label hydration.
//!
//! ## Flow
//!
//! ```text
//! search_embeddings(start_utc, end_utc, k, ef)
//!     │
//!     ├─ scan ~/.skill/YYYYMMDD/ dirs
//!     │       load every eeg_embeddings.hnsw (read-only)
//!     │
//!     ├─ for each day that overlaps [start_utc, end_utc]:
//!     │       query eeg.sqlite → (timestamp, hnsw_id, embedding BLOB)
//!     │
//!     ├─ for each query embedding:
//!     │       search ALL loaded HNSW indices → Vec<(hnsw_id, timestamp, distance)>
//!     │       keep top-k by distance
//!     │
//!     └─ for each neighbor:
//!             lookup device info from the matching day's eeg.sqlite (by hnsw_id)
//!             lookup labels.sqlite WHERE eeg_start ≤ neighbor_unix ≤ eeg_end
//! ```
//!
//! All SQLite connections are opened **read-only** so they can run concurrently
//! with the embed-worker's write transactions.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fast_hnsw::{distance::Cosine, labeled::LabeledIndex};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use skill_constants::{hnsw_index_file_for, HNSW_INDEX_FILE, LABELS_FILE, SQLITE_FILE};

pub mod graph;
pub use graph::{dot_edge_label, dot_esc, dot_node_label, generate_dot, generate_svg, generate_svg_3d, SvgLabels};

// Re-export shared utilities so downstream crates keep compiling.
pub use skill_data::util::{fmt_unix_utc, ts_to_unix, unix_to_ts, MutexExt};

/// Shared, optionally-ready global HNSW index.
///
/// The outer `Option` lets callers pass `None` when no global index is
/// available (e.g. WebSocket path before the startup build finishes).
/// The inner `Option` is `None` while the background build thread is still
/// running and `Some` once the index is ready.
pub type GlobalIndexHandle = Option<Arc<Mutex<Option<LabeledIndex<Cosine, i64>>>>>;

// Timestamp helpers are re-exported from skill_data::util above.

// ── Result types (all Serialize so Tauri returns them as JSON) ────────────────

/// A user label whose EEG window overlaps a found embedding.
#[derive(Debug, Serialize, Clone)]
/// A label with its text, context, and time range from the label store.
pub struct LabelEntry {
    pub id: i64,
    /// Unix-second start of the EEG window captured during labelling.
    pub eeg_start: u64,
    /// Unix-second end of the EEG window.
    pub eeg_end: u64,
    pub label_start: u64,
    pub label_end: u64,
    /// Free-text label entered by the user.
    pub text: String,
}

/// Compact EEG metrics attached to a search neighbor.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
/// Per-epoch EEG band-power metrics attached to a search neighbor.
pub struct NeighborMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relaxation: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engagement: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub faa: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tar: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mood: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meditation: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cognitive_load: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drowsiness: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hr: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snr: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel_alpha: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel_beta: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel_theta: Option<f64>,
    // Headache / Migraine correlate indices
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headache_index: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migraine_index: Option<f64>,
    // Consciousness metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consciousness_lzc: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consciousness_wakefulness: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consciousness_integration: Option<f64>,
}

/// One embedding found by the HNSW nearest-neighbour search.
#[derive(Debug, Serialize, Clone)]
/// A single EEG embedding neighbor found by HNSW search.
pub struct NeighborEntry {
    /// Zero-based insertion id within the day's HNSW index.
    pub hnsw_id: usize,
    /// `YYYYMMDDHHmmss` UTC timestamp stored in the index payload.
    pub timestamp: i64,
    /// Same timestamp converted to Unix seconds (for JS `Date` construction).
    pub timestamp_unix: u64,
    /// Cosine distance from the query embedding (0 = identical).
    pub distance: f32,
    /// Which YYYYMMDD index this neighbor came from.
    pub date: String,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    /// Labels whose EEG window contains this embedding's timestamp.
    pub labels: Vec<LabelEntry>,
    /// Key EEG metrics for this epoch (if available in SQLite).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<NeighborMetrics>,
}

/// Results for one query embedding.
#[derive(Debug, Serialize, Clone)]
/// One query epoch with its K nearest neighbors from the HNSW search.
pub struct QueryEntry {
    pub timestamp: i64,
    pub timestamp_unix: u64,
    pub neighbors: Vec<NeighborEntry>,
}

/// Top-level result returned by [`search_embeddings_in_range`].
#[derive(Debug, Serialize)]
/// Complete search result: all query epochs, their neighbors, and timing.
pub struct SearchResult {
    pub start_utc: u64,
    pub end_utc: u64,
    pub k: usize,
    pub ef: usize,
    /// Total query embeddings found in the input range.
    pub query_count: usize,
    /// YYYYMMDD strings of every day whose index was searched.
    pub searched_days: Vec<String>,
    pub results: Vec<QueryEntry>,
}

/// Progress event streamed by streaming search.
#[derive(Debug, Serialize, Clone)]
/// Progress update during a streaming EEG search.
pub struct SearchProgress {
    /// Kind: "started" | "result" | "done" | "error"
    pub kind: String,
    /// Filled for "started"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub searched_days: Option<Vec<String>>,
    /// Filled for "result": one QueryEntry's worth of data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<QueryEntry>,
    /// Filled for "done"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
    /// Filled for "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// How many results have been emitted so far (for progress bar)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_count: Option<usize>,
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// A loaded daily HNSW index with its date and embedding count.
pub struct DayIndex {
    pub date: String,
    pub dir: PathBuf,
    pub index: LabeledIndex<Cosine, i64>,
}

/// List all valid `YYYYMMDD` sub-directories under `skill_dir`.
///
/// Delegates to [`skill_data::util::date_dirs`].
#[inline]
pub fn list_date_dirs(skill_dir: &Path) -> Vec<(String, PathBuf)> {
    skill_data::util::date_dirs(skill_dir)
}

/// Load a `LabeledIndex<Cosine, i64>` from a date directory (read-only mmap).
///
/// Uses the default HNSW file (`eeg_embeddings.hnsw` for ZUNA).
pub fn load_day_index(date: String, dir: PathBuf) -> Option<DayIndex> {
    load_day_index_for(date, dir, "zuna")
}

/// Load the model-specific HNSW index for a date directory.
pub fn load_day_index_for(date: String, dir: PathBuf, model_backend: &str) -> Option<DayIndex> {
    let filename = hnsw_index_file_for(model_backend);
    let path = dir.join(&filename);
    // Fall back to legacy filename for old data.
    let path = if !path.exists() && model_backend == "zuna" {
        dir.join(HNSW_INDEX_FILE)
    } else {
        path
    };
    if !path.exists() {
        return None;
    }
    match LabeledIndex::load_mmap(&path, Cosine) {
        Ok(idx) => {
            eprintln!(
                "[search] loaded HNSW {} ({} vecs, model={})",
                date,
                idx.len(),
                model_backend
            );
            Some(DayIndex { date, dir, index: idx })
        }
        Err(e) => {
            eprintln!("[search] HNSW load {}: {e}", path.display());
            None
        }
    }
}

#[allow(dead_code)]
struct RawEmb {
    hnsw_id: i64,
    timestamp: i64,
    embedding: Vec<f32>,
}

/// Read every embedding in [start_ts, end_ts] from a single day's SQLite.
fn read_embeddings_in_range(db_path: &Path, start_ts: i64, end_ts: i64) -> Vec<RawEmb> {
    let conn = match skill_data::util::open_readonly(db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[search] open {}: {e}", db_path.display());
            return vec![];
        }
    };

    let mut stmt = match conn.prepare(
        "SELECT hnsw_id, timestamp, eeg_embedding
         FROM embeddings
         WHERE timestamp BETWEEN ?1 AND ?2
         ORDER BY timestamp",
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[search] prepare: {e}");
            return vec![];
        }
    };

    stmt.query_map(params![start_ts, end_ts], |row| {
        let hnsw_id: i64 = row.get(0)?;
        let timestamp: i64 = row.get(1)?;
        let blob: Vec<u8> = row.get(2)?;
        let embedding = skill_data::util::blob_to_f32(&blob);
        Ok(RawEmb {
            hnsw_id,
            timestamp,
            embedding,
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

/// Derive the `YYYYMMDD` date string from a `YYYYMMDDHHmmss` timestamp integer.
fn date_from_ts(ts: i64) -> String {
    format!("{}", ts / 1_000_000)
}

/// Look up a row in `db_path` by its `YYYYMMDDHHmmss` timestamp.
/// Returns `(hnsw_id, device_id, device_name)`.
fn get_embedding_by_ts(db_path: &Path, timestamp: i64) -> (i64, Option<String>, Option<String>) {
    let Ok(conn) = skill_data::util::open_readonly(db_path) else {
        return (0, None, None);
    };

    conn.query_row(
        "SELECT hnsw_id, device_id, device_name \
         FROM embeddings WHERE timestamp = ?1 LIMIT 1",
        params![timestamp],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        },
    )
    .unwrap_or((0, None, None))
}

/// Fetch key EEG metrics for a row identified by its `YYYYMMDDHHmmss` timestamp.
fn get_embedding_metrics_by_ts(db_path: &Path, timestamp: i64) -> Option<NeighborMetrics> {
    let conn = skill_data::util::open_readonly(db_path).ok()?;

    conn.query_row(
        "SELECT json_extract(metrics_json, '$.relaxation_score'),
                json_extract(metrics_json, '$.engagement_score'),
                json_extract(metrics_json, '$.faa'),
                json_extract(metrics_json, '$.tar'),
                json_extract(metrics_json, '$.mood'),
                json_extract(metrics_json, '$.meditation'),
                json_extract(metrics_json, '$.cognitive_load'),
                json_extract(metrics_json, '$.drowsiness'),
                json_extract(metrics_json, '$.hr'),
                json_extract(metrics_json, '$.snr'),
                json_extract(metrics_json, '$.rel_alpha'),
                json_extract(metrics_json, '$.rel_beta'),
                json_extract(metrics_json, '$.rel_theta'),
                json_extract(metrics_json, '$.headache_index'),
                json_extract(metrics_json, '$.migraine_index'),
                json_extract(metrics_json, '$.consciousness_lzc'),
                json_extract(metrics_json, '$.consciousness_wakefulness'),
                json_extract(metrics_json, '$.consciousness_integration')
         FROM embeddings WHERE timestamp = ?1 LIMIT 1",
        params![timestamp],
        |row| {
            let g = |i: usize| -> Option<f64> { row.get::<_, Option<f64>>(i).unwrap_or(None) };
            Ok(NeighborMetrics {
                relaxation: g(0),
                engagement: g(1),
                faa: g(2),
                tar: g(3),
                mood: g(4),
                meditation: g(5),
                cognitive_load: g(6),
                drowsiness: g(7),
                hr: g(8),
                snr: g(9),
                rel_alpha: g(10),
                rel_beta: g(11),
                rel_theta: g(12),
                headache_index: g(13),
                migraine_index: g(14),
                consciousness_lzc: g(15),
                consciousness_wakefulness: g(16),
                consciousness_integration: g(17),
            })
        },
    )
    .ok()
}

/// Fetch all labels from `labels.sqlite` whose EEG window contains `ts_unix`.
pub fn get_labels_for(labels_db: &Path, ts_unix: u64) -> Vec<LabelEntry> {
    let Ok(conn) = skill_data::util::open_readonly(labels_db) else {
        return vec![];
    };

    let Ok(mut stmt) = conn.prepare(
        "SELECT id, eeg_start, eeg_end, label_start, label_end, text
         FROM labels
         WHERE eeg_start <= ?1 AND eeg_end >= ?1
         ORDER BY eeg_start",
    ) else {
        return vec![];
    };

    stmt.query_map(params![ts_unix as i64], |row| {
        Ok(LabelEntry {
            id: row.get(0)?,
            eeg_start: row.get::<_, i64>(1)? as u64,
            eeg_end: row.get::<_, i64>(2)? as u64,
            label_start: row.get::<_, i64>(3)? as u64,
            label_end: row.get::<_, i64>(4)? as u64,
            text: row.get(5)?,
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

// ── Core search ───────────────────────────────────────────────────────────────

/// Search for the `k` nearest EEG embeddings to every embedding recorded
/// between `start_utc` and `end_utc`, then hydrate each hit with any
/// overlapping user labels.
///
/// When `global_index` is `Some`, a single cross-day HNSW search is performed
/// against the persistent global index (better recall, lower latency).  When
/// `None`, all available per-day HNSW files are loaded and searched (fallback
/// used while the global index is still being built on startup).
#[allow(clippy::needless_pass_by_value)] // GlobalIndexHandle is Option<Arc<…>> — cheap clone, shared with callers
pub fn search_embeddings_in_range(
    skill_dir: &Path,
    start_utc: u64,
    end_utc: u64,
    k: usize,
    ef: usize,
    global_index: GlobalIndexHandle,
) -> SearchResult {
    search_embeddings_in_range_for(skill_dir, start_utc, end_utc, k, ef, global_index, "zuna")
}

/// Model-aware variant of [`search_embeddings_in_range`].
#[allow(clippy::needless_pass_by_value)]
pub fn search_embeddings_in_range_for(
    skill_dir: &Path,
    start_utc: u64,
    end_utc: u64,
    k: usize,
    ef: usize,
    global_index: GlobalIndexHandle,
    model_backend: &str,
) -> SearchResult {
    let start_ts = (start_utc as i64) * 1000;
    let end_ts = (end_utc as i64) * 1000;
    let labels_db = skill_dir.join(LABELS_FILE);
    let date_dirs = list_date_dirs(skill_dir);

    // ── Collect query embeddings from days that overlap [start_ts, end_ts] ────
    // Store index into `date_dirs` to avoid cloning String/PathBuf per embedding.
    let mut query_embs: Vec<(usize, RawEmb)> = Vec::new();
    for (dd_idx, (date, dir)) in date_dirs.iter().enumerate() {
        let db_path = dir.join(SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }
        let embs = read_embeddings_in_range(&db_path, start_ts, end_ts);
        if !embs.is_empty() {
            eprintln!("[search] {} query embs from {}", embs.len(), date);
        }
        for emb in embs {
            query_embs.push((dd_idx, emb));
        }
    }
    let query_count = query_embs.len();

    // ── Decide search backend ─────────────────────────────────────────────
    let global_guard = global_index.as_ref().map(|arc| arc.lock_or_recover());
    let global_idx: Option<&LabeledIndex<Cosine, i64>> = global_guard.as_deref().and_then(|opt| opt.as_ref());
    let global_ready = global_idx.map(|idx| !idx.is_empty()).unwrap_or(false);

    // Per-day indices — only loaded when the global index is not ready.
    let day_indices: Vec<DayIndex> = if global_ready {
        Vec::new()
    } else {
        eprintln!(
            "[search] global index not ready — loading per-day HNSW files (model={})",
            model_backend
        );
        date_dirs
            .iter()
            .filter_map(|(date, dir)| load_day_index_for(date.clone(), dir.clone(), model_backend))
            .collect()
    };

    let searched_days: Vec<String> = if global_ready {
        date_dirs.iter().map(|(d, _)| d.clone()).collect()
    } else {
        day_indices.iter().map(|d| d.date.clone()).collect()
    };

    // ── For each query embedding, search and hydrate ───────────────────────
    let mut results: Vec<QueryEntry> = Vec::with_capacity(query_count);

    for (_dd_idx, qemb) in &query_embs {
        let ts_unix = (qemb.timestamp / 1000) as u64;

        // Candidates: (date, dir, hnsw_id, timestamp, distance).
        // For the per-day branch we build lightweight tuples referencing
        // `day_indices` by index to avoid cloning String/PathBuf per hit.
        let mut candidates: Vec<(String, PathBuf, usize, i64, f32)> = Vec::new();

        if let Some(gidx) = global_idx {
            let hits = gidx.search(&qemb.embedding, k, ef.max(k));
            for hit in hits {
                let neighbor_ts = *hit.payload;
                let date = date_from_ts(neighbor_ts);
                let dir = skill_dir.join(&date);
                candidates.push((date, dir, hit.id, neighbor_ts, hit.distance));
            }
        } else {
            // Collect with day-index reference; only materialize owned
            // Strings for the top-k candidates that survive truncation.
            let mut raw_candidates: Vec<(usize, usize, i64, f32)> = Vec::new();
            for (di, day) in day_indices.iter().enumerate() {
                if day.index.is_empty() {
                    continue;
                }
                let hits = day.index.search(&qemb.embedding, k, ef.max(k));
                for hit in hits {
                    raw_candidates.push((di, hit.id, *hit.payload, hit.distance));
                }
            }
            raw_candidates.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));
            raw_candidates.truncate(k);
            candidates.reserve(raw_candidates.len());
            for (di, hid, ts, dist) in raw_candidates {
                candidates.push((day_indices[di].date.clone(), day_indices[di].dir.clone(), hid, ts, dist));
            }
        }

        // ── Hydrate each candidate ────────────────────────────────────────
        let mut neighbors: Vec<NeighborEntry> = Vec::with_capacity(candidates.len());
        for (date, dir, candidate_hnsw_id, neighbor_ts, distance) in candidates {
            let neighbor_unix = (neighbor_ts / 1000) as u64;
            let db_path = dir.join(SQLITE_FILE);

            let (hnsw_id, device_id, device_name, metrics) = if db_path.exists() {
                // Always hydrate by timestamp to stay robust when a day index was
                // rebuilt (internal HNSW ids can diverge from SQLite ids).
                let (hid, did, dn) = get_embedding_by_ts(&db_path, neighbor_ts);
                let m = get_embedding_metrics_by_ts(&db_path, neighbor_ts);
                let resolved_hid = if hid > 0 { hid as usize } else { candidate_hnsw_id };
                (resolved_hid, did, dn, m)
            } else {
                (candidate_hnsw_id, None, None, None)
            };

            let labels = if labels_db.exists() {
                get_labels_for(&labels_db, neighbor_unix)
            } else {
                vec![]
            };

            neighbors.push(NeighborEntry {
                hnsw_id,
                timestamp: neighbor_ts,
                timestamp_unix: neighbor_unix,
                distance,
                date,
                device_id,
                device_name,
                labels,
                metrics,
            });
        }

        results.push(QueryEntry {
            timestamp: qemb.timestamp,
            timestamp_unix: ts_unix,
            neighbors,
        });
    }

    SearchResult {
        start_utc,
        end_utc,
        k,
        ef,
        query_count,
        searched_days,
        results,
    }
}

/// Execute the streaming search logic, calling `emit` for each progress event.
///
/// This is the pure-logic core that both the Tauri command and the WebSocket
/// handler delegate to.  The caller is responsible for running this on a
/// blocking thread if needed.
#[allow(clippy::needless_pass_by_value)]
pub fn stream_search_inner(
    skill_dir: &Path,
    start_utc: u64,
    end_utc: u64,
    k: usize,
    ef: usize,
    global_index: GlobalIndexHandle,
    emit: &dyn Fn(SearchProgress),
) {
    stream_search_inner_for(skill_dir, start_utc, end_utc, k, ef, global_index, emit, "zuna");
}

/// Model-aware variant of [`stream_search_inner`].
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub fn stream_search_inner_for(
    skill_dir: &Path,
    start_utc: u64,
    end_utc: u64,
    k: usize,
    ef: usize,
    global_index: GlobalIndexHandle,
    emit: &dyn Fn(SearchProgress),
    model_backend: &str,
) {
    let start_ts = (start_utc as i64) * 1000;
    let end_ts = (end_utc as i64) * 1000;
    let labels_db = skill_dir.join(LABELS_FILE);
    let date_dirs = list_date_dirs(skill_dir);

    // ── Decide backend ───────────────────────────────────────────────────
    let global_guard = global_index.as_ref().map(|arc| arc.lock_or_recover());
    let global_idx: Option<&LabeledIndex<Cosine, i64>> = global_guard.as_deref().and_then(|opt| opt.as_ref());
    let global_ready = global_idx.map(|i| !i.is_empty()).unwrap_or(false);

    let day_indices: Vec<DayIndex> = if global_ready {
        Vec::new()
    } else {
        date_dirs
            .iter()
            .filter_map(|(date, dir)| load_day_index_for(date.clone(), dir.clone(), model_backend))
            .collect()
    };

    let searched_days: Vec<String> = if global_ready {
        date_dirs.iter().map(|(d, _)| d.clone()).collect()
    } else {
        day_indices.iter().map(|d| d.date.clone()).collect()
    };

    // ── Collect query embeddings ─────────────────────────────────────────
    // Store index into `date_dirs` to avoid cloning String/PathBuf per embedding.
    let mut query_embs: Vec<(usize, RawEmb)> = Vec::new();
    for (dd_idx, (date, dir)) in date_dirs.iter().enumerate() {
        let db_path = dir.join(SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }
        let embs = read_embeddings_in_range(&db_path, start_ts, end_ts);
        let _ = date; // used only for db_path
        for emb in embs {
            query_embs.push((dd_idx, emb));
        }
    }
    let query_count = query_embs.len();

    emit(SearchProgress {
        kind: "started".into(),
        query_count: Some(query_count),
        searched_days: Some(searched_days),
        entry: None,
        total: None,
        error: None,
        done_count: None,
    });

    for (idx, (_dd_idx, qemb)) in query_embs.iter().enumerate() {
        let ts_unix = (qemb.timestamp / 1000) as u64;

        let mut candidates: Vec<(String, PathBuf, usize, i64, f32)> = Vec::new();

        if let Some(gidx) = global_idx {
            let hits = gidx.search(&qemb.embedding, k, ef.max(k));
            for hit in hits {
                let neighbor_ts = *hit.payload;
                let date = date_from_ts(neighbor_ts);
                let dir = skill_dir.join(&date);
                candidates.push((date, dir, hit.id, neighbor_ts, hit.distance));
            }
        } else {
            // Collect with day-index reference; only materialize owned
            // Strings for the top-k candidates that survive truncation.
            let mut raw_candidates: Vec<(usize, usize, i64, f32)> = Vec::new();
            for (di, day) in day_indices.iter().enumerate() {
                if day.index.is_empty() {
                    continue;
                }
                let hits = day.index.search(&qemb.embedding, k, ef.max(k));
                for hit in hits {
                    raw_candidates.push((di, hit.id, *hit.payload, hit.distance));
                }
            }
            raw_candidates.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));
            raw_candidates.truncate(k);
            candidates.reserve(raw_candidates.len());
            for (di, hid, ts, dist) in raw_candidates {
                candidates.push((day_indices[di].date.clone(), day_indices[di].dir.clone(), hid, ts, dist));
            }
        }

        let mut neighbors: Vec<NeighborEntry> = Vec::with_capacity(candidates.len());
        for (date, dir, candidate_hnsw_id, neighbor_ts, distance) in candidates {
            let neighbor_unix = (neighbor_ts / 1000) as u64;
            let db_path = dir.join(SQLITE_FILE);

            let (hnsw_id, device_id, device_name, metrics) = if db_path.exists() {
                // Always hydrate by timestamp to stay robust when a day index was
                // rebuilt (internal HNSW ids can diverge from SQLite ids).
                let (hid, did, dn) = get_embedding_by_ts(&db_path, neighbor_ts);
                let m = get_embedding_metrics_by_ts(&db_path, neighbor_ts);
                let resolved_hid = if hid > 0 { hid as usize } else { candidate_hnsw_id };
                (resolved_hid, did, dn, m)
            } else {
                (candidate_hnsw_id, None, None, None)
            };

            let labels = if labels_db.exists() {
                get_labels_for(&labels_db, neighbor_unix)
            } else {
                vec![]
            };
            neighbors.push(NeighborEntry {
                hnsw_id,
                timestamp: neighbor_ts,
                timestamp_unix: neighbor_unix,
                distance,
                date,
                device_id,
                device_name,
                labels,
                metrics,
            });
        }

        let entry = QueryEntry {
            timestamp: qemb.timestamp,
            timestamp_unix: ts_unix,
            neighbors,
        };
        emit(SearchProgress {
            kind: "result".into(),
            entry: Some(entry),
            done_count: Some(idx + 1),
            query_count: None,
            searched_days: None,
            total: None,
            error: None,
        });
    }

    emit(SearchProgress {
        kind: "done".into(),
        total: Some(query_count),
        query_count: None,
        searched_days: None,
        entry: None,
        error: None,
        done_count: None,
    });
}

/// Find which recording session (csv_path) a given timestamp belongs to.
/// Returns session metadata if found.
#[derive(Debug, Serialize)]
pub struct SessionRef {
    pub csv_path: String,
    pub session_start_utc: Option<u64>,
    pub session_end_utc: Option<u64>,
    pub device_name: Option<String>,
}

pub fn find_session_for_timestamp_in(skill_dir: &Path, timestamp_unix: u64, date: &str) -> Option<SessionRef> {
    let day_dir = skill_dir.join(date);
    if !day_dir.exists() {
        return None;
    }

    let rd = std::fs::read_dir(&day_dir).ok()?;
    let mut best: Option<SessionRef> = None;
    let mut best_dist: u64 = u64::MAX;

    for entry in rd.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".json") || !(name.starts_with("exg_") || name.starts_with("muse_")) {
            continue;
        }
        if name.contains("_ppg") || name.contains("_metrics") {
            continue;
        }

        let json_path = entry.path();
        let Ok(text) = std::fs::read_to_string(&json_path) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };

        let start = meta.get("session_start_utc").and_then(serde_json::Value::as_u64);
        let end = meta.get("session_end_utc").and_then(serde_json::Value::as_u64);

        if let (Some(s), Some(e)) = (start, end) {
            // Resolve data file: try .parquet first, fall back to .csv.
            let resolve_data_path = |json_name: &str| -> std::path::PathBuf {
                let pq = day_dir.join(json_name.replace(".json", ".parquet"));
                if pq.exists() {
                    pq
                } else {
                    day_dir.join(json_name.replace(".json", ".csv"))
                }
            };

            if timestamp_unix >= s && timestamp_unix <= e {
                let data_path = resolve_data_path(&name);
                return Some(SessionRef {
                    csv_path: data_path.to_string_lossy().to_string(),
                    session_start_utc: start,
                    session_end_utc: end,
                    device_name: meta
                        .get("device_name")
                        .and_then(|v| v.as_str())
                        .map(std::string::ToString::to_string),
                });
            }
            let dist = if timestamp_unix < s {
                s - timestamp_unix
            } else {
                timestamp_unix - e
            };
            if dist < best_dist {
                best_dist = dist;
                let data_path = resolve_data_path(&name);
                best = Some(SessionRef {
                    csv_path: data_path.to_string_lossy().to_string(),
                    session_start_utc: start,
                    session_end_utc: end,
                    device_name: meta
                        .get("device_name")
                        .and_then(|v| v.as_str())
                        .map(std::string::ToString::to_string),
                });
            }
        }
    }

    if best_dist <= 300 {
        best
    } else {
        None
    }
}

// ── Interactive Cross-Modal Search ────────────────────────────────────────────

/// A single node in the interactive search graph.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct InteractiveGraphNode {
    /// Stable identifier used for edge references.
    pub id: String,
    /// Node layer: "query" | "text_label" | "eeg_point" | "found_label" | "screenshot"
    pub kind: String,
    /// Human-readable label text (query string / label annotation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Unix-second timestamp (for EEG points and found labels).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_unix: Option<u64>,
    /// Cosine distance from the parent node (0 = identical, higher = farther).
    pub distance: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eeg_metrics: Option<NeighborMetrics>,
    /// ID of the parent node that this node was discovered from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// 2-D / 3-D PCA projection of the node's text embedding.
    /// All axes are normalised to [-1, 1].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proj_x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proj_y: Option<f32>,
    /// Third PCA axis for 3-D projection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proj_z: Option<f32>,

    // ── Screenshot-specific fields ────────────────────────────────────────
    /// Relative path to the screenshot image file (screenshot nodes only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// Application name at capture time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    /// Window title at capture time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
    /// OCR-extracted text from the screenshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocr_text: Option<String>,
    /// Cosine similarity between the query text and the OCR text embedding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocr_similarity: Option<f32>,
}

/// A directed edge in the interactive search graph.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InteractiveGraphEdge {
    pub from_id: String,
    pub to_id: String,
    /// Strength of connection — same scale as the corresponding distance.
    pub distance: f32,
    /// Edge kind: "text_sim" | "eeg_bridge" | "eeg_sim" | "label_prox" | "screenshot_prox" | "ocr_sim"
    pub kind: String,
}

/// Complete result returned by interactive search.
#[derive(Debug, Serialize)]
pub struct InteractiveSearchResult {
    pub nodes: Vec<InteractiveGraphNode>,
    pub edges: Vec<InteractiveGraphEdge>,
    /// Graphviz DOT source for the same graph.
    pub dot: String,
    /// Pre-rendered SVG – PCA-scatter layout for found_labels (when available).
    pub svg: String,
    /// Pre-rendered SVG – traditional column-per-EEG-parent layout (always present).
    pub svg_col: String,
    /// Pre-rendered SVG – 3-D perspective projection of all nodes including screenshots.
    pub svg_3d: String,
}

pub fn get_labels_near(labels_db: &Path, ts_unix: u64, window_secs: u64) -> Vec<LabelEntry> {
    let Ok(conn) = skill_data::util::open_readonly(labels_db) else {
        return vec![];
    };

    let ts = ts_unix as i64;
    let lo = ts_unix.saturating_sub(window_secs) as i64;
    let hi = (ts_unix.saturating_add(window_secs)) as i64;

    let Ok(mut stmt) = conn.prepare(
        "SELECT id, eeg_start, eeg_end, label_start, label_end, text
         FROM labels
         WHERE (eeg_start <= ?1 AND eeg_end >= ?1)
            OR (eeg_start BETWEEN ?2 AND ?3)
         ORDER BY ABS(CAST(eeg_start AS INTEGER) - ?4)
         LIMIT 5",
    ) else {
        return vec![];
    };

    stmt.query_map(params![ts, lo, hi, ts], |row| {
        Ok(LabelEntry {
            id: row.get(0)?,
            eeg_start: row.get::<_, i64>(1)? as u64,
            eeg_end: row.get::<_, i64>(2)? as u64,
            label_start: row.get::<_, i64>(3)? as u64,
            label_end: row.get::<_, i64>(4)? as u64,
            text: row.get(5)?,
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

// ── PCA helpers ────────────────────────────────────────────────────────────

/// Fetch the `text_embedding` BLOB for one label (read-only, no metrics).
pub fn get_found_label_embedding(labels_db: &Path, label_id: i64) -> Option<Vec<f32>> {
    let conn = skill_data::util::open_readonly(labels_db).ok()?;
    let blob: Option<Vec<u8>> = conn
        .query_row(
            "SELECT text_embedding FROM labels WHERE id = ?1",
            params![label_id],
            |row| row.get(0),
        )
        .ok()?;
    let blob = blob?;
    if blob.len() < 4 {
        return None;
    }
    Some(skill_data::util::blob_to_f32(&blob))
}

/// 2-component PCA via covariance-free power iteration.
///
/// Returns one `(x, y)` per input, normalised so every axis spans [-1, 1].
pub fn pca_2d(embeddings: &[Vec<f32>]) -> Vec<(f32, f32)> {
    let n = embeddings.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![(0.0, 0.0)];
    }
    let d = embeddings[0].len();
    if d < 2 {
        return vec![(0.0, 0.0); n];
    }

    let inv_n = 1.0 / n as f32;
    let mut mean = vec![0f32; d];
    for emb in embeddings {
        for (j, &v) in emb.iter().enumerate() {
            mean[j] += v * inv_n;
        }
    }
    let centered: Vec<Vec<f32>> = embeddings
        .iter()
        .map(|emb| emb.iter().zip(&mean).map(|(&v, &m)| v - m).collect())
        .collect();

    fn dot(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(&x, &y)| x * y).sum()
    }

    fn cov_mul(c: &[Vec<f32>], v: &[f32]) -> Vec<f32> {
        let xv: Vec<f32> = c.iter().map(|row| dot(row, v)).collect();
        let mut res = vec![0f32; v.len()];
        for (row, &coeff) in c.iter().zip(&xv) {
            for (r, &x) in res.iter_mut().zip(row) {
                *r += x * coeff;
            }
        }
        let inv = 1.0 / c.len() as f32;
        res.iter_mut().for_each(|x| *x *= inv);
        res
    }

    fn power_iter(c: &[Vec<f32>], mut v: Vec<f32>) -> Vec<f32> {
        for _ in 0..25 {
            v = cov_mul(c, &v);
            let norm = dot(&v, &v).sqrt().max(1e-12);
            v.iter_mut().for_each(|x| *x /= norm);
        }
        v
    }

    let norm0 = dot(&centered[0], &centered[0]).sqrt().max(1e-12);
    let init1: Vec<f32> = centered[0].iter().map(|&v| v / norm0).collect();
    let pc1 = power_iter(&centered, init1);

    let centered2: Vec<Vec<f32>> = centered
        .iter()
        .map(|v| {
            let p = dot(v, &pc1);
            v.iter().zip(&pc1).map(|(&vi, &pi)| vi - p * pi).collect()
        })
        .collect();
    let norm2 = dot(&centered2[0], &centered2[0]).sqrt();
    let init2 = if norm2 > 1e-12 {
        centered2[0].iter().map(|&v| v / norm2).collect::<Vec<_>>()
    } else {
        let mut perp = vec![0f32; d];
        if d > 1 {
            perp[1] = 1.0;
        }
        perp
    };
    let pc2 = power_iter(&centered2, init2);

    let coords: Vec<(f32, f32)> = centered.iter().map(|v| (dot(v, &pc1), dot(v, &pc2))).collect();
    let x_min = coords.iter().map(|&(x, _)| x).fold(f32::MAX, f32::min);
    let x_max = coords.iter().map(|&(x, _)| x).fold(f32::MIN, f32::max);
    let y_min = coords.iter().map(|&(_, y)| y).fold(f32::MAX, f32::min);
    let y_max = coords.iter().map(|&(_, y)| y).fold(f32::MIN, f32::max);
    let xr = (x_max - x_min).max(1e-6);
    let yr = (y_max - y_min).max(1e-6);
    coords
        .iter()
        .map(|&(x, y)| ((x - x_min) / xr * 2.0 - 1.0, (y - y_min) / yr * 2.0 - 1.0))
        .collect()
}

/// 3-component PCA via covariance-free power iteration.
///
/// Returns one `(x, y, z)` per input, normalised so every axis spans [-1, 1].
pub fn pca_3d(embeddings: &[Vec<f32>]) -> Vec<(f32, f32, f32)> {
    let n = embeddings.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![(0.0, 0.0, 0.0)];
    }
    let d = embeddings[0].len();
    if d < 3 {
        return vec![(0.0, 0.0, 0.0); n];
    }

    let inv_n = 1.0 / n as f32;
    let mut mean = vec![0f32; d];
    for emb in embeddings {
        for (j, &v) in emb.iter().enumerate() {
            mean[j] += v * inv_n;
        }
    }
    let centered: Vec<Vec<f32>> = embeddings
        .iter()
        .map(|emb| emb.iter().zip(&mean).map(|(&v, &m)| v - m).collect())
        .collect();

    fn dot(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(&x, &y)| x * y).sum()
    }

    fn cov_mul(c: &[Vec<f32>], v: &[f32]) -> Vec<f32> {
        let xv: Vec<f32> = c.iter().map(|row| dot(row, v)).collect();
        let mut res = vec![0f32; v.len()];
        for (row, &coeff) in c.iter().zip(&xv) {
            for (r, &x) in res.iter_mut().zip(row) {
                *r += x * coeff;
            }
        }
        let inv = 1.0 / c.len() as f32;
        res.iter_mut().for_each(|x| *x *= inv);
        res
    }

    fn power_iter(c: &[Vec<f32>], mut v: Vec<f32>) -> Vec<f32> {
        for _ in 0..25 {
            v = cov_mul(c, &v);
            let norm = dot(&v, &v).sqrt().max(1e-12);
            v.iter_mut().for_each(|x| *x /= norm);
        }
        v
    }

    fn deflate(centered: &[Vec<f32>], pc: &[f32]) -> Vec<Vec<f32>> {
        centered
            .iter()
            .map(|v| {
                let p = dot(v, pc);
                v.iter().zip(pc).map(|(&vi, &pi)| vi - p * pi).collect()
            })
            .collect()
    }

    fn init_vec(centered: &[Vec<f32>], d: usize) -> Vec<f32> {
        let norm = dot(&centered[0], &centered[0]).sqrt().max(1e-12);
        centered[0]
            .iter()
            .map(|&v| v / norm)
            .collect::<Vec<_>>()
            .into_iter()
            .chain(std::iter::repeat(0.0))
            .take(d)
            .collect()
    }

    let pc1 = power_iter(&centered, init_vec(&centered, d));
    let deflated1 = deflate(&centered, &pc1);

    let norm2 = dot(&deflated1[0], &deflated1[0]).sqrt();
    let init2 = if norm2 > 1e-12 {
        deflated1[0].iter().map(|&v| v / norm2).collect::<Vec<_>>()
    } else {
        let mut perp = vec![0f32; d];
        perp[1] = 1.0;
        perp
    };
    let pc2 = power_iter(&deflated1, init2);
    let deflated2 = deflate(&deflated1, &pc2);

    let norm3 = dot(&deflated2[0], &deflated2[0]).sqrt();
    let init3 = if norm3 > 1e-12 {
        deflated2[0].iter().map(|&v| v / norm3).collect::<Vec<_>>()
    } else {
        let mut perp = vec![0f32; d];
        perp[2] = 1.0;
        perp
    };
    let pc3 = power_iter(&deflated2, init3);

    let coords: Vec<(f32, f32, f32)> = centered
        .iter()
        .map(|v| (dot(v, &pc1), dot(v, &pc2), dot(v, &pc3)))
        .collect();

    let x_min = coords.iter().map(|c| c.0).fold(f32::MAX, f32::min);
    let x_max = coords.iter().map(|c| c.0).fold(f32::MIN, f32::max);
    let y_min = coords.iter().map(|c| c.1).fold(f32::MAX, f32::min);
    let y_max = coords.iter().map(|c| c.1).fold(f32::MIN, f32::max);
    let z_min = coords.iter().map(|c| c.2).fold(f32::MAX, f32::min);
    let z_max = coords.iter().map(|c| c.2).fold(f32::MIN, f32::max);
    let xr = (x_max - x_min).max(1e-6);
    let yr = (y_max - y_min).max(1e-6);
    let zr = (z_max - z_min).max(1e-6);

    coords
        .iter()
        .map(|&(x, y, z)| {
            (
                (x - x_min) / xr * 2.0 - 1.0,
                (y - y_min) / yr * 2.0 - 1.0,
                (z - z_min) / zr * 2.0 - 1.0,
            )
        })
        .collect()
}

// ── File save helpers ─────────────────────────────────────────────────────────

/// Sanitise a query string for use as part of a filename.
pub fn query_slug(query: &str, max: usize) -> String {
    query
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .take(max)
        .collect::<String>()
        .trim()
        .replace(' ', "_")
        .to_lowercase()
}

/// `YYYYMMDD_HHMMSS` timestamp from a Unix second value (UTC).
pub fn file_ts(secs: u64) -> String {
    let tod = secs % 86400;
    let h = tod / 3600;
    let m = (tod % 3600) / 60;
    let s = tod % 60;
    let z = (secs / 86400) as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if mo <= 2 { y + 1 } else { y };
    format!("{yr:04}{mo:02}{d:02}_{h:02}{m:02}{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── query_slug ────────────────────────────────────────────────────────

    #[test]
    fn query_slug_basic() {
        assert_eq!(query_slug("Hello World", 50), "hello_world");
    }

    #[test]
    fn query_slug_special_chars_stripped() {
        assert_eq!(query_slug("focus!@#$%^&*()", 50), "focus");
    }

    #[test]
    fn query_slug_max_length() {
        // Takes first 10 chars "this is a ", trims, then replaces spaces → "this_is_a"
        assert_eq!(query_slug("this is a very long query", 10), "this_is_a");
    }

    #[test]
    fn query_slug_preserves_hyphens_underscores() {
        assert_eq!(query_slug("my-cool_query", 50), "my-cool_query");
    }

    #[test]
    fn query_slug_empty() {
        assert_eq!(query_slug("", 50), "");
    }

    // ── file_ts ───────────────────────────────────────────────────────────

    #[test]
    fn file_ts_epoch() {
        // 1970-01-01 00:00:00 UTC
        assert_eq!(file_ts(0), "19700101_000000");
    }

    #[test]
    fn file_ts_known_date() {
        // 2024-01-15 13:30:45 UTC = 1705325445
        assert_eq!(file_ts(1705325445), "20240115_133045");
    }

    #[test]
    fn file_ts_recent() {
        // 2026-03-24 12:00:00 UTC = 1774267200
        let ts = file_ts(1774267200);
        assert!(ts.starts_with("2026"), "expected 2026, got {ts}");
    }

    // ── pca_2d ────────────────────────────────────────────────────────────

    #[test]
    fn pca_2d_empty() {
        assert!(pca_2d(&[]).is_empty());
    }

    #[test]
    fn pca_2d_single_point() {
        let result = pca_2d(&[vec![1.0, 2.0, 3.0]]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], (0.0, 0.0));
    }

    #[test]
    fn pca_2d_output_length_matches_input() {
        let data = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0], vec![0.0, 0.0, 1.0]];
        assert_eq!(pca_2d(&data).len(), 3);
    }

    #[test]
    fn pca_2d_points_are_separated() {
        let data = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0], vec![0.0, 0.0, 1.0]];
        let pts = pca_2d(&data);
        // Not all identical
        assert!(pts.iter().any(|p| *p != pts[0]));
    }

    // ── pca_3d ────────────────────────────────────────────────────────────

    #[test]
    fn pca_3d_output_length_matches_input() {
        let data = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
            vec![1.0, 1.0, 1.0],
        ];
        assert_eq!(pca_3d(&data).len(), 4);
    }
}
