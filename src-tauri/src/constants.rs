// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Single source of truth for every numeric and string constant shared across
//! the EEG processing pipeline.
//!
//! All signal-processing constants here must stay in sync with their
//! TypeScript mirrors in `src/lib/constants.ts`.

// ── Onboarding ───────────────────────────────────────────────────────────────

/// Canonical staged model-download order used by onboarding.
///
/// The frontend queries this at runtime so the sequence is configured from one
/// backend source of truth.
pub const ONBOARDING_MODEL_DOWNLOAD_ORDER: [&str; 5] = [
    "zuna",
    "kitten",
    "neutts",
    "llm",
    "ocr",
];

// ── EEG constants (canonical definitions in skill-eeg crate) ──────────────────
//
// Re-exported here so the rest of the main crate can keep using
// `crate::constants::EEG_CHANNELS` etc. unchanged.
#[allow(unused_imports)]
pub use skill_eeg::constants::{
    // Hardware
    EEG_CHANNELS, CHANNEL_NAMES, MUSE_SAMPLE_RATE,
    // Signal filter
    FILTER_WINDOW, FILTER_HOP, FILTER_OVERLAP,
    // Filter defaults
    DEFAULT_LP_HZ, DEFAULT_HP_HZ, DEFAULT_NOTCH_BW_HZ,
    // Spectrogram
    SPEC_N_FREQ,
    // Band analysis
    BAND_WINDOW, BAND_HOP, NUM_BANDS, BANDS, BAND_COLORS, BAND_SYMBOLS,
    // EEG embedding / ZUNA model
    EMBEDDING_EPOCH_SECS, EMBEDDING_EPOCH_SAMPLES,
    EMBEDDING_OVERLAP_SECS, EMBEDDING_OVERLAP_MIN_SECS, EMBEDDING_OVERLAP_MAX_SECS,
    EMBEDDING_OVERLAP_SAMPLES, EMBEDDING_HOP_SAMPLES,
    ZUNA_DATA_NORM, ZUNA_HF_REPO, ZUNA_WEIGHTS_FILE, ZUNA_CONFIG_FILE,
    // HNSW
    HNSW_M, HNSW_EF_CONSTRUCTION,
    // Model config
    MODEL_CONFIG_FILE,
};

/// OpenBCI Ganglion channel labels (default 10-20 sites when unset).
pub const GANGLION_CHANNEL_NAMES: [&str; 4] = ["Ch1", "Ch2", "Ch3", "Ch4"];

/// Filename of the UMAP projection configuration.
pub const UMAP_CONFIG_FILE: &str = "umap_config.json";

/// SQLite database that stores user-authored labels (`~/.skill/labels.sqlite`).
pub const LABELS_FILE: &str = "labels.sqlite";

// ── Screenshot capture + vision embedding ─────────────────────────────────────

/// Directory name under `~/.skill/` for captured screenshot images.
pub const SCREENSHOTS_DIR: &str = "screenshots";

/// SQLite database for screenshot metadata and embedding blobs.
pub const SCREENSHOTS_SQLITE: &str = "screenshots.sqlite";

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

/// Default label for the first calibration action.
pub const CALIBRATION_ACTION1_LABEL: &str = "Eyes Open";

/// Default label for the second calibration action.
pub const CALIBRATION_ACTION2_LABEL: &str = "Eyes Closed";

/// Default duration of each calibration action (seconds).
pub const CALIBRATION_ACTION_DURATION_SECS: u32 = 10;

/// Default duration of the break between actions (seconds).
pub const CALIBRATION_BREAK_DURATION_SECS: u32 = 5;

/// Default number of action1→break→action2 loop iterations.
pub const CALIBRATION_LOOP_COUNT: u32 = 3;

/// Whether to automatically open the calibration window on app startup.
pub const CALIBRATION_AUTO_START: bool = true;

/// Filename of all user-configured app settings (`~/.skill/settings.json`).
/// Replaces the old Tauri app-data `data.json`; all settings now live under
/// the skill data directory so they are easy to inspect and back up.
pub const SETTINGS_FILE: &str = "settings.json";

/// Filename of the per-subsystem logging configuration (`~/.skill/log_config.json`).
pub const LOG_CONFIG_FILE: &str = "log_config.json";

/// Filename of the persisted HNSW embedding index inside each daily folder
/// (`~/.skill/YYYYMMDD/eeg_embeddings.hnsw`).
pub const HNSW_INDEX_FILE: &str = "eeg_embeddings.hnsw";

/// Filename of the persistent cross-day HNSW index stored directly in the
/// skill data directory (`~/.skill/eeg_global.hnsw`).
///
/// This index accumulates every EEG embedding from all recording days into a
/// single flat file.  The HNSW payload is the `YYYYMMDDHHmmss` timestamp
/// (i64), which encodes the date so the matching per-day SQLite can be found
/// during result hydration.
///
/// The file is built from scratch at startup if absent, updated incrementally
/// by the embed worker after each epoch, and saved every
/// `GLOBAL_HNSW_SAVE_EVERY` insertions.
pub const GLOBAL_HNSW_FILE: &str = "eeg_global.hnsw";

/// Number of new embeddings added to the global index between periodic disk
/// saves.  Balances write amplification against data loss on crash.
pub const GLOBAL_HNSW_SAVE_EVERY: usize = 10;

/// Filename of the SQLite database inside each daily folder
/// (`~/.skill/YYYYMMDD/eeg.sqlite`).
///
/// Schema — `embeddings` table:
/// ```text
/// id              INTEGER PRIMARY KEY AUTOINCREMENT
/// timestamp       INTEGER NOT NULL   -- YYYYMMDDHHmmss (UTC)
/// device_id       TEXT               -- BLE device id (nullable)
/// device_name     TEXT               -- headset name  (nullable)
/// hnsw_id         INTEGER NOT NULL   -- row index in the daily HNSW file
/// eeg_embedding   BLOB    NOT NULL   -- f32 LE × dim  (default 32 floats = 128 bytes)
/// label           TEXT               -- user-defined tag (nullable)
/// extra_embedding BLOB               -- reserved second embedding (nullable)
/// ```
pub const SQLITE_FILE: &str = "eeg.sqlite";

/// Activity tracking database — active windows + input-activity samples.
pub const ACTIVITY_FILE: &str = "activity.sqlite";

/// Hooks audit-log database — persists every hook trigger with full context.
pub const HOOKS_LOG_FILE: &str = "hooks.sqlite";

// ── WebSocket server ──────────────────────────────────────────────────────────

/// Capacity of the `tokio::sync::broadcast` channel used to fan messages out
/// to all connected WebSocket clients.  Old messages are dropped (lagged) when
/// a slow client's slot count exceeds this value.
pub const WS_BROADCAST_CAPACITY: usize = 512;

// ── mDNS / Bonjour ────────────────────────────────────────────────────────────

/// DNS-SD service-type suffix appended to the lowercased app name.
///
/// Full service type: `format!("_{}{}", app_name, MDNS_SERVICE_SUFFIX)`  
/// Example: `_skill._tcp.local.`
pub const MDNS_SERVICE_SUFFIX: &str = "._tcp.local.";

/// Suffix appended to the lowercased app name to form the mDNS host name.
///
/// Example: `skill.local.`
pub const MDNS_HOST_SUFFIX: &str = ".local.";

/// `version` field broadcast in the DNS-SD TXT record.
pub const MDNS_TXT_VERSION: &str = "1";

/// `format` field broadcast in the DNS-SD TXT record (payload encoding).
pub const MDNS_TXT_FORMAT: &str = "json";/// Interval between automatic background update checks (seconds).
/// Set to 0 to disable automatic checking (manual only via Settings).
pub const UPDATER_CHECK_INTERVAL_SECS: u64 = 3600; // 1 hour

pub const WS_HOST:         &str = "127.0.0.1";
pub const WS_DEFAULT_PORT: u16  = 8375;

/// Reverse-DNS prefix used as the macOS LaunchAgent label and plist filename.
/// Results in e.g. `com.neuroskill.skill.loginitem.plist`.
#[cfg(target_os = "macos")]
pub const AUTOSTART_PLIST_LABEL_PREFIX: &str = "com.neuroskill";

// ── Application identity & credits ───────────────────────────────────────────

/// Human-readable application name shown in About dialogs and window titles.
pub const APP_DISPLAY_NAME: &str = "NeuroSkill™";

/// One-line description of what the app does.
pub const APP_TAGLINE: &str =
    "Real-time EXG State of Mind system and brain-state monitoring for Muse, OpenBCI, and other BCI devices.";

/// Public website URL — shown in the About window and the native About dialog.
pub const APP_WEBSITE: &str = "https://neuroskill.com";

/// Website display label (the human-readable text for the hyperlink).
pub const APP_WEBSITE_LABEL: &str = "neuroskill.com";

/// Source-code / repository URL.
pub const APP_REPO_URL: &str = "https://github.com/NeuroSkill-com/skill";

/// Discord community invite URL.
pub const APP_DISCORD_URL: &str = "https://discord.gg/nA6Xk5MV";

/// SPDX licence identifier for the application.
pub const APP_LICENSE: &str = "GPL-3.0-only";

/// Full licence name (used in UI copy).
pub const APP_LICENSE_NAME: &str = "GNU General Public License v3";

/// URL to the full licence text.
pub const APP_LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";

/// Copyright line shown in the native About dialog and the About window.
pub const APP_COPYRIGHT: &str = "© 2025–2026 NeuroSkill.com";

/// Ordered list of contributors.  Each entry is a `(display_name, role)` pair.
///
/// The first entry is treated as the primary author.
pub const APP_AUTHORS: &[(&str, &str)] = &[
    ("Eugene Hauptmann",    "Lead developer & EEG signal processing"),
    ("Nataliya Kosmyna",    "Neuroscience and Brain Computer Interfaces"),
];

/// Acknowledgements / third-party credits shown in the About window.
pub const APP_ACKNOWLEDGEMENTS: &str =
    "Built with Tauri, SvelteKit, and the ZUNA EEG foundation model by Zyphra. \
     EEG band-power research based on work by Klimesch (1999), \
     Pope et al. (1995), and Kosmyna & Maes (2019).";


// ── Skill data directory ──────────────────────────────────────────────────────

/// The skill data directory name used on macOS and Linux (`~/.skill`).
///
/// On **Windows** the data directory is `%LOCALAPPDATA%\NeuroSkill`
/// (`C:\Users\<user>\AppData\Local\NeuroSkill`) — see
/// [`crate::settings::default_skill_dir`].  The dot-prefix convention is not
/// idiomatic on Windows and `$HOME` is often unset there, so `AppData\Local`
/// is used instead.
///
/// Only used on macOS / Linux; excluded from Windows builds entirely.
#[cfg(not(target_os = "windows"))]
pub const SKILL_DIR: &str = ".skill";

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Derived constant correctness ──────────────────────────────────────────

    #[test]
    fn filter_overlap_equals_window_minus_hop() {
        assert_eq!(FILTER_OVERLAP, FILTER_WINDOW - FILTER_HOP);
    }

    #[test]
    fn filter_window_is_power_of_two() {
        assert!(FILTER_WINDOW.is_power_of_two(), "FILTER_WINDOW must be a power of two for FFT");
    }

    #[test]
    fn filter_hop_divides_filter_window() {
        assert_eq!(FILTER_WINDOW % FILTER_HOP, 0);
    }

    #[test]
    fn embedding_epoch_samples_correct() {
        // MUSE_SAMPLE_RATE × EMBEDDING_EPOCH_SECS = 256 × 5 = 1280
        let expected = (MUSE_SAMPLE_RATE as usize) * (EMBEDDING_EPOCH_SECS as usize);
        assert_eq!(EMBEDDING_EPOCH_SAMPLES, expected);
        assert_eq!(EMBEDDING_EPOCH_SAMPLES, 1280);
    }

    #[test]
    fn embedding_overlap_samples_correct() {
        // EMBEDDING_OVERLAP_SECS × MUSE_SAMPLE_RATE = 2.5 × 256 = 640
        let expected = (EMBEDDING_OVERLAP_SECS * MUSE_SAMPLE_RATE) as usize;
        assert_eq!(EMBEDDING_OVERLAP_SAMPLES, expected);
        assert_eq!(EMBEDDING_OVERLAP_SAMPLES, 640);
    }

    #[test]
    fn embedding_hop_samples_correct() {
        // EPOCH_SAMPLES − OVERLAP_SAMPLES = 1280 − 640 = 640
        assert_eq!(EMBEDDING_HOP_SAMPLES, EMBEDDING_EPOCH_SAMPLES - EMBEDDING_OVERLAP_SAMPLES);
        assert_eq!(EMBEDDING_HOP_SAMPLES, 640);
    }

    #[test]
    fn embedding_overlap_max_is_epoch_minus_half() {
        // EMBEDDING_EPOCH_SECS − 0.5 = 4.5
        assert!((EMBEDDING_OVERLAP_MAX_SECS - (EMBEDDING_EPOCH_SECS - 0.5)).abs() < 1e-6);
    }

    // ── Band table integrity ──────────────────────────────────────────────────

    #[test]
    fn num_bands_matches_all_band_arrays() {
        assert_eq!(BANDS.len(),        NUM_BANDS);
        assert_eq!(BAND_COLORS.len(),  NUM_BANDS);
        assert_eq!(BAND_SYMBOLS.len(), NUM_BANDS);
    }

    #[test]
    fn band_ranges_are_contiguous() {
        for i in 0..BANDS.len() - 1 {
            assert_eq!(
                BANDS[i].2, BANDS[i + 1].1,
                "band[{i}].hi ({}) != band[{}].lo ({})", BANDS[i].2, i + 1, BANDS[i + 1].1
            );
        }
    }

    #[test]
    fn delta_starts_at_0_5_hz() {
        assert!((BANDS[0].1 - 0.5).abs() < 1e-6, "delta lo = {}", BANDS[0].1);
    }

    #[test]
    fn high_gamma_ends_at_100_hz() {
        assert!((BANDS[NUM_BANDS - 1].2 - 100.0).abs() < 1e-6);
    }

    #[test]
    fn every_band_has_positive_width() {
        for (name, lo, hi) in &BANDS {
            assert!(hi > lo, "band '{name}': hi={hi} is not > lo={lo}");
        }
    }

    #[test]
    fn band_colors_start_with_hash_and_are_seven_chars() {
        for c in &BAND_COLORS {
            assert!(c.starts_with('#'), "color '{c}' does not start with '#'");
            assert_eq!(c.len(), 7, "color '{c}' is not 7 chars (#RRGGBB)");
        }
    }

    #[test]
    fn band_symbols_are_non_empty() {
        for sym in &BAND_SYMBOLS {
            assert!(!sym.is_empty(), "band symbol is empty");
        }
    }

    // ── Skill directory ───────────────────────────────────────────────────────

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn skill_dir_is_dot_skill() {
        assert_eq!(SKILL_DIR, ".skill");
    }

    // ── Hardware constants ────────────────────────────────────────────────────

    #[test]
    fn eeg_channels_is_four() {
        assert_eq!(EEG_CHANNELS, 4);
    }

    #[test]
    fn channel_names_match_eeg_channels() {
        assert_eq!(CHANNEL_NAMES.len(), EEG_CHANNELS);
    }

    #[test]
    fn channel_names_are_muse_sites() {
        assert_eq!(CHANNEL_NAMES, ["TP9", "AF7", "AF8", "TP10"]);
    }

    #[test]
    fn muse_sample_rate_is_256() {
        assert!((MUSE_SAMPLE_RATE - 256.0).abs() < 1e-6);
    }

    #[test]
    fn spec_n_freq_is_51() {
        // 0 Hz (DC) through 50 Hz inclusive at 1 Hz / bin = 51 bins
        assert_eq!(SPEC_N_FREQ, 51);
    }

    // ── Calibration defaults ──────────────────────────────────────────────────

    #[test]
    fn calibration_action1_label_is_eyes_open() {
        assert_eq!(CALIBRATION_ACTION1_LABEL, "Eyes Open");
    }

    #[test]
    fn calibration_action2_label_is_eyes_closed() {
        assert_eq!(CALIBRATION_ACTION2_LABEL, "Eyes Closed");
    }

    #[test]
    fn calibration_action_duration_is_10s() {
        assert_eq!(CALIBRATION_ACTION_DURATION_SECS, 10);
    }

    #[test]
    fn calibration_break_duration_is_5s() {
        assert_eq!(CALIBRATION_BREAK_DURATION_SECS, 5);
    }

    #[test]
    fn calibration_loop_count_is_3() {
        assert_eq!(CALIBRATION_LOOP_COUNT, 3);
    }

    #[test]
    fn calibration_auto_start_is_true() {
        const { assert!(CALIBRATION_AUTO_START); }
    }

    // ── WebSocket defaults ────────────────────────────────────────────────────

    #[test]
    fn ws_host_is_loopback() {
        assert_eq!(WS_HOST, "127.0.0.1");
    }

    #[test]
    fn ws_default_port_is_nonzero() {
        const { assert!(WS_DEFAULT_PORT > 0); }
    }

    // ── Band analysis window ──────────────────────────────────────────────────

    #[test]
    fn band_window_is_power_of_two() {
        assert!(BAND_WINDOW.is_power_of_two());
    }

    #[test]
    fn band_hop_divides_band_window() {
        assert_eq!(BAND_WINDOW % BAND_HOP, 0);
    }
}
