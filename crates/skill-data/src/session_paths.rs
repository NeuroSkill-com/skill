// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Parquet path helpers — pure path manipulation, no parquet dependency.

use std::path::{Path, PathBuf};

use crate::session_csv::{metrics_csv_path, ppg_csv_path};

/// Convert a `.csv` path to `.parquet`.
fn to_parquet_ext(p: &Path) -> PathBuf {
    p.with_extension("parquet")
}

/// Parquet EEG path from EEG CSV path.
pub fn eeg_parquet_path(csv_path: &Path) -> PathBuf {
    to_parquet_ext(csv_path)
}

/// Parquet PPG path from EEG CSV path.
pub fn ppg_parquet_path(csv_path: &Path) -> PathBuf {
    to_parquet_ext(&ppg_csv_path(csv_path))
}

/// Parquet metrics path from EEG CSV path.
pub fn metrics_parquet_path(csv_path: &Path) -> PathBuf {
    to_parquet_ext(&metrics_csv_path(csv_path))
}

/// Parquet IMU path from EEG CSV path.
pub fn imu_parquet_path(csv_path: &Path) -> PathBuf {
    to_parquet_ext(&crate::session_csv::imu_csv_path(csv_path))
}

/// Parquet fNIRS path from EEG CSV path.
pub fn fnirs_parquet_path(csv_path: &Path) -> PathBuf {
    to_parquet_ext(&crate::session_csv::fnirs_csv_path(csv_path))
}
