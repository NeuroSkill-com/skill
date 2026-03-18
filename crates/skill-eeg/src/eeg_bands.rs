// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! GPU-accelerated EEG frequency band power analysis.
//!
//! ## Method
//!
//! A **Hann-windowed, 512-sample FFT** is computed for all 4 Muse channels in
//! a **single `fft_batch` GPU dispatch** every [`BAND_HOP`] = 64 new samples
//! (≈ 250 ms at 256 Hz).  The one-sided PSD is integrated over each clinical
//! EEG frequency band to produce absolute (µV²) and relative (0–1) band powers.
//!
//! ## Why 512 samples?
//!
//! | Window | Duration | Freq. resolution | Lowest delta bin |
//! |--------|----------|------------------|-----------------|
//! | 256    | 1 s      | 1.0 Hz / bin     | bin 1 = 1.0 Hz (misses 0.5 Hz delta onset) |
//! | **512** | **2 s** | **0.5 Hz / bin** | **bin 1 = 0.5 Hz ✓** |
//!
//! The 0.5 Hz lower bound of delta cannot be resolved with a 1 s window;
//! 512 samples give exactly the 0.5 Hz / bin resolution needed.
//!
//! ## Hann windowing
//!
//! The Hann window reduces spectral leakage between adjacent bins — critical
//! for separating the delta (0.5–4 Hz) and theta (4–8 Hz) bands which are
//! only 0.5 Hz apart from the window's lowest resolved frequency.
//!
//! PSD normalisation follows Heinzel et al. (2002):
//!
//! ```text
//! S[k] = factor × |X[k]|² / (fs × Σ wᵢ²)   [µV²/Hz]
//! ```
//!
//! where `factor = 1` for DC and Nyquist, `2` for all other bins.
//! Band power (µV²) is then `P = Σ_k S[k] × Δf`, which simplifies to:
//!
//! ```text
//! P_band = Σ_k factor × psd_raw[k] / Σ wᵢ²
//! ```
//!
//! where `psd_raw[k] = (r[k]² + i[k]²) / n` is the output of
//! [`gpu_fft::psd::psd`].
//!
//! ## Batch GPU execution
//!
//! All 4 channels are submitted together as a `4 × 512` matrix in one
//! `fft_batch` call — the GPU kernel covers all channels in a single 2-D
//! workgroup dispatch with no per-channel overhead.
//!
//! ## Bands
//!
//! | Band       | Range (Hz)  | Association |
//! |------------|-------------|-------------|
//! | Delta      | 0.5 – 4     | Deep sleep, slow waves |
//! | Theta      | 4 – 8       | Drowsiness, meditation, memory |
//! | Alpha      | 8 – 13      | Relaxed wakefulness, eyes-closed |
//! | Beta       | 13 – 30     | Active cognition, focus, anxiety |
//! | Gamma      | 30 – 50     | High-level processing, binding |
//! | High-Gamma | 50 – 100    | Broadband / EMG artefact region |

use std::collections::VecDeque;
use std::f32::consts::PI;
use std::time::{SystemTime, UNIX_EPOCH};

use gpu_fft::{fft_batch, psd::psd};
use serde::{Deserialize, Serialize};

use crate::constants::{
    BAND_COLORS, BAND_HOP, BAND_SYMBOLS, BAND_WINDOW, BANDS,
    CHANNEL_NAMES, EEG_CHANNELS, MUSE_SAMPLE_RATE, NUM_BANDS,
};
use crate::band_metrics::*;

// ── Output types ─────────────────────────────────────────────────────────────

/// Absolute and relative power for all 6 EEG bands on one channel.
///
/// Serialises to a flat JSON object so the frontend can access individual
/// bands directly without nesting (e.g. `ch.rel_alpha`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandPowers {
    /// Channel label: `"TP9"`, `"AF7"`, `"AF8"`, or `"TP10"`.
    pub channel: String,

    // ── Absolute power (µV²) ─────────────────────────────────────────────────
    // Computed from the Hann-corrected one-sided PSD integrated over each band.
    pub delta:      f32,
    pub theta:      f32,
    pub alpha:      f32,
    pub beta:       f32,
    pub gamma:      f32,
    pub high_gamma: f32,

    // ── Relative power (0.0 – 1.0) ───────────────────────────────────────────
    // Each band divided by the sum of all 6 bands (broadband 0.5–100 Hz).
    pub rel_delta:      f32,
    pub rel_theta:      f32,
    pub rel_alpha:      f32,
    pub rel_beta:       f32,
    pub rel_gamma:      f32,
    pub rel_high_gamma: f32,

    /// Name of the band with the highest relative power (e.g. `"alpha"`).
    pub dominant:        String,
    /// Greek symbol of the dominant band (e.g. `"α"`).
    pub dominant_symbol: String,
    /// Hex colour of the dominant band (e.g. `"#22c55e"`).
    pub dominant_color:  String,
}

/// Band power snapshot for all 4 Muse channels, emitted as `"eeg-bands"` event.
///
/// In addition to per-channel relative band powers, the snapshot contains
/// several derived cross-band and cross-channel indices.  Each is grounded
/// in peer-reviewed methodology.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandSnapshot {
    /// Unix timestamp in fractional seconds when this snapshot was computed.
    pub timestamp: f64,
    /// One entry per channel: `[TP9, AF7, AF8, TP10]`.
    pub channels: Vec<BandPowers>,

    // ── Cross-band ratios (averaged across 4 channels) ───────────────────────

    /// Frontal Alpha Asymmetry: ln(AF8 α) − ln(AF7 α).
    /// Positive → left-hemisphere approach bias.  [Coan & Allen 2004]
    pub faa: f32,

    /// Theta / Alpha ratio — drowsiness / meditation indicator.  [Putman 2010]
    pub tar: f32,

    /// Beta / Alpha ratio — attention / stress marker.  [Angelidis 2016]
    pub bar: f32,

    /// Delta / Theta ratio — deep-sleep / deep-relaxation indicator.  [Knyazev 2012]
    pub dtr: f32,

    // ── Spectral shape metrics (averaged across channels) ────────────────────

    /// Power Spectral Entropy — spectral disorder / complexity.  [Inouye 1991]
    /// Higher = more uniform spectrum; lower = dominated by one band.
    pub pse: f32,

    /// Alpha Peak Frequency — frequency of max power in 8–13 Hz.  [Klimesch 1999]
    /// Averaged across channels.  `0.0` if no alpha peak found.
    pub apf: f32,

    /// Band-Power Slope (1/f aperiodic exponent).  [Donoghue 2020]
    /// Linear regression of log₁₀(power) vs log₁₀(freq) across 1–50 Hz.
    /// More negative = steeper spectral fall-off.
    pub bps: f32,

    /// Signal-to-noise ratio (dB) — broadband power / 50–60 Hz noise.  [Cohen 2014]
    /// Averaged across channels.
    pub snr: f32,

    // ── Cross-channel synchrony ──────────────────────────────────────────────

    /// Mean inter-channel coherence in the alpha band (8–13 Hz).  [Lachaux 1999]
    /// Simplified as correlation of alpha relative powers across channel pairs.
    pub coherence: f32,

    // ── Mu suppression (8–12 Hz) ─────────────────────────────────────────────

    /// Mu suppression index: current alpha power / running baseline alpha.
    /// < 1.0 = suppression (motor imagery).  [Pfurtscheller & Lopes da Silva 1999]
    pub mu_suppression: f32,

    // ── Composite indices ────────────────────────────────────────────────────

    /// Mood index: weighted composite of FAA, TAR, BAR (0–100).
    /// Higher = more positive/approach valence.
    pub mood: f32,

    // ── New metrics ──────────────────────────────────────────────────────────

    /// Theta/Beta Ratio (absolute power).  Cortical arousal index.
    pub tbr: f32,

    /// Spectral Edge Frequency — freq below which 95% of power lies.  [Rampil 1998]
    pub sef95: f32,

    /// Spectral Centroid — centre of mass of the power spectrum (Hz).  [Gudmundsson 2007]
    pub spectral_centroid: f32,

    /// Hjorth Activity — total signal variance (µV²).  [Hjorth 1970]
    pub hjorth_activity: f32,
    /// Hjorth Mobility — mean frequency estimate.  [Hjorth 1970]
    pub hjorth_mobility: f32,
    /// Hjorth Complexity — bandwidth / spectral spread.  [Hjorth 1970]
    pub hjorth_complexity: f32,

    /// Permutation Entropy — nonlinear complexity (0–1).  [Bandt & Pompe 2002]
    pub permutation_entropy: f32,

    /// Higuchi Fractal Dimension.  [Higuchi 1988]
    pub higuchi_fd: f32,

    /// DFA scaling exponent — long-range temporal correlations.  [Peng 1994]
    pub dfa_exponent: f32,

    /// Sample Entropy — signal regularity / complexity.  [Richman & Moorman 2000]
    pub sample_entropy: f32,

    /// Phase-Amplitude Coupling (θ–γ) — cross-frequency coupling.  [Canolty 2006]
    pub pac_theta_gamma: f32,

    /// Laterality Index — generalised L/R asymmetry across all bands.  [Homan 1987]
    pub laterality_index: f32,

    // ── Headache / Migraine EEG correlate indices (0–100) ───────────────────
    // Research biomarkers derived from published literature.
    // NOT clinical diagnostic tools — for informational/research purposes only.

    /// Headache correlate — cortical hyperexcitability: high beta + suppressed alpha + high BAR.
    pub headache_index:      f32,
    /// Migraine correlate — cortical spreading depression proxy: elevated delta + alpha
    /// suppression + hemispheric lateralisation.
    pub migraine_index:      f32,

    // ── Consciousness metrics (0–100) ─────────────────────────────────────────

    /// Lempel-Ziv Complexity proxy — signal information richness.
    /// Approximated via permutation entropy + Higuchi FD.
    pub consciousness_lzc:         f32,
    /// Wakefulness level — inverse drowsiness modulated by BAR and TAR.
    pub consciousness_wakefulness: f32,
    /// Information Integration proxy — global workspace (coherence × PAC × spectral entropy).
    pub consciousness_integration: f32,

    // ── PPG-derived metrics (populated from PpgAnalyzer when available) ──────

    /// Heart rate (bpm).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hr: Option<f64>,
    /// RMSSD — root mean square of successive IBI differences (ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rmssd: Option<f64>,
    /// SDNN — standard deviation of IBIs (ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdnn: Option<f64>,
    /// pNN50 — percentage of successive IBIs differing by >50 ms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnn50: Option<f64>,
    /// LF/HF ratio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lf_hf_ratio: Option<f64>,
    /// Respiratory rate estimate (breaths per minute).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respiratory_rate: Option<f64>,
    /// SpO₂ estimate (%).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spo2_estimate: Option<f64>,
    /// Perfusion Index (%).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub perfusion_index: Option<f64>,
    /// Baevsky Stress Index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stress_index: Option<f64>,

    // ── Artifact / event metrics ─────────────────────────────────────────────

    /// Total blink count since connection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blink_count: Option<u64>,
    /// Blinks per minute (rolling 60 s window).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blink_rate: Option<f64>,
    // ── Head pose metrics ────────────────────────────────────────────────────

    /// Head pitch in degrees (positive = up).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_pitch: Option<f64>,
    /// Head roll in degrees (positive = right ear down).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_roll: Option<f64>,
    /// Stillness score 0–100 (100 = perfectly still).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stillness: Option<f64>,
    /// Total nod count since connection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nod_count: Option<u64>,
    /// Total head-shake count since connection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shake_count: Option<u64>,

    // ── Composite scores ─────────────────────────────────────────────────────

    /// Meditation score (0–100).  High alpha + low beta + stillness + HRV.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meditation: Option<f64>,
    /// Cognitive load score (0–100).  Frontal theta / parietal alpha.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cognitive_load: Option<f64>,
    /// Drowsiness score (0–100).  High TAR + alpha spindles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drowsiness: Option<f64>,

    // ── Device telemetry ─────────────────────────────────────────────────────

    /// Raw temperature ADC value from headset (Classic firmware only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_raw: Option<u16>,

    // ── GPU utilisation ──────────────────────────────────────────────────────

    /// GPU overall utilisation 0.0–1.0 (from IOKit on macOS).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_overall: Option<f64>,
    /// GPU render engine utilisation 0.0–1.0 (Apple Silicon).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_render: Option<f64>,
    /// GPU tiler/geometry engine utilisation 0.0–1.0 (Apple Silicon).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_tiler: Option<f64>,
}

// ── BandAnalyzer ─────────────────────────────────────────────────────────────

/// Sliding-window EEG band power analyzer using batched GPU FFT.
///
/// Maintains a per-channel ring buffer of the most recent [`BAND_WINDOW`]
/// samples.  Each time all 4 channels have accumulated [`BAND_HOP`] new
/// samples, [`compute_snapshot`] is called:
///
/// 1. Applies a Hann window to the 512-sample analysis block.
/// 2. Calls **`fft_batch`** — one GPU dispatch for all 4 channels.
/// 3. Computes the one-sided PSD via **`psd::psd`**.
/// 4. Integrates the PSD over each band's frequency range.
/// 5. Normalises to relative powers and identifies the dominant band.
/// 6. Stores the result in [`BandAnalyzer::latest`].
pub struct BandAnalyzer {
    /// Per-channel sliding analysis window (most recent ≤ BAND_WINDOW samples).
    window: [VecDeque<f32>; EEG_CHANNELS],
    /// Per-channel queue of new unprocessed samples (drained in BAND_HOP batches).
    queued: [VecDeque<f32>; EEG_CHANNELS],
    /// Tracks which channels have received at least one sample.
    active: [bool; EEG_CHANNELS],
    /// Precomputed Hann window coefficients, length = BAND_WINDOW.
    hann: Vec<f32>,
    /// Σ wᵢ² — sum of squared Hann coefficients, used for PSD normalisation.
    hann_sum_sq: f32,
    /// Most recently computed snapshot; `None` until the first full window.
    pub latest: Option<BandSnapshot>,
    /// Running EMA of mean alpha power across channels (for mu suppression).
    alpha_baseline: f32,
    /// Number of snapshots computed so far (for EMA warm-up).
    snapshot_count: u64,
    /// Hardware sample rate (Hz).  Used for PSD bin-frequency and PAC computation.
    sample_rate: f32,
}

impl BandAnalyzer {
    /// Create a new analyser with pre-computed Hann coefficients.
    ///
    /// Uses the default Muse sample rate (256 Hz).  For non-Muse devices
    /// call [`new_with_rate`] instead.
    pub fn new() -> Self {
        Self::new_with_rate(MUSE_SAMPLE_RATE)
    }

    /// Create a new analyser for a specific hardware sample rate.
    ///
    /// The sample rate is used for PSD bin-frequency mapping, PAC
    /// computation, and all derived spectral metrics.
    pub fn new_with_rate(sample_rate: f32) -> Self {
        // Hann window: wᵢ = 0.5 × (1 − cos(2π·i / (N−1)))
        // For N = 512: Σ wᵢ² ≈ 512 × 3/8 = 192
        let hann: Vec<f32> = (0..BAND_WINDOW)
            .map(|i| {
                0.5 * (1.0
                    - (2.0 * PI * i as f32 / (BAND_WINDOW as f32 - 1.0)).cos())
            })
            .collect();
        let hann_sum_sq = hann.iter().map(|&w| w * w).sum::<f32>();
        Self {
            window:      std::array::from_fn(|_| VecDeque::new()),
            queued:      std::array::from_fn(|_| VecDeque::new()),
            active:      [false; EEG_CHANNELS],
            hann,
            hann_sum_sq,
            latest: None,
            alpha_baseline: 0.0,
            snapshot_count: 0,
            sample_rate,
        }
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Queue raw µV samples for `channel` (0 = TP9 … 3 = TP10).
    ///
    /// Converts `f64 → f32` internally.  Returns `true` when at least one new
    /// [`BandSnapshot`] was computed and stored in [`Self::latest`].
    pub fn push(&mut self, channel: usize, samples: &[f64]) -> bool {
        if channel >= EEG_CHANNELS || samples.is_empty() {
            return false;
        }
        self.active[channel] = true;
        for &v in samples {
            self.queued[channel].push_back(v as f32);
        }
        // Fire one or more hops while every *active* channel has ≥ BAND_HOP
        // queued samples.  Inactive channels (never pushed to) are skipped.
        let mut fired = false;
        while self.active.iter().enumerate().all(|(ch, &on)| {
            !on || self.queued[ch].len() >= BAND_HOP
        }) {
            // Need at least one active channel to fire.
            if !self.active.iter().any(|&on| on) { break; }
            self.compute_snapshot();
            fired = true;
        }
        fired
    }

    /// Clear all internal state, resetting the analyser to its initial
    /// condition as if `new()` had just been called.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        for ch in 0..EEG_CHANNELS {
            self.window[ch].clear();
            self.queued[ch].clear();
        }
        self.active = [false; EEG_CHANNELS];
        self.latest = None;
        self.alpha_baseline = 0.0;
        self.snapshot_count = 0;
    }

    // ── Core ─────────────────────────────────────────────────────────────────

    /// Drain one hop from every channel's queue, slide the analysis window
    /// forward, and — once the window is full — run the GPU batch and update
    /// [`Self::latest`].
    fn compute_snapshot(&mut self) {
        // ── 1. Slide the analysis window forward by BAND_HOP ─────────────────
        //
        // Each channel's `window` deque holds the most recent ≤ BAND_WINDOW
        // samples.  We drain BAND_HOP samples from `queued` into `window` and
        // evict any excess from the front to keep it capped at BAND_WINDOW.
        for ch in 0..EEG_CHANNELS {
            for _ in 0..BAND_HOP {
                let s = self.queued[ch].pop_front().unwrap_or(0.0);
                self.window[ch].push_back(s);
            }
            while self.window[ch].len() > BAND_WINDOW {
                self.window[ch].pop_front();
            }
        }

        // Wait for a full BAND_WINDOW of data before the first estimate.
        // Warmup = BAND_WINDOW / BAND_HOP × BAND_HOP / sample_rate
        //        = 512 / 256 = 2 seconds.
        if self.window.iter().any(|w| w.len() < BAND_WINDOW) {
            return;
        }

        // ── 2. Apply Hann window and build the fft_batch input ───────────────
        //
        // Element-wise multiply each sample by the Hann coefficient at the same
        // position.  The window tapers to zero at both ends, eliminating the
        // spectral leakage caused by the abrupt block edges.
        let signals: Vec<Vec<f32>> = (0..EEG_CHANNELS)
            .map(|ch| {
                self.window[ch]
                    .iter()
                    .zip(&self.hann)
                    .map(|(&v, &w)| v * w)
                    .collect()
            })
            .collect();

        // ── 3. GPU: forward FFT — all 4 channels in one dispatch ─────────────
        //
        // BAND_WINDOW = 512 is a power of two → no zero-padding.
        // The GPU kernel processes a 4 × 512 matrix in a single 2-D workgroup.
        let spectra = fft_batch(&signals);
        let n       = spectra[0].0.len(); // = BAND_WINDOW = 512
        debug_assert_eq!(n, BAND_WINDOW);

        // ── 4. PSD + band integration ─────────────────────────────────────────
        //
        // One-sided PSD (Heinzel et al. 2002 normalisation):
        //   S[k] = factor × |X[k]|² / (fs × Σwᵢ²)   [µV²/Hz]
        //
        // With psd_raw[k] = |X[k]|² / n (from gpu_fft::psd::psd):
        //   S[k] = factor × n × psd_raw[k] / (fs × Σwᵢ²)
        //
        // Band power (µV²):
        //   P = Σ_k S[k] × Δf  where  Δf = fs/n
        //     = Σ_k factor × n × psd_raw[k] × (fs/n) / (fs × Σwᵢ²)
        //     = Σ_k factor × psd_raw[k] / Σwᵢ²
        //
        // So the per-bin scale is just 1 / hann_sum_sq (independent of fs, n).
        let n_oneside   = n / 2 + 1; // 257 unique positive-frequency bins
        let nyq_bin     = n / 2;     // 256
        let bin_hz      = self.sample_rate / n as f32;
        let abs_scale   = 1.0 / self.hann_sum_sq;      // PSD normalisation

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        let active_count = self.active.iter().filter(|&&a| a).count();
        let mut ch_powers: Vec<BandPowers> = Vec::with_capacity(active_count);
        // Per-channel alpha peak freq accumulator and PSD-derived data for
        // cross-channel metrics computed after the loop.
        let mut ch_apf_sum   = 0.0f32;
        let mut ch_apf_count = 0u32;
        let mut ch_bps_sum   = 0.0f32;
        let mut ch_snr_sum   = 0.0f32;
        let mut ch_alpha_abs_sum = 0.0f32;

        for ch in 0..EEG_CHANNELS {
            if !self.active[ch] { continue; }
            let (real, imag) = &spectra[ch];

            // One-sided raw PSD (length n_oneside = 257).
            // psd::psd gives (r² + i²) / n for each bin.
            let psd_raw = psd(&real[..n_oneside], &imag[..n_oneside]);

            // Integrate each band.
            let mut abs_pwr = [0.0f32; NUM_BANDS];
            for (b, &(_, lo, hi)) in BANDS.iter().enumerate() {
                for (k, &psd_k) in psd_raw.iter().enumerate().take(n_oneside) {
                    let freq = k as f32 * bin_hz;
                    if freq >= lo && freq < hi {
                        // DC (k=0) and Nyquist (k=nyq_bin) are not mirrored;
                        // all other bins appear in both halves of the two-sided
                        // spectrum, so multiply by 2 to restore their power.
                        let factor = if k == 0 || k == nyq_bin { 1.0f32 } else { 2.0 };
                        abs_pwr[b] += psd_k * factor * abs_scale;
                    }
                }
            }

            // Relative powers: normalise by broadband total (all 6 bands).
            let total = abs_pwr.iter().sum::<f32>();
            let safe  = if total > 1e-12 { total } else { 1.0 };
            let rel   = abs_pwr.map(|p| p / safe);

            // Dominant band: the one with highest relative power.
            let dom = rel
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .unwrap_or(2); // fallback: alpha

            // ── Alpha Peak Frequency (APF) — max power in 8–13 Hz ───────────
            {
                let lo_bin = (8.0 / bin_hz).ceil()  as usize;
                let hi_bin = (13.0 / bin_hz).floor() as usize;
                let mut best_k = lo_bin;
                let mut best_p = 0.0f32;
                for (k, &psd_k) in psd_raw.iter().enumerate().take(hi_bin.min(n_oneside - 1) + 1).skip(lo_bin) {
                    let p = psd_k * abs_scale;
                    if p > best_p { best_p = p; best_k = k; }
                }
                if best_p > 1e-12 {
                    ch_apf_sum += best_k as f32 * bin_hz;
                    ch_apf_count += 1;
                }
            }

            // ── Band-Power Slope (BPS) — log–log regression 1–50 Hz ─────────
            // Linear regression: log₁₀(P) = slope × log₁₀(f) + intercept
            {
                let lo_bin = (1.0 / bin_hz).ceil()  as usize;
                let hi_bin = (50.0 / bin_hz).floor() as usize;
                let mut sx  = 0.0f64;
                let mut sy  = 0.0f64;
                let mut sxy = 0.0f64;
                let mut sx2 = 0.0f64;
                let mut cnt = 0u32;
                for (k, &psd_k) in psd_raw.iter().enumerate().take(hi_bin.min(n_oneside - 1) + 1).skip(lo_bin) {
                    let p = (psd_k * abs_scale) as f64;
                    if p > 1e-20 {
                        let x = ((k as f64) * bin_hz as f64).log10();
                        let y = p.log10();
                        sx  += x;
                        sy  += y;
                        sxy += x * y;
                        sx2 += x * x;
                        cnt += 1;
                    }
                }
                if cnt > 2 {
                    let n = cnt as f64;
                    let slope = (n * sxy - sx * sy) / (n * sx2 - sx * sx);
                    ch_bps_sum += slope as f32;
                }
            }

            // ── SNR — broadband (1–50 Hz) vs line-noise band (50–60 Hz) ─────
            {
                let mut sig_power  = 0.0f32;
                let mut noise_power = 0.0f32;
                let mut sig_count  = 0u32;
                let mut noise_count = 0u32;
                for (k, &psd_k) in psd_raw.iter().enumerate().take(n_oneside) {
                    let freq = k as f32 * bin_hz;
                    let p = psd_k * abs_scale;
                    if (1.0..50.0).contains(&freq) {
                        sig_power += p;
                        sig_count += 1;
                    } else if (50.0..60.0).contains(&freq) {
                        noise_power += p;
                        noise_count += 1;
                    }
                }
                if noise_count > 0 && noise_power > 1e-12 {
                    let sig_avg   = sig_power / sig_count.max(1) as f32;
                    let noise_avg = noise_power / noise_count as f32;
                    ch_snr_sum += 10.0 * (sig_avg / noise_avg).log10();
                }
            }

            ch_alpha_abs_sum += abs_pwr[2]; // alpha band

            ch_powers.push(BandPowers {
                channel:        CHANNEL_NAMES.get(ch).copied()
                                    .unwrap_or("Ch?").to_string(),
                delta:          abs_pwr[0],
                theta:          abs_pwr[1],
                alpha:          abs_pwr[2],
                beta:           abs_pwr[3],
                gamma:          abs_pwr[4],
                high_gamma:     abs_pwr[5],
                rel_delta:      rel[0],
                rel_theta:      rel[1],
                rel_alpha:      rel[2],
                rel_beta:       rel[3],
                rel_gamma:      rel[4],
                rel_high_gamma: rel[5],
                dominant:        BANDS[dom].0.to_string(),
                dominant_symbol: BAND_SYMBOLS[dom].to_string(),
                dominant_color:  BAND_COLORS[dom].to_string(),
            });
        }

        let nch = ch_powers.len() as f32;
        let safe_nch = if nch > 0.0 { nch } else { 1.0 };

        // ── Frontal Alpha Asymmetry (FAA) ────────────────────────────────────
        // Resolve left / right frontal electrodes by name so this works across
        // all device montages (Muse, Emotiv, Hermes, MW75, …).
        // Left-frontal  10-20 labels: AF7, AF3, F7, F3, Fp1, FC5, FC1, FT7
        // Right-frontal 10-20 labels: AF8, AF4, F8, F4, Fp2, FC6, FC2, FT8
        let faa = {
            const LEFT:  &[&str] = &["AF7","AF3","F7","F3","Fp1","FC5","FC1","FT7"];
            const RIGHT: &[&str] = &["AF8","AF4","F8","F4","Fp2","FC6","FC2","FT8"];
            let mut l_sum = 0.0f32; let mut l_n = 0u32;
            let mut r_sum = 0.0f32; let mut r_n = 0u32;
            for ch in &ch_powers {
                let name = ch.channel.as_str();
                if LEFT.contains(&name)  { l_sum += ch.alpha; l_n += 1; }
                if RIGHT.contains(&name) { r_sum += ch.alpha; r_n += 1; }
            }
            if l_n > 0 && r_n > 0 {
                let l_avg = (l_sum / l_n as f32).max(1e-6);
                let r_avg = (r_sum / r_n as f32).max(1e-6);
                r_avg.ln() - l_avg.ln()
            } else if ch_powers.len() >= 3 {
                // Fallback for generic labels: use indices [1] vs [2]
                let left  = ch_powers[1].alpha.max(1e-6);
                let right = ch_powers[2].alpha.max(1e-6);
                right.ln() - left.ln()
            } else {
                0.0
            }
        };

        // ── Cross-band ratios (averaged across channels) ─────────────────────
        let (mut sum_tar, mut sum_bar, mut sum_dtr) = (0.0f32, 0.0f32, 0.0f32);
        for ch in &ch_powers {
            let alpha_safe = ch.rel_alpha.max(1e-8);
            let theta_safe = ch.rel_theta.max(1e-8);
            sum_tar += ch.rel_theta / alpha_safe;
            sum_bar += ch.rel_beta  / alpha_safe;
            sum_dtr += ch.rel_delta / theta_safe;
        }
        let tar = sum_tar / safe_nch;
        let bar = sum_bar / safe_nch;
        let dtr = sum_dtr / safe_nch;

        // ── Power Spectral Entropy (PSE) ─────────────────────────────────────
        // Shannon entropy of the relative power distribution (5 main bands).
        // Normalised by log₂(5) to give range [0, 1].
        let mut pse_sum = 0.0f32;
        for ch in &ch_powers {
            let p = [ch.rel_delta, ch.rel_theta, ch.rel_alpha, ch.rel_beta, ch.rel_gamma];
            let total_p: f32 = p.iter().sum();
            let safe_t = if total_p > 1e-12 { total_p } else { 1.0 };
            let mut h = 0.0f32;
            for &pi in &p {
                let q = pi / safe_t;
                if q > 1e-12 {
                    h -= q * q.ln();
                }
            }
            // Normalise by ln(5) ≈ 1.6094 to get [0, 1]
            pse_sum += h / (5.0f32).ln();
        }
        let pse = pse_sum / safe_nch;

        // ── Alpha Peak Frequency (APF) ───────────────────────────────────────
        let apf = if ch_apf_count > 0 {
            ch_apf_sum / ch_apf_count as f32
        } else {
            0.0
        };

        // ── Band-Power Slope (BPS) ───────────────────────────────────────────
        let bps = ch_bps_sum / safe_nch;

        // ── SNR ──────────────────────────────────────────────────────────────
        let snr = ch_snr_sum / safe_nch;

        // ── Inter-channel alpha coherence (simplified) ───────────────────────
        // Pearson correlation of relative alpha across channel pairs.
        // With 4 channels: 6 pairs.
        let coherence = if ch_powers.len() >= 2 {
            let alphas: Vec<f32> = ch_powers.iter().map(|c| c.rel_alpha).collect();
            let n_pairs = ch_powers.len() * (ch_powers.len() - 1) / 2;
            let mean = alphas.iter().sum::<f32>() / alphas.len() as f32;
            let var  = alphas.iter().map(|a| (a - mean).powi(2)).sum::<f32>() / alphas.len() as f32;
            if var > 1e-12 {
                let mut r_sum = 0.0f32;
                for i in 0..alphas.len() {
                    for j in (i + 1)..alphas.len() {
                        r_sum += (alphas[i] - mean) * (alphas[j] - mean) / var;
                    }
                }
                (r_sum / n_pairs as f32 / alphas.len() as f32).clamp(-1.0, 1.0)
            } else {
                1.0 // all channels identical → perfect coherence
            }
        } else {
            0.0
        };

        // ── Mu suppression ───────────────────────────────────────────────────
        // EMA baseline of mean alpha power; ratio = current / baseline.
        let mean_alpha = ch_alpha_abs_sum / safe_nch;
        self.snapshot_count += 1;
        if self.snapshot_count <= 5 {
            // Warm-up: just accumulate baseline
            self.alpha_baseline = mean_alpha;
        } else {
            let ema_alpha = 0.02f32; // slow-moving baseline
            self.alpha_baseline = ema_alpha * mean_alpha
                + (1.0 - ema_alpha) * self.alpha_baseline;
        }
        let mu_suppression = if self.alpha_baseline > 1e-12 {
            (mean_alpha / self.alpha_baseline).clamp(0.0, 5.0)
        } else {
            1.0
        };

        // ── Mood index ───────────────────────────────────────────────────────
        // Weighted composite: FAA (approach), inverse TAR (alertness), BAR (focus).
        // Rescaled to 0–100.  Centre at 50.
        let faa_norm = (faa * 5.0).clamp(-1.0, 1.0);       // ±0.2 → ±1
        let tar_norm = 1.0 - tar.min(3.0) / 3.0;           // 0→1, 3→0
        let bar_norm = bar.min(3.0) / 3.0;                  // 0→0, 3→1
        let mood = (50.0 + 20.0 * faa_norm + 15.0 * (tar_norm - 0.5) + 15.0 * (bar_norm - 0.5))
            .clamp(0.0, 100.0);

        // ── TBR (absolute θ / β) ────────────────────────────────────────────
        let mut tbr_sum = 0.0f32;
        for ch in &ch_powers {
            let beta_safe = ch.beta.max(1e-12);
            tbr_sum += ch.theta / beta_safe;
        }
        let tbr = tbr_sum / safe_nch;

        // ── SEF95 & Spectral Centroid (averaged across channels) ─────────────
        let mut sef95_sum = 0.0f32;
        let mut sc_sum = 0.0f32;
        for (real, imag) in spectra.iter().take(EEG_CHANNELS) {
            let psd_raw = psd(&real[..n_oneside], &imag[..n_oneside]);
            let psd_scaled: Vec<f32> = psd_raw.iter().enumerate().map(|(k, &p)| {
                let factor = if k == 0 || k == nyq_bin { 1.0f32 } else { 2.0 };
                p * factor * abs_scale
            }).collect();
            sef95_sum += spectral_edge_freq(&psd_scaled, bin_hz, 0.95);
            sc_sum += spectral_centroid_fn(&psd_scaled, bin_hz);
        }
        let sef95 = sef95_sum / safe_nch;
        let spectral_centroid = sc_sum / safe_nch;

        // ── Time-domain metrics (averaged across channels) ───────────────────
        let mut ha_sum = 0.0f32; let mut hm_sum = 0.0f32; let mut hc_sum = 0.0f32;
        let mut pe_sum = 0.0f32; let mut hfd_sum = 0.0f32;
        let mut dfa_sum = 0.0f32; let mut se_sum = 0.0f32;
        let mut pac_sum = 0.0f32;
        for ch_idx in 0..EEG_CHANNELS {
            let raw: Vec<f32> = self.window[ch_idx].iter().copied().collect();
            let (ha, hm, hc) = hjorth_params(&raw);
            ha_sum += ha; hm_sum += hm; hc_sum += hc;
            pe_sum += permutation_entropy(&raw);
            hfd_sum += higuchi_fd(&raw);
            dfa_sum += dfa_exponent(&raw);
            se_sum += sample_entropy_fn(&raw);
            pac_sum += pac_theta_gamma_fn(&raw, self.sample_rate);
        }
        let hjorth_activity   = ha_sum / safe_nch;
        let hjorth_mobility   = hm_sum / safe_nch;
        let hjorth_complexity = hc_sum / safe_nch;
        let permutation_entropy_val = pe_sum / safe_nch;
        let higuchi_fd_val    = hfd_sum / safe_nch;
        let dfa_exponent_val  = dfa_sum / safe_nch;
        let sample_entropy_val = se_sum / safe_nch;
        let pac_theta_gamma   = pac_sum / safe_nch;

        // ── Laterality Index ─────────────────────────────────────────────────
        let laterality_index = laterality_index_fn(&ch_powers);

        // ── Consciousness Indices ─────────────────────────────
        // Mean relative band powers across channels (used throughout below)
        let mean_rel_alpha = ch_powers.iter().map(|c| c.rel_alpha).sum::<f32>() / safe_nch;
        let mean_rel_beta  = ch_powers.iter().map(|c| c.rel_beta ).sum::<f32>() / safe_nch;
        let _mean_rel_theta = ch_powers.iter().map(|c| c.rel_theta).sum::<f32>() / safe_nch;
        let mean_rel_delta = ch_powers.iter().map(|c| c.rel_delta).sum::<f32>() / safe_nch;
        let _mean_rel_gamma = ch_powers.iter().map(|c| c.rel_gamma).sum::<f32>() / safe_nch;

        // Clamp helper for [0, 1]
        let c01 = |x: f32| x.clamp(0.0_f32, 1.0_f32);

        // Drowsiness proxy (mirrors lib.rs formula; TAR-weighted + alpha spindle)
        let drowsiness_proxy = (tar / 3.0 * 80.0 + mean_rel_alpha * 20.0).clamp(0.0, 100.0);

        // Mean engagement β/(α+θ) → sigmoid → 0–100
        let mean_eng_raw = ch_powers.iter().map(|ch| {
            let d = ch.rel_alpha + ch.rel_theta;
            if d > 1e-6 { ch.rel_beta / d } else { 0.5 }
        }).sum::<f32>() / safe_nch;
        let _mean_engagement = 100.0_f32 / (1.0 + (-2.0 * (mean_eng_raw - 0.8)).exp());

        // ── Headache ──────────────────────────────────────────────────────────
        // Cortical hyperexcitability: elevated beta + suppressed alpha + high BAR.
        // Headache states show increased excitability and altered alpha generation.
        //
        let headache_index = (0.35 * c01(mean_rel_beta * 5.0)
            + 0.35 * (1.0 - c01(mean_rel_alpha * 4.0))
            + 0.30 * c01(bar / 2.5)) * 100.0;

        // ── Migraine ──────────────────────────────────────────────────────────
        // Cortical spreading depression proxy: elevated delta + alpha suppression
        // + hemispheric lateralisation.  Delta increases and alpha suppresses
        // during migraine attacks and in the interictal period.
        // Uses the same validated source as headache (interictal QEEG in migraine).
        //
        let migraine_index = (0.40 * c01(mean_rel_delta * 6.0)
            + 0.35 * (1.0 - c01(mean_rel_alpha * 4.0))
            + 0.25 * c01(laterality_index.abs() * 4.0)) * 100.0;

        // ── Consciousness: Lempel-Ziv Complexity proxy ─────────────────────────
        // Signal information richness approximated via permutation entropy
        // (ordinal pattern complexity) and Higuchi fractal dimension (fractal
        // complexity of the time series).  Higher = richer, more complex EEG.
        // LZC peaks in conscious states and collapses during NREM sleep / anaesthesia.
        //
        let consciousness_lzc = (0.60 * c01(permutation_entropy_val)
            + 0.40 * c01((higuchi_fd_val - 1.0).max(0.0) / 1.5)) * 100.0;

        // ── Consciousness: Wakefulness level ──────────────────────────────────
        // Inverse drowsiness modulated by BAR (alertness) and TAR (drowsiness).
        // High BAR + low TAR → high wakefulness; these ratios covary tightly with
        // EEG-defined arousal and alpha/theta dominance patterns.
        //
        let consciousness_wakefulness = (0.40 * c01(bar / 2.5)
            + 0.35 * (1.0 - c01(tar / 3.0))
            + 0.25 * (1.0 - c01(drowsiness_proxy / 100.0))) * 100.0;

        // ── Consciousness: Information Integration proxy ───────────────────────
        // Global workspace proxy: inter-channel coherence × theta–gamma PAC
        // (cross-frequency coupling) × spectral entropy.
        // Integrated brain states require both synchrony across regions and
        // complex intra-regional dynamics — captured here by three complementary
        // measures.
        //
        let consciousness_integration = (0.40 * c01(coherence * 2.5)
            + 0.40 * c01(pac_theta_gamma * 3.0)
            + 0.20 * c01(pse)) * 100.0;

        self.latest = Some(BandSnapshot {
            timestamp: now,
            channels: ch_powers,
            faa, tar, bar, dtr, pse, apf, bps, snr,
            coherence, mu_suppression, mood,
            tbr, sef95, spectral_centroid,
            hjorth_activity, hjorth_mobility, hjorth_complexity,
            permutation_entropy: permutation_entropy_val,
            higuchi_fd: higuchi_fd_val,
            dfa_exponent: dfa_exponent_val,
            sample_entropy: sample_entropy_val,
            pac_theta_gamma,
            laterality_index,
            headache_index, migraine_index,
            consciousness_lzc, consciousness_wakefulness, consciousness_integration,
            hr: None, rmssd: None, sdnn: None, pnn50: None,
            lf_hf_ratio: None, respiratory_rate: None, spo2_estimate: None,
            perfusion_index: None, stress_index: None,
            blink_count: None, blink_rate: None,
            head_pitch: None, head_roll: None, stillness: None,
            nod_count: None, shake_count: None,
            meditation: None, cognitive_load: None, drowsiness: None,
            temperature_raw: None,
            gpu_overall: None, gpu_render: None, gpu_tiler: None,
        });
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// Generate a pure sine wave at `freq_hz` Hz.
    fn sine(freq_hz: f64, n: usize) -> Vec<f64> {
        let sr = MUSE_SAMPLE_RATE as f64;
        (0..n)
            .map(|i| (2.0 * PI * freq_hz * i as f64 / sr).sin())
            .collect()
    }

    /// Push identical `signal` to all active channels interleaved (sample by
    /// sample, round-robin across channels) and return the latest snapshot.
    fn run(signal: &[f64]) -> BandSnapshot {
        run_n(EEG_CHANNELS, signal)
    }

    /// Push identical `signal` to `n_ch` channels interleaved.
    fn run_n(n_ch: usize, signal: &[f64]) -> BandSnapshot {
        let mut a = BandAnalyzer::new();
        // Push one sample at a time, round-robin, so all channels accumulate
        // equally and the batch fires with all channels populated.
        for i in 0..signal.len() {
            for ch in 0..n_ch {
                a.push(ch, &signal[i..i+1]);
            }
        }
        a.latest.expect("signal was long enough but snapshot is None")
    }

    // ── Constants sanity ──────────────────────────────────────────────────────

    #[test]
    fn band_window_is_power_of_two() {
        assert!(BAND_WINDOW.is_power_of_two(), "BAND_WINDOW must be a power of two for FFT");
    }

    #[test]
    fn band_hop_divides_band_window() {
        assert_eq!(BAND_WINDOW % BAND_HOP, 0);
    }

    #[test]
    fn num_bands_matches_arrays() {
        assert_eq!(BANDS.len(), NUM_BANDS);
        assert_eq!(BAND_COLORS.len(), NUM_BANDS);
        assert_eq!(BAND_SYMBOLS.len(), NUM_BANDS);
    }

    // ── Warmup ────────────────────────────────────────────────────────────────

    #[test]
    fn no_snapshot_before_full_window() {
        let mut a = BandAnalyzer::new();
        // BAND_WINDOW / BAND_HOP − 1 hops: not yet a full window
        let short = vec![0.0_f64; BAND_HOP * (BAND_WINDOW / BAND_HOP - 1)];
        for ch in 0..EEG_CHANNELS {
            a.push(ch, &short);
        }
        assert!(a.latest.is_none(), "snapshot must not be produced before window is full");
    }

    #[test]
    fn snapshot_produced_after_full_window() {
        let signal = vec![0.0_f64; BAND_WINDOW + BAND_HOP]; // one full window + one hop
        let mut a = BandAnalyzer::new();
        for ch in 0..EEG_CHANNELS {
            a.push(ch, &signal);
        }
        assert!(a.latest.is_some());
    }

    // ── Output shape ──────────────────────────────────────────────────────────

    #[test]
    fn snapshot_has_all_active_channels() {
        let s = run(&vec![0.0_f64; BAND_WINDOW * 2]);
        // run() pushes to all EEG_CHANNELS, so all should be in the snapshot.
        assert_eq!(s.channels.len(), EEG_CHANNELS);
    }

    #[test]
    fn channel_labels_are_correct() {
        let s = run(&vec![0.0_f64; BAND_WINDOW * 2]);
        // First 4 channels have CHANNEL_NAMES labels; rest get "Ch?" fallback.
        for (i, ch) in s.channels.iter().enumerate() {
            if i < CHANNEL_NAMES.len() {
                assert_eq!(ch.channel, CHANNEL_NAMES[i]);
            } else {
                assert_eq!(ch.channel, "Ch?");
            }
        }
    }

    #[test]
    fn relative_powers_sum_to_one() {
        // Feed a non-trivial signal so the spectrum is not all-zero.
        let signal = sine(10.0, BAND_WINDOW * 2);
        let s = run(&signal);
        for ch in &s.channels {
            let sum = ch.rel_delta + ch.rel_theta + ch.rel_alpha
                + ch.rel_beta + ch.rel_gamma + ch.rel_high_gamma;
            assert!(
                (sum - 1.0).abs() < 1e-4,
                "{}: relative powers sum to {sum}, expected 1.0", ch.channel
            );
        }
    }

    #[test]
    fn relative_powers_are_non_negative() {
        let signal = sine(10.0, BAND_WINDOW * 2);
        let s = run(&signal);
        for ch in &s.channels {
            assert!(ch.rel_delta      >= 0.0);
            assert!(ch.rel_theta      >= 0.0);
            assert!(ch.rel_alpha      >= 0.0);
            assert!(ch.rel_beta       >= 0.0);
            assert!(ch.rel_gamma      >= 0.0);
            assert!(ch.rel_high_gamma >= 0.0);
        }
    }

    // ── Spectral localisation ─────────────────────────────────────────────────

    /// A pure 10 Hz sine (alpha band) must show alpha as the dominant band
    /// and alpha relative power must far exceed the others.
    #[test]
    fn alpha_tone_dominates_alpha_band() {
        let signal = sine(10.0, BAND_WINDOW * 4); // 4 windows for good steady-state
        let s = run(&signal);
        for ch in &s.channels {
            assert_eq!(
                ch.dominant, "alpha",
                "{}: expected alpha dominant, got '{}'", ch.channel, ch.dominant
            );
            assert!(
                ch.rel_alpha > 0.8,
                "{}: rel_alpha = {:.3}, expected > 0.8", ch.channel, ch.rel_alpha
            );
        }
    }

    /// A pure 20 Hz sine (beta band) must dominate beta.
    #[test]
    fn beta_tone_dominates_beta_band() {
        let signal = sine(20.0, BAND_WINDOW * 4);
        let s = run(&signal);
        for ch in &s.channels {
            assert_eq!(ch.dominant, "beta", "{}: dominant = '{}'", ch.channel, ch.dominant);
            assert!(
                ch.rel_beta > 0.8,
                "{}: rel_beta = {:.3}", ch.channel, ch.rel_beta
            );
        }
    }

    /// A pure 6 Hz sine (theta band) must dominate theta.
    #[test]
    fn theta_tone_dominates_theta_band() {
        let signal = sine(6.0, BAND_WINDOW * 4);
        let s = run(&signal);
        for ch in &s.channels {
            assert_eq!(ch.dominant, "theta", "{}: dominant = '{}'", ch.channel, ch.dominant);
            assert!(ch.rel_theta > 0.7, "{}: rel_theta = {:.3}", ch.channel, ch.rel_theta);
        }
    }

    /// A pure 2 Hz sine (delta band) must dominate delta.
    #[test]
    fn delta_tone_dominates_delta_band() {
        let signal = sine(2.0, BAND_WINDOW * 4);
        let s = run(&signal);
        for ch in &s.channels {
            assert_eq!(ch.dominant, "delta", "{}: dominant = '{}'", ch.channel, ch.dominant);
            assert!(ch.rel_delta > 0.7, "{}: rel_delta = {:.3}", ch.channel, ch.rel_delta);
        }
    }

    /// A pure 40 Hz sine (gamma band) must dominate gamma.
    #[test]
    fn gamma_tone_dominates_gamma_band() {
        let signal = sine(40.0, BAND_WINDOW * 4);
        let s = run(&signal);
        for ch in &s.channels {
            assert_eq!(ch.dominant, "gamma", "{}: dominant = '{}'", ch.channel, ch.dominant);
            assert!(ch.rel_gamma > 0.7, "{}: rel_gamma = {:.3}", ch.channel, ch.rel_gamma);
        }
    }

    /// A pure 75 Hz sine (high_gamma band) must dominate high_gamma.
    #[test]
    fn high_gamma_tone_dominates_high_gamma_band() {
        let signal = sine(75.0, BAND_WINDOW * 4);
        let s = run(&signal);
        for ch in &s.channels {
            assert_eq!(
                ch.dominant, "high_gamma",
                "{}: dominant = '{}'", ch.channel, ch.dominant
            );
            assert!(
                ch.rel_high_gamma > 0.7,
                "{}: rel_high_gamma = {:.3}", ch.channel, ch.rel_high_gamma
            );
        }
    }

    // ── All four channels processed in each batch ─────────────────────────────

    /// Push different tones to different channels and verify each channel's
    /// dominant band matches its input — confirms the batch doesn't mix channels.
    #[test]
    fn batched_fft_isolates_channels() {
        let n = BAND_WINDOW * 4;
        // ch0 → alpha (10 Hz), ch1 → beta (20 Hz),
        // ch2 → theta (6 Hz),  ch3 → delta (2 Hz)
        let signals = [
            sine(10.0, n),
            sine(20.0, n),
            sine(6.0,  n),
            sine(2.0,  n),
        ];
        let expected = ["alpha", "beta", "theta", "delta"];

        let mut a = BandAnalyzer::new();
        // Push interleaved so all 4 channels accumulate equally.
        for i in 0..n {
            for (ch, sig) in signals.iter().enumerate() {
                a.push(ch, &sig[i..i+1]);
            }
        }
        let s = a.latest.expect("snapshot should exist");
        let freqs = [10, 20, 6, 2];
        for (ch, exp) in expected.iter().enumerate() {
            assert_eq!(
                s.channels[ch].dominant, *exp,
                "ch{ch} ({} Hz): expected '{exp}', got '{}'",
                freqs[ch], s.channels[ch].dominant
            );
        }
    }

    // ── Dominant metadata ─────────────────────────────────────────────────────

    #[test]
    fn dominant_symbol_matches_dominant_name() {
        let signal = sine(10.0, BAND_WINDOW * 4);
        let s = run(&signal);
        for ch in &s.channels {
            // alpha → "α"
            assert_eq!(ch.dominant_symbol, "α",
                "{}: symbol '{}' doesn't match dominant '{}'",
                ch.channel, ch.dominant_symbol, ch.dominant);
        }
    }

    #[test]
    fn dominant_color_is_non_empty_hex() {
        let signal = sine(10.0, BAND_WINDOW * 4);
        let s = run(&signal);
        for ch in &s.channels {
            assert!(ch.dominant_color.starts_with('#'));
            assert_eq!(ch.dominant_color.len(), 7); // #RRGGBB
        }
    }

    // ── Edge cases ────────────────────────────────────────────────────────────

    #[test]
    fn zero_signal_produces_valid_relative_powers() {
        // All-zero input: the PSD is zero → safe_total = 1.0 → all rel = 0.
        let signal = vec![0.0_f64; BAND_WINDOW * 2];
        let s = run(&signal);
        for ch in &s.channels {
            let sum = ch.rel_delta + ch.rel_theta + ch.rel_alpha
                + ch.rel_beta + ch.rel_gamma + ch.rel_high_gamma;
            // All zero → sum = 0, not 1 (zero signal has no valid distribution)
            assert!(sum.is_finite(), "{}: sum is not finite", ch.channel);
        }
    }

    #[test]
    fn invalid_channel_does_not_panic() {
        let mut a = BandAnalyzer::new();
        let fired = a.push(99, &[1.0, 2.0]);
        assert!(!fired);
        assert!(a.latest.is_none());
    }

    #[test]
    fn empty_push_does_not_panic() {
        let mut a = BandAnalyzer::new();
        let fired = a.push(0, &[]);
        assert!(!fired);
    }

    #[test]
    fn reset_clears_all_state() {
        let signal = vec![1.0_f64; BAND_WINDOW * 2];
        let mut a = BandAnalyzer::new();
        for ch in 0..EEG_CHANNELS { a.push(ch, &signal); }
        assert!(a.latest.is_some());
        a.reset();
        assert!(a.latest.is_none());
        for ch in 0..EEG_CHANNELS {
            assert!(a.window[ch].is_empty(), "window[{ch}] not cleared");
            assert!(a.queued[ch].is_empty(), "queued[{ch}] not cleared");
        }
    }

    // ── Multiple hops ─────────────────────────────────────────────────────────

    #[test]
    fn multiple_hops_update_latest() {
        let n = BAND_WINDOW * 6;
        let signal = sine(10.0, n);
        let mut a = BandAnalyzer::new();
        for ch in 0..EEG_CHANNELS { a.push(ch, &signal); }
        // Every hop after the first full window should update `latest`.
        let ts1 = a.latest.as_ref().unwrap().timestamp;

        // Push one more hop to all channels.
        let extra = sine(10.0, BAND_HOP);
        // Short sleep so the wall-clock timestamp can advance.
        std::thread::sleep(std::time::Duration::from_millis(5));
        for ch in 0..EEG_CHANNELS { a.push(ch, &extra); }

        let ts2 = a.latest.as_ref().unwrap().timestamp;
        assert!(ts2 >= ts1, "timestamp should advance with each snapshot");
    }
}
