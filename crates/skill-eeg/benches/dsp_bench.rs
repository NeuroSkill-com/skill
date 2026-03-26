// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Benchmarks for EEG DSP hot paths — FFT, band-power analysis, and filtering.
//!
//! Run: `cargo bench -p skill-eeg`

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use skill_eeg::cpu_fft::{fft_batch, ifft_batch, psd};
use std::hint::black_box;
use skill_eeg::eeg_bands::BandAnalyzer;
use skill_eeg::eeg_filter::{EegFilter, FilterConfig};

fn bench_fft(c: &mut Criterion) {
    let mut group = c.benchmark_group("fft");
    for &size in &[128, 256, 512, 1024] {
        let signal: Vec<f32> = (0..size).map(|i| (i as f32 * 0.1).sin()).collect();
        group.bench_with_input(BenchmarkId::new("fft_batch", size), &signal, |b, s| {
            b.iter(|| fft_batch(black_box(&[s.clone()])));
        });
    }
    group.finish();
}

fn bench_ifft(c: &mut Criterion) {
    let mut group = c.benchmark_group("ifft");
    for &size in &[128, 256, 512, 1024] {
        let signal: Vec<f32> = (0..size).map(|i| (i as f32 * 0.1).sin()).collect();
        let spectra = fft_batch(&[signal]);
        group.bench_with_input(BenchmarkId::new("ifft_batch", size), &spectra, |b, s| {
            b.iter(|| ifft_batch(black_box(s)));
        });
    }
    group.finish();
}

fn bench_psd(c: &mut Criterion) {
    let mut group = c.benchmark_group("psd");
    for &size in &[128, 256, 512] {
        let real: Vec<f32> = (0..size).map(|i| (i as f32 * 0.1).sin()).collect();
        let imag: Vec<f32> = (0..size).map(|i| (i as f32 * 0.1).cos()).collect();
        group.bench_with_input(BenchmarkId::new("psd", size), &size, |b, _| {
            b.iter(|| psd(black_box(&real), black_box(&imag)));
        });
    }
    group.finish();
}

fn bench_band_analyzer(c: &mut Criterion) {
    let mut analyzer = BandAnalyzer::new_with_rate(256.0);
    // Generate 1 second of synthetic EEG at 256 Hz
    let samples: Vec<f64> = (0..256)
        .map(|i| {
            let t = i as f64 / 256.0;
            // Mix of alpha (10 Hz) and beta (20 Hz)
            (10.0 * std::f64::consts::TAU * t).sin() + 0.5 * (20.0 * std::f64::consts::TAU * t).sin()
        })
        .collect();

    c.bench_function("band_analyzer_push_256", |b| {
        b.iter(|| {
            analyzer.push(black_box(0), black_box(&samples));
            analyzer.reset();
        });
    });
}

fn bench_filter(c: &mut Criterion) {
    let config = FilterConfig::full_band_us();
    let samples: Vec<f64> = (0..256)
        .map(|i| {
            let t = i as f64 / 256.0;
            (10.0 * std::f64::consts::TAU * t).sin()
        })
        .collect();

    c.bench_function("eeg_filter_push_256", |b| {
        let mut filter = EegFilter::new(config);
        b.iter(|| {
            filter.push(black_box(0), black_box(&samples));
            let _ = filter.drain(0);
        });
    });
}

criterion_group!(
    benches,
    bench_fft,
    bench_ifft,
    bench_psd,
    bench_band_analyzer,
    bench_filter
);
criterion_main!(benches);
