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

/// With `storage_format = "auto"`, streams of at least this many values per
/// second (channels × sample rate) are recorded as Parquet: a 4-channel
/// 256 Hz headband (1 024) or 8 channels at 500 Hz (4 000) stay CSV, 32
/// channels at 256 Hz (8 192) or 500 Hz (16 000) go to Parquet. At the
/// threshold a CSV recording grows by roughly 120 MB an hour.
pub const AUTO_PARQUET_VALUES_PER_SEC: f64 = 4096.0;

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

    /// The format for a stream of `channels` at `sample_rate` Hz under the
    /// `storage_format` setting. `"auto"` picks by how much data the stream
    /// makes: CSV (readable anywhere) for small streams, Parquet (a third of
    /// the size, crash-recoverable) from [`AUTO_PARQUET_VALUES_PER_SEC`] up.
    /// Every other value is taken as [`parse`](Self::parse) reads it.
    pub fn for_stream(setting: &str, channels: usize, sample_rate: f64) -> Self {
        if !setting.eq_ignore_ascii_case("auto") {
            return Self::parse(setting);
        }
        #[cfg(feature = "parquet")]
        if channels as f64 * sample_rate >= AUTO_PARQUET_VALUES_PER_SEC {
            return Self::Parquet;
        }
        let _ = (channels, sample_rate);
        Self::Csv
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

    /// Declare that the EEG samples are whole counts of `scale_uv` microvolts
    /// (`0.001`: nanovolts), so that a Parquet recording stores them as
    /// integers — exact, and about a third of the size (see
    /// [`ParquetState::set_eeg_scale_uv`]). Only before the first sample.
    /// CSV is text and is written the same either way. Returns whether the
    /// recording now stores integers.
    pub fn set_eeg_scale_uv(&mut self, scale_uv: f64) -> anyhow::Result<bool> {
        match self {
            Self::Csv(_) => {
                let _ = scale_uv;
                Ok(false)
            }
            #[cfg(feature = "parquet")]
            Self::Parquet(p) | Self::Both(_, p) => p.set_eeg_scale_uv(scale_uv),
        }
    }

    pub fn push_eeg(&mut self, electrode: usize, samples: &[f64], packet_ts: f64, sample_rate: f64) {
        dispatch!(self, push_eeg(electrode, samples, packet_ts, sample_rate));
    }

    /// One complete row: a sample of every channel taken at `ts`. Same result
    /// as `push_eeg` once per electrode, without the per-electrode queueing.
    pub fn push_eeg_frame(&mut self, channels: &[f64], ts: f64, sample_rate: f64) {
        dispatch!(self, push_eeg_frame(channels, ts, sample_rate));
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

    pub fn push_vitals(
        &mut self,
        eeg_csv_path: &Path,
        timestamp_s: f64,
        hr_bpm: Option<f64>,
        ibi_ms: Option<f64>,
        spo2_pct: Option<f64>,
        temperature_c: Option<f64>,
    ) {
        dispatch!(
            self,
            push_vitals(eeg_csv_path, timestamp_s, hr_bpm, ibi_ms, spo2_pct, temperature_c)
        );
    }

    /// One sample of a scalar sensor stream (electrodermal activity, raw PPG
    /// counts, ...), recorded to the `_sensors` sidecar in long format.
    pub fn push_sensor(&mut self, eeg_csv_path: &Path, channel: &str, unit: &str, timestamp_s: f64, value: f64) {
        dispatch!(self, push_sensor(eeg_csv_path, channel, unit, timestamp_s, value));
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

    /// Raw EEG rows handed to storage and the number of failed writes, summed
    /// over the open formats. A non-zero error count means samples are being
    /// lost to the disk (full, unplugged, I/O error).
    pub fn eeg_write_counts(&self) -> (u64, u64) {
        match self {
            Self::Csv(c) => c.eeg_write_counts(),
            #[cfg(feature = "parquet")]
            Self::Parquet(p) => (p.eeg_stats().rows, p.eeg_stats().errors),
            #[cfg(feature = "parquet")]
            Self::Both(c, p) => (c.eeg_write_counts().0, c.eeg_write_counts().1 + p.eeg_stats().errors),
        }
    }

    /// Finalise underlying writers (writes Parquet footer; no-op for CSV).
    /// Idempotent. Drop also calls this for crash safety, but calling it
    /// explicitly gives deterministic ordering and surfaces errors via logs.
    pub fn close(&mut self) {
        match self {
            Self::Csv(_) => {}
            #[cfg(feature = "parquet")]
            Self::Parquet(p) => p.close(),
            #[cfg(feature = "parquet")]
            Self::Both(_, p) => p.close(),
        }
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

    #[test]
    fn auto_follows_the_data_rate_and_explicit_settings_are_kept() {
        // small streams stay CSV
        assert_eq!(StorageFormat::for_stream("auto", 4, 256.0), StorageFormat::Csv);
        assert_eq!(StorageFormat::for_stream("AUTO", 8, 500.0), StorageFormat::Csv);
        #[cfg(feature = "parquet")]
        {
            assert_eq!(StorageFormat::for_stream("auto", 16, 256.0), StorageFormat::Parquet);
            assert_eq!(StorageFormat::for_stream("auto", 32, 500.0), StorageFormat::Parquet);
            // an explicit choice wins whatever the stream
            assert_eq!(StorageFormat::for_stream("csv", 32, 500.0), StorageFormat::Csv);
            assert_eq!(StorageFormat::for_stream("parquet", 4, 256.0), StorageFormat::Parquet);
            assert_eq!(StorageFormat::for_stream("both", 4, 256.0), StorageFormat::Both);
        }
        // a plain parse never picks for itself
        assert_eq!(StorageFormat::parse("auto"), StorageFormat::Csv);
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
