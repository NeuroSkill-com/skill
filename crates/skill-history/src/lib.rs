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

#[derive(Serialize, Deserialize, Clone, Debug)]
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
                    if fname.starts_with("muse_") && fname.ends_with(".json") {
                        return true;
                    }
                    if fname.starts_with("muse_") && fname.ends_with(".csv") {
                        if fname.ends_with("_metrics.csv") || fname.ends_with("_ppg.csv") {
                            return false;
                        }
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
        if !fname.starts_with("muse_") || !fname.ends_with(".json") { continue; }

        let json_str = match std::fs::read_to_string(&jp) { Ok(s) => s, Err(_) => continue };
        let meta: serde_json::Value = match serde_json::from_str(&json_str) { Ok(v) => v, Err(_) => continue };

        let csv_file = meta["csv_file"].as_str().unwrap_or("").to_string();
        let csv_full = day_dir.join(&csv_file);
        let csv_size = std::fs::metadata(&csv_full).map(|m| m.len()).unwrap_or(0);
        let start = meta["session_start_utc"].as_u64();
        let end   = meta["session_end_utc"].as_u64();
        let dev   = meta.get("device");
        let str_field = |obj: Option<&serde_json::Value>, nk: &str, fk: &str| -> Option<String> {
            obj.and_then(|d| d.get(nk)).and_then(|v| v.as_str()).map(str::to_owned)
                .or_else(|| meta.get(fk).and_then(|v| v.as_str()).map(str::to_owned))
        };
        raw.push((SessionEntry {
            csv_file,
            csv_path:           csv_full.to_string_lossy().into_owned(),
            session_start_utc:  start,
            session_end_utc:    end,
            session_duration_s: meta.get("session_duration_s").and_then(|v| v.as_u64())
                                    .or_else(|| start.zip(end).map(|(s, e)| e.saturating_sub(s))),
            device_name:        str_field(dev, "name", "device_name"),
            device_id:          str_field(dev, "id", "device_id"),
            serial_number:      str_field(dev, "serial_number", "serial_number"),
            mac_address:        str_field(dev, "mac_address", "mac_address"),
            firmware_version:   str_field(dev, "firmware_version", "firmware_version"),
            hardware_version:   str_field(dev, "hardware_version", "hardware_version"),
            headset_preset:     str_field(dev, "preset", "headset_preset"),
            battery_pct:        meta.get("battery_pct_end").and_then(|v| v.as_f64())
                                    .or_else(|| meta.get("battery_pct").and_then(|v| v.as_f64())),
            total_samples:      meta["total_samples"].as_u64(),
            sample_rate_hz:     meta["sample_rate_hz"].as_u64(),
            labels:             vec![],
            file_size_bytes:    csv_size,
        }, start, end));
    }

    // Second pass: orphaned CSVs
    for cf in &files {
        let cp = cf.path();
        let cfname = cp.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !cfname.ends_with(".csv") { continue; }
        if cfname.ends_with("_metrics.csv") || cfname.ends_with("_ppg.csv") { continue; }
        if cp.with_extension("json").exists() { continue; }
        // Match any device prefix: "muse_", "mw75_", "hermes_", "emotiv_", "idun_",
        // "ganglion_", "openbci_", "eeg_" — pattern: <kind>_<timestamp>.csv
        let ts_part = cfname.rsplit_once('_').map(|(_, ts)| ts);
        let meta_fs = std::fs::metadata(&cp);
        let csv_size = meta_fs.as_ref().map(|m| m.len()).unwrap_or(0);
        let ts: Option<u64> = ts_part
            .and_then(|s| s.strip_suffix(".csv"))
            .and_then(|s| s.parse().ok());
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

    let mut sessions: Vec<SessionEntry> = raw.into_iter().map(|(s, _, _)| s).collect();
    sessions.sort_by(|a, b| b.session_start_utc.cmp(&a.session_start_utc));
    sessions
}

/// Delete a session's CSV + JSON sidecar + metrics cache files.
pub fn delete_session(csv_path: &str) -> Result<(), String> {
    let csv = std::path::PathBuf::from(csv_path);
    let json = csv.with_extension("json");
    let ppg  = ppg_csv_path(&csv);
    let met  = metrics_csv_path(&csv);
    let stem = csv.file_stem().and_then(|s| s.to_str()).unwrap_or("muse");
    let cache = csv.with_file_name(format!("{stem}_metrics_cache.json"));
    if csv.exists()   { std::fs::remove_file(&csv).map_err(|e| e.to_string())?; }
    if json.exists()  { std::fs::remove_file(&json).map_err(|e| e.to_string())?; }
    if ppg.exists()   { std::fs::remove_file(&ppg).map_err(|e| e.to_string())?; }
    if met.exists()   { std::fs::remove_file(&met).map_err(|e| e.to_string())?; }
    if cache.exists() { let _ = std::fs::remove_file(&cache); }
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
                s.starts_with("muse_") && s.ends_with(".json")
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
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let files = match std::fs::read_dir(&path) { Ok(v) => v, Err(_) => continue };
        for file in files.filter_map(|e| e.ok()) {
            let jp = file.path();
            let fname = jp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !fname.starts_with("muse_") || !fname.ends_with(".json") { continue; }
            let json = match std::fs::read_to_string(&jp) { Ok(s) => s, Err(_) => continue };
            let meta: serde_json::Value = match serde_json::from_str(&json) { Ok(v) => v, Err(_) => continue };
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

    let mut all_ts: Vec<(u64, String)> = Vec::new();

    let entries = match std::fs::read_dir(skill_dir) { Ok(e) => e, Err(_) => return vec![] };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let day_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        if day_name.len() != 8 || !day_name.bytes().all(|b| b.is_ascii_digit()) { continue; }
        let db_path = path.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() { continue; }
        let conn = match rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) { Ok(c) => c, Err(_) => continue };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        let mut stmt = match conn.prepare("SELECT timestamp FROM embeddings ORDER BY timestamp") {
            Ok(s) => s, Err(_) => continue,
        };
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0));
        if let Ok(rows) = rows {
            for row in rows.filter_map(|r| r.ok()) {
                all_ts.push((ts_to_unix(row), day_name.clone()));
            }
        }
    }

    if all_ts.is_empty() { return vec![]; }
    all_ts.sort_by_key(|(ts, _)| *ts);

    let mut sessions: Vec<EmbeddingSession> = Vec::new();
    let mut start = all_ts[0].0;
    let mut end   = start;
    let mut count: u64 = 1;
    let mut day   = all_ts[0].1.clone();

    for &(ts, ref d) in &all_ts[1..] {
        if ts.saturating_sub(end) > GAP_SECS {
            sessions.push(EmbeddingSession { start_utc: start, end_utc: end, n_epochs: count, day: day.clone() });
            start = ts; end = ts; count = 1; day = d.clone();
        } else { end = ts; count += 1; }
    }
    sessions.push(EmbeddingSession { start_utc: start, end_utc: end, n_epochs: count, day });
    sessions.reverse();
    sessions
}

// ── CSV timestamp helpers ─────────────────────────────────────────────────────

fn read_metrics_csv_time_range(metrics_path: &Path) -> Option<(u64, u64)> {
    if !metrics_path.exists() { return None; }
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true).flexible(true).from_path(metrics_path).ok()?;
    let mut first: Option<u64> = None;
    let mut last:  Option<u64> = None;
    for result in rdr.records() {
        let rec = match result { Ok(r) => r, Err(_) => continue };
        let ts = match rec.get(0).and_then(|s| s.parse::<f64>().ok()) {
            Some(t) if t > 1_000_000_000.0 => t as u64,
            _ => continue,
        };
        if first.is_none() { first = Some(ts); }
        last = Some(ts);
    }
    Some((first?, last?))
}

fn patch_session_timestamps(raw: &mut [(SessionEntry, Option<u64>, Option<u64>)]) {
    for (session, start, end) in raw.iter_mut() {
        let mp = metrics_csv_path(Path::new(&session.csv_path));
        if let Some((first_ts, last_ts)) = read_metrics_csv_time_range(&mp) {
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

/// Read a `_metrics.csv` file and return aggregated summary + time-series.
pub fn load_metrics_csv(csv_path: &Path) -> Option<CsvMetricsResult> {
    let metrics_path = metrics_csv_path(csv_path);
    if !metrics_path.exists() {
        eprintln!("[csv-metrics] no metrics file: {}", metrics_path.display());
        return None;
    }

    let mut rdr = match csv::ReaderBuilder::new()
        .has_headers(true).flexible(true).from_path(&metrics_path)
    { Ok(r) => r, Err(e) => { eprintln!("[csv-metrics] open error: {e}"); return None; } };

    let mut rows: Vec<EpochRow> = Vec::new();
    let mut sum = SessionMetrics::default();
    let mut count = 0usize;

    for result in rdr.records() {
        let rec = match result { Ok(r) => r, Err(_) => continue };
        if rec.len() < 49 { continue; }

        let f = |i: usize| -> f64 {
            rec.get(i).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)
        };

        let timestamp = f(0);
        if timestamp <= 0.0 { continue; }

        let avg_rel = |band_offset: usize| -> f64 {
            let mut s = 0.0;
            for ch_base in &[1usize, 13, 25, 37] { s += f(ch_base + 6 + band_offset); }
            s / 4.0
        };

        let rd = avg_rel(0);
        let rt = avg_rel(1);
        let ra = avg_rel(2);
        let rb = avg_rel(3);
        let rg = avg_rel(4);

        let faa_v  = f(49);  let tar_v  = f(50);  let bar_v  = f(51);  let dtr_v  = f(52);
        let pse_v  = f(53);  let apf_v  = f(54);  let bps_v  = f(55);  let snr_v  = f(56);
        let coh_v  = f(57);  let mu_v   = f(58);  let mood_v = f(59);
        let tbr_v  = f(60);  let sef_v  = f(61);  let sc_v   = f(62);
        let ha_v   = f(63);  let hm_v   = f(64);  let hc_v   = f(65);
        let pe_v   = f(66);  let hfd_v  = f(67);  let dfa_v  = f(68);
        let se_v   = f(69);  let pac_v  = f(70);  let lat_v  = f(71);
        let hr_v   = f(72);  let rmssd_v= f(73);  let sdnn_v = f(74);
        let pnn_v  = f(75);  let lfhf_v = f(76);  let resp_v = f(77);
        let spo_v  = f(78);  let perf_v = f(79);  let stress_v = f(80);
        let blinks_v = f(81); let blink_r_v = f(82);
        let pitch_v = f(83); let roll_v = f(84); let still_v = f(85);
        let nods_v  = f(86); let shakes_v = f(87);
        let med_v = f(88); let cog_v = f(89); let drow_v = f(90);
        let gpu_v = f(92); let gpu_r_v = f(93); let gpu_t_v = f(94);

        let mut sr = 0.0f64; let mut se2 = 0.0f64;
        for ch_base in &[1usize, 13, 25, 37] {
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

