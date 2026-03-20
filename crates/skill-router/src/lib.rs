// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Pure-logic core of the WebSocket / HTTP command router.
//!
//! This crate contains everything that does **not** depend on the Tauri
//! runtime: UMAP projection + caching, embedding/label loaders, metric
//! rounding types, and UMAP analysis helpers.
//!
//! The Tauri-specific command handlers (which need `AppHandle` and managed
//! state) remain in `src-tauri/src/ws_commands.rs` and delegate here.

use std::path::{Path, PathBuf};

use serde::Serialize;

use skill_commands::{unix_to_ts, ts_to_unix};
use skill_constants::{SQLITE_FILE, LABELS_FILE};

// ── Rounding helpers ──────────────────────────────────────────────────────────

pub fn r1(v: f32) -> f32 { (v * 10.0).round() / 10.0 }
pub fn r2(v: f32) -> f32 { (v * 100.0).round() / 100.0 }
pub fn r3(v: f32) -> f32 { (v * 1000.0).round() / 1000.0 }
pub fn r1d(v: f64) -> f64 { (v * 10.0).round() / 10.0 }
pub fn r2d(v: f64) -> f64 { (v * 100.0).round() / 100.0 }
pub fn r2f(v: f64) -> f64 { (v * 100.0).round() / 100.0 }

// ── Rounded metric types ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct RoundedBands {
    pub rel_delta: f32,
    pub rel_theta: f32,
    pub rel_alpha: f32,
    pub rel_beta:  f32,
    pub rel_gamma: f32,
}

#[derive(Serialize)]
pub struct RoundedScores {
    pub relaxation: f32,
    pub engagement: f32,
    pub faa: f32,
    pub tar: f32,
    pub bar: f32,
    pub dtr: f32,
    pub pse: f32,
    pub apf: f32,
    pub bps: f32,
    pub snr: f32,
    pub coherence: f32,
    pub mu_suppression: f32,
    pub mood: f32,
    pub tbr: f32,
    pub sef95: f32,
    pub spectral_centroid: f32,
    pub hjorth_activity: f32,
    pub hjorth_mobility: f32,
    pub hjorth_complexity: f32,
    pub permutation_entropy: f32,
    pub higuchi_fd: f32,
    pub dfa_exponent: f32,
    pub sample_entropy: f32,
    pub pac_theta_gamma: f32,
    pub laterality_index: f32,
    pub hr: f64,
    pub rmssd: f64,
    pub sdnn: f64,
    pub pnn50: f64,
    pub lf_hf_ratio: f64,
    pub respiratory_rate: f64,
    pub spo2_estimate: f64,
    pub perfusion_index: f64,
    pub stress_index: f64,
    // Artifact detection
    pub blink_count: u64,
    pub blink_rate: f64,
    // Head pose
    pub head_pitch: f64,
    pub head_roll: f64,
    pub stillness: f64,
    pub nod_count: u64,
    pub shake_count: u64,
    // Composite scores
    pub meditation: f64,
    pub cognitive_load: f64,
    pub drowsiness: f64,
    pub bands: RoundedBands,
    pub epoch_timestamp: i64,
}

// ── Embedding / label loaders ─────────────────────────────────────────────────

/// Load all embedding vectors from daily SQLite DBs in [start, end] UTC range.
pub fn load_embeddings_range(
    skill_dir: &Path,
    start_utc: u64,
    end_utc:   u64,
) -> Vec<(u64, Vec<f32>)> {
    let ts_start = unix_to_ts(start_utc);
    let ts_end   = unix_to_ts(end_utc);

    let mut out: Vec<(u64, Vec<f32>)> = Vec::new();
    let entries = match std::fs::read_dir(skill_dir) {
        Ok(e) => e, Err(_) => return out,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let db_path = path.join(SQLITE_FILE);
        if !db_path.exists() { continue; }
        let conn = match skill_data::util::open_readonly(&db_path)
        { Ok(c) => c, Err(_) => continue };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        let mut stmt = match conn.prepare(
            "SELECT timestamp, eeg_embedding FROM embeddings
             WHERE timestamp >= ?1 AND timestamp <= ?2 ORDER BY timestamp"
        ) { Ok(s) => s, Err(_) => continue };

        let rows = stmt.query_map(rusqlite::params![ts_start, ts_end], |row| {
            let ts: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let emb: Vec<f32> = skill_data::util::blob_to_f32(&blob);
            Ok((ts_to_unix(ts), emb))
        });
        if let Ok(rows) = rows {
            for r in rows.flatten() { out.push(r); }
        }
    }
    out.sort_by_key(|e| e.0);
    out
}

/// Load all labels from `labels.sqlite` whose EEG window overlaps [start, end].
/// Returns Vec<(eeg_start_unix, eeg_end_unix, text)>.
pub fn load_labels_range(
    skill_dir: &Path,
    start_utc: u64,
    end_utc:   u64,
) -> Vec<(u64, u64, String)> {
    let labels_db = skill_dir.join(LABELS_FILE);
    if !labels_db.exists() { return vec![]; }
    let conn = match skill_data::util::open_readonly(&labels_db)
    { Ok(c) => c, Err(_) => return vec![] };
    let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
    let mut stmt = match conn.prepare(
        "SELECT eeg_start, eeg_end, text FROM labels
         WHERE eeg_end >= ?1 AND eeg_start <= ?2
         ORDER BY eeg_start"
    ) { Ok(s) => s, Err(_) => return vec![] };
    stmt.query_map(
        rusqlite::params![start_utc as i64, end_utc as i64],
        |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64, row.get::<_, String>(2)?))
    ).map(|rows| rows.flatten().collect()).unwrap_or_default()
}

/// Find the first label whose EEG window contains `epoch_utc`.
pub fn find_label_for_epoch(labels: &[(u64, u64, String)], epoch_utc: u64) -> Option<String> {
    labels.iter()
        .find(|(start, end, _)| epoch_utc >= *start && epoch_utc <= *end)
        .map(|(_, _, text)| text.clone())
}

// ── UMAP analysis ─────────────────────────────────────────────────────────────

/// Cluster analysis of UMAP 3-D projection: centroids, separation score, outliers.
pub fn analyze_umap_points(
    embedding: &[Vec<f64>],
    session_ids: &[u8],    // 0 = A, 1 = B
    timestamps: &[u64],
    _n_a: usize,
) -> serde_json::Value {
    let n = embedding.len().min(session_ids.len());
    if n == 0 { return serde_json::json!(null); }

    // Centroids
    let (mut ca, mut cb) = ([0.0f64; 3], [0.0f64; 3]);
    let (mut na, mut nb) = (0usize, 0usize);
    for i in 0..n {
        let c = if session_ids[i] == 0 { &mut ca } else { &mut cb };
        let cnt = if session_ids[i] == 0 { &mut na } else { &mut nb };
        for d in 0..3 { c[d] += embedding[i][d]; }
        *cnt += 1;
    }
    if na > 0 { for c in ca.iter_mut() { *c /= na as f64; } }
    if nb > 0 { for c in cb.iter_mut() { *c /= nb as f64; } }

    let inter_dist = ((ca[0]-cb[0]).powi(2) + (ca[1]-cb[1]).powi(2) + (ca[2]-cb[2]).powi(2)).sqrt();

    let dist_to = |pt: &[f64], c: &[f64; 3]| -> f64 {
        ((pt[0]-c[0]).powi(2) + (pt[1]-c[1]).powi(2) + (pt[2]-c[2]).powi(2)).sqrt()
    };
    let (mut spread_a, mut spread_b) = (0.0f64, 0.0f64);
    for i in 0..n {
        if session_ids[i] == 0 { spread_a += dist_to(&embedding[i], &ca); }
        else                   { spread_b += dist_to(&embedding[i], &cb); }
    }
    if na > 0 { spread_a /= na as f64; }
    if nb > 0 { spread_b /= nb as f64; }

    let avg_intra = (spread_a + spread_b) / 2.0;
    let separation = if avg_intra > 1e-9 { inter_dist / avg_intra } else { 0.0 };

    let mut all_dists_a: Vec<f64> = Vec::new();
    let mut all_dists_b: Vec<f64> = Vec::new();
    for i in 0..n {
        let d = dist_to(&embedding[i], if session_ids[i] == 0 { &ca } else { &cb });
        if session_ids[i] == 0 { all_dists_a.push(d); } else { all_dists_b.push(d); }
    }
    let std_a = if all_dists_a.len() > 1 {
        let m = all_dists_a.iter().sum::<f64>() / all_dists_a.len() as f64;
        (all_dists_a.iter().map(|x| (x - m).powi(2)).sum::<f64>() / all_dists_a.len() as f64).sqrt()
    } else { 1.0 };
    let std_b = if all_dists_b.len() > 1 {
        let m = all_dists_b.iter().sum::<f64>() / all_dists_b.len() as f64;
        (all_dists_b.iter().map(|x| (x - m).powi(2)).sum::<f64>() / all_dists_b.len() as f64).sqrt()
    } else { 1.0 };

    let mut outliers: Vec<serde_json::Value> = Vec::new();
    let mut oi_a = 0usize;
    let mut oi_b = 0usize;
    for i in 0..n {
        let c = if session_ids[i] == 0 { &ca } else { &cb };
        let d = dist_to(&embedding[i], c);
        let threshold = if session_ids[i] == 0 { spread_a + 2.0 * std_a } else { spread_b + 2.0 * std_b };
        if d > threshold {
            if outliers.len() < 20 {
                outliers.push(serde_json::json!({
                    "x": r2f(embedding[i][0]), "y": r2f(embedding[i][1]), "z": r2f(embedding[i][2]),
                    "session": if session_ids[i] == 0 { "A" } else { "B" },
                    "utc": timestamps.get(i).copied().unwrap_or(0),
                    "distance_to_centroid": r2f(d),
                }));
            }
            if session_ids[i] == 0 { oi_a += 1; } else { oi_b += 1; }
        }
    }

    serde_json::json!({
        "centroid_a": [r2f(ca[0]), r2f(ca[1]), r2f(ca[2])],
        "centroid_b": [r2f(cb[0]), r2f(cb[1]), r2f(cb[2])],
        "inter_cluster_distance": r2f(inter_dist),
        "intra_spread_a": r2f(spread_a),
        "intra_spread_b": r2f(spread_b),
        "separation_score": r2f(separation),
        "n_outliers_a": oi_a,
        "n_outliers_b": oi_b,
        "outliers": outliers,
    })
}

// ── UMAP cache ────────────────────────────────────────────────────────────────

/// Return the path to the UMAP cache directory inside `~/.skill/umap_cache/`.
pub fn umap_cache_dir(skill_dir: &Path) -> PathBuf {
    skill_dir.join("umap_cache")
}

/// Build a deterministic cache filename for a session-pair UMAP result.
pub fn umap_cache_path(
    skill_dir: &Path,
    a_start: u64,
    a_end: u64,
    b_start: u64,
    b_end: u64,
) -> PathBuf {
    umap_cache_dir(skill_dir)
        .join(format!("umap_{a_start}_{a_end}_{b_start}_{b_end}.json"))
}

/// Try to load a cached UMAP result from disk.
pub fn umap_cache_load(path: &Path) -> Option<serde_json::Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Persist a UMAP result to the cache directory (best-effort, errors are logged).
pub fn umap_cache_store(path: &Path, value: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("[umap] failed to create cache dir: {e}");
            return;
        }
    }
    match serde_json::to_vec(value) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(path, bytes) {
                eprintln!("[umap] failed to write cache file: {e}");
            } else {
                eprintln!("[umap] cached result to {}", path.display());
            }
        }
        Err(e) => eprintln!("[umap] failed to serialise cache: {e}"),
    }
}

// ── UMAP compute ──────────────────────────────────────────────────────────────

/// Backend type alias used by fast-umap (GPU-accelerated via wgpu / CubeCL).
type FastUmapBackend = burn::backend::Autodiff<
    burn_cubecl::CubeBackend<cubecl::wgpu::WgpuRuntime, f32, i32, u32>,
>;

/// Inner UMAP compute — shared by both WS and Tauri IPC paths.
///
/// Uses `fast-umap` (parametric, GPU-accelerated) instead of `umap-rs` for
/// significantly faster projection on large embedding sets.
///
/// Results are cached to `~/.skill/umap_cache/umap_{a}_{b}_{c}_{d}.json` so
/// that repeated queries for the same session pair return instantly.
pub fn umap_compute_inner(
    skill_dir: &Path,
    a_start: u64,
    a_end: u64,
    b_start: u64,
    b_end: u64,
    on_progress: Option<Box<dyn Fn(fast_umap::EpochProgress) + Send>>,
) -> Result<serde_json::Value, String> {
    // ── Check cache first ────────────────────────────────────────────────
    let cache_path = umap_cache_path(skill_dir, a_start, a_end, b_start, b_end);
    if let Some(cached) = umap_cache_load(&cache_path) {
        eprintln!("[umap] cache hit: {}", cache_path.display());
        return Ok(cached);
    }

    let embs_a = load_embeddings_range(skill_dir, a_start, a_end);
    let embs_b = load_embeddings_range(skill_dir, b_start, b_end);
    let all_labels = load_labels_range(
        skill_dir,
        a_start.min(b_start),
        a_end.max(b_end),
    );

    let n_a = embs_a.len();
    let n_b = embs_b.len();
    let n   = n_a + n_b;

    let umap_start = std::time::Instant::now();
    eprintln!("[umap] computing 3D projection for {} embeddings (A={}, B={})", n, n_a, n_b);

    if n < 5 {
        return Ok(serde_json::json!({ "points": [], "n_a": n_a, "n_b": n_b, "dim": 0 }));
    }

    let dim = embs_a.first().or(embs_b.first())
        .map(|e| e.1.len()).unwrap_or(0);
    if dim == 0 {
        return Ok(serde_json::json!({ "points": [], "n_a": n_a, "n_b": n_b, "dim": 0 }));
    }

    // ── Load user-configurable UMAP parameters ─────────────────────────────
    let ucfg = skill_settings::load_umap_config(skill_dir);

    let n_use = n;

    // Build Vec<Vec<f64>> input expected by fast-umap.
    let mut data: Vec<Vec<f64>> = Vec::with_capacity(n_use);
    let mut timestamps: Vec<u64> = Vec::with_capacity(n_use);
    let mut labels: Vec<u8> = Vec::with_capacity(n_use);
    for (ts, emb) in embs_a.iter().chain(embs_b.iter()) {
        data.push(emb.iter().map(|&v| v as f64).collect());
        timestamps.push(*ts);
        labels.push(if timestamps.len() <= n_a { 0 } else { 1 });
    }

    let k = ucfg.n_neighbors.clamp(2, 50).min(n_use - 1).min(n_use / 2).max(2);
    let n_epochs = ucfg.n_epochs.clamp(50, 2000);

    let config = fast_umap::UmapConfig {
        n_components: 3,
        graph: fast_umap::GraphParams {
            n_neighbors: k,
            ..Default::default()
        },
        optimization: fast_umap::OptimizationParams {
            n_epochs,
            verbose: false,
            repulsion_strength: ucfg.repulsion_strength.clamp(0.1, 10.0),
            neg_sample_rate: ucfg.neg_sample_rate.clamp(1, 30),
            timeout: Some(ucfg.timeout_secs.clamp(10, 600)),
            cooldown_ms: ucfg.cooldown_ms.clamp(0, 10_000),
            figures_dir: Some(skill_dir.join("tmp/figures")),
            ..Default::default()
        },
        ..Default::default()
    };

    let fit_labels: Vec<String> = (0..n_use).map(|i| {
        let session_tag = if labels[i] == 0 { "A" } else { "B" };
        if let Some(lbl) = find_label_for_epoch(&all_labels, timestamps[i]) {
            format!("{session_tag}:{lbl}")
        } else {
            session_tag.to_string()
        }
    }).collect();

    let fit_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let umap = fast_umap::Umap::<FastUmapBackend>::new(config);
        let (_exit_tx, exit_rx) = crossbeam_channel::unbounded::<()>();
        let fitted = if let Some(cb) = on_progress {
            umap.fit_with_progress(data, Some(fit_labels), exit_rx, cb)
        } else {
            umap.fit_with_signal(data, Some(fit_labels), exit_rx)
        };
        fitted.into_embedding()
    }));

    let embedding = match fit_result {
        Ok(emb) => emb,
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "unknown panic".to_string()
            };
            eprintln!("[umap] UMAP fit panicked: {msg}");
            return Err(format!("UMAP projection failed: {msg}"));
        }
    };

    let points: Vec<serde_json::Value> = (0..n_use).map(|i| {
        let mut pt = serde_json::json!({
            "x": embedding[i][0],
            "y": embedding[i][1],
            "z": embedding[i][2],
            "session": labels[i],
            "utc": timestamps[i],
        });
        if let Some(lbl) = find_label_for_epoch(&all_labels, timestamps[i]) {
            if let Some(obj) = pt.as_object_mut() { obj.insert("label".into(), serde_json::Value::String(lbl)); }
        }
        pt
    }).collect();

    let elapsed_ms = umap_start.elapsed().as_millis() as u64;
    eprintln!("[umap] projection done in {elapsed_ms} ms ({n_use} embeddings)");

    let analysis = analyze_umap_points(&embedding, &labels, &timestamps, n_a);

    let result = serde_json::json!({
        "points":     points,
        "n_a":        n_a,
        "n_b":        n_b,
        "dim":        dim,
        "elapsed_ms": elapsed_ms,
        "analysis":   analysis,
    });

    // ── Persist to cache ─────────────────────────────────────────────────
    umap_cache_store(&cache_path, &result);

    Ok(result)
}

// ── Supported commands ────────────────────────────────────────────────────────

/// Names of all commands understood by the router (for documentation / discovery).
pub const COMMANDS: &[&str] = &[
    "status",
    "calibrate",
    "timer",
    "notify",
    "label",
    "search_labels",
    "interactive_search",
    "search",
    "compare",
    "session_metrics",
    "sessions",
    "sleep",
    "umap",
    "umap_poll",
    "hooks_get",
    "hooks_set",
    "hooks_status",
    "hooks_suggest",
    "hooks_log",
    "list_calibrations",
    "get_calibration",
    "create_calibration",
    "update_calibration",
    "delete_calibration",
    "run_calibration",
    "say",
    "dnd",
    "dnd_set",
    "sleep_schedule",
    "sleep_schedule_set",
    // HealthKit commands
    "health_sync",
    "health_query",
    "health_summary",
    "health_metric_types",
    // LLM commands (when the `llm` feature is enabled in the host binary)
    "llm_status",
    "llm_start",
    "llm_stop",
    "llm_catalog",
    "llm_download",
    "llm_cancel_download",
    "llm_delete",
    "llm_logs",
    "llm_select_model",
    "llm_select_mmproj",
    "llm_pause_download",
    "llm_resume_download",
    "llm_refresh_catalog",
    "llm_downloads",
    "llm_set_autoload_mmproj",
    "llm_add_model",
    "llm_hardware_fit",
];
