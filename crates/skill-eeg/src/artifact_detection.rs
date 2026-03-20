// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Artifact detection — blink detection on frontal EEG electrodes.
//!
//! The detector resolves frontal electrodes by name (10-20 system) so it
//! works across all supported devices, not just Muse.
//!
//! Blink detection uses a threshold-based approach on frontal channels:
//! a blink is registered when the absolute amplitude exceeds a running
//! baseline multiplied by a gain factor, with a refractory period to
//! prevent double-counting.

use std::collections::VecDeque;
use skill_constants::{
    MUSE_SAMPLE_RATE, BLINK_THRESHOLD_UV, BLINK_REFRACTORY_S, BLINK_RATE_WINDOW_S,
};

// ── Public metrics ────────────────────────────────────────────────────────────

/// Snapshot of artifact detection metrics, emitted alongside band snapshots.
#[derive(Clone, Debug, Default)]
pub struct ArtifactMetrics {
    /// Total blink count since connection.
    pub blink_count: u64,
    /// Blinks per minute (rolling 60-second window).
    pub blink_rate: f64,
}

// ── Artifact Detector ─────────────────────────────────────────────────────────

pub struct ArtifactDetector {
    // ── Blink state ──────────────────────────────────────────────────────────
    /// Per-frontal-channel running baseline (EMA of absolute amplitude).
    blink_baseline: [f64; 2],
    /// Samples since last blink (per channel), for refractory.
    blink_refractory: [usize; 2],
    /// Whether we are currently inside a blink spike (per channel).
    in_blink: [bool; 2],
    /// Total blink count.
    blink_count: u64,
    /// Timestamps (sample count) of recent blinks for rate calculation.
    blink_times: VecDeque<u64>,
    /// Global sample counter.
    sample_count: u64,
    /// Hardware sample rate (Hz).
    sr: f64,
    /// Channel indices that correspond to frontal electrodes suitable for
    /// blink detection.  Resolved at construction from channel names.
    /// At most 2 entries (left-frontal, right-frontal).
    frontal_indices: Vec<usize>,
}

impl ArtifactDetector {
    /// Create a detector using the default Muse layout (AF7=1, AF8=2 @ 256 Hz).
    ///
    /// **Deprecated:** defaults to 256 Hz and Muse electrode layout.
    /// Use [`with_channels`] with the device's actual sample rate and channel
    /// names for correct blink detection on non-Muse devices.
    #[deprecated(since = "0.1.0", note = "use ArtifactDetector::with_channels(sample_rate, channel_names) instead")]
    pub fn new() -> Self {
        Self::with_channels(MUSE_SAMPLE_RATE as f64, &["TP9", "AF7", "AF8", "TP10"])
    }

    /// Create a detector that resolves frontal electrodes from `channel_names`.
    ///
    /// Picks up to 2 frontal electrodes (preferring AF7/AF8, then Fp1/Fp2,
    /// then any AF/F-prefixed pair) for blink detection.
    pub fn with_channels(sample_rate: f64, channel_names: &[&str]) -> Self {
        // Preferred frontal electrodes for blink detection, in priority order.
        const PREFERRED: &[&str] = &[
            "AF7", "AF8", "Fp1", "Fp2", "AF3", "AF4",
            "F7", "F8", "F3", "F4",
        ];

        let mut frontal_indices = Vec::new();
        for &pref in PREFERRED {
            if frontal_indices.len() >= 2 { break; }
            if let Some(idx) = channel_names.iter().position(|&n| n == pref) {
                frontal_indices.push(idx);
            }
        }

        Self {
            blink_baseline:    [20.0; 2],
            blink_refractory:  [0; 2],
            in_blink:          [false; 2],
            blink_count:       0,
            blink_times:       VecDeque::new(),
            sample_count:      0,
            sr: sample_rate,
            frontal_indices,
        }
    }

    /// Feed raw EEG samples for one channel.
    ///
    /// Blink detection is only performed on channels identified as frontal
    /// during construction.
    pub fn push(&mut self, electrode: usize, samples: &[f64]) {
        if let Some(slot) = self.frontal_indices.iter().position(|&idx| idx == electrode) {
            if slot < 2 {
                self.push_frontal(slot, samples);
            }
        }
    }

    /// Get current artifact metrics.
    pub fn metrics(&self) -> ArtifactMetrics {
        let rate_window = (BLINK_RATE_WINDOW_S * self.sr) as u64;
        let blink_rate = if self.sample_count > 0 {
            let cutoff = self.sample_count.saturating_sub(rate_window);
            let recent = self.blink_times.iter().filter(|&&t| t >= cutoff).count();
            recent as f64 * 60.0 / BLINK_RATE_WINDOW_S.min(self.sample_count as f64 / self.sr)
        } else { 0.0 };

        ArtifactMetrics {
            blink_count: self.blink_count,
            blink_rate: (blink_rate * 10.0).round() / 10.0,
        }
    }

    /// Reset all state to initial values (as if `new()` had just been called).
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.blink_baseline   = [20.0; 2];
        self.blink_refractory = [0; 2];
        self.in_blink         = [false; 2];
        self.blink_count      = 0;
        self.blink_times.clear();
        self.sample_count     = 0;
    }

    // ── Blink detection (frontal channels) ───────────────────────────────────

    fn push_frontal(&mut self, idx: usize, samples: &[f64]) {
        let refractory_samples = (BLINK_REFRACTORY_S * self.sr) as usize;

        for &v in samples {
            self.sample_count += if idx == 0 { 1 } else { 0 }; // count once per sample pair
            let abs_v = v.abs();

            // Update baseline with slow EMA (τ ≈ 2 s).
            let alpha = 1.0 / (self.sr * 2.0);
            self.blink_baseline[idx] += alpha * (abs_v - self.blink_baseline[idx]);

            // Threshold: fixed minimum OR 4× running baseline, whichever is larger.
            let threshold = BLINK_THRESHOLD_UV.max(self.blink_baseline[idx] * 4.0);

            if self.blink_refractory[idx] > 0 {
                self.blink_refractory[idx] -= 1;
                self.in_blink[idx] = false;
                continue;
            }

            if abs_v > threshold {
                if !self.in_blink[idx] {
                    // Rising edge — new blink detected.
                    self.in_blink[idx] = true;
                    self.blink_count += 1;
                    self.blink_refractory[idx] = refractory_samples;
                    self.blink_times.push_back(self.sample_count);
                    // Prune old timestamps.
                    let cutoff = self.sample_count.saturating_sub(
                        (BLINK_RATE_WINDOW_S * self.sr) as u64 + self.sr as u64
                    );
                    while self.blink_times.front().is_some_and(|&t| t < cutoff) {
                        self.blink_times.pop_front();
                    }
                }
            } else {
                self.in_blink[idx] = false;
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn default_detector() -> ArtifactDetector { ArtifactDetector::new() }
    fn muse_sr() -> f64 { MUSE_SAMPLE_RATE as f64 }

    /// Simulate a blink: spike to `peak_uv` for `n_samples` then back to quiet.
    fn blink_sequence(peak_uv: f64, n_samples: usize) -> Vec<f64> {
        let mut v = vec![5.0; 256]; // 1 s quiet warm-up
        for _ in 0..n_samples { v.push(peak_uv); }
        v.extend(vec![5.0; 64]); // settle
        v
    }

    // ── Basic blink detection ───────────────────────────────────────────────

    #[test]
    fn detects_single_blink_on_frontal_channel() {
        let mut det = default_detector();
        let seq = blink_sequence(200.0, 8);
        // AF7 = electrode 1 on Muse
        for &v in &seq {
            det.push(1, &[v]);
        }
        assert!(det.metrics().blink_count >= 1, "expected at least 1 blink, got {}", det.metrics().blink_count);
    }

    #[test]
    fn no_blink_on_quiet_signal() {
        let mut det = default_detector();
        let quiet: Vec<f64> = (0..256).map(|i| 5.0 * (i as f64 * 0.05).sin()).collect();
        det.push(1, &quiet);
        assert_eq!(det.metrics().blink_count, 0);
    }

    #[test]
    fn ignores_non_frontal_channels() {
        let mut det = default_detector();
        // TP9 = electrode 0 and TP10 = electrode 3 should be ignored.
        let seq = blink_sequence(300.0, 8);
        det.push(0, &seq);
        det.push(3, &seq);
        assert_eq!(det.metrics().blink_count, 0);
    }

    // ── Refractory period ───────────────────────────────────────────────────

    #[test]
    fn refractory_prevents_double_count() {
        let mut det = default_detector();
        // Two spikes 10 ms apart should count as one blink.
        let mut seq: Vec<f64> = vec![5.0; 256]; // warm-up
        seq.extend(vec![200.0; 8]); // first spike
        seq.extend(vec![5.0; 3]);   // tiny gap (< refractory)
        seq.extend(vec![200.0; 8]); // second spike within refractory
        seq.extend(vec![5.0; 64]);  // settle
        det.push(1, &seq);
        assert_eq!(det.metrics().blink_count, 1, "double-counted");
    }

    // ── Blink rate ──────────────────────────────────────────────────────────

    #[test]
    fn blink_rate_plausible() {
        let mut det = default_detector();
        let sr = muse_sr();
        let refractory = (BLINK_REFRACTORY_S * sr) as usize;
        // Warm-up
        det.push(1, &vec![5.0; 256]);
        // Inject 10 blinks with enough spacing
        for _ in 0..10 {
            det.push(1, &vec![200.0; 8]);
            det.push(1, &vec![5.0; refractory + 32]);
        }
        let rate = det.metrics().blink_rate;
        assert!(rate > 0.0, "blink rate should be positive, got {rate}");
    }

    // ── with_channels ───────────────────────────────────────────────────────

    #[test]
    fn with_channels_finds_frontal_electrodes() {
        // Emotiv EPOC layout: AF3=0, F7=1, ..., AF4=13
        let names = &["AF3","F7","F3","FC5","T7","P7","O1","O2","P8","T8","FC6","F4","F8","AF4"];
        let det = ArtifactDetector::with_channels(128.0, names);
        // Should pick AF3 (index 0) and AF4 (index 13) — but AF4 is not in
        // the top-priority list; AF3 is first, then F7 second.
        // Actually the priority list is: AF7, AF8, Fp1, Fp2, AF3, AF4, F7, F8, F3, F4
        // AF7/AF8 not present → Fp1/Fp2 not present → AF3 (idx 0) + AF4 (idx 13)
        assert_eq!(det.frontal_indices, vec![0, 13]);
    }

    #[test]
    fn with_channels_mw75_finds_frontal() {
        // MW75 has no AF/Fp electrodes — should pick FT7 and FT8 via fallback.
        // But FT7/FT8 are not in the preferred list. No frontal electrodes.
        let names = &["FT7","T7","TP7","CP5","P7","C5","FT8","T8","TP8","CP6","P8","C6"];
        let det = ArtifactDetector::with_channels(500.0, names);
        assert!(det.frontal_indices.is_empty(),
            "MW75 has no standard frontal electrodes; got {:?}", det.frontal_indices);
    }

    #[test]
    fn with_channels_hermes_finds_frontal() {
        let names = &["Fp1","Fp2","AF3","AF4","F3","F4","FC1","FC2"];
        let det = ArtifactDetector::with_channels(250.0, names);
        // Should pick Fp1 (idx 0) and Fp2 (idx 1)
        assert_eq!(det.frontal_indices, vec![0, 1]);
    }

    #[test]
    fn with_channels_idun_no_frontal() {
        let names = &["EEG"];
        let det = ArtifactDetector::with_channels(250.0, names);
        assert!(det.frontal_indices.is_empty());
    }

    #[test]
    fn blink_on_custom_channels() {
        // Hermes: Fp1=0, Fp2=1
        let names = &["Fp1","Fp2","AF3","AF4","F3","F4","FC1","FC2"];
        let mut det = ArtifactDetector::with_channels(250.0, names);
        let seq = blink_sequence(200.0, 8);
        det.push(0, &seq); // Fp1
        assert!(det.metrics().blink_count >= 1);
    }
}
