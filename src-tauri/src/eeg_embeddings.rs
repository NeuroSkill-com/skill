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
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::skill_log::SkillLogger;
use crate::settings::{HookLastTrigger, HookRule};

use crate::MutexExt;
use crate::{
    constants::{
        CHANNEL_NAMES, EEG_CHANNELS, EMBEDDING_EPOCH_SAMPLES, EMBEDDING_HOP_SAMPLES,
        EMBEDDING_OVERLAP_MAX_SECS, EMBEDDING_OVERLAP_MIN_SECS,
        GLOBAL_HNSW_SAVE_EVERY, HNSW_INDEX_FILE, MUSE_SAMPLE_RATE, SQLITE_FILE,

    },
    eeg_model_config::{EegModelConfig, EegModelStatus},
    global_eeg_index,
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
// configure_cubecl_cache, GPU_DEVICE_POISONED, panic_msg — delegated to skill_exg.
use skill_exg::configure_cubecl_cache;
use skill_exg::GPU_DEVICE_POISONED;
fn panic_msg(payload: &Box<dyn std::any::Any + Send>) -> &str {
    skill_exg::panic_msg(payload)
}

// EpochMetrics — re-exported from skill_exg.
use skill_exg::EpochMetrics;

/// Original EpochMetrics definition removed — now in skill_exg.
#[cfg(any())]
struct _OriginalEpochMetrics {
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

#[cfg(any())]
impl _OriginalEpochMetrics {
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

#[cfg(any())]
impl Default for _OriginalEpochMetrics {
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
    skill_dir:    PathBuf,
    config:       EegModelConfig,
    status:       Arc<Mutex<EegModelStatus>>,
    cancel:       Arc<std::sync::atomic::AtomicBool>,
    /// When set to `true` by `trigger_weights_download`, the running embed
    /// worker will exit its epoch loop and the accumulator will immediately
    /// respawn a fresh worker that re-runs `resolve_hf_weights` and loads the
    /// newly downloaded encoder — no app restart needed.
    reload_requested: Arc<std::sync::atomic::AtomicBool>,
    /// Shared reference to the persistent cross-day global HNSW index.
    /// `None` inside the Option while the startup build is still running.
    global_index: Arc<Mutex<Option<fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64>>>>,
    hooks: Vec<HookRule>,
    shared_embedder: Arc<crate::label_cmds::EmbedderState>,
    label_idx: Arc<crate::label_index::LabelIndexState>,
    ws_broadcaster: crate::ws_server::WsBroadcaster,
    hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,
    app: tauri::AppHandle,
}

impl EegAccumulator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        skill_dir:        PathBuf,
        config:           EegModelConfig,
        status:           Arc<Mutex<EegModelStatus>>,
        cancel:           Arc<std::sync::atomic::AtomicBool>,
        reload_requested: Arc<std::sync::atomic::AtomicBool>,
        logger:           Arc<SkillLogger>,
        global_index:     Arc<Mutex<Option<fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64>>>>,
        hooks:            Vec<HookRule>,
        shared_embedder:  Arc<crate::label_cmds::EmbedderState>,
        label_idx:        Arc<crate::label_index::LabelIndexState>,
        ws_broadcaster:   crate::ws_server::WsBroadcaster,
        hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,
        app: tauri::AppHandle,
    ) -> Self {
        let tx = Self::spawn_worker(
            skill_dir.clone(), config.clone(),
            status.clone(), cancel.clone(), reload_requested.clone(),
            logger.clone(), global_index.clone(),
            hooks.clone(), shared_embedder.clone(), label_idx.clone(), ws_broadcaster.clone(),
            hook_runtime.clone(), app.clone(),
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
            reload_requested,
            global_index,
            hooks,
            shared_embedder,
            label_idx,
            ws_broadcaster,
            hook_runtime,
            app,
        }
    }

    /// Spawn a fresh `eeg-embed` worker thread and return the sender half of
    /// its channel.  Called both at construction time and whenever `push()`
    /// detects that the previous worker exited (e.g. after a cubecl panic).
    #[allow(clippy::too_many_arguments)]
    fn spawn_worker(
        skill_dir:        PathBuf,
        config:           EegModelConfig,
        status:           Arc<Mutex<EegModelStatus>>,
        cancel:           Arc<std::sync::atomic::AtomicBool>,
        reload_requested: Arc<std::sync::atomic::AtomicBool>,
        logger:           Arc<SkillLogger>,
        global_index:     Arc<Mutex<Option<fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64>>>>,
        hooks:            Vec<HookRule>,
        shared_embedder:  Arc<crate::label_cmds::EmbedderState>,
        label_idx:        Arc<crate::label_index::LabelIndexState>,
        ws_broadcaster:   crate::ws_server::WsBroadcaster,
        hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,
        app: tauri::AppHandle,
    ) -> mpsc::SyncSender<EpochMsg> {
        let (tx, rx) = mpsc::sync_channel::<EpochMsg>(4);
        std::thread::Builder::new()
            .name("eeg-embed".into())
            .spawn(move || embed_worker(
                rx, skill_dir, config, status, cancel, reload_requested, logger, global_index,
                hooks, shared_embedder, label_idx, ws_broadcaster,
                hook_runtime, app,
            ))
            .expect("[embed] failed to spawn background thread");
        tx
    }

    /// Restart the background worker after it has exited unexpectedly (or
    /// after a deliberate reload request from `trigger_weights_download`).
    fn restart_worker(&mut self) {
        skill_log!(self.logger, "embedder", "restarting embed worker");
        self.tx = Self::spawn_worker(
            self.skill_dir.clone(),
            self.config.clone(),
            self.status.clone(),
            self.cancel.clone(),
            self.reload_requested.clone(),
            self.logger.clone(),
            self.global_index.clone(),
            self.hooks.clone(),
            self.shared_embedder.clone(),
            self.label_idx.clone(),
            self.ws_broadcaster.clone(),
            self.hook_runtime.clone(),
            self.app.clone(),
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

    /// Replace hook configuration and restart the worker so it picks up changes.
    pub fn set_hooks(&mut self, hooks: Vec<HookRule>) {
        self.hooks = hooks;
        self.restart_worker();
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

#[derive(serde::Serialize)]
struct HookBroadcastPayload {
    hook: String,
    context: String,
    command: String,
    text: String,
    scenario: String,
    distance: f32,
    label_id: i64,
    label_text: String,
    triggered_at_utc: u64,
}

struct HookReferenceSet {
    hook: HookRule,
    refs: Vec<HookReference>,
}

struct HookReference {
    emb: Vec<f32>,
    label_id: i64,
    label_text: String,
    eeg_start_utc: u64,
}

struct HookMatcher {
    skill_dir: PathBuf,
    hooks: Vec<HookRule>,
    label_idx: Arc<crate::label_index::LabelIndexState>,
    ws_broadcaster: crate::ws_server::WsBroadcaster,
    /// Shared app-wide text embedder — same instance used by labels and
    /// screenshot OCR.  Avoids loading a separate ~130 MB ONNX model copy.
    shared_embedder: Arc<crate::label_cmds::EmbedderState>,
    cache: Vec<HookReferenceSet>,
    last_refresh_unix: u64,
    last_fired_unix: HashMap<String, u64>,
    logger: Arc<SkillLogger>,
    hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,
    app: tauri::AppHandle,
    hooks_log: Option<crate::hooks_log::HooksLog>,
}

impl HookMatcher {
    #[allow(clippy::too_many_arguments)]
    fn new(
        skill_dir: PathBuf,
        hooks: Vec<HookRule>,
        shared_embedder: Arc<crate::label_cmds::EmbedderState>,
        label_idx: Arc<crate::label_index::LabelIndexState>,
        ws_broadcaster: crate::ws_server::WsBroadcaster,
        logger: Arc<SkillLogger>,
        hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,
        app: tauri::AppHandle,
    ) -> Self {
        let hooks_log = crate::hooks_log::HooksLog::open(&skill_dir);

        Self {
            skill_dir,
            hooks,
            label_idx,
            ws_broadcaster,
            shared_embedder,
            cache: Vec::new(),
            last_refresh_unix: 0,
            last_fired_unix: HashMap::new(),
            logger,
            hook_runtime,
            app,
            hooks_log,
        }
    }

    fn maybe_refresh(&mut self) {
        let now = unix_secs_now();
        if now.saturating_sub(self.last_refresh_unix) < 20 {
            return;
        }
        self.last_refresh_unix = now;

        let recent_labels = load_recent_label_texts(&self.skill_dir, 180);

        // ── Phase 1: batch-embed all hook keywords while holding the lock ─────
        // Collect (hook_index, queries, embeddings) tuples.
        struct HookQueries {
            hook_idx:   usize,
            embeddings: Vec<Vec<f32>>,
        }
        let mut hook_queries: Vec<HookQueries> = Vec::new();

        {
            let mut guard = match self.shared_embedder.0.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let Some(te) = guard.as_mut() else {
                self.cache.clear();
                return;
            };

            for (idx, hook) in self.hooks.iter().enumerate().filter(|(_, h)| h.enabled) {
                let mut queries: Vec<String> = hook.keywords
                    .iter()
                    .map(|k| k.trim().to_owned())
                    .filter(|k| !k.is_empty())
                    .collect();

                if queries.is_empty() { continue; }

                for label in &recent_labels {
                    if queries.iter().any(|k| fuzzy_match(k, label)) && !queries.iter().any(|q| q == label) {
                        queries.push(label.clone());
                    }
                }

                let query_refs: Vec<&str> = queries.iter().map(String::as_str).collect();
                let embeddings = match te.embed(query_refs, None) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                hook_queries.push(HookQueries { hook_idx: idx, embeddings });
            }
        } // ── lock released here ──────────────────────────────────────────────

        // ── Phase 2: HNSW search (no lock needed) ─────────────────────────────
        let mut next_cache: Vec<HookReferenceSet> = Vec::new();

        for hq in &hook_queries {
            let hook = &self.hooks[hq.hook_idx];

            let mut neighbors: Vec<crate::label_index::LabelNeighbor> = Vec::new();
            for qvec in &hq.embeddings {
                neighbors.extend(crate::label_index::search_by_text_vec(
                    qvec,
                    6,
                    64,
                    &self.skill_dir,
                    &self.label_idx,
                ));
            }

            neighbors.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
            let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
            let mut refs: Vec<HookReference> = Vec::new();

            for n in neighbors {
                if !seen.insert(n.label_id) {
                    continue;
                }
                if let Some(eeg_ref) = crate::label_index::mean_eeg_for_window(&self.skill_dir, n.eeg_start, n.eeg_end) {
                    refs.push(HookReference {
                        emb: eeg_ref,
                        label_id: n.label_id,
                        label_text: n.text,
                        eeg_start_utc: n.eeg_start,
                    });
                }
                if refs.len() >= hook.recent_limit.clamp(10, 20) {
                    break;
                }
            }

            if !refs.is_empty() {
                next_cache.push(HookReferenceSet {
                    hook: hook.clone(),
                    refs,
                });
            }
        }

        if !next_cache.is_empty() {
            skill_log!(self.logger, "hooks", "cache refreshed: {} active hooks", next_cache.len());
        }
        self.cache = next_cache;
    }

    fn scenario_allows_fire(scenario: &str, metrics: Option<&EpochMetrics>) -> bool {
        let s = scenario.trim().to_lowercase();
        if s.is_empty() || s == "any" {
            return true;
        }
        let Some(m) = metrics else {
            return false;
        };

        match s.as_str() {
            // Elevated cognitive effort / load.
            "cognitive" => (m.cognitive_load >= 55.0) || (m.engagement >= 60.0),
            // Stress / affective strain patterns.
            "emotional" => (m.stress_index >= 55.0) || (m.mood <= 45.0) || (m.relaxation <= 35.0),
            // Physiological fatigue / strain patterns.
            "physical" => {
                (m.drowsiness >= 55.0)
                    || (m.headache_index >= 45.0)
                    || (m.migraine_index >= 45.0)
                    || (m.hr > 0.0 && (m.hr >= 105.0 || m.hr <= 52.0))
            }
            _ => true,
        }
    }

    fn maybe_fire(&mut self, embedding: &[f32], metrics: Option<&EpochMetrics>) {
        self.maybe_refresh();
        if self.cache.is_empty() {
            return;
        }
        let now = unix_secs_now();

        for entry in &self.cache {
            if !Self::scenario_allows_fire(&entry.hook.scenario, metrics) {
                continue;
            }
            let threshold = entry.hook.distance_threshold.clamp(0.01, 1.0);
            let best = entry.refs.iter()
                .map(|r| (r, cosine_distance(embedding, &r.emb)))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            let Some((best_ref, min_dist)) = best else {
                continue;
            };

            if min_dist > threshold {
                continue;
            }

            let last = self.last_fired_unix.get(&entry.hook.name).copied().unwrap_or(0);
            if now.saturating_sub(last) < 10 {
                continue;
            }

            self.last_fired_unix.insert(entry.hook.name.clone(), now);
            let ts_utc = msg_ts_utc_now();
            self.hook_runtime.lock_or_recover().insert(
                entry.hook.name.clone(),
                HookLastTrigger {
                    triggered_at_utc: ts_utc,
                    distance: min_dist,
                    label_id: Some(best_ref.label_id),
                    label_text: Some(best_ref.label_text.clone()),
                    label_eeg_start_utc: Some(best_ref.eeg_start_utc),
                },
            );

            skill_log!(
                self.logger,
                "hooks",
                "triggered hook='{}' scenario='{}' distance={:.4} label='{}' label_id={}",
                entry.hook.name,
                entry.hook.scenario,
                min_dist,
                best_ref.label_text,
                best_ref.label_id
            );

            let payload = HookBroadcastPayload {
                hook: entry.hook.name.clone(),
                context: "labels".to_owned(),
                command: entry.hook.command.clone(),
                text: entry.hook.text.clone(),
                scenario: entry.hook.scenario.clone(),
                distance: min_dist,
                label_id: best_ref.label_id,
                label_text: best_ref.label_text.clone(),
                triggered_at_utc: ts_utc,
            };
            self.ws_broadcaster.send("hook", &payload);

            // ── Audit log ─────────────────────────────────────────────────────
            if let Some(ref log) = self.hooks_log {
                use serde_json::{json, to_string};
                let hook_json   = to_string(&entry.hook).unwrap_or_default();
                let trigger_json = to_string(&json!({
                    "triggered_at_utc": ts_utc,
                    "distance":          min_dist,
                    "label_id":          best_ref.label_id,
                    "label_text":        &best_ref.label_text,
                    "label_eeg_start_utc": best_ref.eeg_start_utc,
                })).unwrap_or_default();
                let payload_json = to_string(&json!({
                    "context": "labels",
                    "command": &entry.hook.command,
                    "text":    &entry.hook.text,
                })).unwrap_or_default();
                log.record(crate::hooks_log::HookFireEntry {
                    triggered_at_utc: ts_utc as i64,
                    hook_json:        &hook_json,
                    trigger_json:     &trigger_json,
                    payload_json:     &payload_json,
                });
            }

            crate::send_toast(
                &self.app,
                crate::ToastLevel::Info,
                "Hook Triggered",
                &format!("{} · {}", entry.hook.name, best_ref.label_text),
            );
        }
    }
}

fn msg_ts_utc_now() -> u64 {
    yyyymmddhhmmss_utc().max(0) as u64
}

fn unix_secs_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) use skill_exg::cosine_distance;

fn load_recent_label_texts(skill_dir: &Path, limit: usize) -> Vec<String> {
    let labels_db = skill_dir.join(crate::constants::LABELS_FILE);
    if !labels_db.exists() {
        return Vec::new();
    }
    let Ok(conn) = rusqlite::Connection::open_with_flags(
        &labels_db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) else {
        return Vec::new();
    };

    let max_rows = limit.clamp(10, 300) as i64;
    let Ok(mut stmt) = conn.prepare(
        "SELECT text FROM labels
         WHERE length(trim(text)) > 0
         GROUP BY text
         ORDER BY MAX(created_at) DESC
         LIMIT ?1",
    ) else {
        return Vec::new();
    };

    stmt.query_map(rusqlite::params![max_rows], |row| row.get::<_, String>(0))
        .map(|rows| {
            rows.flatten()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) use skill_exg::fuzzy_match;

// ── Background worker ─────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn embed_worker(
    rx:               mpsc::Receiver<EpochMsg>,
    skill_dir:        PathBuf,
    config:           EegModelConfig,
    status:           Arc<Mutex<EegModelStatus>>,
    cancel:           Arc<std::sync::atomic::AtomicBool>,
    reload_requested: Arc<std::sync::atomic::AtomicBool>,
    logger:           Arc<SkillLogger>,
    global_index:     Arc<Mutex<Option<fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64>>>>,
    hooks:            Vec<HookRule>,
    shared_embedder:  Arc<crate::label_cmds::EmbedderState>,
    label_idx:        Arc<crate::label_index::LabelIndexState>,
    ws_broadcaster:   crate::ws_server::WsBroadcaster,
    hook_runtime: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, HookLastTrigger>>>,
    app: tauri::AppHandle,
) {
    use burn::backend::{Wgpu, wgpu::WgpuDevice};
    use ndarray::Array2;
    use std::collections::HashMap;
    use zuna_rs::{ZunaEncoder, config::DataConfig, load_from_named_tensor};

    skill_log!(logger, "embedder", "worker started — skill_dir={}", skill_dir.display());
    // Mark worker as active so the UI can distinguish "loading on GPU" from
    // "weights found but no session yet".
    status.lock_or_recover().embed_worker_active = true;

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
                // A successful UI download set reload_requested — exit so the
                // accumulator respawns a fresh worker that finds the new files.
                if reload_requested.load(Ordering::Relaxed) {
                    skill_log!(logger, "embedder", "reload requested during backoff wait — exiting for respawn");
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
        for msg in rx {
            // Still honour reload requests even in metrics-only mode so the
            // accumulator can respawn a worker after a process restart clears
            // the poison flag.
            if reload_requested.load(std::sync::atomic::Ordering::Relaxed) {
                reload_requested.store(false, std::sync::atomic::Ordering::Relaxed);
                break;
            }
            store_metrics_only(&msg, &mut store, &status, &logger, &skill_dir, &config);
        }
        status.lock_or_recover().embed_worker_active = false;
        // No embeddings were produced, so nothing to flush into the global index.
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

    // Counter for periodic global index saves.
    let mut global_save_counter: usize = 0;
    let mut hook_matcher = HookMatcher::new(
        skill_dir.clone(), hooks, shared_embedder, label_idx, ws_broadcaster, logger.clone(),
        hook_runtime, app,
    );

    // ── 4. Process epoch messages ─────────────────────────────────────────────
    for msg in rx {
        // If a new download completed and the UI asked for an in-place reload,
        // exit this worker cleanly.  EegAccumulator::push() detects the
        // channel disconnect and immediately respawns a fresh worker that will
        // call resolve_hf_weights (finding the newly downloaded files) and load
        // the encoder — no full app restart needed.
        if reload_requested.load(std::sync::atomic::Ordering::Relaxed) {
            skill_log!(logger, "embedder", "reload requested — exiting for in-place encoder reload");
            // Reset status so the UI shows the loading state while the new
            // worker initialises.
            {
                let mut st = status.lock_or_recover();
                st.encoder_loaded   = false;
                st.encoder_describe = None;
                st.download_needs_restart = false;
            }
            // Clear the flag so the respawned worker doesn't immediately exit too.
            reload_requested.store(false, std::sync::atomic::Ordering::Relaxed);
            break;
        }

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
            let id = s.insert(
                msg.timestamp,
                msg.device_id.as_deref(),
                msg.device_name.as_deref(),
                emb,
                metrics.as_ref(),
                msg.ppg_averages.as_ref(),
                channels_json.as_deref(),
            );

            // ── Also insert into the persistent cross-day global HNSW ─────
            // The global index accumulates every embedding across all days so
            // that a single HNSW search can find near-neighbors from any date.
            // The payload is the YYYYMMDDHHmmss timestamp; the date is derived
            // from it during search result hydration.
            {
                let mut g = global_index.lock_or_recover();
                if let Some(ref mut gidx) = *g {
                    gidx.insert(emb.clone(), msg.timestamp);
                    global_save_counter += 1;
                    if global_save_counter >= GLOBAL_HNSW_SAVE_EVERY {
                        global_save_counter = 0;
                        global_eeg_index::save_index(gidx, &skill_dir);
                    }
                }
            }

            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                hook_matcher.maybe_fire(emb, metrics.as_ref());
            }))
            .map_err(|p| {
                skill_log!(
                    logger,
                    "hooks",
                    "hook matcher panicked; continuing embed worker: {}",
                    panic_msg(&p)
                );
            });

            id
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

    // Final flush: persist any unsaved insertions to the global index.
    {
        let g = global_index.lock_or_recover();
        if let Some(ref gidx) = *g {
            global_eeg_index::save_index(gidx, &skill_dir);
            skill_log!(logger, "embedder", "global HNSW flushed on exit ({} entries)", gidx.len());
        }
    }

    status.lock_or_recover().embed_worker_active = false;
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

// yyyymmdd_utc, yyyymmddhhmmss_utc — delegated to skill_exg.
use skill_exg::yyyymmdd_utc;
pub use skill_exg::yyyymmddhhmmss_utc;

pub use skill_exg::probe_hf_weights;

fn resolve_hf_weights(hf_repo: &str) -> Option<(PathBuf, PathBuf)> {
    skill_exg::resolve_hf_weights(hf_repo)
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
    _logger:            &Arc<SkillLogger>,
) -> Option<(PathBuf, PathBuf)> {
    // Delegate to skill_exg (logger replaced with eprintln! in the crate).
    skill_exg::download_hf_weights(hf_repo, status, cancel, mark_needs_restart)
}

// Original download_hf_weights body moved to skill_exg crate.
#[cfg(any())]
fn _original_download_hf_weights() {
    use hf_hub::api::sync::Api;
    use std::io::{Read, Write};
    use std::sync::atomic::Ordering;

    const ENDPOINT: &str = "https://huggingface.co";

    skill_log!(logger, "embedder", "ZUNA weights not in cache — downloading from HuggingFace: {hf_repo}");

    // ── Mark download in progress ────────────────────────────────────────────
    {
        let mut st = status.lock_or_recover();
        st.downloading_weights    = true;
        st.download_needs_restart = false;
        st.download_progress      = 0.0;
        st.download_status_msg    = Some(format!("Connecting to HuggingFace ({hf_repo})…"));
    }

    // ── Build the HF Hub API client (used only for config.json) ─────────────
    let api = match Api::new() {
        Ok(a)  => a,
        Err(e) => {
            skill_log!(logger, "embedder", "hf-hub Api::new() failed: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress   = 0.0;
            st.download_status_msg = Some(format!("HF Hub init failed: {e}"));
            return None;
        }
    };
    let repo = api.model(hf_repo.to_string());

    // ── Download config.json (small — use hf_hub as normal) ─────────────────
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
            st.download_progress   = 0.0;
            st.download_status_msg = Some(format!("Download failed ({ZUNA_CONFIG_FILE}): {e}"));
            return None;
        }
    };

    // ── Honour cancellation between the two files ────────────────────────────
    if cancel.load(Ordering::Relaxed) {
        skill_log!(logger, "embedder", "download cancelled by user after config.json");
        let mut st = status.lock_or_recover();
        st.downloading_weights = false;
        st.download_progress   = 0.0;
        st.download_status_msg = Some("Download cancelled.".to_string());
        return None;
    }

    // ── Resolve the HF Hub cache root (same logic as resolve_hf_weights) ─────
    let cache_root = hf_hub::Cache::from_env().path().to_path_buf();
    let folder     = format!("models--{}", hf_repo.replace('/', "--"));
    let model_dir  = cache_root.join(&folder);
    let blobs_dir  = model_dir.join("blobs");
    let refs_dir   = model_dir.join("refs");

    if let Err(e) = std::fs::create_dir_all(&blobs_dir)
        .and(std::fs::create_dir_all(&refs_dir))
    {
        let mut st = status.lock_or_recover();
        st.downloading_weights = false;
        st.download_progress   = 0.0;
        st.download_status_msg = Some(format!("Failed to create cache dirs: {e}"));
        return None;
    }

    // ── Fetch HF metadata: commit SHA + blob SHA256 + file size ─────────────
    {
        let mut st = status.lock_or_recover();
        st.download_status_msg = Some(format!("Fetching metadata for {ZUNA_WEIGHTS_FILE}…"));
    }

    let hf_token = std::env::var("HF_TOKEN").ok()
        .or_else(|| std::env::var("HUGGING_FACE_HUB_TOKEN").ok());

    let meta_agent = ureq::AgentBuilder::new()
        .redirects(10)
        .timeout(std::time::Duration::from_secs(30))
        .build();
    let dl_agent = ureq::AgentBuilder::new()
        .redirects(10)
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout_read(std::time::Duration::from_secs(300))
        .build();

    let auth = |req: ureq::Request| -> ureq::Request {
        match &hf_token {
            Some(tok) => req.set("Authorization", &format!("Bearer {tok}")),
            None      => req,
        }
    };

    let api_url  = format!("{ENDPOINT}/api/models/{hf_repo}?blobs=1");
    let api_resp = match auth(meta_agent.get(&api_url))
        .set("User-Agent", "skill-app/1.0")
        .call()
    {
        Ok(r)  => r,
        Err(e) => {
            skill_log!(logger, "embedder", "HF metadata API error: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress   = 0.0;
            st.download_status_msg = Some(format!("Metadata fetch failed: {e}"));
            return None;
        }
    };

    let info: serde_json::Value = match api_resp.into_json() {
        Ok(v)  => v,
        Err(e) => {
            skill_log!(logger, "embedder", "HF metadata JSON parse error: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress   = 0.0;
            st.download_status_msg = Some(format!("Metadata parse failed: {e}"));
            return None;
        }
    };

    let commit_sha = info["sha"].as_str().unwrap_or("main").to_string();

    let file_meta = info["siblings"]
        .as_array()
        .and_then(|s| s.iter().find(|e| e["rfilename"].as_str() == Some(ZUNA_WEIGHTS_FILE)));

    let (blob_sha, remote_size) = match file_meta {
        Some(m) => {
            let sha = m["lfs"]["sha256"]
                .as_str()
                .map(|s| s.trim_start_matches("sha256:").to_string());
            let size = m["lfs"]["size"].as_u64().or_else(|| m["size"].as_u64());
            match (sha, size) {
                (Some(s), Some(n)) => (s, n),
                _ => {
                    // Non-LFS file or missing metadata — fall back to hf_hub
                    skill_log!(logger, "embedder",
                        "LFS metadata missing for {ZUNA_WEIGHTS_FILE}, falling back to hf_hub");
                    {
                        let mut st = status.lock_or_recover();
                        st.download_status_msg =
                            Some(format!("Downloading {ZUNA_WEIGHTS_FILE}…"));
                    }
                    let weights_path = match repo.get(ZUNA_WEIGHTS_FILE) {
                        Ok(p)  => p,
                        Err(e) => {
                            let mut st = status.lock_or_recover();
                            st.downloading_weights = false;
                            st.download_progress   = 0.0;
                            st.download_status_msg =
                                Some(format!("Download failed ({ZUNA_WEIGHTS_FILE}): {e}"));
                            return None;
                        }
                    };
                    let mut st = status.lock_or_recover();
                    st.downloading_weights    = false;
                    st.download_progress      = 1.0;
                    st.download_status_msg    = None;
                    st.weights_found          = true;
                    st.weights_path           = Some(weights_path.display().to_string());
                    st.download_needs_restart = mark_needs_restart;
                    return Some((weights_path, config_path));
                }
            }
        }
        None => {
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress   = 0.0;
            st.download_status_msg = Some(
                format!("{ZUNA_WEIGHTS_FILE}: not listed in {hf_repo} manifest")
            );
            return None;
        }
    };

    // ── Determine resume offset ───────────────────────────────────────────────
    let blob_path       = blobs_dir.join(&blob_sha);
    let incomplete_path = blobs_dir.join(format!("{blob_sha}.incomplete"));

    // Already fully downloaded by a previous run — just re-register and return.
    if blob_path.exists() && blob_path.metadata().map(|m| m.len()).unwrap_or(0) >= remote_size {
        skill_log!(logger, "embedder", "✓ {ZUNA_WEIGHTS_FILE} already in blob cache");
        let weights_path = match register_hf_snapshot(
            &model_dir, &refs_dir, &commit_sha, ZUNA_WEIGHTS_FILE, &blob_path,
        ) {
            Ok(p)  => p,
            Err(e) => {
                let mut st = status.lock_or_recover();
                st.downloading_weights = false;
                st.download_progress   = 0.0;
                st.download_status_msg = Some(format!("Snapshot registration failed: {e}"));
                return None;
            }
        };
        let mut st = status.lock_or_recover();
        st.downloading_weights    = false;
        st.download_progress      = 1.0;
        st.download_status_msg    = None;
        st.weights_found          = true;
        st.weights_path           = Some(weights_path.display().to_string());
        st.download_needs_restart = mark_needs_restart;
        return Some((weights_path, config_path));
    }

    let resume_from: u64 = incomplete_path.metadata().map(|m| m.len()).unwrap_or(0);

    {
        let mut st = status.lock_or_recover();
        st.download_progress = (resume_from as f32 / remote_size.max(1) as f32).min(0.99);
        st.download_status_msg = Some(if resume_from > 0 {
            format!(
                "Resuming {ZUNA_WEIGHTS_FILE} from {:.0} / {:.0} MB…",
                resume_from as f64 / 1_048_576.0,
                remote_size as f64 / 1_048_576.0,
            )
        } else {
            format!(
                "Downloading {ZUNA_WEIGHTS_FILE} ({:.0} MB)…",
                remote_size as f64 / 1_048_576.0,
            )
        });
    }

    // ── Issue GET (with Range header when resuming) ──────────────────────────
    let file_url = format!("{ENDPOINT}/{hf_repo}/resolve/main/{ZUNA_WEIGHTS_FILE}");
    let mut get  = auth(dl_agent.get(&file_url)).set("User-Agent", "skill-app/1.0");
    if resume_from > 0 {
        get = get.set("Range", &format!("bytes={resume_from}-"));
    }

    let resp = match get.call() {
        Ok(r)  => r,
        Err(e) => {
            skill_log!(logger, "embedder", "HTTP error downloading {ZUNA_WEIGHTS_FILE}: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress   = 0.0;
            st.download_status_msg = Some(format!("Download failed: {e}"));
            return None;
        }
    };

    let http_status  = resp.status();
    let writing_from = if http_status == 206 { resume_from } else { 0 };

    // ── Open (or create) the .incomplete file ────────────────────────────────
    let mut file = match std::fs::OpenOptions::new()
        .create(true).write(true)
        .append(writing_from > 0)
        .truncate(writing_from == 0)
        .open(&incomplete_path)
    {
        Ok(f)  => f,
        Err(e) => {
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress   = 0.0;
            st.download_status_msg = Some(format!("Cannot open temp file: {e}"));
            return None;
        }
    };

    // ── Stream response → disk, updating progress each 128 KB chunk ─────────
    let mut reader  = resp.into_reader();
    let mut buf     = vec![0u8; 128 * 1024];
    let mut written = writing_from;
    let total       = remote_size.max(1);

    loop {
        let n = match reader.read(&mut buf) {
            Ok(n)  => n,
            Err(e) => {
                skill_log!(logger, "embedder", "read error: {e}");
                let mut st = status.lock_or_recover();
                st.downloading_weights = false;
                st.download_progress   = 0.0;
                st.download_status_msg = Some(format!("Read error: {e}"));
                return None;
            }
        };
        if n == 0 { break; }

        if let Err(e) = file.write_all(&buf[..n]) {
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress   = 0.0;
            st.download_status_msg = Some(format!("Write error: {e}"));
            return None;
        }
        written += n as u64;

        {
            let mut st = status.lock_or_recover();
            st.download_progress   = (written as f32 / total as f32).min(0.99);
            st.download_status_msg = Some(format!(
                "{:.0} / {:.0} MB",
                written  as f64 / 1_048_576.0,
                total    as f64 / 1_048_576.0,
            ));
        }

        if cancel.load(Ordering::Relaxed) {
            skill_log!(logger, "embedder", "download cancelled by user");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress   = 0.0;
            st.download_status_msg = Some("Download cancelled.".to_string());
            return None;
        }
    }
    drop(file);

    // ── Sanity-check size ────────────────────────────────────────────────────
    let final_size = incomplete_path.metadata().map(|m| m.len()).unwrap_or(0);
    if final_size < remote_size {
        skill_log!(logger, "embedder",
            "incomplete download: {final_size} < {remote_size} bytes");
        let mut st = status.lock_or_recover();
        st.downloading_weights = false;
        st.download_progress   = final_size as f32 / remote_size as f32;
        st.download_status_msg = Some(format!(
            "Incomplete download ({final_size} / {remote_size} bytes) — retry to resume."
        ));
        return None;
    }

    // ── Promote .incomplete → blob ───────────────────────────────────────────
    if let Err(e) = std::fs::rename(&incomplete_path, &blob_path) {
        let mut st = status.lock_or_recover();
        st.downloading_weights = false;
        st.download_progress   = 0.0;
        st.download_status_msg = Some(format!("Failed to finalise download: {e}"));
        return None;
    }

    // ── Register in HF snapshot structure ────────────────────────────────────
    let weights_path = match register_hf_snapshot(
        &model_dir, &refs_dir, &commit_sha, ZUNA_WEIGHTS_FILE, &blob_path,
    ) {
        Ok(p)  => p,
        Err(e) => {
            skill_log!(logger, "embedder", "snapshot registration failed: {e}");
            let mut st = status.lock_or_recover();
            st.downloading_weights = false;
            st.download_progress   = 0.0;
            st.download_status_msg = Some(format!("Snapshot registration failed: {e}"));
            return None;
        }
    };

    // ── Download complete ────────────────────────────────────────────────────
    {
        let mut st = status.lock_or_recover();
        st.downloading_weights    = false;
        st.download_progress      = 1.0;
        st.download_status_msg    = None;
        st.weights_found          = true;
        st.weights_path           = Some(weights_path.display().to_string());
        st.download_needs_restart = mark_needs_restart;
    }
    skill_log!(logger, "embedder", "ZUNA weights downloaded successfully → {}", weights_path.display());
    Some((weights_path, config_path))
}

// register_hf_snapshot — moved to skill_exg crate.
// Kept here only inside the dead #[cfg(any())] block above.
#[allow(dead_code)]
fn register_hf_snapshot(
    model_dir:  &Path,
    refs_dir:   &Path,
    commit_sha: &str,
    filename:   &str,
    blob_path:  &Path,
) -> Result<PathBuf, String> {
    std::fs::write(refs_dir.join("main"), commit_sha)
        .map_err(|e| format!("write refs/main: {e}"))?;

    let snapshot_dir  = model_dir.join("snapshots").join(commit_sha);
    std::fs::create_dir_all(&snapshot_dir)
        .map_err(|e| format!("create snapshot dir: {e}"))?;

    let snapshot_link = snapshot_dir.join(filename);
    if snapshot_link.exists() || snapshot_link.symlink_metadata().is_ok() {
        std::fs::remove_file(&snapshot_link).ok();
    }

    #[cfg(unix)]
    {
        let blob_name = blob_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let depth     = std::path::Path::new(filename).components().count();
        let parents   = "../".repeat(depth + 1);
        let rel_target = format!("{parents}blobs/{blob_name}");
        std::os::unix::fs::symlink(&rel_target, &snapshot_link)
            .map_err(|e| format!("create symlink: {e}"))?;
    }

    #[cfg(windows)]
    std::fs::hard_link(blob_path, &snapshot_link)
        .map_err(|e| format!("create hardlink: {e}"))?;

    #[cfg(not(any(unix, windows)))]
    std::fs::copy(blob_path, &snapshot_link)
        .map_err(|e| format!("copy blob to snapshot: {e}"))?;

    Ok(snapshot_link)
}
