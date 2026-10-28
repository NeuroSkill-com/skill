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
///
/// # Noise check (after the amplitude rules)
///
/// Amplitude alone passes a sensor that isn't touching the skin, or one
/// buried in muscle noise, as "Good" whenever the noise happens to be a
/// normal size — a worn-but-loose Muse read `good` on every channel while the
/// signal was mostly 60 Hz hum. So, on top of the table above:
///
/// 1. Mains (50 and 60 Hz) is notched out first — it is filtered before
///    analysis anyway, which is the point of the "Why not first-difference"
///    note above.
/// 2. The **effective frequency** of what remains,
///    `f = fs/π · asin(√(var(Δx)/var(x)) / 2)` (Hjorth mobility, exact for a
///    sinusoid), is taken as the median over the last
///    [`QUALITY_NOISE_WINDOWS`] seconds. EEG power sits well below 30 Hz.
///    `f ≥ 45 Hz` → `Poor` (noise-dominated); `f ≥ 35 Hz` → `Fair`.
/// 3. If mains is ≥ 75 % of the raw power, the electrode is picking up hum
///    through a loose contact → `Fair`.
///
/// Thresholds come from Muse S recordings (see `skill-constants`).
use serde::Serialize;
use std::collections::VecDeque;

use skill_constants::{
    QUALITY_CLIP_UV, QUALITY_FAIR_RMS, QUALITY_MAINS_FAIR_SHARE, QUALITY_NOISE_FAIR_HZ, QUALITY_NOISE_POOR_HZ,
    QUALITY_NOISE_WINDOWS, QUALITY_NO_SIGNAL_RMS, QUALITY_POOR_CLIPS, QUALITY_POOR_RMS, QUALITY_WINDOW,
};

// ── Thresholds (from skill-constants) ─────────────────────────────────────────

const WINDOW: usize = QUALITY_WINDOW;
#[cfg(test)]
const MIN_SAMPLES: usize = WINDOW / 4;
const THRESH_NO_SIGNAL_RMS: f64 = QUALITY_NO_SIGNAL_RMS;
const THRESH_POOR_RMS: f64 = QUALITY_POOR_RMS;
const THRESH_CLIP_UV: f64 = QUALITY_CLIP_UV;
const THRESH_POOR_CLIPS: usize = QUALITY_POOR_CLIPS;
const THRESH_FAIR_RMS: f64 = QUALITY_FAIR_RMS;

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

impl SignalQuality {
    /// The wire label: `good` / `fair` / `poor` / `no_signal` (same as serde).
    ///
    /// Not `format!("{q:?}").to_lowercase()`, which gives `nosignal` — a
    /// spelling nothing downstream (monitor, UI electrode map, LLM tool)
    /// recognised, so dead electrodes were never counted as bad.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Fair => "fair",
            Self::Poor => "poor",
            Self::NoSignal => "no_signal",
        }
    }

    /// Poor contact or no signal: the channel shouldn't feed analysis.
    pub fn is_bad(&self) -> bool {
        matches!(self, Self::Poor | Self::NoSignal)
    }
}

// ── QualityMonitor ────────────────────────────────────────────────────────────

/// Maintains a rolling window of raw EEG samples per channel and computes
/// [`SignalQuality`] on demand.
pub struct QualityMonitor {
    bufs: Vec<VecDeque<f64>>,
    /// Mains-notched copy of the signal, [`QUALITY_NOISE_WINDOWS`] windows long.
    notched: Vec<VecDeque<f64>>,
    /// Per-channel state of each mains notch.
    notch_state: Vec<Vec<BiquadState>>,
    /// 50 / 60 Hz notches (those below Nyquist).
    notches: Vec<Biquad>,
    /// Window size in samples (≈1 second at the device sample rate).
    window: usize,
    /// Minimum samples before quality is reported.
    min_samples: usize,
}

/// RBJ notch biquad coefficients (normalised by a0).
#[derive(Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Biquad {
    fn notch(f0: f64, fs: f64, q: f64) -> Self {
        let w0 = 2.0 * std::f64::consts::PI * f0 / fs;
        let alpha = w0.sin() / (2.0 * q);
        let cos = w0.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: 1.0 / a0,
            b1: -2.0 * cos / a0,
            b2: 1.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct BiquadState {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl BiquadState {
    /// Steady state for a constant input `x` (a notch passes DC at unit
    /// gain). Priming with the first sample keeps a large DC offset — e.g.
    /// Emotiv's ~4200 µV baseline — from ringing the filter at 50–60 Hz.
    fn primed(x: f64) -> Self {
        Self {
            x1: x,
            x2: x,
            y1: x,
            y2: x,
        }
    }

    fn step(&mut self, c: &Biquad, x: f64) -> f64 {
        let y = c.b0 * x + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Mean-removed variance.
fn variance<'a>(xs: impl Iterator<Item = &'a f64> + Clone) -> f64 {
    let n = xs.clone().count() as f64;
    if n < 2.0 {
        return 0.0;
    }
    let mean = xs.clone().sum::<f64>() / n;
    xs.map(|&x| (x - mean) * (x - mean)).sum::<f64>() / n
}

/// Hjorth-mobility effective frequency (Hz) of one window; `None` if flat.
fn effective_frequency(w: &[f64], fs: f64) -> Option<f64> {
    let v = variance(w.iter());
    if v < 1e-9 {
        return None;
    }
    let diffs: Vec<f64> = w.windows(2).map(|p| p[1] - p[0]).collect();
    let m = (variance(diffs.iter()) / v).sqrt();
    Some(fs / std::f64::consts::PI * (m / 2.0).min(1.0).asin())
}

impl QualityMonitor {
    /// Create a quality monitor with the default window (256 samples ≈ 1 s @ 256 Hz).
    ///
    /// **Deprecated:** defaults to 256-sample window (Muse 256 Hz).
    /// Use [`with_window(channels, sample_rate as usize)`] for a 1-second
    /// window at any device sample rate.
    #[deprecated(
        since = "0.1.0",
        note = "use QualityMonitor::with_window(channels, sample_rate as usize) instead"
    )]
    pub fn new(channels: usize) -> Self {
        Self::with_window(channels, WINDOW)
    }

    /// Create a quality monitor with a custom window size.
    ///
    /// Use `sample_rate as usize` for a 1-second window at any sample rate.
    ///
    /// `window` is one second of samples, so it is also the sample rate the
    /// noise check uses.
    pub fn with_window(channels: usize, window: usize) -> Self {
        let window = window.max(32); // safety floor
        let fs = window as f64;
        let notches: Vec<Biquad> = [50.0, 60.0]
            .into_iter()
            .filter(|&f0| f0 < fs / 2.0 - 1.0)
            .map(|f0| Biquad::notch(f0, fs, 5.0))
            .collect();
        Self {
            bufs: (0..channels).map(|_| VecDeque::with_capacity(window + 1)).collect(),
            notched: (0..channels)
                .map(|_| VecDeque::with_capacity(window * QUALITY_NOISE_WINDOWS + 1))
                .collect(),
            notch_state: (0..channels)
                .map(|_| vec![BiquadState::default(); notches.len()])
                .collect(),
            notches,
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
        let (notched, state) = (&mut self.notched[channel], &mut self.notch_state[channel]);
        if notched.is_empty() {
            if let Some(&first) = samples.first() {
                state.fill(BiquadState::primed(first));
            }
        }
        for &s in samples {
            let y = self
                .notches
                .iter()
                .zip(state.iter_mut())
                .fold(s, |x, (c, st)| st.step(c, x));
            notched.push_back(y);
        }
        while notched.len() > self.window * QUALITY_NOISE_WINDOWS {
            notched.pop_front();
        }
    }

    /// `(effective frequency Hz, mains share 0–1)` of a channel, once a full
    /// second of signal is in: the median effective frequency of the
    /// mains-notched signal over up to [`QUALITY_NOISE_WINDOWS`] seconds, and
    /// how much of the last second's raw power was mains.
    pub fn noise_profile(&self, channel: usize) -> Option<(f64, f64)> {
        let raw = self.bufs.get(channel)?;
        let notched: Vec<f64> = self.notched.get(channel)?.iter().copied().collect();
        if raw.len() < self.window || notched.len() < self.window {
            return None;
        }
        let fs = self.window as f64;
        let mut fe: Vec<f64> = notched
            .rchunks_exact(self.window)
            .take(QUALITY_NOISE_WINDOWS)
            .filter_map(|w| effective_frequency(w, fs))
            .collect();
        if fe.is_empty() {
            return None;
        }
        fe.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = if fe.len() % 2 == 1 {
            fe[fe.len() / 2]
        } else {
            (fe[fe.len() / 2 - 1] + fe[fe.len() / 2]) / 2.0
        };
        let raw_var = variance(raw.iter());
        let last = &notched[notched.len() - self.window..];
        let share = if raw_var > 1e-9 {
            (1.0 - variance(last.iter()) / raw_var).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some((median, share))
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
        for buf in &mut self.notched {
            buf.clear();
        }
        for st in &mut self.notch_state {
            st.fill(BiquadState::default());
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
        let rms = (buf
            .iter()
            .map(|&x| {
                let ac = x - mean;
                ac * ac
            })
            .sum::<f64>()
            / n)
            .sqrt();

        if rms < THRESH_NO_SIGNAL_RMS {
            return SignalQuality::NoSignal;
        }

        // ── Clip count (AC-coupled — subtract mean so DC-coupled devices
        //    like Emotiv don't trigger false clips from their baseline) ──
        let clips = buf.iter().filter(|&&x| (x - mean).abs() > THRESH_CLIP_UV).count();

        if clips >= THRESH_POOR_CLIPS || rms > THRESH_POOR_RMS {
            return SignalQuality::Poor;
        }

        let by_amplitude = if rms > THRESH_FAIR_RMS {
            SignalQuality::Fair
        } else {
            SignalQuality::Good
        };

        // ── Noise check (see module docs) ──────────────────────────────────
        match self.noise_profile(channel) {
            Some((hz, _)) if hz >= QUALITY_NOISE_POOR_HZ => SignalQuality::Poor,
            Some((hz, share)) if hz >= QUALITY_NOISE_FAIR_HZ || share >= QUALITY_MAINS_FAIR_SHARE => {
                SignalQuality::Fair
            }
            _ => by_amplitude,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    fn monitor() -> QualityMonitor {
        QualityMonitor::new(1)
    }
    fn fill(m: &mut QualityMonitor, s: &[f64]) {
        m.push(0, s);
    }
    fn q(m: &QualityMonitor) -> SignalQuality {
        m.classify(0)
    }

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

    /// 10 Hz EEG + 60 Hz hum, `secs` long.
    fn eeg_plus_hum(eeg_uv: f64, hum_uv: f64, secs: usize) -> Vec<f64> {
        (0..WINDOW * secs)
            .map(|i| {
                let t = i as f64 / 256.0;
                eeg_uv * (2.0 * std::f64::consts::PI * 10.0 * t).sin()
                    + hum_uv * (2.0 * std::f64::consts::PI * 60.0 * t).sin()
            })
            .collect()
    }

    #[test]
    fn moderate_mains_hum_with_eeg_is_good() {
        // Mains is notched before analysis; a worn sensor with some hum is fine.
        let mut m = monitor();
        fill(&mut m, &eeg_plus_hum(40.0, 40.0, 4));
        assert_eq!(q(&m), SignalQuality::Good);
    }

    #[test]
    fn hum_dominated_signal_is_fair() {
        // 40 µV EEG under 120 µV hum (90 % mains): the amplitude rules alone
        // call this Good (RMS ≈ 89 µV), but an electrode picking up that much
        // hum has a loose contact. Fair — still analysed after the notch.
        let mut m = monitor();
        let s = eeg_plus_hum(40.0, 120.0, 4);
        fill(&mut m, &s);
        assert_eq!(q(&m), SignalQuality::Fair);
        let (_, share) = m.noise_profile(0).unwrap();
        assert!(share > 0.85, "share={share}");
    }

    /// Deterministic white-ish noise.
    fn noise(n: usize, sigma: f64) -> Vec<f64> {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                ((x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) * sigma * 1.732
            })
            .collect()
    }

    #[test]
    fn broadband_noise_at_normal_amplitude_is_poor() {
        // A sensor off the skin: ~30 µV of broadband noise and no EEG. The
        // amplitude rules say Good; the noise check says Poor.
        let mut m = monitor();
        fill(&mut m, &noise(WINDOW * 4, 30.0));
        let (hz, _) = m.noise_profile(0).unwrap();
        assert!(hz >= QUALITY_NOISE_POOR_HZ, "hz={hz}");
        assert_eq!(q(&m), SignalQuality::Poor);
    }

    #[test]
    fn moderate_muscle_noise_on_eeg_is_fair() {
        // 38 µV alpha + 22 µV broadband (muscle tension / dry sensor).
        let mut m = monitor();
        let n = noise(WINDOW * 4, 22.0);
        let s: Vec<f64> = eeg_plus_hum(38.0, 0.0, 4).iter().zip(&n).map(|(a, b)| a + b).collect();
        fill(&mut m, &s);
        let (hz, _) = m.noise_profile(0).unwrap();
        assert!((QUALITY_NOISE_FAIR_HZ..QUALITY_NOISE_POOR_HZ).contains(&hz), "hz={hz}");
        assert_eq!(q(&m), SignalQuality::Fair);
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
        fill(&mut m, &poor); // evicts the good data
        assert_eq!(q(&m), SignalQuality::Poor);
    }
}
