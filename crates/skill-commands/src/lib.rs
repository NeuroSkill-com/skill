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
use rusqlite::{Connection, OpenFlags, params};
use serde::Serialize;

use skill_constants::{HNSW_INDEX_FILE, LABELS_FILE, SQLITE_FILE};

// Re-export shared utilities so downstream crates keep compiling.
pub use skill_data::util::{MutexExt, unix_to_ts, ts_to_unix, fmt_unix_utc};

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
pub struct LabelEntry {
    pub id:          i64,
    /// Unix-second start of the EEG window captured during labelling.
    pub eeg_start:   u64,
    /// Unix-second end of the EEG window.
    pub eeg_end:     u64,
    pub label_start: u64,
    pub label_end:   u64,
    /// Free-text label entered by the user.
    pub text:        String,
}

/// Compact EEG metrics attached to a search neighbor.
#[derive(Debug, Serialize, Default, Clone)]
pub struct NeighborMetrics {
    #[serde(skip_serializing_if = "Option::is_none")] pub relaxation: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub engagement: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub faa:        Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub tar:        Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub mood:       Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub meditation: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub cognitive_load: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub drowsiness: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub hr:         Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub snr:        Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub rel_alpha:  Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub rel_beta:   Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub rel_theta:  Option<f64>,
    // Headache / Migraine correlate indices
    #[serde(skip_serializing_if = "Option::is_none")] pub headache_index:      Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub migraine_index:      Option<f64>,
    // Consciousness metrics
    #[serde(skip_serializing_if = "Option::is_none")] pub consciousness_lzc:          Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub consciousness_wakefulness:  Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub consciousness_integration:  Option<f64>,
}

/// One embedding found by the HNSW nearest-neighbour search.
#[derive(Debug, Serialize, Clone)]
pub struct NeighborEntry {
    /// Zero-based insertion id within the day's HNSW index.
    pub hnsw_id:        usize,
    /// `YYYYMMDDHHmmss` UTC timestamp stored in the index payload.
    pub timestamp:      i64,
    /// Same timestamp converted to Unix seconds (for JS `Date` construction).
    pub timestamp_unix: u64,
    /// Cosine distance from the query embedding (0 = identical).
    pub distance:       f32,
    /// Which YYYYMMDD index this neighbor came from.
    pub date:           String,
    pub device_id:      Option<String>,
    pub device_name:    Option<String>,
    /// Labels whose EEG window contains this embedding's timestamp.
    pub labels:         Vec<LabelEntry>,
    /// Key EEG metrics for this epoch (if available in SQLite).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics:        Option<NeighborMetrics>,
}

/// Results for one query embedding.
#[derive(Debug, Serialize, Clone)]
pub struct QueryEntry {
    pub timestamp:      i64,
    pub timestamp_unix: u64,
    pub neighbors:      Vec<NeighborEntry>,
}

/// Top-level result returned by [`search_embeddings_in_range`].
#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub start_utc:    u64,
    pub end_utc:      u64,
    pub k:            usize,
    pub ef:           usize,
    /// Total query embeddings found in the input range.
    pub query_count:  usize,
    /// YYYYMMDD strings of every day whose index was searched.
    pub searched_days: Vec<String>,
    pub results:      Vec<QueryEntry>,
}

/// Progress event streamed by streaming search.
#[derive(Debug, Serialize, Clone)]
pub struct SearchProgress {
    /// Kind: "started" | "result" | "done" | "error"
    pub kind:        String,
    /// Filled for "started"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub searched_days: Option<Vec<String>>,
    /// Filled for "result": one QueryEntry's worth of data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry:       Option<QueryEntry>,
    /// Filled for "done"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total:       Option<usize>,
    /// Filled for "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error:       Option<String>,
    /// How many results have been emitted so far (for progress bar)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_count:  Option<usize>,
}

// ── Internal helpers ──────────────────────────────────────────────────────────

pub struct DayIndex {
    pub date:  String,
    pub dir:   PathBuf,
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
pub fn load_day_index(date: String, dir: PathBuf) -> Option<DayIndex> {
    let path = dir.join(HNSW_INDEX_FILE);
    if !path.exists() { return None; }
    match LabeledIndex::load_mmap(&path, Cosine) {
        Ok(idx) => {
            eprintln!("[search] loaded HNSW {} ({} vecs)", date, idx.len());
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
    hnsw_id:   i64,
    timestamp: i64,
    embedding: Vec<f32>,
}

/// Read every embedding in [start_ts, end_ts] from a single day's SQLite.
fn read_embeddings_in_range(db_path: &Path, start_ts: i64, end_ts: i64) -> Vec<RawEmb> {
    let conn = match skill_data::util::open_readonly(db_path) {
        Ok(c)  => c,
        Err(e) => { eprintln!("[search] open {}: {e}", db_path.display()); return vec![]; }
    };

    let mut stmt = match conn.prepare(
        "SELECT hnsw_id, timestamp, eeg_embedding
         FROM embeddings
         WHERE timestamp BETWEEN ?1 AND ?2
         ORDER BY timestamp",
    ) {
        Ok(s)  => s,
        Err(e) => { eprintln!("[search] prepare: {e}"); return vec![]; }
    };

    stmt.query_map(params![start_ts, end_ts], |row| {
        let hnsw_id:   i64    = row.get(0)?;
        let timestamp: i64    = row.get(1)?;
        let blob:      Vec<u8> = row.get(2)?;
        let embedding = blob
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        Ok(RawEmb { hnsw_id, timestamp, embedding })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

/// Look up `device_id` and `device_name` for a specific `hnsw_id` in a day's SQLite.
fn get_embedding_meta(db_path: &Path, hnsw_id: i64) -> (Option<String>, Option<String>) {
    let Ok(conn) = skill_data::util::open_readonly(db_path) else { return (None, None) };

    conn.query_row(
        "SELECT device_id, device_name FROM embeddings WHERE hnsw_id = ?1 LIMIT 1",
        params![hnsw_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap_or((None, None))
}

/// Fetch key EEG metrics for a single embedding by hnsw_id.
fn get_embedding_metrics(db_path: &Path, hnsw_id: i64) -> Option<NeighborMetrics> {
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
         FROM embeddings WHERE hnsw_id = ?1 LIMIT 1",
        params![hnsw_id],
        |row| {
            let g = |i: usize| -> Option<f64> { row.get::<_, Option<f64>>(i).unwrap_or(None) };
            Ok(NeighborMetrics {
                relaxation: g(0), engagement: g(1),
                faa: g(2), tar: g(3), mood: g(4),
                meditation: g(5), cognitive_load: g(6), drowsiness: g(7),
                hr: g(8), snr: g(9),
                rel_alpha: g(10), rel_beta: g(11), rel_theta: g(12),
                headache_index: g(13), migraine_index: g(14),
                consciousness_lzc: g(15), consciousness_wakefulness: g(16),
                consciousness_integration: g(17),
            })
        },
    ).ok()
}

/// Derive the `YYYYMMDD` date string from a `YYYYMMDDHHmmss` timestamp integer.
fn date_from_ts(ts: i64) -> String {
    format!("{}", ts / 1_000_000)
}

/// Look up a row in `db_path` by its `YYYYMMDDHHmmss` timestamp.
/// Returns `(hnsw_id, device_id, device_name)`.
fn get_embedding_by_ts(
    db_path:   &Path,
    timestamp: i64,
) -> (i64, Option<String>, Option<String>) {
    let Ok(conn) = skill_data::util::open_readonly(db_path) else { return (0, None, None) };

    conn.query_row(
        "SELECT hnsw_id, device_id, device_name \
         FROM embeddings WHERE timestamp = ?1 LIMIT 1",
        params![timestamp],
        |row| Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
        )),
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
                relaxation: g(0), engagement: g(1),
                faa: g(2), tar: g(3), mood: g(4),
                meditation: g(5), cognitive_load: g(6), drowsiness: g(7),
                hr: g(8), snr: g(9),
                rel_alpha: g(10), rel_beta: g(11), rel_theta: g(12),
                headache_index: g(13), migraine_index: g(14),
                consciousness_lzc: g(15), consciousness_wakefulness: g(16),
                consciousness_integration: g(17),
            })
        },
    ).ok()
}

/// Fetch all labels from `labels.sqlite` whose EEG window contains `ts_unix`.
pub fn get_labels_for(labels_db: &Path, ts_unix: u64) -> Vec<LabelEntry> {
    let Ok(conn) = skill_data::util::open_readonly(labels_db) else { return vec![] };

    let Ok(mut stmt) = conn.prepare(
        "SELECT id, eeg_start, eeg_end, label_start, label_end, text
         FROM labels
         WHERE eeg_start <= ?1 AND eeg_end >= ?1
         ORDER BY eeg_start",
    ) else { return vec![] };

    stmt.query_map(params![ts_unix as i64], |row| {
        Ok(LabelEntry {
            id:          row.get(0)?,
            eeg_start:   row.get::<_, i64>(1)? as u64,
            eeg_end:     row.get::<_, i64>(2)? as u64,
            label_start: row.get::<_, i64>(3)? as u64,
            label_end:   row.get::<_, i64>(4)? as u64,
            text:        row.get(5)?,
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
pub fn search_embeddings_in_range(
    skill_dir:    &Path,
    start_utc:    u64,
    end_utc:      u64,
    k:            usize,
    ef:           usize,
    global_index: GlobalIndexHandle,
) -> SearchResult {
    let start_ts  = unix_to_ts(start_utc);
    let end_ts    = unix_to_ts(end_utc);
    let labels_db = skill_dir.join(LABELS_FILE);
    let date_dirs = list_date_dirs(skill_dir);

    // ── Collect query embeddings from days that overlap [start_ts, end_ts] ────
    let mut query_embs: Vec<(String, PathBuf, RawEmb)> = Vec::new();
    for (date, dir) in &date_dirs {
        let db_path = dir.join(SQLITE_FILE);
        if !db_path.exists() { continue; }
        let embs = read_embeddings_in_range(&db_path, start_ts, end_ts);
        if !embs.is_empty() {
            eprintln!("[search] {} query embs from {}", embs.len(), date);
        }
        for emb in embs {
            query_embs.push((date.clone(), dir.clone(), emb));
        }
    }
    let query_count = query_embs.len();

    // ── Decide search backend ─────────────────────────────────────────────
    let global_guard = global_index.as_ref().map(|arc| arc.lock_or_recover());
    let global_idx: Option<&LabeledIndex<Cosine, i64>> = global_guard
        .as_deref()
        .and_then(|opt| opt.as_ref());
    let global_ready = global_idx.map(|idx| !idx.is_empty()).unwrap_or(false);

    // Per-day indices — only loaded when the global index is not ready.
    let day_indices: Vec<DayIndex> = if global_ready {
        Vec::new()
    } else {
        eprintln!("[search] global index not ready — loading per-day HNSW files");
        date_dirs
            .iter()
            .filter_map(|(date, dir)| load_day_index(date.clone(), dir.clone()))
            .collect()
    };

    let searched_days: Vec<String> = if global_ready {
        date_dirs.iter().map(|(d, _)| d.clone()).collect()
    } else {
        day_indices.iter().map(|d| d.date.clone()).collect()
    };

    // ── For each query embedding, search and hydrate ───────────────────────
    let mut results: Vec<QueryEntry> = Vec::with_capacity(query_count);

    for (_qdate, _qdir, qemb) in &query_embs {
        let ts_unix = ts_to_unix(qemb.timestamp);

        let mut candidates: Vec<(String, PathBuf, usize, i64, f32)> = Vec::new();

        if let Some(gidx) = global_idx {
            let hits = gidx.search(&qemb.embedding, k, ef.max(k));
            for hit in hits {
                let neighbor_ts = *hit.payload;
                let date        = date_from_ts(neighbor_ts);
                let dir         = skill_dir.join(&date);
                candidates.push((date, dir, hit.id, neighbor_ts, hit.distance));
            }
        } else {
            for day in &day_indices {
                if day.index.is_empty() { continue; }
                let hits = day.index.search(&qemb.embedding, k, ef.max(k));
                for hit in hits {
                    candidates.push((
                        day.date.clone(),
                        day.dir.clone(),
                        hit.id,
                        *hit.payload,
                        hit.distance,
                    ));
                }
            }
            candidates.sort_by(|a, b| a.4.partial_cmp(&b.4).unwrap_or(std::cmp::Ordering::Equal));
            candidates.truncate(k);
        }

        // ── Hydrate each candidate ────────────────────────────────────────
        let mut neighbors: Vec<NeighborEntry> = Vec::with_capacity(candidates.len());
        for (date, dir, candidate_hnsw_id, neighbor_ts, distance) in candidates {
            let neighbor_unix = ts_to_unix(neighbor_ts);
            let db_path = dir.join(SQLITE_FILE);

            let (hnsw_id, device_id, device_name, metrics) = if db_path.exists() {
                if global_idx.is_some() {
                    let (hid, did, dn) = get_embedding_by_ts(&db_path, neighbor_ts);
                    let m = get_embedding_metrics_by_ts(&db_path, neighbor_ts);
                    (hid as usize, did, dn, m)
                } else {
                    let (did, dn) = get_embedding_meta(&db_path, candidate_hnsw_id as i64);
                    let m = get_embedding_metrics(&db_path, candidate_hnsw_id as i64);
                    (candidate_hnsw_id, did, dn, m)
                }
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
                timestamp:      neighbor_ts,
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
            timestamp:      qemb.timestamp,
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
pub fn stream_search_inner(
    skill_dir:    &Path,
    start_utc:    u64,
    end_utc:      u64,
    k:            usize,
    ef:           usize,
    global_index: GlobalIndexHandle,
    emit:         &dyn Fn(SearchProgress),
) {
    let start_ts  = unix_to_ts(start_utc);
    let end_ts    = unix_to_ts(end_utc);
    let labels_db = skill_dir.join(LABELS_FILE);
    let date_dirs = list_date_dirs(skill_dir);

    // ── Decide backend ───────────────────────────────────────────────────
    let global_guard = global_index.as_ref().map(|arc| arc.lock_or_recover());
    let global_idx: Option<&LabeledIndex<Cosine, i64>> = global_guard
        .as_deref()
        .and_then(|opt| opt.as_ref());
    let global_ready = global_idx.map(|i| !i.is_empty()).unwrap_or(false);

    let day_indices: Vec<DayIndex> = if global_ready {
        Vec::new()
    } else {
        date_dirs.iter()
            .filter_map(|(date, dir)| load_day_index(date.clone(), dir.clone()))
            .collect()
    };

    let searched_days: Vec<String> = if global_ready {
        date_dirs.iter().map(|(d, _)| d.clone()).collect()
    } else {
        day_indices.iter().map(|d| d.date.clone()).collect()
    };

    // ── Collect query embeddings ─────────────────────────────────────────
    let mut query_embs: Vec<(String, PathBuf, RawEmb)> = Vec::new();
    for (date, dir) in &date_dirs {
        let db_path = dir.join(SQLITE_FILE);
        if !db_path.exists() { continue; }
        let embs = read_embeddings_in_range(&db_path, start_ts, end_ts);
        for emb in embs { query_embs.push((date.clone(), dir.clone(), emb)); }
    }
    let query_count = query_embs.len();

    emit(SearchProgress {
        kind: "started".into(),
        query_count: Some(query_count),
        searched_days: Some(searched_days),
        entry: None, total: None, error: None, done_count: None,
    });

    for (idx, (_qdate, _qdir, qemb)) in query_embs.iter().enumerate() {
        let ts_unix = ts_to_unix(qemb.timestamp);

        let mut candidates: Vec<(String, PathBuf, usize, i64, f32)> = Vec::new();

        if let Some(gidx) = global_idx {
            let hits = gidx.search(&qemb.embedding, k, ef.max(k));
            for hit in hits {
                let neighbor_ts = *hit.payload;
                let date = date_from_ts(neighbor_ts);
                let dir  = skill_dir.join(&date);
                candidates.push((date, dir, hit.id, neighbor_ts, hit.distance));
            }
        } else {
            for day in &day_indices {
                if day.index.is_empty() { continue; }
                let hits = day.index.search(&qemb.embedding, k, ef.max(k));
                for hit in hits {
                    candidates.push((day.date.clone(), day.dir.clone(), hit.id, *hit.payload, hit.distance));
                }
            }
            candidates.sort_by(|a, b| a.4.partial_cmp(&b.4).unwrap_or(std::cmp::Ordering::Equal));
            candidates.truncate(k);
        }

        let mut neighbors: Vec<NeighborEntry> = Vec::with_capacity(candidates.len());
        for (date, dir, candidate_hnsw_id, neighbor_ts, distance) in candidates {
            let neighbor_unix = ts_to_unix(neighbor_ts);
            let db_path = dir.join(SQLITE_FILE);

            let (hnsw_id, device_id, device_name, metrics) = if db_path.exists() {
                if global_idx.is_some() {
                    let (hid, did, dn) = get_embedding_by_ts(&db_path, neighbor_ts);
                    let m = get_embedding_metrics_by_ts(&db_path, neighbor_ts);
                    (hid as usize, did, dn, m)
                } else {
                    let (did, dn) = get_embedding_meta(&db_path, candidate_hnsw_id as i64);
                    let m = get_embedding_metrics(&db_path, candidate_hnsw_id as i64);
                    (candidate_hnsw_id, did, dn, m)
                }
            } else {
                (candidate_hnsw_id, None, None, None)
            };

            let labels = if labels_db.exists() { get_labels_for(&labels_db, neighbor_unix) } else { vec![] };
            neighbors.push(NeighborEntry { hnsw_id, timestamp: neighbor_ts, timestamp_unix: neighbor_unix, distance, date, device_id, device_name, labels, metrics });
        }

        let entry = QueryEntry { timestamp: qemb.timestamp, timestamp_unix: ts_unix, neighbors };
        emit(SearchProgress {
            kind: "result".into(),
            entry: Some(entry),
            done_count: Some(idx + 1),
            query_count: None, searched_days: None, total: None, error: None,
        });
    }

    emit(SearchProgress {
        kind: "done".into(),
        total: Some(query_count),
        query_count: None, searched_days: None, entry: None, error: None, done_count: None,
    });
}

/// Find which recording session (csv_path) a given timestamp belongs to.
/// Returns session metadata if found.
#[derive(Debug, Serialize)]
pub struct SessionRef {
    pub csv_path: String,
    pub session_start_utc: Option<u64>,
    pub session_end_utc:   Option<u64>,
    pub device_name:       Option<String>,
}

pub fn find_session_for_timestamp_in(
    skill_dir: &Path,
    timestamp_unix: u64,
    date: &str,
) -> Option<SessionRef> {
    let day_dir = skill_dir.join(date);
    if !day_dir.exists() { return None; }

    let rd = std::fs::read_dir(&day_dir).ok()?;
    let mut best: Option<SessionRef> = None;
    let mut best_dist: u64 = u64::MAX;

    for entry in rd.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".json") || !name.starts_with("muse_") { continue; }
        if name.contains("_ppg") || name.contains("_metrics") { continue; }

        let json_path = entry.path();
        let Ok(text) = std::fs::read_to_string(&json_path) else { continue };
        let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else { continue };

        let start = meta.get("session_start_utc").and_then(|v| v.as_u64());
        let end   = meta.get("session_end_utc").and_then(|v| v.as_u64());

        if let (Some(s), Some(e)) = (start, end) {
            if timestamp_unix >= s && timestamp_unix <= e {
                let csv_name = name.replace(".json", ".csv");
                let csv_path = day_dir.join(&csv_name);
                return Some(SessionRef {
                    csv_path: csv_path.to_string_lossy().to_string(),
                    session_start_utc: start,
                    session_end_utc: end,
                    device_name: meta.get("device_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
                });
            }
            let dist = if timestamp_unix < s { s - timestamp_unix } else { timestamp_unix - e };
            if dist < best_dist {
                best_dist = dist;
                let csv_name = name.replace(".json", ".csv");
                let csv_path = day_dir.join(&csv_name);
                best = Some(SessionRef {
                    csv_path: csv_path.to_string_lossy().to_string(),
                    session_start_utc: start,
                    session_end_utc: end,
                    device_name: meta.get("device_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
                });
            }
        }
    }

    if best_dist <= 300 { best } else { None }
}

// ── Interactive Cross-Modal Search ────────────────────────────────────────────

/// A single node in the interactive search graph.
#[derive(Debug, Serialize, Clone)]
pub struct InteractiveGraphNode {
    /// Stable identifier used for edge references.
    pub id:             String,
    /// Node layer: "query" | "text_label" | "eeg_point" | "found_label"
    pub kind:           String,
    /// Human-readable label text (query string / label annotation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text:           Option<String>,
    /// Unix-second timestamp (for EEG points and found labels).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_unix: Option<u64>,
    /// Cosine distance from the parent node (0 = identical, higher = farther).
    pub distance:       f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eeg_metrics:    Option<NeighborMetrics>,
    /// ID of the parent node that this node was discovered from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id:      Option<String>,
    /// 2-D PCA projection of the node's text embedding (found_label only).
    /// Both axes are normalised to [-1, 1].  Similar labels are close together.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proj_x:         Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proj_y:         Option<f32>,
}

/// A directed edge in the interactive search graph.
#[derive(Debug, Serialize, Clone)]
pub struct InteractiveGraphEdge {
    pub from_id:  String,
    pub to_id:    String,
    /// Strength of connection — same scale as the corresponding distance.
    pub distance: f32,
    /// Edge kind: "text_sim" | "eeg_bridge" | "eeg_sim" | "label_prox"
    pub kind:     String,
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
}

// ── DOT generation helpers ─────────────────────────────────────────────────

// `fmt_unix_utc` is re-exported from `skill_data::util` at the top of this file.

/// Escape a string for use inside a DOT double-quoted label.
pub fn dot_esc(s: &str) -> String {
    s.chars().flat_map(|c| match c {
        '"'  => vec!['\\', '"'],
        '\\' => vec!['\\', '\\'],
        '\n' | '\r' => vec![],
        _    => vec![c],
    }).collect()
}

/// Build a DOT label string for a node (may contain `\n` for graphviz newlines).
pub fn dot_node_label(n: &InteractiveGraphNode) -> String {
    match n.kind.as_str() {
        "query" => dot_esc(n.text.as_deref().unwrap_or("query")),
        "text_label" => {
            let text = dot_esc(n.text.as_deref().unwrap_or("?"));
            match n.timestamp_unix {
                Some(ts) => format!("{text}\\n{}", fmt_unix_utc(ts)),
                None     => text,
            }
        }
        "eeg_point" => match n.timestamp_unix {
            Some(ts) => fmt_unix_utc(ts),
            None     => n.id.clone(),
        },
        "found_label" => {
            let text = dot_esc(n.text.as_deref().unwrap_or("?"));
            match n.timestamp_unix {
                Some(ts) => format!("{text}\\n{}", fmt_unix_utc(ts)),
                None     => text,
            }
        }
        _ => dot_esc(n.text.as_deref().unwrap_or(&n.id)),
    }
}

/// Build a short edge label.
pub fn dot_edge_label(
    e:      &InteractiveGraphEdge,
    ts_map: &std::collections::HashMap<String, u64>,
) -> String {
    match e.kind.as_str() {
        "text_sim" => {
            let pct = ((1.0 - e.distance) * 100.0).clamp(0.0, 100.0);
            format!("{pct:.0}%")
        }
        "eeg_bridge" | "eeg_sim" => format!("d={:.3}", e.distance),
        "label_prox" => {
            if let (Some(&a), Some(&b)) = (ts_map.get(&e.from_id), ts_map.get(&e.to_id)) {
                let diff_m = (a as i64 - b as i64).unsigned_abs() / 60;
                format!("{diff_m}min")
            } else {
                format!("{:.2}", e.distance)
            }
        }
        _ => String::new(),
    }
}

/// Render `nodes` + `edges` as a Graphviz DOT string.
pub fn generate_dot(nodes: &[InteractiveGraphNode], edges: &[InteractiveGraphEdge]) -> String {
    let mut o = String::with_capacity(8 * 1024);

    o.push_str("digraph interactive_search {\n");
    o.push_str("  graph [rankdir=TB, bgcolor=\"white\", fontname=\"Helvetica\",\n");
    o.push_str("         splines=curved, pad=0.5, nodesep=0.55, ranksep=1.1];\n");
    o.push_str("  node  [fontname=\"Helvetica\", fontsize=10,\n");
    o.push_str("         style=\"filled,rounded\", penwidth=0, margin=\"0.18,0.10\"];\n");
    o.push_str("  edge  [fontname=\"Helvetica\", fontsize=8, arrowsize=0.75];\n\n");

    let ts_map: std::collections::HashMap<String, u64> = nodes.iter()
        .filter_map(|n| n.timestamp_unix.map(|ts| (n.id.clone(), ts)))
        .collect();

    let ids_of = |kind: &str| -> String {
        nodes.iter()
            .filter(|n| n.kind == kind)
            .map(|n| format!("\"{}\"", n.id))
            .collect::<Vec<_>>()
            .join(" ")
    };

    let query_row  = ids_of("query");
    let text_row   = ids_of("text_label");
    let eeg_row    = ids_of("eeg_point");
    let found_row  = ids_of("found_label");

    if !query_row.is_empty()  { o.push_str(&format!("  {{ rank=source; {query_row} }}\n")); }
    if !text_row.is_empty()   { o.push_str(&format!("  {{ rank=same;   {text_row} }}\n")); }
    if !eeg_row.is_empty()    { o.push_str(&format!("  {{ rank=same;   {eeg_row} }}\n")); }
    if !found_row.is_empty()  { o.push_str(&format!("  {{ rank=sink;   {found_row} }}\n")); }
    o.push('\n');

    for n in nodes {
        let (shape, fill, fc) = match n.kind.as_str() {
            "query"       => ("doublecircle", "#8b5cf6", "white"),
            "text_label"  => ("box",          "#3b82f6", "white"),
            "eeg_point"   => ("diamond",      "#f59e0b", "white"),
            "found_label" => ("ellipse",      "#10b981", "white"),
            _             => ("box",          "#888888", "white"),
        };
        let lbl   = dot_node_label(n);
        let title = n.text.as_deref().unwrap_or(&n.id);
        o.push_str(&format!(
            "  \"{id}\" [label=\"{lbl}\", shape={shape}, \
             fillcolor=\"{fill}\", fontcolor=\"{fc}\", \
             tooltip=\"{tip}\"];\n",
            id    = n.id,
            tip   = dot_esc(title),
        ));
    }
    o.push('\n');

    for e in edges {
        let (color, style, pw) = match e.kind.as_str() {
            "text_sim"    => ("#8b5cf6", "solid",  2.0_f32),
            "eeg_bridge"  => ("#f59e0b", "dashed", 1.5_f32),
            "eeg_sim"     => ("#f59e0b", "dotted", 1.5_f32),
            "label_prox"  => ("#10b981", "solid",  1.5_f32),
            _             => ("#888888", "solid",  1.0_f32),
        };
        let lbl = dot_edge_label(e, &ts_map);
        o.push_str(&format!(
            "  \"{from}\" -> \"{to}\" \
             [color=\"{color}\", style={style}, penwidth={pw:.1}, label=\"{lbl}\"];\n",
            from = e.from_id,
            to   = e.to_id,
        ));
    }

    o.push_str("}\n");
    o
}

// ── SVG generation ─────────────────────────────────────────────────────────

/// Escape a string for SVG/XML text content.
fn svg_esc(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
}

/// Truncate to at most `n` Unicode chars, appending `…` if clipped.
fn trunc(s: &str, n: usize) -> String {
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(n).collect();
    if chars.next().is_some() { format!("{head}…") } else { head }
}

/// Turbo colormap: t ∈ [0,1] → `#rrggbb` (matches the JS component).
fn turbo_hex(t: f64) -> String {
    let c = t.clamp(0.0, 1.0);
    let r = (0.13572138 + c*(4.61539260 + c*(-42.66032258 + c*(132.13108234 + c*(-152.54893924 + c*59.28637943))))).clamp(0.0,1.0);
    let g = (0.09140261 + c*(2.19418839 + c*(4.84296658   + c*(-14.18503333 + c*(4.27729857   + c*2.82956604))))).clamp(0.0,1.0);
    let b = (0.10667330 + c*(12.64194608+ c*(-60.58204836 + c*(110.36276771 + c*(-89.90310912 + c*27.34824973))))).clamp(0.0,1.0);
    format!("#{:02x}{:02x}{:02x}", (r*255.0) as u8, (g*255.0) as u8, (b*255.0) as u8)
}

/// Localised strings embedded into the SVG export.
/// Every field is plain text (already translated by the frontend).
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SvgLabels {
    pub layer_query:        String,
    pub layer_text_matches: String,
    pub layer_eeg_neighbors:String,
    pub layer_found_labels: String,
    pub legend_query:       String,
    pub legend_text:        String,
    pub legend_eeg:         String,
    pub legend_found:       String,
    /// Already interpolated: "Generated by Skill"
    pub generated_by:       String,
}

/// Iteratively separate overlapping label ellipses in the SVG scatter area.
fn separate_labels_svg(
    pos:    &mut [(f64, f64)],
    w:      f64,
    h:      f64,
    cx_min: f64,
    cx_max: f64,
    cy_min: f64,
    cy_max: f64,
) {
    let min_x = w + 8.0;
    let min_y = h + 8.0;

    for _ in 0..80 {
        let mut changed = false;
        for i in 0..pos.len() {
            for j in (i + 1)..pos.len() {
                let dx = pos[j].0 - pos[i].0;
                let dy = pos[j].1 - pos[i].1;
                let ox = min_x - dx.abs();
                let oy = min_y - dy.abs();
                if ox <= 0.0 || oy <= 0.0 { continue; }
                changed = true;
                if ox < oy {
                    let push = ox * 0.5 + 1.0;
                    let sign = if dx >= 0.0 { 1.0 } else { -1.0 };
                    pos[i].0 -= push * sign;
                    pos[j].0 += push * sign;
                } else {
                    let push = oy * 0.5 + 1.0;
                    let sign = if dy >= 0.0 { 1.0 } else { -1.0 };
                    pos[i].1 -= push * sign;
                    pos[j].1 += push * sign;
                }
            }
        }
        for p in pos.iter_mut() {
            p.0 = p.0.clamp(cx_min, cx_max);
            p.1 = p.1.clamp(cy_min, cy_max);
        }
        if !changed { break; }
    }
}

/// Render an SVG of the interactive search graph.
pub fn generate_svg(
    nodes:   &[InteractiveGraphNode],
    edges:   &[InteractiveGraphEdge],
    labels:  &SvgLabels,
    use_pca: bool,
) -> String {
    // ── Layout constants ──────────────────────────────────────────────────
    const NW:           f64 = 140.0;
    const NH:           f64 = 34.0;
    const QR:           f64 = 24.0;
    const TOP:          f64 = 60.0;
    const SIDE:         f64 = 40.0;
    const DAY_LBL_W:    f64 = 50.0;
    const HOUR_LBL_H:   f64 = 16.0;
    const BAND_GAP:     f64 = 10.0;
    const BAND_PAD:     f64 = 10.0;
    const TL_COL_GAP:   f64 = 8.0;
    const TL_ROW_GAP:   f64 = 6.0;
    const TL_CELL_PAD:  f64 = 5.0;
    const EEG_CELL_W:   f64 = 54.0;
    const EEG_CELL_H:   f64 = 36.0;
    const EEG_S:        f64 = 11.0;
    const FL_COL_GAP:   f64 = 10.0;
    const FL_ROW_GAP:   f64 = 6.0;
    const FL_HDR_H:     f64 = 14.0;

    let kind_order = ["query", "text_label", "eeg_point", "found_label"];
    let layers: Vec<Vec<&InteractiveGraphNode>> = kind_order.iter()
        .map(|k| nodes.iter().filter(|n| n.kind == *k).collect())
        .collect();

    let ts_dhm = |ts: u64| -> (String, u32, u32) {
        let dt   = fmt_unix_utc(ts);
        let date = dt[..10].to_string();
        let h    = ((ts % 86400) / 3600) as u32;
        let m    = ((ts % 3600)  / 60)   as u32;
        (date, h, m)
    };

    // ── Text-matches grid analysis ────────────────────────────────────────
    let has_tl = !layers[1].is_empty();
    let tl_info: Vec<(String, u32, u32)> = layers[1].iter()
        .map(|nd| ts_dhm(nd.timestamp_unix.unwrap_or(0)))
        .collect();
    let mut tl_days:  Vec<String> = tl_info.iter().map(|(d,_,_)| d.clone()).collect();
    tl_days.sort_unstable(); tl_days.dedup();
    let mut tl_hours: Vec<u32> = tl_info.iter().map(|(_,h,_)| *h).collect();
    tl_hours.sort_unstable(); tl_hours.dedup();
    let n_tl_days  = tl_days.len().max(1);
    let n_tl_hours = tl_hours.len().max(1);
    let tl_day_idx:  std::collections::HashMap<&str, usize> =
        tl_days.iter().enumerate().map(|(i, d)| (d.as_str(), i)).collect();
    let tl_hour_idx: std::collections::HashMap<u32, usize>  =
        tl_hours.iter().enumerate().map(|(i, &h)| (h, i)).collect();
    let max_tl_stack: usize = {
        let mut counts: std::collections::HashMap<(usize, usize), usize> = Default::default();
        for (date, hour, _) in &tl_info {
            *counts.entry((tl_day_idx[date.as_str()], tl_hour_idx[hour])).or_insert(0) += 1;
        }
        counts.values().copied().max().unwrap_or(1)
    };
    let tl_col_w  = NW + TL_COL_GAP;
    let tl_cell_h = TL_CELL_PAD * 2.0
        + max_tl_stack as f64 * NH
        + max_tl_stack.saturating_sub(1) as f64 * TL_ROW_GAP;
    let tl_grid_w = n_tl_hours as f64 * tl_col_w - TL_COL_GAP;
    let tl_grid_h = n_tl_days  as f64 * tl_cell_h;

    // ── EEG grid analysis ─────────────────────────────────────────────────
    let has_eeg = !layers[2].is_empty();
    let eeg_info: Vec<(String, u32, u32)> = layers[2].iter()
        .map(|nd| ts_dhm(nd.timestamp_unix.unwrap_or(0)))
        .collect();
    let mut eeg_days:  Vec<String> = eeg_info.iter().map(|(d,_,_)| d.clone()).collect();
    eeg_days.sort_unstable(); eeg_days.dedup();
    let mut eeg_hours: Vec<u32> = eeg_info.iter().map(|(_,h,_)| *h).collect();
    eeg_hours.sort_unstable(); eeg_hours.dedup();
    let n_eeg_days  = eeg_days.len().max(1);
    let n_eeg_hours = eeg_hours.len().max(1);
    let eeg_grid_w  = n_eeg_hours as f64 * EEG_CELL_W;

    // ── Found-label cluster analysis ─────────────────────────────────────
    let has_fl = !layers[3].is_empty();
    let mut fl_parents: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        layers[3].iter()
            .filter_map(|nd| nd.parent_id.clone())
            .filter(|p| seen.insert(p.clone()))
            .collect()
    };
    fl_parents.sort_by_key(|p| {
        p.strip_prefix("ep_").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0)
    });
    let mut fl_by_parent: std::collections::HashMap<String, Vec<&InteractiveGraphNode>> =
        Default::default();
    for nd in &layers[3] {
        if let Some(pid) = nd.parent_id.as_deref() {
            fl_by_parent.entry(pid.to_string()).or_default().push(nd);
        }
    }
    let n_fl_cols    = fl_parents.len().max(1);
    let max_fl_stack = fl_parents.iter()
        .map(|p| fl_by_parent.get(p).map_or(0, |v| v.len()))
        .max().unwrap_or(1);
    let fl_col_w  = NW + FL_COL_GAP;
    let fl_row_h  = NH + FL_ROW_GAP;

    let fl_has_proj = use_pca && layers[3].iter().any(|nd| nd.proj_x.is_some());

    let n_fl = layers[3].len().max(1);
    let fl_scatter_cols = ((n_fl as f64).sqrt().ceil() as usize).max(2);
    let fl_scatter_rows = ((n_fl as f64 / fl_scatter_cols as f64).ceil() as usize).max(1);
    let fl_scatter_w = ((fl_scatter_cols as f64) * (NW + 12.0)).max(380.0);
    let fl_scatter_h = ((fl_scatter_rows as f64) * (NH + 14.0)).max(150.0);

    let (fl_grid_w, fl_grid_h) = if fl_has_proj {
        (fl_scatter_w, fl_scatter_h)
    } else {
        (
            n_fl_cols as f64 * fl_col_w - FL_COL_GAP,
            FL_HDR_H + max_fl_stack as f64 * fl_row_h - FL_ROW_GAP,
        )
    };

    // ── SVG width ─────────────────────────────────────────────────────────
    let tl_total_w  = DAY_LBL_W + tl_grid_w + SIDE * 2.0;
    let eeg_total_w = DAY_LBL_W + eeg_grid_w + SIDE * 2.0;
    let fl_total_w  = fl_grid_w  + SIDE * 2.0;
    let svg_w = (QR * 2.0 + SIDE * 2.0)
        .max(if has_tl  { tl_total_w  } else { 0.0 })
        .max(if has_eeg { eeg_total_w } else { 0.0 })
        .max(if has_fl  { fl_total_w  } else { 0.0 });

    // ── Y positions ───────────────────────────────────────────────────────
    let query_y     = TOP;
    let tl_band_top = query_y + (QR + 8.0) + BAND_GAP;
    let tl_grid_top = tl_band_top + BAND_PAD + HOUR_LBL_H;
    let tl_band_bot = tl_grid_top + tl_grid_h + BAND_PAD;
    let eeg_band_top = if has_tl { tl_band_bot + BAND_GAP }
                       else       { tl_band_top };
    let eeg_grid_top = eeg_band_top + BAND_PAD + HOUR_LBL_H;
    let eeg_band_bot = eeg_grid_top + n_eeg_days as f64 * EEG_CELL_H + BAND_PAD;
    let fl_band_top = if has_eeg      { eeg_band_bot + BAND_GAP }
                      else if has_tl  { tl_band_bot  + BAND_GAP }
                      else            { tl_band_top };
    let fl_grid_top = fl_band_top + BAND_PAD;
    let fl_band_bot = fl_grid_top + fl_grid_h + BAND_PAD;
    let svg_h = fl_band_bot + 56.0;

    // ── Centre positions ──────────────────────────────────────────────────
    let mut pos: std::collections::HashMap<String, (f64, f64)> = Default::default();

    for nd in &layers[0] {
        pos.insert(nd.id.clone(), (svg_w / 2.0, query_y));
    }

    if has_tl {
        let block_w  = DAY_LBL_W + tl_grid_w;
        let cells_x0 = (svg_w - block_w) / 2.0 + DAY_LBL_W;
        let mut cell_slots: std::collections::HashMap<(usize, usize), usize> = Default::default();
        for (nd, (date, hour, _)) in layers[1].iter().zip(tl_info.iter()) {
            let col  = tl_hour_idx[hour];
            let row  = tl_day_idx[date.as_str()];
            let slot = *cell_slots.entry((row, col)).or_insert(0);
            cell_slots.entry((row, col)).and_modify(|s| *s += 1);
            let cx = cells_x0 + col as f64 * tl_col_w + NW / 2.0;
            let cy = tl_grid_top + row as f64 * tl_cell_h
                   + TL_CELL_PAD + slot as f64 * (NH + TL_ROW_GAP) + NH / 2.0;
            pos.insert(nd.id.clone(), (cx, cy));
        }
    }

    if has_eeg {
        let day_idx:  std::collections::HashMap<&str, usize> =
            eeg_days.iter().enumerate().map(|(i, d)| (d.as_str(), i)).collect();
        let hour_idx: std::collections::HashMap<u32, usize>  =
            eeg_hours.iter().enumerate().map(|(i, &h)| (h, i)).collect();
        let block_w  = DAY_LBL_W + eeg_grid_w;
        let cells_x0 = (svg_w - block_w) / 2.0 + DAY_LBL_W;
        for (nd, (date, hour, min)) in layers[2].iter().zip(eeg_info.iter()) {
            let col = hour_idx[hour];
            let row = day_idx[date.as_str()];
            let cell_cx = cells_x0 + col as f64 * EEG_CELL_W + EEG_CELL_W / 2.0;
            let cell_cy = eeg_grid_top + row as f64 * EEG_CELL_H + EEG_CELL_H / 2.0;
            let jitter  = (*min as f64 / 59.0 - 0.5) * (EEG_CELL_W - EEG_S * 3.0).max(0.0);
            pos.insert(nd.id.clone(), (cell_cx + jitter, cell_cy));
        }
    }

    if has_fl {
        if fl_has_proj {
            let pairs: Vec<(f32, f32)> = layers[3].iter()
                .map(|nd| (nd.proj_x.unwrap_or(0.0), nd.proj_y.unwrap_or(0.0)))
                .collect();
            let px_min = pairs.iter().map(|&(x, _)| x).fold(f32::MAX, f32::min);
            let px_max = pairs.iter().map(|&(x, _)| x).fold(f32::MIN, f32::max);
            let py_min = pairs.iter().map(|&(_, y)| y).fold(f32::MAX, f32::min);
            let py_max = pairs.iter().map(|&(_, y)| y).fold(f32::MIN, f32::max);
            let px_range = ((px_max - px_min) as f64).max(0.01);
            let py_range = ((py_max - py_min) as f64).max(0.01);
            let margin_x = NW / 2.0 + 6.0;
            let margin_y = NH / 2.0 + 6.0;
            let usable_w  = fl_scatter_w - margin_x * 2.0;
            let usable_h  = fl_scatter_h - margin_y * 2.0;
            let scatter_x0 = (svg_w - fl_scatter_w) / 2.0 + margin_x;
            let scatter_y0 = fl_grid_top + margin_y;
            let cx_min = scatter_x0;
            let cx_max = scatter_x0 + usable_w;
            let cy_min = scatter_y0;
            let cy_max = scatter_y0 + usable_h;

            let mut raw_pos: Vec<(f64, f64)> = pairs.iter().map(|&(px, py)| {
                let cx = scatter_x0 + (px - px_min) as f64 / px_range * usable_w;
                let cy = scatter_y0 + (py - py_min) as f64 / py_range * usable_h;
                (cx, cy)
            }).collect();

            separate_labels_svg(&mut raw_pos, NW, NH, cx_min, cx_max, cy_min, cy_max);

            for (nd, &(cx, cy)) in layers[3].iter().zip(raw_pos.iter()) {
                pos.insert(nd.id.clone(), (cx, cy));
            }
        } else {
            let x0 = (svg_w - fl_grid_w) / 2.0 + NW / 2.0;
            for (ci, parent_id) in fl_parents.iter().enumerate() {
                let cx = x0 + ci as f64 * fl_col_w;
                if let Some(group) = fl_by_parent.get(parent_id) {
                    for (ri, nd) in group.iter().enumerate() {
                        let cy = fl_grid_top + FL_HDR_H + ri as f64 * fl_row_h + NH / 2.0;
                        pos.insert(nd.id.clone(), (cx, cy));
                    }
                }
            }
        }
    }

    // ── Colour helpers ────────────────────────────────────────────────────
    let eeg_ts: Vec<u64> = nodes.iter()
        .filter(|n| n.kind == "eeg_point").filter_map(|n| n.timestamp_unix).collect();
    let ts_min = eeg_ts.iter().copied().min().unwrap_or(0);
    let ts_rng = eeg_ts.iter().copied().max().unwrap_or(1).saturating_sub(ts_min).max(1) as f64;
    let eeg_fill = |ts: Option<u64>| -> String {
        ts.map(|t| turbo_hex((t.saturating_sub(ts_min)) as f64 / ts_rng))
          .unwrap_or_else(|| "#f59e0b".into())
    };
    let node_fill = |nd: &InteractiveGraphNode| -> String {
        match nd.kind.as_str() {
            "query"       => "#8b5cf6".into(),
            "text_label"  => "#3b82f6".into(),
            "eeg_point"   => eeg_fill(nd.timestamp_unix),
            "found_label" => "#10b981".into(),
            _             => "#888888".into(),
        }
    };
    let half_h = |kind: &str| -> f64 {
        match kind { "query" => QR, "eeg_point" => EEG_S, _ => NH / 2.0 }
    };
    let edge_col = |kind: &str| -> (&str, &str, &str) {
        match kind {
            "text_sim"   => ("#8b5cf6", "",    "mv"),
            "eeg_bridge" => ("#f59e0b", "5,3", "ma"),
            "eeg_sim"    => ("#f59e0b", "2,3", "ma"),
            "label_prox" => ("#10b981", "",    "me"),
            _            => ("#999999", "",    "mg"),
        }
    };

    // ── SVG document ──────────────────────────────────────────────────────
    let mut o = String::with_capacity(64 * 1024);
    let w = svg_w.ceil() as i64;
    let h = svg_h.ceil() as i64;

    o.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Helvetica Neue,Helvetica,Arial,sans-serif">
  <rect width="{w}" height="{h}" fill="#ffffff"/>
  <defs>
"##));
    for (id, col) in [("mv","#8b5cf6"),("ma","#f59e0b"),("me","#10b981"),("mg","#999999")] {
        o.push_str(&format!(
            "    <marker id=\"{id}\" markerWidth=\"7\" markerHeight=\"5\" refX=\"6\" refY=\"2.5\" orient=\"auto\" markerUnits=\"strokeWidth\">\
             <path d=\"M0,0 L7,2.5 L0,5 Z\" fill=\"{col}\"/></marker>\n"));
    }
    o.push_str("  </defs>\n");

    // ── Layer bands ───────────────────────────────────────────────────────
    if !layers[0].is_empty() {
        let by = query_y - (QR + 8.0);
        let bh = (QR + 8.0) * 2.0;
        o.push_str(&format!(
            "  <rect x=\"0\" y=\"{by:.1}\" width=\"{w}\" height=\"{bh:.1}\" fill=\"#8b5cf6\" fill-opacity=\"0.05\" rx=\"4\"/>\n\
             <text x=\"10\" y=\"{:.1}\" font-size=\"9\" fill=\"#8b5cf6\" opacity=\"0.55\" font-weight=\"600\" letter-spacing=\"1\">{}</text>\n",
            by + 13.0, svg_esc(&labels.layer_query)));
    }
    if has_tl {
        let bh = tl_band_bot - tl_band_top;
        o.push_str(&format!(
            "  <rect x=\"0\" y=\"{tl_band_top:.1}\" width=\"{w}\" height=\"{bh:.1}\" \
             fill=\"#3b82f6\" fill-opacity=\"0.05\" rx=\"4\"/>\n\
             <text x=\"10\" y=\"{:.1}\" font-size=\"9\" fill=\"#3b82f6\" opacity=\"0.55\" \
             font-weight=\"600\" letter-spacing=\"1\">{}</text>\n",
            tl_band_top + 13.0, svg_esc(&labels.layer_text_matches)));

        let block_w  = DAY_LBL_W + tl_grid_w;
        let cells_x0 = (svg_w - block_w) / 2.0 + DAY_LBL_W;
        let grid_bot = tl_grid_top + tl_grid_h;

        for (ci, &hour) in tl_hours.iter().enumerate() {
            let hx = cells_x0 + ci as f64 * tl_col_w + NW / 2.0;
            o.push_str(&format!(
                "  <text x=\"{hx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
                 font-size=\"8\" fill=\"#3b82f6\" opacity=\"0.75\">{hour:02}h</text>\n",
                tl_band_top + BAND_PAD + HOUR_LBL_H - 3.0));
        }
        let day_lbl_x = cells_x0 - 6.0;
        for (ri, day) in tl_days.iter().enumerate() {
            let row_top = tl_grid_top + ri as f64 * tl_cell_h;
            let row_cy  = row_top + tl_cell_h / 2.0;
            o.push_str(&format!(
                "  <text x=\"{day_lbl_x:.1}\" y=\"{row_cy:.1}\" text-anchor=\"end\" \
                 dominant-baseline=\"middle\" font-size=\"8\" fill=\"#999\">{}</text>\n",
                svg_esc(&day[5..])));
            if ri > 0 {
                o.push_str(&format!(
                    "  <line x1=\"{cells_x0:.1}\" y1=\"{row_top:.1}\" \
                     x2=\"{:.1}\" y2=\"{row_top:.1}\" \
                     stroke=\"#3b82f6\" stroke-opacity=\"0.2\" stroke-width=\"1\"/>\n",
                    cells_x0 + tl_grid_w));
            }
        }
        for ci in 0..=n_tl_hours {
            let lx = cells_x0 + ci as f64 * tl_col_w;
            o.push_str(&format!(
                "  <line x1=\"{lx:.1}\" y1=\"{tl_grid_top:.1}\" \
                 x2=\"{lx:.1}\" y2=\"{grid_bot:.1}\" \
                 stroke=\"#3b82f6\" stroke-opacity=\"0.2\" stroke-width=\"1\"/>\n"));
        }
    }
    if has_eeg {
        let by = eeg_band_top;
        let bh = eeg_band_bot - by;
        o.push_str(&format!(
            "  <rect x=\"0\" y=\"{by:.1}\" width=\"{w}\" height=\"{bh:.1}\" fill=\"#f59e0b\" fill-opacity=\"0.05\" rx=\"4\"/>\n\
             <text x=\"10\" y=\"{:.1}\" font-size=\"9\" fill=\"#f59e0b\" opacity=\"0.55\" font-weight=\"600\" letter-spacing=\"1\">{}</text>\n",
            by + 13.0, svg_esc(&labels.layer_eeg_neighbors)));

        let block_w  = DAY_LBL_W + eeg_grid_w;
        let cells_x0 = (svg_w - block_w) / 2.0 + DAY_LBL_W;
        let grid_bot = eeg_grid_top + n_eeg_days as f64 * EEG_CELL_H;

        for (ci, &hour) in eeg_hours.iter().enumerate() {
            let hx = cells_x0 + ci as f64 * EEG_CELL_W + EEG_CELL_W / 2.0;
            o.push_str(&format!(
                "  <text x=\"{hx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
                 font-size=\"8\" fill=\"#f59e0b\" opacity=\"0.75\">{hour:02}h</text>\n",
                eeg_band_top + BAND_PAD + HOUR_LBL_H - 3.0));
        }
        let day_lbl_x = cells_x0 - 6.0;
        for (ri, day) in eeg_days.iter().enumerate() {
            let row_cy = eeg_grid_top + ri as f64 * EEG_CELL_H + EEG_CELL_H / 2.0;
            o.push_str(&format!(
                "  <text x=\"{day_lbl_x:.1}\" y=\"{row_cy:.1}\" text-anchor=\"end\" \
                 dominant-baseline=\"middle\" font-size=\"8\" fill=\"#999\">{}</text>\n",
                svg_esc(&day[5..])));
            if ri > 0 {
                let ry = eeg_grid_top + ri as f64 * EEG_CELL_H;
                o.push_str(&format!(
                    "  <line x1=\"{cells_x0:.1}\" y1=\"{ry:.1}\" \
                     x2=\"{:.1}\" y2=\"{ry:.1}\" \
                     stroke=\"#f59e0b\" stroke-opacity=\"0.2\" stroke-width=\"1\"/>\n",
                    cells_x0 + eeg_grid_w));
            }
        }
        for ci in 0..=n_eeg_hours {
            let lx = cells_x0 + ci as f64 * EEG_CELL_W;
            o.push_str(&format!(
                "  <line x1=\"{lx:.1}\" y1=\"{eeg_grid_top:.1}\" \
                 x2=\"{lx:.1}\" y2=\"{grid_bot:.1}\" \
                 stroke=\"#f59e0b\" stroke-opacity=\"0.2\" stroke-width=\"1\"/>\n"));
        }
    }
    if has_fl {
        let bh = fl_band_bot - fl_band_top;
        o.push_str(&format!(
            "  <rect x=\"0\" y=\"{fl_band_top:.1}\" width=\"{w}\" height=\"{bh:.1}\" \
             fill=\"#10b981\" fill-opacity=\"0.05\" rx=\"4\"/>\n\
             <text x=\"10\" y=\"{:.1}\" font-size=\"9\" fill=\"#10b981\" opacity=\"0.55\" \
             font-weight=\"600\" letter-spacing=\"1\">{}</text>\n",
            fl_band_top + 13.0, svg_esc(&labels.layer_found_labels)));

        if fl_has_proj {
            let scatter_left = (svg_w - fl_scatter_w) / 2.0;
            let scatter_top  = fl_grid_top;
            let scatter_bot  = fl_grid_top + fl_scatter_h;
            let scatter_cx   = svg_w / 2.0;
            let scatter_cy   = fl_grid_top + fl_scatter_h / 2.0;

            o.push_str(&format!(
                "  <rect x=\"{scatter_left:.1}\" y=\"{scatter_top:.1}\" \
                 width=\"{fl_scatter_w:.1}\" height=\"{fl_scatter_h:.1}\" \
                 rx=\"4\" fill=\"none\" stroke=\"#10b981\" stroke-opacity=\"0.18\" \
                 stroke-width=\"1\"/>\n"));
            o.push_str(&format!(
                "  <line x1=\"{:.1}\" y1=\"{scatter_cy:.1}\" \
                 x2=\"{:.1}\" y2=\"{scatter_cy:.1}\" \
                 stroke=\"#10b981\" stroke-opacity=\"0.12\" stroke-width=\"1\" \
                 stroke-dasharray=\"3,3\"/>\n",
                scatter_left + 8.0, scatter_left + fl_scatter_w - 8.0));
            o.push_str(&format!(
                "  <line x1=\"{scatter_cx:.1}\" y1=\"{:.1}\" \
                 x2=\"{scatter_cx:.1}\" y2=\"{:.1}\" \
                 stroke=\"#10b981\" stroke-opacity=\"0.12\" stroke-width=\"1\" \
                 stroke-dasharray=\"3,3\"/>\n",
                scatter_top + 8.0, scatter_bot - 8.0));
            o.push_str(&format!(
                "  <text x=\"{scatter_cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
                 font-size=\"6.5\" fill=\"#10b981\" opacity=\"0.40\"\
                 >← text embedding similarity →</text>\n",
                scatter_bot - 2.5));
        } else {
            let x0_col0 = (svg_w - fl_grid_w) / 2.0;
            for (ci, parent_id) in fl_parents.iter().enumerate() {
                let col_left = x0_col0 + ci as f64 * fl_col_w;
                let col_cx   = col_left + NW / 2.0;
                let hdr_y    = fl_grid_top + FL_HDR_H - 3.0;

                if ci % 2 == 0 {
                    o.push_str(&format!(
                        "  <rect x=\"{col_left:.1}\" y=\"{fl_grid_top:.1}\" \
                         width=\"{NW:.1}\" height=\"{:.1}\" \
                         fill=\"#10b981\" fill-opacity=\"0.04\" rx=\"3\"/>\n",
                        fl_grid_h));
                }

                let hdr_text = parent_id.strip_prefix("ep_")
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|ts| {
                        let dt = fmt_unix_utc(ts);
                        format!("{} {}", &dt[5..10], &dt[11..])
                    })
                    .unwrap_or_default();
                o.push_str(&format!(
                    "  <text x=\"{col_cx:.1}\" y=\"{hdr_y:.1}\" text-anchor=\"middle\" \
                     font-size=\"7.5\" fill=\"#10b981\" opacity=\"0.75\">{}</text>\n",
                    svg_esc(&hdr_text)));

                if ci > 0 {
                    o.push_str(&format!(
                        "  <line x1=\"{col_left:.1}\" y1=\"{fl_grid_top:.1}\" \
                         x2=\"{col_left:.1}\" y2=\"{:.1}\" \
                         stroke=\"#10b981\" stroke-opacity=\"0.2\" stroke-width=\"1\"/>\n",
                        fl_grid_top + fl_grid_h));
                }
            }
        }
    }

    // ── Edges ─────────────────────────────────────────────────────────────
    for e in edges {
        let (Some(&(x1,y1)), Some(&(x2,y2))) = (pos.get(&e.from_id), pos.get(&e.to_id))
            else { continue };
        let dx = x2 - x1; let dy = y2 - y1;
        let len = (dx*dx + dy*dy).sqrt().max(1.0);
        let src_h = nodes.iter().find(|n| n.id == e.from_id).map(|n| half_h(&n.kind)).unwrap_or(NH / 2.0);
        let dst_h = nodes.iter().find(|n| n.id == e.to_id  ).map(|n| half_h(&n.kind)).unwrap_or(NH / 2.0);
        let sx1 = x1 + dx/len*(src_h + 2.0); let sy1 = y1 + dy/len*(src_h + 2.0);
        let sx2 = x2 - dx/len*(dst_h + 9.0); let sy2 = y2 - dy/len*(dst_h + 9.0);
        let midy = (sy1 + sy2) / 2.0;
        let cp1y = sy1 + (midy - sy1) * 0.55;
        let cp2y = sy2 - (sy2 - midy) * 0.55;
        let (col, dash, mid) = edge_col(&e.kind);
        let da = if dash.is_empty() { String::new() }
                 else { format!(" stroke-dasharray=\"{dash}\"") };
        o.push_str(&format!(
            "  <path d=\"M{sx1:.1},{sy1:.1} C{x1:.1},{cp1y:.1} {x2:.1},{cp2y:.1} {sx2:.1},{sy2:.1}\" \
             fill=\"none\" stroke=\"{col}\" stroke-width=\"1.8\" opacity=\"0.65\"{da} marker-end=\"url(#{mid})\"/>\n"));
    }

    // ── Nodes ─────────────────────────────────────────────────────────────
    for nd in nodes {
        let Some(&(cx, cy)) = pos.get(&nd.id) else { continue };
        let fill = node_fill(nd);

        match nd.kind.as_str() {
            "query" => {
                o.push_str(&format!(
                    "  <circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{ro:.1}\" fill=\"{fill}\" fill-opacity=\"0.18\" stroke=\"{fill}\" stroke-width=\"2\"/>\n\
                     <circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{QR:.1}\" fill=\"{fill}\" fill-opacity=\"0.92\"/>\n",
                    ro = QR + 8.0));
            }
            "text_label" => {
                o.push_str(&format!(
                    "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{NW:.1}\" height=\"{NH:.1}\" rx=\"6\" \
                     fill=\"{fill}\" fill-opacity=\"0.90\"/>\n",
                    cx - NW / 2.0, cy - NH / 2.0));
            }
            "found_label" => {
                o.push_str(&format!(
                    "  <ellipse cx=\"{cx:.1}\" cy=\"{cy:.1}\" rx=\"{:.1}\" ry=\"{:.1}\" \
                     fill=\"{fill}\" fill-opacity=\"0.90\"/>\n",
                    NW / 2.0, NH / 2.0));
            }
            "eeg_point" => {
                let s = EEG_S;
                o.push_str(&format!(
                    "  <polygon points=\"{cx:.1},{:.1} {:.1},{cy:.1} {cx:.1},{:.1} {:.1},{cy:.1}\" \
                     fill=\"{fill}\" fill-opacity=\"0.92\"/>\n",
                    cy - s, cx + s * 1.35, cy + s, cx - s * 1.35));
            }
            _ => {}
        }

        match nd.kind.as_str() {
            "eeg_point" => {
                let time_str = nd.timestamp_unix.map(|ts| {
                    let h = (ts % 86400) / 3600;
                    let m = (ts % 3600)  / 60;
                    format!("{h:02}:{m:02}")
                }).unwrap_or_default();
                o.push_str(&format!(
                    "  <text x=\"{cx:.1}\" y=\"{cy:.1}\" text-anchor=\"middle\" \
                     dominant-baseline=\"middle\" font-size=\"7\" font-weight=\"600\" fill=\"white\">{}</text>\n",
                    svg_esc(&time_str)));
            }
            _ => {
                let primary = trunc(nd.text.as_deref().unwrap_or(""), 20);
                let has_sub = nd.timestamp_unix.is_some()
                    && matches!(nd.kind.as_str(), "text_label" | "found_label");
                let ty = if has_sub { cy - 7.0 } else { cy };
                o.push_str(&format!(
                    "  <text x=\"{cx:.1}\" y=\"{ty:.1}\" text-anchor=\"middle\" \
                     dominant-baseline=\"middle\" font-size=\"10\" font-weight=\"600\" fill=\"white\">{}</text>\n",
                    svg_esc(&primary)));
                if has_sub {
                    if let Some(ts) = nd.timestamp_unix {
                        o.push_str(&format!(
                            "  <text x=\"{cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
                             dominant-baseline=\"middle\" font-size=\"7.5\" fill=\"white\" opacity=\"0.72\">{}</text>\n",
                            cy + 8.5, svg_esc(&fmt_unix_utc(ts))));
                    }
                }
            }
        }
    }

    // ── Legend ────────────────────────────────────────────────────────────
    let legend_y = svg_h - 30.0;
    let legend_items = [
        ("#8b5cf6", labels.legend_query.as_str()),
        ("#3b82f6", labels.legend_text.as_str()),
        ("#f59e0b", labels.legend_eeg.as_str()),
        ("#10b981", labels.legend_found.as_str()),
    ];
    let lw = 72.0_f64;
    let lx0 = (svg_w - lw * legend_items.len() as f64) / 2.0;
    for (i, (col, lbl)) in legend_items.iter().enumerate() {
        let x = lx0 + i as f64 * lw;
        o.push_str(&format!(
            "  <circle cx=\"{:.1}\" cy=\"{legend_y:.1}\" r=\"4.5\" fill=\"{col}\" opacity=\"0.85\"/>\n\
             <text x=\"{:.1}\" y=\"{legend_y:.1}\" dominant-baseline=\"middle\" font-size=\"8.5\" fill=\"#555\">{}</text>\n",
            x + 4.5, x + 13.0, svg_esc(lbl)));
    }

    let footer_y = svg_h - 12.0;
    o.push_str(&format!(
        "  <text x=\"{:.1}\" y=\"{footer_y:.1}\" text-anchor=\"middle\" \
         font-size=\"7.5\" fill=\"#aaa\">{}</text>\n",
        svg_w / 2.0, svg_esc(&labels.generated_by)));

    o.push_str("</svg>\n");
    o
}

/// Fetch labels from `labels.sqlite` whose EEG window contains `ts_unix`,
/// or — if none — labels whose window starts within `window_secs` of it.
pub fn get_labels_near(labels_db: &Path, ts_unix: u64, window_secs: u64) -> Vec<LabelEntry> {
    let Ok(conn) = skill_data::util::open_readonly(labels_db) else { return vec![] };

    let ts  = ts_unix as i64;
    let lo  = ts_unix.saturating_sub(window_secs) as i64;
    let hi  = (ts_unix.saturating_add(window_secs)) as i64;

    let Ok(mut stmt) = conn.prepare(
        "SELECT id, eeg_start, eeg_end, label_start, label_end, text
         FROM labels
         WHERE (eeg_start <= ?1 AND eeg_end >= ?1)
            OR (eeg_start BETWEEN ?2 AND ?3)
         ORDER BY ABS(CAST(eeg_start AS INTEGER) - ?4)
         LIMIT 5",
    ) else { return vec![] };

    stmt.query_map(params![ts, lo, hi, ts], |row| {
        Ok(LabelEntry {
            id:          row.get(0)?,
            eeg_start:   row.get::<_, i64>(1)? as u64,
            eeg_end:     row.get::<_, i64>(2)? as u64,
            label_start: row.get::<_, i64>(3)? as u64,
            label_end:   row.get::<_, i64>(4)? as u64,
            text:        row.get(5)?,
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

// ── PCA helpers ────────────────────────────────────────────────────────────

/// Fetch the `text_embedding` BLOB for one label (read-only, no metrics).
pub fn get_found_label_embedding(labels_db: &Path, label_id: i64) -> Option<Vec<f32>> {
    let conn = Connection::open_with_flags(
        labels_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ).ok()?;
    let blob: Option<Vec<u8>> = conn.query_row(
        "SELECT text_embedding FROM labels WHERE id = ?1",
        params![label_id],
        |row| row.get(0),
    ).ok()?;
    let blob = blob?;
    if blob.len() < 4 { return None; }
    Some(blob.chunks_exact(4)
         .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
         .collect())
}

/// 2-component PCA via covariance-free power iteration.
///
/// Returns one `(x, y)` per input, normalised so every axis spans [-1, 1].
pub fn pca_2d(embeddings: &[Vec<f32>]) -> Vec<(f32, f32)> {
    let n = embeddings.len();
    if n == 0 { return vec![]; }
    if n == 1 { return vec![(0.0, 0.0)]; }
    let d = embeddings[0].len();
    if d < 2  { return vec![(0.0, 0.0); n]; }

    let inv_n = 1.0 / n as f32;
    let mut mean = vec![0f32; d];
    for emb in embeddings { for (j, &v) in emb.iter().enumerate() { mean[j] += v * inv_n; } }
    let centered: Vec<Vec<f32>> = embeddings.iter()
        .map(|emb| emb.iter().zip(&mean).map(|(&v, &m)| v - m).collect())
        .collect();

    fn dot(a: &[f32], b: &[f32]) -> f32 { a.iter().zip(b).map(|(&x, &y)| x * y).sum() }

    fn cov_mul(c: &[Vec<f32>], v: &[f32]) -> Vec<f32> {
        let xv: Vec<f32> = c.iter().map(|row| dot(row, v)).collect();
        let mut res = vec![0f32; v.len()];
        for (row, &coeff) in c.iter().zip(&xv) {
            for (r, &x) in res.iter_mut().zip(row) { *r += x * coeff; }
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

    let centered2: Vec<Vec<f32>> = centered.iter().map(|v| {
        let p = dot(v, &pc1);
        v.iter().zip(&pc1).map(|(&vi, &pi)| vi - p * pi).collect()
    }).collect();
    let norm2 = dot(&centered2[0], &centered2[0]).sqrt();
    let init2 = if norm2 > 1e-12 {
        centered2[0].iter().map(|&v| v / norm2).collect::<Vec<_>>()
    } else {
        let mut perp = vec![0f32; d];
        if d > 1 { perp[1] = 1.0; }
        perp
    };
    let pc2 = power_iter(&centered2, init2);

    let coords: Vec<(f32, f32)> = centered.iter()
        .map(|v| (dot(v, &pc1), dot(v, &pc2)))
        .collect();
    let x_min = coords.iter().map(|&(x, _)| x).fold(f32::MAX, f32::min);
    let x_max = coords.iter().map(|&(x, _)| x).fold(f32::MIN, f32::max);
    let y_min = coords.iter().map(|&(_, y)| y).fold(f32::MAX, f32::min);
    let y_max = coords.iter().map(|&(_, y)| y).fold(f32::MIN, f32::max);
    let xr = (x_max - x_min).max(1e-6);
    let yr = (y_max - y_min).max(1e-6);
    coords.iter().map(|&(x, y)| (
        (x - x_min) / xr * 2.0 - 1.0,
        (y - y_min) / yr * 2.0 - 1.0,
    )).collect()
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
    let tod  = secs % 86400;
    let h    = tod / 3600;
    let m    = (tod % 3600) / 60;
    let s    = tod % 60;
    let z    = (secs / 86400) as i64 + 719_468;
    let era  = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe  = z - era * 146_097;
    let yoe  = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y    = yoe + era * 400;
    let doy  = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp   = (5 * doy + 2) / 153;
    let d    = doy - (153 * mp + 2) / 5 + 1;
    let mo   = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr   = if mo <= 2 { y + 1 } else { y };
    format!("{yr:04}{mo:02}{d:02}_{h:02}{m:02}{s:02}")
}
