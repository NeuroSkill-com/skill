// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! CPU-based FFT fallback used when the `gpu` feature is disabled.
//!
//! Provides the same API surface as `gpu_fft` (fft_batch, ifft_batch, psd)
//! using `rustfft` so that tests and headless CI environments work without
//! a GPU.

use rustfft::{num_complex::Complex, FftPlanner};

/// Forward FFT of a single real signal → (real, imag) vectors.
fn fft_single(input: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let n = input.len().next_power_of_two();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);

    let mut buf: Vec<Complex<f32>> = input.iter().map(|&v| Complex::new(v, 0.0)).collect();
    buf.resize(n, Complex::new(0.0, 0.0));

    fft.process(&mut buf);

    let real = buf.iter().map(|c| c.re).collect();
    let imag = buf.iter().map(|c| c.im).collect();
    (real, imag)
}

/// Batched forward FFT — matches `gpu_fft::fft_batch` API.
pub fn fft_batch(signals: &[Vec<f32>]) -> Vec<(Vec<f32>, Vec<f32>)> {
    signals.iter().map(|s| fft_single(s)).collect()
}

/// Batched inverse FFT — matches `gpu_fft::ifft_batch` API.
///
/// Each input is a `(real, imag)` pair representing a full complex spectrum of
/// length `n`.  Returns one `Vec<f32>` per signal containing the real part of
/// the reconstructed time-domain signal (length `n`), matching the layout that
/// `gpu_fft::ifft_batch` produces (the first `n` elements are real, the next
/// `n` are the imaginary residual — we only return the real slice).
pub fn ifft_batch(spectra: &[(Vec<f32>, Vec<f32>)]) -> Vec<Vec<f32>> {
    spectra
        .iter()
        .map(|(real, imag)| {
            let n = real.len();
            let mut planner = FftPlanner::<f32>::new();
            let ifft = planner.plan_fft_inverse(n);

            let mut buf: Vec<Complex<f32>> = real
                .iter()
                .zip(imag.iter())
                .map(|(&r, &i)| Complex::new(r, i))
                .collect();

            ifft.process(&mut buf);

            // rustfft does NOT normalise — divide by n.
            let inv_n = 1.0 / n as f32;
            buf.iter().map(|c| c.re * inv_n).collect()
        })
        .collect()
}

/// One-sided Power Spectral Density: `(r² + i²) / n` for each bin.
///
/// Matches `gpu_fft::psd::psd` — expects the first `n_oneside` bins of the
/// real and imaginary FFT output.
pub fn psd(real: &[f32], imag: &[f32]) -> Vec<f32> {
    let n = real.len();
    real.iter()
        .zip(imag.iter())
        .map(|(&r, &i)| (r * r + i * i) / n as f32)
        .collect()
}
