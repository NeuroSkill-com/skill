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
//! worker and exposed through the `get_eeg_model_status` Tauri command.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::constants::{
    HNSW_EF_CONSTRUCTION, HNSW_M, LUNA_DEFAULT_VARIANT, LUNA_HF_REPO, MODEL_CONFIG_FILE, ZUNA_DATA_NORM, ZUNA_HF_REPO,
};

// ── EXG embedding model backend ──────────────────────────────────────────────

/// Which EEG/EXG foundation model to use for embedding generation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExgModelBackend {
    /// ZUNA encoder (Zyphra) — default, recommended.
    #[default]
    Zuna,
    /// LUNA encoder (PulpBio) — topology-agnostic, multiple sizes.
    Luna,
    /// REVE encoder (Brain-BZH) — 4D Fourier positional encoding.
    Reve,
    /// CBraMod encoder (braindecode) — criss-cross attention.
    Cbramod,
    /// EEGPT encoder (braindecode) — dual self-supervised transformer.
    Eegpt,
    /// LaBraM encoder (braindecode) — BEiTv2-style neural tokenization.
    Labram,
    /// SignalJEPA encoder (braindecode) — JEPA with dynamic spatial attention.
    Signaljepa,
    /// OSF encoder (yang-ai-lab) — multimodal PSG ViT-Base.
    Osf,
    /// SleepFM encoder (zou-group) — contrastive multimodal PSG.
    Sleepfm,
    /// SleepLM encoder (yang-ai-lab) — CoCa PSG + language.
    Sleeplm,
    /// SensorLM encoder (Google) — wearable sensor + language.
    Sensorlm,
    /// OpenTSLM encoder (Stanford BDHG) — time-series + LLM.
    Opentslm,
    /// TRIBE v2 encoder (Meta AI) — multimodal fMRI brain encoding (video + audio + text → cortical surface).
    Tribev2,
}

impl std::fmt::Display for ExgModelBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl ExgModelBackend {
    /// Parse from a string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "luna" => Self::Luna,
            "reve" => Self::Reve,
            "cbramod" => Self::Cbramod,
            "eegpt" => Self::Eegpt,
            "labram" => Self::Labram,
            "signaljepa" => Self::Signaljepa,
            "osf" => Self::Osf,
            "sleepfm" => Self::Sleepfm,
            "sleeplm" => Self::Sleeplm,
            "sensorlm" => Self::Sensorlm,
            "opentslm" => Self::Opentslm,
            "tribev2" => Self::Tribev2,
            _ => Self::Zuna,
        }
    }

    /// Canonical short name used in SQLite `model_backend` column.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Zuna => "zuna",
            Self::Luna => "luna",
            Self::Reve => "reve",
            Self::Cbramod => "cbramod",
            Self::Eegpt => "eegpt",
            Self::Labram => "labram",
            Self::Signaljepa => "signaljepa",
            Self::Osf => "osf",
            Self::Sleepfm => "sleepfm",
            Self::Sleeplm => "sleeplm",
            Self::Sensorlm => "sensorlm",
            Self::Opentslm => "opentslm",
            Self::Tribev2 => "tribev2",
        }
    }
}

// ── Persisted configuration ───────────────────────────────────────────────────

/// All user-tunable parameters for the EXG embedding pipeline.
///
/// Saved to `~/.skill/model_config.json`.
/// Changes to HNSW parameters take effect when the next daily index is created
/// (i.e., at midnight UTC or on the next app launch).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExgModelConfig {
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

    /// Which EXG model backend to use for embeddings.
    ///
    /// Default: `Zuna`.  Set to `Luna` to use the LUNA foundation model.
    #[serde(default)]
    pub model_backend: ExgModelBackend,

    /// LUNA model size variant: `"base"`, `"large"`, or `"huge"`.
    ///
    /// Ignored when `model_backend` is `Zuna`.
    #[serde(default = "default_luna_variant")]
    pub luna_variant: String,

    /// HuggingFace repository for LUNA weights.
    #[serde(default = "default_luna_hf_repo")]
    pub luna_hf_repo: String,
}

fn default_hf_repo() -> String {
    ZUNA_HF_REPO.to_string()
}
fn default_hnsw_m() -> usize {
    HNSW_M
}
fn default_hnsw_ef() -> usize {
    HNSW_EF_CONSTRUCTION
}
fn default_data_norm() -> f32 {
    ZUNA_DATA_NORM
}
fn default_luna_variant() -> String {
    LUNA_DEFAULT_VARIANT.to_string()
}
fn default_luna_hf_repo() -> String {
    LUNA_HF_REPO.to_string()
}

/// Legacy alias — old configs may have the old struct name.
pub type EegModelConfig = ExgModelConfig;

impl Default for ExgModelConfig {
    fn default() -> Self {
        Self {
            hf_repo: default_hf_repo(),
            hnsw_m: default_hnsw_m(),
            hnsw_ef_construction: default_hnsw_ef(),
            data_norm: default_data_norm(),
            model_backend: ExgModelBackend::default(),
            luna_variant: default_luna_variant(),
            luna_hf_repo: default_luna_hf_repo(),
        }
    }
}

impl ExgModelConfig {
    /// Return the LUNA safetensors filename for the current variant.
    ///
    /// Falls back to `LUNA_base.safetensors` if the variant is unrecognised.
    pub fn luna_weights_file(&self) -> &'static str {
        crate::constants::LUNA_VARIANTS
            .iter()
            .find(|(v, _)| *v == self.luna_variant.as_str())
            .map(|(_, f)| *f)
            .unwrap_or(crate::constants::LUNA_VARIANTS[0].1)
    }
}

// ── Runtime status (not persisted) ───────────────────────────────────────────

/// Live snapshot of the embed worker's state.
///
/// Held in an `Arc<Mutex<EegModelStatus>>` shared between the worker thread
/// and the Tauri command handler.  The worker writes; the UI polls via
/// `get_eeg_model_status`.
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

    /// Which model backend is currently active.
    pub active_model_backend: Option<String>,

    /// Embedding speed: milliseconds for the most recent embedding.
    pub last_embed_ms: f64,

    /// Embedding speed: exponential moving average (ms).
    pub avg_embed_ms: f64,

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
    pub rel_delta: f32,
    pub rel_theta: f32,
    pub rel_alpha: f32,
    pub rel_beta: f32,
    pub rel_gamma: f32,
    pub rel_high_gamma: f32,
    pub relaxation_score: f32,
    pub engagement_score: f32,
    /// Frontal Alpha Asymmetry: ln(AF8 α) − ln(AF7 α).
    pub faa: f32,
    /// Theta / Alpha ratio (drowsiness indicator).
    pub tar: f32,
    /// Beta / Alpha ratio (attention/stress marker).
    pub bar: f32,
    /// Delta / Theta ratio (deep-relaxation indicator).
    pub dtr: f32,
    /// Power Spectral Entropy [0–1] (spectral complexity).
    pub pse: f32,
    /// Alpha Peak Frequency in Hz.
    pub apf: f32,
    /// Band-Power Slope (1/f exponent, log–log regression).
    pub bps: f32,
    /// Signal-to-Noise Ratio in dB.
    pub snr: f32,
    /// Mean inter-channel alpha coherence [−1, 1].
    pub coherence: f32,
    /// Mu suppression index (current alpha / baseline alpha).
    pub mu_suppression: f32,
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
    // PPG-derived
    pub hr: f64,
    pub rmssd: f64,
    pub sdnn: f64,
    pub pnn50: f64,
    pub lf_hf_ratio: f64,
    pub respiratory_rate: f64,
    pub spo2_estimate: f64,
    pub perfusion_index: f64,
    pub stress_index: f64,
    /// Mood index (composite, 0–100).
    pub mood: f32,
    // ── Artifact / event metrics ─────────────────────────────────────
    pub blink_count: u64,
    pub blink_rate: f64,

    // ── Head pose ────────────────────────────────────────────────────
    pub head_pitch: f64,
    pub head_roll: f64,
    pub stillness: f64,
    pub nod_count: u64,
    pub shake_count: u64,
    // ── Composite scores ─────────────────────────────────────────────
    pub meditation: f64,
    pub cognitive_load: f64,
    pub drowsiness: f64,
    // ── Headache / Migraine EEG correlate indices (0–100) ───────────────────
    pub headache_index: f32,
    pub migraine_index: f32,
    // ── Consciousness metrics (0–100) ─────────────────────────────────
    pub consciousness_lzc: f32,
    pub consciousness_wakefulness: f32,
    pub consciousness_integration: f32,
    /// `YYYYMMDDHHmmss` UTC timestamp of the epoch.
    pub epoch_timestamp: i64,
}

// ── Persistence helpers ───────────────────────────────────────────────────────

pub fn load_model_config(skill_dir: &Path) -> ExgModelConfig {
    let path = skill_dir.join(MODEL_CONFIG_FILE);
    let mut cfg: ExgModelConfig = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    // Migration: thorir/LUNA → PulpBio/LUNA (repo moved).
    if cfg.luna_hf_repo == "thorir/LUNA" {
        cfg.luna_hf_repo = LUNA_HF_REPO.to_string();
        save_model_config(skill_dir, &cfg);
    }

    cfg
}

pub fn save_model_config(skill_dir: &Path, cfg: &ExgModelConfig) {
    let _ = std::fs::create_dir_all(skill_dir);
    let path = skill_dir.join(MODEL_CONFIG_FILE);
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, json);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    // ── EegModelConfig defaults ───────────────────────────────────────────

    #[test]
    fn default_config_has_zuna_repo() {
        let cfg = EegModelConfig::default();
        assert_eq!(cfg.hf_repo, ZUNA_HF_REPO);
        assert_eq!(cfg.model_backend, ExgModelBackend::Zuna);
        assert_eq!(cfg.luna_variant, LUNA_DEFAULT_VARIANT);
        assert_eq!(cfg.luna_hf_repo, LUNA_HF_REPO);
    }

    #[test]
    fn default_hnsw_m_matches_constant() {
        let cfg = EegModelConfig::default();
        assert_eq!(cfg.hnsw_m, HNSW_M);
    }

    #[test]
    fn default_hnsw_ef_matches_constant() {
        let cfg = EegModelConfig::default();
        assert_eq!(cfg.hnsw_ef_construction, HNSW_EF_CONSTRUCTION);
    }

    #[test]
    fn default_data_norm_matches_constant() {
        let cfg = EegModelConfig::default();
        assert!((cfg.data_norm - ZUNA_DATA_NORM).abs() < f32::EPSILON);
    }

    // ── JSON round-trip ───────────────────────────────────────────────────

    #[test]
    fn config_round_trips_through_json() {
        let cfg = EegModelConfig {
            hf_repo: "custom/repo".into(),
            hnsw_m: 32,
            hnsw_ef_construction: 400,
            data_norm: 5.0,
            model_backend: ExgModelBackend::Luna,
            luna_variant: "large".into(),
            luna_hf_repo: "PulpBio/LUNA".into(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: EegModelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.hf_repo, "custom/repo");
        assert_eq!(parsed.hnsw_m, 32);
        assert_eq!(parsed.hnsw_ef_construction, 400);
        assert!((parsed.data_norm - 5.0).abs() < f32::EPSILON);
        assert_eq!(parsed.model_backend, ExgModelBackend::Luna);
        assert_eq!(parsed.luna_variant, "large");
    }

    #[test]
    fn config_deserialises_with_missing_fields() {
        let json = r#"{"hf_repo": "test/model"}"#;
        let cfg: EegModelConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.hf_repo, "test/model");
        assert_eq!(cfg.hnsw_m, HNSW_M);
        assert_eq!(cfg.hnsw_ef_construction, HNSW_EF_CONSTRUCTION);
    }

    #[test]
    fn config_deserialises_from_empty_json() {
        let cfg: EegModelConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg.hf_repo, ZUNA_HF_REPO);
    }

    // ── EegModelStatus defaults ───────────────────────────────────────────

    #[test]
    fn status_default_is_inactive() {
        let st = EegModelStatus::default();
        assert!(!st.encoder_loaded);
        assert!(!st.embed_worker_active);
        assert!(!st.weights_found);
        assert!(!st.downloading_weights);
        assert_eq!(st.embeddings_today, 0);
        assert!(st.encoder_describe.is_none());
        assert!(st.weights_path.is_none());
    }

    #[test]
    fn status_round_trips_through_json() {
        let mut st = EegModelStatus::default();
        st.encoder_loaded = true;
        st.embeddings_today = 42;
        st.weights_path = Some("/path/to/weights.safetensors".into());
        let json = serde_json::to_string(&st).unwrap();
        let parsed: EegModelStatus = serde_json::from_str(&json).unwrap();
        assert!(parsed.encoder_loaded);
        assert_eq!(parsed.embeddings_today, 42);
        assert_eq!(parsed.weights_path.as_deref(), Some("/path/to/weights.safetensors"));
    }

    // ── LatestEpochMetrics ────────────────────────────────────────────────

    #[test]
    fn epoch_metrics_default_is_zeroed() {
        let m = LatestEpochMetrics::default();
        assert!((m.rel_alpha).abs() < f32::EPSILON);
        assert!((m.meditation).abs() < f64::EPSILON);
        assert_eq!(m.blink_count, 0);
        assert_eq!(m.epoch_timestamp, 0);
    }

    // ── Persistence ───────────────────────────────────────────────────────

    #[test]
    fn save_and_load_config() {
        let dir = std::env::temp_dir().join("skill_test_model_config");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let cfg = EegModelConfig {
            hf_repo: "test/repo".into(),
            hnsw_m: 24,
            hnsw_ef_construction: 300,
            data_norm: 7.5,
            model_backend: ExgModelBackend::Luna,
            luna_variant: "huge".into(),
            luna_hf_repo: "PulpBio/LUNA".into(),
        };
        save_model_config(&dir, &cfg);
        let loaded = load_model_config(&dir);
        assert_eq!(loaded.hf_repo, "test/repo");
        assert_eq!(loaded.hnsw_m, 24);
        assert_eq!(loaded.hnsw_ef_construction, 300);
        assert!((loaded.data_norm - 7.5).abs() < f32::EPSILON);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_config_returns_default_for_missing_dir() {
        let cfg = load_model_config(Path::new("/nonexistent/path"));
        assert_eq!(cfg.hf_repo, ZUNA_HF_REPO);
    }

    #[test]
    fn load_config_returns_default_for_corrupt_json() {
        let dir = std::env::temp_dir().join("skill_test_model_config_corrupt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MODEL_CONFIG_FILE), "not valid json!!!").unwrap();

        let cfg = load_model_config(&dir);
        assert_eq!(cfg.hf_repo, ZUNA_HF_REPO);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── ExgModelBackend enum ──────────────────────────────────────────────

    const ALL_BACKENDS: &[ExgModelBackend] = &[
        ExgModelBackend::Zuna,
        ExgModelBackend::Luna,
        ExgModelBackend::Reve,
        ExgModelBackend::Cbramod,
        ExgModelBackend::Eegpt,
        ExgModelBackend::Labram,
        ExgModelBackend::Signaljepa,
        ExgModelBackend::Osf,
        ExgModelBackend::Sleepfm,
        ExgModelBackend::Sleeplm,
        ExgModelBackend::Sensorlm,
        ExgModelBackend::Opentslm,
        ExgModelBackend::Tribev2,
    ];

    #[test]
    fn backend_default_is_zuna() {
        assert_eq!(ExgModelBackend::default(), ExgModelBackend::Zuna);
    }

    #[test]
    fn backend_as_str_round_trips_through_from_str_loose() {
        for b in ALL_BACKENDS {
            let s = b.as_str();
            let parsed = ExgModelBackend::from_str_loose(s);
            assert_eq!(&parsed, b, "round-trip failed for {s}");
        }
    }

    #[test]
    fn backend_as_str_is_lowercase_ascii() {
        for b in ALL_BACKENDS {
            let s = b.as_str();
            assert!(!s.is_empty(), "empty as_str for {:?}", b);
            assert!(
                s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                "non-lowercase as_str for {:?}: {s}",
                b
            );
        }
    }

    #[test]
    fn backend_display_matches_as_str() {
        for b in ALL_BACKENDS {
            assert_eq!(b.to_string(), b.as_str());
        }
    }

    #[test]
    fn backend_from_str_loose_is_case_insensitive() {
        assert_eq!(ExgModelBackend::from_str_loose("ZUNA"), ExgModelBackend::Zuna);
        assert_eq!(ExgModelBackend::from_str_loose("Luna"), ExgModelBackend::Luna);
        assert_eq!(ExgModelBackend::from_str_loose("REVE"), ExgModelBackend::Reve);
        assert_eq!(ExgModelBackend::from_str_loose("CbraMod"), ExgModelBackend::Cbramod);
        assert_eq!(ExgModelBackend::from_str_loose("EEGPT"), ExgModelBackend::Eegpt);
        assert_eq!(ExgModelBackend::from_str_loose("LaBraM"), ExgModelBackend::Labram);
        assert_eq!(
            ExgModelBackend::from_str_loose("SignalJEPA"),
            ExgModelBackend::Signaljepa
        );
        assert_eq!(ExgModelBackend::from_str_loose("OSF"), ExgModelBackend::Osf);
        assert_eq!(ExgModelBackend::from_str_loose("SleepFM"), ExgModelBackend::Sleepfm);
        assert_eq!(ExgModelBackend::from_str_loose("SleepLM"), ExgModelBackend::Sleeplm);
        assert_eq!(ExgModelBackend::from_str_loose("SensorLM"), ExgModelBackend::Sensorlm);
        assert_eq!(ExgModelBackend::from_str_loose("OpenTSLM"), ExgModelBackend::Opentslm);
        assert_eq!(ExgModelBackend::from_str_loose("TRIBEv2"), ExgModelBackend::Tribev2);
        assert_eq!(ExgModelBackend::from_str_loose("tribev2"), ExgModelBackend::Tribev2);
    }

    #[test]
    fn backend_from_str_loose_unknown_defaults_to_zuna() {
        assert_eq!(ExgModelBackend::from_str_loose(""), ExgModelBackend::Zuna);
        assert_eq!(ExgModelBackend::from_str_loose("unknown"), ExgModelBackend::Zuna);
        assert_eq!(ExgModelBackend::from_str_loose("gpt4"), ExgModelBackend::Zuna);
    }

    #[test]
    fn backend_serde_round_trip_all_variants() {
        for b in ALL_BACKENDS {
            let json = serde_json::to_string(b).unwrap();
            let parsed: ExgModelBackend = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, b, "serde round-trip failed for {:?}", b);
        }
    }

    #[test]
    fn backend_serde_snake_case_format() {
        // Verify serde uses snake_case as specified by rename_all
        let json = serde_json::to_string(&ExgModelBackend::Signaljepa).unwrap();
        assert_eq!(json, "\"signaljepa\"");
        let json = serde_json::to_string(&ExgModelBackend::Sleepfm).unwrap();
        assert_eq!(json, "\"sleepfm\"");
        let json = serde_json::to_string(&ExgModelBackend::Opentslm).unwrap();
        assert_eq!(json, "\"opentslm\"");
    }

    #[test]
    fn backend_all_variants_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for b in ALL_BACKENDS {
            assert!(seen.insert(b.as_str()), "duplicate as_str for {:?}", b);
        }
    }

    #[test]
    fn config_with_new_backend_round_trips() {
        for b in ALL_BACKENDS {
            let cfg = ExgModelConfig {
                model_backend: b.clone(),
                ..ExgModelConfig::default()
            };
            let json = serde_json::to_string(&cfg).unwrap();
            let parsed: ExgModelConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.model_backend, *b, "config round-trip failed for {:?}", b);
        }
    }

    #[test]
    fn config_with_new_backend_persists_and_loads() {
        let dir = std::env::temp_dir().join("skill_test_exg_backends");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        for b in ALL_BACKENDS {
            let cfg = ExgModelConfig {
                model_backend: b.clone(),
                hf_repo: format!("test/{}", b.as_str()),
                ..ExgModelConfig::default()
            };
            save_model_config(&dir, &cfg);
            let loaded = load_model_config(&dir);
            assert_eq!(loaded.model_backend, *b);
            assert_eq!(loaded.hf_repo, format!("test/{}", b.as_str()));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── ExgModelConfig alias ──────────────────────────────────────────────

    #[test]
    fn eeg_model_config_alias_works() {
        // EegModelConfig is a type alias for ExgModelConfig
        let cfg: EegModelConfig = ExgModelConfig::default();
        assert_eq!(cfg.model_backend, ExgModelBackend::Zuna);
    }
}
