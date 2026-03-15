// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! EEG signal-processing constants.
//!
//! These were originally in the monolithic `constants.rs`.  They are the
//! canonical definitions — the main crate re-exports them.

// ── Hardware ──────────────────────────────────────────────────────────────────

/// Number of EEG channels in the primary pipeline (matches Muse and Ganglion).
pub const EEG_CHANNELS: usize = 4;

/// Human-readable label for each channel index (TP9=0, AF7=1, AF8=2, TP10=3).
pub const CHANNEL_NAMES: [&str; EEG_CHANNELS] = ["TP9", "AF7", "AF8", "TP10"];

/// EEG hardware sample rate (Hz) — Muse and Ganglion both run at 256 Hz.
pub const MUSE_SAMPLE_RATE: f32 = 256.0;

// ── Signal filter (overlap-save, GPU fft_batch) ───────────────────────────────

/// FFT analysis window length (samples).  Must be a power of two.
pub const FILTER_WINDOW: usize = 256;

/// New samples required per channel before a GPU batch is triggered.
pub const FILTER_HOP: usize = 32;

/// Samples carried over from the previous hop as leading context.
pub const FILTER_OVERLAP: usize = FILTER_WINDOW - FILTER_HOP; // 224

// ── Filter defaults ───────────────────────────────────────────────────────────

/// Default low-pass cut-off (Hz).
pub const DEFAULT_LP_HZ: f32 = 50.0;

/// Default high-pass cut-off (Hz).
pub const DEFAULT_HP_HZ: f32 = 0.5;

/// Default notch half-bandwidth (Hz).
pub const DEFAULT_NOTCH_BW_HZ: f32 = 1.0;

// ── Spectrogram ───────────────────────────────────────────────────────────────

/// Number of frequency bins in each spectrogram column (0–50 Hz inclusive).
pub const SPEC_N_FREQ: usize = 51;

// ── Band analysis (Hann-windowed GPU fft_batch) ───────────────────────────────

/// Analysis window length for band power estimation (samples, power of two).
pub const BAND_WINDOW: usize = 512;

/// New samples required per channel before a band snapshot is triggered.
pub const BAND_HOP: usize = 64;

/// Number of clinical EEG frequency bands.
pub const NUM_BANDS: usize = 6;

/// Band table: `(name, lo_hz inclusive, hi_hz exclusive)`.
pub const BANDS: [(&str, f32, f32); NUM_BANDS] = [
    ("delta",       0.5,   4.0),
    ("theta",       4.0,   8.0),
    ("alpha",       8.0,  13.0),
    ("beta",       13.0,  30.0),
    ("gamma",      30.0,  50.0),
    ("high_gamma", 50.0, 100.0),
];

/// Hex colour for each band (same order as [`BANDS`]).
pub const BAND_COLORS: [&str; NUM_BANDS] = [
    "#6366f1", // delta      — indigo
    "#8b5cf6", // theta      — violet
    "#22c55e", // alpha      — green
    "#3b82f6", // beta       — blue
    "#f59e0b", // gamma      — amber
    "#ef4444", // high_gamma — red
];

/// Greek-letter shorthand for each band (same order as [`BANDS`]).
pub const BAND_SYMBOLS: [&str; NUM_BANDS] = ["δ", "θ", "α", "β", "γ", "γ+"];

// ── EEG Embedding (ZUNA model + HNSW index) ──────────────────────────────────

/// Duration of each EEG epoch fed to the ZUNA embedding model (seconds).
pub const EMBEDDING_EPOCH_SECS: f32 = 5.0;

/// Raw samples per embedding epoch per channel.
pub const EMBEDDING_EPOCH_SAMPLES: usize =
    (MUSE_SAMPLE_RATE as usize) * (EMBEDDING_EPOCH_SECS as usize);

/// Default overlap between consecutive embedding epochs (seconds).
pub const EMBEDDING_OVERLAP_SECS: f32 = 2.5;

/// Minimum configurable overlap (seconds).
pub const EMBEDDING_OVERLAP_MIN_SECS: f32 = 0.0;

/// Maximum configurable overlap (seconds).
pub const EMBEDDING_OVERLAP_MAX_SECS: f32 = EMBEDDING_EPOCH_SECS - 0.5;

/// Default overlap expressed in samples.
pub const EMBEDDING_OVERLAP_SAMPLES: usize =
    (EMBEDDING_OVERLAP_SECS * MUSE_SAMPLE_RATE) as usize;

/// Default hop size (samples between epoch emissions).
pub const EMBEDDING_HOP_SAMPLES: usize =
    EMBEDDING_EPOCH_SAMPLES - EMBEDDING_OVERLAP_SAMPLES;

/// Divisor applied to z-scored EEG before entering the ZUNA model.
pub const ZUNA_DATA_NORM: f32 = 10.0;

/// HuggingFace repository identifier for the ZUNA EEG foundation model.
pub const ZUNA_HF_REPO: &str = "Zyphra/ZUNA";

/// Safetensors weights filename within the ZUNA HF repo snapshot.
pub const ZUNA_WEIGHTS_FILE: &str = "model-00001-of-00001.safetensors";

/// Config filename within the ZUNA HF repo snapshot.
pub const ZUNA_CONFIG_FILE: &str = "config.json";

/// HNSW graph connectivity parameter `M`.
pub const HNSW_M: usize = 16;

/// HNSW build-time `ef_construction` — beam width during graph construction.
pub const HNSW_EF_CONSTRUCTION: usize = 200;

/// Filename of the EEG model configuration persisted inside the skill data dir.
pub const MODEL_CONFIG_FILE: &str = "model_config.json";
