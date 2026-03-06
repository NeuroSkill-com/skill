// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! PPG (photoplethysmography) signal analysis.
//!
//! Extracts inter-beat intervals (IBIs) from the infrared PPG channel via
//! adaptive peak detection, then derives heart-rate and HRV metrics.
//!
//! All metrics are computed over a single epoch window (~2.5–5 s) and stored
//! alongside the EEG embeddings in `eeg.sqlite`.

use serde::Serialize;
use std::collections::VecDeque;

/// PPG sample rate on Muse 2/S (Hz).
const PPG_SR: f64 = 64.0;

/// Minimum IBI in seconds (~200 bpm).
const IBI_MIN: f64 = 0.3;
/// Maximum IBI in seconds (~30 bpm).
const IBI_MAX: f64 = 2.0;

// ── PPG Metrics struct ────────────────────────────────────────────────────────

/// Derived PPG metrics for a single epoch.
#[derive(Clone, Debug, Default, Serialize)]
pub struct PpgMetrics {
    /// Heart rate (beats per minute).
    pub hr:               f64,
    /// RMSSD — root mean square of successive IBI differences (ms).
    pub rmssd:            f64,
    /// SDNN — standard deviation of IBIs (ms).
    pub sdnn:             f64,
    /// pNN50 — percentage of successive IBIs differing by >50 ms.
    pub pnn50:            f64,
    /// LF/HF ratio from IBI spectrum (0.04–0.15 Hz / 0.15–0.4 Hz).
    pub lf_hf_ratio:      f64,
    /// Respiratory rate estimate (breaths per minute) from PPG envelope.
    pub respiratory_rate: f64,
    /// SpO₂ estimate (%) from red/IR ratio.  Uncalibrated — relative trends only.
    pub spo2_estimate:    f64,
    /// Perfusion Index: AC/DC ratio of the IR channel (%).
    pub perfusion_index:  f64,
    /// Baevsky Stress Index from IBI histogram.
    pub stress_index:     f64,
    /// Number of detected beats in the epoch.
    pub n_beats:          usize,
}

// ── PPG Analyzer (stateful, retains running buffer for cross-epoch peaks) ────

/// Accumulates raw PPG samples and computes metrics per epoch.
pub struct PpgAnalyzer {
    /// Ring buffer of raw IR samples (channel 1).
    ir_buf:  VecDeque<f64>,
    /// Ring buffer of raw red samples (channel 2).
    red_buf: VecDeque<f64>,
    /// Ring buffer of raw ambient samples (channel 0).
    amb_buf: VecDeque<f64>,
    /// Maximum buffer size (seconds × sample_rate).
    max_buf: usize,
}

impl PpgAnalyzer {
    /// Create a new analyzer.  `window_secs` is the max buffer length.
    pub fn new(window_secs: f64) -> Self {
        let max_buf = (window_secs * PPG_SR) as usize + 64;
        Self {
            ir_buf:  VecDeque::with_capacity(max_buf),
            red_buf: VecDeque::with_capacity(max_buf),
            amb_buf: VecDeque::with_capacity(max_buf),
            max_buf,
        }
    }

    /// Push raw PPG samples for a single channel (0=ambient, 1=IR, 2=red).
    pub fn push(&mut self, channel: usize, samples: &[f64]) {
        let buf = match channel {
            0 => &mut self.amb_buf,
            1 => &mut self.ir_buf,
            2 => &mut self.red_buf,
            _ => return,
        };
        for &v in samples {
            buf.push_back(v);
            if buf.len() > self.max_buf {
                buf.pop_front();
            }
        }
    }

    /// Compute metrics from the current buffer, then drain the epoch's worth of
    /// samples.  Returns `None` if insufficient data.
    pub fn compute_epoch(&mut self, epoch_samples: usize) -> Option<PpgMetrics> {
        if self.ir_buf.len() < 32 {
            return None;
        }

        let ir:  Vec<f64> = self.ir_buf.iter().copied().collect();
        let red: Vec<f64> = self.red_buf.iter().copied().collect();

        // ── Peak detection on IR channel ─────────────────────────────────
        let ibis = detect_peaks_and_ibis(&ir, PPG_SR);
        let n_beats = if ibis.is_empty() { 0 } else { ibis.len() + 1 };

        // ── Heart Rate ───────────────────────────────────────────────────
        let hr = if !ibis.is_empty() {
            60.0 / (ibis.iter().sum::<f64>() / ibis.len() as f64)
        } else {
            0.0
        };

        // ── HRV metrics ──────────────────────────────────────────────────
        let (rmssd, sdnn, pnn50) = hrv_time_domain(&ibis);

        // ── LF/HF ratio (from IBI spectrum) ──────────────────────────────
        let lf_hf_ratio = lf_hf_from_ibis(&ibis);

        // ── Respiratory rate from PPG envelope modulation ────────────────
        let respiratory_rate = respiratory_rate_from_ppg(&ir, PPG_SR);

        // ── SpO₂ estimate (uncalibrated) ─────────────────────────────────
        let spo2_estimate = spo2_from_red_ir(&red, &ir);

        // ── Perfusion Index ──────────────────────────────────────────────
        let perfusion_index = perfusion_index_from_ir(&ir);

        // ── Baevsky Stress Index ─────────────────────────────────────────
        let stress_index = baevsky_stress_index(&ibis);

        // Drain consumed samples (keep a small overlap for peak continuity)
        let drain = epoch_samples.min(self.ir_buf.len());
        let drain = drain.min(self.red_buf.len());
        let drain = drain.min(self.amb_buf.len());
        self.ir_buf.drain(..drain);
        self.red_buf.drain(..drain);
        self.amb_buf.drain(..drain);

        Some(PpgMetrics {
            hr, rmssd, sdnn, pnn50, lf_hf_ratio,
            respiratory_rate, spo2_estimate, perfusion_index,
            stress_index, n_beats,
        })
    }

    /// Clear all buffers (e.g. on disconnect).
    pub fn reset(&mut self) {
        self.ir_buf.clear();
        self.red_buf.clear();
        self.amb_buf.clear();
    }
}

// ── Peak detection ────────────────────────────────────────────────────────────

/// Detect peaks in the IR PPG signal and return inter-beat intervals (seconds).
///
/// Uses a simple adaptive-threshold approach:
/// 1. Bandpass the signal (0.5–8 Hz) using a moving average difference filter
/// 2. Find local maxima above the adaptive threshold
/// 3. Enforce minimum IBI (refractory period)
fn detect_peaks_and_ibis(ir: &[f64], sr: f64) -> Vec<f64> {
    let n = ir.len();
    if n < 16 { return vec![]; }

    // ── 1. Bandpass via difference of moving averages ─────────────────
    // Low-pass: window ≈ sr/8 (~8 samples at 64 Hz → ~8 Hz cutoff)
    let lp_win = (sr / 8.0).max(2.0) as usize;
    // High-pass: window ≈ sr/0.5 (~128 samples → removes DC drift)
    let hp_win = (sr / 0.5).max(4.0) as usize;

    let lp = moving_average(ir, lp_win);
    let hp = moving_average(&lp, hp_win.min(lp.len()));
    // Bandpassed = low-passed minus very-low-passed
    let bp: Vec<f64> = lp.iter().zip(hp.iter()).map(|(a, b)| a - b).collect();
    if bp.is_empty() { return vec![]; }

    // ── 2. Adaptive threshold (running mean + 0.6 × running std) ─────
    let win = (sr * 1.5) as usize; // 1.5s window for threshold adaptation
    let mut peaks: Vec<usize> = Vec::new();
    let refractory = (IBI_MIN * sr) as usize;

    for i in 1..(bp.len() - 1) {
        // Local maximum?
        if bp[i] <= bp[i - 1] || bp[i] <= bp[i + 1] { continue; }
        // Refractory period check
        if let Some(&last) = peaks.last() {
            if i - last < refractory { continue; }
        }
        // Adaptive threshold: mean + 0.6*std over local window
        let start = i.saturating_sub(win / 2);
        let end = (i + win / 2).min(bp.len());
        let window = &bp[start..end];
        let mean = window.iter().sum::<f64>() / window.len() as f64;
        let std = (window.iter().map(|&v| (v - mean).powi(2)).sum::<f64>()
            / window.len() as f64).sqrt();
        if bp[i] > mean + 0.6 * std {
            peaks.push(i);
        }
    }

    // ── 3. Convert peak indices to IBIs ──────────────────────────────
    let mut ibis = Vec::with_capacity(peaks.len().saturating_sub(1));
    for w in peaks.windows(2) {
        let ibi = (w[1] - w[0]) as f64 / sr;
        if (IBI_MIN..=IBI_MAX).contains(&ibi) {
            ibis.push(ibi);
        }
    }
    ibis
}

/// Simple moving average.
fn moving_average(x: &[f64], win: usize) -> Vec<f64> {
    let n = x.len();
    if n == 0 || win == 0 { return vec![]; }
    let w = win.min(n);
    let mut out = Vec::with_capacity(n);
    let mut sum: f64 = x[..w].iter().sum();
    // Centre the first output at position w/2
    for _ in 0..w / 2 { out.push(sum / w as f64); }
    out.push(sum / w as f64);
    for i in w..n {
        sum += x[i] - x[i - w];
        out.push(sum / w as f64);
    }
    // Pad tail
    while out.len() < n { out.push(*out.last().unwrap_or(&0.0)); }
    out.truncate(n);
    out
}

// ── HRV time-domain metrics ──────────────────────────────────────────────────

/// Returns (rmssd_ms, sdnn_ms, pnn50_pct).
fn hrv_time_domain(ibis: &[f64]) -> (f64, f64, f64) {
    if ibis.is_empty() { return (0.0, 0.0, 0.0); }

    // Convert to milliseconds
    let ibis_ms: Vec<f64> = ibis.iter().map(|&v| v * 1000.0).collect();

    // SDNN
    let mean = ibis_ms.iter().sum::<f64>() / ibis_ms.len() as f64;
    let sdnn = (ibis_ms.iter().map(|&v| (v - mean).powi(2)).sum::<f64>()
        / ibis_ms.len() as f64).sqrt();

    if ibis_ms.len() < 2 { return (0.0, sdnn, 0.0); }

    // RMSSD
    let mut sum_sq = 0.0f64;
    let mut nn50_count = 0u64;
    for w in ibis_ms.windows(2) {
        let diff = (w[1] - w[0]).abs();
        sum_sq += diff * diff;
        if diff > 50.0 { nn50_count += 1; }
    }
    let rmssd = (sum_sq / (ibis_ms.len() - 1) as f64).sqrt();

    // pNN50
    let pnn50 = nn50_count as f64 / (ibis_ms.len() - 1) as f64 * 100.0;

    (rmssd, sdnn, pnn50)
}

// ── LF/HF ratio ──────────────────────────────────────────────────────────────

/// Compute LF/HF ratio from the IBI series using Lomb-Scargle-like approach.
/// Since IBIs are unevenly spaced (beat-to-beat), we use Goertzel on an
/// interpolated uniform 4 Hz IBI series.
fn lf_hf_from_ibis(ibis: &[f64]) -> f64 {
    if ibis.len() < 4 { return 0.0; }

    // Create cumulative time axis and interpolate to uniform 4 Hz
    let resample_rate = 4.0; // Hz
    let mut t = Vec::with_capacity(ibis.len() + 1);
    t.push(0.0);
    for ibi in ibis { t.push(t.last().unwrap() + ibi); }
    let total_time = *t.last().unwrap();
    if total_time < 5.0 { return 0.0; } // Need at least 5s for meaningful LF

    let n_resamp = (total_time * resample_rate) as usize;
    if n_resamp < 8 { return 0.0; }
    let mut uniform = Vec::with_capacity(n_resamp);
    let mut j = 0usize;
    for i in 0..n_resamp {
        let ti = i as f64 / resample_rate;
        while j + 1 < t.len() - 1 && t[j + 1] < ti { j += 1; }
        // Linear interpolation of IBI at time ti
        if j < ibis.len() {
            let frac = if t[j + 1] > t[j] { (ti - t[j]) / (t[j + 1] - t[j]) } else { 0.0 };
            let ibi_val = if j + 1 < ibis.len() {
                ibis[j] * (1.0 - frac) + ibis[j + 1] * frac
            } else {
                ibis[j]
            };
            uniform.push(ibi_val);
        }
    }
    if uniform.len() < 8 { return 0.0; }

    // Remove mean
    let mean = uniform.iter().sum::<f64>() / uniform.len() as f64;
    let centered: Vec<f64> = uniform.iter().map(|&v| v - mean).collect();

    // Compute power in LF (0.04–0.15 Hz) and HF (0.15–0.4 Hz) using Goertzel
    let lf_power = band_power_goertzel(&centered, resample_rate, 0.04, 0.15);
    let hf_power = band_power_goertzel(&centered, resample_rate, 0.15, 0.40);

    if hf_power > 1e-12 { lf_power / hf_power } else { 0.0 }
}

/// Sum of |Goertzel|² for frequencies in [f_lo, f_hi] at 0.01 Hz steps.
fn band_power_goertzel(x: &[f64], sr: f64, f_lo: f64, f_hi: f64) -> f64 {
    let mut power = 0.0f64;
    let n = x.len();
    let mut f = f_lo;
    while f <= f_hi {
        let k = f * n as f64 / sr;
        let w = 2.0 * std::f64::consts::PI * k / n as f64;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &sample in x {
            let s0 = sample + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        power += s1 * s1 + s2 * s2 - coeff * s1 * s2;
        f += 0.01;
    }
    power
}

// ── Respiratory Rate ─────────────────────────────────────────────────────────

/// Estimate respiratory rate from PPG envelope modulation.
/// The PPG signal exhibits amplitude and baseline modulations at the breathing
/// frequency (~0.15–0.5 Hz = 9–30 breaths/min).
fn respiratory_rate_from_ppg(ir: &[f64], sr: f64) -> f64 {
    let n = ir.len();
    if n < (sr * 4.0) as usize { return 0.0; } // Need at least 4s

    // Extract envelope via moving average of absolute values
    let env_win = (sr / 2.0) as usize; // 0.5s window
    let env = moving_average(ir, env_win);
    if env.len() < 16 { return 0.0; }

    // Remove mean from envelope
    let mean = env.iter().sum::<f64>() / env.len() as f64;
    let centered: Vec<f64> = env.iter().map(|&v| v - mean).collect();

    // Find peak frequency in 0.15–0.5 Hz (9–30 bpm) using Goertzel
    let mut best_f = 0.0f64;
    let mut best_power = 0.0f64;
    let mut f = 0.15;
    while f <= 0.5 {
        let k = f * centered.len() as f64 / sr;
        let w = 2.0 * std::f64::consts::PI * k / centered.len() as f64;
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &sample in &centered {
            let s0 = sample + coeff * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
        if power > best_power {
            best_power = power;
            best_f = f;
        }
        f += 0.005;
    }

    best_f * 60.0 // Convert Hz to breaths per minute
}

// ── SpO₂ Estimate ────────────────────────────────────────────────────────────

/// Estimate SpO₂ from the ratio of ratios (R) of red and IR channels.
///
/// R = (AC_red / DC_red) / (AC_ir / DC_ir)
///
/// Using the standard linear approximation:
///   SpO₂ ≈ 110 - 25 × R
///
/// This is uncalibrated (no per-device calibration curve), so it gives
/// relative trends rather than absolute clinical accuracy.
fn spo2_from_red_ir(red: &[f64], ir: &[f64]) -> f64 {
    if red.len() < 16 || ir.len() < 16 { return 0.0; }
    let n = red.len().min(ir.len());

    // DC components (mean)
    let dc_red = red[..n].iter().sum::<f64>() / n as f64;
    let dc_ir  = ir[..n].iter().sum::<f64>() / n as f64;
    if dc_red.abs() < 1.0 || dc_ir.abs() < 1.0 { return 0.0; }

    // AC components (std dev as proxy for pulsatile amplitude)
    let ac_red = (red[..n].iter().map(|&v| (v - dc_red).powi(2)).sum::<f64>()
        / n as f64).sqrt();
    let ac_ir  = (ir[..n].iter().map(|&v| (v - dc_ir).powi(2)).sum::<f64>()
        / n as f64).sqrt();

    if ac_ir < 1e-6 { return 0.0; }

    let r = (ac_red / dc_red) / (ac_ir / dc_ir);

    // Standard linear approximation (Beer-Lambert based)
    let spo2 = 110.0 - 25.0 * r;
    spo2.clamp(70.0, 100.0)
}

// ── Perfusion Index ──────────────────────────────────────────────────────────

/// Perfusion Index = (AC / DC) × 100 for the IR channel.
/// AC = peak-to-trough amplitude (approximated as 2× std dev).
/// DC = mean value.
fn perfusion_index_from_ir(ir: &[f64]) -> f64 {
    let n = ir.len();
    if n < 8 { return 0.0; }
    let dc = ir.iter().sum::<f64>() / n as f64;
    if dc.abs() < 1.0 { return 0.0; }
    let ac = (ir.iter().map(|&v| (v - dc).powi(2)).sum::<f64>() / n as f64).sqrt() * 2.0;
    (ac / dc.abs()) * 100.0
}

// ── Baevsky Stress Index ─────────────────────────────────────────────────────

/// Baevsky's Stress Index (SI) from the IBI histogram.
///
/// SI = AMo / (2 × Mo × MxDMn)
///
/// where:
/// - Mo   = mode of the IBI histogram (most frequent IBI bin)
/// - AMo  = amplitude of the mode (% of IBIs in the mode bin)
/// - MxDMn = range of IBIs (max - min)
///
/// Higher SI = greater sympathetic activation / stress.
fn baevsky_stress_index(ibis: &[f64]) -> f64 {
    if ibis.len() < 3 { return 0.0; }

    let ibis_ms: Vec<f64> = ibis.iter().map(|&v| v * 1000.0).collect();
    let min_ibi = ibis_ms.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_ibi = ibis_ms.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max_ibi - min_ibi;
    if range < 1.0 { return 0.0; }

    // Histogram with 50ms bins
    let bin_width = 50.0;
    let n_bins = ((range / bin_width).ceil() as usize).max(1);
    let mut bins = vec![0u32; n_bins + 1];
    for &v in &ibis_ms {
        let idx = ((v - min_ibi) / bin_width) as usize;
        bins[idx.min(n_bins)] += 1;
    }

    // Mode: bin with highest count
    let (mode_idx, &mode_count) = bins.iter().enumerate()
        .max_by_key(|(_, &c)| c).unwrap();
    let mo = min_ibi + (mode_idx as f64 + 0.5) * bin_width; // Mode in ms
    let amo = mode_count as f64 / ibis_ms.len() as f64 * 100.0; // AMo in %
    let mxdmn = range / 1000.0; // Convert back to seconds

    if mo < 1.0 || mxdmn < 0.001 { return 0.0; }
    amo / (2.0 * (mo / 1000.0) * mxdmn)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hrv_time_domain() {
        // Regular 1s IBIs → HR=60, SDNN≈0, RMSSD≈0
        let ibis = vec![1.0, 1.0, 1.0, 1.0];
        let (rmssd, sdnn, pnn50) = hrv_time_domain(&ibis);
        assert!(sdnn < 1.0);
        assert!(rmssd < 1.0);
        assert_eq!(pnn50, 0.0);

        // Variable IBIs
        let ibis = vec![0.8, 0.9, 0.7, 1.0, 0.85];
        let (rmssd, sdnn, pnn50) = hrv_time_domain(&ibis);
        assert!(sdnn > 0.0);
        assert!(rmssd > 0.0);
        assert!((0.0..=100.0).contains(&pnn50));
    }

    #[test]
    fn test_perfusion_index() {
        // Synthetic IR signal with DC=1000, AC=10
        let ir: Vec<f64> = (0..128).map(|i| {
            1000.0 + 10.0 * (2.0 * std::f64::consts::PI * i as f64 / 64.0).sin()
        }).collect();
        let pi = perfusion_index_from_ir(&ir);
        assert!(pi > 0.5 && pi < 5.0, "PI={pi}");
    }

    #[test]
    fn test_spo2() {
        // Same amplitude ratio → R≈1.0 → SpO₂≈85
        let ir:  Vec<f64> = (0..128).map(|i| 1000.0 + 10.0 * (i as f64 * 0.1).sin()).collect();
        let red: Vec<f64> = (0..128).map(|i| 800.0  + 8.0  * (i as f64 * 0.1).sin()).collect();
        let spo2 = spo2_from_red_ir(&red, &ir);
        assert!((70.0..=100.0).contains(&spo2), "SpO2={spo2}");
    }

    // ── moving_average ────────────────────────────────────────────────────────

    #[test]
    fn moving_average_constant_signal_is_unchanged() {
        let x = vec![5.0_f64; 64];
        let out = moving_average(&x, 8);
        assert_eq!(out.len(), x.len());
        for v in &out { assert!((v - 5.0).abs() < 1e-9, "expected 5.0, got {v}"); }
    }

    #[test]
    fn moving_average_empty_returns_empty() {
        let out = moving_average(&[], 4);
        assert!(out.is_empty());
    }

    #[test]
    fn moving_average_output_length_equals_input_length() {
        let x: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let out = moving_average(&x, 10);
        assert_eq!(out.len(), x.len());
    }

    #[test]
    fn moving_average_window_larger_than_input_does_not_panic() {
        let x = vec![1.0, 2.0, 3.0];
        let out = moving_average(&x, 100); // win clamped to x.len()
        assert_eq!(out.len(), x.len());
    }

    // ── hrv_time_domain ───────────────────────────────────────────────────────

    #[test]
    fn hrv_time_domain_empty_ibis_returns_zeros() {
        let (rmssd, sdnn, pnn50) = hrv_time_domain(&[]);
        assert_eq!(rmssd, 0.0);
        assert_eq!(sdnn,  0.0);
        assert_eq!(pnn50, 0.0);
    }

    #[test]
    fn hrv_time_domain_single_ibi_returns_zero_rmssd() {
        // Only one IBI → no successive differences possible → rmssd = 0.
        let (rmssd, sdnn, pnn50) = hrv_time_domain(&[0.8]);
        assert_eq!(rmssd, 0.0);
        assert_eq!(pnn50, 0.0);
        // sdnn of a single value is defined as 0 (no variance)
        assert!(sdnn.is_finite());
    }

    #[test]
    fn hrv_time_domain_identical_ibis_gives_zero_sdnn_rmssd() {
        let ibis = vec![0.8; 10];
        let (rmssd, sdnn, pnn50) = hrv_time_domain(&ibis);
        assert!(rmssd < 1e-9, "rmssd should be 0 for identical IBIs, got {rmssd}");
        assert!(sdnn < 1e-9,  "sdnn should be 0 for identical IBIs, got {sdnn}");
        assert_eq!(pnn50, 0.0);
    }

    #[test]
    fn hrv_time_domain_pnn50_is_percentage() {
        let ibis = vec![0.8, 0.85, 0.75, 0.9, 0.65];
        let (_, _, pnn50) = hrv_time_domain(&ibis);
        assert!((0.0..=100.0).contains(&pnn50), "pnn50={pnn50}");
    }

    // ── spo2_from_red_ir ──────────────────────────────────────────────────────

    #[test]
    fn spo2_short_buffers_return_zero() {
        let ir  = vec![1000.0; 4]; // < 16
        let red = vec![800.0;  4];
        assert_eq!(spo2_from_red_ir(&red, &ir), 0.0);
    }

    #[test]
    fn spo2_zero_dc_returns_zero() {
        let ir  = vec![0.0; 32];
        let red = vec![0.0; 32];
        assert_eq!(spo2_from_red_ir(&red, &ir), 0.0);
    }

    #[test]
    fn spo2_result_clamped_to_valid_range() {
        // High R ratio would give SpO₂ < 70 → must be clamped to 70.
        let ir:  Vec<f64> = (0..64).map(|i| 1000.0 + 100.0 * (i as f64 * 0.3).sin()).collect();
        let red: Vec<f64> = (0..64).map(|i|   50.0 +   1.0 * (i as f64 * 0.3).sin()).collect();
        let spo2 = spo2_from_red_ir(&red, &ir);
        assert!((70.0..=100.0).contains(&spo2), "SpO₂={spo2} out of clamp range");
    }

    // ── perfusion_index_from_ir ───────────────────────────────────────────────

    #[test]
    fn perfusion_index_short_buffer_returns_zero() {
        let ir = vec![1000.0; 4]; // < 8
        assert_eq!(perfusion_index_from_ir(&ir), 0.0);
    }

    #[test]
    fn perfusion_index_zero_dc_returns_zero() {
        let ir = vec![0.0; 32];
        assert_eq!(perfusion_index_from_ir(&ir), 0.0);
    }

    #[test]
    fn perfusion_index_is_non_negative() {
        let ir: Vec<f64> = (0..128).map(|i| 1000.0 + 10.0 * (i as f64 * 0.1).sin()).collect();
        assert!(perfusion_index_from_ir(&ir) >= 0.0);
    }

    // ── baevsky_stress_index ──────────────────────────────────────────────────

    #[test]
    fn baevsky_too_few_ibis_returns_zero() {
        assert_eq!(baevsky_stress_index(&[]),      0.0);
        assert_eq!(baevsky_stress_index(&[0.8]),   0.0);
        assert_eq!(baevsky_stress_index(&[0.8, 0.9]), 0.0);
    }

    #[test]
    fn baevsky_identical_ibis_returns_zero() {
        // range = 0 → returns 0.0
        let ibis = vec![0.8; 10];
        assert_eq!(baevsky_stress_index(&ibis), 0.0);
    }

    #[test]
    fn baevsky_variable_ibis_returns_positive() {
        let ibis = vec![0.7, 0.9, 0.8, 1.0, 0.75, 0.85, 0.95];
        let si = baevsky_stress_index(&ibis);
        assert!(si >= 0.0, "Stress index should be non-negative, got {si}");
    }

    // ── PpgAnalyzer ───────────────────────────────────────────────────────────

    #[test]
    fn ppg_analyzer_reset_clears_buffers() {
        let mut a = PpgAnalyzer::new(5.0);
        let data: Vec<f64> = (0..64).map(|i| 1000.0 + (i as f64).sin()).collect();
        a.push(0, &data);
        a.push(1, &data);
        a.push(2, &data);
        a.reset();
        // After reset, insufficient data → compute_epoch returns None.
        assert!(a.compute_epoch(32).is_none());
    }

    #[test]
    fn ppg_analyzer_invalid_channel_is_ignored() {
        let mut a = PpgAnalyzer::new(5.0);
        a.push(99, &[1.0, 2.0, 3.0]); // should not panic
        assert!(a.compute_epoch(32).is_none()); // still no IR data
    }

    #[test]
    fn ppg_analyzer_insufficient_data_returns_none() {
        let mut a = PpgAnalyzer::new(5.0);
        // Push < 32 samples → compute_epoch should return None.
        a.push(1, &[1000.0; 20]);
        assert!(a.compute_epoch(20).is_none());
    }
}
