// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Disk cache for session metrics — avoids recomputing from CSV on every load.

use std::collections::HashMap;
use std::path::Path;

// metrics_csv_path kept for backward compat if needed; find_metrics_path handles both formats.

use super::{
    find_metrics_path, load_metrics_csv, CsvMetricsResult, EpochRow, SessionMetrics, SleepEpoch, SleepStages,
    SleepSummary,
};

// ── Disk cache ────────────────────────────────────────────────────────────────

/// Cache file path: `exg_XXX.csv` → `exg_XXX_metrics_cache.json`
fn metrics_cache_path(csv_path: &Path) -> std::path::PathBuf {
    let stem = csv_path.file_stem().and_then(|s| s.to_str()).unwrap_or("exg");
    csv_path.with_file_name(format!("{stem}_metrics_cache.json"))
}

/// Load metrics from disk cache if valid, otherwise compute from data file and cache.
pub fn load_csv_metrics_cached(csv_path: &Path) -> Option<CsvMetricsResult> {
    let metrics_file = find_metrics_path(csv_path);
    let metrics_file = metrics_file?;

    let cache_path = metrics_cache_path(csv_path);

    if cache_path.exists() {
        let csv_mtime = std::fs::metadata(&metrics_file).ok().and_then(|m| m.modified().ok());
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
///
/// Uses in-place swaps to avoid cloning the large `EpochRow` structs.
pub fn downsample_timeseries(ts: &mut Vec<EpochRow>, max: usize) {
    let n = ts.len();
    if n <= max || max < 2 {
        return;
    }
    let step = (n - 1) as f64 / (max - 1) as f64;
    for i in 0..max {
        let src = (i as f64 * step).round() as usize;
        let src = src.min(n - 1);
        if src != i {
            ts.swap(i, src);
        }
    }
    ts.truncate(max);
}

/// Batch-load metrics for multiple sessions.
pub fn get_day_metrics_batch(csv_paths: &[String], max_ts_points: usize) -> HashMap<String, CsvMetricsResult> {
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
    // Index on timestamp — critical for range queries (WHERE timestamp >= ? AND timestamp <= ?).
    // Without it every query does a full table scan on 100K+ rows.
    let _ = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_embeddings_timestamp ON embeddings(timestamp)",
        [],
    );
}

/// Return only subdirectories of `skill_dir` whose `YYYYMMDD` name overlaps
/// the given UTC timestamp range.  Adds ±1 day of padding to account for
/// timezone differences between the directory name (UTC midnight) and the
/// user's local calendar day.
fn dirs_for_range(skill_dir: &Path, start_utc: u64, end_utc: u64) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(skill_dir) else {
        return Vec::new();
    };
    let range_start = start_utc.saturating_sub(86400);
    let range_end = end_utc + 86400;
    let mut dirs = Vec::new();
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.len() == 8 {
                if let (Ok(y), Ok(m), Ok(d)) = (
                    name[0..4].parse::<i32>(),
                    name[4..6].parse::<u32>(),
                    name[6..8].parse::<u32>(),
                ) {
                    let dir_utc = super::local_days::ymd_to_days(y, m, d) as u64 * 86400;
                    let dir_end = dir_utc + 86400;
                    if dir_end >= range_start && dir_utc <= range_end {
                        dirs.push(path);
                    }
                    continue;
                }
            }
        }
        // Non-YYYYMMDD directory — include it (could contain data).
        dirs.push(path);
    }
    dirs
}

/// Serde helper: deserialize a single metrics_json blob into an EpochRow.
#[derive(serde::Deserialize, Default)]
struct MetricsBlob {
    #[serde(default)]
    rel_delta: f64,
    #[serde(default)]
    rel_theta: f64,
    #[serde(default)]
    rel_alpha: f64,
    #[serde(default)]
    rel_beta: f64,
    #[serde(default)]
    rel_gamma: f64,
    #[serde(default)]
    relaxation_score: f64,
    #[serde(default)]
    engagement_score: f64,
    #[serde(default)]
    faa: f64,
    #[serde(default)]
    tar: f64,
    #[serde(default)]
    bar: f64,
    #[serde(default)]
    dtr: f64,
    #[serde(default)]
    pse: f64,
    #[serde(default)]
    apf: f64,
    #[serde(default)]
    bps: f64,
    #[serde(default)]
    snr: f64,
    #[serde(default)]
    coherence: f64,
    #[serde(default)]
    mu_suppression: f64,
    #[serde(default)]
    mood: f64,
    #[serde(default)]
    tbr: f64,
    #[serde(default)]
    sef95: f64,
    #[serde(default)]
    spectral_centroid: f64,
    #[serde(default)]
    hjorth_activity: f64,
    #[serde(default)]
    hjorth_mobility: f64,
    #[serde(default)]
    hjorth_complexity: f64,
    #[serde(default)]
    permutation_entropy: f64,
    #[serde(default)]
    higuchi_fd: f64,
    #[serde(default)]
    dfa_exponent: f64,
    #[serde(default)]
    sample_entropy: f64,
    #[serde(default)]
    pac_theta_gamma: f64,
    #[serde(default)]
    laterality_index: f64,
    #[serde(default)]
    hr: f64,
    #[serde(default)]
    rmssd: f64,
    #[serde(default)]
    sdnn: f64,
    #[serde(default)]
    pnn50: f64,
    #[serde(default)]
    lf_hf_ratio: f64,
    #[serde(default)]
    respiratory_rate: f64,
    #[serde(default)]
    spo2_estimate: f64,
    #[serde(default)]
    perfusion_idx: f64,
    #[serde(default)]
    stress_index: f64,
    #[serde(default)]
    blink_count: f64,
    #[serde(default)]
    blink_rate: f64,
    #[serde(default)]
    head_pitch: f64,
    #[serde(default)]
    head_roll: f64,
    #[serde(default)]
    stillness: f64,
    #[serde(default)]
    nod_count: f64,
    #[serde(default)]
    shake_count: f64,
    #[serde(default)]
    meditation: f64,
    #[serde(default)]
    cognitive_load: f64,
    #[serde(default)]
    drowsiness: f64,
}

impl MetricsBlob {
    fn to_epoch_row(&self, utc: f64) -> EpochRow {
        EpochRow {
            t: utc,
            rd: self.rel_delta,
            rt: self.rel_theta,
            ra: self.rel_alpha,
            rb: self.rel_beta,
            rg: self.rel_gamma,
            relaxation: self.relaxation_score,
            engagement: self.engagement_score,
            faa: self.faa,
            tar: self.tar,
            bar: self.bar,
            dtr: self.dtr,
            pse: self.pse,
            apf: self.apf,
            bps: self.bps,
            snr: self.snr,
            coherence: self.coherence,
            mu: self.mu_suppression,
            mood: self.mood,
            tbr: self.tbr,
            sef95: self.sef95,
            sc: self.spectral_centroid,
            ha: self.hjorth_activity,
            hm: self.hjorth_mobility,
            hc: self.hjorth_complexity,
            pe: self.permutation_entropy,
            hfd: self.higuchi_fd,
            dfa: self.dfa_exponent,
            se: self.sample_entropy,
            pac: self.pac_theta_gamma,
            lat: self.laterality_index,
            hr: self.hr,
            rmssd: self.rmssd,
            sdnn: self.sdnn,
            pnn50: self.pnn50,
            lf_hf: self.lf_hf_ratio,
            resp: self.respiratory_rate,
            spo2: self.spo2_estimate,
            perf: self.perfusion_idx,
            stress: self.stress_index,
            blinks: self.blink_count,
            blink_r: self.blink_rate,
            pitch: self.head_pitch,
            roll: self.head_roll,
            still: self.stillness,
            nods: self.nod_count,
            shakes: self.shake_count,
            med: self.meditation,
            cog: self.cognitive_load,
            drow: self.drowsiness,
            gpu: 0.0,
            gpu_render: 0.0,
            gpu_tiler: 0.0,
        }
    }

    fn accumulate_into(&self, total: &mut SessionMetrics) {
        total.rel_delta += self.rel_delta;
        total.rel_theta += self.rel_theta;
        total.rel_alpha += self.rel_alpha;
        total.rel_beta += self.rel_beta;
        total.rel_gamma += self.rel_gamma;
        total.relaxation += self.relaxation_score;
        total.engagement += self.engagement_score;
        total.faa += self.faa;
        total.tar += self.tar;
        total.bar += self.bar;
        total.dtr += self.dtr;
        total.pse += self.pse;
        total.apf += self.apf;
        total.bps += self.bps;
        total.snr += self.snr;
        total.coherence += self.coherence;
        total.mu_suppression += self.mu_suppression;
        total.mood += self.mood;
        total.tbr += self.tbr;
        total.sef95 += self.sef95;
        total.spectral_centroid += self.spectral_centroid;
        total.hjorth_activity += self.hjorth_activity;
        total.hjorth_mobility += self.hjorth_mobility;
        total.hjorth_complexity += self.hjorth_complexity;
        total.permutation_entropy += self.permutation_entropy;
        total.higuchi_fd += self.higuchi_fd;
        total.dfa_exponent += self.dfa_exponent;
        total.sample_entropy += self.sample_entropy;
        total.pac_theta_gamma += self.pac_theta_gamma;
        total.laterality_index += self.laterality_index;
        total.hr += self.hr;
        total.rmssd += self.rmssd;
        total.sdnn += self.sdnn;
        total.pnn50 += self.pnn50;
        total.lf_hf_ratio += self.lf_hf_ratio;
        total.respiratory_rate += self.respiratory_rate;
        total.spo2_estimate += self.spo2_estimate;
        total.perfusion_index += self.perfusion_idx;
        total.stress_index += self.stress_index;
        total.blink_count += self.blink_count;
        total.blink_rate += self.blink_rate;
        total.head_pitch += self.head_pitch;
        total.head_roll += self.head_roll;
        total.stillness += self.stillness;
        total.nod_count += self.nod_count;
        total.shake_count += self.shake_count;
        total.meditation += self.meditation;
        total.cognitive_load += self.cognitive_load;
        total.drowsiness += self.drowsiness;
    }
}

/// Maximum number of epoch rows returned for timeseries charts.
/// The compare UI downsamples to ~400 columns anyway, so returning more
/// wastes bandwidth, memory, and serde time.
const TIMESERIES_MAX_ROWS: usize = 800;

/// Return per-epoch time-series data for a session range (from SQLite).
///
/// Reads `metrics_json` as a single TEXT column and deserializes once in Rust.
/// When the total row count exceeds `TIMESERIES_MAX_ROWS`, does a fast COUNT
/// first and then reads every Nth row to produce an evenly-spaced sample.
pub fn get_session_timeseries(skill_dir: &Path, start_utc: u64, end_utc: u64) -> Vec<EpochRow> {
    // Epoch timestamps use two formats in the DB:
    // - Unix milliseconds (e.g. 1775512050594)
    // - YYYYMMDDHHmmss × 1000 (e.g. 20260413234815000)
    let r = skill_data::util::DualTimestampRange::from_unix_secs(start_utc, end_utc);
    let mut rows: Vec<EpochRow> = Vec::new();

    for path in dirs_for_range(skill_dir, start_utc, end_utc) {
        let db_path = path.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }

        let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        migrate_embeddings_schema(&conn);

        // Fast count to decide whether to downsample.
        let total_in_db: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM embeddings WHERE {}",
                    skill_data::util::DualTimestampRange::WHERE_CLAUSE
                ),
                rusqlite::params![
                    r.unix_ms_start,
                    r.unix_ms_end,
                    r.dt14_start,
                    r.dt14_end,
                    r.dt17_start,
                    r.dt17_end
                ],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let total_in_db = total_in_db.max(0) as usize;

        if total_in_db == 0 {
            continue;
        }

        // Determine step for downsampling.  We budget TIMESERIES_MAX_ROWS
        // across all day-DBs proportionally, but at minimum take every row
        // if under budget.
        let budget = TIMESERIES_MAX_ROWS.saturating_sub(rows.len()).max(2);
        let step = if total_in_db > budget { total_in_db / budget } else { 1 };

        // Use a CTE with ROW_NUMBER to sample every Nth row inside SQLite,
        // avoiding transfer of rows we'd discard.
        let where_cl = skill_data::util::DualTimestampRange::WHERE_CLAUSE;
        let query = if step > 1 {
            format!(
                "SELECT timestamp, metrics_json FROM (
                   SELECT timestamp, metrics_json,
                          (ROW_NUMBER() OVER (ORDER BY timestamp)) AS rn
                   FROM embeddings
                   WHERE {where_cl}
                 ) WHERE rn % {step} = 1
                 ORDER BY timestamp ASC"
            )
        } else {
            format!(
                "SELECT timestamp, metrics_json
                 FROM embeddings
                 WHERE {where_cl}
                 ORDER BY timestamp ASC"
            )
        };

        let Ok(mut stmt) = conn.prepare(&query) else {
            continue;
        };

        let iter = stmt.query_map(
            rusqlite::params![
                r.unix_ms_start,
                r.unix_ms_end,
                r.dt14_start,
                r.dt14_end,
                r.dt17_start,
                r.dt17_end
            ],
            |row| {
                let ts_val: i64 = row.get(0)?;
                let json_str: Option<String> = row.get(1)?;
                Ok((ts_val, json_str))
            },
        );

        if let Ok(iter) = iter {
            for pair in iter.filter_map(std::result::Result::ok) {
                let (ts_val, json_str) = pair;
                let utc = (ts_val / 1000) as f64;
                let blob: MetricsBlob = json_str
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();
                rows.push(blob.to_epoch_row(utc));
            }
        }
    }
    rows.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    rows
}

/// Query aggregated band-power metrics from SQLite databases.
///
/// Reads `metrics_json` as raw TEXT and deserializes once per row in Rust.
/// This is ~12× faster than SQL `AVG(json_extract(...))` which re-parses
/// the JSON blob 50 times per row inside SQLite.
pub fn get_session_metrics(skill_dir: &Path, start_utc: u64, end_utc: u64) -> SessionMetrics {
    let r = skill_data::util::DualTimestampRange::from_unix_secs(start_utc, end_utc);

    let mut total = SessionMetrics::default();
    let mut count = 0u64;

    for path in dirs_for_range(skill_dir, start_utc, end_utc) {
        let db_path = path.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }

        let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        migrate_embeddings_schema(&conn);

        let Ok(mut stmt) = conn.prepare(&format!(
            "SELECT metrics_json FROM embeddings WHERE ({}) AND metrics_json IS NOT NULL",
            skill_data::util::DualTimestampRange::WHERE_CLAUSE
        )) else {
            continue;
        };

        let rows = stmt.query_map(
            rusqlite::params![
                r.unix_ms_start,
                r.unix_ms_end,
                r.dt14_start,
                r.dt14_end,
                r.dt17_start,
                r.dt17_end
            ],
            |row| row.get::<_, String>(0),
        );

        if let Ok(rows) = rows {
            for json_str in rows.filter_map(std::result::Result::ok) {
                let blob: MetricsBlob = match serde_json::from_str(&json_str) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                if blob.rel_delta == 0.0 && blob.rel_theta == 0.0 {
                    continue;
                }
                blob.accumulate_into(&mut total);
                count += 1;
            }
        }
    }

    if count > 0 {
        let n = count as f64;
        total.rel_delta /= n;
        total.rel_theta /= n;
        total.rel_alpha /= n;
        total.rel_beta /= n;
        total.rel_gamma /= n;
        total.rel_high_gamma /= n;
        total.relaxation /= n;
        total.engagement /= n;
        total.faa /= n;
        total.tar /= n;
        total.bar /= n;
        total.dtr /= n;
        total.tbr /= n;
        total.pse /= n;
        total.apf /= n;
        total.bps /= n;
        total.snr /= n;
        total.coherence /= n;
        total.mu_suppression /= n;
        total.mood /= n;
        total.sef95 /= n;
        total.spectral_centroid /= n;
        total.hjorth_activity /= n;
        total.hjorth_mobility /= n;
        total.hjorth_complexity /= n;
        total.permutation_entropy /= n;
        total.higuchi_fd /= n;
        total.dfa_exponent /= n;
        total.sample_entropy /= n;
        total.pac_theta_gamma /= n;
        total.laterality_index /= n;
        total.hr /= n;
        total.rmssd /= n;
        total.sdnn /= n;
        total.pnn50 /= n;
        total.lf_hf_ratio /= n;
        total.respiratory_rate /= n;
        total.spo2_estimate /= n;
        total.perfusion_index /= n;
        total.stress_index /= n;
        total.blink_count /= n;
        total.blink_rate /= n;
        total.head_pitch /= n;
        total.head_roll /= n;
        total.stillness /= n;
        total.nod_count /= n;
        total.shake_count /= n;
        total.meditation /= n;
        total.cognitive_load /= n;
        total.drowsiness /= n;
        total.n_epochs = count as usize;
    }
    total
}

// ═══════════════════════════════════════════════════════════════════════════════
// Sleep staging
// ═══════════════════════════════════════════════════════════════════════════════

/// Classify each embedding epoch in `[start_utc, end_utc]` into a sleep stage.
pub fn get_sleep_stages(skill_dir: &Path, start_utc: u64, end_utc: u64) -> SleepStages {
    let r = skill_data::util::DualTimestampRange::from_unix_secs(start_utc, end_utc);

    struct RawEpoch {
        utc: u64,
        rd: f64,
        rt: f64,
        ra: f64,
        rb: f64,
    }
    let mut raw: Vec<RawEpoch> = Vec::new();

    for path in dirs_for_range(skill_dir, start_utc, end_utc) {
        let db_path = path.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }
        let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        let _ = conn.execute_batch("PRAGMA busy_timeout=2000;");
        let Ok(mut stmt) = conn.prepare(&format!(
            "SELECT timestamp, metrics_json FROM embeddings WHERE {} ORDER BY timestamp",
            skill_data::util::DualTimestampRange::WHERE_CLAUSE
        )) else {
            continue;
        };
        let rows = stmt.query_map(
            rusqlite::params![
                r.unix_ms_start,
                r.unix_ms_end,
                r.dt14_start,
                r.dt14_end,
                r.dt17_start,
                r.dt17_end
            ],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
        );
        if let Ok(rows) = rows {
            for row in rows.filter_map(std::result::Result::ok) {
                let (ts, json_str) = row;
                let blob: MetricsBlob = json_str
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();
                if blob.rel_delta == 0.0 && blob.rel_theta == 0.0 {
                    continue;
                }
                raw.push(RawEpoch {
                    utc: (ts / 1000) as u64,
                    rd: blob.rel_delta,
                    rt: blob.rel_theta,
                    ra: blob.rel_alpha,
                    rb: blob.rel_beta,
                });
            }
        }
    }
    raw.sort_by_key(|e| e.utc);

    let mut summary = SleepSummary::default();
    let epochs: Vec<SleepEpoch> = raw
        .iter()
        .map(|e| {
            let stage = classify_sleep(e.rd, e.rt, e.ra, e.rb);
            match stage {
                0 => summary.wake_epochs += 1,
                1 => summary.n1_epochs += 1,
                2 => summary.n2_epochs += 1,
                3 => summary.n3_epochs += 1,
                5 => summary.rem_epochs += 1,
                _ => {}
            }
            SleepEpoch {
                utc: e.utc,
                stage,
                rel_delta: e.rd,
                rel_theta: e.rt,
                rel_alpha: e.ra,
                rel_beta: e.rb,
            }
        })
        .collect();

    summary.total_epochs = epochs.len();
    if epochs.len() >= 2 {
        let mut gaps: Vec<f64> = epochs
            .windows(2)
            .map(|w| (w[1].utc as f64) - (w[0].utc as f64))
            .filter(|g| *g > 0.0 && *g < 30.0)
            .collect();
        if !gaps.is_empty() {
            gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            summary.epoch_secs = gaps[gaps.len() / 2];
        } else {
            summary.epoch_secs = 2.5;
        }
    } else {
        summary.epoch_secs = 2.5;
    }

    SleepStages { epochs, summary }
}

fn classify_sleep(rd: f64, rt: f64, ra: f64, rb: f64) -> u8 {
    if ra > 0.30 || rb > 0.30 {
        return 0;
    }
    if rt > 0.30 && ra < 0.15 && rd < 0.45 {
        return 5;
    }
    if rd > 0.50 {
        return 3;
    }
    if rt > 0.25 && rd < 0.50 {
        return 1;
    }
    2
}

// ═══════════════════════════════════════════════════════════════════════════════
// Analysis helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn r2f(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn linear_slope(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 2 {
        return 0.0;
    }
    let x_mean = (n - 1) as f64 / 2.0;
    let y_mean = values.iter().sum::<f64>() / n as f64;
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for (i, &y) in values.iter().enumerate() {
        let dx = i as f64 - x_mean;
        num += dx * (y - y_mean);
        den += dx * dx;
    }
    if den.abs() < 1e-15 {
        0.0
    } else {
        num / den
    }
}

fn metric_stats_vec(values: &[f64]) -> serde_json::Value {
    if values.is_empty() {
        return serde_json::json!(null);
    }
    let n = values.len();
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean = values.iter().sum::<f64>() / n as f64;
    let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
    let stddev = variance.sqrt();
    let median = if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    };
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
        "relaxation" => row.relaxation,
        "engagement" => row.engagement,
        "faa" => row.faa,
        "tar" => row.tar,
        "bar" => row.bar,
        "dtr" => row.dtr,
        "tbr" => row.tbr,
        "mood" => row.mood,
        "hr" => row.hr,
        "rmssd" => row.rmssd,
        "sdnn" => row.sdnn,
        "stress" => row.stress,
        "snr" => row.snr,
        "coherence" => row.coherence,
        "stillness" => row.still,
        "blink_rate" => row.blink_r,
        "meditation" => row.med,
        "cognitive_load" => row.cog,
        "drowsiness" => row.drow,
        "rel_delta" => row.rd,
        "rel_theta" => row.rt,
        "rel_alpha" => row.ra,
        "rel_beta" => row.rb,
        "pse" => row.pse,
        "apf" => row.apf,
        "sef95" => row.sef95,
        _ => 0.0,
    }
}

fn session_field(m: &SessionMetrics, name: &str) -> f64 {
    match name {
        "relaxation" => m.relaxation,
        "engagement" => m.engagement,
        "faa" => m.faa,
        "tar" => m.tar,
        "bar" => m.bar,
        "dtr" => m.dtr,
        "tbr" => m.tbr,
        "mood" => m.mood,
        "hr" => m.hr,
        "rmssd" => m.rmssd,
        "sdnn" => m.sdnn,
        "stress" => m.stress_index,
        "snr" => m.snr,
        "coherence" => m.coherence,
        "stillness" => m.stillness,
        "blink_rate" => m.blink_rate,
        "meditation" => m.meditation,
        "cognitive_load" => m.cognitive_load,
        "drowsiness" => m.drowsiness,
        "rel_delta" => m.rel_delta,
        "rel_theta" => m.rel_theta,
        "rel_alpha" => m.rel_alpha,
        "rel_beta" => m.rel_beta,
        "pse" => m.pse,
        "apf" => m.apf,
        "sef95" => m.sef95,
        _ => 0.0,
    }
}

const INSIGHT_METRICS: &[&str] = &[
    "relaxation",
    "engagement",
    "meditation",
    "cognitive_load",
    "drowsiness",
    "mood",
    "faa",
    "tar",
    "bar",
    "dtr",
    "tbr",
    "hr",
    "rmssd",
    "stress",
    "snr",
    "coherence",
    "stillness",
    "blink_rate",
    "rel_alpha",
    "rel_beta",
    "rel_theta",
    "rel_delta",
    "pse",
    "apf",
    "sef95",
];

const STATUS_METRICS: &[&str] = &[
    "relaxation",
    "engagement",
    "meditation",
    "cognitive_load",
    "drowsiness",
    "mood",
    "hr",
    "snr",
    "stillness",
];

/// Compute per-metric stats, deltas, and trends for an A/B session comparison.
pub fn compute_compare_insights(
    skill_dir: &Path,
    a_start: u64,
    a_end: u64,
    b_start: u64,
    b_end: u64,
    avg_a: &SessionMetrics,
    avg_b: &SessionMetrics,
) -> serde_json::Value {
    let ts_a = get_session_timeseries(skill_dir, a_start, a_end);
    let ts_b = get_session_timeseries(skill_dir, b_start, b_end);

    let mut stats_a = serde_json::Map::new();
    let mut stats_b = serde_json::Map::new();
    let mut deltas = serde_json::Map::new();
    let mut improved: Vec<String> = Vec::new();
    let mut declined: Vec<String> = Vec::new();
    let mut stable: Vec<String> = Vec::new();

    for &metric in INSIGHT_METRICS {
        let vals_a: Vec<f64> = ts_a.iter().map(|r| epoch_field(r, metric)).collect();
        let vals_b: Vec<f64> = ts_b.iter().map(|r| epoch_field(r, metric)).collect();
        stats_a.insert(metric.into(), metric_stats_vec(&vals_a));
        stats_b.insert(metric.into(), metric_stats_vec(&vals_b));

        let ma = session_field(avg_a, metric);
        let mb = session_field(avg_b, metric);
        let abs_delta = mb - ma;
        let pct = if ma.abs() > 1e-6 {
            abs_delta / ma.abs() * 100.0
        } else {
            0.0
        };
        let direction = if pct > 5.0 {
            "up"
        } else if pct < -5.0 {
            "down"
        } else {
            "stable"
        };

        deltas.insert(
            metric.into(),
            serde_json::json!({
                "a": r2f(ma), "b": r2f(mb), "abs": r2f(abs_delta), "pct": r2f(pct), "direction": direction,
            }),
        );
        match direction {
            "up" => improved.push(metric.into()),
            "down" => declined.push(metric.into()),
            _ => stable.push(metric.into()),
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
    if epochs.is_empty() {
        return serde_json::json!(null);
    }

    let epoch_secs = if summary.epoch_secs > 0.0 {
        summary.epoch_secs
    } else {
        5.0
    };
    let total = summary.total_epochs as f64;
    let wake = summary.wake_epochs as f64;
    let efficiency = if total > 0.0 {
        (total - wake) / total * 100.0
    } else {
        0.0
    };
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
        epochs[si..]
            .iter()
            .find(|e| e.stage == 5)
            .map(|e| r2f(e.utc.saturating_sub(start) as f64 / 60.0))
    });
    let mut transitions = 0u32;
    let mut awakenings = 0u32;
    for w in epochs.windows(2) {
        if w[0].stage != w[1].stage {
            transitions += 1;
            if w[1].stage == 0 && w[0].stage != 0 {
                awakenings += 1;
            }
        }
    }
    let stage_ids: &[(u8, &str)] = &[(0, "wake"), (1, "n1"), (2, "n2"), (3, "n3"), (5, "rem")];
    let mut bouts = serde_json::Map::new();
    for &(sid, name) in stage_ids {
        let mut lengths: Vec<f64> = Vec::new();
        let mut cur = 0u32;
        for e in epochs {
            if e.stage == sid {
                cur += 1;
            } else {
                if cur > 0 {
                    lengths.push(cur as f64 * epoch_secs / 60.0);
                }
                cur = 0;
            }
        }
        if cur > 0 {
            lengths.push(cur as f64 * epoch_secs / 60.0);
        }
        if !lengths.is_empty() {
            let count = lengths.len();
            let mean = lengths.iter().sum::<f64>() / count as f64;
            let max = lengths.iter().cloned().fold(0.0f64, f64::max);
            bouts.insert(
                name.into(),
                serde_json::json!({ "count": count, "mean_min": r2f(mean), "max_min": r2f(max) }),
            );
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
    let all_distances: Vec<f64> = result
        .results
        .iter()
        .flat_map(|q| q.neighbors.iter().map(|n| n.distance as f64))
        .collect();
    let distance_stats = metric_stats_vec(&all_distances);
    let mut hour_dist: HashMap<u8, u32> = HashMap::new();
    let mut day_dist: HashMap<&str, u32> = HashMap::new();
    let mut all_utcs: Vec<u64> = Vec::new();
    for q in &result.results {
        for n in &q.neighbors {
            all_utcs.push(n.timestamp_unix);
            *hour_dist.entry(((n.timestamp_unix % 86400) / 3600) as u8).or_insert(0) += 1;
            *day_dist.entry(&n.date).or_insert(0) += 1;
        }
    }
    let mut hourly = serde_json::Map::new();
    for h in 0..24u8 {
        if let Some(&c) = hour_dist.get(&h) {
            hourly.insert(format!("{h:02}"), c.into());
        }
    }
    let mut top_days: Vec<(&str, u32)> = day_dist.into_iter().collect();
    top_days.sort_by_key(|b| std::cmp::Reverse(b.1));
    top_days.truncate(10);
    let time_span_hours = if all_utcs.len() >= 2 {
        let mn = all_utcs.iter().copied().min().unwrap_or(0);
        let mx = all_utcs.iter().copied().max().unwrap_or(0);
        mx.saturating_sub(mn) as f64 / 3600.0
    } else {
        0.0
    };
    let metric_names = [
        "relaxation",
        "engagement",
        "meditation",
        "cognitive_load",
        "drowsiness",
        "hr",
        "snr",
        "mood",
    ];
    let mut neighbor_metrics = serde_json::Map::new();
    for &name in &metric_names {
        let vals: Vec<f64> = result
            .results
            .iter()
            .flat_map(|q| q.neighbors.iter())
            .filter_map(|n| n.metrics.as_ref())
            .filter_map(|m| match name {
                "relaxation" => m.relaxation,
                "engagement" => m.engagement,
                "meditation" => m.meditation,
                "cognitive_load" => m.cognitive_load,
                "drowsiness" => m.drowsiness,
                "hr" => m.hr,
                "snr" => m.snr,
                "mood" => m.mood,
                _ => None,
            })
            .collect();
        if !vals.is_empty() {
            neighbor_metrics.insert(
                name.into(),
                serde_json::json!(r2f(vals.iter().sum::<f64>() / vals.len() as f64)),
            );
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
    if sessions_json.is_empty() {
        return serde_json::json!(null);
    }

    let today_day = now_utc / 86400;
    let mut total_secs = 0u64;
    let mut longest_secs = 0u64;
    let mut day_set = std::collections::BTreeSet::<u64>::new();
    let total_sessions = sessions_json.len();
    let mut total_epochs = 0u64;

    for s in sessions_json {
        let start = s["start_utc"].as_u64().unwrap_or(0);
        let end = s["end_utc"].as_u64().unwrap_or(0);
        let n_ep = s["n_epochs"].as_u64().unwrap_or(0);
        let dur = end.saturating_sub(start);
        total_secs += dur;
        longest_secs = longest_secs.max(dur);
        total_epochs += n_ep;
        day_set.insert(start / 86400);
    }

    let recording_days = day_set.len();
    let total_hours = total_secs as f64 / 3600.0;
    let avg_session_min = if total_sessions > 0 {
        total_hours * 60.0 / total_sessions as f64
    } else {
        0.0
    };

    let mut streak = 0u32;
    let mut check = today_day;
    loop {
        if day_set.contains(&check) {
            streak += 1;
            if check == 0 {
                break;
            }
            check -= 1;
        } else if check == today_day {
            if check == 0 {
                break;
            }
            check -= 1;
        } else {
            break;
        }
    }

    let today_start = today_day * 86400;
    let week_start = today_day.saturating_sub(7) * 86400;
    let today_metrics = get_session_metrics(skill_dir, today_start, now_utc);
    let week_metrics = get_session_metrics(skill_dir, week_start, now_utc);

    let mut today_vs_avg = serde_json::Map::new();
    if today_metrics.n_epochs > 0 && week_metrics.n_epochs > 0 {
        for &metric in STATUS_METRICS {
            let tv = session_field(&today_metrics, metric);
            let wv = session_field(&week_metrics, metric);
            let delta_pct = if wv.abs() > 1e-6 {
                (tv - wv) / wv.abs() * 100.0
            } else {
                0.0
            };
            let direction = if delta_pct > 5.0 {
                "up"
            } else if delta_pct < -5.0 {
                "down"
            } else {
                "stable"
            };
            today_vs_avg.insert(
                metric.into(),
                serde_json::json!({
                    "today": r2f(tv), "avg_7d": r2f(wv), "delta_pct": r2f(delta_pct), "direction": direction,
                }),
            );
        }
    }
    serde_json::json!({
        "total_sessions": total_sessions, "total_recording_hours": r2f(total_hours),
        "total_epochs": total_epochs, "recording_days": recording_days,
        "current_streak_days": streak, "longest_session_min": r2f(longest_secs as f64 / 60.0),
        "avg_session_min": r2f(avg_session_min), "today_vs_avg": today_vs_avg,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_epoch(ts: u64) -> EpochRow {
        EpochRow {
            t: ts as f64,
            ..Default::default()
        }
    }

    #[test]
    fn downsample_noop_when_under_max() {
        let mut ts = vec![make_epoch(1), make_epoch(2), make_epoch(3)];
        downsample_timeseries(&mut ts, 10);
        assert_eq!(ts.len(), 3);
    }

    #[test]
    fn downsample_exact_count() {
        let mut ts: Vec<EpochRow> = (0..100).map(|i| make_epoch(i)).collect();
        downsample_timeseries(&mut ts, 10);
        assert_eq!(ts.len(), 10);
    }

    #[test]
    fn downsample_preserves_first_and_last() {
        let mut ts: Vec<EpochRow> = (0..100).map(|i| make_epoch(i)).collect();
        downsample_timeseries(&mut ts, 10);
        assert_eq!(ts.first().unwrap().t, 0.0);
        assert_eq!(ts.last().unwrap().t, 99.0);
    }

    #[test]
    fn downsample_max_2_keeps_endpoints() {
        let mut ts: Vec<EpochRow> = (0..50).map(|i| make_epoch(i)).collect();
        downsample_timeseries(&mut ts, 2);
        assert_eq!(ts.len(), 2);
        assert_eq!(ts[0].t, 0.0);
        assert_eq!(ts[1].t, 49.0);
    }

    #[test]
    fn analyze_sleep_stages_empty() {
        let stages = SleepStages {
            epochs: vec![],
            summary: SleepSummary::default(),
        };
        let result = analyze_sleep_stages(&stages);
        assert!(result.is_object() || result.is_null() || result.is_string());
    }

    // ── metrics_cache_path ────────────────────────────────────────────────

    #[test]
    fn metrics_cache_path_from_csv() {
        let p = metrics_cache_path(Path::new("/data/20260320/exg_1710000000.csv"));
        assert_eq!(
            p.file_name().unwrap().to_str().unwrap(),
            "exg_1710000000_metrics_cache.json"
        );
    }

    #[test]
    fn metrics_cache_path_from_parquet() {
        let p = metrics_cache_path(Path::new("/data/20260320/exg_1710000000.parquet"));
        assert_eq!(
            p.file_name().unwrap().to_str().unwrap(),
            "exg_1710000000_metrics_cache.json"
        );
    }

    // ── downsample edge cases ─────────────────────────────────────────────

    #[test]
    fn downsample_empty_is_noop() {
        let mut ts: Vec<EpochRow> = vec![];
        downsample_timeseries(&mut ts, 10);
        assert_eq!(ts.len(), 0);
    }

    #[test]
    fn downsample_single_element() {
        let mut ts = vec![make_epoch(42)];
        downsample_timeseries(&mut ts, 10);
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].t, 42.0);
    }

    #[test]
    fn downsample_max_zero_is_noop() {
        let mut ts: Vec<EpochRow> = (0..10).map(|i| make_epoch(i)).collect();
        downsample_timeseries(&mut ts, 0);
        assert_eq!(ts.len(), 10);
    }

    #[test]
    fn downsample_max_one_is_noop() {
        let mut ts: Vec<EpochRow> = (0..10).map(|i| make_epoch(i)).collect();
        downsample_timeseries(&mut ts, 1);
        assert_eq!(ts.len(), 10); // max < 2 → noop
    }

    #[test]
    fn downsample_evenly_spaced() {
        let mut ts: Vec<EpochRow> = (0..10).map(|i| make_epoch(i)).collect();
        downsample_timeseries(&mut ts, 5);
        assert_eq!(ts.len(), 5);
        // First and last preserved
        assert_eq!(ts[0].t, 0.0);
        assert_eq!(ts[4].t, 9.0);
    }

    // ── SleepSummary default ──────────────────────────────────────────────

    #[test]
    fn sleep_summary_default_is_zeroed() {
        let s = SleepSummary::default();
        assert_eq!(s.total_epochs, 0);
        assert_eq!(s.rem_epochs, 0);
        assert_eq!(s.n3_epochs, 0);
    }

    // ── analyze_sleep_stages with data ────────────────────────────────────

    #[test]
    fn analyze_sleep_stages_with_epochs() {
        use super::super::SleepEpoch;
        let stages = SleepStages {
            epochs: vec![
                SleepEpoch {
                    utc: 1000,
                    stage: 4,
                    ..Default::default()
                }, // REM
                SleepEpoch {
                    utc: 1030,
                    stage: 3,
                    ..Default::default()
                }, // N3
                SleepEpoch {
                    utc: 1060,
                    stage: 2,
                    ..Default::default()
                }, // N2
            ],
            summary: SleepSummary {
                total_epochs: 3,
                wake_epochs: 0,
                n1_epochs: 0,
                n2_epochs: 1,
                n3_epochs: 1,
                rem_epochs: 1,
                epoch_secs: 30.0,
            },
        };
        let result = analyze_sleep_stages(&stages);
        assert!(result.is_object() || result.is_string());
    }

    // ── r2f ──────────────────────────────────────────────────────────────

    #[test]
    fn r2f_rounds_to_2_decimals() {
        assert_eq!(r2f(3.14159), 3.14);
        assert_eq!(r2f(1.006), 1.01);
        assert_eq!(r2f(0.0), 0.0);
        assert_eq!(r2f(-2.567), -2.57);
    }

    // ── linear_slope ─────────────────────────────────────────────────────

    #[test]
    fn linear_slope_constant_is_zero() {
        assert_eq!(linear_slope(&[5.0, 5.0, 5.0, 5.0]), 0.0);
    }

    #[test]
    fn linear_slope_increasing() {
        let slope = linear_slope(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!((slope - 1.0).abs() < 1e-10, "expected slope ~1.0, got {slope}");
    }

    #[test]
    fn linear_slope_decreasing() {
        let slope = linear_slope(&[5.0, 4.0, 3.0, 2.0, 1.0]);
        assert!((slope - (-1.0)).abs() < 1e-10, "expected slope ~-1.0, got {slope}");
    }

    #[test]
    fn linear_slope_single_value() {
        assert_eq!(linear_slope(&[42.0]), 0.0);
    }

    #[test]
    fn linear_slope_empty() {
        assert_eq!(linear_slope(&[]), 0.0);
    }

    #[test]
    fn linear_slope_two_values() {
        let slope = linear_slope(&[0.0, 10.0]);
        assert!((slope - 10.0).abs() < 1e-10);
    }

    // ── classify_sleep ───────────────────────────────────────────────────

    #[test]
    fn classify_sleep_wake_high_alpha() {
        assert_eq!(classify_sleep(0.1, 0.1, 0.35, 0.1), 0); // high alpha → wake
    }

    #[test]
    fn classify_sleep_wake_high_beta() {
        assert_eq!(classify_sleep(0.1, 0.1, 0.1, 0.35), 0); // high beta → wake
    }

    #[test]
    fn classify_sleep_rem() {
        assert_eq!(classify_sleep(0.2, 0.35, 0.10, 0.10), 5); // high theta, low alpha/delta → REM
    }

    #[test]
    fn classify_sleep_deep() {
        assert_eq!(classify_sleep(0.55, 0.1, 0.1, 0.1), 3); // high delta → deep
    }

    #[test]
    fn classify_sleep_light() {
        assert_eq!(classify_sleep(0.3, 0.30, 0.1, 0.1), 1); // moderate theta → light
    }

    #[test]
    fn classify_sleep_default() {
        assert_eq!(classify_sleep(0.2, 0.2, 0.2, 0.2), 2); // no dominant band → stage 2
    }

    // ── metric_stats_vec ─────────────────────────────────────────────────

    #[test]
    fn metric_stats_vec_empty_returns_null() {
        assert!(metric_stats_vec(&[]).is_null());
    }

    #[test]
    fn metric_stats_vec_single_value() {
        let result = metric_stats_vec(&[42.0]);
        assert_eq!(result["min"], 42.0);
        assert_eq!(result["max"], 42.0);
        assert_eq!(result["mean"], 42.0);
    }

    #[test]
    fn metric_stats_vec_basic_stats() {
        let result = metric_stats_vec(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        assert_eq!(result["min"], 1.0);
        assert_eq!(result["max"], 8.0);
        assert_eq!(result["mean"], 4.5);
        assert!(result["stddev"].as_f64().unwrap() > 0.0);
        assert!(result["trend"].is_number());
    }

    // ── analyze_sleep_stages ─────────────────────────────────────────────

    fn make_sleep_epoch(utc: u64, stage: u8) -> crate::SleepEpoch {
        crate::SleepEpoch {
            utc,
            stage,
            rel_delta: 0.0,
            rel_theta: 0.0,
            rel_alpha: 0.0,
            rel_beta: 0.0,
        }
    }

    #[test]
    fn analyze_sleep_stages_empty_returns_null() {
        let stages = crate::SleepStages {
            epochs: vec![],
            summary: crate::SleepSummary::default(),
        };
        assert!(analyze_sleep_stages(&stages).is_null());
    }

    #[test]
    fn analyze_sleep_stages_all_wake() {
        let epochs: Vec<crate::SleepEpoch> = (0..60).map(|i| make_sleep_epoch(1000 + i * 5, 0)).collect();
        let stages = crate::SleepStages {
            epochs,
            summary: crate::SleepSummary {
                total_epochs: 60,
                wake_epochs: 60,
                n1_epochs: 0,
                n2_epochs: 0,
                n3_epochs: 0,
                rem_epochs: 0,
                epoch_secs: 5.0,
            },
        };
        let result = analyze_sleep_stages(&stages);
        assert_eq!(result["efficiency_pct"], 0.0);
        assert_eq!(result["transitions"], 0);
        assert_eq!(result["awakenings"], 0);
    }

    #[test]
    fn analyze_sleep_stages_mixed() {
        let epochs = vec![
            make_sleep_epoch(1000, 0), // wake
            make_sleep_epoch(1005, 0), // wake
            make_sleep_epoch(1010, 1), // N1 (sleep onset)
            make_sleep_epoch(1015, 2), // N2
            make_sleep_epoch(1020, 3), // N3 (deep)
            make_sleep_epoch(1025, 3), // N3
            make_sleep_epoch(1030, 5), // REM
            make_sleep_epoch(1035, 0), // wake (awakening)
        ];
        let stages = crate::SleepStages {
            epochs,
            summary: crate::SleepSummary {
                total_epochs: 8,
                wake_epochs: 3,
                n1_epochs: 1,
                n2_epochs: 1,
                n3_epochs: 2,
                rem_epochs: 1,
                epoch_secs: 5.0,
            },
        };
        let result = analyze_sleep_stages(&stages);
        assert!(result["efficiency_pct"].as_f64().unwrap() > 0.0);
        assert!(result["onset_latency_min"].as_f64().unwrap() > 0.0);
        assert!(result["transitions"].as_u64().unwrap() > 0);
        assert_eq!(result["awakenings"], 1);
        assert!(result["bouts"].is_object());
        assert!(result["rem_latency_min"].is_number());
    }

    // ── analyze_search_results ──────────────────────────────────────────

    #[test]
    fn analyze_search_results_empty() {
        let result = skill_commands::SearchResult {
            start_utc: 0,
            end_utc: 0,
            k: 5,
            ef: 50,
            query_count: 0,
            searched_days: vec![],
            results: vec![],
        };
        let insights = analyze_search_results(&result);
        assert_eq!(insights["total_neighbors"], 0);
        assert!(insights["distance_stats"].is_null());
    }

    /// Create a test skill_dir with a YYYYMMDD/eeg.sqlite containing fixture embeddings.
    fn create_fixture_db(skill_dir: &std::path::Path, date: &str, rows: &[(i64, &str)]) {
        let day_dir = skill_dir.join(date);
        std::fs::create_dir_all(&day_dir).unwrap();
        let db_path = day_dir.join("eeg.sqlite");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS embeddings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                device_id TEXT,
                device_name TEXT,
                hnsw_id INTEGER DEFAULT 0,
                eeg_embedding BLOB,
                label TEXT,
                metrics_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_embeddings_timestamp ON embeddings(timestamp);",
        )
        .unwrap();
        let mut stmt = conn
            .prepare("INSERT INTO embeddings (timestamp, metrics_json) VALUES (?1, ?2)")
            .unwrap();
        for (ts, json) in rows {
            stmt.execute(rusqlite::params![ts, json]).unwrap();
        }
    }

    fn sample_metrics_json(rd: f64, rt: f64, ra: f64, rb: f64) -> String {
        serde_json::json!({
            "rel_delta": rd, "rel_theta": rt, "rel_alpha": ra, "rel_beta": rb,
            "rel_gamma": 0.05, "relaxation_score": 50.0, "engagement_score": 50.0,
            "faa": 0.1, "tar": 0.5, "bar": 0.3, "dtr": 0.8, "pse": 10.0,
            "apf": 10.0, "bps": 5.0, "snr": 15.0, "coherence": 0.5,
            "mu_suppression": 0.9, "mood": 60.0, "tbr": 1.5,
            "hr": 72.0, "rmssd": 35.0, "sdnn": 45.0, "meditation": 55.0,
        })
        .to_string()
    }

    // ── get_session_timeseries ───────────────────────────────────────────

    #[test]
    fn get_session_timeseries_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let rows = get_session_timeseries(dir.path(), 1700000000, 1700003600);
        assert!(rows.is_empty());
    }

    #[test]
    fn get_session_timeseries_with_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let base_ts = 1700000000i64 * 1000; // milliseconds
        let rows_data: Vec<(i64, &str)> = (0..10).map(|i| (base_ts + i * 5000, "{}")).collect();
        let rows_ref: Vec<(i64, &str)> = rows_data.iter().map(|(ts, json)| (*ts, *json)).collect();
        create_fixture_db(dir.path(), "20231114", &rows_ref);

        let result = get_session_timeseries(dir.path(), 1700000000, 1700000050);
        assert_eq!(result.len(), 10, "should return all 10 epochs");
        assert!(result[0].t > 0.0);
    }

    #[test]
    fn get_session_timeseries_with_metrics_json() {
        let dir = tempfile::tempdir().unwrap();
        let base_ts = 1700000000i64 * 1000;
        let json = sample_metrics_json(0.3, 0.2, 0.25, 0.15);
        let rows_data: Vec<(i64, String)> = (0..5).map(|i| (base_ts + i * 5000, json.clone())).collect();
        let rows_ref: Vec<(i64, &str)> = rows_data.iter().map(|(ts, j)| (*ts, j.as_str())).collect();
        create_fixture_db(dir.path(), "20231114", &rows_ref);

        let result = get_session_timeseries(dir.path(), 1700000000, 1700000050);
        assert_eq!(result.len(), 5);
        assert!((result[0].rd - 0.3).abs() < 0.01, "rel_delta should be 0.3");
        assert!((result[0].hr - 72.0).abs() < 0.01, "hr should be 72");
    }

    // ── get_session_metrics ─────────────────────────────────────────────

    #[test]
    fn get_session_metrics_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let metrics = get_session_metrics(dir.path(), 1700000000, 1700003600);
        assert_eq!(metrics.n_epochs, 0);
    }

    #[test]
    fn get_session_metrics_with_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let base_ts = 1700000000i64 * 1000;
        let json = sample_metrics_json(0.3, 0.2, 0.25, 0.15);
        let rows_data: Vec<(i64, String)> = (0..20).map(|i| (base_ts + i * 5000, json.clone())).collect();
        let rows_ref: Vec<(i64, &str)> = rows_data.iter().map(|(ts, j)| (*ts, j.as_str())).collect();
        create_fixture_db(dir.path(), "20231114", &rows_ref);

        let metrics = get_session_metrics(dir.path(), 1700000000, 1700000100);
        assert_eq!(metrics.n_epochs, 20);
        assert!((metrics.rel_delta - 0.3).abs() < 0.01);
        assert!((metrics.hr - 72.0).abs() < 0.01);
    }

    // ── get_sleep_stages ─────────────────────────────────────────────────

    #[test]
    fn get_sleep_stages_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let stages = get_sleep_stages(dir.path(), 1700000000, 1700003600);
        assert!(stages.epochs.is_empty());
    }

    #[test]
    fn get_sleep_stages_with_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let base_ts = 1700000000i64 * 1000;
        // Mix of band ratios → different sleep stages
        let jsons = vec![
            sample_metrics_json(0.1, 0.1, 0.35, 0.35),  // wake (high alpha+beta)
            sample_metrics_json(0.1, 0.35, 0.10, 0.10), // REM (high theta)
            sample_metrics_json(0.55, 0.1, 0.10, 0.10), // deep (high delta)
            sample_metrics_json(0.2, 0.3, 0.10, 0.10),  // light (moderate theta)
            sample_metrics_json(0.2, 0.2, 0.20, 0.20),  // stage 2 (default)
        ];
        let rows_data: Vec<(i64, &str)> = jsons
            .iter()
            .enumerate()
            .map(|(i, j)| (base_ts + i as i64 * 5000, j.as_str()))
            .collect();
        create_fixture_db(dir.path(), "20231114", &rows_data);

        let stages = get_sleep_stages(dir.path(), 1700000000, 1700000030);
        assert_eq!(stages.epochs.len(), 5);
        assert_eq!(stages.summary.total_epochs, 5);
        // Verify different stages were classified
        let stage_set: std::collections::HashSet<u8> = stages.epochs.iter().map(|e| e.stage).collect();
        assert!(
            stage_set.len() > 1,
            "should have multiple sleep stages: {:?}",
            stage_set
        );
    }

    // ── compute_compare_insights ─────────────────────────────────────────

    #[test]
    fn compute_compare_insights_with_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let base_a = 1700000000i64 * 1000;
        let base_b = 1700010000i64 * 1000;
        let json_a = sample_metrics_json(0.3, 0.2, 0.25, 0.15);
        let json_b = sample_metrics_json(0.15, 0.25, 0.35, 0.20);

        let rows_a: Vec<(i64, String)> = (0..10).map(|i| (base_a + i * 5000, json_a.clone())).collect();
        let rows_b: Vec<(i64, String)> = (0..10).map(|i| (base_b + i * 5000, json_b.clone())).collect();
        let mut all: Vec<(i64, &str)> = rows_a.iter().map(|(t, j)| (*t, j.as_str())).collect();
        all.extend(rows_b.iter().map(|(t, j)| (*t, j.as_str())));
        create_fixture_db(dir.path(), "20231114", &all);

        let metrics_a = get_session_metrics(dir.path(), 1700000000, 1700000050);
        let metrics_b = get_session_metrics(dir.path(), 1700010000, 1700010050);
        let insights = compute_compare_insights(
            dir.path(),
            1700000000,
            1700000050,
            1700010000,
            1700010050,
            &metrics_a,
            &metrics_b,
        );
        assert!(insights.is_object());
        assert!(insights.get("deltas").is_some());
    }

    #[test]
    fn analyze_search_results_with_data() {
        let result = skill_commands::SearchResult {
            start_utc: 1700000000,
            end_utc: 1700003600,
            k: 3,
            ef: 50,
            query_count: 1,
            searched_days: vec!["20231114".into()],
            results: vec![skill_commands::QueryEntry {
                timestamp: 20231114120000,
                timestamp_unix: 1700000000,
                neighbors: vec![
                    skill_commands::NeighborEntry {
                        hnsw_id: 0,
                        timestamp: 20231114120100,
                        timestamp_unix: 1700000060,
                        distance: 0.1,
                        date: "20231114".into(),
                        device_id: None,
                        device_name: None,
                        labels: vec![],
                        metrics: None,
                    },
                    skill_commands::NeighborEntry {
                        hnsw_id: 1,
                        timestamp: 20231114130000,
                        timestamp_unix: 1700003600,
                        distance: 0.5,
                        date: "20231114".into(),
                        device_id: None,
                        device_name: None,
                        labels: vec![],
                        metrics: None,
                    },
                ],
            }],
        };
        let insights = analyze_search_results(&result);
        assert_eq!(insights["total_neighbors"], 2);
        assert!(insights["distance_stats"].is_object());
        assert!(insights["time_span_hours"].as_f64().unwrap() > 0.0);
    }
}
