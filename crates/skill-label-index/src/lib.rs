// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Cross-modal label HNSW indices.
//!
//! Two independent indices are maintained:
//!
//! * **Text index** (`label_text_index.hnsw`): one node per label that has a
//!   `text_embedding` in `labels.sqlite`.  Vectors live in the fastembed
//!   embedding space (e.g. 384-dim for bge-small-en-v1.5).
//!
//!   Query with: a fastembed vector from a free-text search string.
//!
//! * **EEG index** (`label_eeg_index.hnsw`): one node per label whose EEG
//!   time window (`eeg_start … eeg_end`) overlaps with at least one recorded
//!   epoch in the daily `eeg.sqlite` files.  The vector is the *mean-pooled*
//!   EEG embedding across all epochs in that window (zuna-rs space).
//!
//!   Query with: an EEG embedding from the current session or from history.
//!
//! Both indices store `label_id: i64` as the HNSW payload so results can be
//! joined back to `labels.sqlite` for full hydration.

use std::{path::Path, sync::Mutex};

use fast_hnsw::{distance::Cosine, labeled::LabeledIndex, Builder};
use rusqlite::params;
use serde::Serialize;

use skill_commands::NeighborMetrics;
use skill_constants::{
    HNSW_EF_CONSTRUCTION, HNSW_M, LABELS_FILE, LABEL_CONTEXT_INDEX_FILE, LABEL_EEG_INDEX_FILE, LABEL_TEXT_INDEX_FILE,
    SQLITE_FILE,
};
use skill_data::util::MutexExt;

// Local aliases for readability.
const TEXT_INDEX_FILE: &str = LABEL_TEXT_INDEX_FILE;
const CONTEXT_INDEX_FILE: &str = LABEL_CONTEXT_INDEX_FILE;
const EEG_INDEX_FILE: &str = LABEL_EEG_INDEX_FILE;
const HNSW_EF: usize = HNSW_EF_CONSTRUCTION;

fn fresh_index() -> LabeledIndex<Cosine, i64> {
    Builder::new().m(HNSW_M).ef_construction(HNSW_EF).build_labeled(Cosine)
}

fn load_or_fresh(path: &Path) -> LabeledIndex<Cosine, i64> {
    if path.exists() {
        match LabeledIndex::load(path, Cosine) {
            Ok(idx) => {
                eprintln!("[label_idx] loaded {} ({} nodes)", path.display(), idx.len());
                idx
            }
            Err(e) => {
                eprintln!("[label_idx] load failed ({e}), starting fresh");
                fresh_index()
            }
        }
    } else {
        fresh_index()
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct LabelIndexState {
    pub text: Mutex<Option<LabeledIndex<Cosine, i64>>>,
    pub context: Mutex<Option<LabeledIndex<Cosine, i64>>>,
    pub eeg: Mutex<Option<LabeledIndex<Cosine, i64>>>,
}

impl Default for LabelIndexState {
    fn default() -> Self {
        Self {
            text: Mutex::new(None),
            context: Mutex::new(None),
            eeg: Mutex::new(None),
        }
    }
}

impl LabelIndexState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load (or create) all three indices from `skill_dir`.  Called on startup.
    pub fn load(&self, skill_dir: &Path) {
        let text_path = skill_dir.join(TEXT_INDEX_FILE);
        let context_path = skill_dir.join(CONTEXT_INDEX_FILE);
        let eeg_path = skill_dir.join(EEG_INDEX_FILE);
        *self.text.lock_or_recover() = Some(load_or_fresh(&text_path));
        *self.context.lock_or_recover() = Some(load_or_fresh(&context_path));
        *self.eeg.lock_or_recover() = Some(load_or_fresh(&eeg_path));
    }
}

// ── Result types ──────────────────────────────────────────────────────────────

/// One label returned by either index search, fully hydrated.
#[derive(Debug, Serialize, Clone)]
pub struct LabelNeighbor {
    pub label_id: i64,
    pub text: String,
    pub context: String,
    pub eeg_start: u64,
    pub eeg_end: u64,
    pub created_at: u64,
    /// fastembed model code that produced `text_embedding` / `context_embedding`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    /// Cosine distance in the queried space (0 = identical, 2 = opposite).
    pub distance: f32,
    /// Mean EEG metrics averaged across the label's EEG time window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eeg_metrics: Option<NeighborMetrics>,
}

/// Statistics returned after a (re-)build.
#[derive(Debug, Serialize)]
pub struct RebuildStats {
    pub text_nodes: usize,
    pub eeg_nodes: usize,
    /// Labels that had no EEG data in their time window.
    pub eeg_skipped: usize,
}

// ── Low-level helpers ─────────────────────────────────────────────────────────

// Shared blob→f32 helper from skill-data; also removes the date_dirs wrapper.
use skill_data::util::blob_to_f32;

/// Fetch EEG embeddings from every `eeg.sqlite` whose date overlaps
/// `[eeg_start, eeg_end]` (unix seconds) and return their component-wise mean.
/// Returns `None` if no epochs exist in that window.
pub fn mean_eeg_for_window(skill_dir: &Path, eeg_start: u64, eeg_end: u64) -> Option<Vec<f32>> {
    let ts_start = (eeg_start as i64) * 1000;
    let ts_end = (eeg_end as i64) * 1000;

    let mut sum: Vec<f32> = Vec::new();
    let mut count = 0usize;

    for (_date, dir) in skill_data::util::date_dirs(skill_dir) {
        let db_path = dir.join(SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }
        let Ok(conn) = skill_data::util::open_readonly(&db_path) else {
            continue;
        };

        let Ok(mut stmt) = conn.prepare(
            "SELECT eeg_embedding FROM embeddings \
             WHERE timestamp >= ?1 AND timestamp <= ?2",
        ) else {
            continue;
        };

        let rows: Vec<Vec<u8>> = stmt
            .query_map(params![ts_start, ts_end], |row| row.get::<_, Vec<u8>>(0))
            .ok()?
            .filter_map(std::result::Result::ok)
            .collect();

        for blob in rows {
            let v = blob_to_f32(&blob);
            if v.is_empty() {
                continue;
            }
            if sum.is_empty() {
                sum.resize(v.len(), 0.0);
            }
            if v.len() != sum.len() {
                continue;
            } // dimension mismatch — skip
            for (s, &x) in sum.iter_mut().zip(v.iter()) {
                *s += x;
            }
            count += 1;
        }
    }

    if count == 0 {
        return None;
    }
    let scale = 1.0 / count as f32;
    Some(sum.iter().map(|&s| s * scale).collect())
}

/// Fetch EEG metrics averaged over `[eeg_start, eeg_end]` for label hydration.
fn mean_metrics_for_window(skill_dir: &Path, eeg_start: u64, eeg_end: u64) -> Option<NeighborMetrics> {
    let ts_start = (eeg_start as i64) * 1000;
    let ts_end = (eeg_end as i64) * 1000;

    // Accumulators
    let mut relax = 0f64;
    let mut engage = 0f64;
    let mut faa = 0f64;
    let mut tar = 0f64;
    let mut mood = 0f64;
    let mut meditation = 0f64;
    let mut cog_load = 0f64;
    let mut drowsy = 0f64;
    let mut hr = 0f64;
    let mut snr = 0f64;
    let mut rel_alpha = 0f64;
    let mut rel_beta = 0f64;
    let mut rel_theta = 0f64;
    let mut count = 0u64;

    for (_date, dir) in skill_data::util::date_dirs(skill_dir) {
        let db_path = dir.join(SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }
        let Ok(conn) = skill_data::util::open_readonly(&db_path) else {
            continue;
        };

        let Ok(mut stmt) = conn.prepare(
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
                    json_extract(metrics_json, '$.rel_theta')
             FROM embeddings WHERE timestamp >= ?1 AND timestamp <= ?2",
        ) else {
            continue;
        };

        let _ = stmt
            .query_map(params![ts_start, ts_end], |row| {
                let g = |i: usize| row.get::<_, Option<f64>>(i).unwrap_or(None).unwrap_or(0.0);
                relax += g(0);
                engage += g(1);
                faa += g(2);
                tar += g(3);
                mood += g(4);
                meditation += g(5);
                cog_load += g(6);
                drowsy += g(7);
                hr += g(8);
                snr += g(9);
                rel_alpha += g(10);
                rel_beta += g(11);
                rel_theta += g(12);
                count += 1;
                Ok(())
            })
            .map(|rows| rows.for_each(drop));
    }

    if count == 0 {
        return None;
    }
    let n = count as f64;
    Some(NeighborMetrics {
        relaxation: Some(relax / n),
        engagement: Some(engage / n),
        faa: Some(faa / n),
        tar: Some(tar / n),
        mood: Some(mood / n),
        meditation: Some(meditation / n),
        cognitive_load: Some(cog_load / n),
        drowsiness: Some(drowsy / n),
        hr: if hr > 0.0 { Some(hr / n) } else { None },
        snr: if snr > 0.0 { Some(snr / n) } else { None },
        rel_alpha: Some(rel_alpha / n),
        rel_beta: Some(rel_beta / n),
        rel_theta: Some(rel_theta / n),
        // indices and consciousness metrics are not aggregated
        // in the label-context query (they require per-epoch data from the
        // embeddings table, not the label store). Set to None here.
        headache_index: None,
        migraine_index: None,
        consciousness_lzc: None,
        consciousness_wakefulness: None,
        consciousness_integration: None,
    })
}

// ── Row type for labels.sqlite ────────────────────────────────────────────────

struct LabelRow {
    id: i64,
    text: String,
    context: String,
    eeg_start: u64,
    eeg_end: u64,
    created_at: u64,
    embedding_model: Option<String>,
    text_embedding: Option<Vec<f32>>,
    context_embedding: Option<Vec<f32>>,
}

fn read_label_rows(labels_db: &Path) -> Vec<LabelRow> {
    let Ok(conn) = skill_data::util::open_readonly(labels_db) else {
        return vec![];
    };

    let mut stmt = match conn.prepare(
        "SELECT id, text, context, eeg_start, eeg_end, created_at,
                embedding_model, text_embedding, context_embedding
         FROM labels",
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[label_idx] prepare: {e}");
            return vec![];
        }
    };

    stmt.query_map([], |row| {
        Ok(LabelRow {
            id: row.get(0)?,
            text: row.get(1)?,
            context: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            eeg_start: row.get::<_, i64>(3)? as u64,
            eeg_end: row.get::<_, i64>(4)? as u64,
            created_at: row.get::<_, i64>(5)? as u64,
            embedding_model: row.get(6)?,
            text_embedding: row.get::<_, Option<Vec<u8>>>(7)?.map(|b| blob_to_f32(&b)),
            context_embedding: row.get::<_, Option<Vec<u8>>>(8)?.map(|b| blob_to_f32(&b)),
        })
    })
    .map(|rows| rows.filter_map(std::result::Result::ok).collect())
    .unwrap_or_default()
}

fn fetch_label_by_id(labels_db: &Path, label_id: i64) -> Option<LabelRow> {
    let conn = skill_data::util::open_readonly(labels_db).ok()?;
    conn.query_row(
        "SELECT id, text, context, eeg_start, eeg_end, created_at,
                embedding_model, text_embedding, context_embedding
         FROM labels WHERE id = ?1",
        params![label_id],
        |row| {
            Ok(LabelRow {
                id: row.get(0)?,
                text: row.get(1)?,
                context: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                eeg_start: row.get::<_, i64>(3)? as u64,
                eeg_end: row.get::<_, i64>(4)? as u64,
                created_at: row.get::<_, i64>(5)? as u64,
                embedding_model: row.get(6)?,
                text_embedding: row.get::<_, Option<Vec<u8>>>(7)?.map(|b| blob_to_f32(&b)),
                context_embedding: row.get::<_, Option<Vec<u8>>>(8)?.map(|b| blob_to_f32(&b)),
            })
        },
    )
    .ok()
}

fn hydrate(row: LabelRow, distance: f32, skill_dir: &Path) -> LabelNeighbor {
    let eeg_metrics = mean_metrics_for_window(skill_dir, row.eeg_start, row.eeg_end);
    LabelNeighbor {
        label_id: row.id,
        text: row.text,
        context: row.context,
        eeg_start: row.eeg_start,
        eeg_end: row.eeg_end,
        created_at: row.created_at,
        embedding_model: row.embedding_model,
        distance,
        eeg_metrics,
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// (Re-)build both HNSW indices from the current state of `labels.sqlite`.
///
/// The text index is always fully rebuilt.  
/// The EEG index is rebuilt only for labels whose time window has EEG data.
pub fn rebuild(skill_dir: &Path, state: &LabelIndexState) -> RebuildStats {
    let labels_db = skill_dir.join(LABELS_FILE);
    if !labels_db.exists() {
        return RebuildStats {
            text_nodes: 0,
            eeg_nodes: 0,
            eeg_skipped: 0,
        };
    }

    let rows = read_label_rows(&labels_db);

    let mut text_idx = fresh_index();
    let mut context_idx = fresh_index();
    let mut eeg_idx = fresh_index();
    let mut eeg_skipped = 0usize;

    for row in rows {
        // ── text HNSW ─────────────────────────────────────────────────────────
        if let Some(emb) = row.text_embedding {
            if !emb.is_empty() {
                text_idx.insert(emb, row.id);
            }
        }

        // ── context HNSW ──────────────────────────────────────────────────────
        if let Some(emb) = row.context_embedding {
            if !emb.is_empty() {
                context_idx.insert(emb, row.id);
            }
        }

        // ── EEG HNSW ──────────────────────────────────────────────────────────
        if let Some(mean_emb) = mean_eeg_for_window(skill_dir, row.eeg_start, row.eeg_end) {
            eeg_idx.insert(mean_emb, row.id);
        } else {
            eeg_skipped += 1;
        }
    }

    let text_nodes = text_idx.len();
    let context_nodes = context_idx.len();
    let eeg_nodes = eeg_idx.len();

    // Persist to disk.
    let text_path = skill_dir.join(TEXT_INDEX_FILE);
    let context_path = skill_dir.join(CONTEXT_INDEX_FILE);
    let eeg_path = skill_dir.join(EEG_INDEX_FILE);
    if let Err(e) = text_idx.save(&text_path) {
        eprintln!("[label_idx] text save: {e}");
    }
    if let Err(e) = context_idx.save(&context_path) {
        eprintln!("[label_idx] context save: {e}");
    }
    if let Err(e) = eeg_idx.save(&eeg_path) {
        eprintln!("[label_idx] eeg save: {e}");
    }

    // Update in-memory state.
    *state.text.lock_or_recover() = Some(text_idx);
    *state.context.lock_or_recover() = Some(context_idx);
    *state.eeg.lock_or_recover() = Some(eeg_idx);

    eprintln!(
        "[label_idx] rebuilt: {text_nodes} text, {context_nodes} context, {eeg_nodes} eeg ({eeg_skipped} skipped)"
    );
    RebuildStats {
        text_nodes,
        eeg_nodes,
        eeg_skipped,
    }
}

/// Insert a single label into all indices after it has been embedded.
/// Call this from the `embed_and_store_label` background task.
pub fn insert_label(
    skill_dir: &Path,
    label_id: i64,
    text_embedding: &[f32],
    context_embedding: &[f32],
    eeg_start: u64,
    eeg_end: u64,
    state: &LabelIndexState,
) {
    let skill_dir_buf = skill_dir.to_path_buf();

    // ── Text HNSW ─────────────────────────────────────────────────────────────
    if !text_embedding.is_empty() {
        let mut guard = state.text.lock_or_recover();
        if let Some(ref mut idx) = *guard {
            idx.insert(text_embedding.to_vec(), label_id);
            let path = skill_dir.join(TEXT_INDEX_FILE);
            if let Err(e) = idx.save(&path) {
                eprintln!("[label_idx] text save: {e}");
            }
        }
    }

    // ── Context HNSW ──────────────────────────────────────────────────────────
    if !context_embedding.is_empty() {
        let mut guard = state.context.lock_or_recover();
        if let Some(ref mut idx) = *guard {
            idx.insert(context_embedding.to_vec(), label_id);
            let path = skill_dir.join(CONTEXT_INDEX_FILE);
            if let Err(e) = idx.save(&path) {
                eprintln!("[label_idx] context save: {e}");
            }
        }
    }

    // ── EEG HNSW ──────────────────────────────────────────────────────────────
    if let Some(mean_emb) = mean_eeg_for_window(&skill_dir_buf, eeg_start, eeg_end) {
        let mut guard = state.eeg.lock_or_recover();
        if let Some(ref mut idx) = *guard {
            idx.insert(mean_emb, label_id);
            let path = skill_dir.join(EEG_INDEX_FILE);
            if let Err(e) = idx.save(&path) {
                eprintln!("[label_idx] eeg save: {e}");
            }
        }
    }
}

/// Search the **text** HNSW with a pre-computed text embedding vector.
/// Returns up to `k` nearest labels, hydrated with EEG metrics.
pub fn search_by_text_vec(
    query: &[f32],
    k: usize,
    ef: usize,
    skill_dir: &Path,
    state: &LabelIndexState,
) -> Vec<LabelNeighbor> {
    let labels_db = skill_dir.join(LABELS_FILE);
    let guard = state.text.lock_or_recover();
    let Some(ref idx) = *guard else { return vec![] };
    if idx.is_empty() {
        return vec![];
    }

    idx.search(query, k, ef.max(k))
        .into_iter()
        .filter_map(|hit| {
            let row = fetch_label_by_id(&labels_db, *hit.payload)?;
            Some(hydrate(row, hit.distance, skill_dir))
        })
        .collect()
}

/// Search the **context** HNSW with a pre-computed text embedding vector.
/// Falls back to an empty result if no labels have context embeddings yet.
pub fn search_by_context_vec(
    query: &[f32],
    k: usize,
    ef: usize,
    skill_dir: &Path,
    state: &LabelIndexState,
) -> Vec<LabelNeighbor> {
    let labels_db = skill_dir.join(LABELS_FILE);
    let guard = state.context.lock_or_recover();
    let Some(ref idx) = *guard else { return vec![] };
    if idx.is_empty() {
        return vec![];
    }

    idx.search(query, k, ef.max(k))
        .into_iter()
        .filter_map(|hit| {
            let row = fetch_label_by_id(&labels_db, *hit.payload)?;
            Some(hydrate(row, hit.distance, skill_dir))
        })
        .collect()
}

/// Search the **EEG** HNSW with an EEG embedding vector.
/// Returns up to `k` nearest labels, hydrated with EEG metrics.
pub fn search_by_eeg_vec(
    query: &[f32],
    k: usize,
    ef: usize,
    skill_dir: &Path,
    state: &LabelIndexState,
) -> Vec<LabelNeighbor> {
    let labels_db = skill_dir.join(LABELS_FILE);
    let guard = state.eeg.lock_or_recover();
    let Some(ref idx) = *guard else { return vec![] };
    if idx.is_empty() {
        return vec![];
    }

    idx.search(query, k, ef.max(k))
        .into_iter()
        .filter_map(|hit| {
            let row = fetch_label_by_id(&labels_db, *hit.payload)?;
            Some(hydrate(row, hit.distance, skill_dir))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn label_index_state_new_is_empty() {
        let s = LabelIndexState::new();
        assert!(s.text.lock().unwrap().is_none());
        assert!(s.context.lock().unwrap().is_none());
        assert!(s.eeg.lock().unwrap().is_none());
    }

    #[test]
    fn fresh_index_is_empty() {
        let idx = fresh_index();
        let results = idx.search(&vec![0.0f32; 10], 5, HNSW_EF);
        assert!(results.is_empty());
    }

    #[test]
    fn load_from_empty_dir() {
        let dir = tempdir().unwrap();
        let state = LabelIndexState::new();
        state.load(dir.path());
        // load_or_fresh always creates a fresh index even without files
        assert!(state.text.lock().unwrap().is_some());
        assert!(state.context.lock().unwrap().is_some());
        assert!(state.eeg.lock().unwrap().is_some());
    }

    fn create_labels_db(dir: &std::path::Path) -> rusqlite::Connection {
        let db_path = dir.join(LABELS_FILE);
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS labels (
                id INTEGER PRIMARY KEY,
                eeg_start INTEGER, eeg_end INTEGER,
                wall_start INTEGER, wall_end INTEGER,
                text TEXT, context TEXT, created_at INTEGER,
                text_embedding BLOB, context_embedding BLOB,
                embedding_model TEXT
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn rebuild_empty_dir() {
        let dir = tempdir().unwrap();
        create_labels_db(dir.path());

        let state = LabelIndexState::new();
        let stats = rebuild(dir.path(), &state);
        assert_eq!(stats.text_nodes, 0);
        assert_eq!(stats.eeg_nodes, 0);
    }

    #[test]
    fn insert_and_search_text() {
        let dir = tempdir().unwrap();
        let state = LabelIndexState::new();

        // Initialize with a fresh index
        *state.text.lock().unwrap() = Some(fresh_index());

        // Create labels.sqlite for hydration
        let conn = create_labels_db(dir.path());
        conn.execute(
            "INSERT INTO labels (id, eeg_start, eeg_end, wall_start, wall_end, text, context, created_at)
             VALUES (42, 100, 200, 100, 200, 'test label', '', 1000)",
            [],
        )
        .unwrap();

        let dim = 8;
        let embedding = vec![1.0f32; dim];
        insert_label(dir.path(), 42, &embedding, &[], 100, 200, &state);

        // Search
        let results = search_by_text_vec(&embedding, 5, HNSW_EF, dir.path(), &state);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label_id, 42);
        assert_eq!(results[0].text, "test label");
        assert!(results[0].distance < 0.01);
    }

    #[test]
    fn search_empty_index_returns_empty() {
        let dir = tempdir().unwrap();
        let state = LabelIndexState::new();
        *state.text.lock().unwrap() = Some(fresh_index());

        let query = vec![1.0f32; 8];
        let results = search_by_text_vec(&query, 5, HNSW_EF, dir.path(), &state);
        assert!(results.is_empty());
    }

    #[test]
    fn mean_eeg_for_window_empty_dir() {
        let dir = tempdir().unwrap();
        let result = mean_eeg_for_window(dir.path(), 0, 100);
        assert!(result.is_none());
    }

    #[test]
    fn multiple_inserts_and_knn() {
        let dir = tempdir().unwrap();
        let state = LabelIndexState::new();
        *state.text.lock().unwrap() = Some(fresh_index());

        // Create labels DB
        let conn = create_labels_db(dir.path());
        conn.execute_batch(
            "INSERT INTO labels (id, eeg_start, eeg_end, wall_start, wall_end, text, context, created_at) VALUES
             (1, 100, 200, 100, 200, 'alpha', '', 1000),
             (2, 200, 300, 200, 300, 'beta', '', 2000),
             (3, 300, 400, 300, 400, 'gamma', '', 3000);",
        )
        .unwrap();

        // Insert 3 labels with different embeddings
        insert_label(dir.path(), 1, &[1.0, 0.0, 0.0, 0.0], &[], 100, 200, &state);
        insert_label(dir.path(), 2, &[0.9, 0.1, 0.0, 0.0], &[], 200, 300, &state);
        insert_label(dir.path(), 3, &[0.0, 0.0, 1.0, 0.0], &[], 300, 400, &state);

        // Query close to label 1 and 2
        let results = search_by_text_vec(&[1.0, 0.0, 0.0, 0.0], 3, HNSW_EF, dir.path(), &state);
        assert_eq!(results.len(), 3);
        // Closest should be label 1 (exact match)
        assert_eq!(results[0].label_id, 1);
        assert!(results[0].distance < 0.01);
        // Label 2 should be next (cosine-close)
        assert_eq!(results[1].label_id, 2);
    }
}
