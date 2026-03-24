// SPDX-License-Identifier: GPL-3.0-only
//! Integration tests for the DSP pipeline: band analysis → quality.
//!
//! These tests verify that the pipeline works end-to-end with synthetic
//! signals at different sample rates (multi-device support).

use skill_eeg::eeg_bands::BandAnalyzer;
use skill_eeg::eeg_quality::QualityMonitor;

/// Generate a pure sine wave at a given frequency.
fn sine_wave(freq: f64, sample_rate: f64, n_samples: usize) -> Vec<f64> {
    (0..n_samples)
        .map(|i| {
            let t = i as f64 / sample_rate;
            (2.0 * std::f64::consts::PI * freq * t).sin() * 10.0
        })
        .collect()
}

#[test]
fn band_analyzer_produces_snapshot_after_enough_samples() {
    let sr = 256.0_f32;
    let mut analyzer = BandAnalyzer::new_with_rate(sr);

    // Push enough samples for all 4 channels to fire a snapshot
    let signal = sine_wave(10.0, sr as f64, 1024);
    for ch in 0..4 {
        analyzer.push(ch, &signal);
    }

    assert!(analyzer.latest.is_some(), "should have produced at least one snapshot");
    let snap = analyzer.latest.as_ref().unwrap();
    assert!(!snap.channels.is_empty(), "snapshot should have channel powers");

    // 10 Hz is alpha — check that alpha is present
    for ch_pow in &snap.channels {
        assert!(ch_pow.alpha > 0.0, "alpha should be > 0 for 10 Hz sine");
    }
}

#[test]
fn band_analyzer_different_sample_rates() {
    // Verify the analyzer works with non-Muse sample rates
    for &sr in &[128.0_f32, 250.0, 256.0, 500.0] {
        let signal = sine_wave(10.0, sr as f64, (sr as usize) * 4);
        let mut analyzer = BandAnalyzer::new_with_rate(sr);
        for ch in 0..4 {
            analyzer.push(ch, &signal);
        }
        let snap = analyzer.latest.as_ref();
        assert!(
            snap.is_some(),
            "should produce snapshot at sr={sr}"
        );
    }
}

#[test]
fn band_analyzer_beta_dominates_for_20hz() {
    let sr = 256.0_f32;
    let signal = sine_wave(20.0, sr as f64, 2048);
    let mut analyzer = BandAnalyzer::new_with_rate(sr);
    for ch in 0..4 {
        analyzer.push(ch, &signal);
    }

    let snap = analyzer.latest.as_ref().expect("snapshot");
    for ch_pow in &snap.channels {
        assert!(
            ch_pow.beta > ch_pow.alpha,
            "beta ({}) should exceed alpha ({}) for 20 Hz",
            ch_pow.beta, ch_pow.alpha
        );
    }
}

#[test]
fn quality_monitor_channels() {
    let mut monitor = QualityMonitor::new(4);

    let clean = sine_wave(10.0, 256.0, 512);
    for ch in 0..4 {
        monitor.push(ch, &clean);
    }

    let qualities = monitor.all_qualities();
    assert_eq!(qualities.len(), 4);
}

#[test]
fn band_analyzer_reset_clears_state() {
    let sr = 256.0_f32;
    let signal = sine_wave(10.0, sr as f64, 1024);
    let mut analyzer = BandAnalyzer::new_with_rate(sr);
    for ch in 0..4 {
        analyzer.push(ch, &signal);
    }
    assert!(analyzer.latest.as_ref().is_some());

    analyzer.reset();
    assert!(analyzer.latest.as_ref().is_none(), "reset should clear latest snapshot");
}
