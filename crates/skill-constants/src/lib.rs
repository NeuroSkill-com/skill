// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Single source of truth for every constant shared across the NeuroSkill
//! workspace.
//!
//! All signal-processing constants here must stay in sync with their
//! TypeScript mirrors in `src/lib/constants.ts`.

// ── Onboarding ───────────────────────────────────────────────────────────────

/// Canonical staged model-download order used by onboarding.
pub const ONBOARDING_MODEL_DOWNLOAD_ORDER: [&str; 5] = [
    "zuna", "kitten", "neutts", "llm", "ocr",
];

// ── Hardware ──────────────────────────────────────────────────────────────────

/// Number of EEG channels in the primary pipeline (matches Muse and Ganglion).
pub const EEG_CHANNELS: usize = 4;

/// Human-readable label for each channel index (TP9=0, AF7=1, AF8=2, TP10=3).
pub const CHANNEL_NAMES: [&str; EEG_CHANNELS] = ["TP9", "AF7", "AF8", "TP10"];

/// EEG hardware sample rate (Hz) — Muse and Ganglion both run at 256 Hz.
pub const MUSE_SAMPLE_RATE: f32 = 256.0;

/// PPG hardware sample rate (Hz) — Muse PPG stream runs at 64 Hz.
pub const PPG_SAMPLE_RATE: f32 = 64.0;

/// OpenBCI Ganglion channel labels (default 10-20 sites when unset).
pub const GANGLION_CHANNEL_NAMES: [&str; 4] = ["Ch1", "Ch2", "Ch3", "Ch4"];

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
pub const OCR_DETECTION_MODEL_URL: &str =
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten";

/// URL for the ocrs text-recognition model (~10 MB).
pub const OCR_RECOGNITION_MODEL_URL: &str =
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten";

/// Filename for the cached OCR detection model.
pub const OCR_DETECTION_MODEL_FILE: &str = "text-detection.rten";

/// Filename for the cached OCR recognition model.
pub const OCR_RECOGNITION_MODEL_FILE: &str = "text-recognition.rten";

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

/// Per-day HNSW embedding index filename.
pub const HNSW_INDEX_FILE: &str = "eeg_embeddings.hnsw";

/// Global cross-day HNSW index filename.
pub const GLOBAL_HNSW_FILE: &str = "eeg_global.hnsw";

/// Periodic save interval for the global HNSW index.
pub const GLOBAL_HNSW_SAVE_EVERY: usize = 10;

/// Per-day SQLite database filename.
pub const SQLITE_FILE: &str = "eeg.sqlite";

/// Activity tracking database filename.
pub const ACTIVITY_FILE: &str = "activity.sqlite";

/// Hooks audit-log database filename.
pub const HOOKS_LOG_FILE: &str = "hooks.sqlite";

// ── WebSocket server ──────────────────────────────────────────────────────────

/// Broadcast channel capacity for WebSocket clients.
pub const WS_BROADCAST_CAPACITY: usize = 512;

pub const WS_HOST: &str = "127.0.0.1";
pub const WS_DEFAULT_PORT: u16 = 8375;

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
    "Real-time EXG State of Mind system and brain-state monitoring for Muse, OpenBCI, and other BCI devices.";
pub const APP_WEBSITE: &str = "https://neuroskill.com";
pub const APP_WEBSITE_LABEL: &str = "neuroskill.com";
pub const APP_REPO_URL: &str = "https://github.com/NeuroSkill-com/skill";
pub const APP_DISCORD_URL: &str = "https://discord.gg/nA6Xk5MV";
pub const APP_LICENSE: &str = "GPL-3.0-only";
pub const APP_LICENSE_NAME: &str = "GNU General Public License v3";
pub const APP_LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";
pub const APP_COPYRIGHT: &str = "© 2025–2026 NeuroSkill.com";

/// Ordered list of contributors.
pub const APP_AUTHORS: &[(&str, &str)] = &[
    ("Eugene Hauptmann",    "Lead developer & EEG signal processing"),
    ("Nataliya Kosmyna",    "Neuroscience and Brain Computer Interfaces"),
];

pub const APP_ACKNOWLEDGEMENTS: &str =
    "Built with Tauri, SvelteKit, and the ZUNA EEG foundation model by Zyphra. \
     EEG band-power research based on work by Klimesch (1999), \
     Pope et al. (1995), and Kosmyna & Maes (2019).";

// ── Skill data directory ──────────────────────────────────────────────────────

/// The skill data directory name used on macOS and Linux (`~/.skill`).
#[cfg(not(target_os = "windows"))]
pub const SKILL_DIR: &str = ".skill";
