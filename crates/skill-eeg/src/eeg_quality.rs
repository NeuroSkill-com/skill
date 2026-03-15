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

// ── Thresholds ────────────────────────────────────────────────────────────────

/// Rolling window length in samples (1 second at 256 Hz).
const WINDOW: usize = 256;

/// Minimum samples required before making a quality judgement (~250 ms).
const MIN_SAMPLES: usize = WINDOW / 4;

/// RMS below this → electrode not in contact (µV).
const THRESH_NO_SIGNAL_RMS: f64 = 5.0;

/// RMS above this → gross movement or sustained saturation (µV).
const THRESH_POOR_RMS: f64 = 400.0;

/// Samples whose absolute value exceeds this are counted as clips (µV).
/// Set near (but below) the actual ±1500 µV ADC rail.
const THRESH_CLIP_UV: f64 = 1200.0;

/// Eight or more clips per window → Poor quality.
const THRESH_POOR_CLIPS: usize = 8;

/// RMS above this → noticeable artifact or poor contact / Fair (µV).
const THRESH_FAIR_RMS: f64 = 100.0;

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
}

impl QualityMonitor {
    pub fn new(channels: usize) -> Self {
        Self {
            bufs: (0..channels)
                .map(|_| VecDeque::with_capacity(WINDOW + 1))
                .collect(),
        }
    }

    /// Append `samples` for `channel` to the rolling window, discarding the
    /// oldest samples when the buffer exceeds [`WINDOW`].
    pub fn push(&mut self, channel: usize, samples: &[f64]) {
        if channel >= self.bufs.len() {
            return;
        }
        let buf = &mut self.bufs[channel];
        for &s in samples {
            buf.push_back(s);
        }
        while buf.len() > WINDOW {
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

        if buf.len() < MIN_SAMPLES {
            return SignalQuality::NoSignal;
        }

        let n = buf.len() as f64;

        // ── RMS ────────────────────────────────────────────────────────────
        let rms = (buf.iter().map(|&x| x * x).sum::<f64>() / n).sqrt();

        if rms < THRESH_NO_SIGNAL_RMS {
            return SignalQuality::NoSignal;
        }

        // ── Clip count ─────────────────────────────────────────────────────
        let clips = buf.iter().filter(|&&x| x.abs() > THRESH_CLIP_UV).count();

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
        fill(&mut m, &vec![2.0; WINDOW]);   // 2 µV < THRESH_NO_SIGNAL_RMS=5
        assert_eq!(q(&m), SignalQuality::NoSignal);
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
        // 120 µV constant — above THRESH_FAIR_RMS=100, below THRESH_POOR_RMS=400
        fill(&mut m, &vec![120.0; WINDOW]);
        assert_eq!(q(&m), SignalQuality::Fair);
    }

    #[test]
    fn high_rms_is_poor() {
        let mut m = monitor();
        // 450 µV constant — above THRESH_POOR_RMS=400
        fill(&mut m, &vec![450.0; WINDOW]);
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

    #[test]
    fn reset_clears_window() {
        let mut m = monitor();
        fill(&mut m, &vec![30.0; WINDOW]);
        assert_eq!(q(&m), SignalQuality::Good);
        m.reset();
        assert_eq!(q(&m), SignalQuality::NoSignal);
    }

    #[test]
    fn window_is_rolling() {
        let mut m = monitor();
        fill(&mut m, &vec![30.0; WINDOW]);
        assert_eq!(q(&m), SignalQuality::Good);
        fill(&mut m, &vec![450.0; WINDOW]);    // evicts the good data
        assert_eq!(q(&m), SignalQuality::Poor);
    }
}
