// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! EEG model configuration and runtime status.
//!
//! [`EegModelConfig`] holds every knob that affects how embeddings are
//! produced and indexed.  It is persisted as JSON at
//! `~/.skill/model_config.json` and loaded at app startup.
//!
//! [`EegModelStatus`] is a live snapshot populated by the background embed
//! worker and exposed through the [`get_eeg_model_status`] Tauri command.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::constants::{
    HNSW_EF_CONSTRUCTION, HNSW_M, MODEL_CONFIG_FILE,
    ZUNA_DATA_NORM, ZUNA_HF_REPO,
};

// ── Persisted configuration ───────────────────────────────────────────────────

/// All user-tunable parameters for the ZUNA embedding pipeline.
///
/// Saved to `~/.skill/model_config.json`.
/// Changes to HNSW parameters take effect when the next daily index is created
/// (i.e., at midnight UTC or on the next app launch).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EegModelConfig {
    /// HuggingFace repository that contains the ZUNA weights.
    ///
    /// Default: `"Zyphra/ZUNA"`.  Weights must be downloaded manually:
    /// ```bash
    /// python3 -c "from huggingface_hub import snapshot_download; \
    ///             snapshot_download('Zyphra/ZUNA')"
    /// ```
    #[serde(default = "default_hf_repo")]
    pub hf_repo: String,

    /// HNSW graph connectivity (`M`).
    ///
    /// Each node keeps up to `2 × M` bidirectional edges.
    /// Higher values → better recall, more RAM, slower inserts.
    /// Typical range: 8 – 64.  Default: 16.
    #[serde(default = "default_hnsw_m")]
    pub hnsw_m: usize,

    /// HNSW beam width during index construction (`ef_construction`).
    ///
    /// Larger values produce a higher-quality graph at the cost of insert
    /// time.  Has no effect on query speed.  Typical range: 100 – 400.
    /// Default: 200.
    #[serde(default = "default_hnsw_ef")]
    pub hnsw_ef_construction: usize,

    /// Divisor applied to z-scored EEG before entering the ZUNA encoder.
    ///
    /// Must match the training-time normalisation — **do not change** unless
    /// you are using a custom ZUNA checkpoint.  Default: 10.0.
    #[serde(default = "default_data_norm")]
    pub data_norm: f32,
}

fn default_hf_repo()  -> String { ZUNA_HF_REPO.to_string() }
fn default_hnsw_m()   -> usize  { HNSW_M }
fn default_hnsw_ef()  -> usize  { HNSW_EF_CONSTRUCTION }
fn default_data_norm() -> f32   { ZUNA_DATA_NORM }

impl Default for EegModelConfig {
    fn default() -> Self {
        Self {
            hf_repo:             default_hf_repo(),
            hnsw_m:              default_hnsw_m(),
            hnsw_ef_construction: default_hnsw_ef(),
            data_norm:           default_data_norm(),
        }
    }
}

// ── Runtime status (not persisted) ───────────────────────────────────────────

/// Live snapshot of the embed worker's state.
///
/// Held in an `Arc<Mutex<EegModelStatus>>` shared between the worker thread
/// and the Tauri command handler.  The worker writes; the UI polls via
/// [`get_eeg_model_status`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EegModelStatus {
    /// `true` once the ZUNA encoder has been loaded on the wgpu device.
    pub encoder_loaded: bool,

    /// `true` while the embed worker thread is alive (weights resolved,
    /// actively loading or running inference).  `false` before any session
    /// starts and after the worker exits.  Used by the UI to distinguish
    /// "weights found, worker loading encoder" from "weights found but no
    /// active session yet — connect headset to begin".
    pub embed_worker_active: bool,

    /// Human-readable encoder summary, e.g.
    /// `"ZUNA  dim=1024  layers=16  head_dim=64  out_dim=32"`.
    pub encoder_describe: Option<String>,

    /// `true` if the weight files were found in the HF disk cache.
    pub weights_found: bool,

    /// Absolute path to the `.safetensors` weights file, if found.
    pub weights_path: Option<String>,

    /// `true` while the background worker is downloading ZUNA weights from
    /// HuggingFace Hub.  Cleared to `false` once the download finishes
    /// (whether successfully or not).
    pub downloading_weights: bool,

    /// Download progress in [0.0, 1.0] for the current file being fetched.
    /// 0.0 while connecting / fetching metadata; approaches 1.0 as bytes
    /// arrive; reset to 0.0 when not downloading.
    pub download_progress: f32,

    /// Human-readable description of the current download step, e.g.
    /// `"Downloading model-00001-of-00001.safetensors…"`.
    /// Set to `None` after a successful download; contains an error message
    /// if the download failed.
    pub download_status_msg: Option<String>,

    /// Set to `true` after a user-triggered (`trigger_weights_download`)
    /// download completes successfully, indicating the encoder has not yet
    /// been loaded from the newly downloaded files and an app restart is
    /// required.  Always `false` for the automatic startup download (the
    /// startup path loads the encoder immediately after downloading).
    pub download_needs_restart: bool,

    /// Which automatic retry attempt the embed worker is on (0-based).
    /// Incremented after each failed download before the backoff wait begins.
    /// Reset to 0 on success.
    pub download_retry_attempt: u32,

    /// Seconds remaining until the next automatic download retry.
    /// Non-zero only while the embed worker is in the backoff wait between
    /// attempts.  Counts down to 0 each second.
    pub download_retry_in_secs: u64,

    /// Number of embeddings inserted into today's HNSW index.
    pub embeddings_today: usize,

    /// Absolute path to today's SQLite database
    /// (e.g. `~/.skill/20260223/eeg.sqlite`).
    pub daily_db_path: String,

    /// Absolute path to today's HNSW index file
    /// (e.g. `~/.skill/20260223/eeg_embeddings.hnsw`).
    pub daily_hnsw_path: String,

    /// Latest per-epoch band metrics (averaged over 5s epoch, updated ~every hop).
    /// Stored as a flat struct for easy serialisation.
    pub latest_metrics: Option<LatestEpochMetrics>,
}

/// Band-derived metrics from the most recent 5-second embedding epoch.
/// Exposed in the WebSocket `status` response and available to the frontend.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LatestEpochMetrics {
    pub rel_delta:        f32,
    pub rel_theta:        f32,
    pub rel_alpha:        f32,
    pub rel_beta:         f32,
    pub rel_gamma:        f32,
    pub rel_high_gamma:   f32,
    pub relaxation_score: f32,
    pub engagement_score: f32,
    /// Frontal Alpha Asymmetry: ln(AF8 α) − ln(AF7 α).
    pub faa:              f32,
    /// Theta / Alpha ratio (drowsiness indicator).
    pub tar:              f32,
    /// Beta / Alpha ratio (attention/stress marker).
    pub bar:              f32,
    /// Delta / Theta ratio (deep-relaxation indicator).
    pub dtr:              f32,
    /// Power Spectral Entropy [0–1] (spectral complexity).
    pub pse:              f32,
    /// Alpha Peak Frequency in Hz.
    pub apf:              f32,
    /// Band-Power Slope (1/f exponent, log–log regression).
    pub bps:              f32,
    /// Signal-to-Noise Ratio in dB.
    pub snr:              f32,
    /// Mean inter-channel alpha coherence [−1, 1].
    pub coherence:        f32,
    /// Mu suppression index (current alpha / baseline alpha).
    pub mu_suppression:   f32,
    pub tbr:              f32,
    pub sef95:            f32,
    pub spectral_centroid: f32,
    pub hjorth_activity:  f32,
    pub hjorth_mobility:  f32,
    pub hjorth_complexity: f32,
    pub permutation_entropy: f32,
    pub higuchi_fd:       f32,
    pub dfa_exponent:     f32,
    pub sample_entropy:   f32,
    pub pac_theta_gamma:  f32,
    pub laterality_index: f32,
    // PPG-derived
    pub hr:               f64,
    pub rmssd:            f64,
    pub sdnn:             f64,
    pub pnn50:            f64,
    pub lf_hf_ratio:      f64,
    pub respiratory_rate: f64,
    pub spo2_estimate:    f64,
    pub perfusion_index:  f64,
    pub stress_index:     f64,
    /// Mood index (composite, 0–100).
    pub mood:             f32,
    // ── Artifact / event metrics ─────────────────────────────────────
    pub blink_count:      u64,
    pub blink_rate:       f64,

    // ── Head pose ────────────────────────────────────────────────────
    pub head_pitch:       f64,
    pub head_roll:        f64,
    pub stillness:        f64,
    pub nod_count:        u64,
    pub shake_count:      u64,
    // ── Composite scores ─────────────────────────────────────────────
    pub meditation:       f64,
    pub cognitive_load:   f64,
    pub drowsiness:       f64,
    // ── Headache / Migraine EEG correlate indices (0–100) ───────────────────
    pub headache_index:         f32,
    pub migraine_index:         f32,
    // ── Consciousness metrics (0–100) ─────────────────────────────────
    pub consciousness_lzc:          f32,
    pub consciousness_wakefulness:  f32,
    pub consciousness_integration:  f32,
    /// `YYYYMMDDHHmmss` UTC timestamp of the epoch.
    pub epoch_timestamp:  i64,
}

// ── Persistence helpers ───────────────────────────────────────────────────────

pub fn load_model_config(skill_dir: &Path) -> EegModelConfig {
    let path = skill_dir.join(MODEL_CONFIG_FILE);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_model_config(skill_dir: &Path, cfg: &EegModelConfig) {
    let _ = std::fs::create_dir_all(skill_dir);
    let path = skill_dir.join(MODEL_CONFIG_FILE);
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, json);
    }
}
