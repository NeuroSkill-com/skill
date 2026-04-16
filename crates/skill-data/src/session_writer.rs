// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Unified session writer — dispatches to CSV or Parquet based on settings.

use std::path::Path;

use crate::ppg_analysis::PpgMetrics;
use crate::session_csv::CsvState;
#[cfg(feature = "parquet")]
use crate::session_parquet::ParquetState;
use anyhow::Context;
use skill_eeg::eeg_bands::BandSnapshot;

/// Storage format for EEG recordings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFormat {
    Csv,
    #[cfg(feature = "parquet")]
    Parquet,
    #[cfg(feature = "parquet")]
    Both,
}

impl StorageFormat {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            #[cfg(feature = "parquet")]
            "parquet" => Self::Parquet,
            #[cfg(feature = "parquet")]
            "both" => Self::Both,
            _ => Self::Csv,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            #[cfg(feature = "parquet")]
            Self::Parquet => "parquet",
            #[cfg(feature = "parquet")]
            Self::Both => "both",
        }
    }
}

/// Unified writer that delegates to either CSV or Parquet (or both).
#[allow(clippy::large_enum_variant)]
pub enum SessionWriter {
    Csv(CsvState),
    #[cfg(feature = "parquet")]
    Parquet(ParquetState),
    #[cfg(feature = "parquet")]
    Both(CsvState, ParquetState),
}

macro_rules! dispatch {
    ($self:expr, $method:ident ( $($arg:expr),* $(,)? )) => {
        match $self {
            Self::Csv(c) => c.$method($($arg),*),
            #[cfg(feature = "parquet")]
            Self::Parquet(p) => p.$method($($arg),*),
            #[cfg(feature = "parquet")]
            Self::Both(c, p) => {
                c.$method($($arg),*);
                p.$method($($arg),*);
            }
        }
    };
}

impl SessionWriter {
    /// Open a new session file with the given channel labels.
    pub fn open(csv_path: &Path, labels: &[&str], format: StorageFormat) -> anyhow::Result<Self> {
        match format {
            StorageFormat::Csv => CsvState::open_with_labels(csv_path, labels)
                .map(SessionWriter::Csv)
                .context("CSV open error"),
            #[cfg(feature = "parquet")]
            StorageFormat::Parquet => ParquetState::open_with_labels(csv_path, labels).map(SessionWriter::Parquet),
            #[cfg(feature = "parquet")]
            StorageFormat::Both => {
                let csv = CsvState::open_with_labels(csv_path, labels).context("CSV open error")?;
                let pq = ParquetState::open_with_labels(csv_path, labels)?;
                Ok(SessionWriter::Both(csv, pq))
            }
        }
    }

    pub fn push_eeg(&mut self, electrode: usize, samples: &[f64], packet_ts: f64, sample_rate: f64) {
        dispatch!(self, push_eeg(electrode, samples, packet_ts, sample_rate));
    }

    pub fn push_ppg(
        &mut self,
        eeg_csv_path: &Path,
        channel: usize,
        samples: &[f64],
        packet_ts: f64,
        ppg_vitals: Option<&PpgMetrics>,
    ) {
        dispatch!(self, push_ppg(eeg_csv_path, channel, samples, packet_ts, ppg_vitals));
    }

    pub fn push_metrics(&mut self, eeg_csv_path: &Path, snap: &BandSnapshot) {
        dispatch!(self, push_metrics(eeg_csv_path, snap));
    }

    pub fn push_fnirs(&mut self, eeg_csv_path: &Path, channels: &[f64], channel_names: &[String], timestamp_s: f64) {
        dispatch!(self, push_fnirs(eeg_csv_path, channels, channel_names, timestamp_s));
    }

    pub fn push_imu(
        &mut self,
        eeg_csv_path: &Path,
        timestamp_s: f64,
        accel: [f32; 3],
        gyro: Option<[f32; 3]>,
        mag: Option<[f32; 3]>,
    ) {
        dispatch!(self, push_imu(eeg_csv_path, timestamp_s, accel, gyro, mag));
    }

    pub fn flush(&mut self) {
        dispatch!(self, flush());
    }
}

#[cfg(test)]
mod tests {
    use super::StorageFormat;

    #[test]
    fn storage_format_parse_csv() {
        assert_eq!(StorageFormat::parse("csv"), StorageFormat::Csv);
        assert_eq!(StorageFormat::parse("CSV"), StorageFormat::Csv);
        assert_eq!(StorageFormat::parse("unknown"), StorageFormat::Csv);
        assert_eq!(StorageFormat::parse(""), StorageFormat::Csv);
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn storage_format_parse_parquet() {
        assert_eq!(StorageFormat::parse("parquet"), StorageFormat::Parquet);
        assert_eq!(StorageFormat::parse("PARQUET"), StorageFormat::Parquet);
        assert_eq!(StorageFormat::parse("Parquet"), StorageFormat::Parquet);
    }

    #[cfg(feature = "parquet")]
    #[test]
    fn storage_format_parse_both() {
        assert_eq!(StorageFormat::parse("both"), StorageFormat::Both);
        assert_eq!(StorageFormat::parse("BOTH"), StorageFormat::Both);
    }

    #[test]
    fn storage_format_as_str_roundtrip() {
        assert_eq!(StorageFormat::Csv.as_str(), "csv");
        #[cfg(feature = "parquet")]
        {
            assert_eq!(StorageFormat::Parquet.as_str(), "parquet");
            assert_eq!(StorageFormat::Both.as_str(), "both");
        }
    }
}
