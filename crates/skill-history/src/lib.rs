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

pub mod cache;

// Re-export types consumed by Tauri wrappers.
pub use skill_data::label_store;
pub use cache::*;

// ── Session file helpers ──────────────────────────────────────────────────────

/// Canonical prefix for new session files.
const SESSION_PREFIX: &str = "exg_";
/// Legacy prefix kept for backward compatibility.
const LEGACY_PREFIX: &str = "muse_";

/// Returns `true` if `fname` is a session JSON sidecar (exg_*.json or muse_*.json).
fn is_session_json(fname: &str) -> bool {
    (fname.starts_with(SESSION_PREFIX) || fname.starts_with(LEGACY_PREFIX))
        && fname.ends_with(".json")
}

/// Returns `true` if `fname` is a primary EEG data file (CSV or Parquet, not metrics/ppg).
fn is_session_data(fname: &str) -> bool {
    let has_prefix = fname.starts_with(SESSION_PREFIX) || fname.starts_with(LEGACY_PREFIX);
    if !has_prefix { return false; }
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
fn is_session_csv(fname: &str) -> bool { is_session_data(fname) }

/// Extract the Unix timestamp from a session filename like `exg_1700000000.csv`,
/// `exg_1700000000.parquet`, or `muse_1700000000.json`.
fn extract_timestamp(fname: &str) -> Option<u64> {
    fname.rsplit_once('_')
        .and_then(|(_, ts_part)| {
            ts_part.strip_suffix(".csv")
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
    if pq.exists() { return Some(pq); }
    let csv = metrics_csv_path(eeg_path);
    if csv.exists() { return Some(csv); }
    None
}

/// Given a base EEG path, find the corresponding PPG data file.
pub fn find_ppg_path(eeg_path: &Path) -> Option<std::path::PathBuf> {
    use skill_data::session_parquet::ppg_parquet_path;
    let pq = ppg_parquet_path(eeg_path);
    if pq.exists() { return Some(pq); }
    let csv = ppg_csv_path(eeg_path);
    if csv.exists() { return Some(csv); }
    None
}

// ── SessionEntry ──────────────────────────────────────────────────────────────

/// A session entry read from a JSON sidecar file.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionEntry {
    pub csv_file:          String,
    pub csv_path:          String,
    pub session_start_utc: Option<u64>,
    pub session_end_utc:   Option<u64>,
    pub session_duration_s: Option<u64>,
    pub device_name:       Option<String>,
    pub device_id:         Option<String>,
    pub serial_number:     Option<String>,
    pub mac_address:       Option<String>,
    pub firmware_version:  Option<String>,
    pub hardware_version:  Option<String>,
    pub headset_preset:    Option<String>,
    pub battery_pct:       Option<f64>,
    pub total_samples:     Option<u64>,
    pub sample_rate_hz:    Option<u64>,
    pub labels:            Vec<LabelRow>,
    pub file_size_bytes:   u64,
    /// Average signal-to-noise ratio (dB) for the session.
    /// `None` for legacy sessions recorded before SNR tracking.
    pub avg_snr_db:        Option<f64>,
}

// ── Typed session JSON sidecar (replaces serde_json::Value for speed) ─────────

/// Lightweight typed representation of the session JSON sidecar.
///
/// Using a typed struct avoids the cost of `serde_json::Value` which internally
/// uses `BTreeMap<String, Value>` for JSON objects — expensive to build and
/// drop for every session file.  Only the fields needed by `list_sessions_for_day`
/// are included; unknown fields are silently ignored via `deny_unknown_fields = false`.
#[derive(Deserialize, Default)]
struct SessionJsonMeta {
    #[serde(default)]
    csv_file: Option<String>,
    #[serde(default)]
    session_start_utc: Option<u64>,
    #[serde(default)]
    session_end_utc: Option<u64>,
    #[serde(default)]
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
    #[serde(default)]
    battery_pct_end: Option<f64>,
    #[serde(default)]
    battery_pct: Option<f64>,
    /// Average SNR (dB) across the session — written by the session runner.
    #[serde(default)]
    avg_snr_db: Option<f64>,
    #[serde(default)]
    total_samples: Option<u64>,
    #[serde(default)]
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

// ── SessionMetrics ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SessionMetrics {
    pub n_epochs:         usize,
    pub rel_delta:        f64,
    pub rel_theta:        f64,
    pub rel_alpha:        f64,
    pub rel_beta:         f64,
    pub rel_gamma:        f64,
    pub rel_high_gamma:   f64,
    pub relaxation:       f64,
    pub engagement:       f64,
    pub faa:              f64,
    pub tar:              f64,
    pub bar:              f64,
    pub dtr:              f64,
    pub pse:              f64,
    pub apf:              f64,
    pub bps:              f64,
    pub snr:              f64,
    pub coherence:        f64,
    pub mu_suppression:   f64,
    pub mood:             f64,
    pub tbr:              f64,
    pub sef95:            f64,
    pub spectral_centroid: f64,
    pub hjorth_activity:  f64,
    pub hjorth_mobility:  f64,
    pub hjorth_complexity: f64,
    pub permutation_entropy: f64,
    pub higuchi_fd:       f64,
    pub dfa_exponent:     f64,
    pub sample_entropy:   f64,
    pub pac_theta_gamma:  f64,
    pub laterality_index: f64,
    pub hr:               f64,
    pub rmssd:            f64,
    pub sdnn:             f64,
    pub pnn50:            f64,
    pub lf_hf_ratio:      f64,
    pub respiratory_rate: f64,
    pub spo2_estimate:    f64,
    pub perfusion_index:  f64,
    pub stress_index:     f64,
    pub blink_count:      f64,
    pub blink_rate:       f64,
    pub head_pitch:       f64,
    pub head_roll:        f64,
    pub stillness:        f64,
    pub nod_count:        f64,
    pub shake_count:      f64,
    pub meditation:       f64,
    pub cognitive_load:   f64,
    pub drowsiness:       f64,
}

// ── EpochRow ──────────────────────────────────────────────────────────────────

/// A single epoch's metrics, returned as part of a time-series query.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EpochRow {
    pub t: f64,
    pub rd: f64, pub rt: f64, pub ra: f64, pub rb: f64, pub rg: f64,
    pub relaxation: f64, pub engagement: f64,
    pub faa: f64,
    pub tar: f64, pub bar: f64, pub dtr: f64, pub tbr: f64,
    pub pse: f64, pub apf: f64, pub sef95: f64, pub sc: f64, pub bps: f64, pub snr: f64,
    pub coherence: f64, pub mu: f64,
    pub ha: f64, pub hm: f64, pub hc: f64,
    pub pe: f64, pub hfd: f64, pub dfa: f64, pub se: f64, pub pac: f64, pub lat: f64,
    pub mood: f64,
    pub hr: f64, pub rmssd: f64, pub sdnn: f64, pub pnn50: f64, pub lf_hf: f64,
    pub resp: f64, pub spo2: f64, pub perf: f64, pub stress: f64,
    pub blinks: f64, pub blink_r: f64,
    pub pitch: f64, pub roll: f64, pub still: f64, pub nods: f64, pub shakes: f64,
    pub med: f64, pub cog: f64, pub drow: f64,
    pub gpu: f64, pub gpu_render: f64, pub gpu_tiler: f64,
}

// ── CsvMetricsResult ──────────────────────────────────────────────────────────

/// Combined summary + time-series data loaded directly from `_metrics.csv`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CsvMetricsResult {
    pub n_rows: usize,
    pub summary: SessionMetrics,
    pub timeseries: Vec<EpochRow>,
}

// ── Sleep types ───────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SleepEpoch {
    pub utc: u64,
    pub stage: u8,
    pub rel_delta: f64,
    pub rel_theta: f64,
    pub rel_alpha: f64,
    pub rel_beta:  f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SleepSummary {
    pub total_epochs:  usize,
    pub wake_epochs:   usize,
    pub n1_epochs:     usize,
    pub n2_epochs:     usize,
    pub n3_epochs:     usize,
    pub rem_epochs:    usize,
    pub epoch_secs:    f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SleepStages {
    pub epochs:  Vec<SleepEpoch>,
    pub summary: SleepSummary,
}

// ── History stats ─────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryStats {
    pub total_sessions: usize,
    pub total_secs:     u64,
    pub this_week_secs: u64,
    pub last_week_secs: u64,
}

// ── EmbeddingSession ──────────────────────────────────────────────────────────

/// One contiguous recording range discovered from embedding timestamps.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EmbeddingSession {
    pub start_utc: u64,
    pub end_utc:   u64,
    pub n_epochs:  u64,
    pub day:       String,
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
            let has_sessions = std::fs::read_dir(e.path())
                .into_iter()
                .flatten()
                .flatten()
                .any(|f| {
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
            if has_sessions { Some(s.to_string()) } else { None }
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
    if !day_dir.is_dir() { return vec![]; }

    let files: Vec<_> = std::fs::read_dir(&day_dir)
        .into_iter().flatten().flatten().collect();
    let mut raw: Vec<(SessionEntry, Option<u64>, Option<u64>)> = Vec::new();

    // First pass: JSON sidecars
    for jf in &files {
        let jp = jf.path();
        let fname = jp.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !is_session_json(fname) { continue; }

        let Ok(json_bytes) = std::fs::read(&jp) else { continue };
        let Ok(meta) = serde_json::from_slice::<SessionJsonMeta>(&json_bytes) else { continue };

        let csv_file = meta.csv_file.unwrap_or_default();
        let csv_full = day_dir.join(&csv_file);
        // Check for both CSV and Parquet data files.
        let pq_full = csv_full.with_extension("parquet");
        let csv_size = if pq_full.exists() {
            std::fs::metadata(&pq_full).map(|m| m.len()).unwrap_or(0)
        } else {
            std::fs::metadata(&csv_full).map(|m| m.len()).unwrap_or(0)
        };
        let start = meta.session_start_utc;
        let end   = meta.session_end_utc;
        let dev   = &meta.device;
        let str_field = |dev_field: &Option<String>, fallback: &Option<String>| -> Option<String> {
            dev_field.clone().or_else(|| fallback.clone())
        };
        raw.push((SessionEntry {
            csv_file,
            csv_path:           csv_full.to_string_lossy().into_owned(),
            session_start_utc:  start,
            session_end_utc:    end,
            session_duration_s: meta.session_duration_s
                                    .or_else(|| start.zip(end).map(|(s, e)| e.saturating_sub(s))),
            device_name:        str_field(&dev.name, &meta.device_name),
            device_id:          str_field(&dev.id, &meta.device_id),
            serial_number:      str_field(&dev.serial_number, &meta.serial_number),
            mac_address:        str_field(&dev.mac_address, &meta.mac_address),
            firmware_version:   str_field(&dev.firmware_version, &meta.firmware_version),
            hardware_version:   str_field(&dev.hardware_version, &meta.hardware_version),
            headset_preset:     str_field(&dev.preset, &meta.headset_preset),
            battery_pct:        meta.battery_pct_end.or(meta.battery_pct),
            total_samples:      meta.total_samples,
            sample_rate_hz:     meta.sample_rate_hz,
            labels:             vec![],
            file_size_bytes:    csv_size,
            avg_snr_db:         meta.avg_snr_db,
        }, start, end));
    }

    // Second pass: orphaned CSVs
    for cf in &files {
        let cp = cf.path();
        let cfname = cp.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !is_session_csv(cfname) { continue; }
        if cp.with_extension("json").exists() { continue; }
        let meta_fs = std::fs::metadata(&cp);
        let csv_size = meta_fs.as_ref().map(std::fs::Metadata::len).unwrap_or(0);
        let ts: Option<u64> = extract_timestamp(cfname);
        let end_ts: Option<u64> = meta_fs.ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        raw.push((SessionEntry {
            csv_file:           cfname.to_string(),
            csv_path:           cp.to_string_lossy().into_owned(),
            session_start_utc:  ts,
            session_end_utc:    end_ts,
            session_duration_s: ts.zip(end_ts).map(|(s, e)| e.saturating_sub(s)),
            device_name:        None, device_id: None, serial_number: None,
            mac_address:        None, firmware_version: None, hardware_version: None,
            headset_preset:     None,
            battery_pct:        None,
            total_samples:      None,
            sample_rate_hz:     None, // unknown — no JSON metadata available
            labels:             vec![],
            file_size_bytes:    csv_size,
            avg_snr_db:         None, // no sidecar available
        }, ts, end_ts));
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
fn backfill_avg_snr(
    day_dir: &Path,
    sessions: &mut [(SessionEntry, Option<u64>, Option<u64>)],
) {
    // Any work to do?
    let needs_backfill = sessions.iter().any(|(s, ..)| s.avg_snr_db.is_none());
    if !needs_backfill { return; }

    let db_path = day_dir.join(skill_constants::SQLITE_FILE);
    if !db_path.exists() { return; }

    // Try read-only first (fast, no write-locking).  If the metrics_json
    // column doesn't exist (very old database), the prepare will fail.
    // In that case, run the migration via a read-write connection and
    // return early (all values will be NULL after migration anyway).
    let flags_ro = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, flags_ro) else { return };
    let _ = conn.execute_batch("PRAGMA busy_timeout=1000;");

    let query = "SELECT AVG(json_extract(metrics_json, '$.snr'))
         FROM embeddings
         WHERE timestamp >= ?1 AND timestamp <= ?2
           AND json_extract(metrics_json, '$.snr') IS NOT NULL";

    // If prepare fails the column is missing — migrate and bail.
    let needs_migration = conn.prepare(query).is_err();
    if needs_migration {
        drop(conn);
        let flags_rw = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
        if let Ok(rw) = rusqlite::Connection::open_with_flags(&db_path, flags_rw) {
            let _ = rw.execute("ALTER TABLE embeddings ADD COLUMN metrics_json TEXT", []);
        }
        return;
    }

    let Ok(mut stmt) = conn.prepare(query) else { return };

    for (session, start, end) in sessions.iter_mut() {
        if session.avg_snr_db.is_some() { continue; }
        let (Some(s), Some(e)) = (*start, *end) else { continue; };
        let ts_start = skill_data::util::unix_to_ts(s);
        let ts_end   = skill_data::util::unix_to_ts(e);
        if let Ok(avg) = stmt.query_row(
            rusqlite::params![ts_start, ts_end],
            |row| row.get::<_, Option<f64>>(0),
        ) {
            session.avg_snr_db = avg;
        }
    }
}

/// Delete a session's CSV + JSON sidecar + metrics + IMU cache files.
pub fn delete_session(csv_path: &str) -> anyhow::Result<()> {
    let csv = std::path::PathBuf::from(csv_path);
    let json = csv.with_extension("json");
    let ppg  = ppg_csv_path(&csv);
    let met  = metrics_csv_path(&csv);
    let imu  = skill_data::session_csv::imu_csv_path(&csv);
    let stem = csv.file_stem().and_then(|s| s.to_str()).unwrap_or("muse");
    let cache = csv.with_file_name(format!("{stem}_metrics_cache.json"));
    if csv.exists()   { std::fs::remove_file(&csv)?; }
    if json.exists()  { std::fs::remove_file(&json)?; }
    if ppg.exists()   { std::fs::remove_file(&ppg)?; }
    if met.exists()   { std::fs::remove_file(&met)?; }
    if imu.exists()   { std::fs::remove_file(&imu)?; }
    if cache.exists() { let _ = std::fs::remove_file(&cache); }
    // Also try to delete Parquet variants.
    for suffix in ["", "_ppg", "_metrics", "_imu"] {
        let pq = csv.with_file_name(format!("{stem}{suffix}.parquet"));
        if pq.exists() { let _ = std::fs::remove_file(&pq); }
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
    let weekday           = (days_since_epoch + 3) % 7;
    let this_week_start   = (days_since_epoch - weekday) * 86400;
    let last_week_start   = this_week_start.saturating_sub(7 * 86400);

    let mut total_sessions = 0usize;
    let mut total_secs     = 0u64;
    let mut this_week_secs = 0u64;
    let mut last_week_secs = 0u64;

    let day_dirs = std::fs::read_dir(skill_dir)
        .into_iter().flatten().flatten()
        .filter(|e| {
            let n = e.file_name(); let s = n.to_string_lossy();
            s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) && e.path().is_dir()
        });

    for day_entry in day_dirs {
        let json_files = std::fs::read_dir(day_entry.path())
            .into_iter().flatten().flatten()
            .filter(|e| {
                let n = e.file_name(); let s = n.to_string_lossy();
                is_session_json(&s)
            });
        for jf in json_files {
            let Ok(text) = std::fs::read_to_string(jf.path()) else { continue };
            let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let Some(start) = meta["session_start_utc"].as_u64() else { continue };
            let end = meta["session_end_utc"].as_u64().unwrap_or(start);
            let dur = end.saturating_sub(start);
            total_sessions += 1;
            total_secs     += dur;
            if start >= this_week_start       { this_week_secs += dur; }
            else if start >= last_week_start  { last_week_secs += dur; }
        }
    }
    HistoryStats { total_sessions, total_secs, this_week_secs, last_week_secs }
}

/// Find a session CSV path that contains or is nearest to a given timestamp.
pub fn find_session_csv_for_timestamp(skill_dir: &Path, ts_utc: u64) -> Option<String> {
    let mut containing: Option<String> = None;
    let mut nearest: Option<(u64, String)> = None;

    let entries = std::fs::read_dir(skill_dir).ok()?;
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let Ok(files) = std::fs::read_dir(&path) else { continue };
        for file in files.filter_map(std::result::Result::ok) {
            let jp = file.path();
            let fname = jp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !is_session_json(fname) { continue; }
            let Ok(json) = std::fs::read_to_string(&jp) else { continue };
            let Ok(meta) = serde_json::from_str::<serde_json::Value>(&json) else { continue };
            let start = meta["session_start_utc"].as_u64();
            let end = meta["session_end_utc"].as_u64().or(start);
            let csv_file = meta["csv_file"].as_str().unwrap_or("");
            if csv_file.is_empty() { continue; }
            let csv_path = path.join(csv_file).to_string_lossy().into_owned();
            if let (Some(s), Some(e)) = (start, end) {
                if ts_utc >= s && ts_utc <= e { containing = Some(csv_path); break; }
                let dist = if ts_utc < s { s - ts_utc } else { ts_utc.saturating_sub(e) };
                match &nearest {
                    Some((best, _)) if *best <= dist => {}
                    _ => nearest = Some((dist, csv_path)),
                }
            }
        }
        if containing.is_some() { break; }
    }
    containing.or_else(|| nearest.map(|(_, p)| p))
}

/// Scan embedding databases and return distinct recording sessions.
pub fn list_embedding_sessions(skill_dir: &Path) -> Vec<EmbeddingSession> {
    const GAP_SECS: u64 = skill_constants::SESSION_GAP_SECS;

    let mut all_ts: Vec<(u64, usize)> = Vec::new();
    let mut day_names: Vec<String> = Vec::new();

    let Ok(entries) = std::fs::read_dir(skill_dir) else { return vec![] };
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let day_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        if day_name.len() != 8 || !day_name.bytes().all(|b| b.is_ascii_digit()) { continue; }
        let db_path = path.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() { continue; }
        let Ok(conn) = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) else { continue };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        let Ok(mut stmt) = conn.prepare("SELECT timestamp FROM embeddings ORDER BY timestamp") else { continue };
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0));
        let day_idx = day_names.len();
        day_names.push(day_name);
        if let Ok(rows) = rows {
            for row in rows.filter_map(std::result::Result::ok) {
                all_ts.push((ts_to_unix(row), day_idx));
            }
        }
    }

    if all_ts.is_empty() { return vec![]; }
    all_ts.sort_by_key(|(ts, _)| *ts);

    let mut sessions: Vec<EmbeddingSession> = Vec::new();
    let mut start = all_ts[0].0;
    let mut end   = start;
    let mut count: u64 = 1;
    let mut day_idx = all_ts[0].1;

    for &(ts, di) in &all_ts[1..] {
        if ts.saturating_sub(end) > GAP_SECS {
            sessions.push(EmbeddingSession { start_utc: start, end_utc: end, n_epochs: count, day: day_names[day_idx].clone() });
            start = ts; end = ts; count = 1; day_idx = di;
        } else { end = ts; count += 1; }
    }
    sessions.push(EmbeddingSession { start_utc: start, end_utc: end, n_epochs: count, day: day_names[day_idx].clone() });
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
        let ts_col = batch.column(0)
            .as_any().downcast_ref::<arrow_array::Float64Array>()?;
        for i in 0..ts_col.len() {
            if ts_col.is_null(i) { continue; }
            let t = ts_col.value(i);
            if t > 1_000_000_000.0 {
                let ts = t as u64;
                if first.is_none() { first = Some(ts); }
                last = Some(ts);
            }
        }
    }
    Some((first?, last?))
}

fn read_metrics_csv_time_range(metrics_path: &Path) -> Option<(u64, u64)> {
    use std::io::{Read, Seek, SeekFrom};

    if !metrics_path.exists() { return None; }

    let mut file = std::fs::File::open(metrics_path).ok()?;
    let file_len = file.metadata().ok()?.len();
    if file_len == 0 { return None; }

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
fn parse_first_ts_from_bytes(data: &[u8]) -> Option<u64> {
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
fn parse_last_ts_from_bytes(data: &[u8]) -> Option<u64> {
    // Walk backwards through the last few lines (skip trailing newlines).
    let mut end = data.len();
    while end > 0 && data[end - 1] == b'\n' {
        end -= 1;
    }
    // Try up to 5 lines from the end to find a valid timestamp.
    for _ in 0..5 {
        if end == 0 { break; }
        let line_start = data[..end].iter().rposition(|&b| b == b'\n')
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
fn parse_ts_from_line(line: &[u8]) -> Option<u64> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.is_empty() { return None; }
    let field_end = line.iter().position(|&b| b == b',').unwrap_or(line.len());
    let field = std::str::from_utf8(&line[..field_end]).ok()?;
    let t: f64 = field.parse().ok()?;
    if t > 1_000_000_000.0 { Some(t as u64) } else { None }
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
            *start                     = Some(first_ts);
            *end                       = Some(last_ts);
            session.session_start_utc  = Some(first_ts);
            session.session_end_utc    = Some(last_ts);
            session.session_duration_s = Some(last_ts.saturating_sub(first_ts));
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Metrics & time-series (CSV-based)
// ═══════════════════════════════════════════════════════════════════════════════

/// Sigmoid mapping (0, ∞) → (0, 100) with tuneable steepness and midpoint.
fn sigmoid100(x: f32, k: f32, mid: f32) -> f32 {
    100.0 / (1.0 + (-k * (x - mid)).exp())
}

/// Read a `_metrics` file (CSV or Parquet) and return aggregated summary + time-series.
pub fn load_metrics_csv(csv_path: &Path) -> Option<CsvMetricsResult> {
    let Some(metrics_path) = find_metrics_path(csv_path) else {
        eprintln!("[metrics] no metrics file for: {}", csv_path.display());
        return None;
    };

    // Parquet path: convert to CSV-style records and process identically.
    if metrics_path.extension().and_then(|e| e.to_str()) == Some("parquet") {
        return load_metrics_from_parquet(&metrics_path);
    }

    if !metrics_path.exists() {
        return None;
    }

    let mut rdr = match csv::ReaderBuilder::new()
        .has_headers(true).flexible(true).from_path(&metrics_path)
    { Ok(r) => r, Err(e) => { eprintln!("[csv-metrics] open error: {e}"); return None; } };

    // Detect channel count from header: find the "faa" column to determine
    // where per-channel band powers end and cross-channel indices begin.
    let header = rdr.headers().ok()?.clone();
    let faa_idx = header.iter().position(|h| h == "faa").unwrap_or(49);
    let n_band_cols = faa_idx - 1; // columns 1..faa_idx are per-channel bands
    let n_ch = n_band_cols / 12;   // 12 band columns per channel
    let ch_bases: Vec<usize> = (0..n_ch).map(|c| 1 + c * 12).collect();
    let x = faa_idx; // cross-channel offset

    let mut rows: Vec<EpochRow> = Vec::new();
    let mut sum = SessionMetrics::default();
    let mut count = 0usize;

    for result in rdr.records() {
        let Ok(rec) = result else { continue };
        if rec.len() < x + 23 { continue; } // need at least through laterality_index

        let f = |i: usize| -> f64 {
            rec.get(i).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)
        };

        let timestamp = f(0);
        if timestamp <= 0.0 { continue; }

        let avg_rel = |band_offset: usize| -> f64 {
            if ch_bases.is_empty() { return 0.0; }
            let mut s = 0.0;
            for &base in &ch_bases { s += f(base + 6 + band_offset); }
            s / ch_bases.len() as f64
        };

        let rd = avg_rel(0);
        let rt = avg_rel(1);
        let ra = avg_rel(2);
        let rb = avg_rel(3);
        let rg = avg_rel(4);

        let faa_v =f(x);   let tar_v =f(x+1); let bar_v =f(x+2); let dtr_v =f(x+3);
        let pse_v =f(x+4); let apf_v =f(x+5); let bps_v =f(x+6); let snr_v =f(x+7);
        let coh_v =f(x+8); let mu_v  =f(x+9); let mood_v=f(x+10);
        let tbr_v =f(x+11);let sef_v =f(x+12);let sc_v  =f(x+13);
        let ha_v  =f(x+14);let hm_v  =f(x+15);let hc_v  =f(x+16);
        let pe_v  =f(x+17);let hfd_v =f(x+18);let dfa_v =f(x+19);
        let se_v  =f(x+20);let pac_v =f(x+21);let lat_v =f(x+22);
        let hr_v  =f(x+23);let rmssd_v=f(x+24);let sdnn_v=f(x+25);
        let pnn_v =f(x+26);let lfhf_v=f(x+27);let resp_v=f(x+28);
        let spo_v =f(x+29);let perf_v=f(x+30);let stress_v=f(x+31);
        let blinks_v=f(x+32);let blink_r_v=f(x+33);
        let pitch_v=f(x+34);let roll_v=f(x+35);let still_v=f(x+36);
        let nods_v =f(x+37);let shakes_v=f(x+38);
        let med_v =f(x+39);let cog_v =f(x+40);let drow_v=f(x+41);
        let gpu_v =f(x+43);let gpu_r_v=f(x+44);let gpu_t_v=f(x+45);

        let mut sr = 0.0f64; let mut se2 = 0.0f64;
        for &ch_base in &ch_bases {
            let a = f(ch_base + 6 + 2);
            let b = f(ch_base + 6 + 3);
            let t = f(ch_base + 6 + 1);
            let d1 = a + t;
            let d2 = b + t;
            if d1 > 1e-6 { se2 += b / d1; }
            if d2 > 1e-6 { sr += a / d2; }
        }
        let relax_v   = sigmoid100((sr / 4.0) as f32, 2.5, 1.0) as f64;
        let engage_v  = sigmoid100((se2 / 4.0) as f32, 2.0, 0.8) as f64;

        let row = EpochRow {
            t: timestamp,
            rd, rt, ra, rb, rg,
            relaxation: relax_v, engagement: engage_v,
            faa: faa_v,
            tar: tar_v, bar: bar_v, dtr: dtr_v, tbr: tbr_v,
            pse: pse_v, apf: apf_v, sef95: sef_v, sc: sc_v, bps: bps_v, snr: snr_v,
            coherence: coh_v, mu: mu_v,
            ha: ha_v, hm: hm_v, hc: hc_v,
            pe: pe_v, hfd: hfd_v, dfa: dfa_v, se: se_v, pac: pac_v, lat: lat_v,
            mood: mood_v,
            hr: hr_v, rmssd: rmssd_v, sdnn: sdnn_v, pnn50: pnn_v, lf_hf: lfhf_v,
            resp: resp_v, spo2: spo_v, perf: perf_v, stress: stress_v,
            blinks: blinks_v, blink_r: blink_r_v,
            pitch: pitch_v, roll: roll_v, still: still_v, nods: nods_v, shakes: shakes_v,
            med: med_v, cog: cog_v, drow: drow_v,
            gpu: gpu_v, gpu_render: gpu_r_v, gpu_tiler: gpu_t_v,
        };

        sum.rel_delta += rd;   sum.rel_theta += rt;   sum.rel_alpha += ra;
        sum.rel_beta  += rb;   sum.rel_gamma += rg;
        sum.relaxation += relax_v;  sum.engagement += engage_v;
        sum.faa += faa_v;      sum.tar += tar_v;      sum.bar += bar_v;
        sum.dtr += dtr_v;      sum.tbr += tbr_v;
        sum.pse += pse_v;      sum.apf += apf_v;      sum.bps += bps_v;
        sum.snr += snr_v;      sum.coherence += coh_v; sum.mu_suppression += mu_v;
        sum.mood += mood_v;    sum.sef95 += sef_v;     sum.spectral_centroid += sc_v;
        sum.hjorth_activity += ha_v; sum.hjorth_mobility += hm_v; sum.hjorth_complexity += hc_v;
        sum.permutation_entropy += pe_v; sum.higuchi_fd += hfd_v; sum.dfa_exponent += dfa_v;
        sum.sample_entropy += se_v; sum.pac_theta_gamma += pac_v; sum.laterality_index += lat_v;
        sum.hr += hr_v;        sum.rmssd += rmssd_v;   sum.sdnn += sdnn_v;
        sum.pnn50 += pnn_v;    sum.lf_hf_ratio += lfhf_v; sum.respiratory_rate += resp_v;
        sum.spo2_estimate += spo_v; sum.perfusion_index += perf_v; sum.stress_index += stress_v;
        sum.blink_count += blinks_v; sum.blink_rate += blink_r_v;
        sum.head_pitch += pitch_v; sum.head_roll += roll_v; sum.stillness += still_v;
        sum.nod_count += nods_v; sum.shake_count += shakes_v;
        sum.meditation += med_v; sum.cognitive_load += cog_v; sum.drowsiness += drow_v;

        rows.push(row);
        count += 1;
    }

    if count == 0 { return None; }

    let n = count as f64;
    sum.n_epochs = count;
    sum.rel_delta /= n;  sum.rel_theta /= n;  sum.rel_alpha /= n;
    sum.rel_beta  /= n;  sum.rel_gamma /= n;
    sum.relaxation /= n;  sum.engagement /= n;
    sum.faa /= n;        sum.tar /= n;         sum.bar /= n;
    sum.dtr /= n;        sum.tbr /= n;
    sum.pse /= n;        sum.apf /= n;         sum.bps /= n;
    sum.snr /= n;        sum.coherence /= n;   sum.mu_suppression /= n;
    sum.mood /= n;       sum.sef95 /= n;       sum.spectral_centroid /= n;
    sum.hjorth_activity /= n; sum.hjorth_mobility /= n; sum.hjorth_complexity /= n;
    sum.permutation_entropy /= n; sum.higuchi_fd /= n; sum.dfa_exponent /= n;
    sum.sample_entropy /= n; sum.pac_theta_gamma /= n; sum.laterality_index /= n;
    sum.hr /= n;         sum.rmssd /= n;        sum.sdnn /= n;
    sum.pnn50 /= n;      sum.lf_hf_ratio /= n;  sum.respiratory_rate /= n;
    sum.spo2_estimate /= n; sum.perfusion_index /= n; sum.stress_index /= n;
    sum.blink_rate /= n;
    sum.head_pitch /= n; sum.head_roll /= n;    sum.stillness /= n;
    sum.meditation /= n; sum.cognitive_load /= n; sum.drowsiness /= n;

    eprintln!("[csv-metrics] loaded {} rows from {}", count, metrics_path.display());
    Some(CsvMetricsResult { n_rows: count, summary: sum, timeseries: rows })
}

/// Read metrics from a Parquet file.  Converts each row batch into the same
/// index-based field access used by the CSV path, then delegates to the
/// shared aggregation logic.
fn load_metrics_from_parquet(path: &Path) -> Option<CsvMetricsResult> {
    use arrow_array::Array;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    let file = std::fs::File::open(path).ok()?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).ok()?;
    let schema = builder.schema().clone();
    let reader = builder.build().ok()?;

    // Detect cross-channel offset from schema (find "faa" column).
    let x = schema.fields().iter().position(|f| f.name() == "faa").unwrap_or(49);
    let n_band_cols = x - 1;
    let n_ch = n_band_cols / 12;
    let ch_bases: Vec<usize> = (0..n_ch).map(|c| 1 + c * 12).collect();

    let mut rows: Vec<EpochRow> = Vec::new();
    let mut sum = SessionMetrics::default();
    let mut count = 0usize;

    for batch in reader {
        let Ok(batch) = batch else { continue };
        let n_cols = batch.num_columns();
        let n_rows = batch.num_rows();

        let cols: Vec<Option<&arrow_array::Float64Array>> = (0..n_cols)
            .map(|i| batch.column(i).as_any().downcast_ref::<arrow_array::Float64Array>())
            .collect();

        for row_idx in 0..n_rows {
            let f = |i: usize| -> f64 {
                if i >= cols.len() { return 0.0; }
                cols[i].map_or(0.0, |c| if c.is_null(row_idx) { 0.0 } else { c.value(row_idx) })
            };

            if n_cols < x + 23 { continue; }
            let timestamp = f(0);
            if timestamp <= 0.0 { continue; }

            let avg_rel = |band_offset: usize| -> f64 {
                if ch_bases.is_empty() { return 0.0; }
                let mut s = 0.0;
                for &base in &ch_bases { s += f(base + 6 + band_offset); }
                s / ch_bases.len() as f64
            };

            let rd = avg_rel(0); let rt = avg_rel(1); let ra = avg_rel(2);
            let rb = avg_rel(3); let rg = avg_rel(4);

            let faa_v=f(x);   let tar_v=f(x+1); let bar_v=f(x+2); let dtr_v=f(x+3);
            let pse_v=f(x+4); let apf_v=f(x+5); let bps_v=f(x+6); let snr_v=f(x+7);
            let coh_v=f(x+8); let mu_v=f(x+9);  let mood_v=f(x+10);
            let tbr_v=f(x+11);let sef_v=f(x+12);let sc_v=f(x+13);
            let ha_v=f(x+14); let hm_v=f(x+15); let hc_v=f(x+16);
            let pe_v=f(x+17); let hfd_v=f(x+18);let dfa_v=f(x+19);
            let se_v=f(x+20); let pac_v=f(x+21);let lat_v=f(x+22);
            let hr_v=f(x+23); let rmssd_v=f(x+24);let sdnn_v=f(x+25);
            let pnn_v=f(x+26);let lfhf_v=f(x+27);let resp_v=f(x+28);
            let spo_v=f(x+29);let perf_v=f(x+30);let stress_v=f(x+31);
            let blinks_v=f(x+32);let blink_r_v=f(x+33);
            let pitch_v=f(x+34);let roll_v=f(x+35);let still_v=f(x+36);
            let nods_v=f(x+37);let shakes_v=f(x+38);
            let med_v=f(x+39);let cog_v=f(x+40);let drow_v=f(x+41);
            let gpu_v=f(x+43);let gpu_r_v=f(x+44);let gpu_t_v=f(x+45);

            let mut sr = 0.0f64; let mut se2 = 0.0f64;
            for &ch_base in &ch_bases {
                let a = f(ch_base + 6 + 2); let b = f(ch_base + 6 + 3); let t = f(ch_base + 6 + 1);
                let d1 = a + t; let d2 = b + t;
                if d1 > 1e-6 { se2 += b / d1; }
                if d2 > 1e-6 { sr += a / d2; }
            }
            let relax_v  = sigmoid100((sr / 4.0) as f32, 2.5, 1.0) as f64;
            let engage_v = sigmoid100((se2 / 4.0) as f32, 2.0, 0.8) as f64;

            let row = EpochRow {
                t: timestamp, rd, rt, ra, rb, rg,
                relaxation: relax_v, engagement: engage_v,
                faa: faa_v, tar: tar_v, bar: bar_v, dtr: dtr_v, tbr: tbr_v,
                pse: pse_v, apf: apf_v, sef95: sef_v, sc: sc_v, bps: bps_v, snr: snr_v,
                coherence: coh_v, mu: mu_v,
                ha: ha_v, hm: hm_v, hc: hc_v,
                pe: pe_v, hfd: hfd_v, dfa: dfa_v, se: se_v, pac: pac_v, lat: lat_v,
                mood: mood_v,
                hr: hr_v, rmssd: rmssd_v, sdnn: sdnn_v, pnn50: pnn_v, lf_hf: lfhf_v,
                resp: resp_v, spo2: spo_v, perf: perf_v, stress: stress_v,
                blinks: blinks_v, blink_r: blink_r_v,
                pitch: pitch_v, roll: roll_v, still: still_v, nods: nods_v, shakes: shakes_v,
                med: med_v, cog: cog_v, drow: drow_v,
                gpu: gpu_v, gpu_render: gpu_r_v, gpu_tiler: gpu_t_v,
            };

            sum.rel_delta += rd;   sum.rel_theta += rt;   sum.rel_alpha += ra;
            sum.rel_beta  += rb;   sum.rel_gamma += rg;
            sum.relaxation += relax_v;  sum.engagement += engage_v;
            sum.faa += faa_v; sum.tar += tar_v; sum.bar += bar_v; sum.dtr += dtr_v; sum.tbr += tbr_v;
            sum.pse += pse_v; sum.apf += apf_v; sum.bps += bps_v; sum.snr += snr_v;
            sum.coherence += coh_v; sum.mu_suppression += mu_v; sum.mood += mood_v;
            sum.sef95 += sef_v; sum.spectral_centroid += sc_v;
            sum.hjorth_activity += ha_v; sum.hjorth_mobility += hm_v; sum.hjorth_complexity += hc_v;
            sum.permutation_entropy += pe_v; sum.higuchi_fd += hfd_v; sum.dfa_exponent += dfa_v;
            sum.sample_entropy += se_v; sum.pac_theta_gamma += pac_v; sum.laterality_index += lat_v;
            sum.hr += hr_v; sum.rmssd += rmssd_v; sum.sdnn += sdnn_v;
            sum.pnn50 += pnn_v; sum.lf_hf_ratio += lfhf_v; sum.respiratory_rate += resp_v;
            sum.spo2_estimate += spo_v; sum.perfusion_index += perf_v; sum.stress_index += stress_v;
            sum.blink_count += blinks_v; sum.blink_rate += blink_r_v;
            sum.head_pitch += pitch_v; sum.head_roll += roll_v; sum.stillness += still_v;
            sum.nod_count += nods_v; sum.shake_count += shakes_v;
            sum.meditation += med_v; sum.cognitive_load += cog_v; sum.drowsiness += drow_v;

            rows.push(row);
            count += 1;
        }
    }

    if count == 0 { return None; }
    let n = count as f64;
    sum.n_epochs = count;
    sum.rel_delta /= n; sum.rel_theta /= n; sum.rel_alpha /= n;
    sum.rel_beta /= n; sum.rel_gamma /= n;
    sum.relaxation /= n; sum.engagement /= n;
    sum.faa /= n; sum.tar /= n; sum.bar /= n; sum.dtr /= n; sum.tbr /= n;
    sum.pse /= n; sum.apf /= n; sum.bps /= n; sum.snr /= n;
    sum.coherence /= n; sum.mu_suppression /= n; sum.mood /= n;
    sum.sef95 /= n; sum.spectral_centroid /= n;
    sum.hjorth_activity /= n; sum.hjorth_mobility /= n; sum.hjorth_complexity /= n;
    sum.permutation_entropy /= n; sum.higuchi_fd /= n; sum.dfa_exponent /= n;
    sum.sample_entropy /= n; sum.pac_theta_gamma /= n; sum.laterality_index /= n;
    sum.hr /= n; sum.rmssd /= n; sum.sdnn /= n; sum.pnn50 /= n;
    sum.lf_hf_ratio /= n; sum.respiratory_rate /= n;
    sum.spo2_estimate /= n; sum.perfusion_index /= n; sum.stress_index /= n;
    sum.blink_rate /= n;
    sum.head_pitch /= n; sum.head_roll /= n; sum.stillness /= n;
    sum.meditation /= n; sum.cognitive_load /= n; sum.drowsiness /= n;

    eprintln!("[parquet-metrics] loaded {} rows from {}", count, path.display());
    Some(CsvMetricsResult { n_rows: count, summary: sum, timeseries: rows })
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ts_from_csv_bytes() {
        let csv = b"timestamp,col1,col2\n1700000000.123,1.0,2.0\n1700000005.456,3.0,4.0\n1700000010.789,5.0,6.0\n";
        assert_eq!(parse_first_ts_from_bytes(csv), Some(1700000000));
        assert_eq!(parse_last_ts_from_bytes(csv), Some(1700000010));
    }

    #[test]
    fn parse_ts_handles_trailing_newlines() {
        let csv = b"ts\n1700000001.0,x\n1700000002.0,y\n\n\n";
        assert_eq!(parse_first_ts_from_bytes(csv), Some(1700000001));
        assert_eq!(parse_last_ts_from_bytes(csv), Some(1700000002));
    }

    #[test]
    fn parse_ts_handles_crlf() {
        let csv = b"ts\r\n1700000001.0,x\r\n1700000002.0,y\r\n";
        assert_eq!(parse_first_ts_from_bytes(csv), Some(1700000001));
        assert_eq!(parse_last_ts_from_bytes(csv), Some(1700000002));
    }

    #[test]
    fn parse_ts_single_data_row() {
        let csv = b"ts\n1700000042.5,val\n";
        assert_eq!(parse_first_ts_from_bytes(csv), Some(1700000042));
        assert_eq!(parse_last_ts_from_bytes(csv), Some(1700000042));
    }

    #[test]
    fn parse_ts_empty_file() {
        assert_eq!(parse_first_ts_from_bytes(b""), None);
        assert_eq!(parse_last_ts_from_bytes(b""), None);
    }

    #[test]
    fn parse_ts_header_only() {
        let csv = b"timestamp,col1\n";
        assert_eq!(parse_first_ts_from_bytes(csv), None);
        // last_ts sees the header line but it's not a valid timestamp
        assert_eq!(parse_last_ts_from_bytes(csv), None);
    }

    #[test]
    fn typed_session_json_deserializes() {
        let json = r#"{
            "csv_file": "exg_1700000000.csv",
            "session_start_utc": 1700000000,
            "session_end_utc": 1700000300,
            "session_duration_s": 300,
            "device": {
                "name": "Muse 2",
                "id": "AA:BB:CC:DD:EE:FF",
                "serial_number": "SN123"
            },
            "total_samples": 76800,
            "sample_rate_hz": 256,
            "battery_pct_end": 85.0,
            "extra_field_ignored": true
        }"#;
        let meta: SessionJsonMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.csv_file.as_deref(), Some("exg_1700000000.csv"));
        assert_eq!(meta.session_start_utc, Some(1700000000));
        assert_eq!(meta.session_end_utc, Some(1700000300));
        assert_eq!(meta.device.name.as_deref(), Some("Muse 2"));
        assert_eq!(meta.device.id.as_deref(), Some("AA:BB:CC:DD:EE:FF"));
        assert_eq!(meta.total_samples, Some(76800));
        assert_eq!(meta.battery_pct_end, Some(85.0));
    }

    #[test]
    fn typed_session_json_legacy_flat_fields() {
        let json = r#"{
            "csv_file": "muse_1700000000.csv",
            "device_name": "Muse S",
            "device_id": "XX:YY",
            "battery_pct": 42.0
        }"#;
        let meta: SessionJsonMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.device_name.as_deref(), Some("Muse S"));
        assert_eq!(meta.device.name, None);
        assert_eq!(meta.battery_pct, Some(42.0));
    }

    #[test]
    fn typed_session_json_with_avg_snr() {
        let json = r#"{
            "csv_file": "exg_1700000000.csv",
            "session_start_utc": 1700000000,
            "avg_snr_db": 12.5
        }"#;
        let meta: SessionJsonMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.avg_snr_db, Some(12.5));
    }

    #[test]
    fn typed_session_json_without_avg_snr() {
        let json = r#"{
            "csv_file": "exg_1700000000.csv",
            "session_start_utc": 1700000000
        }"#;
        let meta: SessionJsonMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.avg_snr_db, None);
    }
}
