// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Re-exports all constants from `skill-constants` so the rest of the crate
//! can keep using `crate::constants::*` unchanged.

#[allow(unused_imports)]
pub use skill_constants::*;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_overlap_equals_window_minus_hop() {
        assert_eq!(FILTER_OVERLAP, FILTER_WINDOW - FILTER_HOP);
    }

    #[test]
    fn filter_window_is_power_of_two() {
        assert!(FILTER_WINDOW.is_power_of_two());
    }

    #[test]
    fn filter_hop_divides_filter_window() {
        assert_eq!(FILTER_WINDOW % FILTER_HOP, 0);
    }

    #[test]
    fn embedding_epoch_samples_correct() {
        assert_eq!(EMBEDDING_EPOCH_SAMPLES, 1280);
    }

    #[test]
    fn embedding_overlap_samples_correct() {
        // EMBEDDING_OVERLAP_SECS is 0.0 → overlap = 0 samples
        assert_eq!(
            EMBEDDING_OVERLAP_SAMPLES,
            (EMBEDDING_OVERLAP_SECS * MUSE_SAMPLE_RATE) as usize
        );
    }

    #[test]
    fn embedding_hop_samples_correct() {
        // hop = epoch - overlap
        assert_eq!(
            EMBEDDING_HOP_SAMPLES,
            EMBEDDING_EPOCH_SAMPLES - EMBEDDING_OVERLAP_SAMPLES
        );
    }

    #[test]
    fn num_bands_matches_all_band_arrays() {
        assert_eq!(BANDS.len(), NUM_BANDS);
        assert_eq!(BAND_COLORS.len(), NUM_BANDS);
        assert_eq!(BAND_SYMBOLS.len(), NUM_BANDS);
    }

    #[test]
    fn band_ranges_are_contiguous() {
        for i in 0..BANDS.len() - 1 {
            assert_eq!(BANDS[i].2, BANDS[i + 1].1);
        }
    }

    #[test]
    fn eeg_channels_is_32() {
        assert_eq!(EEG_CHANNELS, 32);
    }

    #[test]
    fn channel_names_has_four_default_labels() {
        assert_eq!(CHANNEL_NAMES.len(), 4);
    }

    #[test]
    fn band_window_is_power_of_two() {
        assert!(BAND_WINDOW.is_power_of_two());
    }

    #[test]
    fn ws_host_is_loopback() {
        assert_eq!(WS_HOST, "127.0.0.1");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn skill_dir_is_dot_skill() {
        assert_eq!(SKILL_DIR, ".skill");
    }
}
