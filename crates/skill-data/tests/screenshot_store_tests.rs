// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the ScreenshotStore.

use skill_data::screenshot_store::{ScreenshotRow, ScreenshotStore};
use tempfile::tempdir;

fn make_row(ts: u64, filename: &str) -> ScreenshotRow {
    ScreenshotRow {
        timestamp: ts as i64,
        unix_ts: ts,
        filename: filename.into(),
        width: 1920,
        height: 1080,
        file_size: 50000,
        hnsw_id: None,
        embedding: None,
        embedding_dim: 0,
        model_backend: "fastembed".into(),
        model_id: "clip-vit-b-32".into(),
        image_size: 1920,
        quality: 80,
        app_name: "Firefox".into(),
        window_title: "Example Page".into(),
        ocr_text: String::new(),
        ocr_embedding: None,
        ocr_embedding_dim: 0,
        ocr_hnsw_id: None,
        source: "auto".into(),
        chat_session_id: None,
        caption: String::new(),
    }
}

fn make_row_with_app(ts: u64, filename: &str, app: &str, title: &str) -> ScreenshotRow {
    let mut r = make_row(ts, filename);
    r.app_name = app.into();
    r.window_title = title.into();
    r
}

#[test]
fn open_creates_db() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path());
    assert!(store.is_some());
}

#[test]
fn insert_and_count() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");

    assert_eq!(store.count_embedded(), 0);
    assert_eq!(store.count_unembedded(), 0);

    let id = store.insert(&make_row(1700000000, "screenshot_001.webp"));
    assert!(id.is_some());

    // Without embedding, it should be unembedded
    assert_eq!(store.count_unembedded(), 1);
}

#[test]
fn insert_with_embedding() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");

    let mut row = make_row(1700000001, "screenshot_002.webp");
    row.embedding = Some(vec![0.1, 0.2, 0.3]);
    row.embedding_dim = 3;

    let id = store.insert(&row);
    assert!(id.is_some());

    assert_eq!(store.count_embedded(), 1);
}

#[test]
fn multiple_inserts() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");

    for i in 0..5 {
        store.insert(&make_row(1700000000 + i, &format!("ss_{i}.webp")));
    }

    assert_eq!(store.count_unembedded(), 5);
}

// ── find_by_timestamp ────────────────────────────────────────────────────────

#[test]
fn find_by_timestamp_found() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");
    store.insert(&make_row(1700000042, "ss_42.webp"));

    let result = store.find_by_timestamp(1700000042);
    assert!(result.is_some());
    let r = result.unwrap();
    assert_eq!(r.filename, "ss_42.webp");
    assert_eq!(r.app_name, "Firefox");
}

#[test]
fn find_by_timestamp_not_found() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");
    assert!(store.find_by_timestamp(9999999).is_none());
}

// ── around_timestamp ─────────────────────────────────────────────────────────

#[test]
fn around_timestamp_window() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");
    for i in 0..10 {
        store.insert(&make_row(1700000000 + i * 10, &format!("ss_{i}.webp")));
    }

    // Window of ±25s around ts=1700000050 should find ts=30,40,50,60,70
    let results = store.around_timestamp(1700000050, 25);
    assert_eq!(results.len(), 5);
}

// ── update_embedding ─────────────────────────────────────────────────────────

#[test]
fn update_embedding_changes_counts() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");
    let id = store.insert(&make_row(1700000000, "ss.webp")).expect("insert");

    assert_eq!(store.count_unembedded(), 1);
    assert_eq!(store.count_embedded(), 0);

    store.update_embedding(id, &[0.1, 0.2, 0.3], Some(1), "fastembed", "clip-vit-b-32", 224);

    assert_eq!(store.count_unembedded(), 0);
    assert_eq!(store.count_embedded(), 1);
}

// ── count_stale ──────────────────────────────────────────────────────────────

#[test]
fn count_stale_detects_model_change() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");

    let mut row = make_row(1700000000, "ss.webp");
    row.embedding = Some(vec![0.1, 0.2]);
    row.embedding_dim = 2;
    row.model_backend = "old_backend".into();
    row.model_id = "old_model".into();
    store.insert(&row);

    assert_eq!(store.count_stale("new_backend", "new_model"), 1);
    assert_eq!(store.count_stale("old_backend", "old_model"), 0);
}

// ── rows_needing_embed ───────────────────────────────────────────────────────

#[test]
fn rows_needing_embed_finds_unembedded_and_stale() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");

    // Unembedded
    store.insert(&make_row(1700000000, "unembedded.webp"));

    // Stale (different model)
    let mut stale = make_row(1700000001, "stale.webp");
    stale.embedding = Some(vec![0.1]);
    stale.embedding_dim = 1;
    stale.model_id = "old_model".into();
    store.insert(&stale);

    // Current (same model)
    let mut current = make_row(1700000002, "current.webp");
    current.embedding = Some(vec![0.2]);
    current.embedding_dim = 1;
    store.insert(&current);

    let needing = store.rows_needing_embed("fastembed", "clip-vit-b-32");
    assert_eq!(needing.len(), 2);
}

// ── search_by_ocr_text ───────────────────────────────────────────────────────

#[test]
fn search_by_ocr_text_finds_match() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");

    let mut row = make_row(1700000000, "ss.webp");
    row.ocr_text = "Hello World from NeuroSkill".into();
    store.insert(&row);

    let results = store.search_by_ocr_text("NeuroSkill", 10);
    assert_eq!(results.len(), 1);
    assert!(results[0].ocr_text.contains("NeuroSkill"));
}

#[test]
fn search_by_ocr_text_no_match() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");

    let mut row = make_row(1700000000, "ss.webp");
    row.ocr_text = "Hello World".into();
    store.insert(&row);

    let results = store.search_by_ocr_text("nonexistent", 10);
    assert!(results.is_empty());
}

// ── update_ocr ───────────────────────────────────────────────────────────────

#[test]
fn update_ocr_text() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");
    let id = store.insert(&make_row(1700000000, "ss.webp")).expect("insert");

    store.update_ocr(id, "Updated OCR text", None, None);

    let results = store.search_by_ocr_text("Updated OCR", 10);
    assert_eq!(results.len(), 1);
}

// ── summary_counts ───────────────────────────────────────────────────────────

#[test]
fn summary_counts_all_states() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");

    // Plain (no embedding, no OCR)
    store.insert(&make_row(1700000000, "plain.webp"));

    // With embedding
    let mut with_emb = make_row(1700000001, "emb.webp");
    with_emb.embedding = Some(vec![0.1]);
    with_emb.embedding_dim = 1;
    store.insert(&with_emb);

    // With OCR text
    let mut with_ocr = make_row(1700000002, "ocr.webp");
    with_ocr.ocr_text = "some text".into();
    store.insert(&with_ocr);

    // With OCR embedding
    let mut with_ocr_emb = make_row(1700000003, "ocr_emb.webp");
    with_ocr_emb.ocr_text = "text".into();
    with_ocr_emb.ocr_embedding = Some(vec![0.5]);
    with_ocr_emb.ocr_embedding_dim = 1;
    store.insert(&with_ocr_emb);

    let s = store.summary_counts();
    assert_eq!(s.total, 4);
    assert_eq!(s.with_embedding, 1);
    assert_eq!(s.with_ocr, 2); // ocr + ocr_emb
    assert_eq!(s.with_ocr_embedding, 1);
}

// ── count_all ────────────────────────────────────────────────────────────────

#[test]
fn count_all_matches_inserts() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");
    assert_eq!(store.count_all(), 0);

    for i in 0..7 {
        store.insert(&make_row(1700000000 + i, &format!("s{i}.webp")));
    }
    assert_eq!(store.count_all(), 7);
}

// ── top_screenshot_apps ──────────────────────────────────────────────────────

#[test]
fn top_screenshot_apps_grouped() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");

    for i in 0..5 {
        store.insert(&make_row_with_app(
            1700000000 + i,
            &format!("f{i}.webp"),
            "Firefox",
            "Tab",
        ));
    }
    for i in 0..3 {
        store.insert(&make_row_with_app(
            1700000010 + i,
            &format!("c{i}.webp"),
            "Chrome",
            "Page",
        ));
    }
    store.insert(&make_row_with_app(1700000020, "v.webp", "VSCode", "main.rs"));

    let apps = store.top_screenshot_apps(10, None);
    assert!(apps.len() >= 3);
    assert_eq!(apps[0].app_name, "Firefox");
    assert_eq!(apps[0].count, 5);
    assert_eq!(apps[1].app_name, "Chrome");
    assert_eq!(apps[1].count, 3);
}

#[test]
fn top_screenshot_apps_since_filter() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");

    // Old screenshots
    for i in 0..5 {
        store.insert(&make_row_with_app(
            1700000000 + i,
            &format!("old{i}.webp"),
            "OldApp",
            "x",
        ));
    }
    // New screenshots
    for i in 0..3 {
        store.insert(&make_row_with_app(
            1700001000 + i,
            &format!("new{i}.webp"),
            "NewApp",
            "y",
        ));
    }

    let apps = store.top_screenshot_apps(10, Some(1700001000));
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].app_name, "NewApp");
}

// ── all_embeddings round-trip ────────────────────────────────────────────────

#[test]
fn all_embeddings_round_trip() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");

    let emb1 = vec![0.1, 0.2, 0.3];
    let emb2 = vec![0.4, 0.5, 0.6];

    let mut r1 = make_row(1700000001, "a.webp");
    r1.embedding = Some(emb1.clone());
    r1.embedding_dim = 3;
    store.insert(&r1);

    let mut r2 = make_row(1700000002, "b.webp");
    r2.embedding = Some(emb2.clone());
    r2.embedding_dim = 3;
    store.insert(&r2);

    // No embedding
    store.insert(&make_row(1700000003, "c.webp"));

    let all = store.all_embeddings();
    assert_eq!(all.len(), 2);

    // Check values survived the f32->blob->f32 round trip
    for (v1, v2) in all[0].1.iter().zip(emb1.iter()) {
        assert!((v1 - v2).abs() < 1e-6);
    }
}

// ── gif_filename update ──────────────────────────────────────────────────────

#[test]
fn update_gif_filename() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");
    let id = store.insert(&make_row(1700000000, "ss.webp")).expect("insert");

    store.update_gif_filename(id, "ss_motion.gif");

    let result = store.find_by_timestamp(1700000000).expect("find");
    assert_eq!(result.gif_filename, "ss_motion.gif");
}

// ── source and caption fields ────────────────────────────────────────────────

#[test]
fn source_and_caption_stored() {
    let dir = tempdir().expect("tmpdir");
    let store = ScreenshotStore::open(dir.path()).expect("open");

    let mut row = make_row(1700000000, "chat.webp");
    row.source = "llm_chat".into();
    row.chat_session_id = Some(42);
    row.caption = "User uploaded image".into();
    store.insert(&row);

    // Verify it was stored (indirectly, via count)
    assert_eq!(store.count_all(), 1);
}
