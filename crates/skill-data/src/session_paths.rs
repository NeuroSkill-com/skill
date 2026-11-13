// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Parquet path helpers — pure path manipulation, no parquet dependency.

use std::path::{Path, PathBuf};

use crate::session_csv::{metrics_csv_path, ppg_csv_path};

/// Suffix (before the extension) of every file written beside a recording's
/// primary EEG file.
///
/// The one list that history listing, session deletion and the EEG-file filter
/// all consult. They used to keep three hand-written lists, and a new sidecar
/// (`_fnirs`, `_vitals`) was added to the writer without being added to them:
/// it was then miscounted as a session and left behind by a delete. Adding a
/// sidecar means adding its suffix here, and `every_path_helper_uses_a_listed_suffix`
/// fails if a helper below drifts from it.
pub const SIDECAR_SUFFIXES: &[&str] = &["_metrics", "_ppg", "_imu", "_fnirs", "_vitals", "_sensors"];

/// True for a sidecar's file name (`exg_1700000000_vitals.csv`), as opposed to a
/// primary recording (`exg_1700000000.csv`).
pub fn is_sidecar_file(fname: &str) -> bool {
    fname
        .strip_suffix(".csv")
        .or_else(|| fname.strip_suffix(".parquet"))
        .is_some_and(|stem| SIDECAR_SUFFIXES.iter().any(|suffix| stem.ends_with(suffix)))
}

/// Every sidecar path, CSV and Parquet, that may exist beside `eeg_path`.
///
/// Whether each exists depends on the device and the storage format, so callers
/// that delete or move a session check existence per path.
pub fn sidecar_paths(eeg_path: &Path) -> Vec<PathBuf> {
    let stem = eeg_path.file_stem().and_then(|s| s.to_str()).unwrap_or("exg");
    SIDECAR_SUFFIXES
        .iter()
        .flat_map(|suffix| ["csv", "parquet"].map(|ext| eeg_path.with_file_name(format!("{stem}{suffix}.{ext}"))))
        .collect()
}

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

/// Parquet vitals path from EEG CSV path.
pub fn vitals_parquet_path(csv_path: &Path) -> PathBuf {
    to_parquet_ext(&crate::session_csv::vitals_csv_path(csv_path))
}

/// Parquet sensors path from EEG CSV path.
pub fn sensors_parquet_path(csv_path: &Path) -> PathBuf {
    to_parquet_ext(&crate::session_csv::sensors_csv_path(csv_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A primary recording is not a sidecar, whatever its extension.
    #[test]
    fn primary_recordings_are_not_sidecars() {
        for name in ["exg_1700000000.csv", "exg_1700000000.parquet", "muse_1700000000.csv"] {
            assert!(!is_sidecar_file(name), "{name}");
        }
    }

    #[test]
    fn every_sidecar_kind_is_recognised_in_both_formats() {
        for suffix in SIDECAR_SUFFIXES {
            for ext in ["csv", "parquet"] {
                let name = format!("exg_1700000000{suffix}.{ext}");
                assert!(is_sidecar_file(&name), "{name}");
            }
        }
    }

    /// The JSON metadata and cache files are not data sidecars.
    #[test]
    fn non_data_files_are_not_sidecars() {
        for name in ["exg_1700000000.json", "exg_1700000000_metrics_cache.json", "notes.txt"] {
            assert!(!is_sidecar_file(name), "{name}");
        }
    }

    #[test]
    fn sidecar_paths_cover_every_suffix_and_format() {
        let paths = sidecar_paths(Path::new("/data/exg_1700000000.csv"));
        assert_eq!(paths.len(), SIDECAR_SUFFIXES.len() * 2);
        assert!(paths.contains(&PathBuf::from("/data/exg_1700000000_vitals.parquet")));
        assert!(paths.contains(&PathBuf::from("/data/exg_1700000000_sensors.csv")));
        assert!(
            !paths.contains(&PathBuf::from("/data/exg_1700000000.csv")),
            "never the primary file"
        );
    }

    /// Each path helper must produce a name `is_sidecar_file` recognises, so the
    /// list cannot drift from the writers that create these files.
    #[test]
    fn every_path_helper_uses_a_listed_suffix() {
        use crate::session_csv::{fnirs_csv_path, imu_csv_path, sensors_csv_path, vitals_csv_path};
        let eeg = Path::new("/data/exg_1700000000.csv");
        let csvs = [
            metrics_csv_path(eeg),
            ppg_csv_path(eeg),
            imu_csv_path(eeg),
            fnirs_csv_path(eeg),
            vitals_csv_path(eeg),
            sensors_csv_path(eeg),
        ];
        for p in csvs {
            let name = p.file_name().unwrap().to_str().unwrap().to_string();
            assert!(is_sidecar_file(&name), "{name} is written but not in SIDECAR_SUFFIXES");
            assert!(sidecar_paths(eeg).contains(&p), "{name} missing from sidecar_paths");
        }
    }
}
