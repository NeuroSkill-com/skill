// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Unified session writer — dispatches to CSV or Parquet based on settings.

use std::path::Path;

use crate::ppg_analysis::PpgMetrics;
use crate::session_csv::CsvState;
use crate::session_parquet::ParquetState;
use skill_eeg::eeg_bands::BandSnapshot;

/// Storage format for EEG recordings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFormat {
    Csv,
    Parquet,
    Both,
}

impl StorageFormat {
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "parquet" => Self::Parquet,
            "both"    => Self::Both,
            _         => Self::Csv,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv     => "csv",
            Self::Parquet => "parquet",
            Self::Both    => "both",
        }
    }
}

/// Unified writer that delegates to either CSV or Parquet (or both).
pub enum SessionWriter {
    Csv(CsvState),
    Parquet(ParquetState),
    Both(CsvState, ParquetState),
}

impl SessionWriter {
    /// Open a new session file with the given channel labels.
    pub fn open(csv_path: &Path, labels: &[&str], format: StorageFormat) -> Result<Self, String> {
        match format {
            StorageFormat::Csv => {
                CsvState::open_with_labels(csv_path, labels)
                    .map(SessionWriter::Csv)
                    .map_err(|e| format!("CSV open error: {e}"))
            }
            StorageFormat::Parquet => {
                ParquetState::open_with_labels(csv_path, labels)
                    .map(SessionWriter::Parquet)
            }
            StorageFormat::Both => {
                let csv = CsvState::open_with_labels(csv_path, labels)
                    .map_err(|e| format!("CSV open error: {e}"))?;
                let pq = ParquetState::open_with_labels(csv_path, labels)?;
                Ok(SessionWriter::Both(csv, pq))
            }
        }
    }

    pub fn push_eeg(&mut self, electrode: usize, samples: &[f64], packet_ts: f64, sample_rate: f64) {
        match self {
            Self::Csv(c)            => c.push_eeg(electrode, samples, packet_ts, sample_rate),
            Self::Parquet(p)        => p.push_eeg(electrode, samples, packet_ts, sample_rate),
            Self::Both(c, p) => {
                c.push_eeg(electrode, samples, packet_ts, sample_rate);
                p.push_eeg(electrode, samples, packet_ts, sample_rate);
            }
        }
    }

    pub fn push_ppg(
        &mut self,
        eeg_csv_path: &Path,
        channel: usize,
        samples: &[f64],
        packet_ts: f64,
        ppg_vitals: Option<&PpgMetrics>,
    ) {
        match self {
            Self::Csv(c)     => c.push_ppg(eeg_csv_path, channel, samples, packet_ts, ppg_vitals),
            Self::Parquet(p) => p.push_ppg(eeg_csv_path, channel, samples, packet_ts, ppg_vitals),
            Self::Both(c, p) => {
                c.push_ppg(eeg_csv_path, channel, samples, packet_ts, ppg_vitals);
                p.push_ppg(eeg_csv_path, channel, samples, packet_ts, ppg_vitals);
            }
        }
    }

    pub fn push_metrics(&mut self, eeg_csv_path: &Path, snap: &BandSnapshot) {
        match self {
            Self::Csv(c)     => c.push_metrics(eeg_csv_path, snap),
            Self::Parquet(p) => p.push_metrics(eeg_csv_path, snap),
            Self::Both(c, p) => {
                c.push_metrics(eeg_csv_path, snap);
                p.push_metrics(eeg_csv_path, snap);
            }
        }
    }

    pub fn push_imu(
        &mut self,
        eeg_csv_path: &Path,
        timestamp_s:  f64,
        accel:        [f32; 3],
        gyro:         Option<[f32; 3]>,
        mag:          Option<[f32; 3]>,
    ) {
        match self {
            Self::Csv(c)     => c.push_imu(eeg_csv_path, timestamp_s, accel, gyro, mag),
            Self::Parquet(p) => p.push_imu(eeg_csv_path, timestamp_s, accel, gyro, mag),
            Self::Both(c, p) => {
                c.push_imu(eeg_csv_path, timestamp_s, accel, gyro, mag);
                p.push_imu(eeg_csv_path, timestamp_s, accel, gyro, mag);
            }
        }
    }

    pub fn flush(&mut self) {
        match self {
            Self::Csv(c)     => c.flush(),
            Self::Parquet(p) => p.flush(),
            Self::Both(c, p) => { c.flush(); p.flush(); }
        }
    }
}
