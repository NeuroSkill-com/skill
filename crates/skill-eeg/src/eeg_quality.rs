// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/// Per-channel EEG signal quality assessment.
///
/// Quality is computed from a 256-sample (1 second) rolling window of **raw**
/// (unfiltered) samples using two amplitude-based statistics:
///
/// | Metric | What it measures |
/// |---|---|
/// | RMS | Overall signal amplitude |
/// | Clip count | Samples near the ADC rail (sustained saturation) |
///
/// # Why not mean|first-difference|?
///
/// A first-difference metric amplifies high-frequency content and is therefore
/// dominated by the powerline noise (50/60 Hz) and scalp EMG present in every
/// unfiltered Muse signal.  100 µV of 60 Hz PLN alone produces
/// ≈ 85 µV mean|Δ| — above any reasonable EMG threshold — making the metric
/// environment-dependent and impossible to calibrate to a single value.
/// Amplitude alone is stable and directly interpretable.
///
/// # Classification (Muse S, 256 Hz, ±1500 µV ADC range)
///
/// | Class | Condition |
/// |---|---|
/// | `NoSignal` | RMS < 5 µV — electrode not in contact |
/// | `Poor` | RMS > 400 µV OR ≥ 8 clips @ 1200 µV — sustained saturation or gross movement |
/// | `Fair` | RMS > 100 µV — noticeable artifact or poor contact |
/// | `Good` | RMS in 5–100 µV — normal EEG range |
///
/// Quality is only reported after at least 64 samples (250 ms) have been
/// collected, returning `NoSignal` during the warm-up period.
use serde::Serialize;
use std::collections::VecDeque;

use skill_constants::{
    QUALITY_WINDOW, QUALITY_NO_SIGNAL_RMS, QUALITY_POOR_RMS,
    QUALITY_CLIP_UV, QUALITY_POOR_CLIPS, QUALITY_FAIR_RMS,
};

// ── Thresholds (from skill-constants) ─────────────────────────────────────────

const WINDOW: usize            = QUALITY_WINDOW;
#[cfg(test)]
const MIN_SAMPLES: usize       = WINDOW / 4;
const THRESH_NO_SIGNAL_RMS: f64 = QUALITY_NO_SIGNAL_RMS;
const THRESH_POOR_RMS: f64     = QUALITY_POOR_RMS;
const THRESH_CLIP_UV: f64      = QUALITY_CLIP_UV;
const THRESH_POOR_CLIPS: usize = QUALITY_POOR_CLIPS;
const THRESH_FAIR_RMS: f64     = QUALITY_FAIR_RMS;

// ── SignalQuality ─────────────────────────────────────────────────────────────

/// Quality classification for a single EEG channel over a 1-second window.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SignalQuality {
    Good,
    Fair,
    Poor,
    #[default]
    NoSignal,
}

// ── QualityMonitor ────────────────────────────────────────────────────────────

/// Maintains a rolling window of raw EEG samples per channel and computes
/// [`SignalQuality`] on demand.
pub struct QualityMonitor {
    bufs: Vec<VecDeque<f64>>,
    /// Window size in samples (≈1 second at the device sample rate).
    window: usize,
    /// Minimum samples before quality is reported.
    min_samples: usize,
}

impl QualityMonitor {
    /// Create a quality monitor with the default window (256 samples ≈ 1 s @ 256 Hz).
    ///
    /// **Deprecated:** defaults to 256-sample window (Muse 256 Hz).
    /// Use [`with_window(channels, sample_rate as usize)`] for a 1-second
    /// window at any device sample rate.
    #[deprecated(since = "0.1.0", note = "use QualityMonitor::with_window(channels, sample_rate as usize) instead")]
    pub fn new(channels: usize) -> Self {
        Self::with_window(channels, WINDOW)
    }

    /// Create a quality monitor with a custom window size.
    ///
    /// Use `sample_rate as usize` for a 1-second window at any sample rate.
    pub fn with_window(channels: usize, window: usize) -> Self {
        let window = window.max(32); // safety floor
        Self {
            bufs: (0..channels)
                .map(|_| VecDeque::with_capacity(window + 1))
                .collect(),
            window,
            min_samples: window / 4,
        }
    }

    /// Append `samples` for `channel` to the rolling window, discarding the
    /// oldest samples when the buffer exceeds the window size.
    pub fn push(&mut self, channel: usize, samples: &[f64]) {
        if channel >= self.bufs.len() {
            return;
        }
        let buf = &mut self.bufs[channel];
        for &s in samples {
            buf.push_back(s);
        }
        while buf.len() > self.window {
            buf.pop_front();
        }
    }

    /// Compute quality for all channels and return one [`SignalQuality`] per
    /// channel in order.
    pub fn all_qualities(&self) -> Vec<SignalQuality> {
        (0..self.bufs.len()).map(|ch| self.classify(ch)).collect()
    }

    /// Clear all rolling windows, resetting every channel to `NoSignal`.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        for buf in &mut self.bufs {
            buf.clear();
        }
    }

    // ── private ───────────────────────────────────────────────────────────────

    fn classify(&self, channel: usize) -> SignalQuality {
        let buf = &self.bufs[channel];

        if buf.len() < self.min_samples {
            return SignalQuality::NoSignal;
        }

        let n = buf.len() as f64;

        // ── AC-coupled RMS ─────────────────────────────────────────────────
        // Subtract the mean (DC offset) before computing RMS.  Emotiv and
        // other DC-coupled devices report EEG with a large baseline offset
        // (e.g. ~4200 µV).  Without DC removal, the RMS is dominated by the
        // offset and every channel reads as "Poor".  For Muse (AC-coupled,
        // mean ≈ 0) this is a no-op.
        let mean = buf.iter().sum::<f64>() / n;
        let rms = (buf.iter().map(|&x| { let ac = x - mean; ac * ac }).sum::<f64>() / n).sqrt();

        if rms < THRESH_NO_SIGNAL_RMS {
            return SignalQuality::NoSignal;
        }

        // ── Clip count (AC-coupled — subtract mean so DC-coupled devices
        //    like Emotiv don't trigger false clips from their baseline) ──
        let clips = buf.iter().filter(|&&x| (x - mean).abs() > THRESH_CLIP_UV).count();

        if clips >= THRESH_POOR_CLIPS || rms > THRESH_POOR_RMS {
            return SignalQuality::Poor;
        }

        if rms > THRESH_FAIR_RMS {
            return SignalQuality::Fair;
        }

        SignalQuality::Good
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    fn monitor() -> QualityMonitor { QualityMonitor::new(1) }
    fn fill(m: &mut QualityMonitor, s: &[f64]) { m.push(0, s); }
    fn q(m: &QualityMonitor) -> SignalQuality { m.classify(0) }

    #[test]
    fn warm_up_returns_no_signal() {
        assert_eq!(q(&monitor()), SignalQuality::NoSignal);
    }

    #[test]
    fn insufficient_samples_returns_no_signal() {
        let mut m = monitor();
        fill(&mut m, &vec![20.0; MIN_SAMPLES - 1]);
        assert_eq!(q(&m), SignalQuality::NoSignal);
    }

    #[test]
    fn flat_low_amplitude_is_no_signal() {
        let mut m = monitor();
        // Constant 2 µV — AC RMS = 0 after DC removal → NoSignal
        fill(&mut m, &vec![2.0; WINDOW]);
        assert_eq!(q(&m), SignalQuality::NoSignal);
    }

    #[test]
    fn dc_offset_with_small_ac_is_good() {
        let mut m = monitor();
        // 4200 µV DC offset + 30 µV sine — simulates Emotiv Insight.
        // AC RMS ≈ 21 µV → Good (DC offset is removed).
        let samples: Vec<f64> = (0..WINDOW)
            .map(|i| 4200.0 + 30.0 * (2.0 * std::f64::consts::PI * 10.0 * i as f64 / 256.0).sin())
            .collect();
        fill(&mut m, &samples);
        assert_eq!(q(&m), SignalQuality::Good);
    }

    #[test]
    fn normal_eeg_is_good() {
        let mut m = monitor();
        // 30 µV sine @ 10 Hz — typical alpha amplitude, well within 5–100 µV
        let samples: Vec<f64> = (0..WINDOW)
            .map(|i| 30.0 * (2.0 * std::f64::consts::PI * 10.0 * i as f64 / 256.0).sin())
            .collect();
        fill(&mut m, &samples);
        assert_eq!(q(&m), SignalQuality::Good);
    }

    #[test]
    fn sixty_hz_pln_with_eeg_is_good() {
        let mut m = monitor();
        // 40 µV EEG + 80 µV PLN — total RMS ≈ 89 µV, still below THRESH_FAIR_RMS=100
        let samples: Vec<f64> = (0..WINDOW)
            .map(|i| {
                let t = i as f64 / 256.0;
                40.0 * (2.0 * std::f64::consts::PI * 10.0 * t).sin()
                    + 80.0 * (2.0 * std::f64::consts::PI * 60.0 * t).sin()
            })
            .collect();
        let actual_rms = (samples.iter().map(|x| x*x).sum::<f64>() / WINDOW as f64).sqrt();
        assert!(actual_rms < THRESH_FAIR_RMS, "rms={actual_rms:.1}");
        fill(&mut m, &samples);
        assert_eq!(q(&m), SignalQuality::Good);
    }

    #[test]
    fn elevated_rms_is_fair() {
        let mut m = monitor();
        // 170 µV sine → AC RMS ≈ 120 µV — above THRESH_FAIR_RMS=100, below THRESH_POOR_RMS=400
        let samples: Vec<f64> = (0..WINDOW)
            .map(|i| 170.0 * (2.0 * std::f64::consts::PI * 10.0 * i as f64 / 256.0).sin())
            .collect();
        fill(&mut m, &samples);
        assert_eq!(q(&m), SignalQuality::Fair);
    }

    #[test]
    fn high_rms_is_poor() {
        let mut m = monitor();
        // 640 µV sine → AC RMS ≈ 450 µV — above THRESH_POOR_RMS=400
        let samples: Vec<f64> = (0..WINDOW)
            .map(|i| 640.0 * (2.0 * std::f64::consts::PI * 10.0 * i as f64 / 256.0).sin())
            .collect();
        fill(&mut m, &samples);
        assert_eq!(q(&m), SignalQuality::Poor);
    }

    #[test]
    fn clipping_is_poor() {
        let mut m = monitor();
        let mut samples = vec![30.0; WINDOW];
        // 8 clips at ±1300 µV (≥ THRESH_POOR_CLIPS=8, > THRESH_CLIP_UV=1200)
        for i in 0..8 {
            samples[i * 20] = if i % 2 == 0 { 1300.0 } else { -1300.0 };
        }
        fill(&mut m, &samples);
        assert_eq!(q(&m), SignalQuality::Poor);
    }

    /// Helper: 30 µV sine at 10 Hz — AC RMS ≈ 21 µV (Good).
    fn good_signal() -> Vec<f64> {
        (0..WINDOW)
            .map(|i| 30.0 * (2.0 * std::f64::consts::PI * 10.0 * i as f64 / 256.0).sin())
            .collect()
    }

    #[test]
    fn reset_clears_window() {
        let mut m = monitor();
        fill(&mut m, &good_signal());
        assert_eq!(q(&m), SignalQuality::Good);
        m.reset();
        assert_eq!(q(&m), SignalQuality::NoSignal);
    }

    #[test]
    fn window_is_rolling() {
        let mut m = monitor();
        fill(&mut m, &good_signal());
        assert_eq!(q(&m), SignalQuality::Good);
        // 640 µV sine → AC RMS ≈ 452 µV — above THRESH_POOR_RMS=400
        let poor: Vec<f64> = (0..WINDOW)
            .map(|i| 640.0 * (2.0 * std::f64::consts::PI * 10.0 * i as f64 / 256.0).sin())
            .collect();
        fill(&mut m, &poor);    // evicts the good data
        assert_eq!(q(&m), SignalQuality::Poor);
    }
}
