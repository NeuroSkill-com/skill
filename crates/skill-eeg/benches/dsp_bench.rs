// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Benchmarks for EEG DSP hot paths — FFT, band-power analysis, and filtering.
//!
//! Run: `cargo bench -p skill-eeg`
//! Run (Metal): `cargo bench -p skill-eeg --features rlx-fft-metal`

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use skill_eeg::eeg_bands::BandAnalyzer;
use skill_eeg::eeg_filter::{EegFilter, FilterConfig};
use std::hint::black_box;

// ── FFT benchmarks: backed by whichever FFT path is compiled in ───────────────
//
// Sweeps batch × n to show the GPU crossover point.
// Batches model EEG channel counts: 1 ch (single), 8 ch (Mendi/MW75),
// 32 ch (research cap), 64 ch (dense array).

const FFT_SIZES: &[usize] = &[128, 256, 512, 1024];
const BATCH_SIZES: &[usize] = &[1, 8, 32, 64];

#[cfg(feature = "rlx-fft")]
fn bench_fft(c: &mut Criterion) {
    use skill_eeg::rlx_fft::{fft_batch, ifft_batch};

    let mut group = c.benchmark_group("fft_batch");
    for &batch in BATCH_SIZES {
        for &n in FFT_SIZES {
            let signals: Vec<Vec<f32>> = (0..batch)
                .map(|ch| (0..n).map(|i| ((i + ch) as f32 * 0.1).sin()).collect())
                .collect();
            let total_samples = (batch * n) as u64;
            group.throughput(Throughput::Elements(total_samples));
            group.bench_with_input(BenchmarkId::new(format!("b{batch}"), n), &signals, |b, s| {
                b.iter(|| fft_batch(black_box(s)));
            });
        }
    }
    group.finish();

    let mut group = c.benchmark_group("ifft_batch");
    for &batch in BATCH_SIZES {
        for &n in FFT_SIZES {
            let signals: Vec<Vec<f32>> = (0..batch)
                .map(|ch| (0..n).map(|i| ((i + ch) as f32 * 0.1).sin()).collect())
                .collect();
            let spectra = fft_batch(&signals);
            let total_samples = (batch * n) as u64;
            group.throughput(Throughput::Elements(total_samples));
            group.bench_with_input(BenchmarkId::new(format!("b{batch}"), n), &spectra, |b, s| {
                b.iter(|| ifft_batch(black_box(s)));
            });
        }
    }
    group.finish();
}

#[cfg(not(any(feature = "rlx-fft", feature = "gpu")))]
fn bench_fft(c: &mut Criterion) {
    use skill_eeg::cpu_fft::{fft_batch, ifft_batch};

    let mut group = c.benchmark_group("fft_batch");
    for &batch in BATCH_SIZES {
        for &n in FFT_SIZES {
            let signals: Vec<Vec<f32>> = (0..batch)
                .map(|ch| (0..n).map(|i| ((i + ch) as f32 * 0.1).sin()).collect())
                .collect();
            let total_samples = (batch * n) as u64;
            group.throughput(Throughput::Elements(total_samples));
            group.bench_with_input(BenchmarkId::new(format!("b{batch}"), n), &signals, |b, s| {
                b.iter(|| fft_batch(black_box(s)));
            });
        }
    }
    group.finish();

    let mut group = c.benchmark_group("ifft_batch");
    for &batch in BATCH_SIZES {
        for &n in FFT_SIZES {
            let signals: Vec<Vec<f32>> = (0..batch)
                .map(|ch| (0..n).map(|i| ((i + ch) as f32 * 0.1).sin()).collect())
                .collect();
            let spectra = fft_batch(&signals);
            let total_samples = (batch * n) as u64;
            group.throughput(Throughput::Elements(total_samples));
            group.bench_with_input(BenchmarkId::new(format!("b{batch}"), n), &spectra, |b, s| {
                b.iter(|| ifft_batch(black_box(s)));
            });
        }
    }
    group.finish();
}

// ── PSD benchmarks ────────────────────────────────────────────────────────────

#[cfg(feature = "rlx-fft")]
fn bench_psd(c: &mut Criterion) {
    use skill_eeg::rlx_fft::psd;
    let mut group = c.benchmark_group("psd");
    for &size in &[128, 256, 512, 1024] {
        let real: Vec<f32> = (0..size).map(|i| (i as f32 * 0.1).sin()).collect();
        let imag: Vec<f32> = (0..size).map(|i| (i as f32 * 0.1).cos()).collect();
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("psd", size), &size, |b, _| {
            b.iter(|| psd(black_box(&real), black_box(&imag)));
        });
    }
    group.finish();
}

#[cfg(not(any(feature = "rlx-fft", feature = "gpu")))]
fn bench_psd(c: &mut Criterion) {
    use skill_eeg::cpu_fft::psd;
    let mut group = c.benchmark_group("psd");
    for &size in &[128, 256, 512, 1024] {
        let real: Vec<f32> = (0..size).map(|i| (i as f32 * 0.1).sin()).collect();
        let imag: Vec<f32> = (0..size).map(|i| (i as f32 * 0.1).cos()).collect();
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("psd", size), &size, |b, _| {
            b.iter(|| psd(black_box(&real), black_box(&imag)));
        });
    }
    group.finish();
}

// ── Band analyzer & filter (realistic multi-channel) ─────────────────────────

fn bench_band_analyzer(c: &mut Criterion) {
    let mut group = c.benchmark_group("band_analyzer");
    for &num_channels in &[1usize, 8, 32] {
        for &n_samples in &[256usize, 512] {
            let samples: Vec<f64> = (0..n_samples)
                .map(|i| {
                    let t = i as f64 / 256.0;
                    (10.0 * std::f64::consts::TAU * t).sin() + 0.5 * (20.0 * std::f64::consts::TAU * t).sin()
                })
                .collect();
            group.throughput(Throughput::Elements((num_channels * n_samples) as u64));
            group.bench_with_input(
                BenchmarkId::new(format!("ch{num_channels}"), n_samples),
                &samples,
                |b, s| {
                    let mut analyzer = BandAnalyzer::new_with_rate(256.0);
                    b.iter(|| {
                        for ch in 0..num_channels {
                            analyzer.push(black_box(ch), black_box(s));
                        }
                        analyzer.reset();
                    });
                },
            );
        }
    }
    group.finish();
}

fn bench_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("eeg_filter");
    for &num_channels in &[1usize, 8, 32] {
        for &n_samples in &[256usize, 512] {
            let samples: Vec<f64> = (0..n_samples)
                .map(|i| {
                    let t = i as f64 / 256.0;
                    (10.0 * std::f64::consts::TAU * t).sin()
                })
                .collect();
            let config = FilterConfig::full_band_us();
            group.throughput(Throughput::Elements((num_channels * n_samples) as u64));
            group.bench_with_input(
                BenchmarkId::new(format!("ch{num_channels}"), n_samples),
                &samples,
                |b, s| {
                    let mut filter = EegFilter::new(config);
                    b.iter(|| {
                        for ch in 0..num_channels {
                            filter.push(black_box(ch), black_box(s));
                            let _ = filter.drain(ch);
                        }
                    });
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, bench_fft, bench_psd, bench_band_analyzer, bench_filter);
criterion_main!(benches);
