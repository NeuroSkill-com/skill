// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//

//! Parquet recording writer — drop-in alternative to CSV.
//!
//! Stores EEG, PPG, and metrics data in columnar Apache Parquet files.
//! Enabled when `storage_format = "parquet"` in settings.
//!
//! The raw EEG file is written for size and for survival:
//!
//! - **Size.** Samples are buffered in columns and written as one row group
//!   every [`EEG_ROW_GROUP_SECS`] of signal (sized from the stream's own rate,
//!   so a 4-channel 256 Hz headband and a 32-channel 500 Hz cap both get row
//!   groups of the same duration), byte-stream-split and ZSTD compressed.
//! - **Survival.** A Parquet file has no meaning until its footer is written,
//!   and that only happens at `close()`. So after every row group the writer
//!   syncs the data and stores, beside it, the footer the file would have if it
//!   ended right there (`.parquet.ckpt`); and the rows of the row group still in
//!   memory are appended to a small journal (`.parquet.tail`) as they arrive.
//!   [`recover_eeg_parquet`] turns what a crash, a kill or a power cut leaves
//!   behind back into a complete file. Both side files go away on `close()`.
//!
//! Files produced:
//! - `exg_<ts>.parquet` — raw EEG samples (timestamp + N channel columns)
//! - `exg_<ts>_ppg.parquet` — PPG optical data
//! - `exg_<ts>_metrics.parquet` — derived band-power metrics (~4 Hz)

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::{Float64Array, Int64Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::ppg_analysis::PpgMetrics;
use crate::session_csv::{build_metrics_header, PPG_SAMPLE_RATE};
use anyhow::Context;
use skill_eeg::eeg_bands::BandSnapshot;

// ── Row-group flush threshold ─────────────────────────────────────────────────

/// Seconds of signal per EEG row group. Bounds the writer's memory, and is
/// the most a power cut can cost (a crash or kill costs [`EEG_TAIL_SECS`]).
pub const EEG_ROW_GROUP_SECS: f64 = 10.0;
/// Row-group size limits, whatever the sample rate says.
const EEG_ROW_GROUP_MIN_ROWS: usize = 1024;
const EEG_ROW_GROUP_MAX_ROWS: usize = 1 << 16;
/// Row-group size until the first sample has told the writer the rate.
const EEG_ROW_GROUP_DEFAULT_ROWS: usize = 4096;
/// Seconds of signal that may sit in memory before going to the tail journal.
pub const EEG_TAIL_SECS: f64 = 0.25;
const PPG_FLUSH_ROWS: usize = 1024;
/// Band-metrics rows per row group: a minute of them, or this many rows,
/// whichever comes first. The metrics file is wide — twelve columns per
/// channel, 432 for a 32-channel cap — and every row group costs a page
/// header and a footer entry *per column*, so a row group of a few rows is
/// almost all overhead. (It used to be cut on every `flush()`, twice a second:
/// 32 channels wrote 700 MB an hour, nearly all of it headers.) Metrics are
/// derived from the raw signal, so the minute a crash can cost is recomputable.
const METRICS_ROW_GROUP_SECS: f64 = 60.0;
const METRICS_ROW_GROUP_MAX_ROWS: usize = 1024;

// ── Path helpers (re-exported from session_paths) ────────────────────────────

pub use crate::session_paths::*;

// ── Writer properties ─────────────────────────────────────────────────────────

fn writer_props() -> WriterProperties {
    WriterProperties::builder().set_compression(Compression::SNAPPY).build()
}

/// Properties for the raw EEG file: every column is a dense `f64` signal, for
/// which a dictionary only costs time and memory, and whose bytes compress far
/// better split by significance (the high bytes of neighbouring samples are
/// nearly constant) than interleaved. Row groups are cut by the writer itself.
fn eeg_writer_props() -> WriterProperties {
    WriterProperties::builder()
        .set_compression(Compression::ZSTD(parquet::basic::ZstdLevel::default()))
        .set_dictionary_enabled(false)
        .set_encoding(parquet::basic::Encoding::BYTE_STREAM_SPLIT)
        .set_max_row_group_row_count(Some(usize::MAX))
        .build()
}

/// Properties for the band-metrics file. Its values are `f32`s stored in
/// `f64` columns (the low four bytes of every value are zero) and many columns
/// are constant for a whole session (no PPG: all NaN), which byte-stream-split
/// plus ZSTD reduces to almost nothing; min/max statistics for hundreds of
/// columns nobody filters on would only pad the footer.
fn metrics_writer_props() -> WriterProperties {
    WriterProperties::builder()
        .set_compression(Compression::ZSTD(parquet::basic::ZstdLevel::default()))
        .set_dictionary_enabled(false)
        .set_encoding(parquet::basic::Encoding::BYTE_STREAM_SPLIT)
        .set_statistics_enabled(parquet::file::properties::EnabledStatistics::None)
        .set_max_row_group_row_count(Some(usize::MAX))
        .build()
}

/// File metadata key of an EEG file whose channel columns are integers: the
/// microvolts one count stands for (`0.001` for nanovolts). Absent from files
/// that store microvolts as `f64`.
pub const EEG_SCALE_KEY: &str = "skill.eeg_scale_uv";

fn eeg_scale_metadata(scale_uv: f64) -> parquet::file::metadata::KeyValue {
    parquet::file::metadata::KeyValue::new(EEG_SCALE_KEY.to_string(), scale_uv.to_string())
}

/// Properties for a raw EEG file stored as integer counts: neighbouring
/// samples of a signal differ by little, so the deltas pack into a few bits
/// each — about a third of the bytes the same signal takes as `f64`.
fn eeg_counts_writer_props(scale_uv: f64) -> WriterProperties {
    WriterProperties::builder()
        .set_compression(Compression::ZSTD(parquet::basic::ZstdLevel::default()))
        .set_dictionary_enabled(false)
        .set_encoding(parquet::basic::Encoding::DELTA_BINARY_PACKED)
        .set_column_encoding(
            parquet::schema::types::ColumnPath::from("timestamp_s"),
            parquet::basic::Encoding::BYTE_STREAM_SPLIT,
        )
        .set_max_row_group_row_count(Some(usize::MAX))
        .set_key_value_metadata(Some(vec![eeg_scale_metadata(scale_uv)]))
        .build()
}

fn eeg_props_for(scale_uv: Option<f64>) -> WriterProperties {
    scale_uv.map_or_else(eeg_writer_props, eeg_counts_writer_props)
}

/// The scale a file's metadata declares, if it stores counts.
fn eeg_scale_of(schema: &Schema) -> Option<f64> {
    schema
        .metadata()
        .get(EEG_SCALE_KEY)?
        .parse()
        .ok()
        .filter(|q: &f64| q.is_finite() && *q > 0.0)
}

/// Microvolts as counts of `scale_uv`; a missing value (NaN) is a null.
fn counts_array(values: &[f64], scale_uv: f64) -> Int64Array {
    values
        .iter()
        .map(|&v| v.is_finite().then(|| (v / scale_uv).round() as i64))
        .collect()
}

/// A channel column of an EEG file, read as microvolts whichever way it is
/// stored.
enum EegColumn<'a> {
    Microvolts(&'a Float64Array),
    Counts(&'a Int64Array, f64),
}

impl<'a> EegColumn<'a> {
    fn new(col: &'a dyn arrow_array::Array, scale_uv: Option<f64>) -> Option<Self> {
        let any = col.as_any();
        any.downcast_ref::<Float64Array>().map(Self::Microvolts).or_else(|| {
            any.downcast_ref::<Int64Array>()
                .map(|c| Self::Counts(c, scale_uv.unwrap_or(1.0)))
        })
    }

    fn get(&self, i: usize) -> Option<f64> {
        use arrow_array::Array;
        match self {
            Self::Microvolts(c) => (!c.is_null(i)).then(|| c.value(i)),
            Self::Counts(c, q) => (!c.is_null(i)).then(|| c.value(i) as f64 * q),
        }
    }
}

// ── EEG crash journal ─────────────────────────────────────────────────────────

/// `<file>.parquet.ckpt`: the end of the last synced row group and the footer
/// for a file that ends there.
pub fn eeg_checkpoint_path(pq_path: &Path) -> PathBuf {
    side_path(pq_path, "ckpt")
}

/// `<file>.parquet.tail`: the rows that are not in a row group yet.
pub fn eeg_tail_path(pq_path: &Path) -> PathBuf {
    side_path(pq_path, "tail")
}

fn side_path(pq_path: &Path, ext: &str) -> PathBuf {
    let mut name = pq_path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(ext);
    pq_path.with_file_name(name)
}

/// First line of the tail journal; the rows follow as little-endian `f64`s,
/// `1 + channels` per row (timestamp first).
#[derive(serde::Serialize, serde::Deserialize)]
struct TailHeader {
    magic: String,
    channels: Vec<String>,
    /// Set when the file stores counts ([`EEG_SCALE_KEY`]); the journal rows
    /// are microvolts either way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scale_uv: Option<f64>,
}

const TAIL_MAGIC: &str = "skill-eeg-tail-1";

fn eeg_schema_for(labels: &[&str]) -> Arc<Schema> {
    eeg_schema_scaled(labels, None)
}

/// The EEG schema: channels as `f64` microvolts, or with a scale as `i64`
/// counts of it.
fn eeg_schema_scaled(labels: &[&str], scale_uv: Option<f64>) -> Arc<Schema> {
    let ty = if scale_uv.is_some() {
        DataType::Int64
    } else {
        DataType::Float64
    };
    let mut fields = vec![Field::new("timestamp_s", DataType::Float64, false)];
    for label in labels {
        fields.push(Field::new(*label, ty.clone(), true));
    }
    Arc::new(Schema::new(fields))
}

/// Size and throughput of the raw EEG file so far.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EegWriteStats {
    /// Rows handed to the writer (in row groups or in the journal).
    pub rows: u64,
    /// Rows in synced row groups.
    pub rows_durable: u64,
    pub row_groups: u64,
    /// Bytes of the Parquet file on disk.
    pub bytes: u64,
    /// Failed writes (disk full, I/O error). Not zero means data is being lost.
    pub errors: u64,
    /// Time spent encoding and writing, in seconds.
    pub write_secs: f64,
}

/// One buffered vitals row: `(timestamp, hr_bpm, ibi_ms, spo2_pct, temperature_c)`.
///
/// Named because the tuple is otherwise five near-identical optionals whose
/// meaning is carried only by position.
type VitalsRow = (f64, Option<f64>, Option<f64>, Option<f64>, Option<f64>);

/// Schema for `_vitals.parquet`.  Fixed, and every value column nullable.
fn vitals_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("timestamp_s", DataType::Float64, false),
        Field::new("hr_bpm", DataType::Float64, true),
        Field::new("ibi_ms", DataType::Float64, true),
        Field::new("spo2_pct", DataType::Float64, true),
        Field::new("temperature_c", DataType::Float64, true),
    ]))
}

/// One buffered sensor sample: `(timestamp_s, channel, unit, value)`.
type SensorRow = (f64, String, String, f64);

/// Schema for `_sensors.parquet`: long format, one row per sample. The channel
/// and unit are strings repeated on every row, which Parquet's dictionary
/// encoding stores as a few bits each.
fn sensors_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("timestamp_s", DataType::Float64, false),
        Field::new("channel", DataType::Utf8, false),
        Field::new("unit", DataType::Utf8, false),
        Field::new("value", DataType::Float64, false),
    ]))
}

// ── ParquetState ──────────────────────────────────────────────────────────────

/// Multiplexed Parquet writer for a recording session.
///
/// API-compatible with `CsvState`: call `push_eeg`, `push_ppg`, `push_metrics`,
/// and `flush` in the same way.
pub struct ParquetState {
    // ── EEG ──────────────────────────────────────────────────────────────────
    eeg_wtr: Option<ArrowWriter<std::fs::File>>,
    eeg_schema: Arc<Schema>,
    /// Microvolts per count when the channels are stored as integers.
    eeg_scale: Option<f64>,
    eeg_path: PathBuf,
    n_eeg: usize,
    /// Per-channel queues that line samples up into rows.
    eeg_ts: Vec<VecDeque<f64>>,
    eeg_bufs: Vec<VecDeque<f64>>,
    /// Complete rows waiting for their row group: timestamps, then one column
    /// per channel.
    eeg_pending_ts: Vec<f64>,
    eeg_pending: Vec<Vec<f64>>,
    /// How many of the pending rows are already in the tail journal.
    eeg_journaled: usize,
    eeg_tail: Option<std::fs::File>,
    /// Rows per row group / per journal append, from the sample rate.
    eeg_group_rows: usize,
    eeg_tail_rows: usize,
    eeg_rate_seen: f64,
    eeg_stats: EegWriteStats,

    // ── PPG (lazy) ───────────────────────────────────────────────────────────
    ppg_wtr: Option<ArrowWriter<std::fs::File>>,
    ppg_schema: Arc<Schema>,
    ppg_bufs: [VecDeque<f64>; 3],
    ppg_ts: [VecDeque<f64>; 3],
    ppg_rows: usize,
    ppg_path: PathBuf,

    // ── Metrics (lazy) ───────────────────────────────────────────────────────
    metrics_wtr: Option<ArrowWriter<std::fs::File>>,
    metrics_schema: Arc<Schema>,
    metrics_n_cols: usize,
    metrics_rows: usize,
    metrics_path: PathBuf,

    /// Accumulated metrics rows before flush.
    metrics_pending: Vec<Vec<f64>>,

    // ── IMU (lazy) ───────────────────────────────────────────────────────────
    imu_wtr: Option<ArrowWriter<std::fs::File>>,
    imu_schema: Arc<Schema>,
    imu_rows: usize,
    imu_path: PathBuf,
    imu_pending: Vec<[f64; 10]>,

    // ── fNIRS (lazy) ─────────────────────────────────────────────────────────
    fnirs_wtr: Option<ArrowWriter<std::fs::File>>,
    fnirs_schema: Arc<Schema>,
    fnirs_rows: usize,
    fnirs_path: PathBuf,
    fnirs_pending: Vec<Vec<f64>>, // [timestamp_s, ch0, ch1, ...]

    // ── Vitals ───────────────────────────────────────────────────────────────
    vitals_wtr: Option<ArrowWriter<std::fs::File>>,
    vitals_rows: usize,
    vitals_path: PathBuf,
    /// Every column but the timestamp is nullable, because a wearable reports
    /// one vital at a time.
    vitals_pending: Vec<VitalsRow>,

    // ── Scalar sensor streams ────────────────────────────────────────────────
    sensors_wtr: Option<ArrowWriter<std::fs::File>>,
    sensors_path: PathBuf,
    sensors_pending: Vec<SensorRow>,
}

impl ParquetState {
    /// Open a new Parquet EEG file with the given channel labels.
    pub fn open_with_labels(csv_path: &Path, labels: &[&str]) -> anyhow::Result<Self> {
        let pq_path = eeg_parquet_path(csv_path);
        let n = labels.len();

        let eeg_schema = eeg_schema_for(labels);

        let file = std::fs::File::create(&pq_path).with_context(|| format!("parquet create {}", pq_path.display()))?;
        let eeg_wtr =
            ArrowWriter::try_new(file, eeg_schema.clone(), Some(eeg_writer_props())).context("parquet writer")?;
        // The journal is what makes the recording survive a crash; a session
        // that cannot have one still records.
        let eeg_tail = open_tail(&pq_path, labels, None)
            .map_err(|e| eprintln!("[parquet] EEG tail journal unavailable for {}: {e}", pq_path.display()))
            .ok();

        // PPG schema
        let ppg_fields = vec![
            Field::new("timestamp_s", DataType::Float64, false),
            Field::new("ambient", DataType::Float64, true),
            Field::new("infrared", DataType::Float64, true),
            Field::new("red", DataType::Float64, true),
            Field::new("hr_bpm", DataType::Float64, true),
            Field::new("rmssd_ms", DataType::Float64, true),
            Field::new("sdnn_ms", DataType::Float64, true),
            Field::new("pnn50_pct", DataType::Float64, true),
            Field::new("lf_hf_ratio", DataType::Float64, true),
            Field::new("respiratory_rate_bpm", DataType::Float64, true),
            Field::new("spo2_pct", DataType::Float64, true),
            Field::new("perfusion_index_pct", DataType::Float64, true),
            Field::new("stress_index", DataType::Float64, true),
        ];
        let ppg_schema = Arc::new(Schema::new(ppg_fields));

        // Metrics schema: dynamic columns from channel labels + cross-channel indices
        let metrics_header = build_metrics_header(labels);
        let metrics_fields: Vec<Field> = metrics_header
            .iter()
            .map(|name| Field::new(name, DataType::Float64, true))
            .collect();
        let n_metrics_cols = metrics_fields.len();
        let metrics_schema = Arc::new(Schema::new(metrics_fields));

        // IMU schema
        let imu_fields = vec![
            Field::new("timestamp_s", DataType::Float64, false),
            Field::new("accel_x", DataType::Float64, true),
            Field::new("accel_y", DataType::Float64, true),
            Field::new("accel_z", DataType::Float64, true),
            Field::new("gyro_x", DataType::Float64, true),
            Field::new("gyro_y", DataType::Float64, true),
            Field::new("gyro_z", DataType::Float64, true),
            Field::new("mag_x", DataType::Float64, true),
            Field::new("mag_y", DataType::Float64, true),
            Field::new("mag_z", DataType::Float64, true),
        ];
        let imu_schema = Arc::new(Schema::new(imu_fields));

        Ok(Self {
            eeg_wtr: Some(eeg_wtr),
            eeg_schema,
            eeg_scale: None,
            eeg_path: pq_path,
            n_eeg: n,
            eeg_ts: (0..n).map(|_| VecDeque::new()).collect(),
            eeg_bufs: (0..n).map(|_| VecDeque::new()).collect(),
            eeg_pending_ts: Vec::new(),
            eeg_pending: (0..n).map(|_| Vec::new()).collect(),
            eeg_journaled: 0,
            eeg_tail,
            eeg_group_rows: EEG_ROW_GROUP_DEFAULT_ROWS,
            eeg_tail_rows: 64,
            eeg_rate_seen: 0.0,
            eeg_stats: EegWriteStats::default(),

            ppg_wtr: None,
            ppg_schema,
            ppg_bufs: std::array::from_fn(|_| VecDeque::new()),
            ppg_ts: std::array::from_fn(|_| VecDeque::new()),
            ppg_rows: 0,
            ppg_path: ppg_parquet_path(csv_path),

            metrics_wtr: None,
            metrics_schema,
            metrics_n_cols: n_metrics_cols,
            metrics_rows: 0,
            metrics_path: metrics_parquet_path(csv_path),
            metrics_pending: Vec::new(),

            imu_wtr: None,
            imu_schema,
            imu_rows: 0,
            imu_path: imu_parquet_path(csv_path),
            imu_pending: Vec::new(),

            fnirs_wtr: None,
            // Schema is built lazily in push_fnirs once channel names are known.
            fnirs_schema: Arc::new(Schema::empty()),
            fnirs_rows: 0,
            fnirs_path: fnirs_parquet_path(csv_path),
            fnirs_pending: Vec::new(),
            vitals_wtr: None,
            vitals_rows: 0,
            vitals_path: crate::session_paths::vitals_parquet_path(csv_path),
            vitals_pending: Vec::new(),
            sensors_wtr: None,
            sensors_path: crate::session_paths::sensors_parquet_path(csv_path),
            sensors_pending: Vec::new(),
        })
    }

    // ── EEG ──────────────────────────────────────────────────────────────────

    /// Store the EEG channels as integer counts of `scale_uv` microvolts
    /// (`0.001`: nanovolts) instead of `f64` microvolts — for a source whose
    /// samples are integers, which this keeps exact in about a third of the
    /// bytes. Samples are still pushed in microvolts and read back in
    /// microvolts; a value that is not a whole number of counts is rounded to
    /// one. Takes effect only before the first sample: returns whether it did.
    pub fn set_eeg_scale_uv(&mut self, scale_uv: f64) -> anyhow::Result<bool> {
        let untouched = self.eeg_stats.rows == 0 && self.eeg_bufs.iter().all(VecDeque::is_empty);
        if !(scale_uv.is_finite() && scale_uv > 0.0) || !untouched || self.eeg_wtr.is_none() {
            return Ok(false);
        }
        let labels: Vec<String> = self
            .eeg_schema
            .fields()
            .iter()
            .skip(1)
            .map(|f| f.name().clone())
            .collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        let schema = eeg_schema_scaled(&labels, Some(scale_uv));
        // Start the (still empty) file again under the new schema.
        self.eeg_wtr = None;
        let file = std::fs::File::create(&self.eeg_path)
            .with_context(|| format!("parquet create {}", self.eeg_path.display()))?;
        self.eeg_wtr = Some(
            ArrowWriter::try_new(file, schema.clone(), Some(eeg_counts_writer_props(scale_uv)))
                .context("parquet writer")?,
        );
        self.eeg_schema = schema;
        self.eeg_scale = Some(scale_uv);
        self.eeg_tail = open_tail(&self.eeg_path, &labels, Some(scale_uv))
            .map_err(|e| {
                eprintln!(
                    "[parquet] EEG tail journal unavailable for {}: {e}",
                    self.eeg_path.display()
                );
            })
            .ok();
        Ok(true)
    }

    pub fn push_eeg(&mut self, electrode: usize, samples: &[f64], packet_ts: f64, sample_rate: f64) {
        if electrode >= self.n_eeg {
            return;
        }
        for (i, &v) in samples.iter().enumerate() {
            self.eeg_bufs[electrode].push_back(v);
            self.eeg_ts[electrode].push_back(packet_ts + i as f64 / sample_rate);
        }

        // Size the row groups and journal appends from the stream itself.
        if sample_rate > 0.0 && sample_rate != self.eeg_rate_seen {
            self.eeg_rate_seen = sample_rate;
            self.eeg_group_rows = ((sample_rate * EEG_ROW_GROUP_SECS).round() as usize)
                .clamp(EEG_ROW_GROUP_MIN_ROWS, EEG_ROW_GROUP_MAX_ROWS);
            self.eeg_tail_rows = ((sample_rate * EEG_TAIL_SECS).round() as usize).max(1);
        }

        // Move complete rows (all channels have data) to the column buffers.
        let ready = self
            .eeg_bufs
            .iter()
            .map(std::collections::VecDeque::len)
            .min()
            .unwrap_or(0);
        if ready == 0 {
            return;
        }
        self.eeg_pending_ts.extend(self.eeg_ts[0].drain(..ready));
        for k in 0..self.n_eeg {
            if k > 0 {
                self.eeg_ts[k].drain(..ready);
            }
            self.eeg_pending[k].extend(self.eeg_bufs[k].drain(..ready));
        }
        self.eeg_stats.rows += ready as u64;

        if self.eeg_pending_ts.len() >= self.eeg_group_rows {
            self.write_eeg_row_group();
        } else if self.eeg_pending_ts.len() - self.eeg_journaled >= self.eeg_tail_rows {
            self.journal_eeg_tail();
        }
    }

    /// Add one complete row — a sample of every channel taken at `ts` —
    /// straight to the column buffers. For sources that deliver whole frames;
    /// a missing channel is stored as NaN.
    pub fn push_eeg_frame(&mut self, channels: &[f64], ts: f64, sample_rate: f64) {
        // Samples queued per electrode must come out first and in their rows.
        if self.eeg_bufs.iter().any(|b| !b.is_empty()) {
            for (el, &v) in channels.iter().enumerate() {
                self.push_eeg(el, &[v], ts, sample_rate);
            }
            return;
        }
        if sample_rate > 0.0 && sample_rate != self.eeg_rate_seen {
            self.eeg_rate_seen = sample_rate;
            self.eeg_group_rows = ((sample_rate * EEG_ROW_GROUP_SECS).round() as usize)
                .clamp(EEG_ROW_GROUP_MIN_ROWS, EEG_ROW_GROUP_MAX_ROWS);
            self.eeg_tail_rows = ((sample_rate * EEG_TAIL_SECS).round() as usize).max(1);
        }
        self.eeg_pending_ts.push(ts);
        for (k, col) in self.eeg_pending.iter_mut().enumerate() {
            col.push(channels.get(k).copied().unwrap_or(f64::NAN));
        }
        self.eeg_stats.rows += 1;
        if self.eeg_pending_ts.len() >= self.eeg_group_rows {
            self.write_eeg_row_group();
        } else if self.eeg_pending_ts.len() - self.eeg_journaled >= self.eeg_tail_rows {
            self.journal_eeg_tail();
        }
    }

    /// Append the pending rows that are not in the tail journal yet.
    fn journal_eeg_tail(&mut self) {
        let rows = self.eeg_pending_ts.len();
        if rows <= self.eeg_journaled {
            return;
        }
        let Some(tail) = self.eeg_tail.as_mut() else {
            self.eeg_journaled = rows;
            return;
        };
        let t0 = std::time::Instant::now();
        let mut buf = Vec::with_capacity((rows - self.eeg_journaled) * (self.n_eeg + 1) * 8);
        for r in self.eeg_journaled..rows {
            buf.extend_from_slice(&self.eeg_pending_ts[r].to_le_bytes());
            for col in &self.eeg_pending {
                buf.extend_from_slice(&col[r].to_le_bytes());
            }
        }
        use std::io::Write as _;
        if let Err(e) = tail.write_all(&buf) {
            self.eeg_stats.errors += 1;
            eprintln!(
                "[parquet] EEG tail journal write failed for {}: {e}",
                self.eeg_path.display()
            );
        }
        self.eeg_journaled = rows;
        self.eeg_stats.write_secs += t0.elapsed().as_secs_f64();
    }

    /// Write the pending rows as one row group, make it durable, checkpoint
    /// the footer and start the tail journal afresh.
    fn write_eeg_row_group(&mut self) {
        let rows = self.eeg_pending_ts.len();
        if rows == 0 {
            return;
        }
        let t0 = std::time::Instant::now();
        let mut columns: Vec<Arc<dyn arrow_array::Array>> = Vec::with_capacity(self.n_eeg + 1);
        columns.push(Arc::new(Float64Array::from(std::mem::take(&mut self.eeg_pending_ts))));
        let scale = self.eeg_scale;
        for col in &mut self.eeg_pending {
            let col = std::mem::take(col);
            columns.push(match scale {
                None => Arc::new(Float64Array::from(col)),
                Some(q) => Arc::new(counts_array(&col, q)),
            });
        }
        self.eeg_journaled = 0;

        let Some(w) = self.eeg_wtr.as_mut() else {
            return;
        };
        let written = RecordBatch::try_new(self.eeg_schema.clone(), columns)
            .map_err(parquet::errors::ParquetError::from)
            .and_then(|batch| w.write(&batch))
            .and_then(|()| w.flush());
        match written {
            Ok(()) => {
                // Data first, then the footer that vouches for it, then the
                // journal those rows no longer need. The writer buffers what
                // it writes: a row group whose last pages are small is not
                // all in the file until that buffer is flushed too.
                let synced = w.sync().and_then(|()| w.inner().sync_data());
                if let Err(e) = synced.and_then(|()| write_checkpoint(&self.eeg_path, w, scale)) {
                    self.eeg_stats.errors += 1;
                    eprintln!("[parquet] EEG checkpoint failed for {}: {e}", self.eeg_path.display());
                } else {
                    self.eeg_stats.rows_durable += rows as u64;
                    if let Some(tail) = self.eeg_tail.as_mut() {
                        if let Err(e) = truncate_tail(tail) {
                            eprintln!(
                                "[parquet] EEG tail journal reset failed for {}: {e}",
                                self.eeg_path.display()
                            );
                        }
                    }
                }
                self.eeg_stats.row_groups += 1;
                self.eeg_stats.bytes = w.bytes_written() as u64;
            }
            Err(e) => {
                // The rows stay in the journal (it is only cut after a good
                // row group), so recovery can still bring them back.
                self.eeg_stats.errors += 1;
                eprintln!(
                    "[parquet] EEG row group write failed for {}: {e}",
                    self.eeg_path.display()
                );
            }
        }
        self.eeg_stats.write_secs += t0.elapsed().as_secs_f64();
    }

    /// Size and throughput of the raw EEG file so far.
    pub fn eeg_stats(&self) -> EegWriteStats {
        self.eeg_stats
    }

    // ── PPG ──────────────────────────────────────────────────────────────────

    pub fn push_ppg(
        &mut self,
        _eeg_csv_path: &Path,
        channel: usize,
        samples: &[f64],
        packet_ts: f64,
        ppg_vitals: Option<&PpgMetrics>,
    ) {
        if channel >= 3 {
            return;
        }
        for (i, &v) in samples.iter().enumerate() {
            self.ppg_bufs[channel].push_back(v);
            self.ppg_ts[channel].push_back(packet_ts + i as f64 / PPG_SAMPLE_RATE);
        }

        let ready = self
            .ppg_bufs
            .iter()
            .map(std::collections::VecDeque::len)
            .min()
            .unwrap_or(0);
        if ready == 0 {
            return;
        }

        // Lazy-open PPG writer.
        if self.ppg_wtr.is_none() {
            match std::fs::File::create(&self.ppg_path) {
                Ok(f) => match ArrowWriter::try_new(f, self.ppg_schema.clone(), Some(writer_props())) {
                    Ok(w) => {
                        self.ppg_wtr = Some(w);
                    }
                    Err(e) => {
                        eprintln!("[parquet] PPG writer error: {e}");
                        return;
                    }
                },
                Err(e) => {
                    eprintln!("[parquet] PPG create error: {e}");
                    return;
                }
            }
        }

        let ts_col: Vec<f64> = (0..ready).filter_map(|_| self.ppg_ts[0].pop_front()).collect();
        for ts in self.ppg_ts.iter_mut().take(3).skip(1) {
            for _ in 0..ready {
                ts.pop_front();
            }
        }

        let ambient: Vec<f64> = (0..ready).filter_map(|_| self.ppg_bufs[0].pop_front()).collect();
        let infrared: Vec<f64> = (0..ready).filter_map(|_| self.ppg_bufs[1].pop_front()).collect();
        let red: Vec<f64> = (0..ready).filter_map(|_| self.ppg_bufs[2].pop_front()).collect();

        let vitals_row = |field: fn(&PpgMetrics) -> f64| -> Vec<f64> {
            (0..ready).map(|_| ppg_vitals.map_or(f64::NAN, field)).collect()
        };

        let columns: Vec<Arc<dyn arrow_array::Array>> = vec![
            Arc::new(Float64Array::from(ts_col)),
            Arc::new(Float64Array::from(ambient)),
            Arc::new(Float64Array::from(infrared)),
            Arc::new(Float64Array::from(red)),
            Arc::new(Float64Array::from(vitals_row(|v| v.hr))),
            Arc::new(Float64Array::from(vitals_row(|v| v.rmssd))),
            Arc::new(Float64Array::from(vitals_row(|v| v.sdnn))),
            Arc::new(Float64Array::from(vitals_row(|v| v.pnn50))),
            Arc::new(Float64Array::from(vitals_row(|v| v.lf_hf_ratio))),
            Arc::new(Float64Array::from(vitals_row(|v| v.respiratory_rate))),
            Arc::new(Float64Array::from(vitals_row(|v| v.spo2_estimate))),
            Arc::new(Float64Array::from(vitals_row(|v| v.perfusion_index))),
            Arc::new(Float64Array::from(vitals_row(|v| v.stress_index))),
        ];

        if let Some(ref mut wtr) = self.ppg_wtr {
            if let Ok(batch) = RecordBatch::try_new(self.ppg_schema.clone(), columns) {
                let _ = wtr.write(&batch);
                self.ppg_rows += ready;
            }
            if self.ppg_rows >= PPG_FLUSH_ROWS {
                let _ = wtr.flush();
                self.ppg_rows = 0;
            }
        }
    }

    // ── Metrics ──────────────────────────────────────────────────────────────

    pub fn push_metrics(&mut self, _eeg_csv_path: &Path, snap: &BandSnapshot) {
        // Lazy-open metrics writer.
        if self.metrics_wtr.is_none() {
            match std::fs::File::create(&self.metrics_path) {
                Ok(f) => match ArrowWriter::try_new(f, self.metrics_schema.clone(), Some(metrics_writer_props())) {
                    Ok(w) => {
                        self.metrics_wtr = Some(w);
                    }
                    Err(e) => {
                        eprintln!("[parquet] metrics writer error: {e}");
                        return;
                    }
                },
                Err(e) => {
                    eprintln!("[parquet] metrics create error: {e}");
                    return;
                }
            }
        }

        // Build a flat row of f64 values matching METRICS_CSV_HEADER order.
        let opt = |v: Option<f64>| v.unwrap_or(f64::NAN);
        let opt_u64 = |v: Option<u64>| v.map_or(f64::NAN, |x| x as f64);
        let opt_u16 = |v: Option<u16>| v.map_or(f64::NAN, |x| x as f64);

        let mut row: Vec<f64> = Vec::with_capacity(95);
        row.push(snap.timestamp);

        for ch in &snap.channels {
            row.extend_from_slice(&[
                ch.delta as f64,
                ch.theta as f64,
                ch.alpha as f64,
                ch.beta as f64,
                ch.gamma as f64,
                ch.high_gamma as f64,
                ch.rel_delta as f64,
                ch.rel_theta as f64,
                ch.rel_alpha as f64,
                ch.rel_beta as f64,
                ch.rel_gamma as f64,
                ch.rel_high_gamma as f64,
            ]);
        }
        // Pad to 48 band-power columns (4 channels × 12) if fewer channels.
        while row.len() < 1 + 48 {
            row.push(f64::NAN);
        }

        row.extend_from_slice(&[
            snap.faa as f64,
            snap.tar as f64,
            snap.bar as f64,
            snap.dtr as f64,
            snap.pse as f64,
            snap.apf as f64,
            snap.bps as f64,
            snap.snr as f64,
            snap.coherence as f64,
            snap.mu_suppression as f64,
            snap.mood as f64,
            snap.tbr as f64,
            snap.sef95 as f64,
            snap.spectral_centroid as f64,
            snap.hjorth_activity as f64,
            snap.hjorth_mobility as f64,
            snap.hjorth_complexity as f64,
            snap.permutation_entropy as f64,
            snap.higuchi_fd as f64,
            snap.dfa_exponent as f64,
            snap.sample_entropy as f64,
            snap.pac_theta_gamma as f64,
            snap.laterality_index as f64,
        ]);
        row.extend_from_slice(&[
            opt(snap.hr),
            opt(snap.rmssd),
            opt(snap.sdnn),
            opt(snap.pnn50),
            opt(snap.lf_hf_ratio),
            opt(snap.respiratory_rate),
            opt(snap.spo2_estimate),
            opt(snap.perfusion_index),
            opt(snap.stress_index),
        ]);
        row.extend_from_slice(&[opt_u64(snap.blink_count), opt(snap.blink_rate)]);
        row.extend_from_slice(&[
            opt(snap.head_pitch),
            opt(snap.head_roll),
            opt(snap.stillness),
            opt_u64(snap.nod_count),
            opt_u64(snap.shake_count),
        ]);
        row.extend_from_slice(&[opt(snap.meditation), opt(snap.cognitive_load), opt(snap.drowsiness)]);
        row.push(opt_u16(snap.temperature_raw));
        row.extend_from_slice(&[opt(snap.gpu_overall), opt(snap.gpu_render), opt(snap.gpu_tiler)]);
        row.push(snap.echt as f64);

        // A row group per minute of metrics (by their own time stamps), or
        // per METRICS_ROW_GROUP_MAX_ROWS rows.
        let group_started = self.metrics_pending.first().map_or(snap.timestamp, |r| r[0]);
        self.metrics_pending.push(row);
        self.metrics_rows += 1;

        if self.metrics_rows >= METRICS_ROW_GROUP_MAX_ROWS
            || (snap.timestamp - group_started).abs() >= METRICS_ROW_GROUP_SECS
        {
            self.flush_metrics();
        }
    }

    fn flush_metrics(&mut self) {
        if self.metrics_pending.is_empty() {
            return;
        }
        let Some(ref mut wtr) = self.metrics_wtr else {
            return;
        };

        let n_cols = self.metrics_n_cols;
        let n_rows = self.metrics_pending.len();
        let mut col_data: Vec<Vec<f64>> = vec![Vec::with_capacity(n_rows); n_cols];

        for row in &self.metrics_pending {
            for (ci, col) in col_data.iter_mut().enumerate() {
                col.push(if ci < row.len() { row[ci] } else { f64::NAN });
            }
        }

        let columns: Vec<Arc<dyn arrow_array::Array>> = col_data
            .into_iter()
            .map(|c| Arc::new(Float64Array::from(c)) as Arc<dyn arrow_array::Array>)
            .collect();

        if let Ok(batch) = RecordBatch::try_new(self.metrics_schema.clone(), columns) {
            let _ = wtr.write(&batch);
        }
        let _ = wtr.flush();
        self.metrics_pending.clear();
        self.metrics_rows = 0;
    }

    // ── IMU ───────────────────────────────────────────────────────────────────

    pub fn push_imu(
        &mut self,
        _eeg_csv_path: &Path,
        timestamp_s: f64,
        accel: [f32; 3],
        gyro: Option<[f32; 3]>,
        mag: Option<[f32; 3]>,
    ) {
        // Lazy-open IMU writer.
        if self.imu_wtr.is_none() {
            match std::fs::File::create(&self.imu_path) {
                Ok(f) => match ArrowWriter::try_new(f, self.imu_schema.clone(), Some(writer_props())) {
                    Ok(w) => {
                        self.imu_wtr = Some(w);
                    }
                    Err(e) => {
                        eprintln!("[parquet] IMU writer error: {e}");
                        return;
                    }
                },
                Err(e) => {
                    eprintln!("[parquet] IMU create error: {e}");
                    return;
                }
            }
        }

        let g = gyro.unwrap_or([0.0; 3]);
        let m = mag.unwrap_or([0.0; 3]);
        self.imu_pending.push([
            timestamp_s,
            accel[0] as f64,
            accel[1] as f64,
            accel[2] as f64,
            g[0] as f64,
            g[1] as f64,
            g[2] as f64,
            m[0] as f64,
            m[1] as f64,
            m[2] as f64,
        ]);
        self.imu_rows += 1;

        if self.imu_rows >= 256 {
            self.flush_imu();
        }
    }

    fn flush_imu(&mut self) {
        if self.imu_pending.is_empty() {
            return;
        }
        let Some(ref mut wtr) = self.imu_wtr else {
            return;
        };

        let n_rows = self.imu_pending.len();
        let mut col_data: Vec<Vec<f64>> = (0..10).map(|_| Vec::with_capacity(n_rows)).collect();

        for row in &self.imu_pending {
            for (ci, col) in col_data.iter_mut().enumerate() {
                col.push(row[ci]);
            }
        }

        let columns: Vec<Arc<dyn arrow_array::Array>> = col_data
            .into_iter()
            .map(|c| Arc::new(Float64Array::from(c)) as Arc<dyn arrow_array::Array>)
            .collect();

        if let Ok(batch) = RecordBatch::try_new(self.imu_schema.clone(), columns) {
            let _ = wtr.write(&batch);
        }
        let _ = wtr.flush();
        self.imu_pending.clear();
        self.imu_rows = 0;
    }

    // ── Flush / close ────────────────────────────────────────────────────────

    /// Write fNIRS optical channels to `_fnirs.parquet`.
    /// Schema is built lazily on the first call using `channel_names`.
    pub fn push_fnirs(&mut self, _eeg_csv_path: &Path, channels: &[f64], channel_names: &[String], timestamp_s: f64) {
        // Build schema on first call.
        if self.fnirs_wtr.is_none() && self.fnirs_schema.fields().is_empty() {
            let mut fields = vec![Field::new("timestamp_s", DataType::Float64, false)];
            for name in channel_names {
                fields.push(Field::new(name.as_str(), DataType::Float64, true));
            }
            self.fnirs_schema = Arc::new(Schema::new(fields));
        }
        if self.fnirs_wtr.is_none() {
            match std::fs::File::create(&self.fnirs_path) {
                Ok(f) => match ArrowWriter::try_new(f, self.fnirs_schema.clone(), Some(writer_props())) {
                    Ok(w) => {
                        self.fnirs_wtr = Some(w);
                    }
                    Err(e) => {
                        eprintln!("[parquet] fNIRS writer error: {e}");
                        return;
                    }
                },
                Err(e) => {
                    eprintln!("[parquet] fNIRS create error: {e}");
                    return;
                }
            }
        }
        let mut row = vec![timestamp_s];
        row.extend_from_slice(channels);
        self.fnirs_pending.push(row);
        self.fnirs_rows += 1;
        if self.fnirs_rows >= 256 {
            self.flush_fnirs();
        }
    }

    /// Write a device-reported vitals sample to `_vitals.parquet`.
    ///
    /// The schema is fixed (unlike fNIRS, whose channels are device-specific),
    /// so it is built up front.  All value columns are nullable: a live heart
    /// rate carries `hr_bpm`/`ibi_ms` and nothing else, a cached SpO2 poll
    /// carries only `spo2_pct`, and writing 0 for the rest would be a lie.
    pub fn push_vitals(
        &mut self,
        _eeg_csv_path: &Path,
        timestamp_s: f64,
        hr_bpm: Option<f64>,
        ibi_ms: Option<f64>,
        spo2_pct: Option<f64>,
        temperature_c: Option<f64>,
    ) {
        if self.vitals_wtr.is_none() {
            match std::fs::File::create(&self.vitals_path) {
                Ok(f) => match ArrowWriter::try_new(f, vitals_schema(), Some(writer_props())) {
                    Ok(w) => {
                        self.vitals_wtr = Some(w);
                    }
                    Err(e) => {
                        eprintln!("[parquet] vitals writer error: {e}");
                        return;
                    }
                },
                Err(e) => {
                    eprintln!("[parquet] vitals create error: {e}");
                    return;
                }
            }
        }
        self.vitals_pending
            .push((timestamp_s, hr_bpm, ibi_ms, spo2_pct, temperature_c));
        self.vitals_rows += 1;
        // A smaller batch than the high-rate streams: vitals arrive about once
        // per heartbeat, so 256 rows would be minutes of unwritten data.
        if self.vitals_rows >= 32 {
            self.flush_vitals();
        }
    }

    fn flush_vitals(&mut self) {
        if self.vitals_pending.is_empty() {
            return;
        }
        let Some(ref mut wtr) = self.vitals_wtr else {
            return;
        };
        let n = self.vitals_pending.len();
        let mut ts = Vec::with_capacity(n);
        let mut hr = Vec::with_capacity(n);
        let mut ibi = Vec::with_capacity(n);
        let mut spo2 = Vec::with_capacity(n);
        let mut temp = Vec::with_capacity(n);
        for (t, h, i, s, c) in self.vitals_pending.drain(..) {
            ts.push(t);
            hr.push(h);
            ibi.push(i);
            spo2.push(s);
            temp.push(c);
        }
        let columns: Vec<Arc<dyn arrow_array::Array>> = vec![
            Arc::new(Float64Array::from(ts)),
            Arc::new(Float64Array::from(hr)),
            Arc::new(Float64Array::from(ibi)),
            Arc::new(Float64Array::from(spo2)),
            Arc::new(Float64Array::from(temp)),
        ];
        if let Ok(batch) = RecordBatch::try_new(vitals_schema(), columns) {
            let _ = wtr.write(&batch);
            let _ = wtr.flush();
        }
        self.vitals_rows = 0;
    }

    /// Buffer one scalar sensor sample for `_sensors.parquet` (long format).
    pub fn push_sensor(&mut self, _eeg_csv_path: &Path, channel: &str, unit: &str, timestamp_s: f64, value: f64) {
        if self.sensors_wtr.is_none() {
            match std::fs::File::create(&self.sensors_path) {
                Ok(f) => match ArrowWriter::try_new(f, sensors_schema(), Some(writer_props())) {
                    Ok(w) => self.sensors_wtr = Some(w),
                    Err(e) => {
                        eprintln!("[parquet] sensors writer error: {e}");
                        return;
                    }
                },
                Err(e) => {
                    eprintln!("[parquet] sensors create error: {e}");
                    return;
                }
            }
        }
        self.sensors_pending
            .push((timestamp_s, channel.to_string(), unit.to_string(), value));
        // Several slow streams share the file; this batch is a few seconds of
        // the busiest mix rather than the 256 rows of a single fast stream.
        if self.sensors_pending.len() >= 128 {
            self.flush_sensors();
        }
    }

    fn flush_sensors(&mut self) {
        if self.sensors_pending.is_empty() {
            return;
        }
        let Some(ref mut wtr) = self.sensors_wtr else {
            return;
        };
        let n = self.sensors_pending.len();
        let (mut ts, mut channel, mut unit, mut value) = (
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
        );
        for (t, c, u, v) in self.sensors_pending.drain(..) {
            ts.push(t);
            channel.push(c);
            unit.push(u);
            value.push(v);
        }
        let columns: Vec<Arc<dyn arrow_array::Array>> = vec![
            Arc::new(Float64Array::from(ts)),
            Arc::new(StringArray::from(channel)),
            Arc::new(StringArray::from(unit)),
            Arc::new(Float64Array::from(value)),
        ];
        if let Ok(batch) = RecordBatch::try_new(sensors_schema(), columns) {
            let _ = wtr.write(&batch);
            let _ = wtr.flush();
        }
    }

    fn flush_fnirs(&mut self) {
        if self.fnirs_pending.is_empty() {
            return;
        }
        let Some(ref mut wtr) = self.fnirs_wtr else {
            return;
        };
        let n_rows = self.fnirs_pending.len();
        let n_cols = self.fnirs_pending[0].len(); // timestamp + channels
        let mut cols: Vec<Vec<f64>> = (0..n_cols).map(|_| Vec::with_capacity(n_rows)).collect();
        for row in self.fnirs_pending.drain(..) {
            for (c, v) in row.into_iter().enumerate() {
                cols[c].push(v);
            }
        }
        let columns: Vec<Arc<dyn arrow_array::Array>> = cols
            .into_iter()
            .map(|c| Arc::new(Float64Array::from(c)) as Arc<dyn arrow_array::Array>)
            .collect();
        if let Ok(batch) = RecordBatch::try_new(self.fnirs_schema.clone(), columns) {
            let _ = wtr.write(&batch);
            let _ = wtr.flush();
        }
    }

    pub fn flush(&mut self) {
        // EEG: put what is in memory into the journal. Row groups are cut by
        // duration in `push_eeg` — forcing one here would shred the file into
        // thousands of tiny groups (the session calls this every 256 samples).
        self.journal_eeg_tail();
        if let Some(tail) = self.eeg_tail.as_mut() {
            let _ = tail.sync_data();
        }
        if let Some(ref mut w) = self.ppg_wtr {
            let _ = w.flush();
        }
        // Not the metrics: their row groups are cut by time in `push_metrics`
        // (see METRICS_ROW_GROUP_SECS), and written out at `close()`.
        self.flush_imu();
        self.flush_fnirs();
        self.flush_sensors();
    }

    /// Close all writers, finalising the Parquet files. Idempotent.
    ///
    /// A Parquet file is invalid until its footer is written by `close()`.
    /// This is also called from `Drop`, so a daemon panic or unexpected
    /// shutdown won't leave a footerless file.
    pub fn close(&mut self) {
        self.flush_metrics();
        self.flush_imu();
        self.flush_fnirs();
        self.flush_vitals();
        self.flush_sensors();
        if self.eeg_wtr.is_some() {
            self.write_eeg_row_group();
        }
        if let Some(w) = self.eeg_wtr.take() {
            match w.close() {
                Ok(_) => {
                    // The real footer is in place: the side files have done their job.
                    self.eeg_tail = None;
                    let _ = std::fs::remove_file(eeg_tail_path(&self.eeg_path));
                    let _ = std::fs::remove_file(eeg_checkpoint_path(&self.eeg_path));
                    self.eeg_stats.bytes = std::fs::metadata(&self.eeg_path).map_or(self.eeg_stats.bytes, |m| m.len());
                }
                Err(e) => {
                    // Leave them: recovery can rebuild the file from them.
                    self.eeg_stats.errors += 1;
                    eprintln!(
                        "[parquet] EEG parquet close failed for {}: {e}",
                        self.eeg_path.display()
                    );
                }
            }
        }
        if let Some(w) = self.ppg_wtr.take() {
            let _ = w.close();
        }
        if let Some(w) = self.metrics_wtr.take() {
            let _ = w.close();
        }
        if let Some(w) = self.imu_wtr.take() {
            let _ = w.close();
        }
        if let Some(w) = self.fnirs_wtr.take() {
            let _ = w.close();
        }
        if let Some(w) = self.vitals_wtr.take() {
            let _ = w.close();
        }
        if let Some(w) = self.sensors_wtr.take() {
            let _ = w.close();
        }
    }
}

impl Drop for ParquetState {
    fn drop(&mut self) {
        self.close();
    }
}

fn open_tail(pq_path: &Path, labels: &[&str], scale_uv: Option<f64>) -> std::io::Result<std::fs::File> {
    use std::io::Write as _;
    // Read access too: cutting the journal back re-reads where the header ends.
    let mut f = std::fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(eeg_tail_path(pq_path))?;
    let header = TailHeader {
        magic: TAIL_MAGIC.into(),
        channels: labels.iter().map(|l| (*l).to_string()).collect(),
        scale_uv,
    };
    let mut line = serde_json::to_vec(&header).map_err(std::io::Error::other)?;
    line.push(b'\n');
    f.write_all(&line)?;
    f.sync_data()?;
    Ok(f)
}

/// Cut the journal back to its header line.
fn truncate_tail(tail: &mut std::fs::File) -> std::io::Result<()> {
    use std::io::{BufRead as _, Seek as _, SeekFrom};
    tail.seek(SeekFrom::Start(0))?;
    let mut header = Vec::new();
    std::io::BufReader::new(&*tail).read_until(b'\n', &mut header)?;
    tail.set_len(header.len() as u64)?;
    tail.seek(SeekFrom::Start(header.len() as u64))?;
    Ok(())
}

/// Store the footer the file would have if it ended after the row groups
/// flushed so far: `[u64 LE: end of data][footer]`, written beside and renamed
/// over so a reader never sees half of one.
fn write_checkpoint(pq_path: &Path, w: &ArrowWriter<std::fs::File>, scale_uv: Option<f64>) -> std::io::Result<()> {
    use parquet::file::metadata::{FileMetaData, ParquetMetaData, ParquetMetaDataWriter};
    let groups = w.flushed_row_groups().to_vec();
    let Some(first) = groups.first() else {
        return Ok(());
    };
    let rows = groups
        .iter()
        .map(parquet::file::metadata::RowGroupMetaData::num_rows)
        .sum();
    let key_values = scale_uv.map(|q| vec![eeg_scale_metadata(q)]);
    let file_meta = FileMetaData::new(
        1,
        rows,
        Some("skill-data".into()),
        key_values,
        first.schema_descr_ptr(),
        None,
    );
    let meta = ParquetMetaData::new(file_meta, groups);
    let mut buf = (w.bytes_written() as u64).to_le_bytes().to_vec();
    ParquetMetaDataWriter::new(&mut buf, &meta)
        .finish()
        .map_err(std::io::Error::other)?;
    let path = eeg_checkpoint_path(pq_path);
    let tmp = side_path(pq_path, "ckpt.tmp");
    std::fs::write(&tmp, &buf)?;
    std::fs::rename(&tmp, &path)
}

/// What [`recover_eeg_parquet`] found and did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EegRecovery {
    /// The file has its footer; nothing to do (stray side files are removed).
    Intact,
    /// The file was rebuilt; this many rows are in it.
    Recovered { rows: u64 },
    /// No footer and nothing to rebuild from.
    Unrecoverable,
}

/// Does the file end in a readable Parquet footer?
pub fn eeg_parquet_is_complete(pq_path: &Path) -> bool {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    std::fs::File::open(pq_path)
        .ok()
        .and_then(|f| ParquetRecordBatchReaderBuilder::try_new(f).ok())
        .is_some()
}

/// Make an EEG Parquet file that was never closed (crash, kill, power cut)
/// whole again: the row groups vouched for by the checkpoint, followed by the
/// rows in the tail journal, rewritten as a normal file in place of the broken
/// one. Safe to call on any file at any time; a file with a footer is left
/// alone. Not for a file that is still being written.
pub fn recover_eeg_parquet(pq_path: &Path) -> anyhow::Result<EegRecovery> {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use std::io::{BufRead as _, Read as _, Write as _};

    let ckpt_path = eeg_checkpoint_path(pq_path);
    let tail_path = eeg_tail_path(pq_path);
    if eeg_parquet_is_complete(pq_path) {
        let _ = std::fs::remove_file(&ckpt_path);
        let _ = std::fs::remove_file(&tail_path);
        return Ok(EegRecovery::Intact);
    }

    // 1. The durable row groups: the data up to the checkpoint plus its footer
    //    is a complete Parquet file.
    let mut labels: Option<Vec<String>> = None;
    let mut scale: Option<f64> = None;
    let mut batches: Vec<RecordBatch> = Vec::new();
    let head_path = side_path(pq_path, "head.tmp");
    if let Ok(ckpt) = std::fs::read(&ckpt_path) {
        let data_end = ckpt
            .get(..8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or_default()));
        let have = std::fs::metadata(pq_path).map_or(0, |m| m.len());
        if let Some(data_end) = data_end.filter(|&e| e >= 4 && e <= have) {
            let mut head = std::fs::File::create(&head_path).context("recovery scratch file")?;
            std::io::copy(&mut std::fs::File::open(pq_path)?.take(data_end), &mut head)?;
            head.write_all(&ckpt[8..])?;
            drop(head);
            // A checkpoint that does not read back is skipped, not fatal: the
            // journal may still hold rows.
            if let Ok(builder) = std::fs::File::open(&head_path)
                .map_err(anyhow::Error::from)
                .and_then(|f| ParquetRecordBatchReaderBuilder::try_new(f).map_err(anyhow::Error::from))
            {
                labels = Some(
                    builder
                        .schema()
                        .fields()
                        .iter()
                        .skip(1)
                        .map(|f| f.name().clone())
                        .collect(),
                );
                scale = eeg_scale_of(builder.schema());
                if let Ok(reader) = builder.build() {
                    // Stop at the first unreadable batch; keep what came before.
                    batches.extend(reader.map_while(Result::ok));
                }
            }
        }
    }

    // 2. The rows that never reached a row group.
    let mut tail_rows: Vec<f64> = Vec::new();
    if let Ok(f) = std::fs::File::open(&tail_path) {
        let mut rd = std::io::BufReader::new(f);
        let mut line = Vec::new();
        rd.read_until(b'\n', &mut line)?;
        if let Ok(h) = serde_json::from_slice::<TailHeader>(&line) {
            if h.magic == TAIL_MAGIC && labels.as_ref().is_none_or(|l| *l == h.channels) {
                let width = (h.channels.len() + 1) * 8;
                let mut raw = Vec::new();
                rd.read_to_end(&mut raw)?;
                // A row cut short by the crash is dropped.
                raw.truncate(raw.len() / width * width);
                tail_rows = raw.as_chunks::<8>().0.iter().map(|b| f64::from_le_bytes(*b)).collect();
                if labels.is_none() {
                    scale = h.scale_uv;
                }
                labels.get_or_insert(h.channels);
            }
        }
    }

    let Some(labels) = labels else {
        let _ = std::fs::remove_file(&head_path);
        return Ok(EegRecovery::Unrecoverable);
    };
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let schema = eeg_schema_scaled(&label_refs, scale);
    let width = labels.len() + 1;
    if !tail_rows.is_empty() {
        let columns: Vec<Arc<dyn arrow_array::Array>> = (0..width)
            .map(|c| {
                let col: Vec<f64> = tail_rows.iter().skip(c).step_by(width).copied().collect();
                match scale.filter(|_| c > 0) {
                    None => Arc::new(Float64Array::from(col)) as Arc<dyn arrow_array::Array>,
                    Some(q) => Arc::new(counts_array(&col, q)),
                }
            })
            .collect();
        batches.push(RecordBatch::try_new(schema.clone(), columns).context("tail rows")?);
    }
    let rows: u64 = batches.iter().map(|b| b.num_rows() as u64).sum();
    if rows == 0 {
        let _ = std::fs::remove_file(&head_path);
        return Ok(EegRecovery::Unrecoverable);
    }

    // 3. Write the whole recording as a normal file, then swap it in. The
    //    broken file and its side files stay until the new one is on disk.
    let out_path = side_path(pq_path, "recovered.tmp");
    let out = std::fs::File::create(&out_path).context("recovery output")?;
    let mut w = ArrowWriter::try_new(out, schema.clone(), Some(eeg_props_for(scale))).context("recovery writer")?;
    for batch in &batches {
        // Columns are matched by position: the schema is the same, but a
        // batch read back carries the file's own field metadata.
        let batch = RecordBatch::try_new(schema.clone(), batch.columns().to_vec()).context("recovered rows")?;
        w.write(&batch).context("recovery write")?;
        w.flush().context("recovery flush")?;
    }
    w.inner().sync_all().ok();
    w.close().context("recovery close")?;
    std::fs::rename(&out_path, pq_path).context("recovery swap")?;
    let _ = std::fs::remove_file(&head_path);
    let _ = std::fs::remove_file(&ckpt_path);
    let _ = std::fs::remove_file(&tail_path);
    Ok(EegRecovery::Recovered { rows })
}

/// Recover every unfinished EEG Parquet recording in a day directory.
/// Returns `(path, outcome)` for the files that were not intact.
///
/// A recording still being written looks just like a crashed one, except that
/// its journal keeps moving: files touched within `min_idle` are left alone
/// (a live writer appends every [`EEG_TAIL_SECS`]).
pub fn recover_eeg_parquets_in(day_dir: &Path, min_idle: std::time::Duration) -> Vec<(PathBuf, EegRecovery)> {
    let idle = |p: &Path| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_none_or(|age| age >= min_idle)
    };
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(day_dir) else {
        return out;
    };
    for path in entries.flatten().map(|e| e.path()) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        // Only raw EEG files have side files, not their `_ppg` / `_metrics` / …
        // sidecars.
        let is_eeg = name.starts_with("exg_") && name.ends_with(".parquet") && !is_sidecar_file(name);
        if !is_eeg {
            continue;
        }
        let (ckpt, tail) = (eeg_checkpoint_path(&path), eeg_tail_path(&path));
        if !ckpt.exists() && !tail.exists() {
            continue;
        }
        if !(idle(&path) && idle(&ckpt) && idle(&tail)) {
            continue;
        }
        match recover_eeg_parquet(&path) {
            Ok(EegRecovery::Intact) => {}
            Ok(outcome) => out.push((path, outcome)),
            Err(e) => {
                eprintln!("[parquet] EEG parquet recovery failed for {}: {e:#}", path.display());
                out.push((path, EegRecovery::Unrecoverable));
            }
        }
    }
    out
}

/// Read just the channel names from an EEG parquet schema (cheap — does not
/// scan row data). Returns the column names after the leading `timestamp_s`.
pub fn read_eeg_parquet_channels(path: &Path) -> anyhow::Result<Vec<String>> {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).context("parquet builder")?;
    let schema = builder.schema();
    if schema.fields().len() < 2 {
        anyhow::bail!("parquet schema has <2 columns");
    }
    Ok(schema.fields().iter().skip(1).map(|f| f.name().clone()).collect())
}

/// Read a recorded EEG parquet file written by `ParquetState`.
///
/// Returns `(first_timestamp_secs, channel_names, samples_per_channel)`.
/// Channel order matches `channel_names`. Rows with a null timestamp are
/// skipped. Nulls in channel columns become `0.0` (matching how CSV blanks
/// would be handled by `extract_epoch_samples`'s row-skip path).
///
/// Used by batch/idle reembed so users on `storage_format = "parquet"` get
/// coverage equivalent to CSV — without this, parquet-only days would be
/// silently skipped (no CSV → `find_eeg_csvs` empty → epochs stay un-embedded
/// forever, looking from the outside like reembed was broken).
pub fn read_eeg_parquet(path: &Path) -> anyhow::Result<(f64, Vec<String>, Vec<Vec<f32>>)> {
    use arrow_array::Array;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).context("parquet builder")?;
    let schema = builder.schema().clone();

    if schema.fields().len() < 2 {
        anyhow::bail!("parquet schema has <2 columns (need timestamp + ≥1 channel)");
    }
    // Column 0 is timestamp_s; the rest are channel columns.
    let channel_names: Vec<String> = schema.fields().iter().skip(1).map(|f| f.name().clone()).collect();
    let n_ch = channel_names.len();

    let scale = eeg_scale_of(&schema);
    let reader = builder.build().context("parquet reader")?;
    let mut first_ts: Option<f64> = None;
    let mut channels: Vec<Vec<f32>> = vec![Vec::new(); n_ch];

    for batch in reader {
        let batch = batch.context("parquet batch")?;
        let ts_col = batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow_array::Float64Array>()
            .ok_or_else(|| anyhow::anyhow!("timestamp_s column is not Float64"))?;
        let mut ch_cols: Vec<EegColumn> = Vec::with_capacity(n_ch);
        for k in 0..n_ch {
            let col = EegColumn::new(batch.column(k + 1).as_ref(), scale)
                .ok_or_else(|| anyhow::anyhow!("channel column {} is neither Float64 nor Int64", k + 1))?;
            ch_cols.push(col);
        }
        for i in 0..ts_col.len() {
            if ts_col.is_null(i) {
                continue;
            }
            if first_ts.is_none() {
                first_ts = Some(ts_col.value(i));
            }
            for (k, col) in ch_cols.iter().enumerate() {
                channels[k].push(col.get(i).unwrap_or(0.0) as f32);
            }
        }
    }

    let first = first_ts.ok_or_else(|| anyhow::anyhow!("parquet file has no rows"))?;
    Ok((first, channel_names, channels))
}

/// The microvolts per count of an EEG parquet file that stores its channels
/// as integers ([`EEG_SCALE_KEY`]); `None` for one that stores `f64`.
pub fn read_eeg_parquet_scale(path: &Path) -> anyhow::Result<Option<f64>> {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).context("parquet builder")?;
    Ok(eeg_scale_of(builder.schema()))
}

/// One channel of an EEG parquet file at full precision, in microvolts
/// (a null is NaN). [`read_eeg_parquet`] returns `f32`.
pub fn read_eeg_parquet_channel(path: &Path, channel: usize) -> anyhow::Result<Vec<f64>> {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use parquet::arrow::ProjectionMask;

    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).context("parquet builder")?;
    let scale = eeg_scale_of(builder.schema());
    let mask = ProjectionMask::leaves(builder.parquet_schema(), [channel + 1]);
    let reader = builder.with_projection(mask).build().context("parquet reader")?;
    let mut out = Vec::new();
    for batch in reader {
        let batch = batch.context("parquet batch")?;
        let col = EegColumn::new(batch.column(0).as_ref(), scale)
            .ok_or_else(|| anyhow::anyhow!("channel column {} is neither Float64 nor Int64", channel + 1))?;
        out.extend((0..batch.num_rows()).map(|i| col.get(i).unwrap_or(f64::NAN)));
    }
    Ok(out)
}

/// The `timestamp_s` column of an EEG parquet file, one value per row (rows
/// with a null timestamp are skipped, as in [`read_eeg_parquet`]).
pub fn read_eeg_parquet_timestamps(path: &Path) -> anyhow::Result<Vec<f64>> {
    use arrow_array::Array;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use parquet::arrow::ProjectionMask;

    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).context("parquet builder")?;
    let mask = ProjectionMask::leaves(builder.parquet_schema(), [0]);
    let reader = builder.with_projection(mask).build().context("parquet reader")?;
    let mut out = Vec::new();
    for batch in reader {
        let batch = batch.context("parquet batch")?;
        let ts = batch
            .column(0)
            .as_any()
            .downcast_ref::<Float64Array>()
            .ok_or_else(|| anyhow::anyhow!("timestamp_s column is not Float64"))?;
        out.extend((0..ts.len()).filter(|&i| !ts.is_null(i)).map(|i| ts.value(i)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn eeg_parquet_path_replaces_extension() {
        let p = eeg_parquet_path(Path::new("/data/exg_1700000000.csv"));
        assert_eq!(p, std::path::PathBuf::from("/data/exg_1700000000.parquet"));
    }

    #[test]
    fn ppg_parquet_path_derives_correctly() {
        let p = ppg_parquet_path(Path::new("/data/exg_1700000000.csv"));
        assert_eq!(p, std::path::PathBuf::from("/data/exg_1700000000_ppg.parquet"));
    }

    #[test]
    fn metrics_parquet_path_derives_correctly() {
        let p = metrics_parquet_path(Path::new("/data/exg_1700000000.csv"));
        assert_eq!(p, std::path::PathBuf::from("/data/exg_1700000000_metrics.parquet"));
    }

    #[test]
    fn imu_parquet_path_derives_correctly() {
        let p = imu_parquet_path(Path::new("/data/exg_1700000000.csv"));
        assert_eq!(p, std::path::PathBuf::from("/data/exg_1700000000_imu.parquet"));
    }

    #[test]
    fn fnirs_parquet_path_derives_correctly() {
        let p = fnirs_parquet_path(Path::new("/data/exg_1700000000.csv"));
        assert_eq!(p, std::path::PathBuf::from("/data/exg_1700000000_fnirs.parquet"));
    }

    #[test]
    fn parquet_open_write_flush() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_test.csv");
        let labels = ["TP9", "AF7", "AF8", "TP10"];

        let mut pq = ParquetState::open_with_labels(&csv_path, &labels).unwrap();

        // Push 4 channels of EEG data
        let samples = [1.0, 2.0, 3.0, 4.0];
        pq.push_eeg(0, &samples, 1000.0, 256.0);
        pq.push_eeg(1, &samples, 1000.0, 256.0);
        pq.push_eeg(2, &samples, 1000.0, 256.0);
        pq.push_eeg(3, &samples, 1000.0, 256.0);
        pq.flush();

        let pq_path = eeg_parquet_path(&csv_path);
        assert!(pq_path.exists(), "Parquet EEG file should exist");
    }

    #[test]
    fn vitals_parquet_path_derives_correctly() {
        let p = crate::session_paths::vitals_parquet_path(Path::new("/data/exg_1700000000.csv"));
        assert_eq!(p, std::path::PathBuf::from("/data/exg_1700000000_vitals.parquet"));
    }

    /// Sparse rows must survive a round trip: absent values stay null rather
    /// than becoming zeros, and `close` has to write the footer or the file is
    /// unreadable.
    #[test]
    fn parquet_vitals_round_trips_nulls() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_test.csv");
        let mut pq = ParquetState::open_with_labels(&csv_path, &["TP9"]).unwrap();

        pq.push_vitals(&csv_path, 1000.0, Some(52.0), Some(1153.0), None, None);
        pq.push_vitals(&csv_path, 1001.0, None, None, Some(97.0), Some(33.4));
        pq.close();

        let path = crate::session_paths::vitals_parquet_path(&csv_path);
        assert!(path.exists(), "vitals parquet should exist");

        // `is_null`/`is_valid` live on the Array trait.
        use arrow_array::Array as _;

        let file = std::fs::File::open(&path).unwrap();
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(file)
            .unwrap()
            .build()
            .unwrap();
        let batches: Vec<_> = reader.map(std::result::Result::unwrap).collect();
        let rows: usize = batches.iter().map(arrow_array::RecordBatch::num_rows).sum();
        assert_eq!(rows, 2);

        let batch = &batches[0];
        let hr = batch
            .column_by_name("hr_bpm")
            .unwrap()
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap();
        assert!(hr.is_valid(0), "first row has a heart rate");
        assert!(hr.is_null(1), "second row must stay null, not 0");

        let spo2 = batch
            .column_by_name("spo2_pct")
            .unwrap()
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap();
        assert!(spo2.is_null(0));
        assert_eq!(spo2.value(1), 97.0);
    }

    #[test]
    fn sensors_parquet_path_derives_correctly() {
        let p = crate::session_paths::sensors_parquet_path(Path::new("/data/exg_1700000000.csv"));
        assert_eq!(p, std::path::PathBuf::from("/data/exg_1700000000_sensors.parquet"));
    }

    /// Long format round trip: every sample keeps its timestamp, channel, unit
    /// and exact value, across more rows than one flush batch.
    #[test]
    fn parquet_sensors_round_trip_across_batches() {
        use arrow_array::Array as _;
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_test.csv");
        let mut pq = ParquetState::open_with_labels(&csv_path, &["TP9"]).unwrap();

        // 300 rows crosses the 128-row flush twice and leaves a tail for `close`.
        for i in 0..300 {
            let (channel, unit) = if i % 2 == 0 { ("eda", "µS") } else { ("ppg_red", "") };
            pq.push_sensor(&csv_path, channel, unit, 1000.0 + f64::from(i), f64::from(i) * 0.25);
        }
        pq.close();

        let file = std::fs::File::open(crate::session_paths::sensors_parquet_path(&csv_path)).unwrap();
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(file)
            .unwrap()
            .build()
            .unwrap();
        let batches: Vec<_> = reader.map(std::result::Result::unwrap).collect();
        let rows: usize = batches.iter().map(arrow_array::RecordBatch::num_rows).sum();
        assert_eq!(rows, 300);

        let col = |b: &arrow_array::RecordBatch, name: &str| b.column_by_name(name).unwrap().clone();
        let b = &batches[0];
        let ts = col(b, "timestamp_s");
        let ts = ts.as_any().downcast_ref::<Float64Array>().unwrap();
        let ch = col(b, "channel");
        let ch = ch.as_any().downcast_ref::<StringArray>().unwrap();
        let unit = col(b, "unit");
        let unit = unit.as_any().downcast_ref::<StringArray>().unwrap();
        let val = col(b, "value");
        let val = val.as_any().downcast_ref::<Float64Array>().unwrap();
        assert_eq!(
            (ts.value(0), ch.value(0), unit.value(0), val.value(0)),
            (1000.0, "eda", "µS", 0.0)
        );
        assert_eq!(
            (ts.value(1), ch.value(1), unit.value(1), val.value(1)),
            (1001.0, "ppg_red", "", 0.25)
        );
        assert!(ch.is_valid(0), "no nulls in long format");
    }

    #[test]
    fn parquet_sensors_file_is_not_created_until_a_sample_arrives() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_test.csv");
        let mut pq = ParquetState::open_with_labels(&csv_path, &["TP9"]).unwrap();
        pq.close();
        assert!(!crate::session_paths::sensors_parquet_path(&csv_path).exists());
    }

    #[test]
    fn parquet_ppg_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_test.csv");
        let labels = ["TP9", "AF7", "AF8", "TP10"];

        let mut pq = ParquetState::open_with_labels(&csv_path, &labels).unwrap();

        let samples = [500.0, 501.0];
        pq.push_ppg(&csv_path, 0, &samples, 1000.0, None);
        pq.push_ppg(&csv_path, 1, &samples, 1000.0, None);
        pq.push_ppg(&csv_path, 2, &samples, 1000.0, None);
        pq.flush();

        let ppg_path = ppg_parquet_path(&csv_path);
        assert!(ppg_path.exists(), "Parquet PPG file should exist");
    }

    #[test]
    fn parquet_readable_after_drop_without_explicit_close() {
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_drop.csv");
        let labels = ["TP9", "AF7", "AF8", "TP10"];

        let pq_path = eeg_parquet_path(&csv_path);
        let ppg_path = ppg_parquet_path(&csv_path);

        {
            let mut pq = ParquetState::open_with_labels(&csv_path, &labels).unwrap();
            for i in 0..10 {
                let s = [i as f64; 4];
                pq.push_eeg(0, &s, 1000.0 + i as f64, 256.0);
                pq.push_eeg(1, &s, 1000.0 + i as f64, 256.0);
                pq.push_eeg(2, &s, 1000.0 + i as f64, 256.0);
                pq.push_eeg(3, &s, 1000.0 + i as f64, 256.0);
            }
            // Force PPG writer creation so Drop must close it too.
            let ppg = [500.0, 501.0];
            pq.push_ppg(&csv_path, 0, &ppg, 1000.0, None);
            pq.push_ppg(&csv_path, 1, &ppg, 1000.0, None);
            pq.push_ppg(&csv_path, 2, &ppg, 1000.0, None);
            // Intentionally drop without calling close() — Drop must finalise.
        }

        let f = std::fs::File::open(&pq_path).expect("parquet exists");
        let builder = ParquetRecordBatchReaderBuilder::try_new(f).expect("eeg footer readable");
        let reader = builder.build().unwrap();
        let total_rows: usize = reader.flatten().map(|b| b.num_rows()).sum();
        assert!(total_rows > 0, "should have read EEG rows back");

        let f = std::fs::File::open(&ppg_path).expect("ppg parquet exists");
        let builder = ParquetRecordBatchReaderBuilder::try_new(f).expect("ppg footer readable");
        let _ = builder.build().unwrap();
    }

    #[test]
    fn parquet_close_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_close.csv");
        let labels = ["TP9", "AF7", "AF8", "TP10"];

        let mut pq = ParquetState::open_with_labels(&csv_path, &labels).unwrap();
        let s = [1.0; 4];
        pq.push_eeg(0, &s, 1000.0, 256.0);
        pq.push_eeg(1, &s, 1000.0, 256.0);
        pq.push_eeg(2, &s, 1000.0, 256.0);
        pq.push_eeg(3, &s, 1000.0, 256.0);

        pq.close();
        pq.close();
        // Drop runs another close — must not panic.
    }

    #[test]
    fn parquet_imu_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_test.csv");
        let labels = ["TP9"];

        let mut pq = ParquetState::open_with_labels(&csv_path, &labels).unwrap();
        pq.push_imu(
            &csv_path,
            1000.0,
            [0.1, 0.2, 0.3],
            Some([0.4, 0.5, 0.6]),
            Some([0.7, 0.8, 0.9]),
        );
        pq.flush();

        let imu_path = imu_parquet_path(&csv_path);
        assert!(imu_path.exists(), "Parquet IMU file should exist");
    }

    // ── Raw EEG: size, row groups, crash recovery ────────────────────────────

    /// A 500 Hz 8-channel signal quantised like an amplifier's samples.
    fn eeg_value(row: usize, ch: usize) -> f64 {
        let t = row as f64 / 500.0;
        let v = 40.0 * (std::f64::consts::TAU * (8.0 + ch as f64) * t).sin()
            + 5.0 * ((row * 31 + ch * 17) % 97) as f64 / 97.0;
        ((v * 1000.0).round() / 1000.0) as f32 as f64
    }

    const EEG_LABELS: [&str; 8] = ["Fp1", "Fp2", "C3", "Cz", "C4", "P3", "Pz", "P4"];

    /// Push `rows` samples the way the session does: one electrode at a time,
    /// with a `flush()` every 256 samples.
    fn push_rows(pq: &mut ParquetState, from: usize, rows: usize) {
        for r in from..from + rows {
            for ch in 0..EEG_LABELS.len() {
                pq.push_eeg(ch, &[eeg_value(r, ch)], 1_700_000_000.0 + r as f64 / 500.0, 500.0);
            }
            if r % 256 == 255 {
                pq.flush();
            }
        }
    }

    fn assert_rows(pq_path: &Path, rows: usize) {
        use arrow_array::Array;
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
        let builder = ParquetRecordBatchReaderBuilder::try_new(std::fs::File::open(pq_path).unwrap()).expect("footer");
        let names: Vec<String> = builder.schema().fields().iter().map(|f| f.name().clone()).collect();
        assert_eq!(names[0], "timestamp_s");
        assert_eq!(&names[1..], &EEG_LABELS);
        let mut r = 0usize;
        for batch in builder.build().unwrap() {
            let batch = batch.unwrap();
            let col = |k: usize| batch.column(k).as_any().downcast_ref::<Float64Array>().unwrap().clone();
            let ts = col(0);
            for i in 0..ts.len() {
                assert_eq!(ts.value(i), 1_700_000_000.0 + r as f64 / 500.0, "row {r} timestamp");
                for ch in 0..EEG_LABELS.len() {
                    assert_eq!(col(ch + 1).value(i), eeg_value(r, ch), "row {r} channel {ch}");
                }
                r += 1;
            }
        }
        assert_eq!(r, rows, "rows in {}", pq_path.display());
    }

    #[test]
    fn eeg_row_groups_follow_the_sample_rate_not_the_flush_calls() {
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_1700000000.csv");
        let mut pq = ParquetState::open_with_labels(&csv_path, &EEG_LABELS).unwrap();
        let rows = 500 * 25; // 25 s
        push_rows(&mut pq, 0, rows);
        let stats = pq.eeg_stats();
        assert_eq!(
            (stats.rows, stats.rows_durable, stats.row_groups, stats.errors),
            (rows as u64, 10_000, 2, 0)
        );
        pq.close();
        let pq_path = eeg_parquet_path(&csv_path);
        assert_rows(&pq_path, rows);
        let meta = ParquetRecordBatchReaderBuilder::try_new(std::fs::File::open(&pq_path).unwrap()).unwrap();
        let groups: Vec<i64> = meta.metadata().row_groups().iter().map(|g| g.num_rows()).collect();
        // 10 s of signal each, and the rest at close — not one per flush()
        assert_eq!(groups, [5000, 5000, 2500]);
        // closed cleanly: no side files left
        assert!(!eeg_tail_path(&pq_path).exists() && !eeg_checkpoint_path(&pq_path).exists());
        assert_eq!(recover_eeg_parquet(&pq_path).unwrap(), EegRecovery::Intact);
        assert_eq!(pq.eeg_stats().bytes, std::fs::metadata(&pq_path).unwrap().len());
    }

    #[test]
    fn eeg_file_is_much_smaller_than_csv_and_than_plain_snappy() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_1700000000.csv");
        let rows = 500 * 60;
        let mut pq = ParquetState::open_with_labels(&csv_path, &EEG_LABELS).unwrap();
        let mut csv = crate::session_csv::CsvState::open_with_labels(&csv_path, &EEG_LABELS).unwrap();
        // the previous layout: Snappy, dictionary on, a row group per flush
        let old_path = dir.path().join("old.parquet");
        let mut old = ArrowWriter::try_new(
            std::fs::File::create(&old_path).unwrap(),
            eeg_schema_for(&EEG_LABELS),
            Some(writer_props()),
        )
        .unwrap();
        for r in 0..rows {
            let ts = 1_700_000_000.0 + r as f64 / 500.0;
            let mut cols: Vec<Arc<dyn arrow_array::Array>> = vec![Arc::new(Float64Array::from(vec![ts]))];
            for ch in 0..EEG_LABELS.len() {
                pq.push_eeg(ch, &[eeg_value(r, ch)], ts, 500.0);
                csv.push_eeg(ch, &[eeg_value(r, ch)], ts, 500.0);
                cols.push(Arc::new(Float64Array::from(vec![eeg_value(r, ch)])));
            }
            old.write(&RecordBatch::try_new(eeg_schema_for(&EEG_LABELS), cols).unwrap())
                .unwrap();
            if r % 256 == 255 {
                pq.flush();
                old.flush().unwrap();
            }
        }
        pq.close();
        csv.flush();
        old.close().unwrap();
        let size = |p: &Path| std::fs::metadata(p).unwrap().len() as f64;
        let (new, old, csv) = (size(&eeg_parquet_path(&csv_path)), size(&old_path), size(&csv_path));
        eprintln!(
            "60 s of 8 ch at 500 Hz: csv {:.0} kB, parquet before {:.0} kB, parquet now {:.0} kB",
            csv / 1e3,
            old / 1e3,
            new / 1e3
        );
        assert!(new < csv / 2.0, "parquet {new} vs csv {csv}");
        assert!(new < old * 0.8, "parquet now {new} vs before {old}");
    }

    /// The process dies without `close()`: no footer. Recovery brings back the
    /// row groups and the rows that were only in the journal.
    #[test]
    fn eeg_recording_survives_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_1700000000.csv");
        let pq_path = eeg_parquet_path(&csv_path);
        let mut pq = ParquetState::open_with_labels(&csv_path, &EEG_LABELS).unwrap();
        let rows = 500 * 23 + 300; // two row groups and 3.6 s more
        push_rows(&mut pq, 0, rows);
        // what `kill -9` leaves: no destructor, no footer
        std::mem::forget(pq);
        assert!(!eeg_parquet_is_complete(&pq_path));

        let outcome = recover_eeg_parquet(&pq_path).unwrap();
        // everything up to the last journal append (every 0.25 s = 125 rows)
        let EegRecovery::Recovered { rows: got } = outcome else {
            panic!("{outcome:?}")
        };
        assert!(
            got as usize >= rows - 125 && got as usize <= rows,
            "{got} of {rows} rows"
        );
        assert_rows(&pq_path, got as usize);
        assert!(!eeg_tail_path(&pq_path).exists() && !eeg_checkpoint_path(&pq_path).exists());
        // and the standard reader takes it
        let (first, names, data) = read_eeg_parquet(&pq_path).unwrap();
        assert_eq!((first, names.len(), data[0].len()), (1_700_000_000.0, 8, got as usize));
        let stamps = read_eeg_parquet_timestamps(&pq_path).unwrap();
        assert_eq!((stamps.len(), stamps[1]), (got as usize, 1_700_000_000.002));
        assert_eq!(recover_eeg_parquet(&pq_path).unwrap(), EegRecovery::Intact);
    }

    /// A sample of an integer source, in nanovolts: a signal of some µV on
    /// an electrode offset of hundreds of mV — more digits than an `f32` has.
    fn eeg_count(row: usize, ch: usize) -> i64 {
        let t = row as f64 / 500.0;
        let signal = 40_000.0 * (std::f64::consts::TAU * (8.0 + ch as f64) * t).sin();
        let noise = ((row * 7919 + ch * 104_729) % 2001) as i64 - 1000;
        (ch as i64 - 4) * 75_000_000 + signal as i64 + noise
    }

    const NV: f64 = 0.001;

    fn push_count_rows(pq: &mut ParquetState, from: usize, rows: usize) {
        for r in from..from + rows {
            let frame: Vec<f64> = (0..EEG_LABELS.len()).map(|ch| eeg_count(r, ch) as f64 * NV).collect();
            pq.push_eeg_frame(&frame, 1_700_000_000.0 + r as f64 / 500.0, 500.0);
        }
    }

    fn assert_count_rows(pq_path: &Path, rows: usize) {
        assert_eq!(read_eeg_parquet_scale(pq_path).unwrap(), Some(NV));
        for ch in [0, 3, 7] {
            let got = read_eeg_parquet_channel(pq_path, ch).unwrap();
            assert_eq!(got.len(), rows);
            for (r, v) in got.iter().enumerate() {
                assert_eq!(*v, eeg_count(r, ch) as f64 * NV, "row {r} channel {ch}");
            }
        }
    }

    /// An integer source is stored as integers: exact, and much smaller than
    /// the same samples as `f64`.
    #[test]
    fn eeg_counts_are_stored_as_integers_exactly_and_smaller() {
        let dir = tempfile::tempdir().unwrap();
        let rows = 500 * 60;
        let size = |name: &str, counts: bool| {
            let csv_path = dir.path().join(name);
            let mut pq = ParquetState::open_with_labels(&csv_path, &EEG_LABELS).unwrap();
            if counts {
                assert!(pq.set_eeg_scale_uv(NV).unwrap());
            }
            push_count_rows(&mut pq, 0, rows);
            // too late now: the file keeps the form it started with
            assert!(!pq.set_eeg_scale_uv(0.5).unwrap());
            pq.close();
            assert_eq!(pq.eeg_stats().errors, 0);
            let pq_path = eeg_parquet_path(&csv_path);
            (std::fs::metadata(&pq_path).unwrap().len(), pq_path)
        };
        let (float_bytes, float_path) = size("exg_1.csv", false);
        let (count_bytes, count_path) = size("exg_2.csv", true);

        assert_count_rows(&count_path, rows);
        assert_eq!(read_eeg_parquet_scale(&float_path).unwrap(), None);
        // the file says what it holds
        let builder = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
            std::fs::File::open(&count_path).unwrap(),
        )
        .unwrap();
        assert_eq!(builder.schema().field(0).data_type(), &DataType::Float64);
        assert!(builder
            .schema()
            .fields()
            .iter()
            .skip(1)
            .all(|f| f.data_type() == &DataType::Int64));
        // the common reader gives microvolts for both
        let (_, _, floats) = read_eeg_parquet(&float_path).unwrap();
        let (first, names, counts) = read_eeg_parquet(&count_path).unwrap();
        assert_eq!((first, names.len()), (1_700_000_000.0, 8));
        assert_eq!(floats, counts);
        assert!(
            count_bytes * 2 < float_bytes,
            "integers {count_bytes} B, f64 {float_bytes} B"
        );
        eprintln!("60 s of 8 ch at 500 Hz: f64 {float_bytes} B, integer counts {count_bytes} B");
    }

    /// A missing sample is a null, not a number.
    #[test]
    fn eeg_counts_keep_missing_samples_missing() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_1.csv");
        let mut pq = ParquetState::open_with_labels(&csv_path, &["A", "B"]).unwrap();
        assert!(pq.set_eeg_scale_uv(NV).unwrap());
        pq.push_eeg_frame(&[1.5, f64::NAN], 1.0, 250.0);
        pq.push_eeg_frame(&[-2.25], 1.004, 250.0);
        pq.close();
        let path = eeg_parquet_path(&csv_path);
        assert_eq!(read_eeg_parquet_channel(&path, 0).unwrap(), [1.5, -2.25]);
        assert!(read_eeg_parquet_channel(&path, 1).unwrap().iter().all(|v| v.is_nan()));
    }

    /// An integer recording comes back from a crash as an integer recording.
    #[test]
    fn eeg_counts_recording_survives_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_1700000000.csv");
        let pq_path = eeg_parquet_path(&csv_path);
        let mut pq = ParquetState::open_with_labels(&csv_path, &EEG_LABELS).unwrap();
        assert!(pq.set_eeg_scale_uv(NV).unwrap());
        let rows = 500 * 23 + 300; // two row groups and 3.6 s more
        push_count_rows(&mut pq, 0, rows);
        std::mem::forget(pq);
        assert!(!eeg_parquet_is_complete(&pq_path));

        let EegRecovery::Recovered { rows: got } = recover_eeg_parquet(&pq_path).unwrap() else {
            panic!("not recovered")
        };
        assert!(
            got as usize >= rows - 125 && got as usize <= rows,
            "{got} of {rows} rows"
        );
        assert_count_rows(&pq_path, got as usize);

        // and from the journal alone, before any row group exists
        let csv_path = dir.path().join("exg_1700000100.csv");
        let pq_path = eeg_parquet_path(&csv_path);
        let mut pq = ParquetState::open_with_labels(&csv_path, &EEG_LABELS).unwrap();
        assert!(pq.set_eeg_scale_uv(NV).unwrap());
        push_count_rows(&mut pq, 0, 1000);
        std::mem::forget(pq);
        let EegRecovery::Recovered { rows: got } = recover_eeg_parquet(&pq_path).unwrap() else {
            panic!("not recovered")
        };
        assert_eq!(got, 1000);
        assert_count_rows(&pq_path, 1000);
    }

    /// Each side file is enough on its own, and damage to one does not cost
    /// what the other holds.
    #[test]
    fn eeg_recovery_uses_whatever_is_left() {
        let dir = tempfile::tempdir().unwrap();
        let crash = |name: &str, rows: usize| -> PathBuf {
            let csv_path = dir.path().join(name);
            let mut pq = ParquetState::open_with_labels(&csv_path, &EEG_LABELS).unwrap();
            push_rows(&mut pq, 0, rows);
            pq.flush();
            std::mem::forget(pq);
            eeg_parquet_path(&csv_path)
        };

        // died within the first row group: only the journal exists
        let p = crash("exg_1700000001.csv", 1000);
        assert!(!eeg_checkpoint_path(&p).exists());
        assert_eq!(recover_eeg_parquet(&p).unwrap(), EegRecovery::Recovered { rows: 1000 });
        assert_rows(&p, 1000);

        // the journal is gone (or torn mid-row): the row groups still come back
        let p = crash("exg_1700000002.csv", 12_000);
        std::fs::remove_file(eeg_tail_path(&p)).unwrap();
        assert_eq!(
            recover_eeg_parquet(&p).unwrap(),
            EegRecovery::Recovered { rows: 10_000 }
        );
        assert_rows(&p, 10_000);

        let p = crash("exg_1700000003.csv", 12_000);
        let tail = eeg_tail_path(&p);
        let len = std::fs::metadata(&tail).unwrap().len();
        std::fs::File::options()
            .write(true)
            .open(&tail)
            .unwrap()
            .set_len(len - 5)
            .unwrap();
        assert_eq!(
            recover_eeg_parquet(&p).unwrap(),
            EegRecovery::Recovered { rows: 11_999 }
        );
        assert_rows(&p, 11_999);

        // the file itself was cut inside the last row group (power cut before
        // the data reached the disk): the checkpoint no longer fits and is
        // ignored, the journal is used
        let p = crash("exg_1700000004.csv", 6000);
        std::fs::File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_len(1000)
            .unwrap();
        assert_eq!(recover_eeg_parquet(&p).unwrap(), EegRecovery::Recovered { rows: 1000 });

        // nothing at all
        let p = dir.path().join("exg_1700000005.parquet");
        std::fs::write(&p, b"PAR1").unwrap();
        assert_eq!(recover_eeg_parquet(&p).unwrap(), EegRecovery::Unrecoverable);

        // the day sweep leaves files that were touched a moment ago (a live
        // recording looks the same) and repairs them once they have gone quiet
        let a = crash("exg_1700000006.csv", 700);
        assert!(recover_eeg_parquets_in(dir.path(), std::time::Duration::from_secs(60)).is_empty());
        assert!(!eeg_parquet_is_complete(&a));
        let done = recover_eeg_parquets_in(dir.path(), std::time::Duration::ZERO);
        assert_eq!(done, vec![(a.clone(), EegRecovery::Recovered { rows: 700 })]);
        assert_rows(&a, 700);
    }

    /// Whole frames give the same files as one electrode at a time.
    #[test]
    fn eeg_frames_write_what_per_electrode_pushes_write() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("exg_1.csv"), dir.path().join("exg_2.csv"));
        let mut pq_a = ParquetState::open_with_labels(&a, &EEG_LABELS).unwrap();
        let mut pq_b = ParquetState::open_with_labels(&b, &EEG_LABELS).unwrap();
        let mut csv_a = crate::session_csv::CsvState::open_with_labels(&a, &EEG_LABELS).unwrap();
        let mut csv_b = crate::session_csv::CsvState::open_with_labels(&b, &EEG_LABELS).unwrap();
        let rows = 6000;
        push_rows(&mut pq_a, 0, rows);
        for r in 0..rows {
            let ts = 1_700_000_000.0 + r as f64 / 500.0;
            let frame: Vec<f64> = (0..EEG_LABELS.len()).map(|ch| eeg_value(r, ch)).collect();
            pq_b.push_eeg_frame(&frame, ts, 500.0);
            csv_b.push_eeg_frame(&frame, ts, 500.0);
            for (ch, &v) in frame.iter().enumerate() {
                csv_a.push_eeg(ch, &[v], ts, 500.0);
            }
        }
        pq_a.close();
        pq_b.close();
        csv_a.flush();
        csv_b.flush();
        assert_rows(&eeg_parquet_path(&b), rows);
        assert_eq!(
            std::fs::read(eeg_parquet_path(&a)).unwrap(),
            std::fs::read(eeg_parquet_path(&b)).unwrap(),
            "parquet bytes"
        );
        assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap(), "csv bytes");
        assert_eq!(csv_b.eeg_write_counts(), (rows as u64, 0));
    }

    // ── Band metrics ─────────────────────────────────────────────────────────

    /// A 32-channel snapshot like the band analyser produces: `f32` powers,
    /// no PPG or head pose.
    fn metrics_snapshot(base: &BandSnapshot, k: usize) -> BandSnapshot {
        let mut snap = base.clone();
        snap.timestamp = 1_700_000_000.0 + k as f64 * 0.128;
        let mut x = (k as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut noise = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 40) as f32 / (1u64 << 24) as f32
        };
        for ch in &mut snap.channels {
            ch.delta = 20.0 + 10.0 * noise();
            ch.theta = 8.0 + 4.0 * noise();
            ch.alpha = 12.0 + 6.0 * noise();
            ch.beta = 3.0 + noise();
            ch.gamma = 1.0 + 0.5 * noise();
            ch.high_gamma = 0.3 * noise();
            let total = ch.delta + ch.theta + ch.alpha + ch.beta + ch.gamma + ch.high_gamma;
            ch.rel_delta = ch.delta / total;
            ch.rel_theta = ch.theta / total;
            ch.rel_alpha = ch.alpha / total;
            ch.rel_beta = ch.beta / total;
            ch.rel_gamma = ch.gamma / total;
            ch.rel_high_gamma = ch.high_gamma / total;
        }
        snap.faa = noise() - 0.5;
        snap.tar = noise();
        snap.snr = 10.0 * noise();
        snap
    }

    /// One real snapshot from the band analyser, for `channels` channels.
    fn analysed_snapshot(channels: usize) -> BandSnapshot {
        let mut analyser = skill_eeg::eeg_bands::BandAnalyzer::new_with_rate(500.0);
        for i in 0..4000 {
            let t = i as f64 / 500.0;
            for ch in 0..channels {
                analyser.push(
                    ch,
                    &[30.0 * (std::f64::consts::TAU * (9.0 + ch as f64 / 8.0) * t).sin()],
                );
            }
            if analyser.latest.is_some() {
                break;
            }
        }
        analyser.latest.clone().expect("a snapshot")
    }

    /// Five minutes of 32-channel metrics, written the way a session writes
    /// them (a `flush()` every half second): the file is a small fraction of
    /// what the same rows cost with a row group per flush, and reads back
    /// value for value.
    #[test]
    fn metrics_file_is_compact_and_lossless() {
        use arrow_array::Array;
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("exg_1700000000.csv");
        let labels: Vec<String> = (1..=32).map(|i| format!("CH{i}")).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut pq = ParquetState::open_with_labels(&csv_path, &refs).unwrap();
        let schema = pq.metrics_schema.clone();

        // the previous layout, for comparison: Snappy and a row group per flush
        let old_path = dir.path().join("old_metrics.parquet");
        let mut old = ArrowWriter::try_new(
            std::fs::File::create(&old_path).unwrap(),
            schema.clone(),
            Some(writer_props()),
        )
        .unwrap();

        let n_ch = 32.min(skill_constants::EEG_CHANNELS);
        let base = analysed_snapshot(n_ch);
        assert_eq!(base.channels.len(), n_ch);
        let rows = 2344; // 5 min at one snapshot per 64 samples of a 500 Hz stream
        for k in 0..rows {
            let snap = metrics_snapshot(&base, k);
            pq.push_metrics(&csv_path, &snap);
            let mut row = vec![f64::NAN; schema.fields().len()];
            row[0] = snap.timestamp;
            for (c, ch) in snap.channels.iter().enumerate() {
                row[1 + c * 12] = f64::from(ch.delta);
            }
            let cols: Vec<Arc<dyn arrow_array::Array>> = row
                .iter()
                .map(|v| Arc::new(Float64Array::from(vec![*v])) as Arc<dyn arrow_array::Array>)
                .collect();
            old.write(&RecordBatch::try_new(schema.clone(), cols).unwrap()).unwrap();
            // the session flushes every 256 samples = every 4 snapshots
            if k % 4 == 3 {
                pq.flush();
                old.flush().unwrap();
            }
        }
        pq.close();
        old.close().unwrap();

        let path = metrics_parquet_path(&csv_path);
        let (new, old) = (
            std::fs::metadata(&path).unwrap().len() as f64,
            std::fs::metadata(&old_path).unwrap().len() as f64,
        );
        eprintln!(
            "5 min of 32-channel metrics: before {:.1} MB, now {:.2} MB ({:.0}x)",
            old / 1e6,
            new / 1e6,
            old / new
        );
        assert!(new < old / 10.0, "now {new} vs before {old}");

        let builder = ParquetRecordBatchReaderBuilder::try_new(std::fs::File::open(&path).unwrap()).unwrap();
        let groups: Vec<i64> = builder.metadata().row_groups().iter().map(|g| g.num_rows()).collect();
        // a minute of snapshots each (60 / 0.128 = 469), not four
        assert!(groups.len() <= 6 && groups[0] >= 460, "{groups:?}");
        let mut k = 0usize;
        for batch in builder.build().unwrap() {
            let batch = batch.unwrap();
            let col = |i: usize| batch.column(i).as_any().downcast_ref::<Float64Array>().unwrap().clone();
            let last = n_ch - 1;
            let (ts, delta0, rel_hg_last) = (col(0), col(1), col(1 + last * 12 + 11));
            let faa = col(batch.schema().index_of("faa").unwrap());
            let hr = col(batch.schema().index_of("hr_bpm").unwrap());
            for i in 0..batch.num_rows() {
                let want = metrics_snapshot(&base, k);
                assert_eq!(ts.value(i), want.timestamp);
                assert_eq!(delta0.value(i), f64::from(want.channels[0].delta));
                assert_eq!(rel_hg_last.value(i), f64::from(want.channels[last].rel_high_gamma));
                assert_eq!(faa.value(i), f64::from(want.faa));
                assert!(hr.value(i).is_nan() || want.hr.is_some());
                k += 1;
            }
        }
        assert_eq!(k, rows);
    }
}
