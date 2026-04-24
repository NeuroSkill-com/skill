// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// End-to-end FFT tests for the gpu-fft backend (wgpu + MLX).
//
// Run with:
//   cargo test -p skill-eeg --features gpu      -- fft_e2e --nocapture
//   cargo test -p skill-eeg --features mlx       -- fft_e2e --nocapture
//   cargo test -p skill-eeg --features gpu,mlx   -- fft_e2e --nocapture

#[cfg(any(feature = "gpu", feature = "mlx"))]
mod fft_e2e {
    use gpu_fft::{fft_batch, ifft_batch, psd::psd};

    /// Generate a sine wave at `freq` Hz, `n` samples at `sr` sample rate.
    fn sine(freq: f32, sr: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
            .collect()
    }

    #[test]
    fn fft_e2e_roundtrip_256() {
        let n = 256;
        let signal = sine(10.0, 256.0, n);
        let start = std::time::Instant::now();
        let spectra = fft_batch(&[signal.clone()]);
        let output = ifft_batch(&spectra);
        let elapsed = start.elapsed();

        // ifft_batch returns [real(0..n), imag(n..2n)] — take real part only
        let out = &output[0];
        assert!(out.len() >= n, "output has at least {n} samples (got {})", out.len());
        let max_err: f32 = signal
            .iter()
            .zip(out[..n].iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(max_err < 1e-3, "round-trip max error {max_err} < 1e-3");

        eprintln!("── fft_e2e_roundtrip_256 ──");
        eprintln!("  elapsed: {:.2} ms", elapsed.as_secs_f64() * 1000.0);
        eprintln!("  max_err: {max_err:.6}");
    }

    #[test]
    fn fft_e2e_batch_4ch() {
        // Simulate 4-channel EEG at 256 Hz
        let signals: Vec<Vec<f32>> = (0..4).map(|ch| sine(10.0 + ch as f32 * 5.0, 256.0, 256)).collect();

        let start = std::time::Instant::now();
        let spectra = fft_batch(&signals);
        let outputs = ifft_batch(&spectra);
        let elapsed = start.elapsed();

        assert_eq!(spectra.len(), 4, "4 spectra");
        assert_eq!(outputs.len(), 4, "4 outputs");

        for (ch, (sig, out)) in signals.iter().zip(outputs.iter()).enumerate() {
            let max_err: f32 = sig
                .iter()
                .zip(out[..sig.len()].iter())
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            assert!(max_err < 1e-3, "ch{ch} round-trip max error {max_err}");
        }

        eprintln!("── fft_e2e_batch_4ch ──");
        eprintln!("  elapsed: {:.2} ms", elapsed.as_secs_f64() * 1000.0);
    }

    #[test]
    fn fft_e2e_psd_peak_detection() {
        // 10 Hz sine at 256 Hz sample rate, 256 samples → 1 Hz/bin
        let n = 256;
        let signal = sine(10.0, 256.0, n);
        let spectra = fft_batch(&[signal]);
        let (re, im) = &spectra[0];
        let power = psd(re, im);

        // One-sided spectrum: only first n/2+1 bins are meaningful
        let one_sided = n / 2 + 1; // 129 bins (0..128 Hz)
        let peak_bin = power[..one_sided]
            .iter()
            .enumerate()
            .skip(1) // skip DC
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();

        assert_eq!(peak_bin, 10, "PSD peak at bin 10 (10 Hz)");

        eprintln!("── fft_e2e_psd_peak_detection ──");
        eprintln!("  peak_bin: {peak_bin} (expected 10)");
        eprintln!("  peak_power: {:.4}", power[peak_bin]);
    }

    #[test]
    fn fft_e2e_large_batch() {
        // 32 channels × 1024 samples — stress test
        let n = 1024;
        let signals: Vec<Vec<f32>> = (0..32).map(|ch| sine(5.0 + ch as f32, 256.0, n)).collect();

        let start = std::time::Instant::now();
        let spectra = fft_batch(&signals);
        let outputs = ifft_batch(&spectra);
        let elapsed = start.elapsed();

        assert_eq!(outputs.len(), 32);
        let throughput = (32 * n) as f64 / elapsed.as_secs_f64();

        eprintln!("── fft_e2e_large_batch ──");
        eprintln!("  32 × {n} samples");
        eprintln!("  elapsed:    {:.2} ms", elapsed.as_secs_f64() * 1000.0);
        eprintln!("  throughput: {:.0} samples/sec", throughput);
    }
}
