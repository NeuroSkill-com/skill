// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Parquet recording writer — drop-in alternative to CSV.
//!
//! Stores EEG, PPG, and metrics data in columnar Apache Parquet files with
//! Snappy compression.  Enabled when `storage_format = "parquet"` in settings.
//!
//! Files produced:
//! - `exg_<ts>.parquet` — raw EEG samples (timestamp + N channel columns)
//! - `exg_<ts>_ppg.parquet` — PPG optical data
//! - `exg_<ts>_metrics.parquet` — derived band-power metrics (~4 Hz)

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::{Float64Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::ppg_analysis::PpgMetrics;
use crate::session_csv::{ppg_csv_path, metrics_csv_path, PPG_SAMPLE_RATE, build_metrics_header};
use skill_eeg::eeg_bands::BandSnapshot;

// ── Row-group flush threshold ─────────────────────────────────────────────────

/// Flush a Parquet row group after this many rows to bound memory.
const EEG_FLUSH_ROWS: usize = 4096;
const PPG_FLUSH_ROWS: usize = 1024;
const METRICS_FLUSH_ROWS: usize = 64;

// ── Path helpers ──────────────────────────────────────────────────────────────

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

// ── Writer properties ─────────────────────────────────────────────────────────

fn writer_props() -> WriterProperties {
    WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build()
}

// ── ParquetState ──────────────────────────────────────────────────────────────

/// Multiplexed Parquet writer for a recording session.
///
/// API-compatible with `CsvState`: call `push_eeg`, `push_ppg`, `push_metrics`,
/// and `flush` in the same way.
pub struct ParquetState {
    // ── EEG ──────────────────────────────────────────────────────────────────
    eeg_wtr:    ArrowWriter<std::fs::File>,
    eeg_schema: Arc<Schema>,
    n_eeg:      usize,
    eeg_ts:     Vec<VecDeque<f64>>,
    eeg_bufs:   Vec<VecDeque<f64>>,
    eeg_rows:   usize,

    // ── PPG (lazy) ───────────────────────────────────────────────────────────
    ppg_wtr:    Option<ArrowWriter<std::fs::File>>,
    ppg_schema: Arc<Schema>,
    ppg_bufs:   [VecDeque<f64>; 3],
    ppg_ts:     [VecDeque<f64>; 3],
    ppg_rows:   usize,
    ppg_path:   PathBuf,

    // ── Metrics (lazy) ───────────────────────────────────────────────────────
    metrics_wtr:    Option<ArrowWriter<std::fs::File>>,
    metrics_schema: Arc<Schema>,
    metrics_n_cols: usize,
    metrics_rows:   usize,
    metrics_path:   PathBuf,

    /// Accumulated metrics rows before flush.
    metrics_pending: Vec<Vec<f64>>,

    // ── IMU (lazy) ───────────────────────────────────────────────────────────
    imu_wtr:    Option<ArrowWriter<std::fs::File>>,
    imu_schema: Arc<Schema>,
    imu_rows:   usize,
    imu_path:   PathBuf,
    imu_pending: Vec<[f64; 10]>,
}

impl ParquetState {
    /// Open a new Parquet EEG file with the given channel labels.
    pub fn open_with_labels(csv_path: &Path, labels: &[&str]) -> Result<Self, String> {
        let pq_path = eeg_parquet_path(csv_path);
        let n = labels.len();

        // EEG schema: timestamp_s + N channel columns
        let mut fields = vec![Field::new("timestamp_s", DataType::Float64, false)];
        for label in labels {
            fields.push(Field::new(*label, DataType::Float64, true));
        }
        let eeg_schema = Arc::new(Schema::new(fields));

        let file = std::fs::File::create(&pq_path)
            .map_err(|e| format!("parquet create {}: {e}", pq_path.display()))?;
        let eeg_wtr = ArrowWriter::try_new(file, eeg_schema.clone(), Some(writer_props()))
            .map_err(|e| format!("parquet writer: {e}"))?;

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
        let metrics_fields: Vec<Field> = metrics_header.iter()
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
            eeg_wtr,
            eeg_schema,
            n_eeg: n,
            eeg_ts:   (0..n).map(|_| VecDeque::new()).collect(),
            eeg_bufs: (0..n).map(|_| VecDeque::new()).collect(),
            eeg_rows: 0,

            ppg_wtr: None,
            ppg_schema,
            ppg_bufs: std::array::from_fn(|_| VecDeque::new()),
            ppg_ts:   std::array::from_fn(|_| VecDeque::new()),
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
        })
    }

    // ── EEG ──────────────────────────────────────────────────────────────────

    pub fn push_eeg(&mut self, electrode: usize, samples: &[f64], packet_ts: f64, sample_rate: f64) {
        if electrode >= self.n_eeg { return; }
        for (i, &v) in samples.iter().enumerate() {
            self.eeg_bufs[electrode].push_back(v);
            self.eeg_ts[electrode].push_back(packet_ts + i as f64 / sample_rate);
        }

        // Drain complete rows (all channels have data).
        let ready = self.eeg_bufs.iter().map(|b| b.len()).min().unwrap_or(0);
        if ready == 0 { return; }

        // Build column arrays.
        let n = self.n_eeg;
        let ts_col: Vec<f64> = (0..ready).filter_map(|_| self.eeg_ts[0].pop_front()).collect();
        for k in 1..n { for _ in 0..ready { self.eeg_ts[k].pop_front(); } }

        let mut columns: Vec<Arc<dyn arrow_array::Array>> = Vec::with_capacity(n + 1);
        columns.push(Arc::new(Float64Array::from(ts_col)));
        for k in 0..n {
            let col: Vec<f64> = (0..ready).filter_map(|_| self.eeg_bufs[k].pop_front()).collect();
            columns.push(Arc::new(Float64Array::from(col)));
        }

        if let Ok(batch) = RecordBatch::try_new(self.eeg_schema.clone(), columns) {
            let _ = self.eeg_wtr.write(&batch);
            self.eeg_rows += ready;
        }

        if self.eeg_rows >= EEG_FLUSH_ROWS {
            let _ = self.eeg_wtr.flush();
            self.eeg_rows = 0;
        }
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
        if channel >= 3 { return; }
        for (i, &v) in samples.iter().enumerate() {
            self.ppg_bufs[channel].push_back(v);
            self.ppg_ts[channel].push_back(packet_ts + i as f64 / PPG_SAMPLE_RATE);
        }

        let ready = self.ppg_bufs.iter().map(|b| b.len()).min().unwrap_or(0);
        if ready == 0 { return; }

        // Lazy-open PPG writer.
        if self.ppg_wtr.is_none() {
            match std::fs::File::create(&self.ppg_path) {
                Ok(f) => match ArrowWriter::try_new(f, self.ppg_schema.clone(), Some(writer_props())) {
                    Ok(w) => { self.ppg_wtr = Some(w); }
                    Err(e) => { eprintln!("[parquet] PPG writer error: {e}"); return; }
                },
                Err(e) => { eprintln!("[parquet] PPG create error: {e}"); return; }
            }
        }

        let ts_col: Vec<f64> = (0..ready).filter_map(|_| self.ppg_ts[0].pop_front()).collect();
        for k in 1..3 { for _ in 0..ready { self.ppg_ts[k].pop_front(); } }

        let ambient:  Vec<f64> = (0..ready).filter_map(|_| self.ppg_bufs[0].pop_front()).collect();
        let infrared: Vec<f64> = (0..ready).filter_map(|_| self.ppg_bufs[1].pop_front()).collect();
        let red:      Vec<f64> = (0..ready).filter_map(|_| self.ppg_bufs[2].pop_front()).collect();

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
                Ok(f) => match ArrowWriter::try_new(f, self.metrics_schema.clone(), Some(writer_props())) {
                    Ok(w) => { self.metrics_wtr = Some(w); }
                    Err(e) => { eprintln!("[parquet] metrics writer error: {e}"); return; }
                },
                Err(e) => { eprintln!("[parquet] metrics create error: {e}"); return; }
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
                ch.delta as f64, ch.theta as f64, ch.alpha as f64,
                ch.beta as f64, ch.gamma as f64, ch.high_gamma as f64,
                ch.rel_delta as f64, ch.rel_theta as f64, ch.rel_alpha as f64,
                ch.rel_beta as f64, ch.rel_gamma as f64, ch.rel_high_gamma as f64,
            ]);
        }
        // Pad to 48 band-power columns (4 channels × 12) if fewer channels.
        while row.len() < 1 + 48 { row.push(f64::NAN); }

        row.extend_from_slice(&[
            snap.faa as f64, snap.tar as f64, snap.bar as f64, snap.dtr as f64,
            snap.pse as f64, snap.apf as f64, snap.bps as f64, snap.snr as f64,
            snap.coherence as f64, snap.mu_suppression as f64, snap.mood as f64,
            snap.tbr as f64, snap.sef95 as f64, snap.spectral_centroid as f64,
            snap.hjorth_activity as f64, snap.hjorth_mobility as f64, snap.hjorth_complexity as f64,
            snap.permutation_entropy as f64, snap.higuchi_fd as f64, snap.dfa_exponent as f64,
            snap.sample_entropy as f64, snap.pac_theta_gamma as f64, snap.laterality_index as f64,
        ]);
        row.extend_from_slice(&[
            opt(snap.hr), opt(snap.rmssd), opt(snap.sdnn), opt(snap.pnn50),
            opt(snap.lf_hf_ratio), opt(snap.respiratory_rate),
            opt(snap.spo2_estimate), opt(snap.perfusion_index), opt(snap.stress_index),
        ]);
        row.extend_from_slice(&[
            opt_u64(snap.blink_count), opt(snap.blink_rate),
        ]);
        row.extend_from_slice(&[
            opt(snap.head_pitch), opt(snap.head_roll), opt(snap.stillness),
            opt_u64(snap.nod_count), opt_u64(snap.shake_count),
        ]);
        row.extend_from_slice(&[
            opt(snap.meditation), opt(snap.cognitive_load), opt(snap.drowsiness),
        ]);
        row.push(opt_u16(snap.temperature_raw));
        row.extend_from_slice(&[
            opt(snap.gpu_overall), opt(snap.gpu_render), opt(snap.gpu_tiler),
        ]);

        self.metrics_pending.push(row);
        self.metrics_rows += 1;

        if self.metrics_rows >= METRICS_FLUSH_ROWS {
            self.flush_metrics();
        }
    }

    fn flush_metrics(&mut self) {
        if self.metrics_pending.is_empty() { return; }
        let Some(ref mut wtr) = self.metrics_wtr else { return; };

        let n_cols = self.metrics_n_cols;
        let n_rows = self.metrics_pending.len();
        let mut col_data: Vec<Vec<f64>> = vec![Vec::with_capacity(n_rows); n_cols];

        for row in &self.metrics_pending {
            for (ci, col) in col_data.iter_mut().enumerate() {
                col.push(if ci < row.len() { row[ci] } else { f64::NAN });
            }
        }

        let columns: Vec<Arc<dyn arrow_array::Array>> = col_data.into_iter()
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
        timestamp_s:   f64,
        accel:         [f32; 3],
        gyro:          Option<[f32; 3]>,
        mag:           Option<[f32; 3]>,
    ) {
        // Lazy-open IMU writer.
        if self.imu_wtr.is_none() {
            match std::fs::File::create(&self.imu_path) {
                Ok(f) => match ArrowWriter::try_new(f, self.imu_schema.clone(), Some(writer_props())) {
                    Ok(w) => { self.imu_wtr = Some(w); }
                    Err(e) => { eprintln!("[parquet] IMU writer error: {e}"); return; }
                },
                Err(e) => { eprintln!("[parquet] IMU create error: {e}"); return; }
            }
        }

        let g = gyro.unwrap_or([0.0; 3]);
        let m = mag.unwrap_or([0.0; 3]);
        self.imu_pending.push([
            timestamp_s,
            accel[0] as f64, accel[1] as f64, accel[2] as f64,
            g[0] as f64, g[1] as f64, g[2] as f64,
            m[0] as f64, m[1] as f64, m[2] as f64,
        ]);
        self.imu_rows += 1;

        if self.imu_rows >= 256 {
            self.flush_imu();
        }
    }

    fn flush_imu(&mut self) {
        if self.imu_pending.is_empty() { return; }
        let Some(ref mut wtr) = self.imu_wtr else { return; };

        let n_rows = self.imu_pending.len();
        let mut col_data: Vec<Vec<f64>> = vec![Vec::with_capacity(n_rows); 10];

        for row in &self.imu_pending {
            for (ci, col) in col_data.iter_mut().enumerate() {
                col.push(row[ci]);
            }
        }

        let columns: Vec<Arc<dyn arrow_array::Array>> = col_data.into_iter()
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

    pub fn flush(&mut self) {
        let _ = self.eeg_wtr.flush();
        if let Some(ref mut w) = self.ppg_wtr { let _ = w.flush(); }
        self.flush_metrics();
        self.flush_imu();
    }

    /// Close all writers, finalising the Parquet files.
    pub fn close(mut self) {
        self.flush_metrics();
        self.flush_imu();
        let _ = self.eeg_wtr.close();
        if let Some(w) = self.ppg_wtr { let _ = w.close(); }
        if let Some(w) = self.metrics_wtr { let _ = w.close(); }
        if let Some(w) = self.imu_wtr { let _ = w.close(); }
    }
}
