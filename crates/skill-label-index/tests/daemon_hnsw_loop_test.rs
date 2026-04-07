// SPDX-License-Identifier: GPL-3.0-only
//! End-to-end test: create labels.sqlite → embed → insert into HNSW → search → rebuild.
//!
//! Validates the full lifecycle that the daemon drives:
//! 1. Create a fresh `LabelIndexState`
//! 2. Create `labels.sqlite` with the full schema
//! 3. Insert labels with text embeddings (synthetic vectors)
//! 4. Insert those into the HNSW index via `insert_label`
//! 5. Search by text embedding → verify correct results
//! 6. Rebuild from disk → verify indices survive
//! 7. Persist to disk → reload → verify search still works

use skill_constants::LABELS_FILE;
use skill_label_index::{insert_label, rebuild, search_by_context_vec, search_by_text_vec, LabelIndexState};
use std::path::Path;
use tempfile::tempdir;

const DIM: usize = 8;
const HNSW_EF: usize = skill_constants::HNSW_EF_CONSTRUCTION;

/// Create the `labels.sqlite` database with the full daemon schema.
fn create_labels_db(skill_dir: &Path) -> rusqlite::Connection {
    let db_path = skill_dir.join(LABELS_FILE);
    let conn = rusqlite::Connection::open(&db_path).expect("open labels.sqlite");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS labels (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            text              TEXT NOT NULL,
            context           TEXT DEFAULT '',
            eeg_start         INTEGER NOT NULL DEFAULT 0,
            eeg_end           INTEGER NOT NULL DEFAULT 0,
            wall_start        INTEGER NOT NULL DEFAULT 0,
            wall_end          INTEGER NOT NULL DEFAULT 0,
            created_at        INTEGER NOT NULL DEFAULT 0,
            text_embedding    BLOB,
            context_embedding BLOB,
            embedding_model   TEXT
        );",
    )
    .expect("create labels table");
    conn
}

fn f32_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Insert a label row with text + context embeddings into the database.
fn insert_label_row(
    conn: &rusqlite::Connection,
    text: &str,
    context: &str,
    text_emb: &[f32],
    ctx_emb: &[f32],
    eeg_start: u64,
    eeg_end: u64,
) -> i64 {
    let now = eeg_start;
    let text_blob = f32_to_blob(text_emb);
    let ctx_blob = f32_to_blob(ctx_emb);
    conn.execute(
        "INSERT INTO labels (text, context, eeg_start, eeg_end, wall_start, wall_end,
                             created_at, text_embedding, context_embedding, embedding_model)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'test-model')",
        rusqlite::params![
            text,
            context,
            eeg_start as i64,
            eeg_end as i64,
            now as i64,
            now as i64,
            now as i64,
            text_blob,
            ctx_blob,
        ],
    )
    .expect("insert label row");
    conn.last_insert_rowid()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn full_hnsw_lifecycle() {
    let dir = tempdir().unwrap();
    let skill_dir = dir.path();

    // 1. Fresh state
    let state = LabelIndexState::new();
    state.load(skill_dir);

    // Indices should be loaded (fresh/empty)
    assert!(state.text.lock().unwrap().is_some());
    assert!(state.context.lock().unwrap().is_some());
    assert!(state.eeg.lock().unwrap().is_some());

    // 2. Create labels.sqlite
    let conn = create_labels_db(skill_dir);

    // 3. Insert labels with distinct embeddings
    let emb_a = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; // label "alpha"
    let emb_b = vec![0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; // label "beta"
    let emb_c = vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]; // label "gamma"

    let ctx_a = vec![0.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let ctx_b = vec![0.0, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0];
    let ctx_c = vec![0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0];

    let id_a = insert_label_row(&conn, "alpha", "ctx_alpha", &emb_a, &ctx_a, 1000, 2000);
    let id_b = insert_label_row(&conn, "beta", "ctx_beta", &emb_b, &ctx_b, 2000, 3000);
    let id_c = insert_label_row(&conn, "gamma", "ctx_gamma", &emb_c, &ctx_c, 3000, 4000);

    // 4. Insert into HNSW via insert_label
    insert_label(skill_dir, id_a, &emb_a, &ctx_a, 1000, 2000, &state);
    insert_label(skill_dir, id_b, &emb_b, &ctx_b, 2000, 3000, &state);
    insert_label(skill_dir, id_c, &emb_c, &ctx_c, 3000, 4000, &state);

    // 5. Search by text embedding
    let results = search_by_text_vec(&emb_a, 3, HNSW_EF, skill_dir, &state);
    assert!(!results.is_empty(), "text search should return results");
    assert_eq!(results[0].label_id, id_a, "closest should be alpha");
    assert_eq!(results[0].text, "alpha");
    assert!(results[0].distance < 0.01, "exact match should have ~0 distance");

    // Search near beta
    let results_b = search_by_text_vec(&emb_b, 3, HNSW_EF, skill_dir, &state);
    assert_eq!(results_b[0].label_id, id_b);
    assert_eq!(results_b[0].text, "beta");

    // 5b. Search by context embedding
    let ctx_results = search_by_context_vec(&ctx_a, 3, HNSW_EF, skill_dir, &state);
    assert!(!ctx_results.is_empty(), "context search should return results");
    assert_eq!(ctx_results[0].label_id, id_a);

    // 6. Rebuild from labels.sqlite and verify
    let stats = rebuild(skill_dir, &state);
    assert_eq!(stats.text_nodes, 3);
    // EEG nodes will be 0 (no EEG data in temp dir)
    assert_eq!(stats.eeg_skipped, 3);

    // Search still works after rebuild
    let results_post = search_by_text_vec(&emb_c, 3, HNSW_EF, skill_dir, &state);
    assert_eq!(results_post[0].label_id, id_c);
    assert_eq!(results_post[0].text, "gamma");

    // 7. Reload from disk — simulates daemon restart
    let state2 = LabelIndexState::new();
    state2.load(skill_dir);

    let results_reload = search_by_text_vec(&emb_a, 3, HNSW_EF, skill_dir, &state2);
    assert!(!results_reload.is_empty(), "search should work after reload");
    assert_eq!(results_reload[0].label_id, id_a);
    assert_eq!(results_reload[0].text, "alpha");

    // Context search also survives reload
    let ctx_reload = search_by_context_vec(&ctx_b, 3, HNSW_EF, skill_dir, &state2);
    assert!(!ctx_reload.is_empty());
    assert_eq!(ctx_reload[0].label_id, id_b);
}

#[test]
fn incremental_insert_then_rebuild_consistent() {
    let dir = tempdir().unwrap();
    let skill_dir = dir.path();
    let state = LabelIndexState::new();
    state.load(skill_dir);
    let conn = create_labels_db(skill_dir);

    // Insert one label incrementally
    let emb = vec![0.7, 0.7, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let id = insert_label_row(&conn, "solo", "solo_ctx", &emb, &emb, 100, 200);
    insert_label(skill_dir, id, &emb, &emb, 100, 200, &state);

    let r1 = search_by_text_vec(&emb, 5, HNSW_EF, skill_dir, &state);
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].label_id, id);

    // Insert more labels
    let emb2 = vec![0.0, 0.0, 0.0, 0.0, 0.7, 0.7, 0.0, 0.0];
    let id2 = insert_label_row(&conn, "duo", "duo_ctx", &emb2, &emb2, 300, 400);
    insert_label(skill_dir, id2, &emb2, &emb2, 300, 400, &state);

    // Rebuild should give same results
    rebuild(skill_dir, &state);

    let r2 = search_by_text_vec(&emb, 5, HNSW_EF, skill_dir, &state);
    assert_eq!(r2[0].label_id, id, "solo should still be closest to its own embedding");

    let r3 = search_by_text_vec(&emb2, 5, HNSW_EF, skill_dir, &state);
    assert_eq!(r3[0].label_id, id2, "duo should still be closest to its own embedding");
}

#[test]
fn empty_search_returns_empty() {
    let dir = tempdir().unwrap();
    let state = LabelIndexState::new();
    state.load(dir.path());

    let query = vec![1.0f32; DIM];
    let results = search_by_text_vec(&query, 5, HNSW_EF, dir.path(), &state);
    assert!(results.is_empty());
}

#[test]
fn rebuild_without_labels_db_is_noop() {
    let dir = tempdir().unwrap();
    let state = LabelIndexState::new();
    state.load(dir.path());

    let stats = rebuild(dir.path(), &state);
    assert_eq!(stats.text_nodes, 0);
    assert_eq!(stats.eeg_nodes, 0);
    assert_eq!(stats.eeg_skipped, 0);
}
