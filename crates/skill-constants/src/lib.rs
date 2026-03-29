// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Single source of truth for every constant shared across the NeuroSkill
//! workspace.
//!
//! All signal-processing constants here must stay in sync with their
//! TypeScript mirrors in `src/lib/constants.ts`.
//!
//! # Prelude
//!
//! For convenience, `skill_constants::prelude` re-exports the most frequently
//! used items so crates can write:
//!
//! ```rust,ignore
//! use skill_constants::prelude::*;
//! ```

// ── Poison-recovering Mutex helper ────────────────────────────────────────────

/// Extension trait for `std::sync::Mutex` that recovers from poison.
///
/// Centralised here so every workspace crate can `use skill_constants::MutexExt`
/// instead of duplicating the same 5-line impl.
pub trait MutexExt<T> {
    /// Acquire the lock, recovering the guard even if the mutex is poisoned.
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for std::sync::Mutex<T> {
    #[inline]
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poison| {
            eprintln!(
                "[mutex] WARNING: recovered from poisoned lock at {}:{}",
                file!(),
                line!()
            );
            poison.into_inner()
        })
    }
}

/// Convenience re-exports of the most frequently used constants.
///
/// ```rust,ignore
/// use skill_constants::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{
        emotiv_sample_rate_from_id,
        global_hnsw_file_for,
        hnsw_index_file_for,
        ACTIVITY_FILE,
        BANDS,
        BAND_COLORS,
        BAND_HOP,
        BAND_SYMBOLS,
        BAND_WINDOW,
        CGX_MAX_EEG_CHANNELS,
        CGX_SAMPLE_RATE,
        CHANNEL_NAMES,
        DEFAULT_HP_HZ,
        DEFAULT_LP_HZ,
        DEFAULT_NOTCH_BW_HZ,
        // Hardware
        EEG_CHANNELS,
        EMBEDDING_EPOCH_SAMPLES,
        // Embedding / ZUNA
        EMBEDDING_EPOCH_SECS,
        EMBEDDING_HOP_SAMPLES,
        EMBEDDING_OVERLAP_MAX_SECS,
        EMBEDDING_OVERLAP_MIN_SECS,
        EMBEDDING_OVERLAP_SAMPLES,
        EMBEDDING_OVERLAP_SECS,
        EMOTIV_EPOC_CHANNEL_NAMES,
        EMOTIV_EPOC_EEG_CHANNELS,
        EMOTIV_INSIGHT_CHANNEL_NAMES,
        EMOTIV_INSIGHT_EEG_CHANNELS,
        EMOTIV_SAMPLE_RATE,
        EMOTIV_SAMPLE_RATE_256,
        FILTER_HOP,
        FILTER_OVERLAP,
        // Filter
        FILTER_WINDOW,
        GANGLION_CHANNEL_NAMES,
        GANGLION_SAMPLE_RATE,
        GLOBAL_HNSW_FILE,
        GLOBAL_HNSW_SAVE_EVERY,
        HERMES_CHANNEL_NAMES,
        HERMES_EEG_CHANNELS,
        HERMES_SAMPLE_RATE,
        HNSW_EF_CONSTRUCTION,
        HNSW_INDEX_FILE,
        // HNSW
        HNSW_M,
        HOOKS_LOG_FILE,
        IDUN_CHANNEL_NAMES,
        IDUN_EEG_CHANNELS,
        IDUN_SAMPLE_RATE,
        LABELS_FILE,
        LABEL_CONTEXT_INDEX_FILE,
        LABEL_EEG_INDEX_FILE,
        // Label index
        LABEL_TEXT_INDEX_FILE,
        // LLM
        LLM_CATALOG_FILE,
        LLM_LOG_CAP,
        LLM_LOG_DIR,
        LOG_CONFIG_FILE,
        LUNA_CONFIG_FILE,
        LUNA_DEFAULT_VARIANT,
        LUNA_HF_REPO,
        LUNA_VARIANTS,
        MODEL_CONFIG_FILE,
        MUSE_SAMPLE_RATE,
        MW75_CHANNEL_NAMES,
        MW75_EEG_CHANNELS,
        MW75_SAMPLE_RATE,
        // Bands
        NUM_BANDS,
        PPG_CHANNELS,
        PPG_SAMPLE_RATE,
        SCREENSHOTS_DIR,
        SCREENSHOTS_HNSW,
        SCREENSHOTS_OCR_HNSW,
        // Screenshots
        SCREENSHOTS_SQLITE,
        SCREENSHOT_HNSW_SAVE_EVERY,
        SCREENSHOT_INTERVAL_MAX_MULT,
        SCREENSHOT_INTERVAL_MIN_MULT,
        // Session
        SESSION_GAP_SECS,
        SETTINGS_FILE,
        // Data files
        SQLITE_FILE,
        UMAP_CONFIG_FILE,
        WS_BROADCAST_CAPACITY,
        WS_DEFAULT_PORT,
        // WebSocket
        WS_HOST,
        ZUNA_CONFIG_FILE,
        ZUNA_DATA_NORM,
        ZUNA_HF_REPO,
        ZUNA_WEIGHTS_FILE,
    };

    pub use crate::SKILL_DIR;

    // Agent Skills
    pub use crate::{SKILLS_SUBDIR, SKILL_MARKER};

    // Mutex helper
    pub use crate::MutexExt;
}

// ── Onboarding ───────────────────────────────────────────────────────────────

/// Canonical staged model-download order used by onboarding.
pub const ONBOARDING_MODEL_DOWNLOAD_ORDER: [&str; 5] = ["zuna", "kitten", "neutts", "llm", "ocr"];

// ── Hardware ──────────────────────────────────────────────────────────────────

/// Maximum number of EEG channels in the DSP pipeline.
///
/// Set to 12 to accommodate the MW75 Neuro (12 channels).  Muse and Ganglion
/// sessions only push data to the first 4 channels; the remaining 8 stay
/// silent (zero / no_signal) with negligible overhead.
/// Maximum number of EEG channels processed through the DSP pipeline
/// (filter, FFT, band powers, quality, artifact detection, embeddings).
///
/// All channels are always recorded to CSV/Parquet regardless of this limit.
/// The runtime setting `max_pipeline_channels` (2–1024) controls how many
/// channels are actually processed; this constant is the upper bound for
/// fixed-size DSP arrays.
pub const EEG_CHANNELS: usize = 32;

/// Default channel labels for 4-channel devices (Muse: TP9, AF7, AF8, TP10).
pub const CHANNEL_NAMES: [&str; 4] = ["TP9", "AF7", "AF8", "TP10"];

/// EEG hardware sample rate (Hz) — Muse runs at 256 Hz (Ganglion uses 200 Hz).
pub const MUSE_SAMPLE_RATE: f32 = 256.0;

/// PPG hardware sample rate (Hz) — Muse PPG stream runs at 64 Hz.
pub const PPG_SAMPLE_RATE: f32 = 64.0;

/// Number of PPG optical channels (ambient, infrared, red).
pub const PPG_CHANNELS: usize = 3;

/// IMU sample rate (Hz) — Muse fires at ~52 Hz, 3 samples per notification.
pub const IMU_SAMPLE_RATE: f64 = 52.0;

/// OpenBCI Ganglion hardware sample rate (Hz).
pub const GANGLION_SAMPLE_RATE: f64 = 200.0;

/// OpenBCI Ganglion channel labels (default 10-20 sites when unset).
pub const GANGLION_CHANNEL_NAMES: [&str; 4] = ["Ch1", "Ch2", "Ch3", "Ch4"];

/// Hermes V1 EEG channel count (8-channel ADS1299 at 250 Hz).
pub const HERMES_EEG_CHANNELS: usize = 8;

/// Hermes V1 hardware sample rate (Hz).
pub const HERMES_SAMPLE_RATE: f64 = 250.0;

/// Hermes V1 channel labels (generic — exact placement depends on montage).
pub const HERMES_CHANNEL_NAMES: [&str; HERMES_EEG_CHANNELS] = ["Fp1", "Fp2", "AF3", "AF4", "F3", "F4", "FC1", "FC2"];

/// Neurable MW75 Neuro EEG channel count (12 channels at 500 Hz).
pub const MW75_EEG_CHANNELS: usize = 12;

/// Neurable MW75 hardware sample rate (Hz).
pub const MW75_SAMPLE_RATE: f64 = 500.0;

/// Neurable MW75 channel labels — approximate 10-20 extended positions.
///
/// 6 electrodes are spread equidistantly around each ear cup:
///   Left  (Ch1–Ch6):  FT7, T7, TP7, CP5, P7, C5
///   Right (Ch7–Ch12): FT8, T8, TP8, CP6, P8, C6
pub const MW75_CHANNEL_NAMES: [&str; MW75_EEG_CHANNELS] = [
    "FT7", "T7", "TP7", "CP5", "P7", "C5", "FT8", "T8", "TP8", "CP6", "P8", "C6",
];

/// Emotiv EPOC X / EPOC+ EEG channel count (14 channels).
pub const EMOTIV_EPOC_EEG_CHANNELS: usize = 14;

/// Emotiv Insight EEG channel count (5 channels).
pub const EMOTIV_INSIGHT_EEG_CHANNELS: usize = 5;

/// Emotiv default sample rate (Hz) — used as fallback when the model is
/// unknown.  EPOC X, EPOC+, EPOC Flex, Insight 2, and MN8 stream at 256 Hz;
/// older EPOC (standard) and Insight 1 stream at 128 Hz.
pub const EMOTIV_SAMPLE_RATE: f64 = 128.0;

/// Emotiv EPOC X / EPOC+ / EPOC Flex / Insight 2 / MN8 sample rate (Hz).
pub const EMOTIV_SAMPLE_RATE_256: f64 = 256.0;

/// Derive the Emotiv EEG sample rate from the headset ID prefix.
///
/// Headset IDs follow the pattern `MODEL-SERIAL` (e.g. `EPOCPLUS-06F2DDBC`,
/// `INSIGHT-5AF2C39E`, `EPOCX-A1B2C3D4`, `EPOCFLEX-12345678`).
///
/// Returns 256 Hz for EPOC X, EPOC+, EPOC Flex, Insight 2, MN8, X-Trodes;
/// 128 Hz for Insight (v1) and unknown models.
pub fn emotiv_sample_rate_from_id(headset_id: &str) -> f64 {
    let upper = headset_id.to_uppercase();
    if upper.starts_with("EPOCX")
        || upper.starts_with("EPOCPLUS")
        || upper.starts_with("EPOCFLEX")
        || upper.starts_with("INSIGHT2")
        || upper.starts_with("MN8")
        || upper.starts_with("XTRODES")
    {
        EMOTIV_SAMPLE_RATE_256
    } else {
        // Insight v1, legacy EPOC, unknown
        EMOTIV_SAMPLE_RATE
    }
}

/// Emotiv EPOC X / EPOC+ channel labels (14 electrodes, 10-20 extended).
pub const EMOTIV_EPOC_CHANNEL_NAMES: [&str; EMOTIV_EPOC_EEG_CHANNELS] = [
    "AF3", "F7", "F3", "FC5", "T7", "P7", "O1", "O2", "P8", "T8", "FC6", "F4", "F8", "AF4",
];

/// Emotiv Insight channel labels (5 electrodes).
pub const EMOTIV_INSIGHT_CHANNEL_NAMES: [&str; EMOTIV_INSIGHT_EEG_CHANNELS] = ["AF3", "AF4", "T7", "T8", "Pz"];

/// Maximum EEG channel count across all Cognionics / CGX models.
///
/// The Quick-32r has 30 EEG electrodes — the highest of any CGX headset.
/// The actual channel count and electrode labels are determined at runtime
/// from the USB descriptor via the `cognionics` crate's `DeviceConfig`.
///
/// | Model | EEG ch | ExG | ACC | Rate |
/// |---|---|---|---|---|
/// | Quick-20 / 20r / 20m | 20 | 1–4 | ✓* | 500 Hz |
/// | Quick-32r | 30 | 2 | ✓ | 500 Hz |
/// | Quick-8r | 9 | 1 | ✓ | 500 Hz |
/// | AIM-2 | 0 | 11 | ✗ | 500 Hz |
/// | Dev Kit | 8 | 0 | ✓ | 500 Hz |
/// | Patch-v1 / v2 | 2 | 2–3 | ✓ | 250 Hz |
///
/// *Quick-20 (original wired) has no ACC; all wireless variants do.
pub const CGX_MAX_EEG_CHANNELS: usize = 30;

/// Cognionics / CGX default hardware sample rate (Hz).
/// Most models run at 500 Hz; Patch-v1/v2 run at 250 Hz.
pub const CGX_SAMPLE_RATE: f64 = 500.0;

/// IDUN Guardian EEG channel count (single bipolar channel at 250 Hz).
pub const IDUN_EEG_CHANNELS: usize = 1;

/// IDUN Guardian hardware sample rate (Hz).
pub const IDUN_SAMPLE_RATE: f64 = 250.0;

/// IDUN Guardian channel label (bipolar in-ear montage).
pub const IDUN_CHANNEL_NAMES: [&str; IDUN_EEG_CHANNELS] = ["EEG"];

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
    ("delta", 0.5, 4.0),
    ("theta", 4.0, 8.0),
    ("alpha", 8.0, 13.0),
    ("beta", 13.0, 30.0),
    ("gamma", 30.0, 50.0),
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

// ── EEG signal quality thresholds ─────────────────────────────────────────────

/// Rolling window length for quality assessment (samples, 1 s at 256 Hz).
pub const QUALITY_WINDOW: usize = 256;

/// RMS below this → electrode not in contact (µV).
pub const QUALITY_NO_SIGNAL_RMS: f64 = 5.0;

/// RMS above this → gross movement or sustained saturation (µV).
pub const QUALITY_POOR_RMS: f64 = 400.0;

/// Samples whose absolute value exceeds this are counted as clips (µV).
pub const QUALITY_CLIP_UV: f64 = 1200.0;

/// Eight or more clips per window → Poor quality.
pub const QUALITY_POOR_CLIPS: usize = 8;

/// RMS above this → noticeable artifact or poor contact / Fair (µV).
pub const QUALITY_FAIR_RMS: f64 = 100.0;

// ── Artifact detection (blinks) ───────────────────────────────────────────────

/// Blink detection: minimum µV spike amplitude on frontal channels (AF7/AF8).
pub const BLINK_THRESHOLD_UV: f64 = 80.0;

/// Minimum gap between consecutive blinks (seconds).
pub const BLINK_REFRACTORY_S: f64 = 0.3;

/// Sliding window for blinks-per-minute calculation (seconds).
pub const BLINK_RATE_WINDOW_S: f64 = 60.0;

// ── Head pose (IMU complementary filter) ──────────────────────────────────────

/// Complementary filter coefficient (0–1).  Higher = more trust in gyro.
pub const HEAD_POSE_ALPHA: f64 = 0.96;

/// Stillness EMA smoothing time constant (seconds).
pub const HEAD_POSE_STILL_TAU_S: f64 = 1.0;

/// Angular velocity (°/s) below which stillness score is ~100.
pub const HEAD_POSE_STILL_QUIET_DPS: f64 = 3.0;

/// Angular velocity (°/s) above which stillness score is ~0.
pub const HEAD_POSE_STILL_ACTIVE_DPS: f64 = 50.0;

/// Minimum pitch delta (degrees) within the nod window.
pub const HEAD_POSE_NOD_THRESHOLD_DEG: f64 = 12.0;

/// Minimum yaw delta (degrees) within the shake window.
pub const HEAD_POSE_SHAKE_THRESHOLD_DEG: f64 = 15.0;

/// Gesture detection window (seconds).
pub const HEAD_POSE_GESTURE_WINDOW_S: f64 = 0.6;

/// Gesture refractory period (seconds).
pub const HEAD_POSE_GESTURE_REFRACTORY_S: f64 = 1.0;

// ── PPG analysis ──────────────────────────────────────────────────────────────

/// Minimum inter-beat interval (seconds) — corresponds to ~200 BPM.
pub const PPG_IBI_MIN_S: f64 = 0.3;

/// Maximum inter-beat interval (seconds) — corresponds to ~30 BPM.
pub const PPG_IBI_MAX_S: f64 = 2.0;

// ── SNR / focus-mode thresholds ───────────────────────────────────────────────

/// SNR threshold (dB) below which the signal is considered low quality.
pub const SNR_LOW_DB: f32 = 0.0;

/// Consecutive ticks below [`SNR_LOW_DB`] before focus mode exits (60 s × 4 Hz).
pub const SNR_LOW_TICKS: u32 = 240;

// ── Session segmentation ──────────────────────────────────────────────────────

/// Maximum gap (seconds) between consecutive EEG epochs before a new session
/// boundary is created.  Two minutes without data starts a new session.
pub const SESSION_GAP_SECS: u64 = 120;

// ── EEG Embedding (ZUNA model + HNSW index) ──────────────────────────────────

/// Duration of each EEG epoch fed to the ZUNA embedding model (seconds).
///
/// Screenshot capture interval is aligned to multiples of this value
/// (1× = 5 s, 2× = 10 s, …, 12× = 60 s).
pub const EMBEDDING_EPOCH_SECS: f32 = 5.0;

/// Minimum screenshot interval multiplier (1× epoch = 5 s).
pub const SCREENSHOT_INTERVAL_MIN_MULT: u32 = 1;

/// Maximum screenshot interval multiplier (12× epoch = 60 s).
pub const SCREENSHOT_INTERVAL_MAX_MULT: u32 = 12;

/// Raw samples per embedding epoch per channel.
pub const EMBEDDING_EPOCH_SAMPLES: usize = (MUSE_SAMPLE_RATE as usize) * (EMBEDDING_EPOCH_SECS as usize);

/// Default overlap between consecutive embedding epochs (seconds).
pub const EMBEDDING_OVERLAP_SECS: f32 = 0.0;

/// Minimum configurable overlap (seconds).
pub const EMBEDDING_OVERLAP_MIN_SECS: f32 = 0.0;

/// Maximum configurable overlap (seconds).
pub const EMBEDDING_OVERLAP_MAX_SECS: f32 = EMBEDDING_EPOCH_SECS - 0.5;

/// Default overlap expressed in samples.
pub const EMBEDDING_OVERLAP_SAMPLES: usize = (EMBEDDING_OVERLAP_SECS * MUSE_SAMPLE_RATE) as usize;

/// Default hop size (samples between epoch emissions).
pub const EMBEDDING_HOP_SAMPLES: usize = EMBEDDING_EPOCH_SAMPLES - EMBEDDING_OVERLAP_SAMPLES;

/// Divisor applied to z-scored EEG before entering the ZUNA model.
pub const ZUNA_DATA_NORM: f32 = 10.0;

/// HuggingFace repository identifier for the ZUNA EEG foundation model.
pub const ZUNA_HF_REPO: &str = "Zyphra/ZUNA";

/// Safetensors weights filename within the ZUNA HF repo snapshot.
pub const ZUNA_WEIGHTS_FILE: &str = "model-00001-of-00001.safetensors";

/// Config filename within the ZUNA HF repo snapshot.
pub const ZUNA_CONFIG_FILE: &str = "config.json";

/// HuggingFace repository identifier for the LUNA EEG foundation model.
pub const LUNA_HF_REPO: &str = "PulpBio/LUNA";

/// Config filename within the LUNA HF repo snapshot.
pub const LUNA_CONFIG_FILE: &str = "config.json";

/// Available LUNA model size variants: `(variant_name, weights_filename)`.
pub const LUNA_VARIANTS: [(&str, &str); 3] = [
    ("base", "LUNA_base.safetensors"),
    ("large", "LUNA_large.safetensors"),
    ("huge", "LUNA_huge.safetensors"),
];

/// Default LUNA model variant.
pub const LUNA_DEFAULT_VARIANT: &str = "base";

/// Per-variant LUNA model hyperparameters: `(variant, embed_dim, num_queries, depth, num_heads)`.
///
/// These override the generic `config.json` defaults so that each checkpoint
/// is loaded with the correct architecture.  Derived from weight-tensor shapes.
pub const LUNA_VARIANT_CONFIGS: [(&str, usize, usize, usize, usize); 3] = [
    // variant, embed_dim, num_queries, depth, num_heads
    ("base", 64, 4, 8, 2),
    ("large", 96, 6, 10, 2),
    ("huge", 128, 8, 24, 2),
];

/// Look up LUNA model hyperparameters for a variant name.
///
/// Returns `(embed_dim, num_queries, depth, num_heads)` or `None` if the
/// variant is unrecognised.
pub fn luna_variant_config(variant: &str) -> Option<(usize, usize, usize, usize)> {
    LUNA_VARIANT_CONFIGS
        .iter()
        .find(|(v, _, _, _, _)| *v == variant)
        .map(|(_, e, q, d, h)| (*e, *q, *d, *h))
}

/// HNSW graph connectivity parameter `M`.
pub const HNSW_M: usize = 16;

/// HNSW build-time `ef_construction` — beam width during graph construction.
pub const HNSW_EF_CONSTRUCTION: usize = 200;

/// Filename of the EEG model configuration persisted inside the skill data dir.
pub const MODEL_CONFIG_FILE: &str = "model_config.json";

/// Filename of the UMAP projection configuration.
pub const UMAP_CONFIG_FILE: &str = "umap_config.json";

// ── Data files ────────────────────────────────────────────────────────────────

/// SQLite database that stores user-authored labels (`~/.skill/labels.sqlite`).
pub const LABELS_FILE: &str = "labels.sqlite";

/// SQLite database for screenshot metadata and embedding blobs.
pub const SCREENSHOTS_SQLITE: &str = "screenshots.sqlite";

/// Directory name under `~/.skill/` for captured screenshot images.
pub const SCREENSHOTS_DIR: &str = "screenshots";

/// HNSW index file for visual-similarity search over screenshot embeddings.
pub const SCREENSHOTS_HNSW: &str = "screenshots.hnsw";

/// Number of new screenshot embeddings between periodic HNSW saves.
pub const SCREENSHOT_HNSW_SAVE_EVERY: usize = 10;

/// HNSW index file for text-similarity search over OCR text embeddings.
pub const SCREENSHOTS_OCR_HNSW: &str = "screenshots_ocr.hnsw";

/// URL for the ocrs text-detection model (~10 MB).
pub const OCR_DETECTION_MODEL_URL: &str = "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten";

/// URL for the ocrs text-recognition model (~10 MB).
pub const OCR_RECOGNITION_MODEL_URL: &str = "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten";

/// Filename for the cached OCR detection model.
pub const OCR_DETECTION_MODEL_FILE: &str = "text-detection.rten";

/// Filename for the cached OCR recognition model.
pub const OCR_RECOGNITION_MODEL_FILE: &str = "text-recognition.rten";

// ── Label index files ─────────────────────────────────────────────────────────

/// HNSW index for text embeddings of label text.
pub const LABEL_TEXT_INDEX_FILE: &str = "label_text_index.hnsw";

/// HNSW index for text embeddings of label context.
pub const LABEL_CONTEXT_INDEX_FILE: &str = "label_context_index.hnsw";

/// HNSW index for EEG embeddings of label epochs.
pub const LABEL_EEG_INDEX_FILE: &str = "label_eeg_index.hnsw";

// ── LLM ───────────────────────────────────────────────────────────────────────

/// Filename of the persisted LLM model catalog.
pub const LLM_CATALOG_FILE: &str = "llm_catalog.json";

/// Directory name under `skill_dir` for LLM session log files.
pub const LLM_LOG_DIR: &str = "llm_logs";

/// Maximum entries in the shared LLM log ring-buffer.
pub const LLM_LOG_CAP: usize = 500;

// ── TTS ───────────────────────────────────────────────────────────────────────

/// Tauri event name for TTS progress notifications.
pub const TTS_PROGRESS_EVENT: &str = "tts-progress";

/// Seconds of silence appended after synthesised speech.
pub const TTS_TAIL_SILENCE_SECS: f32 = 0.25;

/// HuggingFace repository for the KittenTTS model.
pub const KITTEN_TTS_HF_REPO: &str = "KittenML/kitten-tts-mini-0.8";

/// Default KittenTTS voice name.
pub const KITTEN_TTS_VOICE_DEFAULT: &str = "Jasper";

/// Default KittenTTS speech speed multiplier.
pub const KITTEN_TTS_SPEED: f32 = 1.0;

// ── Tray ──────────────────────────────────────────────────────────────────────

/// Minimum milliseconds between tray menu rebuilds (debounce).
pub const MENU_REBUILD_MIN_MS: u64 = 300;

// ── Tool calling ──────────────────────────────────────────────────────────────

/// Opening delimiter for tool-call blocks in model output.
pub const TOOL_CALL_START: &str = "[TOOL_CALL]";

/// Closing delimiter for tool-call blocks in model output.
pub const TOOL_CALL_END: &str = "[/TOOL_CALL]";

/// Maximum characters retained from tool results in chat context.
pub const TOOL_MAX_RESULT_CHARS: usize = 2000;

/// Bash command size threshold (bytes) above which the command is written to a
/// script file instead of passed via `bash -c`.
pub const TOOL_BASH_SCRIPT_THRESHOLD: usize = 8 * 1024;

/// Lines of bash output shown in the head portion of a summarised result.
pub const TOOL_BASH_SUMMARY_HEAD: usize = 20;

/// Lines of bash output shown in the tail portion of a summarised result.
pub const TOOL_BASH_SUMMARY_TAIL: usize = 20;

/// Below this many lines, bash output is returned inline without summarisation.
pub const TOOL_BASH_INLINE_THRESHOLD: usize = 200;

/// Maximum number of results returned by the `web_search` tool.
/// Fewer results = less context consumed, leaving room for follow-up fetches.
pub const TOOL_WEB_SEARCH_MAX_RESULTS: usize = 5;

/// Maximum URL length (chars) kept in web search results. Longer URLs are
/// truncated with a `...` suffix to save context space.
pub const TOOL_WEB_SEARCH_MAX_URL_LEN: usize = 120;

/// Maximum characters for the condensed web-search tool result injected into
/// chat context.  Tighter than the generic `TOOL_MAX_RESULT_CHARS` because
/// search results are follow-up pointers, not final answers.
pub const TOOL_WEB_SEARCH_MAX_RESULT_CHARS: usize = 1500;

// ── Active window tracking ────────────────────────────────────────────────────

/// Seconds of inactivity before a user is considered idle.
pub const ACTIVE_WINDOW_IDLE_THRESHOLD_SECS: f64 = 2.0;

// ── WebSocket server ──────────────────────────────────────────────────────────

/// Broadcast channel capacity for WebSocket clients.
pub const WS_BROADCAST_CAPACITY: usize = 512;

/// Maximum entries in the per-client request audit log.
pub const WS_MAX_REQUEST_LOG: usize = 200;

pub const WS_HOST: &str = "127.0.0.1";
pub const WS_DEFAULT_PORT: u16 = 8375;

// ── DND (Do Not Disturb) ─────────────────────────────────────────────────────

/// Reverse-DNS client identifier for DND platform APIs.
pub const DND_CLIENT_ID: &str = "com.neuroskill.app.dnd";

/// Default focus-mode identifier on Linux.
pub const DND_LINUX_MODE_ID: &str = "linux.dnd.default";

/// Default focus-mode identifier on Windows.
pub const DND_WINDOWS_MODE_ID: &str = "windows.dnd.default";

// ── Calibration ───────────────────────────────────────────────────────────────

pub const CALIBRATION_ACTION1_LABEL: &str = "Eyes Open";
pub const CALIBRATION_ACTION2_LABEL: &str = "Eyes Closed";
pub const CALIBRATION_ACTION_DURATION_SECS: u32 = 10;
pub const CALIBRATION_BREAK_DURATION_SECS: u32 = 5;
pub const CALIBRATION_LOOP_COUNT: u32 = 3;
pub const CALIBRATION_AUTO_START: bool = true;

// ── Settings & logging ────────────────────────────────────────────────────────

/// Filename of all user-configured app settings.
pub const SETTINGS_FILE: &str = "settings.json";

/// Filename of the per-subsystem logging configuration.
pub const LOG_CONFIG_FILE: &str = "log_config.json";

// ── HNSW index files ──────────────────────────────────────────────────────────

/// Per-day HNSW embedding index filename (default / ZUNA).
pub const HNSW_INDEX_FILE: &str = "eeg_embeddings.hnsw";

/// Global cross-day HNSW index filename (default / ZUNA).
pub const GLOBAL_HNSW_FILE: &str = "eeg_global.hnsw";

/// Return the per-day HNSW filename for a given model backend.
///
/// - `"zuna"` or `""` or `None`-like → `"eeg_embeddings.hnsw"` (backward compat)
/// - `"luna"` → `"eeg_embeddings_luna.hnsw"`
/// - other   → `"eeg_embeddings_{backend}.hnsw"`
pub fn hnsw_index_file_for(backend: &str) -> String {
    match backend {
        "" | "zuna" => HNSW_INDEX_FILE.to_string(),
        other => format!("eeg_embeddings_{other}.hnsw"),
    }
}

/// Return the global HNSW filename for a given model backend.
pub fn global_hnsw_file_for(backend: &str) -> String {
    match backend {
        "" | "zuna" => GLOBAL_HNSW_FILE.to_string(),
        other => format!("eeg_global_{other}.hnsw"),
    }
}

/// Periodic save interval for the global HNSW index.
pub const GLOBAL_HNSW_SAVE_EVERY: usize = 10;

/// Per-day SQLite database filename.
pub const SQLITE_FILE: &str = "eeg.sqlite";

/// Activity tracking database filename.
pub const ACTIVITY_FILE: &str = "activity.sqlite";

/// Hooks audit-log database filename.
pub const HOOKS_LOG_FILE: &str = "hooks.sqlite";

// ── mDNS / Bonjour ────────────────────────────────────────────────────────────

pub const MDNS_SERVICE_SUFFIX: &str = "._tcp.local.";
pub const MDNS_HOST_SUFFIX: &str = ".local.";
pub const MDNS_TXT_VERSION: &str = "1";
pub const MDNS_TXT_FORMAT: &str = "json";

// ── Updater ───────────────────────────────────────────────────────────────────

/// Interval between automatic background update checks (seconds).
pub const UPDATER_CHECK_INTERVAL_SECS: u64 = 3600;

// ── Autostart ─────────────────────────────────────────────────────────────────

/// Reverse-DNS prefix for macOS LaunchAgent plist filename.
#[cfg(target_os = "macos")]
pub const AUTOSTART_PLIST_LABEL_PREFIX: &str = "com.neuroskill";

// ── Application identity & credits ───────────────────────────────────────────

pub const APP_DISPLAY_NAME: &str = "NeuroSkill™";
pub const APP_TAGLINE: &str =
    "Real-time EXG State of Mind system and brain-state monitoring for Muse, OpenBCI, Emotiv, IDUN, and other BCI devices.";
pub const APP_WEBSITE: &str = "https://neuroskill.com";
pub const APP_WEBSITE_LABEL: &str = "neuroskill.com";
pub const APP_REPO_URL: &str = "https://github.com/NeuroSkill-com/skill";
pub const APP_DISCORD_URL: &str = "https://discord.gg/Rcvb8Cx4cZ";
pub const APP_LICENSE: &str = "GPL-3.0-only";
pub const APP_LICENSE_NAME: &str = "GNU General Public License v3";
pub const APP_LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";
pub const APP_COPYRIGHT: &str = "© 2025–2026 NeuroSkill.com";

/// Ordered list of contributors.
pub const APP_AUTHORS: &[(&str, &str)] = &[
    ("Eugene Hauptmann", "Lead developer & EEG signal processing"),
    ("Nataliya Kosmyna", "Neuroscience and Brain Computer Interfaces"),
];

pub const APP_ACKNOWLEDGEMENTS: &str = "Built with Tauri, SvelteKit, and the ZUNA EEG foundation model by Zyphra. \
     EEG band-power research based on work by Klimesch (1999), \
     Pope et al. (1995), and Kosmyna & Maes (2019).";

// ── Skill data directory ──────────────────────────────────────────────────────

/// The skill data directory name used on macOS and Linux (`~/.skill`).
/// On Windows the *user-global* data directory is `%LOCALAPPDATA%\NeuroSkill`
/// (see `skill-settings::default_skill_dir`), but this constant is still used
/// for *project-local* config directories (`.skill/` in the project root) on
/// all platforms.
pub const SKILL_DIR: &str = ".skill";

/// Subdirectory under `SKILL_DIR` (or project root) for Agent Skills.
pub const SKILLS_SUBDIR: &str = "skills";

/// The marker filename that identifies a directory as a skill root.
pub const SKILL_MARKER: &str = "SKILL.md";

// ── iroh remote streaming ─────────────────────────────────────────────────────

/// Data watchdog timeout for locally connected BLE devices (seconds).
/// If no [`DeviceEvent`] arrives within this window, the session is treated
/// as silently disconnected.  15 s is generous for BLE (supervision timeout
/// is typically 2–6 s).
pub const DATA_WATCHDOG_SECS: u64 = 15;

/// Extended data watchdog for iroh-remote sessions (seconds).
/// The phone's QUIC tunnel may take 30–60 s to reconnect after a network
/// interruption while BLE data continues recording into the phone's local
/// outbox.  90 s prevents premature session termination on the desktop.
/// Extended watchdog for iroh-remote sessions.  With 0.25s streaming
/// chunks the normal 15s watchdog would be fine, but relay reconnection
/// can take 10–20s, so we keep a modest buffer.  The synthetic
/// `DeviceDisconnected` from `device_receiver.rs` handles the fast path.
pub const DATA_WATCHDOG_IROH_SECS: u64 = 30;

/// Maximum age of unsent messages in the phone's outbox before they are
/// pruned (seconds).  24 hours — old enough to survive overnight gaps.
pub const OUTBOX_MAX_AGE_SECS: u64 = 86_400;

/// Duration of each sensor chunk accumulated on the phone before sending
/// to the desktop (seconds).  5 s aligns with the embedding epoch window.
pub const DEVICE_PROXY_EPOCH_SECS: f32 = 5.0;

/// Maximum raw (uncompressed) payload size for a single device proxy
/// message (bytes).  2 MiB — large enough for 5 s × 32 ch × 256 Hz.
pub const DEVICE_PROXY_MAX_PAYLOAD: usize = 2_097_152;

/// Number of outbox messages to send per drain batch.  Small batches
/// yield to live data sends between batches.
pub const OUTBOX_DRAIN_BATCH_SIZE: usize = 4;

/// Pause between drain batches (milliseconds).  Lets live sensor data
/// through the QUIC connection before the next backlog batch is sent.
pub const OUTBOX_DRAIN_INTER_BATCH_MS: u64 = 200;

/// How long the drain loop waits before re-checking an empty outbox
/// (milliseconds).
pub const OUTBOX_DRAIN_IDLE_MS: u64 = 2_000;

/// Maximum consecutive empty outbox polls before the drain loop exits.
/// `set_connection` restarts the drain on the next reconnect.
/// 30 × 2 s = 60 s idle before exiting.
pub const OUTBOX_DRAIN_MAX_IDLE_POLLS: u32 = 30;

/// Discard partial embedding epoch data if no EEG samples arrive for
/// this duration (seconds).  Prevents stale data from producing
/// misleading embeddings when recording resumes after a long gap.
pub const STALE_EPOCH_TIMEOUT_SECS: u64 = 3600;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Filter constants consistency ──────────────────────────────────────

    #[test]
    fn filter_overlap_equals_window_minus_hop() {
        assert_eq!(FILTER_OVERLAP, FILTER_WINDOW - FILTER_HOP);
    }

    #[test]
    fn filter_window_is_power_of_two() {
        assert!(FILTER_WINDOW.is_power_of_two());
    }

    // ── Band definitions ─────────────────────────────────────────────────

    #[test]
    fn bands_count_matches_num_bands() {
        assert_eq!(BANDS.len(), NUM_BANDS);
        assert_eq!(BAND_COLORS.len(), NUM_BANDS);
        assert_eq!(BAND_SYMBOLS.len(), NUM_BANDS);
    }

    #[test]
    fn bands_are_contiguous_and_ascending() {
        for i in 1..BANDS.len() {
            let (_, _, prev_hi) = BANDS[i - 1];
            let (_, curr_lo, curr_hi) = BANDS[i];
            assert!(
                (curr_lo - prev_hi).abs() < f32::EPSILON,
                "band gap between {} and {}: {} vs {}",
                BANDS[i - 1].0,
                BANDS[i].0,
                prev_hi,
                curr_lo
            );
            assert!(curr_hi > curr_lo, "band {} has hi <= lo", BANDS[i].0);
        }
    }

    #[test]
    fn first_band_starts_above_zero() {
        assert!(BANDS[0].1 > 0.0);
    }

    // ── Embedding epoch ──────────────────────────────────────────────────

    #[test]
    fn embedding_epoch_samples_matches_rate_times_secs() {
        let expected = (MUSE_SAMPLE_RATE * EMBEDDING_EPOCH_SECS as f32) as usize;
        assert_eq!(EMBEDDING_EPOCH_SAMPLES, expected);
    }

    #[test]
    fn embedding_overlap_within_bounds() {
        assert!(EMBEDDING_OVERLAP_SECS >= EMBEDDING_OVERLAP_MIN_SECS);
        assert!(EMBEDDING_OVERLAP_SECS <= EMBEDDING_OVERLAP_MAX_SECS);
    }

    // ── Channel names ────────────────────────────────────────────────────

    #[test]
    fn muse_has_4_channels() {
        assert_eq!(CHANNEL_NAMES.len(), 4);
    }

    #[test]
    fn eeg_channels_accommodates_all_devices() {
        assert!(EEG_CHANNELS >= CHANNEL_NAMES.len());
        assert!(EEG_CHANNELS >= HERMES_EEG_CHANNELS);
        assert!(EEG_CHANNELS >= MW75_EEG_CHANNELS);
        assert!(EEG_CHANNELS >= CGX_MAX_EEG_CHANNELS);
    }

    #[test]
    fn device_channel_names_match_counts() {
        assert_eq!(GANGLION_CHANNEL_NAMES.len(), 4);
        assert_eq!(HERMES_CHANNEL_NAMES.len(), HERMES_EEG_CHANNELS);
        assert_eq!(MW75_CHANNEL_NAMES.len(), MW75_EEG_CHANNELS);
        assert_eq!(EMOTIV_EPOC_CHANNEL_NAMES.len(), EMOTIV_EPOC_EEG_CHANNELS);
        assert_eq!(EMOTIV_INSIGHT_CHANNEL_NAMES.len(), EMOTIV_INSIGHT_EEG_CHANNELS);
        assert_eq!(IDUN_CHANNEL_NAMES.len(), IDUN_EEG_CHANNELS);
        assert!(CGX_MAX_EEG_CHANNELS >= 30); // Quick-32r has 30 EEG channels
    }

    // ── Emotiv sample rate derivation ────────────────────────────────────

    #[test]
    fn emotiv_epocx_is_256hz() {
        assert_eq!(emotiv_sample_rate_from_id("EPOCX-A1B2C3D4"), 256.0);
    }

    #[test]
    fn emotiv_epocplus_is_256hz() {
        assert_eq!(emotiv_sample_rate_from_id("EPOCPLUS-06F2DDBC"), 256.0);
    }

    #[test]
    fn emotiv_insight_v1_is_128hz() {
        assert_eq!(emotiv_sample_rate_from_id("INSIGHT-5AF2C39E"), 128.0);
    }

    #[test]
    fn emotiv_unknown_is_128hz() {
        assert_eq!(emotiv_sample_rate_from_id("UNKNOWN-DEVICE"), 128.0);
    }

    #[test]
    fn emotiv_case_insensitive() {
        assert_eq!(emotiv_sample_rate_from_id("epocx-lowercase"), 256.0);
    }

    // ── Quality thresholds ───────────────────────────────────────────────

    #[test]
    fn quality_thresholds_ordered() {
        assert!(QUALITY_NO_SIGNAL_RMS < QUALITY_FAIR_RMS);
        assert!(QUALITY_FAIR_RMS < QUALITY_POOR_RMS);
    }

    // ── MutexExt ─────────────────────────────────────────────────────────

    #[test]
    fn mutex_ext_locks_normally() {
        let m = std::sync::Mutex::new(42);
        let g = m.lock_or_recover();
        assert_eq!(*g, 42);
    }

    #[test]
    fn mutex_ext_recovers_from_poison() {
        let m = std::sync::Arc::new(std::sync::Mutex::new(99));
        let m2 = m.clone();
        let _ = std::thread::spawn(move || {
            let _g = m2.lock().unwrap();
            panic!("intentional poison");
        })
        .join();
        // Mutex is now poisoned — lock_or_recover should still work
        let g = m.lock_or_recover();
        assert_eq!(*g, 99);
    }
}
