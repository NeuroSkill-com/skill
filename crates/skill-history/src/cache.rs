// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Disk cache for session metrics — avoids recomputing from CSV on every load.

use std::collections::HashMap;
use std::path::Path;

// metrics_csv_path kept for backward compat if needed; find_metrics_path handles both formats.
use skill_data::util::{unix_to_ts, ts_to_unix};

use super::{
    CsvMetricsResult, SessionMetrics, EpochRow,
    SleepStages, SleepEpoch, SleepSummary,
    load_metrics_csv, find_metrics_path,
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
            gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
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
        let mn = all_utcs.iter().copied().min().unwrap_or(0);
        let mx = all_utcs.iter().copied().max().unwrap_or(0);
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
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
        // Result can be an object or null — just check it doesn't panic
        assert!(result.is_object() || result.is_null() || result.is_string());
    }
}
