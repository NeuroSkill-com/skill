// SPDX-License-Identifier: GPL-3.0-only
//! Tests for CsvState: write EEG/PPG/metrics and verify the output.
#![allow(clippy::unwrap_used)]

use skill_data::session_csv::{build_metrics_header, CsvState};
use std::path::Path;
use tempfile::tempdir;

// ── build_metrics_header ─────────────────────────────────────────────────────

#[test]
fn build_metrics_header_4ch() {
    let h = build_metrics_header(&["TP9", "AF7", "AF8", "TP10"]);
    // timestamp + 4 channels × 12 bands + 46 cross-channel = 95
    assert_eq!(h.len(), 95);
    assert_eq!(h[0], "timestamp_s");
    assert_eq!(h[1], "TP9_delta");
    assert_eq!(h[12], "TP9_rel_high_gamma"); // last of first channel
    assert_eq!(h[13], "AF7_delta");
    // Cross-channel starts at 1 + 4*12 = 49
    assert_eq!(h[49], "faa");
}

#[test]
fn build_metrics_header_8ch() {
    let labels: Vec<&str> = (0..8)
        .map(|i| ["Fp1", "Fp2", "F3", "F4", "C3", "C4", "O1", "O2"][i])
        .collect();
    let h = build_metrics_header(&labels);
    // timestamp + 8 × 12 + 46 = 143
    assert_eq!(h.len(), 143);
    assert_eq!(h[0], "timestamp_s");
    assert!(h.last().unwrap() == "gpu_tiler_pct");
}

#[test]
fn build_metrics_header_1ch() {
    let h = build_metrics_header(&["Cz"]);
    // timestamp + 1 × 12 + 46 = 59
    assert_eq!(h.len(), 59);
}

// ── CsvState: EEG write ──────────────────────────────────────────────────────

#[test]
fn csv_state_creates_file_with_header() {
    let dir = tempdir().unwrap();
    let csv_path = dir.path().join("exg_1700000000.csv");
    let state = CsvState::open(&csv_path).unwrap();
    drop(state); // flush

    let content = std::fs::read_to_string(&csv_path).unwrap();
    let first_line = content.lines().next().unwrap();
    assert!(first_line.starts_with("timestamp_s,"));
    assert!(first_line.contains("TP9"));
    assert!(first_line.contains("TP10"));
}

#[test]
fn csv_state_custom_labels() {
    let dir = tempdir().unwrap();
    let csv_path = dir.path().join("exg_1700000000.csv");
    let state = CsvState::open_with_labels(&csv_path, &["Fp1", "Fp2", "Cz", "Oz", "Pz"]).unwrap();
    drop(state);

    let content = std::fs::read_to_string(&csv_path).unwrap();
    let header = content.lines().next().unwrap();
    assert!(header.contains("Fp1"));
    assert!(header.contains("Pz"));
    // Should have timestamp + 5 channels = 6 columns
    assert_eq!(header.split(',').count(), 6);
}

#[test]
fn csv_state_writes_eeg_samples() {
    let dir = tempdir().unwrap();
    let csv_path = dir.path().join("exg_1700000000.csv");
    let mut state = CsvState::open(&csv_path).unwrap();

    // Push 10 samples per channel (4 channels for default Muse layout)
    let ts = 1700000000.0;
    for ch in 0..4 {
        let samples: Vec<f64> = (0..10).map(|i| (ch * 10 + i) as f64).collect();
        state.push_eeg(ch, &samples, ts, 256.0);
    }
    drop(state); // flush

    let content = std::fs::read_to_string(&csv_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    // Header + 10 data rows
    assert_eq!(lines.len(), 11, "should have header + 10 rows, got {}", lines.len());

    // First data row should have timestamp close to 1700000000
    let first_data = lines[1];
    let ts_str: &str = first_data.split(',').next().unwrap();
    let ts_val: f64 = ts_str.parse().unwrap();
    assert!((ts_val - 1700000000.0).abs() < 1.0);
}

#[test]
fn csv_state_ignores_out_of_range_electrode() {
    let dir = tempdir().unwrap();
    let csv_path = dir.path().join("exg_1700000000.csv");
    let mut state = CsvState::open(&csv_path).unwrap();

    // Electrode 99 is out of range — should be silently ignored
    state.push_eeg(99, &[1.0, 2.0], 1700000000.0, 256.0);
    drop(state);

    let content = std::fs::read_to_string(&csv_path).unwrap();
    // Should only have the header, no data rows
    assert_eq!(content.lines().count(), 1);
}

// ── CsvState: multi-channel write ─────────────────────────────────────────────

#[test]
fn csv_state_8ch_write() {
    let dir = tempdir().unwrap();
    let csv_path = dir.path().join("exg_1700000000.csv");
    let labels = ["Fp1", "Fp2", "F3", "F4", "C3", "C4", "O1", "O2"];
    let mut state = CsvState::open_with_labels(&csv_path, &labels).unwrap();

    let ts = 1700000000.0;
    for ch in 0..8 {
        let samples: Vec<f64> = (0..5).map(|i| (ch * 100 + i) as f64).collect();
        state.push_eeg(ch, &samples, ts, 256.0);
    }
    drop(state);

    let content = std::fs::read_to_string(&csv_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 6, "header + 5 rows");
    // Each data row should have 9 columns (timestamp + 8 channels)
    assert_eq!(lines[1].split(',').count(), 9);
}

// ── Path helpers (already tested in session_csv_tests.rs, quick smoke) ───────

#[test]
fn parquet_path_helpers() {
    use skill_data::session_paths::{eeg_parquet_path, metrics_parquet_path, ppg_parquet_path};
    let base = Path::new("/data/exg_1700000000.csv");
    assert_eq!(
        eeg_parquet_path(base).file_name().unwrap().to_str().unwrap(),
        "exg_1700000000.parquet"
    );
    assert_eq!(
        ppg_parquet_path(base).file_name().unwrap().to_str().unwrap(),
        "exg_1700000000_ppg.parquet"
    );
    assert_eq!(
        metrics_parquet_path(base).file_name().unwrap().to_str().unwrap(),
        "exg_1700000000_metrics.parquet"
    );
}
