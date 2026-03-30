// SPDX-License-Identifier: GPL-3.0-only
//! Tests for session CSV path helpers.
#![allow(clippy::unwrap_used)]

use skill_data::session_csv::{imu_csv_path, metrics_csv_path, ppg_csv_path};
use std::path::Path;

#[test]
fn ppg_csv_path_standard() {
    let p = ppg_csv_path(Path::new("/data/20260101/exg_1700000000.csv"));
    assert_eq!(p.file_name().unwrap().to_str().unwrap(), "exg_1700000000_ppg.csv");
}

#[test]
fn metrics_csv_path_standard() {
    let p = metrics_csv_path(Path::new("/data/20260101/exg_1700000000.csv"));
    assert_eq!(p.file_name().unwrap().to_str().unwrap(), "exg_1700000000_metrics.csv");
}

#[test]
fn imu_csv_path_standard() {
    let p = imu_csv_path(Path::new("/data/20260101/exg_1700000000.csv"));
    assert_eq!(p.file_name().unwrap().to_str().unwrap(), "exg_1700000000_imu.csv");
}

#[test]
fn ppg_csv_path_legacy_prefix() {
    let p = ppg_csv_path(Path::new("muse_1700000000.csv"));
    assert_eq!(p.file_name().unwrap().to_str().unwrap(), "muse_1700000000_ppg.csv");
}

#[test]
fn path_helpers_preserve_directory() {
    let base = Path::new("/home/user/skill/20260315/exg_1742025600.csv");
    let ppg = ppg_csv_path(base);
    let met = metrics_csv_path(base);
    let imu = imu_csv_path(base);

    // All should be in the same directory
    assert_eq!(ppg.parent(), base.parent());
    assert_eq!(met.parent(), base.parent());
    assert_eq!(imu.parent(), base.parent());
}

#[test]
fn parquet_base_path_works() {
    // Even if the input has .parquet extension, helpers still use the stem
    let p = ppg_csv_path(Path::new("/data/exg_1700000000.parquet"));
    assert_eq!(p.file_name().unwrap().to_str().unwrap(), "exg_1700000000_ppg.csv");
}
