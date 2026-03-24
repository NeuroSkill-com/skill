// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Property-based tests for EEG signal processing primitives.

#[cfg(test)]
mod tests {
    use crate::cpu_fft::{fft_batch, ifft_batch, psd};
    use proptest::prelude::*;

    // ── FFT/IFFT round-trip ──────────────────────────────────────────────────

    proptest! {
        /// FFT → IFFT round-trip must reconstruct the original signal
        /// within floating-point tolerance.
        #[test]
        fn fft_ifft_roundtrip(
            signal in prop::collection::vec(-1000.0f32..1000.0, 8..=256)
        ) {
            // FFT requires power-of-2 length; pad to next power of 2
            let n = signal.len().next_power_of_two();
            let mut padded = signal.clone();
            padded.resize(n, 0.0);

            let spectra = fft_batch(&[padded.clone()]);
            let reconstructed = ifft_batch(&spectra);

            prop_assert_eq!(reconstructed.len(), 1);
            prop_assert_eq!(reconstructed[0].len(), padded.len());

            for (i, (&orig, &recon)) in padded.iter().zip(reconstructed[0].iter()).enumerate() {
                let diff = (orig - recon).abs();
                prop_assert!(
                    diff < 0.01,
                    "sample {i}: original={orig}, reconstructed={recon}, diff={diff}"
                );
            }
        }

        /// PSD values must all be non-negative.
        #[test]
        fn psd_is_non_negative(
            real in prop::collection::vec(-100.0f32..100.0, 4..=128),
            imag in prop::collection::vec(-100.0f32..100.0, 4..=128),
        ) {
            let len = real.len().min(imag.len());
            let r = &real[..len];
            let i = &imag[..len];
            let power = psd(r, i);
            for (j, &p) in power.iter().enumerate() {
                prop_assert!(p >= 0.0, "PSD[{j}] = {p} < 0");
                prop_assert!(!p.is_nan(), "PSD[{j}] is NaN");
            }
        }

        /// PSD of a zero signal is all zeros.
        #[test]
        fn psd_of_zero_is_zero(n in 4usize..=128) {
            let zeros = vec![0.0f32; n];
            let power = psd(&zeros, &zeros);
            for (j, &p) in power.iter().enumerate() {
                prop_assert!((p - 0.0).abs() < f32::EPSILON, "PSD[{j}] = {p} != 0");
            }
        }

        /// FFT batch with multiple signals produces one spectrum per signal.
        #[test]
        fn fft_batch_length_matches(
            n_signals in 1usize..=8,
            signal_len in prop::sample::select(vec![8usize, 16, 32, 64, 128, 256])
        ) {
            let signals: Vec<Vec<f32>> = (0..n_signals)
                .map(|i| (0..signal_len).map(|j| (i * j) as f32 * 0.01).collect())
                .collect();
            let spectra = fft_batch(&signals);
            prop_assert_eq!(spectra.len(), n_signals);
            for (real, imag) in &spectra {
                prop_assert_eq!(real.len(), signal_len);
                prop_assert_eq!(imag.len(), signal_len);
            }
        }
    }
}
