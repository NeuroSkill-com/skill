// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// End-to-end UMAP benchmark on synthetic EEG embeddings (rlx-umap).
//
// Run with:
//   cargo test -p skill-router -- umap_e2e_small --nocapture
//   cargo test -p skill-router -- umap_e2e --ignored --include-ignored --nocapture

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rlx_runtime::device_ext;
use rlx_umap::config::{GraphParams, OptimizationParams, UmapConfig};
use rlx_umap::{Device, Umap};

/// Create a temporary skill_dir with a daily SQLite DB containing synthetic
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

/// Load session embeddings from SQLite and return `(matrix, n_a, n_b)`.
fn load_session_matrix(
    skill_dir: &Path,
    a_start: u64,
    a_end: u64,
    b_start: u64,
    b_end: u64,
) -> (Vec<Vec<f64>>, usize, usize) {
    let embs_a = skill_router::load_embeddings_range(skill_dir, a_start, a_end);
    let embs_b = skill_router::load_embeddings_range(skill_dir, b_start, b_end);
    let n_a = embs_a.len();
    let n_b = embs_b.len();
    let data: Vec<Vec<f64>> = embs_a
        .into_iter()
        .chain(embs_b)
        .map(|(_, emb)| emb.into_iter().map(f64::from).collect())
        .collect();
    (data, n_a, n_b)
}

fn bench_config(n: usize, n_epochs: usize) -> UmapConfig {
    let k = 15_usize.clamp(2, 50).min(n.saturating_sub(1)).min(n / 2).max(2);
    UmapConfig {
        n_components: 3,
        graph: GraphParams {
            n_neighbors: k,
            ..Default::default()
        },
        optimization: OptimizationParams {
            n_epochs,
            verbose: false,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn device_label(device: Device) -> &'static str {
    match device {
        Device::Metal => "metal",
        Device::Mlx => "mlx",
        Device::Cuda => "cuda",
        Device::Gpu => "gpu",
        Device::Rocm => "rocm",
        Device::Cpu => "cpu",
        _ => "other",
    }
}

struct BenchResult {
    wall_ms: u64,
    internal_ms: u64,
    backend: &'static str,
    n_a: usize,
    n_b: usize,
    embedding: Vec<Vec<f64>>,
}

/// Run rlx-umap on SQLite-backed synthetic data.
///
/// Returns `None` when the requested device is unavailable (e.g. headless CI
/// without wgpu/Vulkan). Lets the caller skip rather than fail.
fn run_umap_bench(
    skill_dir: &Path,
    a_start: u64,
    a_end: u64,
    b_start: u64,
    b_end: u64,
    device: Device,
    n_epochs: usize,
) -> Option<BenchResult> {
    if !device_ext::is_available(device) {
        eprintln!("[umap] skip: {device:?} not available");
        return None;
    }

    rlx_umap::register();

    let (data, n_a, n_b) = load_session_matrix(skill_dir, a_start, a_end, b_start, b_end);
    let n = data.len();
    assert!(n >= 5, "seed should produce at least 5 embeddings");

    let config = bench_config(n, n_epochs);
    let backend = device_label(device);

    let wall_start = Instant::now();
    let fit_start = Instant::now();
    let fitted = if device == Device::Cpu {
        Umap::new(config).fit(data)
    } else {
        Umap::with_device(config, device).fit(data)
    };
    let internal_ms = fit_start.elapsed().as_millis() as u64;
    let wall_ms = wall_start.elapsed().as_millis() as u64;

    Some(BenchResult {
        wall_ms,
        internal_ms,
        backend,
        n_a,
        n_b,
        embedding: fitted.embedding,
    })
}

fn report_bench(name: &str, result: &BenchResult) {
    let points_len = result.embedding.len();
    eprintln!("── {name} ──");
    eprintln!("  backend:       {}", result.backend);
    eprintln!("  n_a={}  n_b={}  total={points_len}  dim=32", result.n_a, result.n_b);
    eprintln!("  internal:      {} ms", result.internal_ms);
    eprintln!("  wall (w/ I/O): {} ms", result.wall_ms);
    eprintln!(
        "  throughput:    {:.0} pts/sec",
        points_len as f64 / (result.wall_ms.max(1) as f64 / 1000.0)
    );
}

fn assert_projection(result: &BenchResult) {
    let points_len = result.embedding.len();
    assert_eq!(points_len, result.n_a + result.n_b, "all points projected");
    assert!(result.n_a >= 5, "enough session A points");
    assert!(result.n_b >= 5, "enough session B points");

    let p0 = &result.embedding[0];
    assert_eq!(p0.len(), 3, "point has 3D coordinates");
    for (i, v) in p0.iter().enumerate() {
        assert!(v.is_finite(), "coordinate {i} is finite");
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

/// Small dataset (200 points) — sanity check + fast CI (CPU, no GPU required).
#[test]
fn umap_e2e_small() {
    let (skill_dir, a_start, a_end, b_start, b_end) = seed_synthetic_embeddings("small", 100, 100);

    let Some(result) = run_umap_bench(&skill_dir, a_start, a_end, b_start, b_end, Device::Cpu, 50) else {
        let _ = fs::remove_dir_all(&skill_dir);
        panic!("CPU UMAP should always be available");
    };

    report_bench("umap_e2e_small", &result);
    assert_projection(&result);

    let labels: Vec<u8> = (0..result.n_a).map(|_| 0).chain((0..result.n_b).map(|_| 1)).collect();
    let timestamps: Vec<u64> = (0..result.embedding.len() as u64).collect();
    let analysis = skill_router::analyze_umap_points(&result.embedding, &labels, &timestamps, result.n_a);
    assert!(analysis["separation_score"].is_number());

    let _ = fs::remove_dir_all(skill_dir);
}

/// Medium dataset (1000 points) — GPU/Metal benchmark when hardware is present.
#[test]
#[ignore = "slow benchmark; run with --include-ignored or via npm run test:mlx-e2e"]
#[cfg(feature = "accel")]
fn umap_e2e_medium() {
    let (skill_dir, a_start, a_end, b_start, b_end) = seed_synthetic_embeddings("medium", 500, 500);

    let Some(result) = run_umap_bench(
        &skill_dir,
        a_start,
        a_end,
        b_start,
        b_end,
        skill_router::resolve_umap_device("auto"),
        200,
    ) else {
        let _ = fs::remove_dir_all(&skill_dir);
        return;
    };

    report_bench("umap_e2e_medium", &result);
    assert_eq!(result.embedding.len(), result.n_a + result.n_b);

    let labels: Vec<u8> = (0..result.n_a).map(|_| 0).chain((0..result.n_b).map(|_| 1)).collect();
    let timestamps: Vec<u64> = (0..result.embedding.len() as u64).collect();
    let analysis = skill_router::analyze_umap_points(&result.embedding, &labels, &timestamps, result.n_a);
    let sep = analysis["separation_score"].as_f64().unwrap_or(0.0);
    eprintln!("  separation:    {sep:.3}");

    let _ = fs::remove_dir_all(skill_dir);
}

/// Large dataset (5000 points) — stress test matching real-world cache sizes.
#[test]
#[ignore = "slow benchmark; run with --include-ignored or via npm run test:mlx-e2e"]
#[cfg(feature = "accel")]
fn umap_e2e_large() {
    let (skill_dir, a_start, a_end, b_start, b_end) = seed_synthetic_embeddings("large", 2500, 2500);

    let Some(result) = run_umap_bench(
        &skill_dir,
        a_start,
        a_end,
        b_start,
        b_end,
        skill_router::resolve_umap_device("auto"),
        500,
    ) else {
        let _ = fs::remove_dir_all(&skill_dir);
        return;
    };

    report_bench("umap_e2e_large", &result);
    assert_eq!(result.embedding.len(), result.n_a + result.n_b);

    let labels: Vec<u8> = (0..result.n_a).map(|_| 0).chain((0..result.n_b).map(|_| 1)).collect();
    let timestamps: Vec<u64> = (0..result.embedding.len() as u64).collect();
    let analysis = skill_router::analyze_umap_points(&result.embedding, &labels, &timestamps, result.n_a);
    let sep = analysis["separation_score"].as_f64().unwrap_or(0.0);
    eprintln!("  separation:    {sep:.3}");

    let _ = fs::remove_dir_all(skill_dir);
}
