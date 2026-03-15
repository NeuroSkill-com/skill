// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Real-time artifact and event detection from raw EEG samples.
//!
//! Detects:
//! - **Eye blinks** from frontal channels (AF7, AF8) via amplitude spike detection.
//!
//! Uses simple, low-latency heuristics suitable for real-time display.
//!
//! # References
//! - Gratton, G. et al. (1983). A new method for off-line removal of ocular artifact.
//!   Electroencephalography and Clinical Neurophysiology, 55(4), 468–484.

use std::collections::VecDeque;

/// EEG sample rate (Hz).
const SR: f64 = 256.0;

/// Blink detection: minimum µV spike amplitude on AF7/AF8.
const BLINK_THRESHOLD_UV: f64 = 80.0;
/// Blink refractory period — minimum gap between consecutive blinks (seconds).
const BLINK_REFRACTORY_S: f64 = 0.3;
/// Blink rate window (seconds) — sliding window for blinks/min calculation.
const BLINK_RATE_WINDOW_S: f64 = 60.0;

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
    blink_baseline: [f64; 2],   // AF7, AF8
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
}

impl ArtifactDetector {
    pub fn new() -> Self {
        Self {
            blink_baseline:    [20.0; 2],
            blink_refractory:  [0; 2],
            in_blink:          [false; 2],
            blink_count:       0,
            blink_times:       VecDeque::new(),
            sample_count:      0,
        }
    }

    /// Feed raw EEG samples for one channel.
    /// `electrode`: 0=TP9, 1=AF7, 2=AF8, 3=TP10.
    pub fn push(&mut self, electrode: usize, samples: &[f64]) {
        match electrode {
            1 => self.push_frontal(0, samples), // AF7
            2 => self.push_frontal(1, samples), // AF8
            _ => {}
        }
    }

    /// Get current artifact metrics.
    pub fn metrics(&self) -> ArtifactMetrics {
        let rate_window = (BLINK_RATE_WINDOW_S * SR) as u64;
        let blink_rate = if self.sample_count > 0 {
            let cutoff = self.sample_count.saturating_sub(rate_window);
            let recent = self.blink_times.iter().filter(|&&t| t >= cutoff).count();
            recent as f64 * 60.0 / BLINK_RATE_WINDOW_S.min(self.sample_count as f64 / SR)
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
        let refractory_samples = (BLINK_REFRACTORY_S * SR) as usize;

        for &v in samples {
            self.sample_count += if idx == 0 { 1 } else { 0 }; // count once per sample pair
            let abs_v = v.abs();

            // Update baseline with slow EMA (τ ≈ 2 s).
            let alpha = 1.0 / (SR * 2.0);
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
                        (BLINK_RATE_WINDOW_S * SR) as u64 + 256
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blink_detected_on_large_spike() {
        let mut det = ArtifactDetector::new();
        // Feed 1 second of quiet baseline on AF7 (ch 1).
        let quiet: Vec<f64> = (0..256).map(|i| 5.0 * (i as f64 * 0.05).sin()).collect();
        det.push(1, &quiet);
        assert_eq!(det.metrics().blink_count, 0);

        // Insert a blink-like spike.
        let spike: Vec<f64> = (0..20).map(|i| {
            if i < 10 { 150.0 } else { -50.0 }
        }).collect();
        det.push(1, &spike);
        assert!(det.metrics().blink_count >= 1, "blink should be detected");
    }

    #[test]
    fn no_false_blink_on_quiet_signal() {
        let mut det = ArtifactDetector::new();
        let quiet: Vec<f64> = (0..512).map(|i| 3.0 * (i as f64 * 0.1).sin()).collect();
        det.push(1, &quiet);
        det.push(2, &quiet);
        assert_eq!(det.metrics().blink_count, 0);
    }

    #[test]
    fn non_frontal_channels_do_not_trigger_blink() {
        let mut det = ArtifactDetector::new();
        // Feed large spikes to TP9 (ch 0) and TP10 (ch 3) — should not count.
        let spike: Vec<f64> = vec![300.0; 30];
        det.push(0, &spike); // TP9 — ignored
        det.push(3, &spike); // TP10 — ignored
        assert_eq!(det.metrics().blink_count, 0);
    }

    #[test]
    fn refractory_period_prevents_double_count() {
        let mut det = ArtifactDetector::new();
        // Settle the baseline first.
        let quiet: Vec<f64> = (0..256).map(|i| 5.0 * (i as f64 * 0.05).sin()).collect();
        det.push(1, &quiet);

        // Two rapid spikes within the refractory window (0.3 s = 77 samples).
        let spike: Vec<f64> = (0..10).map(|_| 200.0).collect();
        det.push(1, &spike); // first blink
        // Immediately another spike (inside refractory period).
        det.push(1, &spike);
        // Only 1 blink should be registered.
        assert_eq!(det.metrics().blink_count, 1);
    }

    #[test]
    fn reset_clears_blink_count() {
        let mut det = ArtifactDetector::new();
        let quiet: Vec<f64> = (0..256).map(|i| 5.0 * (i as f64 * 0.05).sin()).collect();
        det.push(1, &quiet);
        let spike: Vec<f64> = vec![200.0; 10];
        det.push(1, &spike);
        assert!(det.metrics().blink_count >= 1);
        det.reset();
        assert_eq!(det.metrics().blink_count, 0);
    }

    #[test]
    fn blink_rate_is_zero_before_any_samples() {
        let det = ArtifactDetector::new();
        assert_eq!(det.metrics().blink_rate, 0.0);
    }

    #[test]
    fn both_frontal_channels_independently_detect_blinks() {
        // AF7 (ch1) and AF8 (ch2) are independent detectors.
        let quiet: Vec<f64> = (0..256).map(|i| 5.0 * (i as f64 * 0.05).sin()).collect();
        let spike: Vec<f64> = vec![200.0; 10];

        let mut det_ch1 = ArtifactDetector::new();
        det_ch1.push(1, &quiet);
        det_ch1.push(1, &spike);
        let count_ch1 = det_ch1.metrics().blink_count;

        let mut det_ch2 = ArtifactDetector::new();
        det_ch2.push(2, &quiet);
        det_ch2.push(2, &spike);
        let count_ch2 = det_ch2.metrics().blink_count;

        assert!(count_ch1 >= 1, "AF7 should detect blink");
        assert!(count_ch2 >= 1, "AF8 should detect blink");
    }
}
