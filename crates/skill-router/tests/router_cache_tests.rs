// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

use std::fs;
use std::path::PathBuf;

use skill_router::{find_label_for_epoch, umap_cache_load, umap_cache_path, umap_cache_store};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("skill-router-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&p).expect("create temp dir");
    p
}

#[test]
fn find_label_for_epoch_matches_inclusive_window() {
    let labels = vec![
        (100_u64, 200_u64, "focus".to_string()),
        (300_u64, 350_u64, "break".to_string()),
    ];

    assert_eq!(find_label_for_epoch(&labels, 100), Some("focus".to_string()));
    assert_eq!(find_label_for_epoch(&labels, 200), Some("focus".to_string()));
    assert_eq!(find_label_for_epoch(&labels, 250), None);
}

#[test]
fn umap_cache_store_and_load_round_trip() {
    let dir = tmp_dir("cache");
    let path = umap_cache_path(&dir, 1, 2, 3, 4);
    let value = serde_json::json!({"ok": true, "n": 7});

    umap_cache_store(&path, &value);
    let loaded = umap_cache_load(&path).expect("cache value");
    assert_eq!(loaded, value);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn umap_cache_path_is_under_umap_cache_dir() {
    let dir = tmp_dir("path");
    let path = umap_cache_path(&dir, 10, 20, 30, 40);
    assert!(path.starts_with(dir.join("umap_cache")));
    assert!(path.to_string_lossy().contains("umap_10_20_30_40.json"));

    let _ = fs::remove_dir_all(dir);
}
