// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Session metrics, time-series data, sleep staging, UMAP comparison,
// and cross-session analysis.  All heavy computation runs on Tokio
// blocking threads or pre-computed from SQLite; the Tauri commands in
// this file are thin IPC wrappers.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{AppState, MutexExt, unix_secs};
use crate::settings::load_umap_config;
use crate::session_csv::metrics_csv_path;
use crate::{job_queue, ws_commands};
use crate::commands;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub(crate) struct SessionMetrics {
    /// Number of embedding epochs in the range.
    n_epochs:         usize,
    /// Mean relative band powers (0.0–1.0), averaged across all epochs.
    rel_delta:        f64,
    rel_theta:        f64,
    rel_alpha:        f64,
    rel_beta:         f64,
    rel_gamma:        f64,
    rel_high_gamma:   f64,
    /// Mean derived scores (0–100).
    relaxation:       f64,
    engagement:       f64,
    /// Mean Frontal Alpha Asymmetry.
    faa:              f64,
    /// Mean Theta / Alpha ratio.
    tar:              f64,
    /// Mean Beta / Alpha ratio.
    bar:              f64,
    /// Mean Delta / Theta ratio.
    dtr:              f64,
    /// Mean Power Spectral Entropy.
    pse:              f64,
    /// Mean Alpha Peak Frequency (Hz).
    apf:              f64,
    /// Mean Band-Power Slope (1/f).
    bps:              f64,
    /// Mean SNR (dB).
    snr:              f64,
    /// Mean inter-channel coherence.
    coherence:        f64,
    /// Mean Mu suppression index.
    mu_suppression:   f64,
    /// Mean Mood index (0–100).
    mood:             f64,
    tbr:              f64,
    sef95:            f64,
    spectral_centroid: f64,
    hjorth_activity:  f64,
    hjorth_mobility:  f64,
    hjorth_complexity: f64,
    permutation_entropy: f64,
    higuchi_fd:       f64,
    dfa_exponent:     f64,
    sample_entropy:   f64,
    pac_theta_gamma:  f64,
    laterality_index: f64,
    // PPG-derived
    hr:               f64,
    rmssd:            f64,
    sdnn:             f64,
    pnn50:            f64,
    lf_hf_ratio:      f64,
    respiratory_rate: f64,
    spo2_estimate:    f64,
    perfusion_index:  f64,
    stress_index:     f64,
    // Artifact events
    blink_count: f64,
    blink_rate:  f64,
    // Head pose
    head_pitch:       f64,
    head_roll:        f64,
    stillness:        f64,
    nod_count:        f64,
    shake_count:      f64,
    // Composite scores
    meditation:       f64,
    cognitive_load:   f64,
    drowsiness:       f64,
}

// ── Time-series epoch data ────────────────────────────────────────────────────

/// A single epoch's metrics, returned as part of a time-series query.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub(crate) struct EpochRow {
    /// Epoch timestamp (Unix seconds UTC).
    t: f64,
    /// Relative band powers.
    rd: f64, rt: f64, ra: f64, rb: f64, rg: f64,
    /// Core scores.
    relaxation: f64, engagement: f64,
    faa: f64,
    /// Band ratios.
    tar: f64, bar: f64, dtr: f64, tbr: f64,
    /// Spectral.
    pse: f64, apf: f64, sef95: f64, sc: f64, bps: f64, snr: f64,
    /// Cross-channel.
    coherence: f64, mu: f64,
    /// Hjorth.
    ha: f64, hm: f64, hc: f64,
    /// Nonlinear.
    pe: f64, hfd: f64, dfa: f64, se: f64, pac: f64, lat: f64,
    /// Mood.
    mood: f64,
    /// PPG vitals.
    hr: f64, rmssd: f64, sdnn: f64, pnn50: f64, lf_hf: f64,
    resp: f64, spo2: f64, perf: f64, stress: f64,
    /// Artifact events.
    blinks: f64, blink_r: f64,
    /// Head pose.
    pitch: f64, roll: f64, still: f64, nods: f64, shakes: f64,
    /// Composite.
    med: f64, cog: f64, drow: f64,
    /// GPU utilisation (0–1).
    gpu: f64, gpu_render: f64, gpu_tiler: f64,
}

// ── CSV-based metrics loading ──────────────────────────────────────────────────

/// Combined summary + time-series data loaded directly from `_metrics.csv`.
/// This is the primary data source for historical session display —
/// it works even when no SQLite epoch data exists.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub(crate) struct CsvMetricsResult {
    /// Number of rows in the CSV (≈ snapshots at ~4 Hz).
    n_rows: usize,
    /// Aggregated averages across all rows.
    summary: SessionMetrics,
    /// Per-row time-series for charts.
    timeseries: Vec<EpochRow>,
}

/// Read a `_metrics.csv` file and return aggregated summary + time-series.
/// Column indices follow `METRICS_CSV_HEADER` (94 columns).
fn load_metrics_csv(csv_path: &std::path::Path) -> Option<CsvMetricsResult> {
    let metrics_path = metrics_csv_path(csv_path);
    if !metrics_path.exists() {
        eprintln!("[csv-metrics] no metrics file: {}", metrics_path.display());
        return None;
    }

    let mut rdr = match csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(&metrics_path)
    {
        Ok(r)  => r,
        Err(e) => { eprintln!("[csv-metrics] open error: {e}"); return None; }
    };

    let mut rows: Vec<EpochRow> = Vec::new();
    let mut sum = SessionMetrics::default();
    let mut count = 0usize;

    for result in rdr.records() {
        let rec = match result {
            Ok(r)  => r,
            Err(_) => continue,
        };
        // Minimum: need at least 49 columns (timestamp + 48 band powers)
        if rec.len() < 49 { continue; }

        let f = |i: usize| -> f64 {
            rec.get(i).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)
        };

        let timestamp = f(0);
        if timestamp <= 0.0 { continue; }

        // Average the 4 channels' relative band powers for summary
        // Rel columns per channel: offset+6..offset+11 (rel_delta..rel_high_gamma)
        // Ch offsets: TP9=1, AF7=13, AF8=25, TP10=37
        let avg_rel = |band_offset: usize| -> f64 {
            let mut s = 0.0;
            for ch_base in &[1usize, 13, 25, 37] {
                s += f(ch_base + 6 + band_offset); // +6 = skip absolute powers
            }
            s / 4.0
        };

        let rd = avg_rel(0); // rel_delta
        let rt = avg_rel(1); // rel_theta
        let ra = avg_rel(2); // rel_alpha
        let rb = avg_rel(3); // rel_beta
        let rg = avg_rel(4); // rel_gamma

        // Cross-channel indices start at col 49
        let faa_v  = f(49);  let tar_v  = f(50);  let bar_v  = f(51);  let dtr_v  = f(52);
        let pse_v  = f(53);  let apf_v  = f(54);  let bps_v  = f(55);  let snr_v  = f(56);
        let coh_v  = f(57);  let mu_v   = f(58);  let mood_v = f(59);
        let tbr_v  = f(60);  let sef_v  = f(61);  let sc_v   = f(62);
        let ha_v   = f(63);  let hm_v   = f(64);  let hc_v   = f(65);
        let pe_v   = f(66);  let hfd_v  = f(67);  let dfa_v  = f(68);
        let se_v   = f(69);  let pac_v  = f(70);  let lat_v  = f(71);

        // PPG vitals (cols 72-80)
        let hr_v    = f(72); let rmssd_v = f(73); let sdnn_v  = f(74);
        let pnn_v   = f(75); let lfhf_v  = f(76); let resp_v  = f(77);
        let spo_v   = f(78); let perf_v  = f(79); let stress_v= f(80);

        // Artifact events (cols 81-82)
        let blinks_v  = f(81); let blink_r_v  = f(82);

        // Head pose (cols 83-87)
        let pitch_v = f(83); let roll_v = f(84); let still_v = f(85);
        let nods_v  = f(86); let shakes_v = f(87);

        // Composite (cols 88-90)
        let med_v = f(88); let cog_v = f(89); let drow_v = f(90);

        // GPU utilisation (cols 92-94, after temperature_raw at 91)
        let gpu_v = f(92); let gpu_r_v = f(93); let gpu_t_v = f(94);

        // Compute focus/relaxation/engagement per-channel, then average
        // (matches eeg_embeddings.rs logic exactly)
        let mut sr = 0.0f64; let mut se2 = 0.0f64;
        for ch_base in &[1usize, 13, 25, 37] {
            let a = f(ch_base + 6 + 2); // rel_alpha
            let b = f(ch_base + 6 + 3); // rel_beta
            let t = f(ch_base + 6 + 1); // rel_theta
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

        // Accumulate for averages
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
    // blink_count, nod_count, shake_count: keep totals (don't average)

    eprintln!("[csv-metrics] loaded {} rows from {}", count, metrics_path.display());

    Some(CsvMetricsResult {
        n_rows: count,
        summary: sum,
        timeseries: rows,
    })
}

/// Sigmoid mapping (0, ∞) → (0, 100) with tuneable steepness and midpoint.
fn sigmoid100(x: f32, k: f32, mid: f32) -> f32 {
    100.0 / (1.0 + (-k * (x - mid)).exp())
}

/// Return per-epoch time-series data for a session range.
/// Used for historical charts in compare and history views.
/// Run schema migrations on a connection so new columns are available
/// even in databases that haven't been opened by the embedding worker yet.
fn migrate_embeddings_schema(conn: &rusqlite::Connection) {
    // Add the metrics_json column to databases created before the JSON schema.
    // All other column additions are no longer needed for new rows; old DBs
    // simply have NULL metrics_json and return 0.0 from json_extract().
    let _ = conn.execute("ALTER TABLE embeddings ADD COLUMN metrics_json TEXT", []);
}

pub(crate) fn get_session_timeseries_impl(
    skill_dir: &std::path::Path,
    start_utc: u64,
    end_utc:   u64,
) -> Vec<EpochRow> {
    use crate::commands::unix_to_ts;
    let ts_start = unix_to_ts(start_utc);
    let ts_end   = unix_to_ts(end_utc);
    let mut rows: Vec<EpochRow> = Vec::new();

    let entries = match std::fs::read_dir(skill_dir) {
        Ok(e)  => e,
        Err(_) => return rows,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let db_path = path.join(crate::constants::SQLITE_FILE);
        if !db_path.exists() { continue; }

        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c)  => c,
            Err(_) => continue,
        };
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
        ) {
            Ok(s)  => s,
            Err(_) => continue,
        };

        let iter = stmt.query_map(rusqlite::params![ts_start, ts_end], |row| {
            let ts_val: i64 = row.get(0)?;
            let utc = crate::commands::ts_to_unix(ts_val);
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
                gpu: 0.0, gpu_render: 0.0, gpu_tiler: 0.0, // not stored in SQLite
            })
        });

        if let Ok(iter) = iter {
            for row in iter.filter_map(|r| r.ok()) {
                rows.push(row);
            }
        }
    }

    rows.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    rows
}

/// Core implementation: query aggregated band-power metrics from all daily
/// `eeg.sqlite` databases that overlap `[start_utc, end_utc]`.
/// Used by both the Tauri IPC command and the WebSocket `compare` handler.
pub(crate) fn get_session_metrics_impl(
    skill_dir: &std::path::Path,
    start_utc: u64,
    end_utc:   u64,
) -> SessionMetrics {
    use crate::commands::unix_to_ts;

    let ts_start = unix_to_ts(start_utc);
    let ts_end   = unix_to_ts(end_utc);

    let mut total = SessionMetrics::default();
    let mut count = 0u64;

    let entries = match std::fs::read_dir(skill_dir) {
        Ok(e)  => e,
        Err(_) => return total,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let db_path = path.join(crate::constants::SQLITE_FILE);
        if !db_path.exists() { continue; }

        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c)  => c,
            Err(_) => continue,
        };
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
        ) {
            Ok(s)  => s,
            Err(_) => continue,
        };

        // Use a Vec instead of fixed-size array for 50 columns
        let rows = stmt.query_map(rusqlite::params![ts_start, ts_end], |row| {
            let mut v = Vec::with_capacity(50);
            for i in 0..50 {
                v.push(row.get::<_, Option<f64>>(i)?);
            }
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
        total.rel_delta      /= n;
        total.rel_theta      /= n;
        total.rel_alpha      /= n;
        total.rel_beta       /= n;
        total.rel_gamma      /= n;
        total.rel_high_gamma /= n;
        total.relaxation     /= n;
        total.engagement     /= n;
        total.faa            /= n;
        total.tar            /= n;
        total.bar            /= n;
        total.dtr            /= n;
        total.pse            /= n;
        total.apf            /= n;
        total.bps            /= n;
        total.snr            /= n;
        total.coherence      /= n;
        total.mu_suppression /= n;
        total.mood           /= n;
        total.tbr            /= n;
        total.sef95          /= n;
        total.spectral_centroid /= n;
        total.hjorth_activity   /= n;
        total.hjorth_mobility   /= n;
        total.hjorth_complexity /= n;
        total.permutation_entropy /= n;
        total.higuchi_fd     /= n;
        total.dfa_exponent   /= n;
        total.sample_entropy /= n;
        total.pac_theta_gamma /= n;
        total.laterality_index /= n;
        total.hr               /= n;
        total.rmssd            /= n;
        total.sdnn             /= n;
        total.pnn50            /= n;
        total.lf_hf_ratio      /= n;
        total.respiratory_rate /= n;
        total.spo2_estimate    /= n;
        total.perfusion_index  /= n;
        total.stress_index     /= n;
        total.blink_count /= n;
        total.blink_rate  /= n;
        total.head_pitch  /= n;
        total.head_roll        /= n;
        total.stillness        /= n;
        total.nod_count        /= n;
        total.shake_count      /= n;
        total.meditation       /= n;
        total.cognitive_load   /= n;
        total.drowsiness       /= n;
        total.n_epochs        = count as usize;
    }

    total
}

// ── Sleep staging ─────────────────────────────────────────────────────────────

/// A single epoch classified into a sleep stage.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct SleepEpoch {
    /// Unix seconds (UTC) of this epoch.
    pub(crate) utc: u64,
    /// Sleep stage: 0 = Wake, 1 = N1, 2 = N2, 3 = N3, 5 = REM.
    /// (Stage numbering follows AASM convention; 4 is unused.)
    pub(crate) stage: u8,
    /// Relative band powers for this epoch.
    pub(crate) rel_delta: f64,
    pub(crate) rel_theta: f64,
    pub(crate) rel_alpha: f64,
    pub(crate) rel_beta:  f64,
}

/// Summary statistics for a sleep session.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub(crate) struct SleepSummary {
    pub(crate) total_epochs:  usize,
    pub(crate) wake_epochs:   usize,
    pub(crate) n1_epochs:     usize,
    pub(crate) n2_epochs:     usize,
    pub(crate) n3_epochs:     usize,
    pub(crate) rem_epochs:    usize,
    /// Epoch duration in seconds (from embedding interval).
    pub(crate) epoch_secs:    f64,
}

/// Result returned by [`get_sleep_stages`].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct SleepStages {
    pub(crate) epochs:  Vec<SleepEpoch>,
    pub(crate) summary: SleepSummary,
}

/// Classify each embedding epoch in `[start_utc, end_utc]` into a sleep stage.
///
/// The classifier uses relative band-power ratios following simplified AASM
/// heuristics (Muse has only 4 dry electrodes — frontal + temporal — so this
/// is an approximation, not a clinical polysomnograph):
///
/// | Stage | Dominant activity | Rule (applied in order) |
/// |-------|-------------------|-------------------------|
/// | Wake  | α / low β, eye blinks | `rel_alpha > 0.30` **or** `rel_beta > 0.30` |
/// | REM   | mixed low-voltage, θ | `rel_theta > 0.30` **and** `rel_alpha < 0.15` **and** `rel_delta < 0.45` |
/// | N1    | θ replaces α | `rel_theta > 0.25` **and** `rel_delta < 0.50` |
/// | N3    | slow-wave, δ dominant | `rel_delta > 0.50` |
/// | N2    | everything else (spindles/K-complexes not resolvable on Muse) |
///
/// These thresholds are tuned for the Muse 2 / Muse S headband.
#[tauri::command]
pub(crate) async fn get_sleep_stages(
    start_utc: u64,
    end_utc:   u64,
    state:     tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<SleepStages, String> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    tokio::task::spawn_blocking(move || {
        get_sleep_stages_impl(&skill_dir, start_utc, end_utc)
    }).await.map_err(|e| e.to_string())
}

pub(crate) fn get_sleep_stages_impl(
    skill_dir: &std::path::Path,
    start_utc: u64,
    end_utc:   u64,
) -> SleepStages {
    use crate::commands::{unix_to_ts, ts_to_unix};

    let ts_start = unix_to_ts(start_utc);
    let ts_end   = unix_to_ts(end_utc);

    // Collect raw epochs from all day-directories.
    struct RawEpoch { utc: u64, rd: f64, rt: f64, ra: f64, rb: f64 }
    let mut raw: Vec<RawEpoch> = Vec::new();

    let entries = match std::fs::read_dir(skill_dir) {
        Ok(e) => e,
        Err(_) => return SleepStages { epochs: vec![], summary: SleepSummary::default() },
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let db_path = path.join(crate::constants::SQLITE_FILE);
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
             FROM embeddings
             WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp"
        ) { Ok(s) => s, Err(_) => continue };

        let rows = stmt.query_map(rusqlite::params![ts_start, ts_end], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, Option<f64>>(3)?,
                row.get::<_, Option<f64>>(4)?,
            ))
        });
        if let Ok(rows) = rows {
            for row in rows.filter_map(|r| r.ok()) {
                let (ts, rd, rt, ra, rb) = row;
                if rd.is_none() && rt.is_none() { continue; }
                raw.push(RawEpoch {
                    utc: ts_to_unix(ts),
                    rd: rd.unwrap_or(0.0),
                    rt: rt.unwrap_or(0.0),
                    ra: ra.unwrap_or(0.0),
                    rb: rb.unwrap_or(0.0),
                });
            }
        }
    }

    raw.sort_by_key(|e| e.utc);

    // Classify each epoch.
    let mut summary = SleepSummary::default();
    let epochs: Vec<SleepEpoch> = raw.iter().map(|e| {
        let stage = classify_sleep(e.rd, e.rt, e.ra, e.rb);
        match stage {
            0 => summary.wake_epochs += 1,
            1 => summary.n1_epochs   += 1,
            2 => summary.n2_epochs   += 1,
            3 => summary.n3_epochs   += 1,
            5 => summary.rem_epochs  += 1,
            _ => {}
        }
        SleepEpoch {
            utc: e.utc, stage,
            rel_delta: e.rd, rel_theta: e.rt,
            rel_alpha: e.ra, rel_beta:  e.rb,
        }
    }).collect();

    summary.total_epochs = epochs.len();
    // Estimate epoch duration from median inter-epoch gap.
    if epochs.len() >= 2 {
        let mut gaps: Vec<f64> = epochs.windows(2)
            .map(|w| (w[1].utc as f64) - (w[0].utc as f64))
            .filter(|g| *g > 0.0 && *g < 30.0)
            .collect();
        if !gaps.is_empty() {
            gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
            summary.epoch_secs = gaps[gaps.len() / 2];
        } else {
            summary.epoch_secs = 2.5; // default
        }
    } else {
        summary.epoch_secs = 2.5;
    }

    SleepStages { epochs, summary }
}

/// Classify a single epoch into a sleep stage from relative band powers.
fn classify_sleep(rd: f64, rt: f64, ra: f64, rb: f64) -> u8 {
    // Wake: strong alpha or beta
    if ra > 0.30 || rb > 0.30 { return 0; }
    // REM: theta-dominant, low alpha, moderate-or-low delta
    if rt > 0.30 && ra < 0.15 && rd < 0.45 { return 5; }
    // N3 (slow-wave / deep): delta-dominant
    if rd > 0.50 { return 3; }
    // N1 (light drowsiness): theta rising, delta not yet dominant
    if rt > 0.25 && rd < 0.50 { return 1; }
    // N2 (default light sleep): everything else
    2
}

// ── Analysis helpers ──────────────────────────────────────────────────────────
//
// These functions compute derived insights from existing data (timeseries,
// sleep stages, search results, UMAP coordinates, session history).
// They return `serde_json::Value` for easy inclusion in WS responses.
// Placed in lib.rs so they have access to private struct fields.

/// Round to 2 decimal places.
fn r2f(v: f64) -> f64 { (v * 100.0).round() / 100.0 }

/// Linear regression slope over a sequence of values.
/// Positive slope = increasing trend over time.
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

/// Descriptive statistics for a slice of f64 values.
fn metric_stats_vec(values: &[f64]) -> serde_json::Value {
    if values.is_empty() { return serde_json::json!(null); }
    let n = values.len();
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean = values.iter().sum::<f64>() / n as f64;
    let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
    let stddev = variance.sqrt();
    let median = if n.is_multiple_of(2) { (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0 } else { sorted[n / 2] };
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

/// Extract a named metric from an EpochRow.
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

/// Extract a named metric from SessionMetrics.
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

/// Metric names used for compare insights and status comparisons.
const INSIGHT_METRICS: &[&str] = &[
    "relaxation", "engagement", "meditation", "cognitive_load", "drowsiness",
    "mood", "faa", "tar", "bar", "dtr", "tbr",
    "hr", "rmssd", "stress", "snr", "coherence", "stillness",
    "blink_rate", "rel_alpha", "rel_beta", "rel_theta", "rel_delta",
    "pse", "apf", "sef95",
];

/// Key composite metrics for status today-vs-average comparison.
const STATUS_METRICS: &[&str] = &[
    "relaxation", "engagement", "meditation", "cognitive_load",
    "drowsiness", "mood", "hr", "snr", "stillness",
];

// ── Compare insights ─────────────────────────────────────────────────────────

/// Compute per-metric stats, deltas, and trends for an A/B session comparison.
///
/// Fetches timeseries internally; takes already-computed aggregate metrics by
/// reference to avoid duplicate work.
pub(crate) fn compute_compare_insights(
    skill_dir: &std::path::Path,
    a_start: u64, a_end: u64,
    b_start: u64, b_end: u64,
    avg_a: &SessionMetrics,
    avg_b: &SessionMetrics,
) -> serde_json::Value {
    let ts_a = get_session_timeseries_impl(skill_dir, a_start, a_end);
    let ts_b = get_session_timeseries_impl(skill_dir, b_start, b_end);

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
            "a": r2f(ma), "b": r2f(mb),
            "abs": r2f(abs_delta), "pct": r2f(pct),
            "direction": direction,
        }));
        match direction {
            "up"   => improved.push(metric.into()),
            "down" => declined.push(metric.into()),
            _      => stable.push(metric.into()),
        }
    }

    serde_json::json!({
        "stats_a": stats_a,
        "stats_b": stats_b,
        "deltas": deltas,
        "improved": improved,
        "declined": declined,
        "stable": stable,
        "n_epochs_a": ts_a.len(),
        "n_epochs_b": ts_b.len(),
    })
}

// ── Sleep analysis ───────────────────────────────────────────────────────────

/// Compute derived sleep-quality metrics from classified sleep stages.
pub(crate) fn analyze_sleep_stages(stages: &SleepStages) -> serde_json::Value {
    let epochs = &stages.epochs;
    let summary = &stages.summary;
    if epochs.is_empty() { return serde_json::json!(null); }

    let epoch_secs = if summary.epoch_secs > 0.0 { summary.epoch_secs } else { 5.0 };
    let total = summary.total_epochs as f64;
    let wake  = summary.wake_epochs as f64;

    // Sleep efficiency: (total − wake) / total × 100
    let efficiency = if total > 0.0 { (total - wake) / total * 100.0 } else { 0.0 };

    // Stage durations in minutes
    let stage_minutes = serde_json::json!({
        "wake": r2f(wake * epoch_secs / 60.0),
        "n1":   r2f(summary.n1_epochs as f64 * epoch_secs / 60.0),
        "n2":   r2f(summary.n2_epochs as f64 * epoch_secs / 60.0),
        "n3":   r2f(summary.n3_epochs as f64 * epoch_secs / 60.0),
        "rem":  r2f(summary.rem_epochs as f64 * epoch_secs / 60.0),
        "total":r2f(total * epoch_secs / 60.0),
    });

    // Sleep onset latency: time from first epoch to first non-wake epoch
    let first_sleep_idx = epochs.iter().position(|e| e.stage != 0);
    let onset_latency_min = match first_sleep_idx {
        Some(idx) if idx > 0 => r2f(epochs[idx].utc.saturating_sub(epochs[0].utc) as f64 / 60.0),
        _ => 0.0,
    };

    // REM latency: time from sleep onset to first REM epoch
    let rem_latency_min = first_sleep_idx.and_then(|si| {
        let start = epochs[si].utc;
        epochs[si..].iter()
            .find(|e| e.stage == 5)
            .map(|e| r2f(e.utc.saturating_sub(start) as f64 / 60.0))
    });

    // Transitions and awakenings
    let mut transitions = 0u32;
    let mut awakenings  = 0u32;
    for w in epochs.windows(2) {
        if w[0].stage != w[1].stage {
            transitions += 1;
            if w[1].stage == 0 && w[0].stage != 0 { awakenings += 1; }
        }
    }

    // Bout analysis per stage
    let stage_ids: &[(u8, &str)] = &[(0,"wake"),(1,"n1"),(2,"n2"),(3,"n3"),(5,"rem")];
    let mut bouts = serde_json::Map::new();
    for &(sid, name) in stage_ids {
        let mut lengths: Vec<f64> = Vec::new();
        let mut cur = 0u32;
        for e in epochs {
            if e.stage == sid { cur += 1; }
            else {
                if cur > 0 { lengths.push(cur as f64 * epoch_secs / 60.0); }
                cur = 0;
            }
        }
        if cur > 0 { lengths.push(cur as f64 * epoch_secs / 60.0); }
        if !lengths.is_empty() {
            let count = lengths.len();
            let mean = lengths.iter().sum::<f64>() / count as f64;
            let max  = lengths.iter().cloned().fold(0.0f64, f64::max);
            bouts.insert(name.into(), serde_json::json!({
                "count": count, "mean_min": r2f(mean), "max_min": r2f(max),
            }));
        }
    }

    serde_json::json!({
        "efficiency_pct":     r2f(efficiency),
        "onset_latency_min":  onset_latency_min,
        "rem_latency_min":    rem_latency_min,
        "stage_minutes":      stage_minutes,
        "transitions":        transitions,
        "awakenings":         awakenings,
        "bouts":              bouts,
    })
}

// ── Search analysis ──────────────────────────────────────────────────────────

/// Compute search-result insights: distance stats, temporal distribution, top days.
pub(crate) fn analyze_search_results(result: &commands::SearchResult) -> serde_json::Value {
    use std::collections::HashMap;

    // Distance statistics across all neighbors
    let all_distances: Vec<f64> = result.results.iter()
        .flat_map(|q| q.neighbors.iter().map(|n| n.distance as f64))
        .collect();
    let distance_stats = metric_stats_vec(&all_distances);

    // Temporal distribution (hour-of-day histogram)
    let mut hour_dist: HashMap<u8, u32> = HashMap::new();
    let mut day_dist:  HashMap<String, u32> = HashMap::new();
    let mut all_utcs: Vec<u64> = Vec::new();

    for q in &result.results {
        for n in &q.neighbors {
            all_utcs.push(n.timestamp_unix);
            let hour = ((n.timestamp_unix % 86400) / 3600) as u8;
            *hour_dist.entry(hour).or_insert(0) += 1;
            *day_dist.entry(n.date.clone()).or_insert(0) += 1;
        }
    }

    let mut hourly = serde_json::Map::new();
    for h in 0..24u8 {
        if let Some(&c) = hour_dist.get(&h) {
            hourly.insert(format!("{h:02}"), c.into());
        }
    }

    let mut top_days: Vec<(String, u32)> = day_dist.into_iter().collect();
    top_days.sort_by(|a, b| b.1.cmp(&a.1));
    top_days.truncate(10);

    let time_span_hours = if all_utcs.len() >= 2 {
        let mn = *all_utcs.iter().min().unwrap();
        let mx = *all_utcs.iter().max().unwrap();
        mx.saturating_sub(mn) as f64 / 3600.0
    } else { 0.0 };

    // Neighbor metrics averages (from the subset that have metrics)
    let metric_names = ["relaxation","engagement","meditation","cognitive_load",
                        "drowsiness","hr","snr","mood"];
    let mut neighbor_metrics = serde_json::Map::new();
    for &name in &metric_names {
        let vals: Vec<f64> = result.results.iter()
            .flat_map(|q| q.neighbors.iter())
            .filter_map(|n| n.metrics.as_ref())
            .filter_map(|m| match name {
                "relaxation"     => m.relaxation,
                "engagement"     => m.engagement,
                "meditation"     => m.meditation,
                "cognitive_load" => m.cognitive_load,
                "drowsiness"     => m.drowsiness,
                "hr"             => m.hr,
                "snr"            => m.snr,
                "mood"           => m.mood,
                _ => None,
            })
            .collect();
        if !vals.is_empty() {
            neighbor_metrics.insert(name.into(), serde_json::json!(r2f(
                vals.iter().sum::<f64>() / vals.len() as f64
            )));
        }
    }

    serde_json::json!({
        "distance_stats":         distance_stats,
        "temporal_distribution":  hourly,
        "top_days":               top_days.iter().map(|(d,c)| serde_json::json!([d, c])).collect::<Vec<_>>(),
        "time_span_hours":        r2f(time_span_hours),
        "total_neighbors":        all_distances.len(),
        "neighbor_metrics":       neighbor_metrics,
    })
}

// ── UMAP analysis ────────────────────────────────────────────────────────────

// analyze_umap_points — moved to skill_router crate.

// ── Status history ───────────────────────────────────────────────────────────

/// Compute recording history stats: totals, streak, today vs 7-day average.
pub(crate) fn compute_status_history(
    skill_dir: &std::path::Path,
    sessions_json: &[serde_json::Value],
) -> serde_json::Value {
    if sessions_json.is_empty() { return serde_json::json!(null); }

    let now = unix_secs();
    let today_day = now / 86400;

    let mut total_secs   = 0u64;
    let mut longest_secs = 0u64;
    let mut day_set      = std::collections::BTreeSet::<u64>::new();
    let total_sessions   = sessions_json.len();
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
    let total_hours    = total_secs as f64 / 3600.0;
    let avg_session_min = if total_sessions > 0 { total_hours * 60.0 / total_sessions as f64 } else { 0.0 };

    // Streak: consecutive recording days ending today (or yesterday)
    let mut streak = 0u32;
    let mut check = today_day;
    loop {
        if day_set.contains(&check) {
            streak += 1;
            if check == 0 { break; }
            check -= 1;
        } else if check == today_day {
            // Today might not have data yet — check yesterday
            if check == 0 { break; }
            check -= 1;
        } else {
            break;
        }
    }

    // Today vs 7-day average
    let today_start = today_day * 86400;
    let week_start  = today_day.saturating_sub(7) * 86400;
    let today_metrics = get_session_metrics_impl(skill_dir, today_start, now);
    let week_metrics  = get_session_metrics_impl(skill_dir, week_start, now);

    let mut today_vs_avg = serde_json::Map::new();
    if today_metrics.n_epochs > 0 && week_metrics.n_epochs > 0 {
        for &metric in STATUS_METRICS {
            let tv = session_field(&today_metrics, metric);
            let wv = session_field(&week_metrics, metric);
            let delta_pct = if wv.abs() > 1e-6 { (tv - wv) / wv.abs() * 100.0 } else { 0.0 };
            let direction = if delta_pct > 5.0 { "up" } else if delta_pct < -5.0 { "down" } else { "stable" };
            today_vs_avg.insert(metric.into(), serde_json::json!({
                "today": r2f(tv), "avg_7d": r2f(wv),
                "delta_pct": r2f(delta_pct), "direction": direction,
            }));
        }
    }

    serde_json::json!({
        "total_sessions":        total_sessions,
        "total_recording_hours": r2f(total_hours),
        "total_epochs":          total_epochs,
        "recording_days":        recording_days,
        "current_streak_days":   streak,
        "longest_session_min":   r2f(longest_secs as f64 / 60.0),
        "avg_session_min":       r2f(avg_session_min),
        "today_vs_avg":          today_vs_avg,
    })
}

// ── UMAP embedding comparison ─────────────────────────────────────────────────

/// A single 2D point in UMAP space, tagged with its session (0 = A, 1 = B).
#[derive(Serialize, Deserialize, Clone, Debug)]
struct UmapPoint {
    x: f32,
    y: f32,
    /// Third UMAP dimension (3D projection).
    z: f32,
    /// 0 = session A, 1 = session B.
    session: u8,
    /// Unix seconds UTC of the source epoch.
    utc: u64,
    /// User-defined label text, if any label's EEG window overlaps this epoch.
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

// load_labels_range — moved to skill_router crate.

/// Result of UMAP projection comparing two sessions.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub(crate) struct UmapResult {
    points:  Vec<UmapPoint>,
    n_a:     usize,
    n_b:     usize,
    dim:     usize,
}

// load_embeddings_range — delegated to skill_router crate.
pub(crate) use skill_router::load_embeddings_range;

/// Tauri command: compute UMAP 3D projection (synchronous fallback).
#[tauri::command]
pub(crate) fn compute_umap_compare(
    a_start_utc: u64,
    a_end_utc:   u64,
    b_start_utc: u64,
    b_end_utc:   u64,
    state:       tauri::State<'_, Mutex<Box<AppState>>>,
) -> UmapResult {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    match ws_commands::umap_compute_inner(&skill_dir, a_start_utc, a_end_utc, b_start_utc, b_end_utc, None) {
        Ok(val) => serde_json::from_value(val).unwrap_or_default(),
        Err(e) => {
            eprintln!("[umap] compute error: {e}");
            UmapResult::default()
        }
    }
}

/// Enqueue a UMAP comparison as a background job.  Returns a ticket immediately
/// with the estimated completion time so the UI stays responsive.
#[tauri::command]
pub(crate) fn enqueue_umap_compare(
    a_start_utc: u64,
    a_end_utc:   u64,
    b_start_utc: u64,
    b_end_utc:   u64,
    state:       tauri::State<'_, Mutex<Box<AppState>>>,
    queue:       tauri::State<'_, std::sync::Arc<job_queue::JobQueue>>,
) -> job_queue::JobTicket {
    let skill_dir = state.lock_or_recover().skill_dir.clone();

    let n_a = load_embeddings_range(&skill_dir, a_start_utc, a_end_utc).len();
    let n_b = load_embeddings_range(&skill_dir, b_start_utc, b_end_utc).len();
    let n = n_a + n_b;
    // Time estimate: KNN is O(n²) on GPU, training is O(epochs × edges).
    let ucfg = load_umap_config(&skill_dir);
    let est_epochs = ucfg.n_epochs.clamp(50, 2000) as u64;
    let estimated_ms = 3000u64
        + (n as u64) * (n as u64) / 20_000
        + (n as u64) * est_epochs / 2000;

    let sd = skill_dir.clone();
    let prog_map = queue.progress_map();
    queue.submit_with_id(estimated_ms, move |job_id| {
        let pm = prog_map;
        let cb: Box<dyn Fn(fast_umap::EpochProgress) + Send> = Box::new(move |ep| {
            let mut map = pm.lock_or_recover();
            map.insert(job_id, job_queue::JobProgress {
                epoch:        ep.epoch,
                total_epochs: ep.total_epochs,
                loss:         ep.loss,
                best_loss:    ep.best_loss,
                elapsed_secs: ep.elapsed_secs,
                epoch_ms:     ep.epoch_ms,
            });
        });
        ws_commands::umap_compute_inner(&sd, a_start_utc, a_end_utc, b_start_utc, b_end_utc, Some(cb))
    })
}

/// Poll the job queue for a result by job id.
#[tauri::command]
pub(crate) fn poll_job(
    job_id: u64,
    queue:  tauri::State<'_, std::sync::Arc<job_queue::JobQueue>>,
) -> job_queue::JobPollResult {
    queue.poll(job_id)
}

/// Tauri IPC wrapper for [`get_session_metrics_impl`].
/// Runs on a blocking thread so the UI stays responsive.
#[tauri::command]
pub(crate) async fn get_session_metrics(
    start_utc: u64,
    end_utc:   u64,
    state:     tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<SessionMetrics, String> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    tokio::task::spawn_blocking(move || {
        get_session_metrics_impl(&skill_dir, start_utc, end_utc)
    }).await.map_err(|e| e.to_string())
}

/// Return per-epoch time-series data for charts.
/// Runs on a blocking thread so the UI stays responsive.
#[tauri::command]
pub(crate) async fn get_session_timeseries(
    start_utc: u64,
    end_utc:   u64,
    state:     tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<EpochRow>, String> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    tokio::task::spawn_blocking(move || {
        get_session_timeseries_impl(&skill_dir, start_utc, end_utc)
    }).await.map_err(|e| e.to_string())
}

/// Load metrics directly from a session's `_metrics.csv` file.
/// This is the primary path for history view — works without SQLite epochs.
/// Runs on a blocking thread so the UI stays responsive.
#[tauri::command]
pub(crate) async fn get_csv_metrics(csv_path: String) -> Result<Option<CsvMetricsResult>, String> {
    tokio::task::spawn_blocking(move || {
        load_csv_metrics_cached(std::path::Path::new(&csv_path))
    }).await.map_err(|e| e.to_string())
}

/// Batch-load metrics for multiple sessions in a single IPC call.
/// Returns a map of csv_path → CsvMetricsResult for all sessions that
/// have data.  Timeseries are downsampled to at most `max_ts_points`
/// (default 360) to keep the payload small for sparklines and heatmaps.
/// All file I/O runs on a blocking thread.
#[tauri::command]
pub(crate) async fn get_day_metrics_batch(
    csv_paths: Vec<String>,
    max_ts_points: Option<usize>,
) -> Result<std::collections::HashMap<String, CsvMetricsResult>, String> {
    tokio::task::spawn_blocking(move || {
        let cap = max_ts_points.unwrap_or(360);
        let mut out = std::collections::HashMap::with_capacity(csv_paths.len());
        for path in &csv_paths {
            if let Some(mut result) = load_csv_metrics_cached(std::path::Path::new(path)) {
                downsample_timeseries(&mut result.timeseries, cap);
                out.insert(path.clone(), result);
            }
        }
        out
    }).await.map_err(|e| e.to_string())
}

/// Downsample a timeseries to at most `max` points using LTTB-like
/// uniform stride selection (keeps first and last).
fn downsample_timeseries(ts: &mut Vec<EpochRow>, max: usize) {
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

// ── Disk cache for pre-computed metrics ────────────────────────────────────────

/// Cache file path: `muse_XXX.csv` → `muse_XXX_metrics_cache.json`
fn metrics_cache_path(csv_path: &std::path::Path) -> std::path::PathBuf {
    let stem = csv_path.file_stem().and_then(|s| s.to_str()).unwrap_or("muse");
    csv_path.with_file_name(format!("{stem}_metrics_cache.json"))
}

/// Load metrics from disk cache if valid, otherwise compute from CSV and cache.
fn load_csv_metrics_cached(csv_path: &std::path::Path) -> Option<CsvMetricsResult> {
    let metrics_csv = metrics_csv_path(csv_path);
    if !metrics_csv.exists() { return None; }

    let cache_path = metrics_cache_path(csv_path);

    // Check if cache exists and is newer than the metrics CSV.
    if cache_path.exists() {
        let csv_mtime = std::fs::metadata(&metrics_csv).ok()
            .and_then(|m| m.modified().ok());
        let cache_mtime = std::fs::metadata(&cache_path).ok()
            .and_then(|m| m.modified().ok());
        if let (Some(cm), Some(ca)) = (csv_mtime, cache_mtime) {
            if ca >= cm {
                // Cache is fresh — read it.
                if let Ok(data) = std::fs::read(&cache_path) {
                    if let Ok(result) = serde_json::from_slice::<CsvMetricsResult>(&data) {
                        return Some(result);
                    }
                }
            }
        }
    }

    // Cache miss — compute from CSV.
    let result = load_metrics_csv(csv_path)?;

    // Write cache asynchronously (best-effort).
    let cache_path_owned = cache_path.to_path_buf();
    let result_clone = result.clone();
    std::thread::spawn(move || {
        if let Ok(json) = serde_json::to_vec(&result_clone) {
            let _ = std::fs::write(&cache_path_owned, json);
        }
    });

    Some(result)
}

/// Open the session comparison window (or focus it if already open).
#[tauri::command]
pub(crate) async fn open_compare_window(app: AppHandle) -> Result<(), String> {
    crate::window_cmds::focus_or_create(&app, crate::window_cmds::WindowSpec {
        label: "compare", route: "compare", title: "NeuroSkill™ – Compare",
        inner_size: (780.0, 640.0), min_inner_size: Some((600.0, 440.0)),
        ..Default::default()
    })
}

/// Open compare window pre-selecting two specific sessions by their UTC ranges.
/// If the window is already open, emit an event to re-select and close/reopen.
#[tauri::command]
pub(crate) async fn open_compare_window_with_sessions(
    app: AppHandle,
    start_a: i64, end_a: i64,
    start_b: i64, end_b: i64,
) -> Result<(), String> {
    // Close the existing compare window if open so we can open with fresh URL.
    if let Some(win) = app.get_webview_window("compare") {
        let _ = win.close();
        // Give it a moment to close
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let url_path = format!(
        "compare?startA={}&endA={}&startB={}&endB={}",
        start_a, end_a, start_b, end_b
    );
    tauri::WebviewWindowBuilder::new(&app, "compare", tauri::WebviewUrl::App(url_path.into()))
        .title("NeuroSkill™ – Compare")
        .inner_size(780.0, 640.0)
        .min_inner_size(600.0, 440.0)
        .resizable(true)
        .center()
        .decorations(false).transparent(true)
        .build()
        .map(|w| { let _ = w.set_focus(); })
        .map_err(|e| e.to_string())
}
