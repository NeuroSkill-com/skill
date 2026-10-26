// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Finding recordings on disk and streaming their samples.
//!
//! A recording is one `exg_<unix>.{csv,parquet}` file in a `YYYYMMDD` day
//! directory, with an optional `exg_<unix>.json` sidecar and `_ppg` / `_imu`
//! CSV siblings. Rollover chunks are separate files and so separate
//! recordings here, matching how they are stored.

use std::io::BufRead;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;

/// Device-internal clocks start near zero; anything below this (2001-09-09)
/// is a relative timestamp, not Unix time.
const MIN_UNIX_TS: f64 = 1_000_000_000.0;

/// A raw EEG recording found on disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    pub eeg_path: PathBuf,
    /// Session start from the sidecar, else the file name.
    pub start_utc: f64,
    /// Session end from the sidecar, when the recording was closed cleanly.
    pub end_utc: Option<f64>,
    pub sample_rate: f64,
    pub channel_names: Vec<String>,
    pub device_name: Option<String>,
    pub device_kind: Option<String>,
    pub firmware_version: Option<String>,
    pub serial_number: Option<String>,
}

impl Recording {
    pub fn is_parquet(&self) -> bool {
        self.eeg_path.extension().and_then(|e| e.to_str()) == Some("parquet")
    }

    /// End time for overlap checks. Open-ended (in-progress or crashed)
    /// recordings fall back to the sidecar sample count, then to "unknown".
    fn overlaps(&self, start: Option<f64>, end: Option<f64>) -> bool {
        if end.is_some_and(|e| self.start_utc >= e) {
            return false;
        }
        match (start, self.end_utc) {
            (Some(s), Some(rec_end)) => rec_end > s,
            _ => true,
        }
    }
}

/// Recordings under `skill_dir` that overlap `[start, end)`, oldest first.
///
/// `only`, when non-empty, restricts the result to those recordings —
/// matched by file stem so a `.csv` path also selects its `.parquet` twin.
pub fn discover(skill_dir: &Path, start: Option<f64>, end: Option<f64>, only: &[String]) -> Vec<Recording> {
    let only_stems: std::collections::HashSet<String> = only
        .iter()
        .filter_map(|p| Path::new(p).file_stem().and_then(|s| s.to_str()).map(String::from))
        .collect();

    let mut out = Vec::new();
    let Ok(days) = std::fs::read_dir(skill_dir) else {
        return out;
    };
    let mut day_dirs: Vec<PathBuf> = days
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.len() == 8 && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect();
    day_dirs.sort();

    for day in day_dirs {
        for path in eeg_files(&day) {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            if !only_stems.is_empty() && !only_stems.contains(stem) {
                continue;
            }
            match load_recording(&path) {
                Ok(rec) if rec.overlaps(start, end) => out.push(rec),
                Ok(_) => {}
                Err(e) => eprintln!("[bids] skipping {}: {e:#}", path.display()),
            }
        }
    }
    out.sort_by(|a, b| a.start_utc.total_cmp(&b.start_utc));
    out
}

/// Raw EEG files in a day directory: `exg_<digits>.csv` / `.parquet`,
/// preferring CSV when a recording was written in both formats.
fn eeg_files(day: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(day)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str());
            let is_data = ext == Some("csv") || (cfg!(feature = "parquet") && ext == Some("parquet"));
            is_data && recording_ts_from_name(p).is_some()
        })
        .collect();
    files.sort();
    let csv_stems: std::collections::HashSet<String> = files
        .iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("csv"))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(String::from))
        .collect();
    files.retain(|p| {
        p.extension().and_then(|e| e.to_str()) != Some("parquet")
            || !csv_stems.contains(p.file_stem().and_then(|s| s.to_str()).unwrap_or_default())
    });
    files
}

/// `exg_1793008760.csv` → `1793008760`. `None` for sibling streams
/// (`exg_<ts>_ppg.csv`, …) and anything else.
fn recording_ts_from_name(path: &Path) -> Option<f64> {
    let digits = path.file_stem()?.to_str()?.strip_prefix("exg_")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn load_recording(eeg_path: &Path) -> Result<Recording> {
    let name_ts = recording_ts_from_name(eeg_path).context("not a recording file name")?;
    let sidecar: serde_json::Value = std::fs::read_to_string(eeg_path.with_extension("json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    let num = |k: &str| {
        sidecar
            .get(k)
            .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
    };
    let text = |k: &str| {
        sidecar
            .get(k)
            .or_else(|| sidecar.get("device").and_then(|d| d.get(k)))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from)
    };

    let channel_names = eeg_channel_names(eeg_path)?;
    if channel_names.is_empty() {
        bail!("no channel columns");
    }
    Ok(Recording {
        eeg_path: eeg_path.to_path_buf(),
        start_utc: num("session_start_utc").filter(|t| *t > MIN_UNIX_TS).unwrap_or(name_ts),
        end_utc: num("session_end_utc").filter(|t| *t > MIN_UNIX_TS),
        sample_rate: num("sample_rate_hz")
            .or_else(|| num("sample_rate"))
            .filter(|r| *r > 0.0)
            .unwrap_or(skill_data::session_csv::EEG_SAMPLE_RATE),
        channel_names,
        device_name: text("device_name"),
        device_kind: text("device_kind"),
        firmware_version: text("firmware_version"),
        serial_number: text("serial_number"),
    })
}

/// Channel names from the data file itself (authoritative over the sidecar).
fn eeg_channel_names(eeg_path: &Path) -> Result<Vec<String>> {
    if eeg_path.extension().and_then(|e| e.to_str()) == Some("parquet") {
        #[cfg(feature = "parquet")]
        return skill_data::session_parquet::read_eeg_parquet_channels(eeg_path);
        #[cfg(not(feature = "parquet"))]
        bail!("parquet support not compiled in");
    }
    let (_, header) = csv_header(eeg_path)?;
    Ok(header)
}

/// First line of a CSV split into `(timestamp column, other columns)`.
fn csv_header(path: &Path) -> Result<(String, Vec<String>)> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut line = String::new();
    std::io::BufReader::new(file).read_line(&mut line)?;
    let mut cols = line.trim().split(',').map(|s| s.trim().to_string());
    let ts = cols.next().unwrap_or_default();
    if !ts.contains("timestamp") {
        bail!("{} has no timestamp column", path.display());
    }
    Ok((ts, cols.collect()))
}

/// Maps a file's timestamps onto Unix time. Relative device clocks are
/// anchored at the recording start, as the rest of skill does.
struct Clock {
    anchor: f64,
    first: Option<f64>,
}

impl Clock {
    fn new(anchor: f64) -> Self {
        Self { anchor, first: None }
    }

    fn wall(&mut self, ts: f64) -> f64 {
        let first = *self.first.get_or_insert(ts);
        if first < MIN_UNIX_TS {
            self.anchor + (ts - first)
        } else {
            ts
        }
    }
}

/// Stream EEG frames with wall-clock time in `[start, end)`.
///
/// `on_frame(wall_ts, samples)` gets one value per channel, in
/// `rec.channel_names` order. Rows that fail to parse are skipped (as the
/// reembed loader does); rows are time-ordered, so reading stops at `end`.
pub fn read_eeg(
    rec: &Recording,
    start: Option<f64>,
    end: Option<f64>,
    mut on_frame: impl FnMut(f64, &[f32]) -> Result<()>,
) -> Result<()> {
    if rec.is_parquet() {
        return read_eeg_parquet(rec, start, end, on_frame);
    }
    let file = std::fs::File::open(&rec.eeg_path).with_context(|| format!("open {}", rec.eeg_path.display()))?;
    let n_ch = rec.channel_names.len();
    let mut clock = Clock::new(rec.start_utc);
    let mut frame = vec![0f32; n_ch];
    for line in std::io::BufReader::with_capacity(1 << 20, file).lines().skip(1) {
        let Ok(line) = line else { break };
        let mut fields = line.split(',');
        let Some(ts) = fields.next().and_then(|t| t.trim().parse::<f64>().ok()) else {
            continue;
        };
        let wall = clock.wall(ts);
        if end.is_some_and(|e| wall >= e) {
            break;
        }
        if start.is_some_and(|s| wall < s) {
            continue;
        }
        let mut ok = true;
        for slot in frame.iter_mut() {
            match fields.next().and_then(|f| f.trim().parse::<f32>().ok()) {
                Some(v) => *slot = v,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            on_frame(wall, &frame)?;
        }
    }
    Ok(())
}

#[cfg(feature = "parquet")]
fn read_eeg_parquet(
    rec: &Recording,
    start: Option<f64>,
    end: Option<f64>,
    mut on_frame: impl FnMut(f64, &[f32]) -> Result<()>,
) -> Result<()> {
    // The parquet reader returns only the first timestamp, so samples are
    // placed on the nominal sample grid from there.
    let (first, _, channels) = skill_data::session_parquet::read_eeg_parquet(&rec.eeg_path)?;
    let t0 = Clock::new(rec.start_utc).wall(first);
    let n = channels.first().map_or(0, Vec::len);
    let mut frame = vec![0f32; channels.len()];
    for i in 0..n {
        let wall = t0 + i as f64 / rec.sample_rate;
        if end.is_some_and(|e| wall >= e) {
            break;
        }
        if start.is_some_and(|s| wall < s) {
            continue;
        }
        for (slot, ch) in frame.iter_mut().zip(&channels) {
            *slot = ch[i];
        }
        on_frame(wall, &frame)?;
    }
    Ok(())
}

#[cfg(not(feature = "parquet"))]
fn read_eeg_parquet(
    rec: &Recording,
    _start: Option<f64>,
    _end: Option<f64>,
    _on_frame: impl FnMut(f64, &[f32]) -> Result<()>,
) -> Result<()> {
    bail!("{}: parquet support not compiled in", rec.eeg_path.display())
}

/// What [`read_aux_csv`] saw: the kept columns and the time span of the
/// rows it delivered.
pub struct AuxStats {
    pub columns: Vec<String>,
    pub n_rows: u64,
    pub first_ts: f64,
    pub last_ts: f64,
}

impl AuxStats {
    /// Sampling rate estimated from the timestamps, rounded to a whole
    /// number of Hz when within 2% of one. `None` with fewer than two rows
    /// or a zero time span.
    pub fn estimated_rate(&self) -> Option<f64> {
        let span = self.last_ts - self.first_ts;
        if self.n_rows < 2 || span <= 0.0 {
            return None;
        }
        let rate = (self.n_rows - 1) as f64 / span;
        let rounded = rate.round();
        Some(if rounded > 0.0 && ((rate - rounded) / rounded).abs() < 0.02 {
            rounded
        } else {
            (rate * 1000.0).round() / 1000.0
        })
    }
}

/// Stream a sibling CSV (`_ppg.csv`, `_imu.csv`), keeping columns for which
/// `keep(name)` is true and rows with wall-clock time in `[start, end)`.
///
/// `on_row(columns, wall_ts, values)` is called per row; unparseable or
/// blank cells are `None`. Returns `None` if the file is absent or no column
/// is kept, otherwise the stats of what was delivered (possibly zero rows).
pub fn read_aux_csv(
    path: &Path,
    anchor_utc: f64,
    start: f64,
    end: f64,
    keep: impl Fn(&str) -> bool,
    mut on_row: impl FnMut(&[String], f64, &[Option<f64>]) -> Result<()>,
) -> Result<Option<AuxStats>> {
    if !path.exists() {
        return Ok(None);
    }
    let (_, header) = csv_header(path)?;
    let picked: Vec<usize> = (0..header.len()).filter(|&i| keep(&header[i])).collect();
    if picked.is_empty() {
        return Ok(None);
    }
    let mut stats = AuxStats {
        columns: picked.iter().map(|&i| header[i].clone()).collect(),
        n_rows: 0,
        first_ts: 0.0,
        last_ts: 0.0,
    };
    let file = std::fs::File::open(path)?;
    let mut clock = Clock::new(anchor_utc);
    let mut values: Vec<Option<f64>> = vec![None; picked.len()];
    for line in std::io::BufReader::with_capacity(1 << 20, file).lines().skip(1) {
        let Ok(line) = line else { break };
        let mut fields = line.split(',');
        let Some(ts) = fields.next().and_then(|t| t.trim().parse::<f64>().ok()) else {
            continue;
        };
        let wall = clock.wall(ts);
        if wall >= end {
            break;
        }
        if wall < start {
            continue;
        }
        // `picked` is ascending, so one pass over the fields fills `values`.
        values.iter_mut().for_each(|v| *v = None);
        let mut want = picked.iter().copied().enumerate().peekable();
        for (col, cell) in fields.enumerate() {
            match want.peek() {
                Some(&(slot, i)) if i == col => {
                    values[slot] = cell.trim().parse::<f64>().ok();
                    want.next();
                }
                Some(_) => {}
                None => break,
            }
        }
        on_row(&stats.columns, wall, &values)?;
        if stats.n_rows == 0 {
            stats.first_ts = wall;
        }
        stats.last_ts = wall;
        stats.n_rows += 1;
    }
    Ok(Some(stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_names() {
        assert_eq!(
            recording_ts_from_name(Path::new("/d/exg_1793008760.csv")),
            Some(1_793_008_760.0)
        );
        assert_eq!(recording_ts_from_name(Path::new("/d/exg_1793008760_ppg.csv")), None);
        assert_eq!(recording_ts_from_name(Path::new("/d/exg_.csv")), None);
        assert_eq!(recording_ts_from_name(Path::new("/d/log_1793008760.txt")), None);
    }

    #[test]
    fn relative_clock_is_anchored() {
        let mut c = Clock::new(1_800_000_000.0);
        assert_eq!(c.wall(12.0), 1_800_000_000.0);
        assert_eq!(c.wall(13.5), 1_800_000_001.5);
        let mut abs = Clock::new(1.0);
        assert_eq!(abs.wall(1_800_000_000.0), 1_800_000_000.0);
    }

    #[test]
    fn rate_estimate_rounds_near_integers() {
        let s = AuxStats {
            columns: vec![],
            n_rows: 65,
            first_ts: 0.0,
            last_ts: 64.0 / 64.2,
        };
        assert_eq!(s.estimated_rate(), Some(64.0));
    }
}
