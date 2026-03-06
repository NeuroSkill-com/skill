// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! GPU-accelerated EEG signal filtering using the `gpu-fft` crate (wgpu backend).
//!
//! ## Algorithm: Overlap-Save
//!
//! The overlap-save (OLS) method is the standard way to apply an FIR filter to a
//! continuous stream of samples using the FFT.  It avoids the circular-convolution
//! artefacts that arise when naïvely zero-padding each block:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │           WINDOW = 256 samples (power-of-two)               │
//! │  ┌──────────────────────────┬────────────────────────┐      │
//! │  │   OVERLAP = 224 samples  │   HOP = 32 new samples │      │
//! │  │   (kept from prev. hop)  │   (just arrived)       │      │
//! │  └──────────────────────────┴────────────────────────┘      │
//! │                      GPU FFT ↓                               │
//! │              Frequency mask (LP + HP + notch) ↓             │
//! │                     GPU IFFT ↓                               │
//! │  ┌──────────────────────────┬────────────────────────┐      │
//! │  │   first OVERLAP samples  │   last HOP samples     │      │
//! │  │   (circular artefacts,   │   (valid output ✓)     │      │
//! │  │   discarded)             │                        │      │
//! │  └──────────────────────────┴────────────────────────┘      │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Batch GPU execution
//!
//! All **4 Muse EEG channels** (TP9, AF7, AF8, TP10) are processed in a
//! single `fft_batch` call and a single `ifft_batch` call per hop — the GPU
//! kernel covers the entire `4 × 256` sample matrix in one dispatch, so there
//! is no per-channel kernel-launch overhead.
//!
//! ## Timing
//!
//! | Parameter | Value |
//! |-----------|-------|
//! | Sample rate | 256 Hz |
//! | Window (W) | 256 samples = 1 s |
//! | Hop (H) | 32 samples ≈ 125 ms |
//! | Freq. resolution | 256 Hz / 256 = **1 Hz / bin** |
//! | Processing latency | ≈ **125 ms** (one hop) |
//!
//! ## Filter stages (applied together in one spectral mask pass)
//!
//! 1. **High-pass** — removes DC drift and slow baseline wander (default 0.5 Hz)
//! 2. **Low-pass**  — removes EMG artefacts and alias noise (default 50 Hz)
//! 3. **Notch**     — removes powerline interference at the fundamental and
//!    every harmonic that falls within the Nyquist limit:
//!    - **US preset** (60 Hz): zeroes bins at 60 Hz and 120 Hz
//!    - **EU preset** (50 Hz): zeroes bins at 50 Hz and 100 Hz
//!    - A configurable bandwidth (default ±1 Hz) is removed around each harmonic.

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use gpu_fft::{fft_batch, ifft_batch, psd::psd as one_sided_psd};
use serde::{Deserialize, Serialize};

use crate::constants::{
    DEFAULT_HP_HZ, DEFAULT_LP_HZ, DEFAULT_NOTCH_BW_HZ,
    EEG_CHANNELS, MUSE_SAMPLE_RATE, SPEC_N_FREQ,
};

// Re-export filter-window aliases so internal code (and test modules that do
// `use super::*`) can reference WINDOW / HOP / OVERLAP without the FILTER_ prefix.
pub(crate) use crate::constants::FILTER_HOP     as HOP;
pub(crate) use crate::constants::FILTER_OVERLAP  as OVERLAP;
pub(crate) use crate::constants::FILTER_WINDOW   as WINDOW;

/// One spectrogram time-slice: raw PSD for all 4 channels at one hop.
///
/// Produced inside [`EegFilter::process_one_hop`] as a zero-cost side-effect
/// of the FFT that is already being run for the signal filter.  The PSD is
/// sampled *before* the LP/HP/notch mask so the spectrogram shows the true
/// raw spectrum, not the filtered one.
///
/// Sent to the frontend as a `"eeg-spectrogram"` Tauri broadcast event at
/// the filter's hop rate: `HOP / sample_rate = 32 / 256 = 125 ms` (8 Hz).
#[derive(Clone, Serialize)]
pub struct SpectrogramColumn {
    /// Wall-clock timestamp of the last sample in this hop (milliseconds,
    /// matching the `EegPacket.timestamp` convention).
    pub timestamp_ms: f64,

    /// Raw one-sided PSD (µV²/bin, not normalised) for each channel.
    ///
    /// Layout: `power[channel_index][freq_bin]`
    /// - `channel_index` 0–3 → TP9, AF7, AF8, TP10
    /// - `freq_bin` 0–50   → 0 Hz … 50 Hz (1 Hz per bin)
    pub power: Vec<Vec<f32>>,
}

// ── PowerlineFreq ─────────────────────────────────────────────────────────────

/// Mains power frequency standard for the notch filter.
///
/// The notch is applied at the fundamental **and every harmonic** that falls
/// within the Nyquist limit (128 Hz at a 256 Hz sample rate):
///
/// | Preset | Fundamental | 2nd harmonic | Regions |
/// |--------|-------------|--------------|---------|
/// | `Hz60` | 60 Hz       | 120 Hz       | US, Canada, Mexico, Japan (partly) |
/// | `Hz50` | 50 Hz       | 100 Hz       | Europe, UK, Asia, Australia, Africa |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerlineFreq {
    /// 60 Hz powerline — North America default.
    Hz60,
    /// 50 Hz powerline — European / international standard.
    Hz50,
}

impl PowerlineFreq {
    /// Returns the fundamental notch frequency in Hz.
    #[inline]
    pub fn hz(self) -> f32 {
        match self {
            Self::Hz60 => 60.0,
            Self::Hz50 => 50.0,
        }
    }

    /// Display label shown in the UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Hz60 => "US — 60 Hz",
            Self::Hz50 => "EU — 50 Hz",
        }
    }
}

impl Default for PowerlineFreq {
    /// US (60 Hz) is the default as specified.
    fn default() -> Self {
        Self::Hz60
    }
}

// ── FilterConfig ──────────────────────────────────────────────────────────────

/// Complete EEG filter configuration.
///
/// All stages are optional: a field set to `None` disables that stage.
/// Setting **all** stages to `None` / `Off` makes the filter a passthrough —
/// no GPU work is performed and raw samples are forwarded without modification.
///
/// ### Typical EEG band presets
///
/// | Band   | `high_pass_hz` | `low_pass_hz` |
/// |--------|----------------|---------------|
/// | Full   | `Some(0.5)`    | `Some(50.0)`  |
/// | Delta  | `Some(0.5)`    | `Some(4.0)`   |
/// | Theta  | `Some(4.0)`    | `Some(8.0)`   |
/// | Alpha  | `Some(8.0)`    | `Some(13.0)`  |
/// | Beta   | `Some(13.0)`   | `Some(30.0)`  |
/// | Gamma  | `Some(30.0)`   | `Some(50.0)`  |
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FilterConfig {
    /// EEG sample rate in Hz.  Should always be [`MUSE_SAMPLE_RATE`] (256 Hz).
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f32,

    /// Low-pass cut-off in Hz.
    ///
    /// Bins whose frequency exceeds this value are zeroed.  `None` disables
    /// the low-pass stage.
    pub low_pass_hz: Option<f32>,

    /// High-pass cut-off in Hz.
    ///
    /// Bins (including DC, i.e. 0 Hz) whose frequency is strictly below this
    /// value are zeroed.  `None` disables the high-pass stage.
    pub high_pass_hz: Option<f32>,

    /// Powerline notch filter preset.
    ///
    /// When `Some`, zeroes the fundamental frequency and every harmonic up to
    /// Nyquist within a ±[`notch_bandwidth_hz`] band.  `None` disables the
    /// notch entirely.
    pub notch: Option<PowerlineFreq>,

    /// Half-width of each notch band in Hz (default `1.0`).
    ///
    /// The zeroed band around a harmonic `h` is `[h − bw, h + bw]`.
    /// With 1 Hz/bin resolution a bandwidth of 1.0 removes 3 bins per harmonic.
    #[serde(default = "default_notch_bw")]
    pub notch_bandwidth_hz: f32,
}

fn default_sample_rate() -> f32 {
    MUSE_SAMPLE_RATE
}
fn default_notch_bw() -> f32 {
    DEFAULT_NOTCH_BW_HZ
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            sample_rate:        MUSE_SAMPLE_RATE,
            low_pass_hz:        Some(DEFAULT_LP_HZ),       // remove EMG / alias noise
            high_pass_hz:       Some(DEFAULT_HP_HZ),       // remove DC drift
            notch:              Some(PowerlineFreq::Hz60), // US 60 Hz default
            notch_bandwidth_hz: DEFAULT_NOTCH_BW_HZ,       // ±BW Hz around each harmonic
        }
    }
}

impl FilterConfig {
    /// Returns `true` when at least one filter stage is enabled.
    ///
    /// When `false`, the filter is a passthrough and no GPU work is done.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.low_pass_hz.is_some()
            || self.high_pass_hz.is_some()
            || self.notch.is_some()
    }

    /// Convenience constructor for a full-band pass (0.5 – 50 Hz) + US notch.
    pub fn full_band_us() -> Self {
        Self::default()
    }

    /// Convenience constructor for a full-band pass (0.5 – 50 Hz) + EU notch.
    pub fn full_band_eu() -> Self {
        Self { notch: Some(PowerlineFreq::Hz50), ..Self::default() }
    }

    /// Passthrough: no filtering, raw samples are forwarded as-is.
    pub fn passthrough() -> Self {
        Self {
            sample_rate:        MUSE_SAMPLE_RATE,
            low_pass_hz:        None,
            high_pass_hz:       None,
            notch:              None,
            notch_bandwidth_hz: DEFAULT_NOTCH_BW_HZ,
        }
    }
}

// ── EegFilter ─────────────────────────────────────────────────────────────────

/// Real-time EEG filter for all 4 Muse channels using GPU-accelerated
/// overlap-save convolution.
///
/// ### Usage
///
/// ```rust,ignore
/// let mut filter = EegFilter::new(FilterConfig::default());
///
/// // Called once per incoming Muse EEG packet:
/// if filter.push(electrode, &raw_samples) {
///     // Filtered output is ready for all channels.
///     for ch in 0..EEG_CHANNELS {
///         let filtered: Vec<f64> = filter.drain(ch);
///         // … forward to frontend …
///     }
/// }
/// ```
pub struct EegFilter {
    /// Active filter configuration (readable for status reporting).
    pub config: FilterConfig,

    /// Per-channel overlap buffer: the last `OVERLAP` samples of the previous
    /// input window (initialised to silence / zero).
    overlap: [[f32; OVERLAP]; EEG_CHANNELS],

    /// Per-channel accumulator for incoming (unprocessed) samples.
    queued: [VecDeque<f32>; EEG_CHANNELS],

    /// Per-channel queue of filtered output samples ready for consumption.
    pending: [VecDeque<f32>; EEG_CHANNELS],

    /// Most recent spectrogram column, produced as a side-effect of each
    /// `fft_batch` call.  Taken (and cleared) by the caller via
    /// [`EegFilter::take_spec_col`] after every [`EegFilter::push`] call that
    /// returns `true`.
    pub latest_spec_col: Option<SpectrogramColumn>,
}

impl EegFilter {
    /// Create a new filter.  All overlap buffers start as silence (zeros).
    pub fn new(config: FilterConfig) -> Self {
        Self {
            config,
            overlap:         [[0.0f32; OVERLAP]; EEG_CHANNELS],
            queued:          std::array::from_fn(|_| VecDeque::new()),
            pending:         std::array::from_fn(|_| VecDeque::new()),
            latest_spec_col: None,
        }
    }

    /// Take the most recent spectrogram column produced by the last GPU batch,
    /// leaving `None` in its place.  Returns `None` before the first full hop,
    /// in passthrough mode, or if called twice without an intervening hop.
    pub fn take_spec_col(&mut self) -> Option<SpectrogramColumn> {
        self.latest_spec_col.take()
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Queue new µV samples for `channel` (0 = TP9, 1 = AF7, 2 = AF8, 3 = TP10).
    ///
    /// Internally converts `f64 → f32` for GPU processing.
    ///
    /// Returns `true` if at least one full GPU batch was triggered, meaning
    /// [`drain`][Self::drain] will return non-empty data for at least one channel.
    ///
    /// **Passthrough mode** (all stages disabled): samples skip the GPU entirely
    /// and land in `pending` immediately — `push` always returns `true`.
    pub fn push(&mut self, channel: usize, samples: &[f64]) -> bool {
        if channel >= EEG_CHANNELS || samples.is_empty() {
            return false;
        }

        if !self.config.is_active() {
            // Fast path: no GPU work, store directly.
            for &v in samples {
                self.pending[channel].push_back(v as f32);
            }
            return true;
        }

        for &v in samples {
            self.queued[channel].push_back(v as f32);
        }

        // Fire one or more GPU hops while every channel holds ≥ HOP samples.
        let mut fired = false;
        while self.queued.iter().all(|q| q.len() >= HOP) {
            self.process_one_hop();
            fired = true;
        }
        fired
    }

    /// Drain all pending filtered µV samples for `channel` as `f64`.
    ///
    /// Call immediately after [`push`][Self::push] returns `true`.
    pub fn drain(&mut self, channel: usize) -> Vec<f64> {
        self.pending[channel]
            .drain(..)
            .map(|v| v as f64)
            .collect()
    }

    /// Number of filtered samples waiting in the queue for `channel`.
    pub fn pending_len(&self, channel: usize) -> usize {
        self.pending[channel].len()
    }

    /// Replace the filter configuration.
    ///
    /// All internal buffers are cleared so stale overlap data from the old
    /// configuration cannot contaminate the output of the new one.
    pub fn set_config(&mut self, config: FilterConfig) {
        self.config = config;
        self.reset();
    }

    /// Clear all internal state: overlap history, queued input, pending output,
    /// and the pending spectrogram column.
    pub fn reset(&mut self) {
        self.overlap = [[0.0f32; OVERLAP]; EEG_CHANNELS];
        for ch in 0..EEG_CHANNELS {
            self.queued[ch].clear();
            self.pending[ch].clear();
        }
        self.latest_spec_col = None;
    }

    // ── Core: one overlap-save hop ────────────────────────────────────────────

    /// Execute one overlap-save hop for all 4 channels simultaneously.
    ///
    /// Steps:
    /// 1. Build a `WINDOW`-sample input per channel = `[overlap | HOP new]`.
    /// 2. **`fft_batch`** — 4 × 256 matrix, one GPU dispatch.
    /// 3. Apply the combined frequency mask (LP + HP + notch with harmonics).
    /// 4. **`ifft_batch`** — 4 × 256 matrix, one GPU dispatch.
    /// 5. Append the **last `HOP`** IFFT samples to `pending` (valid region).
    /// 6. Advance the overlap buffer.
    fn process_one_hop(&mut self) {
        // ── 1. Build batch input ──────────────────────────────────────────────
        //
        // Each window: [overlap_buf (OVERLAP=224)] ++ [new HOP=32 samples] = 256
        let signals: Vec<Vec<f32>> = (0..EEG_CHANNELS)
            .map(|ch| {
                let mut w = Vec::with_capacity(WINDOW);
                w.extend_from_slice(&self.overlap[ch]);
                for _ in 0..HOP {
                    w.push(self.queued[ch].pop_front().unwrap_or(0.0));
                }
                w
            })
            .collect();

        // ── 2. Advance overlap buffers ────────────────────────────────────────
        //
        // The next hop's overlap = the last OVERLAP samples of this window.
        // signals[ch][HOP..] has exactly OVERLAP = 224 elements.
        for (ov, sig) in self.overlap.iter_mut().zip(signals.iter()).take(EEG_CHANNELS) {
            ov.copy_from_slice(&sig[HOP..]);
        }

        // ── 3. GPU: forward FFT (4 channels, one dispatch) ───────────────────
        //
        // WINDOW = 256 is already a power of two, so fft_batch does not zero-pad.
        // n = 256, bin_hz = 256/256 = 1 Hz/bin.
        let mut spectra = fft_batch(&signals);
        let n = spectra[0].0.len(); // = WINDOW = 256

        // ── 3b. Spectrogram snapshot (zero extra GPU cost) ────────────────────
        //
        // We read the raw one-sided PSD from the FFT that is *already computed*
        // for the filter — no additional GPU dispatch is needed.  The PSD is
        // sampled HERE, before the frequency mask zeroes any bins, so the
        // spectrogram reflects the true unfiltered spectrum.
        //
        // `one_sided_psd` (= gpu_fft::psd::psd) returns (r²+i²)/n for each bin.
        // We take only the first SPEC_N_FREQ = 51 bins (0 Hz … 50 Hz at 1 Hz/bin).
        {
            let n_spec = SPEC_N_FREQ.min(n / 2 + 1);
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64()
                * 1000.0;

            let power: Vec<Vec<f32>> = spectra
                .iter()
                .map(|(real, imag)| {
                    // one_sided_psd computes (r[k]² + i[k]²) / n for each bin.
                    // Only the first n_spec bins cover 0 … 50 Hz.
                    one_sided_psd(&real[..n_spec], &imag[..n_spec])
                })
                .collect();

            self.latest_spec_col = Some(SpectrogramColumn { timestamp_ms: now_ms, power });
        }

        // ── 4. Combined frequency mask ────────────────────────────────────────
        //
        // Bin k → frequency:
        //   freq = k * (sample_rate / n)         for k in [0,  n/2]  (positive)
        //   freq = (n-k) * (sample_rate / n)     for k in (n/2, n)   (negative mirror)
        //
        // We apply the same boolean keep/discard to both a bin and its mirror
        // (which happens automatically because we compute freq symmetrically).
        // This guarantees the IFFT output remains real-valued.
        let bin_hz   = self.config.sample_rate / n as f32;
        let nyquist  = self.config.sample_rate / 2.0;

        // Pre-compute notch harmonic bounds once (outside the k loop).
        // harmonics: Vec<(lo, hi)> in Hz — one entry per harmonic within Nyquist.
        let notch_bands: Vec<(f32, f32)> = if let Some(preset) = self.config.notch {
            let fund = preset.hz();
            let bw   = self.config.notch_bandwidth_hz;
            let mut bands = Vec::new();
            let mut h = fund;
            while h <= nyquist + bw {
                bands.push((h - bw, h + bw));
                h += fund;
            }
            bands
        } else {
            Vec::new()
        };

        for (real, imag) in spectra.iter_mut() {
            for k in 0..n {
                let freq = if k <= n / 2 {
                    k as f32 * bin_hz
                } else {
                    (n - k) as f32 * bin_hz
                };

                let mut keep = true;

                // ── Low-pass: zero bins above cut-off ─────────────────────────
                if let Some(lp) = self.config.low_pass_hz {
                    if freq > lp {
                        keep = false;
                    }
                }

                // ── High-pass: zero bins (including DC) below cut-off ─────────
                if keep {
                    if let Some(hp) = self.config.high_pass_hz {
                        if freq < hp {
                            keep = false;
                        }
                    }
                }

                // ── Notch: zero fundamental + harmonics within ±bandwidth ──────
                //
                // Skipped when LP already removed the bin (keep == false) to
                // avoid redundant work.
                if keep {
                    for &(lo, hi) in &notch_bands {
                        if freq >= lo && freq <= hi {
                            keep = false;
                            break;
                        }
                    }
                }

                if !keep {
                    real[k] = 0.0;
                    imag[k] = 0.0;
                }
            }
        }

        // ── 5. GPU: inverse FFT (4 channels, one dispatch) ───────────────────
        //
        // ifft_batch returns one Vec<f32> per signal, length 2*n:
        //   [0..n]  = reconstructed real part
        //   [n..2n] = imaginary part (≈ 0 for real inputs, discarded)
        let outputs = ifft_batch(&spectra);

        // ── 6. Extract valid overlap-save region ──────────────────────────────
        //
        // The first OVERLAP = 224 samples of the IFFT may have wrap-around
        // artefacts; only the last HOP = 32 samples [OVERLAP..WINDOW] are clean.
        for (ch, out) in outputs.iter().enumerate() {
            for &v in &out[OVERLAP..WINDOW] {
                self.pending[ch].push_back(v);
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::CHANNEL_NAMES;
    use std::f64::consts::PI;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn sine(freq_hz: f64, n: usize) -> Vec<f64> {
        let sr = MUSE_SAMPLE_RATE as f64;
        (0..n)
            .map(|i| (2.0 * PI * freq_hz * i as f64 / sr).sin())
            .collect()
    }

    fn rms(v: &[f64]) -> f64 {
        (v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64).sqrt()
    }

    // Helper that feeds identical data to all 4 channels and returns ch-0 output.
    fn run_filter(cfg: FilterConfig, signal: &[f64]) -> Vec<f64> {
        let mut f = EegFilter::new(cfg);
        for ch in 0..EEG_CHANNELS {
            f.push(ch, signal);
        }
        f.drain(0)
    }

    // ── PowerlineFreq ─────────────────────────────────────────────────────────

    #[test]
    fn us_hz_is_60() {
        assert_eq!(PowerlineFreq::Hz60.hz(), 60.0);
    }

    #[test]
    fn eu_hz_is_50() {
        assert_eq!(PowerlineFreq::Hz50.hz(), 50.0);
    }

    #[test]
    fn default_powerline_is_us() {
        assert_eq!(PowerlineFreq::default(), PowerlineFreq::Hz60);
    }

    // ── FilterConfig helpers ──────────────────────────────────────────────────

    #[test]
    fn default_config_is_active() {
        assert!(FilterConfig::default().is_active());
    }

    #[test]
    fn default_config_has_us_notch() {
        assert_eq!(FilterConfig::default().notch, Some(PowerlineFreq::Hz60));
    }

    #[test]
    fn passthrough_config_is_inactive() {
        assert!(!FilterConfig::passthrough().is_active());
    }

    #[test]
    fn full_band_eu_has_eu_notch() {
        assert_eq!(FilterConfig::full_band_eu().notch, Some(PowerlineFreq::Hz50));
    }

    #[test]
    fn config_with_notch_only_is_active() {
        let cfg = FilterConfig {
            sample_rate:        256.0,
            low_pass_hz:        None,
            high_pass_hz:       None,
            notch:              Some(PowerlineFreq::Hz60),
            notch_bandwidth_hz: 1.0,
        };
        assert!(cfg.is_active());
    }

    // ── Passthrough ───────────────────────────────────────────────────────────

    #[test]
    fn passthrough_returns_raw_samples() {
        let mut f = EegFilter::new(FilterConfig::passthrough());
        let raw = vec![1.0_f64, 2.0, 3.0, 4.0];
        assert!(f.push(0, &raw));
        assert_eq!(f.drain(0), raw);
    }

    #[test]
    fn passthrough_does_not_use_queued_buffer() {
        let mut f = EegFilter::new(FilterConfig::passthrough());
        for ch in 0..EEG_CHANNELS {
            f.push(ch, &[0.5; 4]);
        }
        for ch in 0..EEG_CHANNELS {
            assert_eq!(f.queued[ch].len(), 0, "queued[{ch}] should be empty in passthrough");
        }
    }

    // ── Batching / pending ────────────────────────────────────────────────────

    #[test]
    fn push_returns_false_before_hop_accumulates() {
        let mut f = EegFilter::new(FilterConfig::default());
        assert!(!f.push(0, &vec![0.1; HOP - 1]));
    }

    #[test]
    fn push_returns_false_until_all_channels_ready() {
        let mut f = EegFilter::new(FilterConfig::default());
        for ch in 0..3 {
            f.push(ch, &vec![0.0; HOP]);
        }
        // Channel 3 missing → no batch
        for ch in 0..3 {
            assert_eq!(f.pending_len(ch), 0);
        }
    }

    #[test]
    fn drain_returns_hop_samples_after_one_batch() {
        let mut f = EegFilter::new(FilterConfig::default());
        for ch in 0..EEG_CHANNELS {
            f.push(ch, &vec![0.0; HOP]);
        }
        for ch in 0..EEG_CHANNELS {
            let out = f.drain(ch);
            assert_eq!(out.len(), HOP, "ch {ch}: expected {HOP}, got {}", out.len());
        }
    }

    #[test]
    fn drain_empties_pending_queue() {
        let mut f = EegFilter::new(FilterConfig::default());
        for ch in 0..EEG_CHANNELS {
            f.push(ch, &vec![0.0; HOP]);
        }
        for ch in 0..EEG_CHANNELS {
            f.drain(ch);
            assert_eq!(f.pending_len(ch), 0);
        }
    }

    #[test]
    fn multiple_hops_accumulate_correctly() {
        let mut f = EegFilter::new(FilterConfig::default());
        let hops = 3;
        for ch in 0..EEG_CHANNELS {
            f.push(ch, &vec![0.0; HOP * hops]);
        }
        for ch in 0..EEG_CHANNELS {
            assert_eq!(
                f.pending_len(ch), HOP * hops,
                "ch {ch}: expected {}", HOP * hops
            );
        }
    }

    #[test]
    fn all_four_channels_produce_output_in_same_hop() {
        let mut f = EegFilter::new(FilterConfig::default());
        for ch in 0..EEG_CHANNELS {
            f.push(ch, &[0.0; HOP]);
        }
        for (ch, name) in CHANNEL_NAMES.iter().enumerate().take(EEG_CHANNELS) {
            assert!(f.pending_len(ch) > 0, "ch {ch} ({name}) empty");
        }
    }

    // ── reset / set_config ────────────────────────────────────────────────────

    #[test]
    fn reset_clears_all_state() {
        let mut f = EegFilter::new(FilterConfig::default());
        for ch in 0..EEG_CHANNELS {
            f.push(ch, &vec![1.0; HOP]);
        }
        f.reset();
        for ch in 0..EEG_CHANNELS {
            assert_eq!(f.queued[ch].len(),  0, "queued[{ch}] not cleared");
            assert_eq!(f.pending_len(ch),   0, "pending[{ch}] not cleared");
            assert!(f.overlap[ch].iter().all(|&v| v == 0.0), "overlap[{ch}] not cleared");
        }
    }

    #[test]
    fn set_config_resets_and_applies_new_config() {
        let mut f = EegFilter::new(FilterConfig::default());
        for ch in 0..EEG_CHANNELS {
            f.push(ch, &vec![1.0; HOP]);
        }
        let new_cfg = FilterConfig { notch: Some(PowerlineFreq::Hz50), ..FilterConfig::default() };
        f.set_config(new_cfg);
        assert_eq!(f.config.notch, Some(PowerlineFreq::Hz50));
        for ch in 0..EEG_CHANNELS {
            assert_eq!(f.pending_len(ch), 0, "pending[{ch}] not cleared");
        }
    }

    // ── Edge cases ────────────────────────────────────────────────────────────

    #[test]
    fn invalid_channel_returns_false() {
        let mut f = EegFilter::new(FilterConfig::default());
        assert!(!f.push(99, &[1.0, 2.0]));
    }

    #[test]
    fn empty_push_is_a_no_op() {
        let mut f = EegFilter::new(FilterConfig::default());
        assert!(!f.push(0, &[]));
        assert_eq!(f.queued[0].len(), 0);
    }

    // ── Spectral correctness — low-pass ───────────────────────────────────────

    /// A 100 Hz tone is above our 50 Hz LP cut-off.  After the filter settles
    /// the RMS of the output should be close to zero.
    #[test]
    fn low_pass_attenuates_out_of_band_signal() {
        let cfg = FilterConfig {
            notch: None, // isolate LP stage
            ..FilterConfig { low_pass_hz: Some(50.0), high_pass_hz: None, ..FilterConfig::default() }
        };
        let tone = sine(100.0, WINDOW * 4);
        let out  = run_filter(cfg, &tone);
        let power = rms(&out[HOP..]); // skip first hop (settling)
        assert!(power < 0.05, "100 Hz should be nearly zeroed by 50 Hz LP; RMS = {power:.4}");
    }

    // ── Spectral correctness — high-pass ─────────────────────────────────────

    /// Pure DC (all ones) must be removed when HP is active.
    #[test]
    fn high_pass_removes_dc_offset() {
        let cfg = FilterConfig {
            low_pass_hz: None, notch: None,
            ..FilterConfig { high_pass_hz: Some(0.5), ..FilterConfig::default() }
        };
        let dc  = vec![1.0_f64; WINDOW * 4];
        let out = run_filter(cfg, &dc);
        let power = rms(&out[HOP..]);
        assert!(power < 0.1, "DC should be nearly zeroed; RMS = {power:.4}");
    }

    // ── Spectral correctness — band-pass ─────────────────────────────────────

    /// A 10 Hz alpha-band tone is well inside the 0.5 – 50 Hz pass-band.
    /// After the filter settles the RMS should be close to 1/√2 ≈ 0.707.
    #[test]
    fn band_pass_preserves_in_band_signal() {
        let cfg = FilterConfig {
            notch: None, // isolate LP+HP stages
            ..FilterConfig { low_pass_hz: Some(50.0), high_pass_hz: Some(0.5), ..FilterConfig::default() }
        };
        let tone = sine(10.0, WINDOW * 6);
        let out  = run_filter(cfg, &tone);
        let power = rms(&out[2 * HOP..]); // skip first two hops for warm-up
        assert!(
            power > 0.5 && power < 0.9,
            "10 Hz should pass with near-unity gain; RMS = {power:.4}"
        );
    }

    // ── Spectral correctness — US notch (60 Hz) ───────────────────────────────

    /// A pure 60 Hz tone must be strongly attenuated by the US notch.
    #[test]
    fn us_notch_attenuates_60hz() {
        let cfg = FilterConfig {
            low_pass_hz:  None,
            high_pass_hz: None,
            notch:        Some(PowerlineFreq::Hz60),
            notch_bandwidth_hz: 1.0,
            sample_rate:  MUSE_SAMPLE_RATE,
        };
        let tone = sine(60.0, WINDOW * 4);
        let out  = run_filter(cfg, &tone);
        let power = rms(&out[HOP..]);
        assert!(power < 0.05, "60 Hz should be nearly zeroed by US notch; RMS = {power:.4}");
    }

    /// The 2nd US harmonic (120 Hz) must also be attenuated.
    #[test]
    fn us_notch_attenuates_120hz_harmonic() {
        let cfg = FilterConfig {
            low_pass_hz:  None,
            high_pass_hz: None,
            notch:        Some(PowerlineFreq::Hz60),
            notch_bandwidth_hz: 1.0,
            sample_rate:  MUSE_SAMPLE_RATE,
        };
        let tone = sine(120.0, WINDOW * 4);
        let out  = run_filter(cfg, &tone);
        let power = rms(&out[HOP..]);
        assert!(power < 0.05, "120 Hz should be zeroed by 2nd US harmonic; RMS = {power:.4}");
    }

    // ── Spectral correctness — EU notch (50 Hz) ───────────────────────────────

    /// A pure 50 Hz tone must be strongly attenuated by the EU notch.
    #[test]
    fn eu_notch_attenuates_50hz() {
        let cfg = FilterConfig {
            low_pass_hz:  None,
            high_pass_hz: None,
            notch:        Some(PowerlineFreq::Hz50),
            notch_bandwidth_hz: 1.0,
            sample_rate:  MUSE_SAMPLE_RATE,
        };
        let tone = sine(50.0, WINDOW * 4);
        let out  = run_filter(cfg, &tone);
        let power = rms(&out[HOP..]);
        assert!(power < 0.05, "50 Hz should be nearly zeroed by EU notch; RMS = {power:.4}");
    }

    /// The 2nd EU harmonic (100 Hz) must also be attenuated.
    #[test]
    fn eu_notch_attenuates_100hz_harmonic() {
        let cfg = FilterConfig {
            low_pass_hz:  None,
            high_pass_hz: None,
            notch:        Some(PowerlineFreq::Hz50),
            notch_bandwidth_hz: 1.0,
            sample_rate:  MUSE_SAMPLE_RATE,
        };
        let tone = sine(100.0, WINDOW * 4);
        let out  = run_filter(cfg, &tone);
        let power = rms(&out[HOP..]);
        assert!(power < 0.05, "100 Hz should be zeroed by 2nd EU harmonic; RMS = {power:.4}");
    }

    // ── Notch does not affect in-band signal ──────────────────────────────────

    /// A 10 Hz tone must be unaffected by either notch (60/50 Hz are far away).
    #[test]
    fn notch_does_not_affect_alpha_band() {
        for preset in [PowerlineFreq::Hz60, PowerlineFreq::Hz50] {
            let cfg = FilterConfig {
                low_pass_hz:  None,
                high_pass_hz: None,
                notch:        Some(preset),
                notch_bandwidth_hz: 1.0,
                sample_rate:  MUSE_SAMPLE_RATE,
            };
            let tone = sine(10.0, WINDOW * 6);
            let out  = run_filter(cfg, &tone);
            let power = rms(&out[2 * HOP..]);
            assert!(
                power > 0.5,
                "{} notch should not attenuate 10 Hz; RMS = {power:.4}",
                preset.label()
            );
        }
    }

    // ── Switching notch preset ────────────────────────────────────────────────

    /// Switching from US to EU preset via set_config must reset state and apply
    /// the new notch.
    #[test]
    fn switching_preset_resets_and_works() {
        let mut f = EegFilter::new(FilterConfig::full_band_us());
        for ch in 0..EEG_CHANNELS {
            f.push(ch, &vec![0.0; HOP]);
        }
        // Switch to EU
        f.set_config(FilterConfig::full_band_eu());
        assert_eq!(f.config.notch, Some(PowerlineFreq::Hz50));
        for ch in 0..EEG_CHANNELS {
            assert_eq!(f.pending_len(ch), 0, "pending[{ch}] must be cleared on preset switch");
        }
    }
}
