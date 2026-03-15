// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Session history Tauri commands: listing, streaming, stats, deletion,
// and embedding-session discovery for the compare picker.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{AppState, MutexExt};
use crate::session_csv::{metrics_csv_path, ppg_csv_path};
use crate::label_store;

#[tauri::command]
pub(crate) async fn open_history_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("history") {
        let _ = win.unminimize(); let _ = win.show(); let _ = win.set_focus(); return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "history", tauri::WebviewUrl::App("history".into()))
        .title("NeuroSkill™ – History")
        .inner_size(920.0, 780.0)
        .min_inner_size(700.0, 560.0)
        .resizable(true)
        .center()
        .decorations(false).transparent(true)
        .build()
        .map(|w| { let _ = w.set_focus(); })
        .map_err(|e| e.to_string())
}

/// A session entry read from a JSON sidecar file.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct SessionEntry {
    csv_file:          String,
    csv_path:          String,
    session_start_utc: Option<u64>,
    session_end_utc:   Option<u64>,
    session_duration_s: Option<u64>,
    device_name:       Option<String>,
    device_id:         Option<String>,
    serial_number:     Option<String>,
    mac_address:       Option<String>,
    firmware_version:  Option<String>,
    hardware_version:  Option<String>,
    headset_preset:    Option<String>,
    battery_pct:       Option<f64>,
    total_samples:     Option<u64>,
    sample_rate_hz:    Option<u64>,
    labels:            Vec<label_store::LabelRow>,
    file_size_bytes:   u64,
}

/// Scan all `~/.skill/*/muse_*.json` sidecar files and return session entries
/// sorted by start time descending (newest first).
#[tauri::command]
pub(crate) fn list_sessions(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<SessionEntry> {
    let (skill_dir, logger) = {
        let s = state.lock_or_recover();
        (s.skill_dir.clone(), s.logger.clone())
    };

    skill_log!(logger, "history", "scanning {:?}", skill_dir);

    // 1. Scan all JSON sidecar files (no lock needed)
    let mut raw: Vec<(SessionEntry, Option<u64>, Option<u64>)> = Vec::new();

    let entries = match std::fs::read_dir(&skill_dir) {
        Ok(e) => e,
        Err(e) => { skill_log!(logger, "history", "read_dir failed: {e}"); return vec![]; },
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let dir_files = match std::fs::read_dir(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        // Collect all files in this date directory
        let files: Vec<_> = dir_files.filter_map(|e| e.ok()).collect();

        // First pass: find JSON sidecars
        for jf in &files {
            let jp = jf.path();
            let fname = jp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !fname.starts_with("muse_") || !fname.ends_with(".json") { continue; }

            let json_str = match std::fs::read_to_string(&jp) {
                Ok(s) => s,
                Err(e) => { skill_log!(logger, "history", "read error: {e}"); continue; },
            };
            let meta: serde_json::Value = match serde_json::from_str(&json_str) {
                Ok(v) => v,
                Err(e) => { skill_log!(logger, "history", "parse error: {e}"); continue; },
            };

            skill_log!(logger, "history", "sidecar {:?}: start={} end={} samples={}",
                jp,
                meta.get("session_start_utc").map(|v| v.to_string()).unwrap_or("null".into()),
                meta.get("session_end_utc").map(|v| v.to_string()).unwrap_or("null".into()),
                meta.get("total_samples").map(|v| v.to_string()).unwrap_or("null".into()),
            );

            let csv_file = meta["csv_file"].as_str().unwrap_or("").to_string();
            let csv_full = path.join(&csv_file);
            let csv_size = std::fs::metadata(&csv_full).map(|m| m.len()).unwrap_or(0);

            let start = meta["session_start_utc"].as_u64();
            let end   = meta["session_end_utc"].as_u64();

            // Support both old flat format and new nested "device" format.
            let dev = meta.get("device");
            let str_field = |obj: Option<&serde_json::Value>, nested_key: &str, flat_key: &str| -> Option<String> {
                obj.and_then(|d| d.get(nested_key)).and_then(|v| v.as_str()).map(str::to_owned)
                    .or_else(|| meta.get(flat_key).and_then(|v| v.as_str()).map(str::to_owned))
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

        // Second pass: find orphaned CSVs (no JSON sidecar)
        for cf in &files {
            let cp = cf.path();
            let cfname = cp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !cfname.starts_with("muse_") || !cfname.ends_with(".csv") { continue; }
            // Skip derived companion files — _metrics.csv and _ppg.csv are not sessions.
            if cfname.ends_with("_metrics.csv") || cfname.ends_with("_ppg.csv") { continue; }
            let json_path = cp.with_extension("json");
            if json_path.exists() { continue; } // already handled above

            let meta = std::fs::metadata(&cp);
            let csv_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            // Start time from filename: muse_{ts}.csv
            let ts: Option<u64> = cfname
                .strip_prefix("muse_")
                .and_then(|s| s.strip_suffix(".csv"))
                .and_then(|s| s.parse().ok());
            // End time from file modification time
            let end_ts: Option<u64> = meta.ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            // Estimate sample count from CSV line count (minus header)
            let samples: Option<u64> = std::fs::read_to_string(&cp).ok()
                .map(|s| {
                    let lines = s.lines().count();
                    if lines > 1 { (lines - 1) as u64 } else { 0 }
                });

            skill_log!(logger, "history", "found orphan CSV: {:?} start={:?} end={:?} samples={:?}", cp, ts, end_ts, samples);

            raw.push((SessionEntry {
                csv_file:           cfname.to_string(),
                csv_path:           cp.to_string_lossy().into_owned(),
                session_start_utc:  ts,
                session_end_utc:    end_ts,
                session_duration_s: ts.zip(end_ts).map(|(s, e)| e.saturating_sub(s)),
                device_name:        None,
                device_id:          None,
                serial_number:      None,
                mac_address:        None,
                firmware_version:   None,
                hardware_version:   None,
                headset_preset:     None,
                battery_pct:        None,
                total_samples:      samples,
                sample_rate_hz:     Some(256),
                labels:             vec![],
                file_size_bytes:    csv_size,
            }, ts, end_ts));
        }
    }

    // Override start/end/duration with ground-truth timestamps from _metrics.csv.
    patch_session_timestamps(&mut raw);

    // 2. Re-acquire lock briefly to query labels for each session
    {
        let s = state.lock_or_recover();
        if let Some(store) = &s.label_store {
            for (session, start, end) in raw.iter_mut() {
                if let (Some(s), Some(e)) = (start, end) {
                    session.labels = store.query_range(*s, *e);
                }
            }
        }
    }

    let mut sessions: Vec<SessionEntry> = raw.into_iter().map(|(s, _, _)| s).collect();
    sessions.sort_by(|a, b| b.session_start_utc.cmp(&a.session_start_utc));
    skill_log!(logger, "history", "returning {} sessions", sessions.len());
    sessions
}

// ── CSV timestamp helpers ─────────────────────────────────────────────────────

/// Read the first and last valid `timestamp_s` (column 0) from a
/// `_metrics.csv` file.  Used to fix session start/end/duration when the
/// JSON sidecar is missing or was written before the session ended cleanly
/// (e.g. after a crash).
///
/// Returns `Some((first_unix_secs, last_unix_secs))` or `None` if the file
/// does not exist or contains no valid rows.
fn read_metrics_csv_time_range(metrics_path: &std::path::Path) -> Option<(u64, u64)> {
    if !metrics_path.exists() { return None; }

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(metrics_path)
        .ok()?;

    let mut first: Option<u64> = None;
    let mut last:  Option<u64> = None;

    for result in rdr.records() {
        let rec = match result { Ok(r) => r, Err(_) => continue };
        // Column 0 is `timestamp_s` — a Unix-seconds float (e.g. 1700000047.123456)
        let ts = match rec.get(0).and_then(|s| s.parse::<f64>().ok()) {
            Some(t) if t > 1_000_000_000.0 => t as u64,   // sanity: after year 2001
            _ => continue,
        };
        if first.is_none() { first = Some(ts); }
        last = Some(ts);
    }

    Some((first?, last?))
}

/// Apply `_metrics.csv` timestamps to a list of `(SessionEntry, start, end)`
/// triples.  Overwrites `session_start_utc`, `session_end_utc`, and
/// `session_duration_s` whenever the metrics file provides tighter, verified
/// bounds.  The `start`/`end` references must stay in sync because they are
/// used downstream for label hydration.
fn patch_session_timestamps(raw: &mut [(SessionEntry, Option<u64>, Option<u64>)]) {
    for (session, start, end) in raw.iter_mut() {
        let metrics_path = metrics_csv_path(std::path::Path::new(&session.csv_path));
        if let Some((first_ts, last_ts)) = read_metrics_csv_time_range(&metrics_path) {
            *start                     = Some(first_ts);
            *end                       = Some(last_ts);
            session.session_start_utc  = Some(first_ts);
            session.session_end_utc    = Some(last_ts);
            session.session_duration_s = Some(last_ts.saturating_sub(first_ts));
        }
    }
}

/// Return recording day directories as `YYYYMMDD` strings, newest first.
///
/// Only directories that contain at least one valid session file are returned:
///   • a `muse_*.json` sidecar, OR
///   • an orphaned `muse_*.csv` with no matching sidecar
///
/// This filters out dirs that hold only log files (or are completely empty),
/// so the frontend never has to handle an "empty day" as the default view.
#[tauri::command]
pub(crate) fn list_session_days(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<String> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    let mut days: Vec<String> = std::fs::read_dir(&skill_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            if !(s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) && e.path().is_dir()) {
                return None;
            }
            // Check for at least one valid session file before including this day.
            let has_sessions = std::fs::read_dir(e.path())
                .into_iter()
                .flatten()
                .flatten()
                .any(|f| {
                    let fname = f.file_name();
                    let fname = fname.to_string_lossy();
                    if fname.starts_with("muse_") && fname.ends_with(".json") {
                        return true; // JSON sidecar
                    }
                    if fname.starts_with("muse_") && fname.ends_with(".csv") {
                        // Skip derived companion files — _metrics.csv and _ppg.csv are not sessions.
                        if fname.ends_with("_metrics.csv") || fname.ends_with("_ppg.csv") {
                            return false;
                        }
                        // Orphaned CSV — only counts if there is no sidecar
                        return !f.path().with_extension("json").exists();
                    }
                    false
                });
            if has_sessions { Some(s.to_string()) } else { None }
        })
        .collect();
    days.sort_by(|a, b| b.cmp(a)); // newest first
    days
}

/// Load all sessions belonging to a single recording day (`YYYYMMDD`).
/// This is the async-friendly counterpart to `list_sessions` — callers
/// iterate over `list_session_days()` and invoke this for each day in turn.
#[tauri::command]
pub(crate) fn list_sessions_for_day(
    day: String,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<SessionEntry> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    let day_dir = skill_dir.join(&day);
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
        // Skip derived companion files — _metrics.csv and _ppg.csv are not sessions.
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

    // Override start/end/duration with ground-truth timestamps from _metrics.csv.
    // This fixes orphaned CSVs (where end = unreliable mtime) and sessions whose
    // sidecar was only written at session-start (app crashed before clean shutdown).
    patch_session_timestamps(&mut raw);

    // Hydrate labels
    {
        let s = state.lock_or_recover();
        if let Some(store) = &s.label_store {
            for (session, start, end) in raw.iter_mut() {
                if let (Some(s), Some(e)) = (start, end) {
                    session.labels = store.query_range(*s, *e);
                }
            }
        }
    }

    let mut sessions: Vec<SessionEntry> = raw.into_iter().map(|(s, _, _)| s).collect();
    sessions.sort_by(|a, b| b.session_start_utc.cmp(&a.session_start_utc));
    sessions
}

// ── Streaming session list ────────────────────────────────────────────────────

/// An event emitted by `stream_sessions`.
///
/// Sequence:
/// 1. `{ kind:"started", total_days:N }`
/// 2. N × `{ kind:"day",  day:"YYYYMMDD", sessions:[…] }`
/// 3. `{ kind:"done",  total_sessions:N }`
#[derive(Serialize, Clone)]
pub(crate) struct SessionStreamEvent {
    kind:           String,
    /// "started" only
    #[serde(skip_serializing_if = "Option::is_none")]
    total_days:     Option<usize>,
    /// "day" only
    #[serde(skip_serializing_if = "Option::is_none")]
    day:            Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sessions:       Option<Vec<SessionEntry>>,
    /// "done" only
    #[serde(skip_serializing_if = "Option::is_none")]
    total_sessions: Option<usize>,
}

/// Stream all recorded sessions to the frontend one day at a time.
///
/// Each channel message is a `SessionStreamEvent`.  All file I/O runs on a
/// Tokio blocking thread so the async runtime and the UI are never stalled.
#[tauri::command]
pub(crate) async fn stream_sessions(
    on_event: tauri::ipc::Channel<SessionStreamEvent>,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();

    tokio::task::spawn_blocking(move || {
        // ── 1. Enumerate day directories ─────────────────────────────────
        let mut days: Vec<String> = std::fs::read_dir(&skill_dir)
            .into_iter().flatten().flatten()
            .filter_map(|e| {
                let name = e.file_name();
                let s    = name.to_string_lossy();
                if s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit())
                   && e.path().is_dir()
                {
                    Some(s.to_string())
                } else {
                    None
                }
            })
            .collect();
        days.sort_by(|a, b| b.cmp(a)); // newest first

        let _ = on_event.send(SessionStreamEvent {
            kind:           "started".into(),
            total_days:     Some(days.len()),
            day:            None,
            sessions:       None,
            total_sessions: None,
        });

        // Open a read-only label store on this thread for label hydration.
        let label_store = label_store::LabelStore::open(&skill_dir);

        // ── 2. Load each day and emit one event per day ───────────────────
        let mut total_sessions = 0usize;
        for day in &days {
            let day_dir = skill_dir.join(day);
            if !day_dir.is_dir() { continue; }

            let files: Vec<_> = match std::fs::read_dir(&day_dir) {
                Ok(rd) => rd.flatten().collect(),
                Err(_) => continue,
            };
            let mut raw: Vec<(SessionEntry, Option<u64>, Option<u64>)> = Vec::new();

            // JSON sidecars
            for jf in &files {
                let jp    = jf.path();
                let fname = jp.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !fname.starts_with("muse_") || !fname.ends_with(".json") { continue; }

                let json_str = match std::fs::read_to_string(&jp) { Ok(s) => s, Err(_) => continue };
                let meta: serde_json::Value = match serde_json::from_str(&json_str) { Ok(v) => v, Err(_) => continue };

                let csv_file = meta["csv_file"].as_str().unwrap_or("").to_string();
                let csv_full = day_dir.join(&csv_file);
                let csv_size = std::fs::metadata(&csv_full).map(|m| m.len()).unwrap_or(0);
                let start    = meta["session_start_utc"].as_u64();
                let end      = meta["session_end_utc"].as_u64();
                let dev      = meta.get("device");
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
                    device_name:        str_field(dev, "name",             "device_name"),
                    device_id:          str_field(dev, "id",               "device_id"),
                    serial_number:      str_field(dev, "serial_number",    "serial_number"),
                    mac_address:        str_field(dev, "mac_address",      "mac_address"),
                    firmware_version:   str_field(dev, "firmware_version", "firmware_version"),
                    hardware_version:   str_field(dev, "hardware_version", "hardware_version"),
                    headset_preset:     str_field(dev, "preset",           "headset_preset"),
                    battery_pct:        meta.get("battery_pct_end").and_then(|v| v.as_f64())
                                            .or_else(|| meta.get("battery_pct").and_then(|v| v.as_f64())),
                    total_samples:      meta["total_samples"].as_u64(),
                    sample_rate_hz:     meta["sample_rate_hz"].as_u64(),
                    labels:             vec![],
                    file_size_bytes:    csv_size,
                }, start, end));
            }

            // Orphaned CSVs (no sidecar JSON)
            for cf in &files {
                let cp    = cf.path();
                let cfname = cp.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !cfname.starts_with("muse_") || !cfname.ends_with(".csv") { continue; }
                // Skip derived companion files — _metrics.csv and _ppg.csv are not sessions.
                if cfname.ends_with("_metrics.csv") || cfname.ends_with("_ppg.csv") { continue; }
                if cp.with_extension("json").exists() { continue; }
                let meta_fs  = std::fs::metadata(&cp);
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

            // Override start/end/duration with ground-truth timestamps from _metrics.csv.
            patch_session_timestamps(&mut raw);

            // Hydrate labels
            if let Some(store) = &label_store {
                for (session, start, end) in raw.iter_mut() {
                    if let (Some(s), Some(e)) = (start, end) {
                        session.labels = store.query_range(*s, *e);
                    }
                }
            }

            let mut sessions: Vec<SessionEntry> = raw.into_iter().map(|(s, _, _)| s).collect();
            sessions.sort_by(|a, b| b.session_start_utc.cmp(&a.session_start_utc));
            total_sessions += sessions.len();

            let _ = on_event.send(SessionStreamEvent {
                kind:           "day".into(),
                total_days:     None,
                day:            Some(day.clone()),
                sessions:       Some(sessions),
                total_sessions: None,
            });
        }

        // ── 3. Signal completion ──────────────────────────────────────────
        let _ = on_event.send(SessionStreamEvent {
            kind:           "done".into(),
            total_days:     None,
            day:            None,
            sessions:       None,
            total_sessions: Some(total_sessions),
        });
    })
    .await
    .map_err(|e| e.to_string())
}

/// Aggregate history stats — total sessions/hours and week-over-week breakdown.
/// Scans only JSON sidecars (fast), never reads CSV data.
#[derive(Serialize)]
pub(crate) struct HistoryStats {
    total_sessions: usize,
    total_secs:     u64,
    this_week_secs: u64,
    last_week_secs: u64,
}

#[tauri::command]
pub(crate) async fn get_history_stats(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<HistoryStats, ()> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    Ok(tokio::task::spawn_blocking(move || {
        // Week boundaries (Monday 00:00 UTC).
        // Jan 1, 1970 was a Thursday; with Mon=0 the offset is +3.
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let days_since_epoch = now_secs / 86400;
        let weekday           = (days_since_epoch + 3) % 7; // 0=Mon … 6=Sun
        let this_week_start   = (days_since_epoch - weekday) * 86400;
        let last_week_start   = this_week_start.saturating_sub(7 * 86400);

        let mut total_sessions = 0usize;
        let mut total_secs     = 0u64;
        let mut this_week_secs = 0u64;
        let mut last_week_secs = 0u64;

        let day_dirs = std::fs::read_dir(&skill_dir)
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
    })
    .await
    .unwrap_or(HistoryStats { total_sessions: 0, total_secs: 0,
                               this_week_secs: 0, last_week_secs: 0 }))
}

/// Delete a session's CSV + JSON sidecar files.
#[tauri::command]
pub(crate) fn delete_session(csv_path: String) -> Result<(), String> {
    let csv = std::path::PathBuf::from(&csv_path);
    let json = csv.with_extension("json");
    let ppg  = ppg_csv_path(&csv);
    let met  = metrics_csv_path(&csv);
    if csv.exists()  { std::fs::remove_file(&csv).map_err(|e| e.to_string())?; }
    if json.exists() { std::fs::remove_file(&json).map_err(|e| e.to_string())?; }
    if ppg.exists()  { std::fs::remove_file(&ppg).map_err(|e| e.to_string())?; }
    if met.exists()  { std::fs::remove_file(&met).map_err(|e| e.to_string())?; }
    Ok(())
}

// ── Embedding sessions (for compare picker) ──────────────────────────────────

/// One contiguous recording range discovered from embedding timestamps.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct EmbeddingSession {
    start_utc: u64,
    end_utc:   u64,
    n_epochs:  u64,
    /// YYYYMMDD directory the epochs live in (may span multiple days).
    day:       String,
}

/// Scan every `YYYYMMDD/eeg.sqlite` and return distinct recording sessions.
///
/// Two consecutive embeddings are considered part of the same session if they
/// are ≤ `GAP_SECS` apart (default 120 s — two minutes without data starts a
/// new session).  This makes the picker independent of CSV sidecar files.
#[tauri::command]
pub(crate) fn list_embedding_sessions(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<EmbeddingSession> {
    const GAP_SECS: u64 = 120;

    let skill_dir = state.lock_or_recover().skill_dir.clone();

    // Collect (utc_seconds, day_label) from every database.
    let mut all_ts: Vec<(u64, String)> = Vec::new();

    let entries = match std::fs::read_dir(&skill_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let day_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        // Only YYYYMMDD directories
        if day_name.len() != 8 || !day_name.bytes().all(|b| b.is_ascii_digit()) { continue; }

        let db_path = path.join(crate::constants::SQLITE_FILE);
        if !db_path.exists() { continue; }

        let conn = match rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(_) => continue,
        };
        // Allow up to 2 s for a write-locked DB (e.g. active recording) before giving up.
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");

        // Read all epoch timestamps as ISO-ish strings, convert to unix secs.
        let mut stmt = match conn.prepare("SELECT timestamp FROM embeddings ORDER BY timestamp") {
            Ok(s) => s,
            Err(_) => continue,
        };
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0));
        if let Ok(rows) = rows {
            for row in rows.filter_map(|r| r.ok()) {
                let utc = crate::commands::ts_to_unix(row);
                all_ts.push((utc, day_name.clone()));
            }
        }
    }

    if all_ts.is_empty() { return vec![]; }

    // Sort globally by timestamp.
    all_ts.sort_by_key(|(ts, _)| *ts);

    // Split into sessions using the gap threshold.
    let mut sessions: Vec<EmbeddingSession> = Vec::new();
    let mut start = all_ts[0].0;
    let mut end   = start;
    let mut count: u64 = 1;
    let mut day   = all_ts[0].1.clone();

    for &(ts, ref d) in &all_ts[1..] {
        if ts.saturating_sub(end) > GAP_SECS {
            // Flush current session
            sessions.push(EmbeddingSession { start_utc: start, end_utc: end, n_epochs: count, day: day.clone() });
            start = ts;
            end   = ts;
            count = 1;
            day   = d.clone();
        } else {
            end = ts;
            count += 1;
        }
    }
    // Flush last
    sessions.push(EmbeddingSession { start_utc: start, end_utc: end, n_epochs: count, day: day.clone() });

    // Return newest first
    sessions.reverse();
    sessions
}

pub(crate) fn find_session_csv_for_timestamp(skill_dir: &std::path::Path, ts_utc: u64) -> Option<String> {
    let mut containing: Option<String> = None;
    let mut nearest: Option<(u64, String)> = None;

    let entries = std::fs::read_dir(skill_dir).ok()?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }

        let files = match std::fs::read_dir(&path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for file in files.filter_map(|e| e.ok()) {
            let jp = file.path();
            let fname = jp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !fname.starts_with("muse_") || !fname.ends_with(".json") { continue; }

            let json = match std::fs::read_to_string(&jp) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let meta: serde_json::Value = match serde_json::from_str(&json) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let start = meta["session_start_utc"].as_u64();
            let end = meta["session_end_utc"].as_u64().or(start);
            let csv_file = meta["csv_file"].as_str().unwrap_or("");
            if csv_file.is_empty() { continue; }
            let csv_path = path.join(csv_file).to_string_lossy().into_owned();

            if let (Some(s), Some(e)) = (start, end) {
                if ts_utc >= s && ts_utc <= e {
                    containing = Some(csv_path);
                    break;
                }
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

