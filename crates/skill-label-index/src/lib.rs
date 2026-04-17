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

/// Find the most common dimension among a set of embedding lengths.
/// Returns `None` if the iterator is empty.
fn dominant_dim(dims: impl Iterator<Item = usize>) -> Option<usize> {
    let mut counts = std::collections::HashMap::<usize, usize>::new();
    for d in dims {
        if d > 0 {
            *counts.entry(d).or_default() += 1;
        }
    }
    counts.into_iter().max_by_key(|&(_, c)| c).map(|(d, _)| d)
}

/// Insert into an HNSW index only if the dimension matches (or the index is
/// empty).  Returns `true` if inserted, `false` if skipped due to mismatch.
fn safe_insert(
    idx: &mut LabeledIndex<Cosine, i64>,
    emb: Vec<f32>,
    label_id: i64,
    expected_dim: &mut Option<usize>,
) -> bool {
    if emb.is_empty() {
        return false;
    }
    let dim = emb.len();
    match *expected_dim {
        None => {
            *expected_dim = Some(dim);
            idx.insert(emb, label_id);
            true
        }
        Some(d) if d == dim => {
            idx.insert(emb, label_id);
            true
        }
        Some(d) => {
            eprintln!("[label_idx] skipping label {label_id}: dim {dim} != expected {d}");
            false
        }
    }
}

/// Check if a query vector matches the index dimension before searching.
/// Returns empty results on mismatch instead of panicking.
fn safe_search<'a>(
    idx: &'a LabeledIndex<Cosine, i64>,
    query: &[f32],
    k: usize,
    ef: usize,
) -> Vec<fast_hnsw::labeled::LabeledResult<'a, i64>> {
    if idx.is_empty() || query.is_empty() {
        return vec![];
    }
    // Check dimension before searching to avoid a panic inside fast_hnsw.
    if let Some(dim) = idx.inner.dim() {
        if query.len() != dim {
            eprintln!(
                "[label_idx] search skipped: query dim {} != index dim {dim}",
                query.len()
            );
            return vec![];
        }
    }
    idx.search(query, k, ef.max(k))
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
    // Epoch timestamps in the DB use two formats:
    // - Unix milliseconds (e.g. 1775512050594)
    // - YYYYMMDDHHmmss × 1000 (e.g. 20260413234815000)
    // Labels store eeg_start/eeg_end as Unix seconds.
    // Query both ranges to match both formats.
    // Widen window by ±30s to catch nearby epochs.
    // EEG epochs are typically 5s apart so ±30s ensures we catch nearby data
    // even for labels with narrow time windows.
    let pad: u64 = 30;
    let r = skill_data::util::DualTimestampRange::from_unix_secs(eeg_start.saturating_sub(pad), eeg_end + pad);

    let mut sum: Vec<f32> = Vec::new();
    let mut count = 0usize;

    for (_date, dir) in skill_data::util::date_dirs(skill_dir) {
        // ── 1. Try SQLite embeddings first ───────────────────────────────
        let db_path = dir.join(SQLITE_FILE);
        if db_path.exists() {
            if let Ok(conn) = skill_data::util::open_readonly(&db_path) {
                if let Ok(mut stmt) = conn.prepare(&format!(
                    "SELECT eeg_embedding FROM embeddings \
                         WHERE eeg_embedding IS NOT NULL AND length(eeg_embedding) >= 4 \
                           AND ({})",
                    skill_data::util::DualTimestampRange::WHERE_CLAUSE
                )) {
                    if let Ok(mapped) = stmt.query_map(
                        params![
                            r.unix_ms_start,
                            r.unix_ms_end,
                            r.dt14_start,
                            r.dt14_end,
                            r.dt17_start,
                            r.dt17_end
                        ],
                        |row| row.get::<_, Vec<u8>>(0),
                    ) {
                        for blob in mapped.filter_map(std::result::Result::ok) {
                            let v = blob_to_f32(&blob);
                            if v.is_empty() {
                                continue;
                            }
                            if sum.is_empty() {
                                sum.resize(v.len(), 0.0);
                            }
                            if v.len() != sum.len() {
                                continue;
                            }
                            for (s, &x) in sum.iter_mut().zip(v.iter()) {
                                *s += x;
                            }
                            count += 1;
                        }
                    }
                }
            }
            // If SQLite found results for this day, skip HNSW fallback.
            if count > 0 {
                continue;
            }
        }

        // ── 2. Fallback: scan per-day HNSW file ─────────────────────────
        // Old data may have embeddings only in eeg_embeddings.hnsw with
        // timestamp payloads but no eeg_embedding column in SQLite.
        let hnsw_path = dir.join(skill_constants::HNSW_INDEX_FILE);
        if !hnsw_path.exists() {
            continue;
        }
        let Ok(idx) = LabeledIndex::<Cosine, i64>::load_mmap(&hnsw_path, Cosine) else {
            continue;
        };
        for i in 0..idx.len() {
            let ts = *idx.get_payload(i);
            // Payload timestamps can be Unix ms, YYYYMMDDHHmmss, or YYYYMMDDHHmmss×1000.
            let in_unix = ts >= r.unix_ms_start && ts <= r.unix_ms_end;
            let in_dt14 = ts >= r.dt14_start && ts <= r.dt14_end;
            let in_dt17 = ts >= r.dt17_start && ts <= r.dt17_end;
            if !in_unix && !in_dt14 && !in_dt17 {
                continue;
            }
            let emb = idx.get_embedding(i);
            if emb.is_empty() {
                continue;
            }
            if sum.is_empty() {
                sum.resize(emb.len(), 0.0);
            }
            if emb.len() != sum.len() {
                continue;
            }
            for (s, &x) in sum.iter_mut().zip(emb.iter()) {
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
    let r = skill_data::util::DualTimestampRange::from_unix_secs(eeg_start, eeg_end);

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

        let Ok(mut stmt) = conn.prepare(&format!(
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
             FROM embeddings WHERE {}",
            skill_data::util::DualTimestampRange::WHERE_CLAUSE
        )) else {
            continue;
        };

        let _ = stmt
            .query_map(
                params![
                    r.unix_ms_start,
                    r.unix_ms_end,
                    r.dt14_start,
                    r.dt14_end,
                    r.dt17_start,
                    r.dt17_end
                ],
                |row| {
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
                },
            )
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

    // Determine the most common text embedding dimension so we index
    // the majority and skip outliers from a previous model.
    let dominant_text_dim = dominant_dim(rows.iter().filter_map(|r| r.text_embedding.as_ref().map(|e| e.len())));
    let dominant_ctx_dim = dominant_dim(
        rows.iter()
            .filter_map(|r| r.context_embedding.as_ref().map(|e| e.len())),
    );

    let mut text_idx = fresh_index();
    let mut context_idx = fresh_index();
    let mut eeg_idx = fresh_index();
    let mut eeg_skipped = 0usize;
    let mut text_dim: Option<usize> = dominant_text_dim;
    let mut ctx_dim: Option<usize> = dominant_ctx_dim;
    let mut eeg_dim: Option<usize> = None;

    for row in rows {
        // ── text HNSW ─────────────────────────────────────────────────────────
        if let Some(emb) = row.text_embedding {
            safe_insert(&mut text_idx, emb, row.id, &mut text_dim);
        }

        // ── context HNSW ──────────────────────────────────────────────────────
        if let Some(emb) = row.context_embedding {
            safe_insert(&mut context_idx, emb, row.id, &mut ctx_dim);
        }

        // ── EEG HNSW ──────────────────────────────────────────────────────────
        if let Some(mean_emb) = mean_eeg_for_window(skill_dir, row.eeg_start, row.eeg_end) {
            if !safe_insert(&mut eeg_idx, mean_emb, row.id, &mut eeg_dim) {
                eeg_skipped += 1;
            }
        } else {
            if eeg_skipped < 3 {
                eprintln!(
                    "[label_idx] eeg skip: label {} eeg_start={} eeg_end={}",
                    row.id, row.eeg_start, row.eeg_end
                );
            }
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

/// Result of inserting a label into the indices.
#[derive(Debug, Default)]
pub struct InsertResult {
    /// `true` if any index was rebuilt due to a dimension change.
    pub rebuilt: bool,
}

/// Insert a single label into all indices after it has been embedded.
///
/// If the new embedding's dimension differs from the current index, the index
/// is automatically rebuilt from SQLite so the new label is immediately
/// searchable.  The caller can check `InsertResult::rebuilt` to notify the
/// user that old labels with a different model are no longer indexed.
pub fn insert_label(
    skill_dir: &Path,
    label_id: i64,
    text_embedding: &[f32],
    context_embedding: &[f32],
    eeg_start: u64,
    eeg_end: u64,
    state: &LabelIndexState,
) -> InsertResult {
    let skill_dir_buf = skill_dir.to_path_buf();
    let mut needs_rebuild = false;

    /// Try to insert; returns `false` on dimension mismatch.
    fn try_insert(
        idx: &mut LabeledIndex<Cosine, i64>,
        emb: &[f32],
        label_id: i64,
        save_path: &Path,
        tag: &str,
    ) -> bool {
        if emb.is_empty() {
            return true; // nothing to do
        }
        if let Some(d) = idx.inner.dim() {
            if emb.len() != d {
                eprintln!(
                    "[label_idx] {tag} dim mismatch for label {label_id}: {} != index {d}",
                    emb.len()
                );
                return false;
            }
        }
        idx.insert(emb.to_vec(), label_id);
        if let Err(e) = idx.save(save_path) {
            eprintln!("[label_idx] {tag} save: {e}");
        }
        true
    }

    // ── Text HNSW ─────────────────────────────────────────────────────────────
    if !text_embedding.is_empty() {
        let mut guard = state.text.lock_or_recover();
        if let Some(ref mut idx) = *guard {
            if !try_insert(idx, text_embedding, label_id, &skill_dir.join(TEXT_INDEX_FILE), "text") {
                needs_rebuild = true;
            }
        }
    }

    // ── Context HNSW ──────────────────────────────────────────────────────────
    if !context_embedding.is_empty() {
        let mut guard = state.context.lock_or_recover();
        if let Some(ref mut idx) = *guard {
            if !try_insert(
                idx,
                context_embedding,
                label_id,
                &skill_dir.join(CONTEXT_INDEX_FILE),
                "context",
            ) {
                needs_rebuild = true;
            }
        }
    }

    // ── EEG HNSW ──────────────────────────────────────────────────────────────
    if let Some(mean_emb) = mean_eeg_for_window(&skill_dir_buf, eeg_start, eeg_end) {
        let mut guard = state.eeg.lock_or_recover();
        if let Some(ref mut idx) = *guard {
            if !try_insert(idx, &mean_emb, label_id, &skill_dir.join(EEG_INDEX_FILE), "eeg") {
                needs_rebuild = true;
            }
        }
    }

    // On dimension mismatch, rebuild all indices from SQLite.
    // This picks the dominant dimension (which now includes the new label),
    // so the new label is immediately searchable.
    if needs_rebuild {
        eprintln!("[label_idx] dimension changed — rebuilding all indices from SQLite");
        rebuild(skill_dir, state);
    }

    InsertResult { rebuilt: needs_rebuild }
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

    safe_search(idx, query, k, ef)
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

    safe_search(idx, query, k, ef)
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

    safe_search(idx, query, k, ef)
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

    // ── Dimension mismatch edge cases ────────────────────────────────────────

    #[test]
    fn insert_dimension_mismatch_is_skipped() {
        let dir = tempdir().unwrap();
        let state = LabelIndexState::new();
        *state.text.lock().unwrap() = Some(fresh_index());

        let conn = create_labels_db(dir.path());
        conn.execute_batch(
            "INSERT INTO labels (id, eeg_start, eeg_end, wall_start, wall_end, text, context, created_at) VALUES
             (1, 0, 0, 0, 0, 'a', '', 1),
             (2, 0, 0, 0, 0, 'b', '', 2);",
        )
        .unwrap();

        let to_blob = |v: &[f32]| -> Vec<u8> { v.iter().flat_map(|f| f.to_le_bytes()).collect() };

        // Store 4-dim embedding in DB and insert into index
        conn.execute(
            "UPDATE labels SET text_embedding = ?1, embedding_model = 'a' WHERE id = 1",
            rusqlite::params![to_blob(&[1.0, 0.0, 0.0, 0.0])],
        )
        .unwrap();
        let r1 = insert_label(dir.path(), 1, &[1.0, 0.0, 0.0, 0.0], &[], 0, 0, &state);
        assert!(!r1.rebuilt);

        // Store 8-dim embedding in DB and try to insert — triggers rebuild
        conn.execute(
            "UPDATE labels SET text_embedding = ?1, embedding_model = 'b' WHERE id = 2",
            rusqlite::params![to_blob(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])],
        )
        .unwrap();
        let r2 = insert_label(
            dir.path(),
            2,
            &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            &[],
            0,
            0,
            &state,
        );
        assert!(r2.rebuilt);

        // After rebuild with mixed dims, each dim has 1 label so either could be dominant.
        // The important thing: no panic, and the index has exactly 1 node (dominant dim).
        let guard = state.text.lock().unwrap();
        let idx = guard.as_ref().unwrap();
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn search_dimension_mismatch_returns_empty() {
        let dir = tempdir().unwrap();
        let state = LabelIndexState::new();
        *state.text.lock().unwrap() = Some(fresh_index());

        let conn = create_labels_db(dir.path());
        conn.execute(
            "INSERT INTO labels (id, eeg_start, eeg_end, wall_start, wall_end, text, context, created_at)
             VALUES (1, 0, 0, 0, 0, 'test', '', 1)",
            [],
        )
        .unwrap();

        // Insert 4-dim embedding
        insert_label(dir.path(), 1, &[1.0, 0.0, 0.0, 0.0], &[], 0, 0, &state);

        // Search with 8-dim query — should return empty, not panic
        let results = search_by_text_vec(
            &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            5,
            HNSW_EF,
            dir.path(),
            &state,
        );
        assert!(results.is_empty());

        // Search with correct dim should still work
        let results = search_by_text_vec(&[1.0, 0.0, 0.0, 0.0], 5, HNSW_EF, dir.path(), &state);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_empty_query_returns_empty() {
        let dir = tempdir().unwrap();
        let state = LabelIndexState::new();
        *state.text.lock().unwrap() = Some(fresh_index());

        insert_label(dir.path(), 1, &[1.0, 0.0, 0.0, 0.0], &[], 0, 0, &state);

        let results = search_by_text_vec(&[], 5, HNSW_EF, dir.path(), &state);
        assert!(results.is_empty());
    }

    #[test]
    fn rebuild_mixed_dimensions_uses_dominant() {
        let dir = tempdir().unwrap();
        let conn = create_labels_db(dir.path());

        // Helper to create embedding blobs
        let to_blob = |v: &[f32]| -> Vec<u8> { v.iter().flat_map(|f| f.to_le_bytes()).collect() };

        // Insert 3 labels: 2 with 4-dim, 1 with 8-dim embeddings
        conn.execute(
            "INSERT INTO labels (id, eeg_start, eeg_end, wall_start, wall_end, text, context, created_at, text_embedding, embedding_model)
             VALUES (1, 0, 0, 0, 0, 'a', '', 1, ?1, 'model-a')",
            rusqlite::params![to_blob(&[1.0, 0.0, 0.0, 0.0])],
        ).unwrap();
        conn.execute(
            "INSERT INTO labels (id, eeg_start, eeg_end, wall_start, wall_end, text, context, created_at, text_embedding, embedding_model)
             VALUES (2, 0, 0, 0, 0, 'b', '', 2, ?1, 'model-a')",
            rusqlite::params![to_blob(&[0.0, 1.0, 0.0, 0.0])],
        ).unwrap();
        conn.execute(
            "INSERT INTO labels (id, eeg_start, eeg_end, wall_start, wall_end, text, context, created_at, text_embedding, embedding_model)
             VALUES (3, 0, 0, 0, 0, 'c', '', 3, ?1, 'model-b')",
            rusqlite::params![to_blob(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])],
        ).unwrap();

        let state = LabelIndexState::new();
        let stats = rebuild(dir.path(), &state);

        // Only the 2 labels with the dominant dimension (4) should be indexed
        assert_eq!(stats.text_nodes, 2);

        // Search with 4-dim should find both
        let results = search_by_text_vec(&[1.0, 0.0, 0.0, 0.0], 5, HNSW_EF, dir.path(), &state);
        assert_eq!(results.len(), 2);

        // Search with 8-dim should return empty (dimension mismatch)
        let results = search_by_text_vec(
            &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            5,
            HNSW_EF,
            dir.path(),
            &state,
        );
        assert!(results.is_empty());
    }

    #[test]
    fn rebuild_after_reembed_picks_up_new_dimension() {
        let dir = tempdir().unwrap();
        let conn = create_labels_db(dir.path());
        let to_blob = |v: &[f32]| -> Vec<u8> { v.iter().flat_map(|f| f.to_le_bytes()).collect() };

        // All labels start with 4-dim
        conn.execute(
            "INSERT INTO labels (id, eeg_start, eeg_end, wall_start, wall_end, text, context, created_at, text_embedding, embedding_model)
             VALUES (1, 0, 0, 0, 0, 'x', '', 1, ?1, 'old')",
            rusqlite::params![to_blob(&[1.0, 0.0, 0.0, 0.0])],
        ).unwrap();
        conn.execute(
            "INSERT INTO labels (id, eeg_start, eeg_end, wall_start, wall_end, text, context, created_at, text_embedding, embedding_model)
             VALUES (2, 0, 0, 0, 0, 'y', '', 2, ?1, 'old')",
            rusqlite::params![to_blob(&[0.0, 1.0, 0.0, 0.0])],
        ).unwrap();

        let state = LabelIndexState::new();
        let stats = rebuild(dir.path(), &state);
        assert_eq!(stats.text_nodes, 2);

        // Simulate reembed: update all labels to 8-dim
        conn.execute(
            "UPDATE labels SET text_embedding = ?1, embedding_model = 'new' WHERE id = 1",
            rusqlite::params![to_blob(&[1.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0])],
        )
        .unwrap();
        conn.execute(
            "UPDATE labels SET text_embedding = ?1, embedding_model = 'new' WHERE id = 2",
            rusqlite::params![to_blob(&[0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.5])],
        )
        .unwrap();

        // Rebuild should now use 8-dim
        let stats = rebuild(dir.path(), &state);
        assert_eq!(stats.text_nodes, 2);

        // 4-dim search should now return empty
        let results = search_by_text_vec(&[1.0, 0.0, 0.0, 0.0], 5, HNSW_EF, dir.path(), &state);
        assert!(results.is_empty());

        // 8-dim search should find both
        let results = search_by_text_vec(
            &[1.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0],
            5,
            HNSW_EF,
            dir.path(),
            &state,
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].label_id, 1); // exact match
    }

    #[test]
    fn safe_insert_tracks_dimension() {
        let mut idx = fresh_index();
        let mut dim: Option<usize> = None;

        // First insert sets dimension
        assert!(safe_insert(&mut idx, vec![1.0, 2.0, 3.0], 1, &mut dim));
        assert_eq!(dim, Some(3));

        // Same dimension succeeds
        assert!(safe_insert(&mut idx, vec![4.0, 5.0, 6.0], 2, &mut dim));
        assert_eq!(idx.len(), 2);

        // Different dimension is rejected
        assert!(!safe_insert(&mut idx, vec![1.0, 2.0], 3, &mut dim));
        assert_eq!(idx.len(), 2); // unchanged

        // Empty is rejected
        assert!(!safe_insert(&mut idx, vec![], 4, &mut dim));
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn dominant_dim_picks_majority() {
        assert_eq!(dominant_dim([4, 4, 8].into_iter()), Some(4));
        assert_eq!(dominant_dim([8, 8, 4].into_iter()), Some(8));
        assert_eq!(dominant_dim([384, 768, 768, 768].into_iter()), Some(768));
        assert_eq!(dominant_dim(std::iter::empty::<usize>()), None);
        assert_eq!(dominant_dim([0, 0].into_iter()), None); // zeros filtered
    }

    #[test]
    fn context_and_eeg_search_dimension_safety() {
        let dir = tempdir().unwrap();
        let state = LabelIndexState::new();
        *state.context.lock().unwrap() = Some(fresh_index());
        *state.eeg.lock().unwrap() = Some(fresh_index());

        let conn = create_labels_db(dir.path());
        conn.execute(
            "INSERT INTO labels (id, eeg_start, eeg_end, wall_start, wall_end, text, context, created_at)
             VALUES (1, 0, 0, 0, 0, 't', 'c', 1)",
            [],
        )
        .unwrap();

        // Insert 4-dim context embedding
        insert_label(dir.path(), 1, &[], &[1.0, 0.0, 0.0, 0.0], 0, 0, &state);

        // Search with wrong dim returns empty
        let r = search_by_context_vec(&[1.0, 0.0], 5, HNSW_EF, dir.path(), &state);
        assert!(r.is_empty());

        // Search with correct dim works
        let r = search_by_context_vec(&[1.0, 0.0, 0.0, 0.0], 5, HNSW_EF, dir.path(), &state);
        assert_eq!(r.len(), 1);

        // EEG search on empty index returns empty
        let r = search_by_eeg_vec(&[1.0, 0.0], 5, HNSW_EF, dir.path(), &state);
        assert!(r.is_empty());
    }

    #[test]
    fn insert_into_none_state_does_not_panic() {
        let dir = tempdir().unwrap();
        let state = LabelIndexState::new();
        // State indices are None (not loaded) — should not panic
        insert_label(dir.path(), 1, &[1.0, 2.0], &[3.0, 4.0], 0, 0, &state);
    }

    #[test]
    fn rebuild_no_db_returns_zeros() {
        let dir = tempdir().unwrap();
        // No labels.sqlite exists
        let state = LabelIndexState::new();
        let stats = rebuild(dir.path(), &state);
        assert_eq!(stats.text_nodes, 0);
        assert_eq!(stats.eeg_nodes, 0);
        assert_eq!(stats.eeg_skipped, 0);
    }

    #[test]
    fn rebuild_labels_without_embeddings() {
        let dir = tempdir().unwrap();
        let conn = create_labels_db(dir.path());
        conn.execute(
            "INSERT INTO labels (id, eeg_start, eeg_end, wall_start, wall_end, text, context, created_at)
             VALUES (1, 0, 0, 0, 0, 'no embedding', '', 1)",
            [],
        )
        .unwrap();

        let state = LabelIndexState::new();
        let stats = rebuild(dir.path(), &state);
        assert_eq!(stats.text_nodes, 0); // no text_embedding column filled
    }
}
