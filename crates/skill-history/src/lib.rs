// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Session history, metrics, time-series, sleep staging, and analysis.
// Pure library crate — no Tauri dependencies.  Thin Tauri IPC wrappers
// live in `src-tauri/src/{history_cmds,session_analysis}.rs`.

use std::path::Path;

use serde::{Deserialize, Serialize};
use skill_data::label_store::LabelRow;
use skill_data::session_csv::{metrics_csv_path, ppg_csv_path};
use skill_data::util::ts_to_unix;

mod local_days;
pub use local_days::*;

pub mod cache;
pub mod metrics;

// Re-export types consumed by Tauri wrappers.
pub use cache::*;
pub use metrics::*;
pub use skill_data::label_store;

// ── Session file helpers ──────────────────────────────────────────────────────

/// Canonical prefix for new session files.
const SESSION_PREFIX: &str = "exg_";
/// Legacy prefix kept for backward compatibility.
const LEGACY_PREFIX: &str = "muse_";

/// Returns `true` if `fname` is a session JSON sidecar (exg_*.json or muse_*.json).
fn is_session_json(fname: &str) -> bool {
    (fname.starts_with(SESSION_PREFIX) || fname.starts_with(LEGACY_PREFIX)) && fname.ends_with(".json")
}

/// Returns `true` if `fname` is a primary EEG data file (CSV or Parquet, not metrics/ppg).
fn is_session_data(fname: &str) -> bool {
    let has_prefix = fname.starts_with(SESSION_PREFIX) || fname.starts_with(LEGACY_PREFIX);
    if !has_prefix {
        return false;
    }
    let is_eeg_csv = fname.ends_with(".csv")
        && !fname.ends_with("_metrics.csv")
        && !fname.ends_with("_ppg.csv")
        && !fname.ends_with("_imu.csv");
    let is_eeg_parquet = fname.ends_with(".parquet")
        && !fname.ends_with("_metrics.parquet")
        && !fname.ends_with("_ppg.parquet")
        && !fname.ends_with("_imu.parquet");
    is_eeg_csv || is_eeg_parquet
}

/// Backward-compatible alias.
fn is_session_csv(fname: &str) -> bool {
    is_session_data(fname)
}

/// Extract the Unix timestamp from a session filename like `exg_1700000000.csv`,
/// `exg_1700000000.parquet`, or `muse_1700000000.json`.
fn extract_timestamp(fname: &str) -> Option<u64> {
    fname
        .rsplit_once('_')
        .and_then(|(_, ts_part)| {
            ts_part
                .strip_suffix(".csv")
                .or_else(|| ts_part.strip_suffix(".parquet"))
                .or_else(|| ts_part.strip_suffix(".json"))
        })
        .and_then(|s| s.parse().ok())
}

/// Given a base EEG path (`.csv` or `.parquet`), find the corresponding
/// metrics data file, preferring `.parquet` if it exists, falling back to `.csv`.
pub fn find_metrics_path(eeg_path: &Path) -> Option<std::path::PathBuf> {
    use skill_data::session_parquet::metrics_parquet_path;
    let pq = metrics_parquet_path(eeg_path);
    if pq.exists() {
        return Some(pq);
    }
    let csv = metrics_csv_path(eeg_path);
    if csv.exists() {
        return Some(csv);
    }
    None
}

/// Given a base EEG path, find the corresponding PPG data file.
pub fn find_ppg_path(eeg_path: &Path) -> Option<std::path::PathBuf> {
    use skill_data::session_parquet::ppg_parquet_path;
    let pq = ppg_parquet_path(eeg_path);
    if pq.exists() {
        return Some(pq);
    }
    let csv = ppg_csv_path(eeg_path);
    if csv.exists() {
        return Some(csv);
    }
    None
}

// ── SessionEntry ──────────────────────────────────────────────────────────────

/// A session entry read from a JSON sidecar file.
#[derive(Serialize, Deserialize, Clone, Debug)]
/// A session entry populated from the JSON sidecar file of an EEG recording.
/// Contains device metadata, timing, sample counts, labels, and file size.
pub struct SessionEntry {
    pub csv_file: String,
    pub csv_path: String,
    pub session_start_utc: Option<u64>,
    pub session_end_utc: Option<u64>,
    pub session_duration_s: Option<u64>,
    pub device_name: Option<String>,
    pub device_id: Option<String>,
    pub serial_number: Option<String>,
    pub mac_address: Option<String>,
    pub firmware_version: Option<String>,
    pub hardware_version: Option<String>,
    pub headset_preset: Option<String>,
    pub battery_pct: Option<f64>,
    pub total_samples: Option<u64>,
    pub sample_rate_hz: Option<u64>,
    pub labels: Vec<LabelRow>,
    pub file_size_bytes: u64,
    /// Average signal-to-noise ratio (dB) for the session.
    /// `None` for legacy sessions recorded before SNR tracking.
    pub avg_snr_db: Option<f64>,
}

// ── Typed session JSON sidecar (replaces serde_json::Value for speed) ─────────

/// Lightweight typed representation of the session JSON sidecar.
///
/// Using a typed struct avoids the cost of `serde_json::Value` which internally
/// uses `BTreeMap<String, Value>` for JSON objects — expensive to build and
/// drop for every session file.  Only the fields needed by `list_sessions_for_day`
/// are included; unknown fields are silently ignored via `deny_unknown_fields = false`.
///
/// All numeric fields use `relaxed_*` deserializers so that int/float/string
/// representations all parse successfully — a `"sample_rate_hz": 128.0` (float)
/// won't silently kill the entire session.
#[derive(Deserialize, Default)]
struct SessionJsonMeta {
    #[serde(default)]
    csv_file: Option<String>,
    #[serde(default, deserialize_with = "relaxed_opt_u64")]
    session_start_utc: Option<u64>,
    #[serde(default, deserialize_with = "relaxed_opt_u64")]
    session_end_utc: Option<u64>,
    #[serde(default, deserialize_with = "relaxed_opt_u64")]
    session_duration_s: Option<u64>,
    #[serde(default)]
    device: SessionDeviceMeta,
    // Flat fallback fields (legacy format without nested `device` object).
    #[serde(default)]
    device_name: Option<String>,
    #[serde(default)]
    device_id: Option<String>,
    #[serde(default)]
    serial_number: Option<String>,
    #[serde(default)]
    mac_address: Option<String>,
    #[serde(default)]
    firmware_version: Option<String>,
    #[serde(default)]
    hardware_version: Option<String>,
    #[serde(default)]
    headset_preset: Option<String>,
    #[serde(default, deserialize_with = "relaxed_opt_f64")]
    battery_pct_end: Option<f64>,
    #[serde(default, deserialize_with = "relaxed_opt_f64")]
    battery_pct: Option<f64>,
    #[serde(default, deserialize_with = "relaxed_opt_f64")]
    avg_snr_db: Option<f64>,
    #[serde(default, deserialize_with = "relaxed_opt_u64")]
    total_samples: Option<u64>,
    #[serde(default, deserialize_with = "relaxed_opt_u64")]
    sample_rate_hz: Option<u64>,
}

#[derive(Deserialize, Default)]
struct SessionDeviceMeta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    serial_number: Option<String>,
    #[serde(default)]
    mac_address: Option<String>,
    #[serde(default)]
    firmware_version: Option<String>,
    #[serde(default)]
    hardware_version: Option<String>,
    #[serde(default)]
    preset: Option<String>,
}

// ── Relaxed numeric deserializers ─────────────────────────────────────────────
//
// Session JSON sidecars are written by various app versions and device drivers.
// Numeric fields may arrive as JSON integers (`128`), floats (`128.0`), or even
// stringified numbers (`"128"`).  These helpers accept all three forms so that
// a type mismatch never silently drops an entire session.

/// Deserialize an `Option<u64>` from int, float, string, or null.
fn relaxed_opt_u64<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Option<u64>, D::Error> {
    use serde::de::{self, Visitor};

    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Option<u64>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number (int, float, or string) or null")
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> { Ok(None) }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> { Ok(None) }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> { Ok(Some(v)) }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(if v >= 0 { Some(v as u64) } else { None })
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
            Ok(if v >= 0.0 && v <= u64::MAX as f64 { Some(v as u64) } else { None })
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(v.parse::<u64>().ok().or_else(|| v.parse::<f64>().ok().map(|f| f as u64)))
        }
    }
    de.deserialize_any(V)
}

/// Deserialize an `Option<f64>` from int, float, string, or null.
fn relaxed_opt_f64<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Option<f64>, D::Error> {
    use serde::de::{self, Visitor};

    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Option<f64>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number (int, float, or string) or null")
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> { Ok(None) }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> { Ok(None) }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> { Ok(Some(v as f64)) }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> { Ok(Some(v as f64)) }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> { Ok(Some(v)) }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(v.parse::<f64>().ok())
        }
    }
    de.deserialize_any(V)
}

// ── SessionMetrics ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
/// Aggregated band-power metrics computed from a session's metrics CSV.
/// Each field is the mean across all epochs in the session.
pub struct SessionMetrics {
    pub n_epochs: usize,
    pub rel_delta: f64,
    pub rel_theta: f64,
    pub rel_alpha: f64,
    pub rel_beta: f64,
    pub rel_gamma: f64,
    pub rel_high_gamma: f64,
    pub relaxation: f64,
    pub engagement: f64,
    pub faa: f64,
    pub tar: f64,
    pub bar: f64,
    pub dtr: f64,
    pub pse: f64,
    pub apf: f64,
    pub bps: f64,
    pub snr: f64,
    pub coherence: f64,
    pub mu_suppression: f64,
    pub mood: f64,
    pub tbr: f64,
    pub sef95: f64,
    pub spectral_centroid: f64,
    pub hjorth_activity: f64,
    pub hjorth_mobility: f64,
    pub hjorth_complexity: f64,
    pub permutation_entropy: f64,
    pub higuchi_fd: f64,
    pub dfa_exponent: f64,
    pub sample_entropy: f64,
    pub pac_theta_gamma: f64,
    pub laterality_index: f64,
    pub hr: f64,
    pub rmssd: f64,
    pub sdnn: f64,
    pub pnn50: f64,
    pub lf_hf_ratio: f64,
    pub respiratory_rate: f64,
    pub spo2_estimate: f64,
    pub perfusion_index: f64,
    pub stress_index: f64,
    pub blink_count: f64,
    pub blink_rate: f64,
    pub head_pitch: f64,
    pub head_roll: f64,
    pub stillness: f64,
    pub nod_count: f64,
    pub shake_count: f64,
    pub meditation: f64,
    pub cognitive_load: f64,
    pub drowsiness: f64,
}

// ── EpochRow ──────────────────────────────────────────────────────────────────

/// A single epoch's metrics, returned as part of a time-series query.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
/// A single epoch (time window) of band-power metrics from a session.
/// Corresponds to one row in the metrics CSV file.
pub struct EpochRow {
    pub t: f64,
    pub rd: f64,
    pub rt: f64,
    pub ra: f64,
    pub rb: f64,
    pub rg: f64,
    pub relaxation: f64,
    pub engagement: f64,
    pub faa: f64,
    pub tar: f64,
    pub bar: f64,
    pub dtr: f64,
    pub tbr: f64,
    pub pse: f64,
    pub apf: f64,
    pub sef95: f64,
    pub sc: f64,
    pub bps: f64,
    pub snr: f64,
    pub coherence: f64,
    pub mu: f64,
    pub ha: f64,
    pub hm: f64,
    pub hc: f64,
    pub pe: f64,
    pub hfd: f64,
    pub dfa: f64,
    pub se: f64,
    pub pac: f64,
    pub lat: f64,
    pub mood: f64,
    pub hr: f64,
    pub rmssd: f64,
    pub sdnn: f64,
    pub pnn50: f64,
    pub lf_hf: f64,
    pub resp: f64,
    pub spo2: f64,
    pub perf: f64,
    pub stress: f64,
    pub blinks: f64,
    pub blink_r: f64,
    pub pitch: f64,
    pub roll: f64,
    pub still: f64,
    pub nods: f64,
    pub shakes: f64,
    pub med: f64,
    pub cog: f64,
    pub drow: f64,
    pub gpu: f64,
    pub gpu_render: f64,
    pub gpu_tiler: f64,
}

// ── CsvMetricsResult ──────────────────────────────────────────────────────────

/// Combined summary + time-series data loaded directly from `_metrics.csv`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
/// Result of loading a session's metrics CSV: per-epoch time-series + summary.
pub struct CsvMetricsResult {
    pub n_rows: usize,
    pub summary: SessionMetrics,
    pub timeseries: Vec<EpochRow>,
}

// ── Sleep types ───────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
/// A single sleep staging epoch (30-second window).
pub struct SleepEpoch {
    pub utc: u64,
    pub stage: u8,
    pub rel_delta: f64,
    pub rel_theta: f64,
    pub rel_alpha: f64,
    pub rel_beta: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
/// Summary statistics for a sleep session (total time in each stage).
pub struct SleepSummary {
    pub total_epochs: usize,
    pub wake_epochs: usize,
    pub n1_epochs: usize,
    pub n2_epochs: usize,
    pub n3_epochs: usize,
    pub rem_epochs: usize,
    pub epoch_secs: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
/// Complete sleep staging result: per-epoch classification + summary.
pub struct SleepStages {
    pub epochs: Vec<SleepEpoch>,
    pub summary: SleepSummary,
}

// ── History stats ─────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
/// High-level recording statistics: total sessions, hours, and weekly breakdown.
pub struct HistoryStats {
    pub total_sessions: usize,
    pub total_secs: u64,
    pub this_week_secs: u64,
    pub last_week_secs: u64,
}

// ── EmbeddingSession ──────────────────────────────────────────────────────────

/// One contiguous recording range discovered from embedding timestamps.
#[derive(Serialize, Deserialize, Clone, Debug)]
/// A contiguous recording session discovered from embedding timestamps.
/// Used when the JSON sidecar is missing (legacy or corrupted data).
pub struct EmbeddingSession {
    pub start_utc: u64,
    pub end_utc: u64,
    pub n_epochs: u64,
    pub day: String,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Session listing
// ═══════════════════════════════════════════════════════════════════════════════

/// Return recording day directories as `YYYYMMDD` strings, newest first.
pub fn list_session_days(skill_dir: &Path) -> Vec<String> {
    let mut days: Vec<String> = std::fs::read_dir(skill_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            if !(s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) && e.path().is_dir()) {
                return None;
            }
            let has_sessions = std::fs::read_dir(e.path()).into_iter().flatten().flatten().any(|f| {
                let fname = f.file_name();
                let fname = fname.to_string_lossy();
                if is_session_json(&fname) {
                    return true;
                }
                if is_session_csv(&fname) {
                    return !f.path().with_extension("json").exists();
                }
                false
            });
            if has_sessions {
                Some(s.to_string())
            } else {
                None
            }
        })
        .collect();
    days.sort_by(|a, b| b.cmp(a));
    days
}

/// Load all sessions belonging to a single recording day (`YYYYMMDD`).
pub fn list_sessions_for_day(
    day: &str,
    skill_dir: &Path,
    label_store: Option<&label_store::LabelStore>,
) -> Vec<SessionEntry> {
    let day_dir = skill_dir.join(day);
    if !day_dir.is_dir() {
        return vec![];
    }

    let files: Vec<_> = std::fs::read_dir(&day_dir).into_iter().flatten().flatten().collect();
    let mut raw: Vec<(SessionEntry, Option<u64>, Option<u64>)> = Vec::new();

    // First pass: JSON sidecars
    for jf in &files {
        let jp = jf.path();
        let fname = jp.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !is_session_json(fname) {
            continue;
        }

        let Ok(json_bytes) = std::fs::read(&jp) else {
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<SessionJsonMeta>(&json_bytes) else {
            continue;
        };

        let csv_file = meta.csv_file.unwrap_or_default();
        let csv_full = day_dir.join(&csv_file);
        // Prefer Parquet over CSV when both exist.
        let pq_full = csv_full.with_extension("parquet");
        let (data_path, csv_size) = if pq_full.exists() {
            (
                pq_full.clone(),
                std::fs::metadata(&pq_full).map(|m| m.len()).unwrap_or(0),
            )
        } else {
            (
                csv_full.clone(),
                std::fs::metadata(&csv_full).map(|m| m.len()).unwrap_or(0),
            )
        };
        let start = meta.session_start_utc;
        let end = meta.session_end_utc;
        let dev = &meta.device;
        let str_field = |dev_field: &Option<String>, fallback: &Option<String>| -> Option<String> {
            dev_field.clone().or_else(|| fallback.clone())
        };
        raw.push((
            SessionEntry {
                csv_file,
                csv_path: data_path.to_string_lossy().into_owned(),
                session_start_utc: start,
                session_end_utc: end,
                session_duration_s: meta
                    .session_duration_s
                    .or_else(|| start.zip(end).map(|(s, e)| e.saturating_sub(s))),
                device_name: str_field(&dev.name, &meta.device_name),
                device_id: str_field(&dev.id, &meta.device_id),
                serial_number: str_field(&dev.serial_number, &meta.serial_number),
                mac_address: str_field(&dev.mac_address, &meta.mac_address),
                firmware_version: str_field(&dev.firmware_version, &meta.firmware_version),
                hardware_version: str_field(&dev.hardware_version, &meta.hardware_version),
                headset_preset: str_field(&dev.preset, &meta.headset_preset),
                battery_pct: meta.battery_pct_end.or(meta.battery_pct),
                total_samples: meta.total_samples,
                sample_rate_hz: meta.sample_rate_hz,
                labels: vec![],
                file_size_bytes: csv_size,
                avg_snr_db: meta.avg_snr_db,
            },
            start,
            end,
        ));
    }

    // Second pass: orphaned CSVs
    for cf in &files {
        let cp = cf.path();
        let cfname = cp.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !is_session_csv(cfname) {
            continue;
        }
        if cp.with_extension("json").exists() {
            continue;
        }
        let meta_fs = std::fs::metadata(&cp);
        let csv_size = meta_fs.as_ref().map(std::fs::Metadata::len).unwrap_or(0);
        let ts: Option<u64> = extract_timestamp(cfname);
        let end_ts: Option<u64> = meta_fs
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        raw.push((
            SessionEntry {
                csv_file: cfname.to_string(),
                csv_path: cp.to_string_lossy().into_owned(),
                session_start_utc: ts,
                session_end_utc: end_ts,
                session_duration_s: ts.zip(end_ts).map(|(s, e)| e.saturating_sub(s)),
                device_name: None,
                device_id: None,
                serial_number: None,
                mac_address: None,
                firmware_version: None,
                hardware_version: None,
                headset_preset: None,
                battery_pct: None,
                total_samples: None,
                sample_rate_hz: None, // unknown — no JSON metadata available
                labels: vec![],
                file_size_bytes: csv_size,
                avg_snr_db: None, // no sidecar available
            },
            ts,
            end_ts,
        ));
    }

    patch_session_timestamps(&mut raw);

    // Hydrate labels
    if let Some(store) = label_store {
        for (session, start, end) in raw.iter_mut() {
            if let (Some(s), Some(e)) = (start, end) {
                session.labels = store.query_range(*s, *e);
            }
        }
    }

    // Backfill avg_snr_db from SQLite for sessions that don't have it in the
    // sidecar (legacy sessions recorded before SNR tracking was added).
    // This is a cheap AVG() query — no full table scan of raw data.
    backfill_avg_snr(&day_dir, &mut raw);

    let mut sessions: Vec<SessionEntry> = raw.into_iter().map(|(s, _, _)| s).collect();
    sessions.sort_by(|a, b| b.session_start_utc.cmp(&a.session_start_utc));
    sessions
}

/// Compute average SNR (dB) from the embeddings SQLite for sessions that
/// lack `avg_snr_db` in their sidecar.  Only issues queries when there are
/// sessions to backfill, and only one connection per day directory.
fn backfill_avg_snr(day_dir: &Path, sessions: &mut [(SessionEntry, Option<u64>, Option<u64>)]) {
    // Any work to do?
    let needs_backfill = sessions.iter().any(|(s, ..)| s.avg_snr_db.is_none());
    if !needs_backfill {
        return;
    }

    let db_path = day_dir.join(skill_constants::SQLITE_FILE);
    if !db_path.exists() {
        return;
    }

    // Try read-only first (fast, no write-locking).  If the metrics_json
    // column doesn't exist (very old database), the prepare will fail.
    // In that case, run the migration via a read-write connection and
    // return early (all values will be NULL after migration anyway).
    let flags_ro = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, flags_ro) else {
        return;
    };
    let _ = conn.execute_batch("PRAGMA busy_timeout=1000;");

    let query = "SELECT AVG(json_extract(metrics_json, '$.snr'))
         FROM embeddings
         WHERE timestamp >= ?1 AND timestamp <= ?2
           AND json_extract(metrics_json, '$.snr') IS NOT NULL";

    // If prepare fails the column is missing — migrate and bail.
    let needs_migration = conn.prepare(query).is_err();
    if needs_migration {
        drop(conn);
        let flags_rw = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
        if let Ok(rw) = rusqlite::Connection::open_with_flags(&db_path, flags_rw) {
            let _ = rw.execute("ALTER TABLE embeddings ADD COLUMN metrics_json TEXT", []);
        }
        return;
    }

    let Ok(mut stmt) = conn.prepare(query) else {
        return;
    };

    for (session, start, end) in sessions.iter_mut() {
        if session.avg_snr_db.is_some() {
            continue;
        }
        let (Some(s), Some(e)) = (*start, *end) else {
            continue;
        };
        let ts_start = skill_data::util::unix_to_ts(s);
        let ts_end = skill_data::util::unix_to_ts(e);
        if let Ok(avg) = stmt.query_row(rusqlite::params![ts_start, ts_end], |row| row.get::<_, Option<f64>>(0)) {
            session.avg_snr_db = avg;
        }
    }
}

/// Delete a session's CSV + JSON sidecar + metrics + IMU cache files.
pub fn delete_session(csv_path: &str) -> anyhow::Result<()> {
    let csv = std::path::PathBuf::from(csv_path);
    let json = csv.with_extension("json");
    let ppg = ppg_csv_path(&csv);
    let met = metrics_csv_path(&csv);
    let imu = skill_data::session_csv::imu_csv_path(&csv);
    let stem = csv.file_stem().and_then(|s| s.to_str()).unwrap_or("muse");
    let cache = csv.with_file_name(format!("{stem}_metrics_cache.json"));
    if csv.exists() {
        std::fs::remove_file(&csv)?;
    }
    if json.exists() {
        std::fs::remove_file(&json)?;
    }
    if ppg.exists() {
        std::fs::remove_file(&ppg)?;
    }
    if met.exists() {
        std::fs::remove_file(&met)?;
    }
    if imu.exists() {
        std::fs::remove_file(&imu)?;
    }
    if cache.exists() {
        let _ = std::fs::remove_file(&cache);
    }
    // Also try to delete Parquet variants.
    for suffix in ["", "_ppg", "_metrics", "_imu"] {
        let pq = csv.with_file_name(format!("{stem}{suffix}.parquet"));
        if pq.exists() {
            let _ = std::fs::remove_file(&pq);
        }
    }
    Ok(())
}

/// Aggregate history stats — total sessions/hours and week-over-week breakdown.
pub fn get_history_stats(skill_dir: &Path) -> HistoryStats {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days_since_epoch = now_secs / 86400;
    let weekday = (days_since_epoch + 3) % 7;
    let this_week_start = (days_since_epoch - weekday) * 86400;
    let last_week_start = this_week_start.saturating_sub(7 * 86400);

    let mut total_sessions = 0usize;
    let mut total_secs = 0u64;
    let mut this_week_secs = 0u64;
    let mut last_week_secs = 0u64;

    let day_dirs = std::fs::read_dir(skill_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let n = e.file_name();
            let s = n.to_string_lossy();
            s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) && e.path().is_dir()
        });

    for day_entry in day_dirs {
        let json_files = std::fs::read_dir(day_entry.path())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                let n = e.file_name();
                let s = n.to_string_lossy();
                is_session_json(&s)
            });
        for jf in json_files {
            let Ok(text) = std::fs::read_to_string(jf.path()) else {
                continue;
            };
            let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let Some(start) = meta["session_start_utc"].as_u64() else {
                continue;
            };
            let end = meta["session_end_utc"].as_u64().unwrap_or(start);
            let dur = end.saturating_sub(start);
            total_sessions += 1;
            total_secs += dur;
            if start >= this_week_start {
                this_week_secs += dur;
            } else if start >= last_week_start {
                last_week_secs += dur;
            }
        }
    }
    HistoryStats {
        total_sessions,
        total_secs,
        this_week_secs,
        last_week_secs,
    }
}

/// Find a session CSV path that contains or is nearest to a given timestamp.
pub fn find_session_csv_for_timestamp(skill_dir: &Path, ts_utc: u64) -> Option<String> {
    let mut containing: Option<String> = None;
    let mut nearest: Option<(u64, String)> = None;

    let entries = std::fs::read_dir(skill_dir).ok()?;
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&path) else {
            continue;
        };
        for file in files.filter_map(std::result::Result::ok) {
            let jp = file.path();
            let fname = jp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !is_session_json(fname) {
                continue;
            }
            let Ok(json) = std::fs::read_to_string(&jp) else {
                continue;
            };
            let Ok(meta) = serde_json::from_str::<serde_json::Value>(&json) else {
                continue;
            };
            let start = meta["session_start_utc"].as_u64();
            let end = meta["session_end_utc"].as_u64().or(start);
            let csv_file = meta["csv_file"].as_str().unwrap_or("");
            if csv_file.is_empty() {
                continue;
            }
            let csv_path = path.join(csv_file).to_string_lossy().into_owned();
            if let (Some(s), Some(e)) = (start, end) {
                if ts_utc >= s && ts_utc <= e {
                    containing = Some(csv_path);
                    break;
                }
                let dist = if ts_utc < s {
                    s - ts_utc
                } else {
                    ts_utc.saturating_sub(e)
                };
                match &nearest {
                    Some((best, _)) if *best <= dist => {}
                    _ => nearest = Some((dist, csv_path)),
                }
            }
        }
        if containing.is_some() {
            break;
        }
    }
    containing.or_else(|| nearest.map(|(_, p)| p))
}

/// Scan embedding databases and return distinct recording sessions.
pub fn list_embedding_sessions(skill_dir: &Path) -> Vec<EmbeddingSession> {
    const GAP_SECS: u64 = skill_constants::SESSION_GAP_SECS;

    let mut all_ts: Vec<(u64, usize)> = Vec::new();
    let mut day_names: Vec<String> = Vec::new();

    let Ok(entries) = std::fs::read_dir(skill_dir) else {
        return vec![];
    };
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let day_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        if day_name.len() != 8 || !day_name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let db_path = path.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }
        let Ok(conn) = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) else {
            continue;
        };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        let Ok(mut stmt) = conn.prepare("SELECT timestamp FROM embeddings ORDER BY timestamp") else {
            continue;
        };
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0));
        let day_idx = day_names.len();
        day_names.push(day_name);
        if let Ok(rows) = rows {
            for row in rows.filter_map(std::result::Result::ok) {
                all_ts.push((ts_to_unix(row), day_idx));
            }
        }
    }

    if all_ts.is_empty() {
        return vec![];
    }
    all_ts.sort_by_key(|(ts, _)| *ts);

    let mut sessions: Vec<EmbeddingSession> = Vec::new();
    let mut start = all_ts[0].0;
    let mut end = start;
    let mut count: u64 = 1;
    let mut day_idx = all_ts[0].1;

    for &(ts, di) in &all_ts[1..] {
        if ts.saturating_sub(end) > GAP_SECS {
            sessions.push(EmbeddingSession {
                start_utc: start,
                end_utc: end,
                n_epochs: count,
                day: day_names[day_idx].clone(),
            });
            start = ts;
            end = ts;
            count = 1;
            day_idx = di;
        } else {
            end = ts;
            count += 1;
        }
    }
    sessions.push(EmbeddingSession {
        start_utc: start,
        end_utc: end,
        n_epochs: count,
        day: day_names[day_idx].clone(),
    });
    sessions.reverse();
    sessions
}

// ── CSV timestamp helpers ─────────────────────────────────────────────────────

/// Read the first and last timestamp from a metrics file (CSV or Parquet).
fn read_metrics_time_range(metrics_path: &Path) -> Option<(u64, u64)> {
    if metrics_path.extension().and_then(|e| e.to_str()) == Some("parquet") {
        return read_metrics_parquet_time_range(metrics_path);
    }
    read_metrics_csv_time_range(metrics_path)
}

fn read_metrics_parquet_time_range(path: &Path) -> Option<(u64, u64)> {
    use arrow_array::Array;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let file = std::fs::File::open(path).ok()?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).ok()?;
    let reader = builder.build().ok()?;
    let mut first: Option<u64> = None;
    let mut last: Option<u64> = None;
    for batch in reader {
        let Ok(batch) = batch else { continue };
        let ts_col = batch.column(0).as_any().downcast_ref::<arrow_array::Float64Array>()?;
        for i in 0..ts_col.len() {
            if ts_col.is_null(i) {
                continue;
            }
            let t = ts_col.value(i);
            if t > 1_000_000_000.0 {
                let ts = t as u64;
                if first.is_none() {
                    first = Some(ts);
                }
                last = Some(ts);
            }
        }
    }
    Some((first?, last?))
}

fn read_metrics_csv_time_range(metrics_path: &Path) -> Option<(u64, u64)> {
    use std::io::{Read, Seek, SeekFrom};

    if !metrics_path.exists() {
        return None;
    }

    let mut file = std::fs::File::open(metrics_path).ok()?;
    let file_len = file.metadata().ok()?.len();
    if file_len == 0 {
        return None;
    }

    // Read the first ~4 KB to get the header + first data record.
    let head_size = (file_len as usize).min(4096);
    let mut head_buf = vec![0u8; head_size];
    file.read_exact(&mut head_buf).ok()?;
    let first = parse_first_ts_from_bytes(&head_buf);

    // Read the last ~4 KB to get the last data record.
    let tail_size = (file_len as usize).min(4096);
    let tail_offset = file_len.saturating_sub(tail_size as u64);
    file.seek(SeekFrom::Start(tail_offset)).ok()?;
    let mut tail_buf = vec![0u8; tail_size];
    let n = file.read(&mut tail_buf).ok()?;
    tail_buf.truncate(n);
    let last = parse_last_ts_from_bytes(&tail_buf);

    Some((first?, last?))
}

/// Parse the first valid timestamp from raw CSV bytes (skipping the header).
pub(crate) fn parse_first_ts_from_bytes(data: &[u8]) -> Option<u64> {
    let mut lines = data.split(|&b| b == b'\n');
    // Skip header.
    lines.next()?;
    for line in lines {
        if let Some(ts) = parse_ts_from_line(line) {
            return Some(ts);
        }
    }
    None
}

/// Parse the last valid timestamp from raw CSV bytes by scanning backwards.
pub(crate) fn parse_last_ts_from_bytes(data: &[u8]) -> Option<u64> {
    // Walk backwards through the last few lines (skip trailing newlines).
    let mut end = data.len();
    while end > 0 && data[end - 1] == b'\n' {
        end -= 1;
    }
    // Try up to 5 lines from the end to find a valid timestamp.
    for _ in 0..5 {
        if end == 0 {
            break;
        }
        let line_start = data[..end]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let line = &data[line_start..end];
        if let Some(ts) = parse_ts_from_line(line) {
            return Some(ts);
        }
        end = if line_start > 0 { line_start - 1 } else { 0 };
    }
    None
}

/// Extract the first CSV field from a raw line and parse it as a Unix timestamp.
pub(crate) fn parse_ts_from_line(line: &[u8]) -> Option<u64> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.is_empty() {
        return None;
    }
    let field_end = line.iter().position(|&b| b == b',').unwrap_or(line.len());
    let field = std::str::from_utf8(&line[..field_end]).ok()?;
    let t: f64 = field.parse().ok()?;
    if t > 1_000_000_000.0 {
        Some(t as u64)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_session_json_accepts_valid() {
        assert!(is_session_json("exg_1700000000.json"));
        assert!(is_session_json("muse_1700000000.json"));
    }

    #[test]
    fn is_session_json_rejects_invalid() {
        assert!(!is_session_json("notes.json"));
        assert!(!is_session_json("exg_1700000000.csv"));
        assert!(!is_session_json("config.json"));
        assert!(!is_session_json("random_file.json"));
    }

    #[test]
    fn is_session_data_accepts_csv_and_parquet() {
        assert!(is_session_data("exg_1700000000.csv"));
        assert!(is_session_data("exg_1700000000.parquet"));
        assert!(is_session_data("muse_1700000000.csv"));
    }

    #[test]
    fn is_session_data_rejects_metrics_and_ppg() {
        assert!(!is_session_data("exg_1700000000_metrics.csv"));
        assert!(!is_session_data("exg_1700000000_ppg.csv"));
        assert!(!is_session_data("exg_1700000000_imu.csv"));
        assert!(!is_session_data("exg_1700000000_metrics.parquet"));
        assert!(!is_session_data("exg_1700000000_ppg.parquet"));
        assert!(!is_session_data("exg_1700000000_imu.parquet"));
    }

    #[test]
    fn extract_timestamp_valid() {
        assert_eq!(extract_timestamp("exg_1700000000.csv"), Some(1700000000));
        assert_eq!(extract_timestamp("muse_1700000000.json"), Some(1700000000));
        assert_eq!(extract_timestamp("exg_1700000000.parquet"), Some(1700000000));
    }

    #[test]
    fn extract_timestamp_invalid() {
        assert_eq!(extract_timestamp("notes.csv"), None);
        assert_eq!(extract_timestamp("exg_abc.csv"), None);
    }

    #[test]
    fn parse_ts_from_line_valid() {
        assert_eq!(parse_ts_from_line(b"1700000000.123,0.5,0.6"), Some(1700000000));
    }

    #[test]
    fn parse_ts_from_line_small_rejected() {
        assert_eq!(parse_ts_from_line(b"999999999,0.1"), None);
    }

    #[test]
    fn parse_ts_from_line_empty() {
        assert_eq!(parse_ts_from_line(b""), None);
    }

    #[test]
    fn parse_ts_from_line_header() {
        assert_eq!(parse_ts_from_line(b"timestamp,ch0"), None);
    }

    #[test]
    fn parse_ts_from_line_crlf() {
        assert_eq!(parse_ts_from_line(b"1700000000.0,0.1\r"), Some(1700000000));
    }

    #[test]
    fn parse_first_ts_skips_header() {
        let data = b"timestamp,ch0\n1700000000.0,0.1\n1700000001.0,0.2\n";
        assert_eq!(parse_first_ts_from_bytes(data), Some(1700000000));
    }

    #[test]
    fn parse_last_ts_finds_last() {
        let data = b"t,ch0\n1700000000.0,0.1\n1700000100.5,0.2\n";
        assert_eq!(parse_last_ts_from_bytes(data), Some(1700000100));
    }

    #[test]
    fn parse_last_ts_trailing_newlines() {
        let data = b"t,ch0\n1700000000.0,0.1\n1700000999.0,0.2\n\n\n";
        assert_eq!(parse_last_ts_from_bytes(data), Some(1700000999));
    }
}

fn patch_session_timestamps(raw: &mut [(SessionEntry, Option<u64>, Option<u64>)]) {
    for (session, start, end) in raw.iter_mut() {
        // Skip sessions that already have valid timestamps from the JSON sidecar.
        if start.is_some() && end.is_some() {
            continue;
        }
        let mp = find_metrics_path(Path::new(&session.csv_path));
        let Some(mp) = mp else { continue };
        if let Some((first_ts, last_ts)) = read_metrics_time_range(&mp) {
            *start = Some(first_ts);
            *end = Some(last_ts);
            session.session_start_utc = Some(first_ts);
            session.session_end_utc = Some(last_ts);
            session.session_duration_s = Some(last_ts.saturating_sub(first_ts));
        }
    }
}
