// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// End-to-end UMAP benchmark on synthetic EEG embeddings.
//
// Run with:
//   cargo test -p skill-router --features gpu  -- umap_e2e --nocapture
//   cargo test -p skill-router --features mlx  -- umap_e2e --nocapture
//   cargo test -p skill-router --features gpu,mlx -- umap_e2e --nocapture

use std::fs;
use std::path::PathBuf;

/// Create a temporary skill_dir with a daily SQLite DB containing `n` synthetic
/// 32-dimensional EEG embeddings spread across two sessions (A and B).
///
/// Returns `(skill_dir, a_start, a_end, b_start, b_end)`.
fn seed_synthetic_embeddings(tag: &str, n_a: usize, n_b: usize) -> (PathBuf, u64, u64, u64, u64) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let skill_dir = std::env::temp_dir().join(format!("skill-umap-e2e-{tag}-{nanos}"));
    let day_dir = skill_dir.join("20260303");
    fs::create_dir_all(&day_dir).expect("create temp day dir");

    let db_path = day_dir.join("eeg.sqlite");
    let conn = rusqlite::Connection::open(&db_path).expect("open db");
    conn.execute_batch(
        "CREATE TABLE embeddings (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp       INTEGER NOT NULL,
            device_id       TEXT,
            device_name     TEXT,
            hnsw_id         INTEGER NOT NULL,
            eeg_embedding   BLOB NOT NULL,
            label           TEXT,
            extra_embedding BLOB,
            metrics_json    TEXT
        );
        CREATE INDEX idx_timestamp ON embeddings (timestamp);",
    )
    .expect("create table");

    // Use unix-ms timestamps so load_embeddings_range can query them.
    // Session A: starts at t=1_700_000_000 (arbitrary), one epoch every 250 ms.
    let a_start_ms: i64 = 1_700_000_000_000;
    let b_start_ms: i64 = a_start_ms + (n_a as i64) * 250 + 60_000; // 1 min gap

    let dim = 32_usize;
    let mut rng_seed: u64 = 42;

    let mut insert = conn
        .prepare(
            "INSERT INTO embeddings (timestamp, device_id, device_name, hnsw_id, eeg_embedding)
             VALUES (?1, 'bench', 'bench', ?2, ?3)",
        )
        .expect("prepare insert");

    let mut write_epochs = |start_ms: i64, count: usize, cluster_offset: f32| {
        for i in 0..count {
            let ts = start_ms + (i as i64) * 250;
            // Simple LCG for deterministic pseudo-random embeddings
            let emb: Vec<f32> = (0..dim)
                .map(|d| {
                    rng_seed = rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let raw = ((rng_seed >> 33) as f32) / (u32::MAX as f32) - 0.5;
                    raw + cluster_offset * (d as f32 / dim as f32)
                })
                .collect();
            let blob: Vec<u8> = emb.iter().flat_map(|v| v.to_le_bytes()).collect();
            insert
                .execute(rusqlite::params![ts, i as i64, blob])
                .expect("insert embedding");
        }
    };

    write_epochs(a_start_ms, n_a, 1.0);
    write_epochs(b_start_ms, n_b, -1.0);

    drop(insert);
    conn.close().ok();

    let a_start_s = (a_start_ms / 1000) as u64;
    let a_end_s = ((a_start_ms + (n_a as i64) * 250) / 1000) as u64;
    let b_start_s = (b_start_ms / 1000) as u64;
    let b_end_s = ((b_start_ms + (n_b as i64) * 250) / 1000) as u64;

    (skill_dir, a_start_s, a_end_s, b_start_s, b_end_s)
}

/// Run `umap_compute_inner` on the synthetic data and return elapsed milliseconds.
///
/// Returns `None` when the host has no usable GPU adapter (e.g. headless Linux
/// CI runners without Vulkan ICDs). Lets the test eprintln-and-skip rather
/// than fail in environments where the hardware prerequisite is absent.
#[cfg(any(feature = "gpu", feature = "mlx"))]
fn run_umap_bench(
    skill_dir: &std::path::Path,
    a_start: u64,
    a_end: u64,
    b_start: u64,
    b_end: u64,
) -> Option<(u64, serde_json::Value)> {
    let wall_start = std::time::Instant::now();
    match skill_router::umap_compute_inner(skill_dir, a_start, a_end, b_start, b_end, None) {
        Ok(value) => Some((wall_start.elapsed().as_millis() as u64, value)),
        Err(e) => {
            let msg = format!("{e:#}");
            if msg.contains("adapter") || msg.contains("Vulkan") || msg.contains("backend") {
                eprintln!("[umap] skipping bench — no usable GPU adapter: {msg}");
                None
            } else {
                panic!("umap_compute_inner failed: {msg}");
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

/// Small dataset (200 points) — sanity check + fast CI.
#[test]
#[cfg(any(feature = "gpu", feature = "mlx"))]
fn umap_e2e_small() {
    let (skill_dir, a_start, a_end, b_start, b_end) = seed_synthetic_embeddings("small", 100, 100);

    let Some((wall_ms, result)) = run_umap_bench(&skill_dir, a_start, a_end, b_start, b_end) else {
        let _ = fs::remove_dir_all(&skill_dir);
        return;
    };

    let n_a = result["n_a"].as_u64().unwrap();
    let n_b = result["n_b"].as_u64().unwrap();
    let points_len = result["points"].as_array().map(|a| a.len()).unwrap_or(0);
    let internal_ms = result["elapsed_ms"].as_u64().unwrap_or(0);
    let backend = result["backend"].as_str().unwrap_or("?");

    eprintln!("── umap_e2e_small ──");
    eprintln!("  backend:       {backend}");
    eprintln!("  n_a={n_a}  n_b={n_b}  total={points_len}  dim=32");
    eprintln!("  internal:      {internal_ms} ms");
    eprintln!("  wall (w/ I/O): {wall_ms} ms");
    eprintln!(
        "  throughput:    {:.0} pts/sec",
        points_len as f64 / (wall_ms.max(1) as f64 / 1000.0)
    );

    assert_eq!(points_len as u64, n_a + n_b, "all points projected");
    assert!(n_a >= 5, "enough session A points");
    assert!(n_b >= 5, "enough session B points");

    // Verify 3D coordinates exist
    let p0 = &result["points"][0];
    assert!(p0["x"].is_f64(), "point has x coordinate");
    assert!(p0["y"].is_f64(), "point has y coordinate");
    assert!(p0["z"].is_f64(), "point has z coordinate");

    let _ = fs::remove_dir_all(skill_dir);
}

/// Medium dataset (1000 points) — representative of a typical EEG session pair.
#[test]
#[ignore = "slow benchmark; run with --include-ignored or via npm run test:mlx-e2e"]
#[cfg(any(feature = "gpu", feature = "mlx"))]
fn umap_e2e_medium() {
    let (skill_dir, a_start, a_end, b_start, b_end) = seed_synthetic_embeddings("medium", 500, 500);

    let Some((wall_ms, result)) = run_umap_bench(&skill_dir, a_start, a_end, b_start, b_end) else {
        let _ = fs::remove_dir_all(&skill_dir);
        return;
    };

    let n_a = result["n_a"].as_u64().unwrap();
    let n_b = result["n_b"].as_u64().unwrap();
    let points_len = result["points"].as_array().map(|a| a.len()).unwrap_or(0);
    let internal_ms = result["elapsed_ms"].as_u64().unwrap_or(0);
    let backend = result["backend"].as_str().unwrap_or("?");
    let sep = result["analysis"]["separation_score"].as_f64().unwrap_or(0.0);

    eprintln!("── umap_e2e_medium ──");
    eprintln!("  backend:       {backend}");
    eprintln!("  n_a={n_a}  n_b={n_b}  total={points_len}  dim=32");
    eprintln!("  internal:      {internal_ms} ms");
    eprintln!("  wall (w/ I/O): {wall_ms} ms");
    eprintln!(
        "  throughput:    {:.0} pts/sec",
        points_len as f64 / (wall_ms.max(1) as f64 / 1000.0)
    );
    eprintln!("  separation:    {sep:.3}");

    assert_eq!(points_len as u64, n_a + n_b);

    let _ = fs::remove_dir_all(skill_dir);
}

/// Large dataset (5000 points) — stress test matching real-world cache sizes.
#[test]
#[ignore = "slow benchmark; run with --include-ignored or via npm run test:mlx-e2e"]
#[cfg(any(feature = "gpu", feature = "mlx"))]
fn umap_e2e_large() {
    let (skill_dir, a_start, a_end, b_start, b_end) = seed_synthetic_embeddings("large", 2500, 2500);

    let Some((wall_ms, result)) = run_umap_bench(&skill_dir, a_start, a_end, b_start, b_end) else {
        let _ = fs::remove_dir_all(&skill_dir);
        return;
    };

    let n_a = result["n_a"].as_u64().unwrap();
    let n_b = result["n_b"].as_u64().unwrap();
    let points_len = result["points"].as_array().map(|a| a.len()).unwrap_or(0);
    let internal_ms = result["elapsed_ms"].as_u64().unwrap_or(0);
    let backend = result["backend"].as_str().unwrap_or("?");
    let sep = result["analysis"]["separation_score"].as_f64().unwrap_or(0.0);

    eprintln!("── umap_e2e_large ──");
    eprintln!("  backend:       {backend}");
    eprintln!("  n_a={n_a}  n_b={n_b}  total={points_len}  dim=32");
    eprintln!("  internal:      {internal_ms} ms");
    eprintln!("  wall (w/ I/O): {wall_ms} ms");
    eprintln!(
        "  throughput:    {:.0} pts/sec",
        points_len as f64 / (wall_ms.max(1) as f64 / 1000.0)
    );
    eprintln!("  separation:    {sep:.3}");

    assert_eq!(points_len as u64, n_a + n_b);

    let _ = fs::remove_dir_all(skill_dir);
}
