// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the ScreenshotStore.

use skill_data::screenshot_store::{ScreenshotStore, ScreenshotRow};
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
    }
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
