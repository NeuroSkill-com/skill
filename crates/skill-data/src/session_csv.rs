// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// CSV recording: CsvState (EEG + PPG + metrics writers) and path helpers.
//
// The Tauri-coupled functions (`new_csv_path`, `write_session_meta`) remain in
// `src-tauri/src/session_csv.rs`; this module holds the pure, framework-free
// CSV writer and path utilities.

use std::{collections::VecDeque, path::{Path, PathBuf}};

use crate::ppg_analysis::PpgMetrics;
use skill_eeg::eeg_bands::BandSnapshot;

// ── Sample-rate constants ─────────────────────────────────────────────────────
// Canonical values live in skill-constants; re-exported here as f64 for CSV use.

pub const EEG_SAMPLE_RATE: f64 = skill_constants::MUSE_SAMPLE_RATE as f64;
pub const PPG_SAMPLE_RATE: f64 = skill_constants::PPG_SAMPLE_RATE as f64;

// ── CSV path helpers ──────────────────────────────────────────────────────────

/// Derive the PPG CSV path from an EEG CSV path.
/// `exg_1700000000.csv` → `exg_1700000000_ppg.csv`
pub fn ppg_csv_path(eeg_path: &Path) -> PathBuf {
    let stem = eeg_path.file_stem().and_then(|s| s.to_str()).unwrap_or("exg");
    eeg_path.with_file_name(format!("{stem}_ppg.csv"))
}

/// Derive the metrics CSV path from an EEG CSV path.
/// `exg_1700000000.csv` → `exg_1700000000_metrics.csv`
pub fn metrics_csv_path(eeg_path: &Path) -> PathBuf {
    let stem = eeg_path.file_stem().and_then(|s| s.to_str()).unwrap_or("exg");
    eeg_path.with_file_name(format!("{stem}_metrics.csv"))
}

/// Derive the IMU CSV path from an EEG CSV path.
/// `exg_1700000000.csv` → `exg_1700000000_imu.csv`
pub fn imu_csv_path(eeg_path: &Path) -> PathBuf {
    let stem = eeg_path.file_stem().and_then(|s| s.to_str()).unwrap_or("exg");
    eeg_path.with_file_name(format!("{stem}_imu.csv"))
}

// ── Metrics CSV column header ─────────────────────────────────────────────────

/// Column headers for the `_metrics.csv` file.
///
/// Layout:
/// - timestamp
/// - 4 channels × (6 absolute + 6 relative) = 48 band power columns
/// - 23 cross-channel EEG indices
/// - 9 PPG vitals
/// - 2 artifact events
/// - 5 head pose
/// - 3 composite scores
/// - 1 temperature
/// - 3 GPU utilisation
/// Cross-channel metric column names (after the per-channel band powers).
pub const METRICS_CROSS_CHANNEL_HEADER: [&str; 46] = [
    // ── Cross-channel EEG indices ──
    "faa", "tar", "bar", "dtr", "pse", "apf", "bps", "snr",
    "coherence", "mu_suppression", "mood", "tbr", "sef95", "spectral_centroid",
    "hjorth_activity", "hjorth_mobility", "hjorth_complexity",
    "permutation_entropy", "higuchi_fd", "dfa_exponent",
    "sample_entropy", "pac_theta_gamma", "laterality_index",
    // ── PPG vitals ──
    "hr_bpm", "rmssd_ms", "sdnn_ms", "pnn50_pct", "lf_hf_ratio",
    "respiratory_rate_bpm", "spo2_pct", "perfusion_index_pct", "stress_index",
    // ── Artifact events ──
    "blink_count", "blink_rate_per_min",
    // ── Head pose ──
    "head_pitch_deg", "head_roll_deg", "stillness", "nod_count", "shake_count",
    // ── Composite scores ──
    "meditation", "cognitive_load", "drowsiness",
    // ── Telemetry ──
    "temperature_raw",
    // ── GPU utilisation ──
    "gpu_overall_pct", "gpu_render_pct", "gpu_tiler_pct",
];

/// Band-power suffixes for each channel (6 absolute + 6 relative = 12 per channel).
const BAND_SUFFIXES: [&str; 12] = [
    "_delta", "_theta", "_alpha", "_beta", "_gamma", "_high_gamma",
    "_rel_delta", "_rel_theta", "_rel_alpha", "_rel_beta", "_rel_gamma", "_rel_high_gamma",
];

/// Build the full metrics CSV header dynamically from channel names.
///
/// Layout: `timestamp_s`, then `<ch>_<band>` × N channels × 12 bands,
/// then the 46 cross-channel columns.
pub fn build_metrics_header(channel_names: &[&str]) -> Vec<String> {
    let mut header = Vec::with_capacity(1 + channel_names.len() * 12 + METRICS_CROSS_CHANNEL_HEADER.len());
    header.push("timestamp_s".to_string());
    for ch in channel_names {
        for suffix in &BAND_SUFFIXES {
            header.push(format!("{ch}{suffix}"));
        }
    }
    for &col in &METRICS_CROSS_CHANNEL_HEADER {
        header.push(col.to_string());
    }
    header
}

/// Legacy fixed header for 4-channel Muse (kept for backward-compat reading).
pub const METRICS_CSV_HEADER: [&str; 95] = [
    "timestamp_s",
    "TP9_delta",  "TP9_theta",  "TP9_alpha",  "TP9_beta",  "TP9_gamma",  "TP9_high_gamma",
    "TP9_rel_delta",  "TP9_rel_theta",  "TP9_rel_alpha",  "TP9_rel_beta",  "TP9_rel_gamma",  "TP9_rel_high_gamma",
    "AF7_delta",  "AF7_theta",  "AF7_alpha",  "AF7_beta",  "AF7_gamma",  "AF7_high_gamma",
    "AF7_rel_delta",  "AF7_rel_theta",  "AF7_rel_alpha",  "AF7_rel_beta",  "AF7_rel_gamma",  "AF7_rel_high_gamma",
    "AF8_delta",  "AF8_theta",  "AF8_alpha",  "AF8_beta",  "AF8_gamma",  "AF8_high_gamma",
    "AF8_rel_delta",  "AF8_rel_theta",  "AF8_rel_alpha",  "AF8_rel_beta",  "AF8_rel_gamma",  "AF8_rel_high_gamma",
    "TP10_delta", "TP10_theta", "TP10_alpha", "TP10_beta", "TP10_gamma", "TP10_high_gamma",
    "TP10_rel_delta", "TP10_rel_theta", "TP10_rel_alpha", "TP10_rel_beta", "TP10_rel_gamma", "TP10_rel_high_gamma",
    "faa", "tar", "bar", "dtr", "pse", "apf", "bps", "snr",
    "coherence", "mu_suppression", "mood", "tbr", "sef95", "spectral_centroid",
    "hjorth_activity", "hjorth_mobility", "hjorth_complexity",
    "permutation_entropy", "higuchi_fd", "dfa_exponent",
    "sample_entropy", "pac_theta_gamma", "laterality_index",
    "hr_bpm", "rmssd_ms", "sdnn_ms", "pnn50_pct", "lf_hf_ratio",
    "respiratory_rate_bpm", "spo2_pct", "perfusion_index_pct", "stress_index",
    "blink_count", "blink_rate_per_min",
    "head_pitch_deg", "head_roll_deg", "stillness", "nod_count", "shake_count",
    "meditation", "cognitive_load", "drowsiness",
    "temperature_raw",
    "gpu_overall_pct", "gpu_render_pct", "gpu_tiler_pct",
];

// ── CSV writer ────────────────────────────────────────────────────────────────

/// Multiplexed CSV writer for a recording session.
///
/// Maintains three lazily-created files:
/// - EEG samples (`muse_<ts>.csv`) — created on construction
/// - PPG samples (`muse_<ts>_ppg.csv`) — created on first PPG packet
/// - Derived metrics (`muse_<ts>_metrics.csv`) — created on first band snapshot
pub struct CsvState {
    wtr:     csv::Writer<std::fs::File>,
    /// Number of EEG channels in this CSV (4 for Muse/Ganglion, 8/16/24 for Cyton/Galea).
    n_eeg:   usize,
    /// Channel labels (used for dynamic metrics header generation).
    channel_labels: Vec<String>,
    /// Queued µV values per EEG channel.
    bufs:    Vec<VecDeque<f64>>,
    /// Per-sample Unix timestamps (seconds) matching each value in `bufs`.
    ts_bufs: Vec<VecDeque<f64>>,
    /// Rows written so far — used to drive periodic disk flushes.
    written: u64,
    /// Separate CSV writer for PPG data (created lazily on first PPG sample).
    ppg_wtr:     Option<csv::Writer<std::fs::File>>,
    /// Queued raw ADC values per PPG channel (0=ambient, 1=infrared, 2=red).
    ppg_bufs:    [VecDeque<f64>; 3],
    /// Per-sample Unix timestamps for PPG channels.
    ppg_ts_bufs: [VecDeque<f64>; 3],
    /// PPG rows written.
    ppg_written: u64,
    /// Separate CSV writer for derived metrics (~4 Hz, created lazily).
    metrics_wtr:     Option<csv::Writer<std::fs::File>>,
    /// Metrics rows written.
    metrics_written: u64,
    /// Separate CSV writer for IMU data (created lazily on first IMU sample).
    imu_wtr:     Option<csv::Writer<std::fs::File>>,
    /// IMU rows written.
    imu_written: u64,
}

impl CsvState {
    pub fn open(path: &Path) -> Result<Self, csv::Error> {
        Self::open_with_labels(path, &["TP9", "AF7", "AF8", "TP10"])
    }

    pub fn open_with_labels(path: &Path, labels: &[&str]) -> Result<Self, csv::Error> {
        let n = labels.len();
        let mut wtr = csv::Writer::from_path(path)?;
        let mut header = vec!["timestamp_s"];
        header.extend_from_slice(labels);
        wtr.write_record(&header)?;
        Ok(Self {
            wtr,
            n_eeg:   n,
            channel_labels: labels.iter().map(|s| s.to_string()).collect(),
            bufs:    (0..n).map(|_| VecDeque::new()).collect(),
            ts_bufs: (0..n).map(|_| VecDeque::new()).collect(),
            written: 0,
            ppg_wtr:     None,
            ppg_bufs:    std::array::from_fn(|_| VecDeque::new()),
            ppg_ts_bufs: std::array::from_fn(|_| VecDeque::new()),
            ppg_written: 0,
            metrics_wtr:     None,
            metrics_written: 0,
            imu_wtr:     None,
            imu_written: 0,
        })
    }

    /// Buffer `samples` for `electrode` and flush any complete rows to disk.
    pub fn push_eeg(&mut self, electrode: usize, samples: &[f64], packet_ts: f64, sample_rate: f64) {
        if electrode >= self.n_eeg { return; }
        for (i, &v) in samples.iter().enumerate() {
            self.bufs[electrode].push_back(v);
            self.ts_bufs[electrode].push_back(packet_ts + i as f64 / sample_rate);
        }

        let ready = self.bufs.iter().map(|b| b.len()).min().unwrap_or(0);
        let n = self.n_eeg;
        for _ in 0..ready {
            let ts = self.ts_bufs[0].pop_front().unwrap();
            for k in 1..n { self.ts_bufs[k].pop_front(); }

            let mut row = vec![format!("{:.6}", ts)];
            for k in 0..n {
                row.push(format!("{:.4}", self.bufs[k].pop_front().unwrap()));
            }
            let refs: Vec<&str> = row.iter().map(String::as_str).collect();
            let _ = self.wtr.write_record(&refs);
            self.written += 1;
        }

        if self.written > 0 && self.written.is_multiple_of(256) {
            let _ = self.wtr.flush();
        }
    }

    /// Buffer PPG samples for `channel` (0-2) and flush complete rows.
    pub fn push_ppg(
        &mut self,
        eeg_csv_path: &Path,
        channel:      usize,
        samples:      &[f64],
        packet_ts:    f64,
        ppg_vitals:   Option<&PpgMetrics>,
    ) {
        if channel >= 3 { return; }

        if self.ppg_wtr.is_none() {
            let ppg_path = ppg_csv_path(eeg_csv_path);
            match csv::Writer::from_path(&ppg_path) {
                Ok(mut w) => {
                    let _ = w.write_record([
                        "timestamp_s", "ambient", "infrared", "red",
                        "hr_bpm", "rmssd_ms", "sdnn_ms", "pnn50_pct", "lf_hf_ratio",
                        "respiratory_rate_bpm", "spo2_pct", "perfusion_index_pct", "stress_index",
                    ]);
                    eprintln!("[csv] PPG file opened: {}", ppg_path.display());
                    self.ppg_wtr = Some(w);
                }
                Err(e) => {
                    eprintln!("[csv] failed to create PPG file {}: {e}", ppg_path.display());
                    return;
                }
            }
        }

        for (i, &v) in samples.iter().enumerate() {
            self.ppg_bufs[channel].push_back(v);
            self.ppg_ts_bufs[channel].push_back(packet_ts + i as f64 / PPG_SAMPLE_RATE);
        }

        let ready = self.ppg_bufs.iter().map(|b| b.len()).min().unwrap_or(0);
        if let Some(ref mut wtr) = self.ppg_wtr {
            for _ in 0..ready {
                let ts = self.ppg_ts_bufs[0].pop_front().unwrap();
                for k in 1..3 { self.ppg_ts_bufs[k].pop_front(); }

                let mut row = vec![format!("{:.6}", ts)];
                for k in 0..3 {
                    row.push(format!("{:.1}", self.ppg_bufs[k].pop_front().unwrap()));
                }
                if let Some(v) = ppg_vitals {
                    row.push(format!("{:.1}", v.hr));
                    row.push(format!("{:.2}", v.rmssd));
                    row.push(format!("{:.2}", v.sdnn));
                    row.push(format!("{:.2}", v.pnn50));
                    row.push(format!("{:.4}", v.lf_hf_ratio));
                    row.push(format!("{:.2}", v.respiratory_rate));
                    row.push(format!("{:.2}", v.spo2_estimate));
                    row.push(format!("{:.4}", v.perfusion_index));
                    row.push(format!("{:.1}", v.stress_index));
                } else {
                    for _ in 0..9 { row.push(String::new()); }
                }
                let refs: Vec<&str> = row.iter().map(String::as_str).collect();
                let _ = wtr.write_record(&refs);
                self.ppg_written += 1;
            }
            if self.ppg_written > 0 && self.ppg_written.is_multiple_of(64) {
                let _ = wtr.flush();
            }
        }
    }

    /// Write an IMU sample to the `_imu.csv` file.
    /// The file is created lazily on the first call.
    pub fn push_imu(
        &mut self,
        eeg_csv_path: &Path,
        timestamp_s:  f64,
        accel:        [f32; 3],
        gyro:         Option<[f32; 3]>,
        mag:          Option<[f32; 3]>,
    ) {
        if self.imu_wtr.is_none() {
            let imu_path = imu_csv_path(eeg_csv_path);
            match csv::Writer::from_path(&imu_path) {
                Ok(mut w) => {
                    let _ = w.write_record([
                        "timestamp_s",
                        "accel_x", "accel_y", "accel_z",
                        "gyro_x", "gyro_y", "gyro_z",
                        "mag_x", "mag_y", "mag_z",
                    ]);
                    eprintln!("[csv] IMU file opened: {}", imu_path.display());
                    self.imu_wtr = Some(w);
                }
                Err(e) => {
                    eprintln!("[csv] failed to create IMU file {}: {e}", imu_path.display());
                    return;
                }
            }
        }

        if let Some(ref mut wtr) = self.imu_wtr {
            let g = gyro.unwrap_or([0.0; 3]);
            let m = mag.unwrap_or([0.0; 3]);
            let row = [
                format!("{:.6}", timestamp_s),
                format!("{:.6}", accel[0]), format!("{:.6}", accel[1]), format!("{:.6}", accel[2]),
                format!("{:.6}", g[0]), format!("{:.6}", g[1]), format!("{:.6}", g[2]),
                format!("{:.6}", m[0]), format!("{:.6}", m[1]), format!("{:.6}", m[2]),
            ];
            let refs: Vec<&str> = row.iter().map(String::as_str).collect();
            let _ = wtr.write_record(&refs);
            self.imu_written += 1;
            if self.imu_written > 0 && self.imu_written.is_multiple_of(64) {
                let _ = wtr.flush();
            }
        }
    }

    /// Write a `BandSnapshot` row to the `_metrics.csv` file (~4 Hz).
    /// The file is created lazily on the first call.
    pub fn push_metrics(&mut self, eeg_csv_path: &Path, snap: &BandSnapshot) {
        if self.metrics_wtr.is_none() {
            let path = metrics_csv_path(eeg_csv_path);
            match csv::Writer::from_path(&path) {
                Ok(mut w) => {
                    let label_refs: Vec<&str> = self.channel_labels.iter().map(|s| s.as_str()).collect();
                    let header = build_metrics_header(&label_refs);
                    let header_refs: Vec<&str> = header.iter().map(|s| s.as_str()).collect();
                    let _ = w.write_record(&header_refs);
                    eprintln!("[csv] Metrics file opened: {}", path.display());
                    self.metrics_wtr = Some(w);
                }
                Err(e) => {
                    eprintln!("[csv] failed to create metrics file {}: {e}", path.display());
                    return;
                }
            }
        }

        if let Some(ref mut wtr) = self.metrics_wtr {
            let opt_f64 = |v: Option<f64>| v.map_or(String::new(), |x| format!("{:.4}", x));
            let opt_u64 = |v: Option<u64>| v.map_or(String::new(), |x| x.to_string());
            let opt_u16 = |v: Option<u16>| v.map_or(String::new(), |x| x.to_string());

            let mut row: Vec<String> = Vec::with_capacity(100);
            row.push(format!("{:.6}", snap.timestamp));

            for ch in &snap.channels {
                row.push(format!("{:.6}", ch.delta));
                row.push(format!("{:.6}", ch.theta));
                row.push(format!("{:.6}", ch.alpha));
                row.push(format!("{:.6}", ch.beta));
                row.push(format!("{:.6}", ch.gamma));
                row.push(format!("{:.6}", ch.high_gamma));
                row.push(format!("{:.6}", ch.rel_delta));
                row.push(format!("{:.6}", ch.rel_theta));
                row.push(format!("{:.6}", ch.rel_alpha));
                row.push(format!("{:.6}", ch.rel_beta));
                row.push(format!("{:.6}", ch.rel_gamma));
                row.push(format!("{:.6}", ch.rel_high_gamma));
            }

            // Cross-channel / derived EEG indices
            row.push(format!("{:.6}", snap.faa));
            row.push(format!("{:.4}", snap.tar));
            row.push(format!("{:.4}", snap.bar));
            row.push(format!("{:.4}", snap.dtr));
            row.push(format!("{:.6}", snap.pse));
            row.push(format!("{:.2}", snap.apf));
            row.push(format!("{:.4}", snap.bps));
            row.push(format!("{:.2}", snap.snr));
            row.push(format!("{:.6}", snap.coherence));
            row.push(format!("{:.6}", snap.mu_suppression));
            row.push(format!("{:.2}", snap.mood));
            row.push(format!("{:.4}", snap.tbr));
            row.push(format!("{:.2}", snap.sef95));
            row.push(format!("{:.2}", snap.spectral_centroid));
            row.push(format!("{:.4}", snap.hjorth_activity));
            row.push(format!("{:.6}", snap.hjorth_mobility));
            row.push(format!("{:.6}", snap.hjorth_complexity));
            row.push(format!("{:.6}", snap.permutation_entropy));
            row.push(format!("{:.6}", snap.higuchi_fd));
            row.push(format!("{:.6}", snap.dfa_exponent));
            row.push(format!("{:.6}", snap.sample_entropy));
            row.push(format!("{:.6}", snap.pac_theta_gamma));
            row.push(format!("{:.6}", snap.laterality_index));

            // PPG vitals
            row.push(opt_f64(snap.hr));
            row.push(opt_f64(snap.rmssd));
            row.push(opt_f64(snap.sdnn));
            row.push(opt_f64(snap.pnn50));
            row.push(opt_f64(snap.lf_hf_ratio));
            row.push(opt_f64(snap.respiratory_rate));
            row.push(opt_f64(snap.spo2_estimate));
            row.push(opt_f64(snap.perfusion_index));
            row.push(opt_f64(snap.stress_index));

            // Artifact events
            row.push(opt_u64(snap.blink_count));
            row.push(opt_f64(snap.blink_rate));

            // Head pose
            row.push(opt_f64(snap.head_pitch));
            row.push(opt_f64(snap.head_roll));
            row.push(opt_f64(snap.stillness));
            row.push(opt_u64(snap.nod_count));
            row.push(opt_u64(snap.shake_count));

            // Composite scores
            row.push(opt_f64(snap.meditation));
            row.push(opt_f64(snap.cognitive_load));
            row.push(opt_f64(snap.drowsiness));

            // Temperature
            row.push(opt_u16(snap.temperature_raw));

            // GPU utilisation
            row.push(opt_f64(snap.gpu_overall));
            row.push(opt_f64(snap.gpu_render));
            row.push(opt_f64(snap.gpu_tiler));

            let refs: Vec<&str> = row.iter().map(String::as_str).collect();
            let _ = wtr.write_record(&refs);
            self.metrics_written += 1;

            if self.metrics_written.is_multiple_of(4) {
                let _ = wtr.flush();
            }
        }
    }

    /// Flush all open CSV writers to disk.
    pub fn flush(&mut self) {
        let _ = self.wtr.flush();
        if let Some(ref mut w) = self.ppg_wtr     { let _ = w.flush(); }
        if let Some(ref mut w) = self.metrics_wtr { let _ = w.flush(); }
        if let Some(ref mut w) = self.imu_wtr     { let _ = w.flush(); }
    }
}
