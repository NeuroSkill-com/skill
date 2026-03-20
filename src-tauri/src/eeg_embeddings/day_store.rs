// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Per-day HNSW index + SQLite database for EEG embeddings.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::skill_log::SkillLogger;
use crate::constants::{HNSW_INDEX_FILE, SQLITE_FILE};
use skill_exg::EpochMetrics;

// ── Metrics JSON serialization ─────────────────────────────────────────────────

/// Serialise all computed metrics into a single JSON object string.
///
/// Shared by both `DayStore::insert` and `DayStore::insert_metrics_only`
/// to avoid duplicating the ~60-field serialization logic.
fn metrics_to_json(
    m: &EpochMetrics,
    ppg_averages: Option<&[f64; 3]>,
    band_channels_json: Option<&str>,
) -> String {
    use serde_json::{Map, Value};

    let nn = |v: f64| -> Value {
        if v > 0.0 { Value::Number(serde_json::Number::from_f64(v).unwrap_or(serde_json::Number::from(0))) }
        else { Value::Null }
    };
    let f32v = |v: f32| -> Value {
        Value::Number(serde_json::Number::from_f64(v as f64).unwrap_or(serde_json::Number::from(0)))
    };
    let f64v = |v: f64| -> Value {
        Value::Number(serde_json::Number::from_f64(v).unwrap_or(serde_json::Number::from(0)))
    };
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
    // Per-channel band powers
    let band_channels_val: Value = band_channels_json
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);
    o.insert("band_channels".into(), band_channels_val);

    Value::Object(o).to_string()
}

// ── Per-day storage (HNSW index + SQLite database) ────────────────────────────

pub(in crate::eeg_embeddings) struct DayStore {
    pub(in crate::eeg_embeddings) index:      fast_hnsw::labeled::LabeledIndex<fast_hnsw::distance::Cosine, i64>,
    pub(in crate::eeg_embeddings) index_path: PathBuf,
    pub(in crate::eeg_embeddings) db_path:    PathBuf,
    pub(in crate::eeg_embeddings) conn:       rusqlite::Connection,
    pub(in crate::eeg_embeddings) logger:     Arc<SkillLogger>,
}

impl DayStore {
    /// Open (or create) the HNSW index and SQLite DB for `date` inside
    /// `skill_dir`, using the supplied HNSW graph parameters.
    pub(super) fn open(skill_dir: &Path, date: &str, hnsw_m: usize, hnsw_ef: usize, logger: Arc<SkillLogger>) -> Option<Self> {
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
        skill_data::util::init_wal_pragmas(&conn);

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
                metrics_json    TEXT,
                -- Which model backend produced this embedding (zuna | luna | NULL for legacy).
                model_backend   TEXT,
                -- Embedding inference time in milliseconds.
                embed_speed_ms  REAL
            );
            CREATE INDEX IF NOT EXISTS idx_timestamp ON embeddings (timestamp);
        ";
        // Migration: add columns to databases created before this schema.
        // Old individual-column rows will have NULL metrics_json and are read
        // through the json_extract() fallback (which returns NULL → 0.0).
        let migrate = [
            "ALTER TABLE embeddings ADD COLUMN metrics_json TEXT",
            "ALTER TABLE embeddings ADD COLUMN model_backend TEXT",
            "ALTER TABLE embeddings ADD COLUMN embed_speed_ms REAL",
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
    pub(super) fn insert(
        &mut self,
        timestamp:          i64,
        device_id:          Option<&str>,
        device_name:        Option<&str>,
        embedding:          &[f32],
        metrics:            Option<&EpochMetrics>,
        ppg_averages:       Option<&[f64; 3]>,
        band_channels_json: Option<&str>,
        model_backend:      Option<&str>,
        embed_speed_ms:     Option<f64>,
    ) -> usize {
        // ── HNSW ─────────────────────────────────────────────────────────────
        let hnsw_id = self.index.insert(embedding.to_vec(), timestamp);

        if let Err(e) = self.index.save(&self.index_path) {
            skill_log!(self.logger, "embedder", "HNSW save error: {e}");
        }

        // ── SQLite ────────────────────────────────────────────────────────────
        let blob: Vec<u8> = embedding.iter().flat_map(|v| v.to_le_bytes()).collect();
        let metrics_json: Option<String> = metrics.map(|m| metrics_to_json(m, ppg_averages, band_channels_json));

        let r = self.conn.execute(
            "INSERT INTO embeddings
             (timestamp, device_id, device_name, hnsw_id, eeg_embedding, label, extra_embedding,
              metrics_json, model_backend, embed_speed_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, ?6, ?7, ?8)",
            rusqlite::params![
                timestamp, device_id, device_name, hnsw_id as i64, blob,
                metrics_json, model_backend, embed_speed_ms,
            ],
        );
        if let Err(e) = r { skill_log!(self.logger, "embedder", "sqlite insert: {e}"); }

        hnsw_id
    }

    pub(super) fn hnsw_len(&self) -> usize { self.index.len() }

    /// Persist metrics to SQLite **without** a wgpu embedding or HNSW entry.
    ///
    /// Used when the GPU pipeline is unavailable (encoder not loaded, or the
    /// wgpu device's internal mutexes were poisoned by a cubecl panic).  Band
    /// and sleep metrics are still valuable without embeddings.
    pub(super) fn insert_metrics_only(
        &mut self,
        timestamp:          i64,
        device_id:          Option<&str>,
        device_name:        Option<&str>,
        metrics:            Option<&EpochMetrics>,
        ppg_averages:       Option<&[f64; 3]>,
        band_channels_json: Option<&str>,
    ) -> usize {
        let metrics_json: Option<String> = metrics.map(|m| metrics_to_json(m, ppg_averages, band_channels_json));

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

