// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! EEG epoch accumulator → ZUNA wgpu embedding → per-date HNSW + SQLite storage.
//!
//! ## Storage layout
//!
//! ```text
//! ~/.skill/
//!   20260223/
//!     eeg_embeddings.hnsw   ← daily HNSW approximate-NN index
//!     eeg.sqlite            ← daily SQLite database
//!   20260224/
//!     ...
//! ```
//!
//! A new date folder is created automatically at midnight (UTC).  Both files
//! are opened at worker startup and re-opened whenever the date rolls over.
//!
//! ## SQLite schema  (`embeddings` table)
//!
//! | column            | type    | notes |
//! |-------------------|---------|-------|
//! | `id`              | INTEGER | PRIMARY KEY AUTOINCREMENT |
//! | `timestamp`       | INTEGER | `YYYYMMDDHHmmss` UTC |
//! | `device_id`       | TEXT    | BLE peripheral id (nullable) |
//! | `device_name`     | TEXT    | headset display name (nullable) |
//! | `hnsw_id`         | INTEGER | zero-based row in the daily HNSW file |
//! | `eeg_embedding`   | BLOB    | `f32 LE × dim` (32 floats = 128 bytes) |
//! | `label`           | TEXT    | user-defined tag (nullable, reserved) |
//! | `extra_embedding` | BLOB    | optional second embedding (nullable) |
//! | `ppg_ambient`     | REAL    | mean PPG ambient ADC value (nullable) |
//! | `ppg_infrared`    | REAL    | mean PPG infrared ADC value (nullable) |
//! | `ppg_red`         | REAL    | mean PPG red ADC value (nullable) |

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::skill_log::SkillLogger;

use crate::MutexExt;
use crate::{
    constants::{
        CHANNEL_NAMES, EEG_CHANNELS, EMBEDDING_EPOCH_SAMPLES, EMBEDDING_HOP_SAMPLES,
        EMBEDDING_OVERLAP_MAX_SECS, EMBEDDING_OVERLAP_MIN_SECS,
        HNSW_INDEX_FILE, MUSE_SAMPLE_RATE, SQLITE_FILE,
        ZUNA_CONFIG_FILE, ZUNA_WEIGHTS_FILE,
    },
    eeg_model_config::{EegModelConfig, EegModelStatus},
};

// ── Message sent to the background worker ─────────────────────────────────────

struct EpochMsg {
    /// Raw µV samples: `[EEG_CHANNELS][EMBEDDING_EPOCH_SAMPLES]`.
    samples:     Vec<Vec<f32>>,
    /// `YYYYMMDDHHmmss` UTC at the epoch boundary.
    timestamp:   i64,
    device_id:   Option<String>,
    device_name: Option<String>,
    /// Band powers snapshot at the moment this epoch was emitted (may be None
    /// if the band analyzer hasn't produced a result yet).
    band_snapshot: Option<crate::eeg_bands::BandSnapshot>,
    /// PPG averages for the epoch window: [ambient, infrared, red].
    /// Each value is the mean of all PPG samples received during this epoch.
    /// `None` if no PPG data was received (e.g. Muse 1 which has no PPG sensor).
    ppg_averages: Option<[f64; 3]>,
    /// Derived PPG metrics (HR, HRV, SpO2, etc.).
    ppg_metrics: Option<crate::ppg_analysis::PpgMetrics>,
}

// ── cubecl cache warm-up ──────────────────────────────────────────────────────

/// Pre-create the platform GPU-kernel cache directories that `cubecl` uses.
///
/// In `cubecl-common ≤ 0.9.0` the cache loader calls `.unwrap()` on
/// `std::fs::read()` without first checking whether the file exists.  On the
/// very first launch (empty cache) the read returns `Err(NotFound)` and the
/// thread panics.  Creating the directory tree ahead of the wgpu device
/// initialisation prevents that `ENOENT` by ensuring at least the *parent*
/// directory is present; cubecl can then safely stat/create individual cache
/// entries without hitting the missing-ancestor case.
// Simple eprintln-based log for use before a logger object is available.
macro_rules! skill_log_plain {
    ($fmt:literal $(, $arg:expr)*) => { eprintln!($fmt $(, $arg)*) };
}

/// Pre-create the cubecl kernel-cache directory tree before any wgpu operation.
///
/// cubecl-common 0.9.0 uses:
///   `dirs::home_dir().join(".cache") / "cubecl" / <crate-version> / <kernel-path>`
///
/// `CacheFile::new` calls `create_dir_all(parent).ok()` — silently swallowing
/// errors — then `File::create(&path).unwrap()`.  On macOS `~/.cache/` does not
/// exist by default (macOS uses `~/Library/Caches`), so `create_dir_all` fails
/// silently, `File::create` gets ENOENT, and the thread panics.
///
/// From a terminal the cache already exists from prior runs, so the bug is
/// invisible there.  Launching as a `.app` (fresh environment, no pre-existing
/// cache) always hits it.
///
/// We pass `skill_dir` so we can derive `$HOME` reliably — even when `$HOME`
/// is absent from the environment — because skill_dir = `$HOME/.skill`.
/// Point cubecl's autotune cache at a known-writable directory inside
/// `skill_dir`, then pre-create it so cubecl's `File::create` never fails.
///
/// ## Why this is necessary
///
/// cubecl-runtime 0.9.0 defaults to `CacheConfig::Target`, which walks up
/// from `std::env::current_dir()` looking for a `Cargo.toml` and uses
/// `<project_root>/target/` as the cache root.
///
/// • **From a terminal** inside the dev tree: `Cargo.toml` is found,
///   `target/` already exists, everything works.
/// • **From a `.app` bundle**: `current_dir()` is `/` (or the bundle path),
///   no `Cargo.toml` is ever found, so it falls back to
///   `current_dir().join("target")` = `/target/`.  macOS returns EACCES
///   when cubecl tries to create `/target/`; that error is silently
///   swallowed (`.ok()`), then `File::create` panics with ENOENT.  The
///   resulting panic poisons the wgpu device's internal mutex for the rest
///   of the process lifetime.
///
/// `GlobalConfig::set()` must be called **before** the first
/// `WgpuDevice::DefaultDevice` access (i.e. before the encoder load).
/// It panics if called a second time, so we guard with `catch_unwind`
/// to make subsequent worker restarts harmless.
fn configure_cubecl_cache(skill_dir: &Path) {
    use cubecl_runtime::config::{cache::CacheConfig, GlobalConfig};

    let cache_dir = skill_dir.join("cubecl_cache");
    match std::fs::create_dir_all(&cache_dir) {
        Ok(_)  => skill_log_plain!("[embedder] cubecl cache dir: {}", cache_dir.display()),
        Err(e) => skill_log_plain!("[embedder] warn: cubecl cache mkdir {}: {e}", cache_dir.display()),
    }

    let mut cfg = GlobalConfig::default();
    cfg.autotune.cache = CacheConfig::File(cache_dir);

    // set() panics if called after the first get().  Catch the panic so a
    // second worker spawned in the same process is safe (the first call
    // already set the config; the second is a no-op).
    let _ = std::panic::catch_unwind(|| GlobalConfig::set(cfg));
}

/// Process-global flag: set to `true` after any GPU panic so that respawned
/// workers don't attempt to use the wgpu device, whose internal mutexes are
/// permanently poisoned for the rest of this process lifetime.
static GPU_DEVICE_POISONED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Attempt to extract a human-readable message from a caught panic payload.
fn panic_msg(payload: &Box<dyn std::any::Any + Send>) -> &str {
    payload.downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("(non-string panic payload)")
}

/// Per-epoch band-derived metrics stored alongside each embedding.
struct EpochMetrics {
    rel_delta:  f32,
    rel_theta:  f32,
    rel_alpha:  f32,
    rel_beta:   f32,
    rel_gamma:  f32,
    rel_high_gamma: f32,
    /// Relaxation score (0–100): α / (β + θ) mapped through a sigmoid.
    relaxation: f32,
    /// Engagement index (0–100): β / (α + θ) with a gentler sigmoid.
    engagement: f32,
    /// Frontal Alpha Asymmetry: ln(AF8 α) − ln(AF7 α).
    faa:        f32,
    /// Theta / Alpha ratio.
    tar:        f32,
    /// Beta / Alpha ratio.
    bar:        f32,
    /// Delta / Theta ratio.
    dtr:        f32,
    /// Power Spectral Entropy [0–1].
    pse:        f32,
    /// Alpha Peak Frequency (Hz).
    apf:        f32,
    /// Band-Power Slope (1/f exponent).
    bps:        f32,
    /// Signal-to-Noise Ratio (dB).
    snr:        f32,
    /// Inter-channel alpha coherence.
    coherence:  f32,
    /// Mu suppression index.
    mu_suppression: f32,
    /// Mood index (0–100).
    mood:       f32,
    tbr:                f32,
    sef95:              f32,
    spectral_centroid:  f32,
    hjorth_activity:    f32,
    hjorth_mobility:    f32,
    hjorth_complexity:  f32,
    permutation_entropy: f32,
    higuchi_fd:         f32,
    dfa_exponent:       f32,
    sample_entropy:     f32,
    pac_theta_gamma:    f32,
    laterality_index:   f32,
    // ── PPG-derived metrics ──────────────────────────────────────────
    hr:               f64,
    rmssd:            f64,
    sdnn:             f64,
    pnn50:            f64,
    lf_hf_ratio:      f64,
    respiratory_rate: f64,
    spo2_estimate:    f64,
    perfusion_index:  f64,
    stress_index:     f64,
    // ── Artifact / event metrics ─────────────────────────────────────
    blink_count:      u64,
    blink_rate:       f64,

    // ── Head pose ────────────────────────────────────────────────────
    head_pitch:       f64,
    head_roll:        f64,
    stillness:        f64,
    nod_count:        u64,
    shake_count:      u64,
    // ── Composite scores ─────────────────────────────────────────────
    meditation:       f64,
    cognitive_load:   f64,
    drowsiness:       f64,
    // ── Headache / Migraine EEG correlate indices (0–100) ───────────────────
    headache_index:      f32,
    migraine_index:      f32,
    // ── Consciousness metrics (0–100) ─────────────────────────────────
    consciousness_lzc:          f32,
    consciousness_wakefulness:  f32,
    consciousness_integration:  f32,
}

impl EpochMetrics {
    /// Derive metrics from a `BandSnapshot` by averaging across all channels.
    fn from_snapshot(snap: &crate::eeg_bands::BandSnapshot) -> Self {
        let n = snap.channels.len() as f32;
        if n < 1.0 {
            return Self::default();
        }

        let mut rd = 0.0f32; let mut rt = 0.0f32;
        let mut ra = 0.0f32; let mut rb = 0.0f32;
        let mut rg = 0.0f32; let mut rhg = 0.0f32;
        let mut sum_relax = 0.0f32;
        let mut sum_engage = 0.0f32;

        for ch in &snap.channels {
            rd += ch.rel_delta;
            rt += ch.rel_theta;
            ra += ch.rel_alpha;
            rb += ch.rel_beta;
            rg += ch.rel_gamma;
            rhg += ch.rel_high_gamma;

            let a = ch.rel_alpha;
            let b = ch.rel_beta;
            let t = ch.rel_theta;
            let d1 = a + t;
            let d2 = b + t;
            if d2 > 1e-6 { sum_relax  += a / d2; }
            if d1 > 1e-6 { sum_engage += b / d1; }
        }

        rd /= n; rt /= n; ra /= n; rb /= n; rg /= n; rhg /= n;
        let avg_relax  = sum_relax  / n;
        let avg_engage = sum_engage / n;

        // Frontal Alpha Asymmetry: ln(AF8 α) − ln(AF7 α).
        // AF7 = channels[1], AF8 = channels[2].  Floor at 1e-6 to avoid ln(0).
        let faa = if snap.channels.len() >= 3 {
            let af7_alpha = snap.channels[1].alpha.max(1e-6);
            let af8_alpha = snap.channels[2].alpha.max(1e-6);
            af8_alpha.ln() - af7_alpha.ln()
        } else {
            0.0
        };

        Self {
            rel_delta: rd, rel_theta: rt, rel_alpha: ra, rel_beta: rb, rel_gamma: rg, rel_high_gamma: rhg,
            relaxation: Self::sigmoid100(avg_relax,  2.5, 1.0),
            engagement: Self::sigmoid100(avg_engage, 2.0, 0.8),
            faa,
            tar:            snap.tar,
            bar:            snap.bar,
            dtr:            snap.dtr,
            pse:            snap.pse,
            apf:            snap.apf,
            bps:            snap.bps,
            snr:            snap.snr,
            coherence:      snap.coherence,
            mu_suppression: snap.mu_suppression,
            mood:           snap.mood,
            tbr:                snap.tbr,
            sef95:              snap.sef95,
            spectral_centroid:  snap.spectral_centroid,
            hjorth_activity:    snap.hjorth_activity,
            hjorth_mobility:    snap.hjorth_mobility,
            hjorth_complexity:  snap.hjorth_complexity,
            permutation_entropy: snap.permutation_entropy,
            higuchi_fd:         snap.higuchi_fd,
            dfa_exponent:       snap.dfa_exponent,
            sample_entropy:     snap.sample_entropy,
            pac_theta_gamma:    snap.pac_theta_gamma,
            laterality_index:   snap.laterality_index,
            // PPG metrics are populated separately (not from BandSnapshot)
            hr: 0.0, rmssd: 0.0, sdnn: 0.0, pnn50: 0.0,
            lf_hf_ratio: 0.0, respiratory_rate: 0.0,
            spo2_estimate: 0.0, perfusion_index: 0.0, stress_index: 0.0,
            // Artifact / head pose / composite — populated from BandSnapshot optional fields
            blink_count:      snap.blink_count.unwrap_or(0),
            blink_rate:       snap.blink_rate.unwrap_or(0.0),

            head_pitch:       snap.head_pitch.unwrap_or(0.0),
            head_roll:        snap.head_roll.unwrap_or(0.0),
            stillness:        snap.stillness.unwrap_or(0.0),
            nod_count:        snap.nod_count.unwrap_or(0),
            shake_count:      snap.shake_count.unwrap_or(0),
            meditation:       snap.meditation.unwrap_or(0.0),
            cognitive_load:   snap.cognitive_load.unwrap_or(0.0),
            drowsiness:       snap.drowsiness.unwrap_or(0.0),
            headache_index:      snap.headache_index,
            migraine_index:      snap.migraine_index,
            consciousness_lzc:          snap.consciousness_lzc,
            consciousness_wakefulness:  snap.consciousness_wakefulness,
            consciousness_integration:  snap.consciousness_integration,
        }
    }

    /// Sigmoid mapping (0, ∞) → (0, 100) with tuneable steepness and midpoint.
    fn sigmoid100(x: f32, k: f32, mid: f32) -> f32 {
        100.0 / (1.0 + (-k * (x - mid)).exp())
    }
}

impl Default for EpochMetrics {
    fn default() -> Self {
        Self {
            rel_delta: 0.0, rel_theta: 0.0, rel_alpha: 0.0, rel_beta: 0.0, rel_gamma: 0.0, rel_high_gamma: 0.0,
            relaxation: 0.0, engagement: 0.0, faa: 0.0,
            tar: 0.0, bar: 0.0, dtr: 0.0, pse: 0.0, apf: 0.0,
            bps: 0.0, snr: 0.0, coherence: 0.0, mu_suppression: 1.0, mood: 50.0,
            tbr: 0.0, sef95: 0.0, spectral_centroid: 0.0,
            hjorth_activity: 0.0, hjorth_mobility: 0.0, hjorth_complexity: 0.0,
            permutation_entropy: 0.0, higuchi_fd: 0.0, dfa_exponent: 0.0,
            sample_entropy: 0.0, pac_theta_gamma: 0.0, laterality_index: 0.0,
            hr: 0.0, rmssd: 0.0, sdnn: 0.0, pnn50: 0.0,
            lf_hf_ratio: 0.0, respiratory_rate: 0.0,
            spo2_estimate: 0.0, perfusion_index: 0.0, stress_index: 0.0,
            blink_count: 0, blink_rate: 0.0,
            head_pitch: 0.0, head_roll: 0.0, stillness: 0.0, nod_count: 0, shake_count: 0,
            meditation: 0.0, cognitive_load: 0.0, drowsiness: 0.0,
            headache_index: 0.0, migraine_index: 0.0,
            consciousness_lzc: 0.0, consciousness_wakefulness: 0.0, consciousness_integration: 0.0,
        }
    }
}

// ── Per-day storage (HNSW index + SQLite database) ────────────────────────────

struct DayStore {
    index:      fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64>,
    index_path: PathBuf,
    db_path:    PathBuf,
    conn:       rusqlite::Connection,
    logger:     Arc<SkillLogger>,
}

impl DayStore {
    /// Open (or create) the HNSW index and SQLite DB for `date` inside
    /// `skill_dir`, using the supplied HNSW graph parameters.
    fn open(skill_dir: &Path, date: &str, hnsw_m: usize, hnsw_ef: usize, logger: Arc<SkillLogger>) -> Option<Self> {
        use fast_hnsw::{Builder, distance::Cosine, labeled::LabeledIndex};

        let dir = skill_dir.join(date);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            skill_log!(logger, "embedder", "mkdir {}: {e}", dir.display());
            return None;
        }

        // ── HNSW ─────────────────────────────────────────────────────────────
        let index_path = dir.join(HNSW_INDEX_FILE);
        let index: LabeledIndex<Cosine, i64> = if index_path.exists() {
            match LabeledIndex::load(&index_path, Cosine) {
                Ok(idx) => {
                    skill_log!(logger, "embedder",
                        "HNSW loaded ({} entries) — {}",
                        idx.len(), index_path.display()
                    );
                    idx
                }
                Err(e) => {
                    skill_log!(logger, "embedder", "HNSW load failed ({e}), fresh index");
                    Builder::new().m(hnsw_m).ef_construction(hnsw_ef).build_labeled(Cosine)
                }
            }
        } else {
            skill_log!(logger, "embedder", "new HNSW (M={hnsw_m}, ef={hnsw_ef}) → {}", index_path.display());
            Builder::new().m(hnsw_m).ef_construction(hnsw_ef).build_labeled(Cosine)
        };

        // ── SQLite ────────────────────────────────────────────────────────────
        let db_path = dir.join(SQLITE_FILE);
        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c)  => c,
            Err(e) => { skill_log!(logger, "embedder", "sqlite open {}: {e}", db_path.display()); return None; }
        };

        // Enable WAL so concurrent readers (compare window, history) never
        // block the writer and the writer never blocks them.
        // synchronous=NORMAL is safe with WAL and much faster than FULL.
        let _ = conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;"
        );

        let ddl = "
            CREATE TABLE IF NOT EXISTS embeddings (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp       INTEGER NOT NULL,
                device_id       TEXT,
                device_name     TEXT,
                hnsw_id         INTEGER NOT NULL,
                eeg_embedding   BLOB    NOT NULL,
                label           TEXT,
                extra_embedding BLOB,
                -- All computed metrics stored as a single JSON object.
                -- Query individual fields with:
                --   json_extract(metrics_json, '$.rel_delta')
                -- Available keys: rel_delta, rel_theta, rel_alpha, rel_beta,
                --   rel_gamma, rel_high_gamma, relaxation_score, engagement_score,
                --   faa, tar, bar, dtr, pse, apf, bps, snr, coherence,
                --   mu_suppression, mood, tbr, sef95, spectral_centroid,
                --   hjorth_activity, hjorth_mobility, hjorth_complexity,
                --   permutation_entropy, higuchi_fd, dfa_exponent, sample_entropy,
                --   pac_theta_gamma, laterality_index,
                --   hr, rmssd, sdnn, pnn50, lf_hf_ratio, respiratory_rate,
                --   spo2_estimate, perfusion_idx, stress_index,
                --   ppg_ambient, ppg_infrared, ppg_red,
                --   blink_count, blink_rate,
                --   head_pitch, head_roll, stillness, nod_count, shake_count,
                --   meditation, cognitive_load, drowsiness,
                --   headache_index, migraine_index,
                --   consciousness_lzc, consciousness_wakefulness,
                --   consciousness_integration,
                --   band_channels (array of per-channel band power objects)
                metrics_json    TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_timestamp ON embeddings (timestamp);
        ";
        // Migration: add metrics_json to databases created before this schema.
        // Old individual-column rows will have NULL metrics_json and are read
        // through the json_extract() fallback (which returns NULL → 0.0).
        let migrate = [
            "ALTER TABLE embeddings ADD COLUMN metrics_json TEXT",
        ];
        if let Err(e) = conn.execute_batch(ddl) {
            skill_log!(logger, "embedder", "sqlite DDL failed: {e}");
            return None;
        }
        // Run migration for existing databases (ignore "duplicate column" errors).
        for stmt in &migrate {
            let _ = conn.execute(stmt, []);
        }

        skill_log!(logger, "embedder", "day store ready — {}", dir.display());
        Some(Self { index, index_path, db_path, conn, logger })
    }

    /// Insert one embedding + optional metrics into both HNSW index and SQLite.
    /// Returns the HNSW insertion id (zero-based for this day).
    #[allow(clippy::too_many_arguments)]
    fn insert(
        &mut self,
        timestamp:          i64,
        device_id:          Option<&str>,
        device_name:        Option<&str>,
        embedding:          &[f32],
        metrics:            Option<&EpochMetrics>,
        ppg_averages:       Option<&[f64; 3]>,
        band_channels_json: Option<&str>,
    ) -> usize {
        // ── HNSW ─────────────────────────────────────────────────────────────
        let hnsw_id = self.index.insert(embedding.to_vec(), timestamp);

        if let Err(e) = self.index.save(&self.index_path) {
            skill_log!(self.logger, "embedder", "HNSW save error: {e}");
        }

        // ── SQLite ────────────────────────────────────────────────────────────
        let blob: Vec<u8> = embedding.iter().flat_map(|v| v.to_le_bytes()).collect();

        // Serialise all computed metrics into a single JSON object.
        // Consumers query individual fields with json_extract(metrics_json, '$.field').
        //
        // Built with serde_json::Map rather than the json!{} macro: the macro
        // expands recursively (one level per key) and panics the compiler with
        // "recursion limit reached" once the object exceeds ~30 fields.
        let metrics_json: Option<String> = metrics.map(|m| {
            use serde_json::{Map, Value};

            // Helper: f64 → Value::Number, or Null when the sensor returned 0.
            let nn = |v: f64| -> Value {
                if v > 0.0 { Value::Number(serde_json::Number::from_f64(v).unwrap_or(serde_json::Number::from(0))) }
                else { Value::Null }
            };
            // Helper: f32 → Value::Number (always present, even when zero).
            let f32v = |v: f32| -> Value {
                Value::Number(serde_json::Number::from_f64(v as f64).unwrap_or(serde_json::Number::from(0)))
            };
            // Helper: f64 → Value::Number (always present).
            let f64v = |v: f64| -> Value {
                Value::Number(serde_json::Number::from_f64(v).unwrap_or(serde_json::Number::from(0)))
            };
            // Helper: u64 → Value::Number.
            let u64v = |v: u64| -> Value { Value::Number(v.into()) };

            let mut o = Map::with_capacity(64);

            // Band powers
            o.insert("rel_delta".into(),      f32v(m.rel_delta));
            o.insert("rel_theta".into(),      f32v(m.rel_theta));
            o.insert("rel_alpha".into(),      f32v(m.rel_alpha));
            o.insert("rel_beta".into(),       f32v(m.rel_beta));
            o.insert("rel_gamma".into(),      f32v(m.rel_gamma));
            o.insert("rel_high_gamma".into(), f32v(m.rel_high_gamma));
            // Scores
            o.insert("relaxation_score".into(), f32v(m.relaxation));
            o.insert("engagement_score".into(), f32v(m.engagement));
            o.insert("faa".into(),              f32v(m.faa));
            // Cross-band ratios
            o.insert("tar".into(), f32v(m.tar));
            o.insert("bar".into(), f32v(m.bar));
            o.insert("dtr".into(), f32v(m.dtr));
            o.insert("tbr".into(), f32v(m.tbr));
            // Spectral
            o.insert("pse".into(),              f32v(m.pse));
            o.insert("apf".into(),              f32v(m.apf));
            o.insert("bps".into(),              f32v(m.bps));
            o.insert("snr".into(),              f32v(m.snr));
            o.insert("sef95".into(),            f32v(m.sef95));
            o.insert("spectral_centroid".into(),f32v(m.spectral_centroid));
            // Synchrony / suppression
            o.insert("coherence".into(),      f32v(m.coherence));
            o.insert("mu_suppression".into(), f32v(m.mu_suppression));
            // Composite
            o.insert("mood".into(), f32v(m.mood));
            // Hjorth
            o.insert("hjorth_activity".into(),   f32v(m.hjorth_activity));
            o.insert("hjorth_mobility".into(),   f32v(m.hjorth_mobility));
            o.insert("hjorth_complexity".into(), f32v(m.hjorth_complexity));
            // Nonlinear
            o.insert("permutation_entropy".into(), f32v(m.permutation_entropy));
            o.insert("higuchi_fd".into(),          f32v(m.higuchi_fd));
            o.insert("dfa_exponent".into(),        f32v(m.dfa_exponent));
            o.insert("sample_entropy".into(),      f32v(m.sample_entropy));
            o.insert("pac_theta_gamma".into(),     f32v(m.pac_theta_gamma));
            o.insert("laterality_index".into(),    f32v(m.laterality_index));
            // PPG-derived (null when sensor absent / zero)
            o.insert("hr".into(),               nn(m.hr));
            o.insert("rmssd".into(),            nn(m.rmssd));
            o.insert("sdnn".into(),             nn(m.sdnn));
            o.insert("pnn50".into(),            if m.hr > 0.0 { f64v(m.pnn50) } else { Value::Null });
            o.insert("lf_hf_ratio".into(),      nn(m.lf_hf_ratio));
            o.insert("respiratory_rate".into(), nn(m.respiratory_rate));
            o.insert("spo2_estimate".into(),    nn(m.spo2_estimate));
            o.insert("perfusion_idx".into(),    nn(m.perfusion_index));
            o.insert("stress_index".into(),     nn(m.stress_index));
            // Raw PPG averages
            if let Some(ppg) = ppg_averages {
                o.insert("ppg_ambient".into(),  f64v(ppg[0]));
                o.insert("ppg_infrared".into(), f64v(ppg[1]));
                o.insert("ppg_red".into(),      f64v(ppg[2]));
            }
            // Artifact / head pose
            o.insert("blink_count".into(),  u64v(m.blink_count));
            o.insert("blink_rate".into(),   f64v(m.blink_rate));
            o.insert("head_pitch".into(),   f64v(m.head_pitch));
            o.insert("head_roll".into(),    f64v(m.head_roll));
            o.insert("stillness".into(),    f64v(m.stillness));
            o.insert("nod_count".into(),    u64v(m.nod_count));
            o.insert("shake_count".into(),  u64v(m.shake_count));
            // Composite scores (null when absent)
            o.insert("meditation".into(),     nn(m.meditation));
            o.insert("cognitive_load".into(), nn(m.cognitive_load));
            o.insert("drowsiness".into(),     nn(m.drowsiness));
            // Headache / migraine
            o.insert("headache_index".into(), f32v(m.headache_index));
            o.insert("migraine_index".into(), f32v(m.migraine_index));
            // Consciousness
            o.insert("consciousness_lzc".into(),         f32v(m.consciousness_lzc));
            o.insert("consciousness_wakefulness".into(),  f32v(m.consciousness_wakefulness));
            o.insert("consciousness_integration".into(),  f32v(m.consciousness_integration));
            // Per-channel band powers — re-parse the pre-serialised JSON array
            // so it nests cleanly instead of being double-encoded as a string.
            let band_channels_val: Value = band_channels_json
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);
            o.insert("band_channels".into(), band_channels_val);

            Value::Object(o).to_string()
        });

        let r = self.conn.execute(
            "INSERT INTO embeddings
             (timestamp, device_id, device_name, hnsw_id, eeg_embedding, label, extra_embedding,
              metrics_json)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6)",
            rusqlite::params![
                timestamp, device_id, device_name, hnsw_id as i64, blob,
                metrics_json,
            ],
        );
        if let Err(e) = r { skill_log!(self.logger, "embedder", "sqlite insert: {e}"); }

        hnsw_id
    }

    fn hnsw_len(&self) -> usize { self.index.len() }

    /// Persist metrics to SQLite **without** a wgpu embedding or HNSW entry.
    ///
    /// Used when the GPU pipeline is unavailable (encoder not loaded, or the
    /// wgpu device's internal mutexes were poisoned by a cubecl panic).  Band
    /// and sleep metrics are still valuable without embeddings.
    fn insert_metrics_only(
        &mut self,
        timestamp:          i64,
        device_id:          Option<&str>,
        device_name:        Option<&str>,
        metrics:            Option<&EpochMetrics>,
        ppg_averages:       Option<&[f64; 3]>,
        band_channels_json: Option<&str>,
    ) -> usize {
        // Reuse the same JSON serialisation helper as `insert`.
        let metrics_json: Option<String> = metrics.map(|m| {
            use serde_json::{Map, Value};
            let nn  = |v: f64| -> Value { if v > 0.0 { Value::Number(serde_json::Number::from_f64(v).unwrap_or(serde_json::Number::from(0))) } else { Value::Null } };
            let f32v = |v: f32| -> Value { Value::Number(serde_json::Number::from_f64(v as f64).unwrap_or(serde_json::Number::from(0))) };
            let f64v = |v: f64| -> Value { Value::Number(serde_json::Number::from_f64(v).unwrap_or(serde_json::Number::from(0))) };
            let u64v = |v: u64| -> Value { Value::Number(v.into()) };

            let mut o = Map::with_capacity(64);
            o.insert("rel_delta".into(), f32v(m.rel_delta));
            o.insert("rel_theta".into(), f32v(m.rel_theta));
            o.insert("rel_alpha".into(), f32v(m.rel_alpha));
            o.insert("rel_beta".into(),  f32v(m.rel_beta));
            o.insert("rel_gamma".into(), f32v(m.rel_gamma));
            o.insert("rel_high_gamma".into(), f32v(m.rel_high_gamma));
            o.insert("relaxation_score".into(), f32v(m.relaxation));
            o.insert("engagement_score".into(), f32v(m.engagement));
            o.insert("faa".into(), f32v(m.faa));
            o.insert("tar".into(), f32v(m.tar));
            o.insert("bar".into(), f32v(m.bar));
            o.insert("dtr".into(), f32v(m.dtr));
            o.insert("tbr".into(), f32v(m.tbr));
            o.insert("pse".into(), f32v(m.pse));
            o.insert("apf".into(), f32v(m.apf));
            o.insert("bps".into(), f32v(m.bps));
            o.insert("snr".into(), f32v(m.snr));
            o.insert("sef95".into(), f32v(m.sef95));
            o.insert("spectral_centroid".into(), f32v(m.spectral_centroid));
            o.insert("coherence".into(),      f32v(m.coherence));
            o.insert("mu_suppression".into(), f32v(m.mu_suppression));
            o.insert("mood".into(), f32v(m.mood));
            o.insert("hjorth_activity".into(),   f32v(m.hjorth_activity));
            o.insert("hjorth_mobility".into(),   f32v(m.hjorth_mobility));
            o.insert("hjorth_complexity".into(), f32v(m.hjorth_complexity));
            o.insert("permutation_entropy".into(), f32v(m.permutation_entropy));
            o.insert("higuchi_fd".into(),          f32v(m.higuchi_fd));
            o.insert("dfa_exponent".into(),        f32v(m.dfa_exponent));
            o.insert("sample_entropy".into(),      f32v(m.sample_entropy));
            o.insert("pac_theta_gamma".into(),     f32v(m.pac_theta_gamma));
            o.insert("laterality_index".into(),    f32v(m.laterality_index));
            o.insert("hr".into(),               nn(m.hr));
            o.insert("rmssd".into(),            nn(m.rmssd));
            o.insert("sdnn".into(),             nn(m.sdnn));
            o.insert("pnn50".into(),            nn(m.pnn50));
            o.insert("lf_hf_ratio".into(),      nn(m.lf_hf_ratio));
            o.insert("respiratory_rate".into(), nn(m.respiratory_rate));
            o.insert("spo2_estimate".into(),    nn(m.spo2_estimate));
            o.insert("perfusion_idx".into(),    nn(m.perfusion_index));
            o.insert("stress_index".into(),     nn(m.stress_index));
            if let Some(ppg) = ppg_averages {
                o.insert("ppg_ambient".into(),  f64v(ppg[0]));
                o.insert("ppg_infrared".into(), f64v(ppg[1]));
                o.insert("ppg_red".into(),      f64v(ppg[2]));
            }
            o.insert("blink_count".into(),  u64v(m.blink_count));
            o.insert("blink_rate".into(),   f64v(m.blink_rate));
            o.insert("head_pitch".into(),   f64v(m.head_pitch));
            o.insert("head_roll".into(),    f64v(m.head_roll));
            o.insert("stillness".into(),    f64v(m.stillness));
            o.insert("nod_count".into(),    u64v(m.nod_count));
            o.insert("shake_count".into(),  u64v(m.shake_count));
            o.insert("meditation".into(),     nn(m.meditation));
            o.insert("cognitive_load".into(), nn(m.cognitive_load));
            o.insert("drowsiness".into(),     nn(m.drowsiness));
            o.insert("headache_index".into(), f32v(m.headache_index));
            o.insert("migraine_index".into(), f32v(m.migraine_index));
            o.insert("consciousness_lzc".into(),         f32v(m.consciousness_lzc));
            o.insert("consciousness_wakefulness".into(),  f32v(m.consciousness_wakefulness));
            o.insert("consciousness_integration".into(),  f32v(m.consciousness_integration));
            let band_channels_val: Value = band_channels_json
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);
            o.insert("band_channels".into(), band_channels_val);
            Value::Object(o).to_string()
        });

        // hnsw_id = 0 (sentinel) — no HNSW entry.
        let r = self.conn.execute(
            "INSERT INTO embeddings
             (timestamp, device_id, device_name, hnsw_id, eeg_embedding, label, extra_embedding,
              metrics_json)
             VALUES (?1, ?2, ?3, 0, NULL, NULL, NULL, ?4)",
            rusqlite::params![timestamp, device_id, device_name, metrics_json],
        );
        if let Err(e) = r { skill_log!(self.logger, "embedder", "sqlite metrics-only insert: {e}"); }
        0
    }
}

// ── EegAccumulator ────────────────────────────────────────────────────────────

/// Sliding-window EEG accumulator that triggers ZUNA embedding on 5-second
/// epochs with configurable overlap.
const PPG_CHANNELS: usize = 3;

pub struct EegAccumulator {
    bufs:        [VecDeque<f32>; EEG_CHANNELS],
    since_last:  [usize; EEG_CHANNELS],
    hop_samples: usize,
    device_id:   Option<String>,
    device_name: Option<String>,
    tx:          mpsc::SyncSender<EpochMsg>,
    /// Latest band power snapshot from the GPU-based BandAnalyzer.
    /// Attached to each epoch message so the worker can store derived metrics
    /// without recomputing any FFT.
    latest_bands: Option<crate::eeg_bands::BandSnapshot>,
    /// PPG sample accumulators [ambient, infrared, red].
    /// Accumulated between epoch boundaries, averaged and attached to each epoch,
    /// then cleared.
    ppg_sums:   [f64; PPG_CHANNELS],
    ppg_counts: [u64; PPG_CHANNELS],
    /// PPG signal analyzer for HR/HRV/SpO2 computation.
    ppg_analyzer: crate::ppg_analysis::PpgAnalyzer,
    /// Cached latest PPG metrics (updated each epoch, read by band snapshot emitter).
    latest_ppg: Option<crate::ppg_analysis::PpgMetrics>,
    logger:     Arc<SkillLogger>,
    // ── Worker-restart plumbing ───────────────────────────────────────────────
    // All fields below are cloned each time we (re)spawn the background worker.
    // They must be kept in sync with the parameters passed to `embed_worker`.
    skill_dir: PathBuf,
    config:    EegModelConfig,
    status:    Arc<Mutex<EegModelStatus>>,
    cancel:    Arc<std::sync::atomic::AtomicBool>,
}

impl EegAccumulator {
    pub fn new(
        skill_dir:   PathBuf,
        config:      EegModelConfig,
        status:      Arc<Mutex<EegModelStatus>>,
        cancel:      Arc<std::sync::atomic::AtomicBool>,
        logger:      Arc<SkillLogger>,
    ) -> Self {
        let tx = Self::spawn_worker(
            skill_dir.clone(), config.clone(),
            status.clone(), cancel.clone(), logger.clone(),
        );
        Self {
            bufs:        std::array::from_fn(|_| VecDeque::new()),
            since_last:  [0; EEG_CHANNELS],
            hop_samples: EMBEDDING_HOP_SAMPLES,
            device_id:    None,
            device_name:  None,
            tx,
            latest_bands: None,
            ppg_sums:   [0.0; PPG_CHANNELS],
            ppg_counts: [0; PPG_CHANNELS],
            ppg_analyzer: crate::ppg_analysis::PpgAnalyzer::new(10.0),
            latest_ppg: None,
            logger,
            skill_dir,
            config,
            status,
            cancel,
        }
    }

    /// Spawn a fresh `eeg-embed` worker thread and return the sender half of
    /// its channel.  Called both at construction time and whenever `push()`
    /// detects that the previous worker exited (e.g. after a cubecl panic).
    fn spawn_worker(
        skill_dir: PathBuf,
        config:    EegModelConfig,
        status:    Arc<Mutex<EegModelStatus>>,
        cancel:    Arc<std::sync::atomic::AtomicBool>,
        logger:    Arc<SkillLogger>,
    ) -> mpsc::SyncSender<EpochMsg> {
        let (tx, rx) = mpsc::sync_channel::<EpochMsg>(4);
        std::thread::Builder::new()
            .name("eeg-embed".into())
            .spawn(move || embed_worker(rx, skill_dir, config, status, cancel, logger))
            .expect("[embed] failed to spawn background thread");
        tx
    }

    /// Restart the background worker after it has exited unexpectedly.
    fn restart_worker(&mut self) {
        skill_log!(self.logger, "embedder", "restarting embed worker after unexpected exit");
        self.tx = Self::spawn_worker(
            self.skill_dir.clone(),
            self.config.clone(),
            self.status.clone(),
            self.cancel.clone(),
            self.logger.clone(),
        );
    }

    /// Update the latest band snapshot (called from lib.rs whenever the
    /// GPU-based BandAnalyzer produces a new result, ~4 Hz).
    pub fn update_bands(&mut self, snap: crate::eeg_bands::BandSnapshot) {
        self.latest_bands = Some(snap);
    }

    /// Update device info included in every subsequent epoch message.
    pub fn update_device(&mut self, id: Option<String>, name: Option<String>) {
        self.device_id   = id;
        self.device_name = name;
    }

    /// Update the overlap between consecutive epochs (seconds).
    pub fn set_overlap_secs(&mut self, secs: f32) {
        let clamped       = secs.clamp(EMBEDDING_OVERLAP_MIN_SECS, EMBEDDING_OVERLAP_MAX_SECS);
        let overlap_samps = (clamped * MUSE_SAMPLE_RATE).round() as usize;
        self.hop_samples  = EMBEDDING_EPOCH_SAMPLES.saturating_sub(overlap_samps).max(1);
        self.since_last   = [0; EEG_CHANNELS];
        skill_log!(self.logger, "embedder", "overlap set to {clamped:.2} s → hop={} samples", self.hop_samples);
    }

    /// Accumulate PPG samples for `channel` (0=ambient, 1=infrared, 2=red).
    /// These are averaged over the epoch window and stored alongside EEG embeddings.
    pub fn push_ppg(&mut self, channel: usize, samples: &[f64]) {
        if channel >= PPG_CHANNELS { return; }
        for &v in samples {
            self.ppg_sums[channel] += v;
            self.ppg_counts[channel] += 1;
        }
        // Also feed the PPG analyzer for HR/HRV computation
        self.ppg_analyzer.push(channel, samples);
    }

    /// Push raw µV samples for `electrode` (0–3).
    pub fn push(&mut self, electrode: usize, samples: &[f32]) {
        if electrode >= EEG_CHANNELS { return; }

        self.bufs[electrode].extend(samples.iter().copied());
        self.since_last[electrode] += samples.len();

        let min_buf        = self.bufs.iter().map(|b| b.len()).min().unwrap_or(0);
        let min_since_last = *self.since_last.iter().min().unwrap_or(&0);

        if min_buf < EMBEDDING_EPOCH_SAMPLES || min_since_last < self.hop_samples {
            return;
        }

        // Extract last EMBEDDING_EPOCH_SAMPLES from each channel.
        let epoch: Vec<Vec<f32>> = self.bufs.iter()
            .map(|b| b.iter().skip(b.len() - EMBEDDING_EPOCH_SAMPLES).copied().collect())
            .collect();

        for b in &mut self.bufs { b.drain(..self.hop_samples); }
        self.since_last = [0; EEG_CHANNELS];

        // Compute PPG averages for this epoch, then reset accumulators.
        let ppg_averages = if self.ppg_counts.iter().any(|&c| c > 0) {
            let avgs = std::array::from_fn(|i| {
                if self.ppg_counts[i] > 0 {
                    self.ppg_sums[i] / self.ppg_counts[i] as f64
                } else {
                    0.0
                }
            });
            self.ppg_sums   = [0.0; PPG_CHANNELS];
            self.ppg_counts = [0; PPG_CHANNELS];
            Some(avgs)
        } else {
            None
        };

        // Compute PPG-derived metrics (HR, HRV, SpO2, etc.)
        let ppg_epoch_samples = (crate::constants::EMBEDDING_EPOCH_SECS as f64 * 64.0) as usize;
        let ppg_metrics = self.ppg_analyzer.compute_epoch(ppg_epoch_samples);
        if let Some(ref pm) = ppg_metrics {
            self.latest_ppg = Some(pm.clone());
        }

        let msg = EpochMsg {
            samples:       epoch,
            timestamp:     yyyymmddhhmmss_utc(),
            device_id:     self.device_id.clone(),
            device_name:   self.device_name.clone(),
            band_snapshot: self.latest_bands.clone(),
            ppg_averages,
            ppg_metrics,
        };
        if let Err(e) = self.tx.try_send(msg) {
            match e {
                mpsc::TrySendError::Full(_) => {
                    skill_log!(self.logger, "embedder", "epoch dropped — worker busy (channel full)");
                }
                mpsc::TrySendError::Disconnected(_) => {
                    // The worker thread exited unexpectedly.  If the wgpu
                    // device was permanently poisoned by a cubecl panic, DO
                    // NOT respawn — the new worker would hit the same poisoned
                    // mutex immediately.  Let the accumulator go quiet instead;
                    // the next app restart will get a fresh process with clean
                    // device state.
                    if GPU_DEVICE_POISONED.load(std::sync::atomic::Ordering::Relaxed) {
                        skill_log!(self.logger, "embedder",
                            "worker exited and wgpu device is poisoned — NOT respawning; \
                             GPU embeddings disabled until app restart");
                    } else {
                        skill_log!(self.logger, "embedder",
                            "worker thread exited unexpectedly — respawning");
                        self.restart_worker();
                    }
                }
            }
        }
    }

    /// Return the most recently computed PPG metrics (if any).
    pub fn latest_ppg(&self) -> Option<&crate::ppg_analysis::PpgMetrics> {
        self.latest_ppg.as_ref()
    }

}

// ── Background worker ─────────────────────────────────────────────────────────

fn embed_worker(
    rx:        mpsc::Receiver<EpochMsg>,
    skill_dir: PathBuf,
    config:    EegModelConfig,
    status:    Arc<Mutex<EegModelStatus>>,
    cancel:    Arc<std::sync::atomic::AtomicBool>,
    logger:    Arc<SkillLogger>,
) {
    use burn::backend::{Wgpu, wgpu::WgpuDevice};
    use ndarray::Array2;
    use std::collections::HashMap;
    use zuna_rs::{ZunaEncoder, config::DataConfig, load_from_named_tensor};

    skill_log!(logger, "embedder", "worker started — skill_dir={}", skill_dir.display());

    // ── 1. Open today's DayStore immediately (files created before encoder) ───
    let mut current_date = yyyymmdd_utc();
    let mut store = DayStore::open(&skill_dir, &current_date, config.hnsw_m, config.hnsw_ef_construction, logger.clone());

    if let Some(ref s) = store {
        let mut st = status.lock_or_recover();
        st.daily_hnsw_path = s.index_path.display().to_string();
        st.daily_db_path   = s.db_path.display().to_string();
        st.embeddings_today = s.hnsw_len();
    }

    // ── 2. Locate ZUNA weights — download with exponential-backoff retry ─────
    // Backoff delays (seconds): 1 2 3 5 15 30 60 120 300 600 900 1800 1800 …
    const BACKOFF_SECS: &[u64] = &[1, 2, 3, 5, 15, 30, 60, 120, 300, 600, 900, 1800];

    let weights = resolve_hf_weights(&config.hf_repo).or_else(|| {
        use std::sync::atomic::Ordering;
        use std::time::Duration;

        let mut attempt = 0u32;
        loop {
            // Respect an explicit user cancellation before each attempt.
            if cancel.load(Ordering::Relaxed) {
                skill_log!(logger, "embedder", "auto-download cancelled by user — stopping retry loop");
                return None;
            }

            // Stamp the current attempt number so the UI can show it.
            {
                let mut st = status.lock_or_recover();
                st.download_retry_attempt = attempt;
                st.download_retry_in_secs = 0;
            }

            if let Some(w) = download_hf_weights(&config.hf_repo, &status, &cancel, false, &logger) {
                let mut st = status.lock_or_recover();
                st.download_retry_attempt = 0;
                st.download_retry_in_secs = 0;
                return Some(w);
            }

            // If the download function itself was cancelled (not just failed),
            // stop the auto-retry so the user is in control.
            if cancel.load(Ordering::Relaxed) {
                skill_log!(logger, "embedder", "download cancelled mid-attempt — stopping auto-retry");
                let mut st = status.lock_or_recover();
                st.download_retry_in_secs = 0;
                return None;
            }

            let delay = BACKOFF_SECS.get(attempt as usize).copied().unwrap_or(1800);
            attempt += 1;
            skill_log!(logger, "embedder", "download failed — retrying in {delay}s (attempt {attempt})");

            // Countdown: drain the epoch channel every second so the bounded
            // sender does not block the EEG accumulator during a long wait.
            for remaining in (1..=delay).rev() {
                {
                    let mut st = status.lock_or_recover();
                    st.download_retry_in_secs = remaining;
                }
                // Drain any queued epochs (encoder not ready yet — discard).
                while rx.try_recv().is_ok() {}
                std::thread::sleep(Duration::from_secs(1));
                if cancel.load(Ordering::Relaxed) {
                    skill_log!(logger, "embedder", "retry wait cancelled by user");
                    let mut st = status.lock_or_recover();
                    st.download_retry_in_secs = 0;
                    return None;
                }
            }
            {
                let mut st = status.lock_or_recover();
                st.download_retry_in_secs = 0;
            }
        }
    });

    {
        let mut st = status.lock_or_recover();
        st.weights_found = weights.is_some();
        st.weights_path  = weights.as_ref().map(|(w, _)| w.display().to_string());
    }
    if weights.is_none() {
        skill_log!(logger, "embedder", "ZUNA weights unavailable — embeddings skipped.");
    }

    // ── 3. Load ZUNA encoder on wgpu ─────────────────────────────────────────
    //
    // Pre-create the cubecl kernel-cache directory tree.
    // cubecl-common 0.9.0 uses dirs::home_dir().join(".cache/cubecl/0.9.0/…")
    // as its cache root.  On macOS ~/.cache/ does not exist by default, so
    // cubecl's own create_dir_all silently fails and File::create panics
    // (ENOENT).  This is invisible from a terminal (cache already built up from
    // prior runs) but always hits on a fresh .app launch.
    configure_cubecl_cache(&skill_dir);

    // If a previous worker already poisoned the wgpu device's internal mutexes,
    // skip GPU entirely — there is no recovery short of restarting the process.
    if GPU_DEVICE_POISONED.load(std::sync::atomic::Ordering::Relaxed) {
        skill_log!(logger, "embedder",
            "wgpu device poisoned from a previous panic — \
             GPU embeddings disabled for this process; metrics-only mode");
        for msg in rx { store_metrics_only(&msg, &mut store, &status, &logger, &skill_dir, &config); }
        return;
    }

    let device = WgpuDevice::DefaultDevice;

    // Wrap the encoder load in `catch_unwind` so that a cubecl panic does not
    // kill the entire thread.  If it panics we mark the device poisoned and
    // fall back to metrics-only mode.
    let mut encoder: Option<ZunaEncoder<Wgpu>> = weights.and_then(|(w, c)| {
        skill_log!(logger, "embedder", "loading ZUNA encoder from {}", w.display());
        let load_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ZunaEncoder::<Wgpu>::load(&c, &w, device.clone())
        }));
        match load_result {
            Ok(Ok((enc, ms))) => {
                let desc = enc.describe().to_string();
                skill_log!(logger, "embedder", "encoder ready ({ms:.0} ms) — {desc}");
                let mut st = status.lock_or_recover();
                st.encoder_loaded   = true;
                st.encoder_describe = Some(desc);
                Some(enc)
            }
            Ok(Err(e)) => {
                skill_log!(logger, "embedder", "encoder load failed: {e:#}");
                None
            }
            Err(panic_payload) => {
                skill_log!(logger, "embedder",
                    "encoder load panicked (cubecl cache issue?): {} — \
                     marking wgpu device poisoned; GPU embeddings disabled",
                    panic_msg(&panic_payload));
                GPU_DEVICE_POISONED.store(true, std::sync::atomic::Ordering::Relaxed);
                None
            }
        }
    });

    let ch_names: Vec<&str>                     = CHANNEL_NAMES.to_vec();
    let data_cfg                                 = DataConfig::default();
    let pos_overrides: HashMap<String, [f32; 3]> = HashMap::new();

    // ── 4. Process epoch messages ─────────────────────────────────────────────
    for msg in rx {
        // Midnight UTC rollover — rotate both HNSW and SQLite.
        let today = yyyymmdd_utc();
        if today != current_date {
            skill_log!(logger, "embedder", "date rolled over {current_date} → {today}");
            current_date = today;
            store = DayStore::open(&skill_dir, &current_date, config.hnsw_m, config.hnsw_ef_construction, logger.clone());
            if let Some(ref s) = store {
                let mut st = status.lock_or_recover();
                st.daily_hnsw_path  = s.index_path.display().to_string();
                st.daily_db_path    = s.db_path.display().to_string();
                st.embeddings_today = 0;
            }
        }

        let Some(ref mut s) = store   else {
            skill_log!(logger, "embedder", "no day store — skipping epoch");
            continue;
        };

        // ── Preprocess + encode on wgpu ───────────────────────────────────────
        let flat: Vec<f32> = msg.samples.iter().flatten().copied().collect();
        let array = match Array2::from_shape_vec((EEG_CHANNELS, EMBEDDING_EPOCH_SAMPLES), flat) {
            Ok(a)  => a,
            Err(e) => { skill_log!(logger, "embedder", "array shape error: {e}"); continue; }
        };

        // If encoder is None (GPU never loaded or was poisoned), fall through
        // to metrics-only storage below.

        // ── GPU pipeline (optional — skipped when encoder is None) ─────────
        //
        // Both load_from_named_tensor (tensor prep on the wgpu device) AND
        // encode_batches (inference) can panic when cubecl's internal mutex has
        // been poisoned by an earlier panic.  A single catch_unwind covers the
        // whole pipeline:
        //   • First epoch, fresh .app launch (no ~/.cache/cubecl/):
        //       CacheFile::new → create_dir_all silently fails → File::create
        //       returns ENOENT → .unwrap() panics
        //   • Every subsequent epoch on the same poisoned device:
        //       SharedStateMap::lock → "poisoned lock" panic
        //
        // On ANY panic we mark the wgpu device permanently unusable for this
        // process lifetime (respawning the thread does not help — the global
        // wgpu device's mutexes stay poisoned) and set encoder = None so the
        // loop continues in metrics-only mode.
        let mean_emb: Option<Vec<f32>> = if let Some(ref enc) = encoder {
            let gpu_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut batches = load_from_named_tensor::<Wgpu>(
                    array, &ch_names, MUSE_SAMPLE_RATE, config.data_norm,
                    &pos_overrides, &data_cfg, &device,
                ).map_err(|e| format!("preprocess: {e:#}"))?;

                if batches.is_empty() { return Err::<Vec<f32>, String>("empty batch".into()); }

                let mut epochs = enc.encode_batches(batches.drain(..1).collect())
                    .map_err(|e| format!("encode: {e:#}"))?;

                let epoch = epochs.pop().ok_or("no epoch output")?;
                let dim   = epoch.output_dim();
                let n_tok = epoch.n_tokens();
                if dim == 0 || n_tok == 0 { return Err("zero dim/tokens".into()); }

                let mut mean_emb = vec![0f32; dim];
                for tok in epoch.embeddings.chunks(dim) {
                    for (i, &v) in tok.iter().enumerate() { mean_emb[i] += v; }
                }
                let scale = 1.0 / n_tok as f32;
                for v in &mut mean_emb { *v *= scale; }
                Ok(mean_emb)
            }));

            match gpu_result {
                Ok(Ok(emb))  => Some(emb),
                Ok(Err(msg)) => {
                    skill_log!(logger, "embedder", "GPU pipeline error: {msg}");
                    None
                }
                Err(payload) => {
                    skill_log!(logger, "embedder",
                        "GPU pipeline panicked (cubecl — wgpu device now poisoned): {} — \
                         disabling GPU embeddings for this process",
                        panic_msg(&payload));
                    GPU_DEVICE_POISONED.store(true, std::sync::atomic::Ordering::Relaxed);
                    encoder = None;
                    None
                }
            }
        } else {
            None  // encoder unavailable — metrics-only mode
        };

        // ── Derive metrics from the band snapshot ───────────────────────────
        let metrics = msg.band_snapshot.as_ref().map(|snap| {
            let mut m = EpochMetrics::from_snapshot(snap);
            // Merge PPG-derived metrics if available
            if let Some(ref ppg) = msg.ppg_metrics {
                m.hr               = ppg.hr;
                m.rmssd            = ppg.rmssd;
                m.sdnn             = ppg.sdnn;
                m.pnn50            = ppg.pnn50;
                m.lf_hf_ratio      = ppg.lf_hf_ratio;
                m.respiratory_rate = ppg.respiratory_rate;
                m.spo2_estimate    = ppg.spo2_estimate;
                m.perfusion_index  = ppg.perfusion_index;
                m.stress_index     = ppg.stress_index;
            }
            m
        });

        // Serialise per-channel band powers as JSON for full-fidelity storage.
        let channels_json = msg.band_snapshot.as_ref().map(|snap| {
            let channels: Vec<serde_json::Value> = snap.channels.iter().map(|ch| {
                serde_json::json!({
                    "channel":        ch.channel,
                    "delta":          ch.delta,
                    "theta":          ch.theta,
                    "alpha":          ch.alpha,
                    "beta":           ch.beta,
                    "gamma":          ch.gamma,
                    "high_gamma":     ch.high_gamma,
                    "rel_delta":      ch.rel_delta,
                    "rel_theta":      ch.rel_theta,
                    "rel_alpha":      ch.rel_alpha,
                    "rel_beta":       ch.rel_beta,
                    "rel_gamma":      ch.rel_gamma,
                    "rel_high_gamma": ch.rel_high_gamma,
                    "dominant":       ch.dominant,
                })
            }).collect();
            serde_json::to_string(&channels).unwrap_or_default()
        });

        // ── Store ─────────────────────────────────────────────────────────────
        let has_metrics  = metrics.is_some();
        let has_channels = channels_json.is_some();
        let channels_len = channels_json.as_ref().map(|s| s.len()).unwrap_or(0);

        // When the GPU pipeline produced an embedding, store it in both HNSW
        // and SQLite.  When it didn't (encoder unavailable / device poisoned),
        // fall through to metrics-only SQLite storage.
        let hnsw_id = if let Some(ref emb) = mean_emb {
            s.insert(
                msg.timestamp,
                msg.device_id.as_deref(),
                msg.device_name.as_deref(),
                emb,
                metrics.as_ref(),
                msg.ppg_averages.as_ref(),
                channels_json.as_deref(),
            )
        } else {
            // Metrics-only path: write a SQLite row without an embedding or
            // HNSW entry so that band/sleep metrics are still persisted.
            s.insert_metrics_only(
                msg.timestamp,
                msg.device_id.as_deref(),
                msg.device_name.as_deref(),
                metrics.as_ref(),
                msg.ppg_averages.as_ref(),
                channels_json.as_deref(),
            )
        };
        if !has_metrics || !has_channels {
            eprintln!(
                "[embed] ⚠ band data incomplete: metrics={has_metrics} channels_json={has_channels} (len={channels_len}) — band_snapshot was {}",
                if msg.band_snapshot.is_some() { "Some" } else { "None" }
            );
        }

        // Verify the row was stored correctly (first 3 rows only, to avoid log spam).
        let total = s.hnsw_len();
        if total <= 3 {
            match s.conn.query_row(
                "SELECT json_extract(metrics_json,'$.tar'), json_extract(metrics_json,'$.bar'), metrics_json IS NOT NULL, json_extract(metrics_json,'$.rel_high_gamma'), json_extract(metrics_json,'$.tbr'), json_extract(metrics_json,'$.sef95'), json_extract(metrics_json,'$.hjorth_activity'), json_extract(metrics_json,'$.permutation_entropy'), json_extract(metrics_json,'$.higuchi_fd'), json_extract(metrics_json,'$.dfa_exponent'), json_extract(metrics_json,'$.sample_entropy'), json_extract(metrics_json,'$.pac_theta_gamma'), json_extract(metrics_json,'$.laterality_index') FROM embeddings WHERE id = (SELECT MAX(id) FROM embeddings)",
                [],
                |row| Ok((
                    row.get::<_, Option<f64>>(0).ok().flatten(),
                    row.get::<_, Option<f64>>(1).ok().flatten(),
                    row.get::<_, bool>(2).unwrap_or(false),
                    row.get::<_, Option<f64>>(3).ok().flatten(),
                    row.get::<_, Option<f64>>(4).ok().flatten(),
                    row.get::<_, Option<f64>>(5).ok().flatten(),
                    row.get::<_, Option<f64>>(6).ok().flatten(),
                    row.get::<_, Option<f64>>(7).ok().flatten(),
                    row.get::<_, Option<f64>>(8).ok().flatten(),
                    row.get::<_, Option<f64>>(9).ok().flatten(),
                    row.get::<_, Option<f64>>(10).ok().flatten(),
                    row.get::<_, Option<f64>>(11).ok().flatten(),
                    row.get::<_, Option<f64>>(12).ok().flatten(),
                )),
            ) {
                Ok((tar, bar, has_json, rhg, tbr, sef95, ha, pe, hfd, dfa, se, pac, lat)) => {
                    eprintln!(
                        "[embed] ✓ verify row: tar={tar:?} bar={bar:?} has_channels_json={has_json} rel_high_gamma={rhg:?} tbr={tbr:?} sef95={sef95:?} hjorth_activity={ha:?} pe={pe:?} hfd={hfd:?} dfa={dfa:?} se={se:?} pac={pac:?} lat={lat:?}"
                    );
                }
                Err(e) => skill_log!(logger, "embedder", "✗ verify query failed: {e}"),
            }
        }
        {
            let mut st = status.lock_or_recover();
            st.embeddings_today = total;
            // Publish latest epoch metrics so the WS status command can return them.
            st.latest_metrics = metrics.as_ref().map(|m| {
                crate::eeg_model_config::LatestEpochMetrics {
                    rel_delta:        m.rel_delta,
                    rel_theta:        m.rel_theta,
                    rel_alpha:        m.rel_alpha,
                    rel_beta:         m.rel_beta,
                    rel_gamma:        m.rel_gamma,
                    rel_high_gamma:   m.rel_high_gamma,
                    relaxation_score: m.relaxation,
                    engagement_score: m.engagement,
                    faa:              m.faa,
                    tar:              m.tar,
                    bar:              m.bar,
                    dtr:              m.dtr,
                    pse:              m.pse,
                    apf:              m.apf,
                    bps:              m.bps,
                    snr:              m.snr,
                    coherence:        m.coherence,
                    mu_suppression:   m.mu_suppression,
                    mood:             m.mood,
                    tbr:              m.tbr,
                    sef95:            m.sef95,
                    spectral_centroid: m.spectral_centroid,
                    hjorth_activity:  m.hjorth_activity,
                    hjorth_mobility:  m.hjorth_mobility,
                    hjorth_complexity: m.hjorth_complexity,
                    permutation_entropy: m.permutation_entropy,
                    higuchi_fd:       m.higuchi_fd,
                    dfa_exponent:     m.dfa_exponent,
                    sample_entropy:   m.sample_entropy,
                    pac_theta_gamma:  m.pac_theta_gamma,
                    laterality_index: m.laterality_index,
                    hr:               m.hr,
                    rmssd:            m.rmssd,
                    sdnn:             m.sdnn,
                    pnn50:            m.pnn50,
                    lf_hf_ratio:      m.lf_hf_ratio,
                    respiratory_rate: m.respiratory_rate,
                    spo2_estimate:    m.spo2_estimate,
                    perfusion_index:  m.perfusion_index,
                    stress_index:     m.stress_index,
                    blink_count:      m.blink_count,
                    blink_rate:       m.blink_rate,
                    head_pitch:       m.head_pitch,
                    head_roll:        m.head_roll,
                    stillness:        m.stillness,
                    nod_count:        m.nod_count,
                    shake_count:      m.shake_count,
                    meditation:       m.meditation,
                    cognitive_load:   m.cognitive_load,
                    drowsiness:       m.drowsiness,
                    headache_index:         m.headache_index,
                    migraine_index:         m.migraine_index,
                    consciousness_lzc:          m.consciousness_lzc,
                    consciousness_wakefulness:  m.consciousness_wakefulness,
                    consciousness_integration:  m.consciousness_integration,
                    epoch_timestamp:  msg.timestamp,
                }
            });
        }

        let dim = mean_emb.as_ref().map(|e| e.len()).unwrap_or(0);
        if let Some(ref m) = metrics {
            eprintln!(
                "[embed] #{hnsw_id} ts={} dev={} dim={dim} relax={:.0} engage={:.0} faa={:.3} tar={:.2} bar={:.2} dtr={:.2} pse={:.2} apf={:.1} bps={:.2} snr={:.1} coh={:.3} mu={:.3} mood={:.0}",
                msg.timestamp,
                msg.device_name.as_deref().unwrap_or("?"),
                m.relaxation, m.engagement, m.faa,
                m.tar, m.bar, m.dtr, m.pse, m.apf, m.bps, m.snr,
                m.coherence, m.mu_suppression, m.mood,
            );
        } else {
            eprintln!(
                "[embed] #{hnsw_id} ts={} dev={} dim={dim} (no band data — metrics will be NULL)",
                msg.timestamp,
                msg.device_name.as_deref().unwrap_or("?"),
            );
        }
    }

    skill_log!(logger, "embedder", "worker exiting");
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Metrics-only drain used by the early-exit path when the wgpu device is
/// permanently poisoned from a previous panic in this process.
fn store_metrics_only(
    msg:       &EpochMsg,
    store:     &mut Option<DayStore>,
    status:    &Arc<Mutex<EegModelStatus>>,
    logger:    &Arc<SkillLogger>,
    skill_dir: &Path,
    config:    &EegModelConfig,
) {
    // Rotate DayStore on midnight UTC rollover.
    let today = yyyymmdd_utc();
    let needs_rotate = store.as_ref()
        .map(|s| !s.db_path.to_string_lossy().contains(&today))
        .unwrap_or(true);
    if needs_rotate {
        *store = DayStore::open(skill_dir, &today, config.hnsw_m, config.hnsw_ef_construction, logger.clone());
    }
    let Some(ref mut s) = store else { return; };

    let metrics = msg.band_snapshot.as_ref().map(|snap| {
        let mut m = EpochMetrics::from_snapshot(snap);
        if let Some(ref ppg) = msg.ppg_metrics {
            m.hr               = ppg.hr;
            m.rmssd            = ppg.rmssd;
            m.sdnn             = ppg.sdnn;
            m.pnn50            = ppg.pnn50;
            m.lf_hf_ratio      = ppg.lf_hf_ratio;
            m.respiratory_rate = ppg.respiratory_rate;
            m.spo2_estimate    = ppg.spo2_estimate;
            m.perfusion_index  = ppg.perfusion_index;
            m.stress_index     = ppg.stress_index;
        }
        m
    });

    let channels_json = msg.band_snapshot.as_ref().map(|snap| {
        let channels: Vec<serde_json::Value> = snap.channels.iter().map(|ch| {
            serde_json::json!({
                "channel": ch.channel, "delta": ch.delta, "theta": ch.theta,
                "alpha": ch.alpha, "beta": ch.beta, "gamma": ch.gamma,
                "high_gamma": ch.high_gamma, "rel_delta": ch.rel_delta,
                "rel_theta": ch.rel_theta, "rel_alpha": ch.rel_alpha,
                "rel_beta": ch.rel_beta, "rel_gamma": ch.rel_gamma,
                "rel_high_gamma": ch.rel_high_gamma, "dominant": ch.dominant,
            })
        }).collect();
        serde_json::to_string(&channels).unwrap_or_default()
    });

    s.insert_metrics_only(
        msg.timestamp,
        msg.device_id.as_deref(),
        msg.device_name.as_deref(),
        metrics.as_ref(),
        msg.ppg_averages.as_ref(),
        channels_json.as_deref(),
    );

    // Keep status.latest_metrics fresh so the UI doesn't go stale.
    if let Some(ref m) = metrics {
        let mut st = status.lock_or_recover();
        st.latest_metrics = Some(crate::eeg_model_config::LatestEpochMetrics {
            rel_delta: m.rel_delta, rel_theta: m.rel_theta, rel_alpha: m.rel_alpha,
            rel_beta: m.rel_beta, rel_gamma: m.rel_gamma, rel_high_gamma: m.rel_high_gamma,
            relaxation_score: m.relaxation, engagement_score: m.engagement,
            faa: m.faa, tar: m.tar, bar: m.bar, dtr: m.dtr, pse: m.pse,
            apf: m.apf, bps: m.bps, snr: m.snr, coherence: m.coherence,
            mu_suppression: m.mu_suppression, mood: m.mood, tbr: m.tbr,
            sef95: m.sef95, spectral_centroid: m.spectral_centroid,
            hjorth_activity: m.hjorth_activity, hjorth_mobility: m.hjorth_mobility,
            hjorth_complexity: m.hjorth_complexity,
            permutation_entropy: m.permutation_entropy,
            higuchi_fd: m.higuchi_fd, dfa_exponent: m.dfa_exponent,
            sample_entropy: m.sample_entropy, pac_theta_gamma: m.pac_theta_gamma,
            laterality_index: m.laterality_index,
            hr: m.hr, rmssd: m.rmssd, sdnn: m.sdnn, pnn50: m.pnn50,
            lf_hf_ratio: m.lf_hf_ratio, respiratory_rate: m.respiratory_rate,
            spo2_estimate: m.spo2_estimate, perfusion_index: m.perfusion_index,
            stress_index: m.stress_index,
            blink_count: m.blink_count, blink_rate: m.blink_rate,
            head_pitch: m.head_pitch, head_roll: m.head_roll,
            stillness: m.stillness, nod_count: m.nod_count, shake_count: m.shake_count,
            meditation: m.meditation, cognitive_load: m.cognitive_load,
            drowsiness: m.drowsiness, headache_index: m.headache_index,
            migraine_index: m.migraine_index,
            consciousness_lzc: m.consciousness_lzc,
            consciousness_wakefulness: m.consciousness_wakefulness,
            consciousness_integration: m.consciousness_integration,
            epoch_timestamp: msg.timestamp,
        });
    }
}

/// Current UTC date as `"YYYYMMDD"`.
fn yyyymmdd_utc() -> String {
    let mut days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86_400;
    let mut y = 1970u32;
    loop {
        let leap  = (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
        let in_yr = if leap { 366u64 } else { 365 };
        if days < in_yr { break; }
        days -= in_yr; y += 1;
    }
    let leap = (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    let ml: [u64; 12] = if leap { [31,29,31,30,31,30,31,31,30,31,30,31] }
                        else    { [31,28,31,30,31,30,31,31,30,31,30,31] };
    let mut m = 1u32;
    for &l in &ml { if days < l { break; } days -= l; m += 1; }
    format!("{y:04}{m:02}{d:02}", d = days + 1)
}

/// Current UTC time as the integer `YYYYMMDDHHmmss`.
pub fn yyyymmddhhmmss_utc() -> i64 {
    let s    = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let sec  = (s % 60) as u32;
    let min  = ((s / 60) % 60) as u32;
    let hour = ((s / 3600) % 24) as u32;
    let mut days = s / 86_400;
    let mut y = 1970u32;
    loop {
        let leap  = (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
        let in_yr = if leap { 366u64 } else { 365 };
        if days < in_yr { break; }
        days -= in_yr; y += 1;
    }
    let leap = (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    let ml: [u64; 12] = if leap { [31,29,31,30,31,30,31,31,30,31,30,31] }
                        else    { [31,28,31,30,31,30,31,31,30,31,30,31] };
    let mut m = 1u32;
    for &l in &ml { if days < l { break; } days -= l; m += 1; }
    let d = days as u32 + 1;
    (y as i64)*10_000_000_000 + (m as i64)*100_000_000 + (d as i64)*1_000_000
        + (hour as i64)*10_000 + (min as i64)*100 + sec as i64
}

/// Find ZUNA weights in the HuggingFace disk cache for the given `hf_repo`.
fn resolve_hf_weights(hf_repo: &str) -> Option<(PathBuf, PathBuf)> {
    // $HF_HOME overrides the default cache location; otherwise fall back to
    // the platform home directory.  On Windows $HOME is typically unset —
    // dirs::home_dir() reads USERPROFILE correctly on all platforms.
    let hf_home = std::env::var("HF_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join(".cache/huggingface/hub")
        });
    let snaps = hf_home
        .join(format!("models--{}", hf_repo.replace('/', "--")))
        .join("snapshots");
    let mut dirs: Vec<_> = std::fs::read_dir(&snaps).ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();
    dirs.sort_by_key(|e| e.metadata().and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH));
    for snap in dirs.into_iter().rev() {
        let w = snap.path().join(ZUNA_WEIGHTS_FILE);
        let c = snap.path().join(ZUNA_CONFIG_FILE);
        if w.exists() && c.exists() { return Some((w, c)); }
    }
    None
}

/// Download ZUNA weights from HuggingFace Hub using the `hf-hub` crate.
///
/// Called automatically by [`embed_worker`] when [`resolve_hf_weights`]
/// returns `None`, and also directly by the `trigger_weights_download` Tauri
/// command when the user presses the Download / Retry button.
///
/// Files are saved into the standard HF disk cache
/// (`~/.cache/huggingface/hub` or `$HF_HOME`), so a subsequent call to
/// [`resolve_hf_weights`] will find them without re-downloading.
///
/// * `cancel` — setting this `AtomicBool` to `true` aborts between the two
///   file downloads (config.json first, then the large safetensors).
/// * `mark_needs_restart` — when `true` and the download succeeds, sets
///   [`EegModelStatus::download_needs_restart`] so the UI can prompt the
///   user to restart the app and load the freshly downloaded encoder.
///   Pass `false` from [`embed_worker`] because the startup path loads the
///   encoder immediately after the download returns.
pub(crate) fn download_hf_weights(
    hf_repo:            &str,
    status:             &Arc<Mutex<EegModelStatus>>,
    cancel:             &Arc<std::sync::atomic::AtomicBool>,
    mark_needs_restart: bool,
    logger:             &Arc<SkillLogger>,
) -> Option<(PathBuf, PathBuf)> {
    use hf_hub::api::sync::Api;
    use std::sync::atomic::Ordering;

    skill_log!(logger, "embedder", "ZUNA weights not in cache — downloading from HuggingFace: {hf_repo}");

    // ── Mark download in progress ────────────────────────────────────────────
    {
        let mut st = status.lock_or_recover();
        st.downloading_weights  = true;
        st.download_needs_restart = false;
        st.download_status_msg  = Some(format!("Connecting to HuggingFace ({hf_repo})…"));
    }

    // ── Build the HF Hub API client ─────────────────────────────────────────
    let api = match Api::new() {
        Ok(a)  => a,
        Err(e) => {
            skill_log!(logger, "embedder", "hf-hub Api::new() failed: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_status_msg = Some(format!("HF Hub init failed: {e}"));
            return None;
        }
    };
    let repo = api.model(hf_repo.to_string());

    // ── Download config.json (small — comes first for quick feedback) ────────
    {
        let mut st = status.lock_or_recover();
        st.download_status_msg = Some(format!("Downloading {ZUNA_CONFIG_FILE}…"));
    }
    let config_path = match repo.get(ZUNA_CONFIG_FILE) {
        Ok(p)  => { skill_log!(logger, "embedder", "✓ {ZUNA_CONFIG_FILE} → {}", p.display()); p }
        Err(e) => {
            skill_log!(logger, "embedder", "failed to download {ZUNA_CONFIG_FILE}: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_status_msg = Some(format!("Download failed ({ZUNA_CONFIG_FILE}): {e}"));
            return None;
        }
    };

    // ── Honour cancellation between the two files ────────────────────────────
    if cancel.load(Ordering::Relaxed) {
        skill_log!(logger, "embedder", "download cancelled by user after config.json");
        let mut st = status.lock_or_recover();
        st.downloading_weights = false;
        st.download_status_msg = Some("Download cancelled.".to_string());
        return None;
    }

    // ── Download the weights safetensors (large — may take a while) ──────────
    {
        let mut st = status.lock_or_recover();
        st.download_status_msg = Some(format!("Downloading {ZUNA_WEIGHTS_FILE} (large file)…"));
    }
    let weights_path = match repo.get(ZUNA_WEIGHTS_FILE) {
        Ok(p)  => { skill_log!(logger, "embedder", "✓ {ZUNA_WEIGHTS_FILE} → {}", p.display()); p }
        Err(e) => {
            skill_log!(logger, "embedder", "failed to download {ZUNA_WEIGHTS_FILE}: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_status_msg = Some(format!("Download failed ({ZUNA_WEIGHTS_FILE}): {e}"));
            return None;
        }
    };

    // ── Download complete ────────────────────────────────────────────────────
    {
        let mut st = status.lock_or_recover();
        st.downloading_weights    = false;
        st.download_status_msg    = None;
        st.weights_found          = true;
        st.weights_path           = Some(weights_path.display().to_string());
        st.download_needs_restart = mark_needs_restart;
    }
    skill_log!(logger, "embedder", "ZUNA weights downloaded successfully");
    Some((weights_path, config_path))
}
