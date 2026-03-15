// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Session history, metrics, time-series, sleep staging, and analysis.
// Pure library crate — no Tauri dependencies.  Thin Tauri IPC wrappers
// live in `src-tauri/src/{history_cmds,session_analysis}.rs`.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use skill_data::label_store::LabelRow;
use skill_data::session_csv::{metrics_csv_path, ppg_csv_path};
use skill_data::util::{unix_to_ts, ts_to_unix};

// Re-export types consumed by Tauri wrappers.
pub use skill_data::label_store;

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
        if !cfname.starts_with("muse_") || !cfname.ends_with(".csv") { continue; }
        if cfname.ends_with("_metrics.csv") || cfname.ends_with("_ppg.csv") { continue; }
        if cp.with_extension("json").exists() { continue; }
        let meta_fs = std::fs::metadata(&cp);
        let csv_size = meta_fs.as_ref().map(|m| m.len()).unwrap_or(0);
        let ts: Option<u64> = cfname.strip_prefix("muse_")
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
            sample_rate_hz:     Some(256),
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

// ── Disk cache ────────────────────────────────────────────────────────────────

/// Cache file path: `muse_XXX.csv` → `muse_XXX_metrics_cache.json`
fn metrics_cache_path(csv_path: &Path) -> std::path::PathBuf {
    let stem = csv_path.file_stem().and_then(|s| s.to_str()).unwrap_or("muse");
    csv_path.with_file_name(format!("{stem}_metrics_cache.json"))
}

/// Load metrics from disk cache if valid, otherwise compute from CSV and cache.
pub fn load_csv_metrics_cached(csv_path: &Path) -> Option<CsvMetricsResult> {
    let metrics_csv = metrics_csv_path(csv_path);
    if !metrics_csv.exists() { return None; }

    let cache_path = metrics_cache_path(csv_path);

    if cache_path.exists() {
        let csv_mtime = std::fs::metadata(&metrics_csv).ok().and_then(|m| m.modified().ok());
        let cache_mtime = std::fs::metadata(&cache_path).ok().and_then(|m| m.modified().ok());
        if let (Some(cm), Some(ca)) = (csv_mtime, cache_mtime) {
            if ca >= cm {
                if let Ok(data) = std::fs::read(&cache_path) {
                    if let Ok(result) = serde_json::from_slice::<CsvMetricsResult>(&data) {
                        return Some(result);
                    }
                }
            }
        }
    }

    let result = load_metrics_csv(csv_path)?;

    let cache_path_owned = cache_path.to_path_buf();
    let result_clone = result.clone();
    std::thread::spawn(move || {
        if let Ok(json) = serde_json::to_vec(&result_clone) {
            let _ = std::fs::write(&cache_path_owned, json);
        }
    });

    Some(result)
}

/// Downsample a timeseries to at most `max` points.
pub fn downsample_timeseries(ts: &mut Vec<EpochRow>, max: usize) {
    let n = ts.len();
    if n <= max || max < 2 { return; }
    let step = (n - 1) as f64 / (max - 1) as f64;
    let mut sampled = Vec::with_capacity(max);
    for i in 0..max {
        let idx = (i as f64 * step).round() as usize;
        sampled.push(ts[idx.min(n - 1)].clone());
    }
    *ts = sampled;
}

/// Batch-load metrics for multiple sessions.
pub fn get_day_metrics_batch(
    csv_paths: &[String],
    max_ts_points: usize,
) -> HashMap<String, CsvMetricsResult> {
    let mut out = HashMap::with_capacity(csv_paths.len());
    for path in csv_paths {
        if let Some(mut result) = load_csv_metrics_cached(Path::new(path)) {
            downsample_timeseries(&mut result.timeseries, max_ts_points);
            out.insert(path.clone(), result);
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════════════
// SQLite-based metrics & time-series
// ═══════════════════════════════════════════════════════════════════════════════

fn migrate_embeddings_schema(conn: &rusqlite::Connection) {
    let _ = conn.execute("ALTER TABLE embeddings ADD COLUMN metrics_json TEXT", []);
}

/// Return per-epoch time-series data for a session range (from SQLite).
pub fn get_session_timeseries(
    skill_dir: &Path,
    start_utc: u64,
    end_utc:   u64,
) -> Vec<EpochRow> {
    let ts_start = unix_to_ts(start_utc);
    let ts_end   = unix_to_ts(end_utc);
    let mut rows: Vec<EpochRow> = Vec::new();

    let entries = match std::fs::read_dir(skill_dir) { Ok(e) => e, Err(_) => return rows };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let db_path = path.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() { continue; }

        let conn = match rusqlite::Connection::open(&db_path) { Ok(c) => c, Err(_) => continue };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        migrate_embeddings_schema(&conn);

        let mut stmt = match conn.prepare(
            "SELECT timestamp,
                    json_extract(metrics_json, '$.rel_delta'),
                    json_extract(metrics_json, '$.rel_theta'),
                    json_extract(metrics_json, '$.rel_alpha'),
                    json_extract(metrics_json, '$.rel_beta'),
                    json_extract(metrics_json, '$.rel_gamma'),
                    json_extract(metrics_json, '$.relaxation_score'),
                    json_extract(metrics_json, '$.engagement_score'),
                    json_extract(metrics_json, '$.faa'),
                    json_extract(metrics_json, '$.tar'),
                    json_extract(metrics_json, '$.bar'),
                    json_extract(metrics_json, '$.dtr'),
                    json_extract(metrics_json, '$.pse'),
                    json_extract(metrics_json, '$.apf'),
                    json_extract(metrics_json, '$.bps'),
                    json_extract(metrics_json, '$.snr'),
                    json_extract(metrics_json, '$.coherence'),
                    json_extract(metrics_json, '$.mu_suppression'),
                    json_extract(metrics_json, '$.mood'),
                    json_extract(metrics_json, '$.tbr'),
                    json_extract(metrics_json, '$.sef95'),
                    json_extract(metrics_json, '$.spectral_centroid'),
                    json_extract(metrics_json, '$.hjorth_activity'),
                    json_extract(metrics_json, '$.hjorth_mobility'),
                    json_extract(metrics_json, '$.hjorth_complexity'),
                    json_extract(metrics_json, '$.permutation_entropy'),
                    json_extract(metrics_json, '$.higuchi_fd'),
                    json_extract(metrics_json, '$.dfa_exponent'),
                    json_extract(metrics_json, '$.sample_entropy'),
                    json_extract(metrics_json, '$.pac_theta_gamma'),
                    json_extract(metrics_json, '$.laterality_index'),
                    json_extract(metrics_json, '$.hr'),
                    json_extract(metrics_json, '$.rmssd'),
                    json_extract(metrics_json, '$.sdnn'),
                    json_extract(metrics_json, '$.pnn50'),
                    json_extract(metrics_json, '$.lf_hf_ratio'),
                    json_extract(metrics_json, '$.respiratory_rate'),
                    json_extract(metrics_json, '$.spo2_estimate'),
                    json_extract(metrics_json, '$.perfusion_idx'),
                    json_extract(metrics_json, '$.stress_index'),
                    json_extract(metrics_json, '$.blink_count'),
                    json_extract(metrics_json, '$.blink_rate'),
                    json_extract(metrics_json, '$.head_pitch'),
                    json_extract(metrics_json, '$.head_roll'),
                    json_extract(metrics_json, '$.stillness'),
                    json_extract(metrics_json, '$.nod_count'),
                    json_extract(metrics_json, '$.shake_count'),
                    json_extract(metrics_json, '$.meditation'),
                    json_extract(metrics_json, '$.cognitive_load'),
                    json_extract(metrics_json, '$.drowsiness')
             FROM embeddings
             WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp ASC"
        ) { Ok(s) => s, Err(_) => continue };

        let iter = stmt.query_map(rusqlite::params![ts_start, ts_end], |row| {
            let ts_val: i64 = row.get(0)?;
            let utc = ts_to_unix(ts_val);
            let g = |i: usize| -> f64 { row.get::<_, Option<f64>>(i).unwrap_or(None).unwrap_or(0.0) };
            Ok(EpochRow {
                t: utc as f64,
                rd: g(1), rt: g(2), ra: g(3), rb: g(4), rg: g(5),
                relaxation: g(6), engagement: g(7), faa: g(8),
                tar: g(9), bar: g(10), dtr: g(11), pse: g(12), apf: g(13),
                bps: g(14), snr: g(15), coherence: g(16), mu: g(17), mood: g(18),
                tbr: g(19), sef95: g(20), sc: g(21),
                ha: g(22), hm: g(23), hc: g(24),
                pe: g(25), hfd: g(26), dfa: g(27), se: g(28), pac: g(29), lat: g(30),
                hr: g(31), rmssd: g(32), sdnn: g(33), pnn50: g(34), lf_hf: g(35),
                resp: g(36), spo2: g(37), perf: g(38), stress: g(39),
                blinks: g(40), blink_r: g(41),
                pitch: g(42), roll: g(43), still: g(44), nods: g(45), shakes: g(46),
                med: g(47), cog: g(48), drow: g(49),
                gpu: 0.0, gpu_render: 0.0, gpu_tiler: 0.0,
            })
        });

        if let Ok(iter) = iter {
            for row in iter.filter_map(|r| r.ok()) { rows.push(row); }
        }
    }
    rows.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    rows
}

/// Query aggregated band-power metrics from SQLite databases.
pub fn get_session_metrics(
    skill_dir: &Path,
    start_utc: u64,
    end_utc:   u64,
) -> SessionMetrics {
    let ts_start = unix_to_ts(start_utc);
    let ts_end   = unix_to_ts(end_utc);

    let mut total = SessionMetrics::default();
    let mut count = 0u64;

    let entries = match std::fs::read_dir(skill_dir) { Ok(e) => e, Err(_) => return total };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let db_path = path.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() { continue; }

        let conn = match rusqlite::Connection::open(&db_path) { Ok(c) => c, Err(_) => continue };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        migrate_embeddings_schema(&conn);

        let mut stmt = match conn.prepare(
            "SELECT json_extract(metrics_json, '$.rel_delta'),
                    json_extract(metrics_json, '$.rel_theta'),
                    json_extract(metrics_json, '$.rel_alpha'),
                    json_extract(metrics_json, '$.rel_beta'),
                    json_extract(metrics_json, '$.rel_gamma'),
                    json_extract(metrics_json, '$.rel_high_gamma'),
                    json_extract(metrics_json, '$.relaxation_score'),
                    json_extract(metrics_json, '$.engagement_score'),
                    json_extract(metrics_json, '$.faa'),
                    json_extract(metrics_json, '$.tar'),
                    json_extract(metrics_json, '$.bar'),
                    json_extract(metrics_json, '$.dtr'),
                    json_extract(metrics_json, '$.pse'),
                    json_extract(metrics_json, '$.apf'),
                    json_extract(metrics_json, '$.bps'),
                    json_extract(metrics_json, '$.snr'),
                    json_extract(metrics_json, '$.coherence'),
                    json_extract(metrics_json, '$.mu_suppression'),
                    json_extract(metrics_json, '$.mood'),
                    json_extract(metrics_json, '$.tbr'),
                    json_extract(metrics_json, '$.sef95'),
                    json_extract(metrics_json, '$.spectral_centroid'),
                    json_extract(metrics_json, '$.hjorth_activity'),
                    json_extract(metrics_json, '$.hjorth_mobility'),
                    json_extract(metrics_json, '$.hjorth_complexity'),
                    json_extract(metrics_json, '$.permutation_entropy'),
                    json_extract(metrics_json, '$.higuchi_fd'),
                    json_extract(metrics_json, '$.dfa_exponent'),
                    json_extract(metrics_json, '$.sample_entropy'),
                    json_extract(metrics_json, '$.pac_theta_gamma'),
                    json_extract(metrics_json, '$.laterality_index'),
                    json_extract(metrics_json, '$.hr'),
                    json_extract(metrics_json, '$.rmssd'),
                    json_extract(metrics_json, '$.sdnn'),
                    json_extract(metrics_json, '$.pnn50'),
                    json_extract(metrics_json, '$.lf_hf_ratio'),
                    json_extract(metrics_json, '$.respiratory_rate'),
                    json_extract(metrics_json, '$.spo2_estimate'),
                    json_extract(metrics_json, '$.perfusion_idx'),
                    json_extract(metrics_json, '$.stress_index'),
                    json_extract(metrics_json, '$.blink_count'),
                    json_extract(metrics_json, '$.blink_rate'),
                    json_extract(metrics_json, '$.head_pitch'),
                    json_extract(metrics_json, '$.head_roll'),
                    json_extract(metrics_json, '$.stillness'),
                    json_extract(metrics_json, '$.nod_count'),
                    json_extract(metrics_json, '$.shake_count'),
                    json_extract(metrics_json, '$.meditation'),
                    json_extract(metrics_json, '$.cognitive_load'),
                    json_extract(metrics_json, '$.drowsiness')
             FROM embeddings
             WHERE timestamp >= ?1 AND timestamp <= ?2"
        ) { Ok(s) => s, Err(_) => continue };

        let rows = stmt.query_map(rusqlite::params![ts_start, ts_end], |row| {
            let mut v = Vec::with_capacity(50);
            for i in 0..50 { v.push(row.get::<_, Option<f64>>(i)?); }
            Ok(v)
        });

        if let Ok(rows) = rows {
            for row in rows.filter_map(|r| r.ok()) {
                let v = row;
                if v[0].is_none() && v[1].is_none() { continue; }
                total.rel_delta      += v[0].unwrap_or(0.0);
                total.rel_theta      += v[1].unwrap_or(0.0);
                total.rel_alpha      += v[2].unwrap_or(0.0);
                total.rel_beta       += v[3].unwrap_or(0.0);
                total.rel_gamma      += v[4].unwrap_or(0.0);
                total.rel_high_gamma += v[5].unwrap_or(0.0);
                total.relaxation     += v[6].unwrap_or(0.0);
                total.engagement     += v[7].unwrap_or(0.0);
                total.faa            += v[8].unwrap_or(0.0);
                total.tar            += v[9].unwrap_or(0.0);
                total.bar            += v[10].unwrap_or(0.0);
                total.dtr            += v[11].unwrap_or(0.0);
                total.pse            += v[12].unwrap_or(0.0);
                total.apf            += v[13].unwrap_or(0.0);
                total.bps            += v[14].unwrap_or(0.0);
                total.snr            += v[15].unwrap_or(0.0);
                total.coherence      += v[16].unwrap_or(0.0);
                total.mu_suppression += v[17].unwrap_or(0.0);
                total.mood           += v[18].unwrap_or(0.0);
                total.tbr            += v[19].unwrap_or(0.0);
                total.sef95          += v[20].unwrap_or(0.0);
                total.spectral_centroid += v[21].unwrap_or(0.0);
                total.hjorth_activity   += v[22].unwrap_or(0.0);
                total.hjorth_mobility   += v[23].unwrap_or(0.0);
                total.hjorth_complexity  += v[24].unwrap_or(0.0);
                total.permutation_entropy += v[25].unwrap_or(0.0);
                total.higuchi_fd     += v[26].unwrap_or(0.0);
                total.dfa_exponent   += v[27].unwrap_or(0.0);
                total.sample_entropy += v[28].unwrap_or(0.0);
                total.pac_theta_gamma += v[29].unwrap_or(0.0);
                total.laterality_index += v[30].unwrap_or(0.0);
                total.hr               += v[31].unwrap_or(0.0);
                total.rmssd            += v[32].unwrap_or(0.0);
                total.sdnn             += v[33].unwrap_or(0.0);
                total.pnn50            += v[34].unwrap_or(0.0);
                total.lf_hf_ratio      += v[35].unwrap_or(0.0);
                total.respiratory_rate += v[36].unwrap_or(0.0);
                total.spo2_estimate    += v[37].unwrap_or(0.0);
                total.perfusion_index  += v[38].unwrap_or(0.0);
                total.stress_index     += v[39].unwrap_or(0.0);
                total.blink_count      += v[40].unwrap_or(0.0);
                total.blink_rate       += v[41].unwrap_or(0.0);
                total.head_pitch       += v[42].unwrap_or(0.0);
                total.head_roll        += v[43].unwrap_or(0.0);
                total.stillness        += v[44].unwrap_or(0.0);
                total.nod_count        += v[45].unwrap_or(0.0);
                total.shake_count      += v[46].unwrap_or(0.0);
                total.meditation       += v[47].unwrap_or(0.0);
                total.cognitive_load   += v[48].unwrap_or(0.0);
                total.drowsiness       += v[49].unwrap_or(0.0);
                count += 1;
            }
        }
    }

    if count > 0 {
        let n = count as f64;
        total.rel_delta /= n; total.rel_theta /= n; total.rel_alpha /= n;
        total.rel_beta  /= n; total.rel_gamma /= n; total.rel_high_gamma /= n;
        total.relaxation /= n; total.engagement /= n;
        total.faa /= n; total.tar /= n; total.bar /= n; total.dtr /= n; total.tbr /= n;
        total.pse /= n; total.apf /= n; total.bps /= n; total.snr /= n;
        total.coherence /= n; total.mu_suppression /= n; total.mood /= n;
        total.sef95 /= n; total.spectral_centroid /= n;
        total.hjorth_activity /= n; total.hjorth_mobility /= n; total.hjorth_complexity /= n;
        total.permutation_entropy /= n; total.higuchi_fd /= n; total.dfa_exponent /= n;
        total.sample_entropy /= n; total.pac_theta_gamma /= n; total.laterality_index /= n;
        total.hr /= n; total.rmssd /= n; total.sdnn /= n; total.pnn50 /= n;
        total.lf_hf_ratio /= n; total.respiratory_rate /= n; total.spo2_estimate /= n;
        total.perfusion_index /= n; total.stress_index /= n;
        total.blink_count /= n; total.blink_rate /= n;
        total.head_pitch /= n; total.head_roll /= n; total.stillness /= n;
        total.nod_count /= n; total.shake_count /= n;
        total.meditation /= n; total.cognitive_load /= n; total.drowsiness /= n;
        total.n_epochs = count as usize;
    }
    total
}

// ═══════════════════════════════════════════════════════════════════════════════
// Sleep staging
// ═══════════════════════════════════════════════════════════════════════════════

/// Classify each embedding epoch in `[start_utc, end_utc]` into a sleep stage.
pub fn get_sleep_stages(skill_dir: &Path, start_utc: u64, end_utc: u64) -> SleepStages {
    let ts_start = unix_to_ts(start_utc);
    let ts_end   = unix_to_ts(end_utc);

    struct RawEpoch { utc: u64, rd: f64, rt: f64, ra: f64, rb: f64 }
    let mut raw: Vec<RawEpoch> = Vec::new();

    let entries = match std::fs::read_dir(skill_dir) {
        Ok(e) => e,
        Err(_) => return SleepStages { epochs: vec![], summary: SleepSummary::default() },
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let db_path = path.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() { continue; }
        let conn = match rusqlite::Connection::open_with_flags(
            &db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) { Ok(c) => c, Err(_) => continue };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        let mut stmt = match conn.prepare(
            "SELECT timestamp,
                    json_extract(metrics_json, '$.rel_delta'),
                    json_extract(metrics_json, '$.rel_theta'),
                    json_extract(metrics_json, '$.rel_alpha'),
                    json_extract(metrics_json, '$.rel_beta')
             FROM embeddings WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp"
        ) { Ok(s) => s, Err(_) => continue };
        let rows = stmt.query_map(rusqlite::params![ts_start, ts_end], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<f64>>(1)?,
                row.get::<_, Option<f64>>(2)?, row.get::<_, Option<f64>>(3)?,
                row.get::<_, Option<f64>>(4)?))
        });
        if let Ok(rows) = rows {
            for row in rows.filter_map(|r| r.ok()) {
                let (ts, rd, rt, ra, rb) = row;
                if rd.is_none() && rt.is_none() { continue; }
                raw.push(RawEpoch {
                    utc: ts_to_unix(ts), rd: rd.unwrap_or(0.0), rt: rt.unwrap_or(0.0),
                    ra: ra.unwrap_or(0.0), rb: rb.unwrap_or(0.0),
                });
            }
        }
    }
    raw.sort_by_key(|e| e.utc);

    let mut summary = SleepSummary::default();
    let epochs: Vec<SleepEpoch> = raw.iter().map(|e| {
        let stage = classify_sleep(e.rd, e.rt, e.ra, e.rb);
        match stage { 0 => summary.wake_epochs += 1, 1 => summary.n1_epochs += 1,
                       2 => summary.n2_epochs += 1, 3 => summary.n3_epochs += 1,
                       5 => summary.rem_epochs += 1, _ => {} }
        SleepEpoch { utc: e.utc, stage, rel_delta: e.rd, rel_theta: e.rt,
                     rel_alpha: e.ra, rel_beta: e.rb }
    }).collect();

    summary.total_epochs = epochs.len();
    if epochs.len() >= 2 {
        let mut gaps: Vec<f64> = epochs.windows(2)
            .map(|w| (w[1].utc as f64) - (w[0].utc as f64))
            .filter(|g| *g > 0.0 && *g < 30.0).collect();
        if !gaps.is_empty() {
            gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
            summary.epoch_secs = gaps[gaps.len() / 2];
        } else { summary.epoch_secs = 2.5; }
    } else { summary.epoch_secs = 2.5; }

    SleepStages { epochs, summary }
}

fn classify_sleep(rd: f64, rt: f64, ra: f64, rb: f64) -> u8 {
    if ra > 0.30 || rb > 0.30 { return 0; }
    if rt > 0.30 && ra < 0.15 && rd < 0.45 { return 5; }
    if rd > 0.50 { return 3; }
    if rt > 0.25 && rd < 0.50 { return 1; }
    2
}

// ═══════════════════════════════════════════════════════════════════════════════
// Analysis helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn r2f(v: f64) -> f64 { (v * 100.0).round() / 100.0 }

fn linear_slope(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 2 { return 0.0; }
    let x_mean = (n - 1) as f64 / 2.0;
    let y_mean = values.iter().sum::<f64>() / n as f64;
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for (i, &y) in values.iter().enumerate() {
        let dx = i as f64 - x_mean;
        num += dx * (y - y_mean);
        den += dx * dx;
    }
    if den.abs() < 1e-15 { 0.0 } else { num / den }
}

fn metric_stats_vec(values: &[f64]) -> serde_json::Value {
    if values.is_empty() { return serde_json::json!(null); }
    let n = values.len();
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean = values.iter().sum::<f64>() / n as f64;
    let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
    let stddev = variance.sqrt();
    let median = if n % 2 == 0 { (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0 } else { sorted[n / 2] };
    let p25 = sorted[n / 4];
    let p75 = sorted[3 * n / 4];
    let slope = linear_slope(values);
    serde_json::json!({
        "min": r2f(sorted[0]), "max": r2f(sorted[n - 1]),
        "mean": r2f(mean), "median": r2f(median),
        "stddev": r2f(stddev), "p25": r2f(p25), "p75": r2f(p75),
        "trend": r2f(slope),
    })
}

fn epoch_field(row: &EpochRow, name: &str) -> f64 {
    match name {
        "relaxation" => row.relaxation, "engagement" => row.engagement,
        "faa" => row.faa, "tar" => row.tar, "bar" => row.bar, "dtr" => row.dtr, "tbr" => row.tbr,
        "mood" => row.mood, "hr" => row.hr, "rmssd" => row.rmssd, "sdnn" => row.sdnn,
        "stress" => row.stress, "snr" => row.snr, "coherence" => row.coherence,
        "stillness" => row.still, "blink_rate" => row.blink_r,
        "meditation" => row.med, "cognitive_load" => row.cog, "drowsiness" => row.drow,
        "rel_delta" => row.rd, "rel_theta" => row.rt, "rel_alpha" => row.ra, "rel_beta" => row.rb,
        "pse" => row.pse, "apf" => row.apf, "sef95" => row.sef95,
        _ => 0.0,
    }
}

fn session_field(m: &SessionMetrics, name: &str) -> f64 {
    match name {
        "relaxation" => m.relaxation, "engagement" => m.engagement,
        "faa" => m.faa, "tar" => m.tar, "bar" => m.bar, "dtr" => m.dtr, "tbr" => m.tbr,
        "mood" => m.mood, "hr" => m.hr, "rmssd" => m.rmssd, "sdnn" => m.sdnn,
        "stress" => m.stress_index, "snr" => m.snr, "coherence" => m.coherence,
        "stillness" => m.stillness, "blink_rate" => m.blink_rate,
        "meditation" => m.meditation, "cognitive_load" => m.cognitive_load, "drowsiness" => m.drowsiness,
        "rel_delta" => m.rel_delta, "rel_theta" => m.rel_theta, "rel_alpha" => m.rel_alpha, "rel_beta" => m.rel_beta,
        "pse" => m.pse, "apf" => m.apf, "sef95" => m.sef95,
        _ => 0.0,
    }
}

const INSIGHT_METRICS: &[&str] = &[
    "relaxation", "engagement", "meditation", "cognitive_load", "drowsiness",
    "mood", "faa", "tar", "bar", "dtr", "tbr",
    "hr", "rmssd", "stress", "snr", "coherence", "stillness",
    "blink_rate", "rel_alpha", "rel_beta", "rel_theta", "rel_delta",
    "pse", "apf", "sef95",
];

const STATUS_METRICS: &[&str] = &[
    "relaxation", "engagement", "meditation", "cognitive_load",
    "drowsiness", "mood", "hr", "snr", "stillness",
];

/// Compute per-metric stats, deltas, and trends for an A/B session comparison.
pub fn compute_compare_insights(
    skill_dir: &Path,
    a_start: u64, a_end: u64,
    b_start: u64, b_end: u64,
    avg_a: &SessionMetrics,
    avg_b: &SessionMetrics,
) -> serde_json::Value {
    let ts_a = get_session_timeseries(skill_dir, a_start, a_end);
    let ts_b = get_session_timeseries(skill_dir, b_start, b_end);

    let mut stats_a = serde_json::Map::new();
    let mut stats_b = serde_json::Map::new();
    let mut deltas  = serde_json::Map::new();
    let mut improved: Vec<String> = Vec::new();
    let mut declined: Vec<String> = Vec::new();
    let mut stable:   Vec<String> = Vec::new();

    for &metric in INSIGHT_METRICS {
        let vals_a: Vec<f64> = ts_a.iter().map(|r| epoch_field(r, metric)).collect();
        let vals_b: Vec<f64> = ts_b.iter().map(|r| epoch_field(r, metric)).collect();
        stats_a.insert(metric.into(), metric_stats_vec(&vals_a));
        stats_b.insert(metric.into(), metric_stats_vec(&vals_b));

        let ma = session_field(avg_a, metric);
        let mb = session_field(avg_b, metric);
        let abs_delta = mb - ma;
        let pct = if ma.abs() > 1e-6 { abs_delta / ma.abs() * 100.0 } else { 0.0 };
        let direction = if pct > 5.0 { "up" } else if pct < -5.0 { "down" } else { "stable" };

        deltas.insert(metric.into(), serde_json::json!({
            "a": r2f(ma), "b": r2f(mb), "abs": r2f(abs_delta), "pct": r2f(pct), "direction": direction,
        }));
        match direction {
            "up"   => improved.push(metric.into()),
            "down" => declined.push(metric.into()),
            _      => stable.push(metric.into()),
        }
    }
    serde_json::json!({
        "stats_a": stats_a, "stats_b": stats_b, "deltas": deltas,
        "improved": improved, "declined": declined, "stable": stable,
        "n_epochs_a": ts_a.len(), "n_epochs_b": ts_b.len(),
    })
}

/// Compute derived sleep-quality metrics from classified sleep stages.
pub fn analyze_sleep_stages(stages: &SleepStages) -> serde_json::Value {
    let epochs = &stages.epochs;
    let summary = &stages.summary;
    if epochs.is_empty() { return serde_json::json!(null); }

    let epoch_secs = if summary.epoch_secs > 0.0 { summary.epoch_secs } else { 5.0 };
    let total = summary.total_epochs as f64;
    let wake  = summary.wake_epochs as f64;
    let efficiency = if total > 0.0 { (total - wake) / total * 100.0 } else { 0.0 };
    let stage_minutes = serde_json::json!({
        "wake": r2f(wake * epoch_secs / 60.0),
        "n1":   r2f(summary.n1_epochs as f64 * epoch_secs / 60.0),
        "n2":   r2f(summary.n2_epochs as f64 * epoch_secs / 60.0),
        "n3":   r2f(summary.n3_epochs as f64 * epoch_secs / 60.0),
        "rem":  r2f(summary.rem_epochs as f64 * epoch_secs / 60.0),
        "total":r2f(total * epoch_secs / 60.0),
    });
    let first_sleep_idx = epochs.iter().position(|e| e.stage != 0);
    let onset_latency_min = match first_sleep_idx {
        Some(idx) if idx > 0 => r2f(epochs[idx].utc.saturating_sub(epochs[0].utc) as f64 / 60.0),
        _ => 0.0,
    };
    let rem_latency_min = first_sleep_idx.and_then(|si| {
        let start = epochs[si].utc;
        epochs[si..].iter().find(|e| e.stage == 5)
            .map(|e| r2f(e.utc.saturating_sub(start) as f64 / 60.0))
    });
    let mut transitions = 0u32;
    let mut awakenings  = 0u32;
    for w in epochs.windows(2) {
        if w[0].stage != w[1].stage {
            transitions += 1;
            if w[1].stage == 0 && w[0].stage != 0 { awakenings += 1; }
        }
    }
    let stage_ids: &[(u8, &str)] = &[(0,"wake"),(1,"n1"),(2,"n2"),(3,"n3"),(5,"rem")];
    let mut bouts = serde_json::Map::new();
    for &(sid, name) in stage_ids {
        let mut lengths: Vec<f64> = Vec::new();
        let mut cur = 0u32;
        for e in epochs {
            if e.stage == sid { cur += 1; }
            else { if cur > 0 { lengths.push(cur as f64 * epoch_secs / 60.0); } cur = 0; }
        }
        if cur > 0 { lengths.push(cur as f64 * epoch_secs / 60.0); }
        if !lengths.is_empty() {
            let count = lengths.len();
            let mean = lengths.iter().sum::<f64>() / count as f64;
            let max  = lengths.iter().cloned().fold(0.0f64, f64::max);
            bouts.insert(name.into(), serde_json::json!({ "count": count, "mean_min": r2f(mean), "max_min": r2f(max) }));
        }
    }
    serde_json::json!({
        "efficiency_pct": r2f(efficiency), "onset_latency_min": onset_latency_min,
        "rem_latency_min": rem_latency_min, "stage_minutes": stage_minutes,
        "transitions": transitions, "awakenings": awakenings, "bouts": bouts,
    })
}

/// Compute search-result insights.
pub fn analyze_search_results(result: &skill_commands::SearchResult) -> serde_json::Value {
    let all_distances: Vec<f64> = result.results.iter()
        .flat_map(|q| q.neighbors.iter().map(|n| n.distance as f64)).collect();
    let distance_stats = metric_stats_vec(&all_distances);
    let mut hour_dist: HashMap<u8, u32> = HashMap::new();
    let mut day_dist:  HashMap<String, u32> = HashMap::new();
    let mut all_utcs: Vec<u64> = Vec::new();
    for q in &result.results {
        for n in &q.neighbors {
            all_utcs.push(n.timestamp_unix);
            *hour_dist.entry(((n.timestamp_unix % 86400) / 3600) as u8).or_insert(0) += 1;
            *day_dist.entry(n.date.clone()).or_insert(0) += 1;
        }
    }
    let mut hourly = serde_json::Map::new();
    for h in 0..24u8 { if let Some(&c) = hour_dist.get(&h) { hourly.insert(format!("{h:02}"), c.into()); } }
    let mut top_days: Vec<(String, u32)> = day_dist.into_iter().collect();
    top_days.sort_by(|a, b| b.1.cmp(&a.1));
    top_days.truncate(10);
    let time_span_hours = if all_utcs.len() >= 2 {
        let mn = *all_utcs.iter().min().unwrap();
        let mx = *all_utcs.iter().max().unwrap();
        mx.saturating_sub(mn) as f64 / 3600.0
    } else { 0.0 };
    let metric_names = ["relaxation","engagement","meditation","cognitive_load","drowsiness","hr","snr","mood"];
    let mut neighbor_metrics = serde_json::Map::new();
    for &name in &metric_names {
        let vals: Vec<f64> = result.results.iter()
            .flat_map(|q| q.neighbors.iter()).filter_map(|n| n.metrics.as_ref())
            .filter_map(|m| match name {
                "relaxation" => m.relaxation, "engagement" => m.engagement,
                "meditation" => m.meditation, "cognitive_load" => m.cognitive_load,
                "drowsiness" => m.drowsiness, "hr" => m.hr, "snr" => m.snr, "mood" => m.mood,
                _ => None,
            }).collect();
        if !vals.is_empty() {
            neighbor_metrics.insert(name.into(), serde_json::json!(r2f(vals.iter().sum::<f64>() / vals.len() as f64)));
        }
    }
    serde_json::json!({
        "distance_stats": distance_stats, "temporal_distribution": hourly,
        "top_days": top_days.iter().map(|(d,c)| serde_json::json!([d, c])).collect::<Vec<_>>(),
        "time_span_hours": r2f(time_span_hours), "total_neighbors": all_distances.len(),
        "neighbor_metrics": neighbor_metrics,
    })
}

/// Compute recording history stats: totals, streak, today vs 7-day average.
pub fn compute_status_history(
    skill_dir: &Path,
    now_utc: u64,
    sessions_json: &[serde_json::Value],
) -> serde_json::Value {
    if sessions_json.is_empty() { return serde_json::json!(null); }

    let today_day = now_utc / 86400;
    let mut total_secs = 0u64;
    let mut longest_secs = 0u64;
    let mut day_set = std::collections::BTreeSet::<u64>::new();
    let total_sessions = sessions_json.len();
    let mut total_epochs = 0u64;

    for s in sessions_json {
        let start = s["start_utc"].as_u64().unwrap_or(0);
        let end   = s["end_utc"].as_u64().unwrap_or(0);
        let n_ep  = s["n_epochs"].as_u64().unwrap_or(0);
        let dur   = end.saturating_sub(start);
        total_secs += dur;
        longest_secs = longest_secs.max(dur);
        total_epochs += n_ep;
        day_set.insert(start / 86400);
    }

    let recording_days = day_set.len();
    let total_hours = total_secs as f64 / 3600.0;
    let avg_session_min = if total_sessions > 0 { total_hours * 60.0 / total_sessions as f64 } else { 0.0 };

    let mut streak = 0u32;
    let mut check = today_day;
    loop {
        if day_set.contains(&check) { streak += 1; if check == 0 { break; } check -= 1; }
        else if check == today_day { if check == 0 { break; } check -= 1; }
        else { break; }
    }

    let today_start = today_day * 86400;
    let week_start  = today_day.saturating_sub(7) * 86400;
    let today_metrics = get_session_metrics(skill_dir, today_start, now_utc);
    let week_metrics  = get_session_metrics(skill_dir, week_start, now_utc);

    let mut today_vs_avg = serde_json::Map::new();
    if today_metrics.n_epochs > 0 && week_metrics.n_epochs > 0 {
        for &metric in STATUS_METRICS {
            let tv = session_field(&today_metrics, metric);
            let wv = session_field(&week_metrics, metric);
            let delta_pct = if wv.abs() > 1e-6 { (tv - wv) / wv.abs() * 100.0 } else { 0.0 };
            let direction = if delta_pct > 5.0 { "up" } else if delta_pct < -5.0 { "down" } else { "stable" };
            today_vs_avg.insert(metric.into(), serde_json::json!({
                "today": r2f(tv), "avg_7d": r2f(wv), "delta_pct": r2f(delta_pct), "direction": direction,
            }));
        }
    }
    serde_json::json!({
        "total_sessions": total_sessions, "total_recording_hours": r2f(total_hours),
        "total_epochs": total_epochs, "recording_days": recording_days,
        "current_streak_days": streak, "longest_session_min": r2f(longest_secs as f64 / 60.0),
        "avg_session_min": r2f(avg_session_min), "today_vs_avg": today_vs_avg,
    })
}
