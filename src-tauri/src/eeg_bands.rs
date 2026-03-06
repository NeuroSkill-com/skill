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
}

impl BandAnalyzer {
    /// Create a new analyser with pre-computed Hann coefficients.
    pub fn new() -> Self {
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
            hann,
            hann_sum_sq,
            latest: None,
            alpha_baseline: 0.0,
            snapshot_count: 0,
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
        for &v in samples {
            self.queued[channel].push_back(v as f32);
        }
        // Fire one or more hops while every channel has ≥ BAND_HOP queued samples.
        let mut fired = false;
        while self.queued.iter().all(|q| q.len() >= BAND_HOP) {
            self.compute_snapshot();
            fired = true;
        }
        fired
    }

    /// Clear all buffers.  Called on disconnect so the next session starts fresh.
    pub fn reset(&mut self) {
        for ch in 0..EEG_CHANNELS {
            self.window[ch].clear();
            self.queued[ch].clear();
        }
        self.latest = None;
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
        let bin_hz      = MUSE_SAMPLE_RATE / n as f32; // 0.5 Hz/bin
        let abs_scale   = 1.0 / self.hann_sum_sq;      // PSD normalisation

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        let mut ch_powers: Vec<BandPowers> = Vec::with_capacity(EEG_CHANNELS);
        // Per-channel alpha peak freq accumulator and PSD-derived data for
        // cross-channel metrics computed after the loop.
        let mut ch_apf_sum   = 0.0f32;
        let mut ch_apf_count = 0u32;
        let mut ch_bps_sum   = 0.0f32;
        let mut ch_snr_sum   = 0.0f32;
        let mut ch_alpha_abs_sum = 0.0f32;

        for ch in 0..EEG_CHANNELS {
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
                channel:        CHANNEL_NAMES[ch].to_string(),
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
        let faa = if ch_powers.len() >= 3 {
            let af7_alpha = ch_powers[1].alpha.max(1e-6);
            let af8_alpha = ch_powers[2].alpha.max(1e-6);
            af8_alpha.ln() - af7_alpha.ln()
        } else {
            0.0
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
            pac_sum += pac_theta_gamma_fn(&raw, MUSE_SAMPLE_RATE);
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

// ── Helper functions for new metrics ──────────────────────────────────────────

/// Spectral Edge Frequency: frequency below which `pct` (0–1) of total power lies.
fn spectral_edge_freq(psd: &[f32], bin_hz: f32, pct: f32) -> f32 {
    let total: f32 = psd.iter().sum();
    if total < 1e-20 { return 0.0; }
    let threshold = total * pct;
    let mut cum = 0.0f32;
    for (k, &p) in psd.iter().enumerate() {
        cum += p;
        if cum >= threshold { return k as f32 * bin_hz; }
    }
    (psd.len() - 1) as f32 * bin_hz
}

/// Spectral Centroid: weighted mean frequency.
fn spectral_centroid_fn(psd: &[f32], bin_hz: f32) -> f32 {
    let mut num = 0.0f32;
    let mut den = 0.0f32;
    for (k, &p) in psd.iter().enumerate() {
        let f = k as f32 * bin_hz;
        num += f * p;
        den += p;
    }
    if den > 1e-20 { num / den } else { 0.0 }
}

/// Hjorth parameters: (activity, mobility, complexity).
fn hjorth_params(x: &[f32]) -> (f32, f32, f32) {
    let n = x.len();
    if n < 3 { return (0.0, 0.0, 0.0); }
    let mean = x.iter().sum::<f32>() / n as f32;
    let var0 = x.iter().map(|&v| (v - mean).powi(2)).sum::<f32>() / n as f32;
    if var0 < 1e-20 { return (0.0, 0.0, 0.0); }
    // First derivative
    let mut dx = Vec::with_capacity(n - 1);
    for i in 1..n { dx.push(x[i] - x[i - 1]); }
    let dm = dx.iter().sum::<f32>() / dx.len() as f32;
    let var1 = dx.iter().map(|&v| (v - dm).powi(2)).sum::<f32>() / dx.len() as f32;
    let mobility = (var1 / var0).sqrt();
    // Second derivative
    let mut ddx = Vec::with_capacity(dx.len() - 1);
    for i in 1..dx.len() { ddx.push(dx[i] - dx[i - 1]); }
    let ddm = ddx.iter().sum::<f32>() / ddx.len().max(1) as f32;
    let var2 = ddx.iter().map(|&v| (v - ddm).powi(2)).sum::<f32>() / ddx.len().max(1) as f32;
    let mob_dx = if var1 > 1e-20 { (var2 / var1).sqrt() } else { 0.0 };
    let complexity = if mobility > 1e-10 { mob_dx / mobility } else { 0.0 };
    (var0, mobility, complexity)
}

/// Permutation Entropy (order m=3, delay τ=1), normalised to [0,1].
fn permutation_entropy(x: &[f32]) -> f32 {
    const M: usize = 3;
    let n = x.len();
    if n < M { return 0.0; }
    // 3! = 6 possible patterns
    let mut counts = [0u32; 6];
    for i in 0..=(n - M) {
        let (a, b, c) = (x[i], x[i + 1], x[i + 2]);
        let pat = if a < b {
            if b < c { 0 } else if a < c { 1 } else { 2 }
        } else if a < c { 3 } else if b < c { 4 } else { 5 };
        counts[pat] += 1;
    }
    let total = counts.iter().sum::<u32>() as f32;
    if total < 1.0 { return 0.0; }
    let mut h = 0.0f32;
    for &c in &counts {
        if c > 0 {
            let p = c as f32 / total;
            h -= p * p.ln();
        }
    }
    h / (6.0f32).ln() // normalise by ln(m!)
}

/// Higuchi Fractal Dimension (k_max=8).
fn higuchi_fd(x: &[f32]) -> f32 {
    let n = x.len();
    let k_max = 8.min(n / 4);
    if k_max < 2 { return 0.0; }
    let mut log_k = Vec::with_capacity(k_max);
    let mut log_l = Vec::with_capacity(k_max);
    for k in 1..=k_max {
        let mut lk = 0.0f64;
        let mut count = 0u32;
        for m in 0..k {
            let mut l_m = 0.0f64;
            let floor_n = (n - 1 - m) / k;
            if floor_n < 1 { continue; }
            for i in 1..=floor_n {
                l_m += (x[m + i * k] as f64 - x[m + (i - 1) * k] as f64).abs();
            }
            l_m *= (n as f64 - 1.0) / (floor_n as f64 * k as f64 * k as f64);
            lk += l_m;
            count += 1;
        }
        if count > 0 {
            lk /= count as f64;
            if lk > 1e-20 {
                log_k.push((1.0 / k as f64).ln());
                log_l.push(lk.ln());
            }
        }
    }
    if log_k.len() < 2 { return 0.0; }
    // Linear regression slope
    lin_reg_slope(&log_k, &log_l) as f32
}

/// DFA scaling exponent.
fn dfa_exponent(x: &[f32]) -> f32 {
    let n = x.len();
    if n < 16 { return 0.0; }
    let mean = x.iter().sum::<f32>() / n as f32;
    // Cumulative sum of deviations
    let mut y = vec![0.0f64; n];
    y[0] = (x[0] - mean) as f64;
    for i in 1..n { y[i] = y[i - 1] + (x[i] - mean) as f64; }
    // Scales: powers of 2 from 4 to n/2
    let mut scales = Vec::new();
    let mut s = 4usize;
    while s <= n / 2 {
        scales.push(s);
        s *= 2;
    }
    if scales.len() < 2 { return 0.0; }
    let mut log_s = Vec::with_capacity(scales.len());
    let mut log_f = Vec::with_capacity(scales.len());
    for &seg_len in &scales {
        let n_seg = n / seg_len;
        if n_seg < 1 { continue; }
        let mut total_var = 0.0f64;
        let mut seg_count = 0u32;
        for seg in 0..n_seg {
            let start = seg * seg_len;
            // Linear detrend within segment
            let mut sx = 0.0f64; let mut sy = 0.0f64;
            let mut sxy = 0.0f64; let mut sx2 = 0.0f64;
            for j in 0..seg_len {
                let xj = j as f64;
                let yj = y[start + j];
                sx += xj; sy += yj; sxy += xj * yj; sx2 += xj * xj;
            }
            let nn = seg_len as f64;
            let denom = nn * sx2 - sx * sx;
            if denom.abs() < 1e-20 { continue; }
            let slope = (nn * sxy - sx * sy) / denom;
            let intercept = (sy - slope * sx) / nn;
            let mut var = 0.0f64;
            for j in 0..seg_len {
                let trend = intercept + slope * j as f64;
                let residual = y[start + j] - trend;
                var += residual * residual;
            }
            total_var += var / nn;
            seg_count += 1;
        }
        if seg_count > 0 {
            let f_n = (total_var / seg_count as f64).sqrt();
            if f_n > 1e-20 {
                log_s.push((seg_len as f64).ln());
                log_f.push(f_n.ln());
            }
        }
    }
    if log_s.len() < 2 { return 0.0; }
    lin_reg_slope(&log_s, &log_f) as f32
}

/// Sample Entropy (m=2, r=0.2*std).
fn sample_entropy_fn(x: &[f32]) -> f32 {
    let n = x.len();
    let m = 2usize;
    if n < m + 2 { return 0.0; }
    let mean = x.iter().sum::<f32>() / n as f32;
    let std = (x.iter().map(|&v| (v - mean).powi(2)).sum::<f32>() / n as f32).sqrt();
    let r = 0.2 * std;
    if r < 1e-10 { return 0.0; }
    // Count template matches
    let mut b_count = 0u64; // matches of length m
    let mut a_count = 0u64; // matches of length m+1
    for i in 0..(n - m) {
        for j in (i + 1)..(n - m) {
            // Check length m match
            let mut match_m = true;
            for k in 0..m {
                if (x[i + k] - x[j + k]).abs() > r { match_m = false; break; }
            }
            if match_m {
                b_count += 1;
                // Check length m+1
                if (x[i + m] - x[j + m]).abs() <= r {
                    a_count += 1;
                }
            }
        }
    }
    if b_count == 0 { return 0.0; }
    if a_count == 0 { return (b_count as f32).ln(); } // convention: large value
    -((a_count as f32) / (b_count as f32)).ln()
}

/// Phase-Amplitude Coupling (θ–γ) via sub-window power correlation.
/// Splits the signal into overlapping sub-windows, computes theta and gamma
/// band power in each using Goertzel, then returns the Pearson correlation.
fn pac_theta_gamma_fn(x: &[f32], sr: f32) -> f32 {
    let n = x.len();
    let sub_len = 128.min(n);
    let hop = sub_len / 2;
    if n < sub_len { return 0.0; }
    let n_subs = (n - sub_len) / hop + 1;
    if n_subs < 3 { return 0.0; }
    let mut theta_pwr = Vec::with_capacity(n_subs);
    let mut gamma_pwr = Vec::with_capacity(n_subs);
    // Target frequencies for Goertzel
    let theta_freqs: &[f32] = &[4.0, 5.0, 6.0, 7.0, 8.0];
    let gamma_freqs: &[f32] = &[30.0, 35.0, 40.0, 45.0, 50.0];
    for s in 0..n_subs {
        let start = s * hop;
        let sub = &x[start..start + sub_len];
        let tp: f32 = theta_freqs.iter().map(|&f| goertzel_power(sub, sr, f)).sum();
        let gp: f32 = gamma_freqs.iter().map(|&f| goertzel_power(sub, sr, f)).sum();
        theta_pwr.push(tp);
        gamma_pwr.push(gp);
    }
    pearson(&theta_pwr, &gamma_pwr).abs()
}

/// Goertzel algorithm: power at a single frequency.
fn goertzel_power(x: &[f32], sr: f32, freq: f32) -> f32 {
    let n = x.len();
    let k = (freq * n as f32 / sr).round();
    let w = 2.0 * std::f32::consts::PI * k / n as f32;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &sample in x {
        let s0 = sample + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - coeff * s1 * s2
}

/// Pearson correlation coefficient.
fn pearson(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len() as f32;
    if n < 2.0 { return 0.0; }
    let ma = a.iter().sum::<f32>() / n;
    let mb = b.iter().sum::<f32>() / n;
    let mut cov = 0.0f32;
    let mut va = 0.0f32;
    let mut vb = 0.0f32;
    for i in 0..a.len() {
        let da = a[i] - ma;
        let db = b[i] - mb;
        cov += da * db;
        va += da * da;
        vb += db * db;
    }
    let denom = (va * vb).sqrt();
    if denom > 1e-12 { cov / denom } else { 0.0 }
}

/// Laterality Index: generalised L/R asymmetry.
/// Uses total broadband power: (right − left) / (right + left).
/// TP9 (left), AF7 (left), AF8 (right), TP10 (right).
fn laterality_index_fn(ch: &[BandPowers]) -> f32 {
    if ch.len() < 4 { return 0.0; }
    let left  = (ch[0].delta + ch[0].theta + ch[0].alpha + ch[0].beta + ch[0].gamma)
              + (ch[1].delta + ch[1].theta + ch[1].alpha + ch[1].beta + ch[1].gamma);
    let right = (ch[2].delta + ch[2].theta + ch[2].alpha + ch[2].beta + ch[2].gamma)
              + (ch[3].delta + ch[3].theta + ch[3].alpha + ch[3].beta + ch[3].gamma);
    let total = left + right;
    if total > 1e-12 { (right - left) / total } else { 0.0 }
}

/// Simple linear regression slope.
fn lin_reg_slope(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    if n < 2.0 { return 0.0; }
    let sx: f64 = x.iter().sum();
    let sy: f64 = y.iter().sum();
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| a * b).sum();
    let sx2: f64 = x.iter().map(|a| a * a).sum();
    let denom = n * sx2 - sx * sx;
    if denom.abs() < 1e-20 { 0.0 } else { (n * sxy - sx * sy) / denom }
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

    /// Push identical `signal` to all 4 channels and return the latest snapshot.
    fn run(signal: &[f64]) -> BandSnapshot {
        let mut a = BandAnalyzer::new();
        for ch in 0..EEG_CHANNELS {
            a.push(ch, signal);
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
    fn snapshot_has_four_channels() {
        let s = run(&vec![0.0_f64; BAND_WINDOW * 2]);
        assert_eq!(s.channels.len(), EEG_CHANNELS);
    }

    #[test]
    fn channel_labels_are_correct() {
        let s = run(&vec![0.0_f64; BAND_WINDOW * 2]);
        let labels: Vec<&str> = s.channels.iter().map(|c| c.channel.as_str()).collect();
        assert_eq!(labels, CHANNEL_NAMES);
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
        for (ch, sig) in signals.iter().enumerate().take(EEG_CHANNELS) {
            a.push(ch, sig);
        }
        let s = a.latest.expect("snapshot should exist");
        let freqs = [10, 20, 6, 2];
        for (ch, exp) in expected.iter().enumerate().take(EEG_CHANNELS) {
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
