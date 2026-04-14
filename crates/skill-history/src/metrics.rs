// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Metrics loading and sleep staging — CSV and Parquet parsing, time-series.

use std::path::Path;

use crate::{find_metrics_path, CsvMetricsResult, EpochRow, SessionMetrics};

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
        .has_headers(true)
        .flexible(true)
        .from_path(&metrics_path)
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[csv-metrics] open error: {e}");
            return None;
        }
    };

    // Detect channel count from header: find the "faa" column to determine
    // where per-channel band powers end and cross-channel indices begin.
    let header = rdr.headers().ok()?.clone();
    let faa_idx = header.iter().position(|h| h == "faa").unwrap_or(49);
    let n_band_cols = faa_idx - 1; // columns 1..faa_idx are per-channel bands
    let n_ch = n_band_cols / 12; // 12 band columns per channel
    let ch_bases: Vec<usize> = (0..n_ch).map(|c| 1 + c * 12).collect();
    let x = faa_idx; // cross-channel offset

    let mut rows: Vec<EpochRow> = Vec::new();
    let mut sum = SessionMetrics::default();
    let mut count = 0usize;

    for result in rdr.records() {
        let Ok(rec) = result else { continue };
        if rec.len() < x + 23 {
            continue;
        } // need at least through laterality_index

        let f = |i: usize| -> f64 { rec.get(i).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) };

        let timestamp = f(0);
        if timestamp <= 0.0 {
            continue;
        }

        let avg_rel = |band_offset: usize| -> f64 {
            if ch_bases.is_empty() {
                return 0.0;
            }
            let mut s = 0.0;
            for &base in &ch_bases {
                s += f(base + 6 + band_offset);
            }
            s / ch_bases.len() as f64
        };

        let rd = avg_rel(0);
        let rt = avg_rel(1);
        let ra = avg_rel(2);
        let rb = avg_rel(3);
        let rg = avg_rel(4);

        let faa_v = f(x);
        let tar_v = f(x + 1);
        let bar_v = f(x + 2);
        let dtr_v = f(x + 3);
        let pse_v = f(x + 4);
        let apf_v = f(x + 5);
        let bps_v = f(x + 6);
        let snr_v = f(x + 7);
        let coh_v = f(x + 8);
        let mu_v = f(x + 9);
        let mood_v = f(x + 10);
        let tbr_v = f(x + 11);
        let sef_v = f(x + 12);
        let sc_v = f(x + 13);
        let ha_v = f(x + 14);
        let hm_v = f(x + 15);
        let hc_v = f(x + 16);
        let pe_v = f(x + 17);
        let hfd_v = f(x + 18);
        let dfa_v = f(x + 19);
        let se_v = f(x + 20);
        let pac_v = f(x + 21);
        let lat_v = f(x + 22);
        let hr_v = f(x + 23);
        let rmssd_v = f(x + 24);
        let sdnn_v = f(x + 25);
        let pnn_v = f(x + 26);
        let lfhf_v = f(x + 27);
        let resp_v = f(x + 28);
        let spo_v = f(x + 29);
        let perf_v = f(x + 30);
        let stress_v = f(x + 31);
        let blinks_v = f(x + 32);
        let blink_r_v = f(x + 33);
        let pitch_v = f(x + 34);
        let roll_v = f(x + 35);
        let still_v = f(x + 36);
        let nods_v = f(x + 37);
        let shakes_v = f(x + 38);
        let med_v = f(x + 39);
        let cog_v = f(x + 40);
        let drow_v = f(x + 41);
        let gpu_v = f(x + 43);
        let gpu_r_v = f(x + 44);
        let gpu_t_v = f(x + 45);

        let mut sr = 0.0f64;
        let mut se2 = 0.0f64;
        for &ch_base in &ch_bases {
            let a = f(ch_base + 6 + 2);
            let b = f(ch_base + 6 + 3);
            let t = f(ch_base + 6 + 1);
            let d1 = a + t;
            let d2 = b + t;
            if d1 > 1e-6 {
                se2 += b / d1;
            }
            if d2 > 1e-6 {
                sr += a / d2;
            }
        }
        let relax_v = sigmoid100((sr / 4.0) as f32, 2.5, 1.0) as f64;
        let engage_v = sigmoid100((se2 / 4.0) as f32, 2.0, 0.8) as f64;

        let row = EpochRow {
            t: timestamp,
            rd,
            rt,
            ra,
            rb,
            rg,
            relaxation: relax_v,
            engagement: engage_v,
            faa: faa_v,
            tar: tar_v,
            bar: bar_v,
            dtr: dtr_v,
            tbr: tbr_v,
            pse: pse_v,
            apf: apf_v,
            sef95: sef_v,
            sc: sc_v,
            bps: bps_v,
            snr: snr_v,
            coherence: coh_v,
            mu: mu_v,
            ha: ha_v,
            hm: hm_v,
            hc: hc_v,
            pe: pe_v,
            hfd: hfd_v,
            dfa: dfa_v,
            se: se_v,
            pac: pac_v,
            lat: lat_v,
            mood: mood_v,
            hr: hr_v,
            rmssd: rmssd_v,
            sdnn: sdnn_v,
            pnn50: pnn_v,
            lf_hf: lfhf_v,
            resp: resp_v,
            spo2: spo_v,
            perf: perf_v,
            stress: stress_v,
            blinks: blinks_v,
            blink_r: blink_r_v,
            pitch: pitch_v,
            roll: roll_v,
            still: still_v,
            nods: nods_v,
            shakes: shakes_v,
            med: med_v,
            cog: cog_v,
            drow: drow_v,
            gpu: gpu_v,
            gpu_render: gpu_r_v,
            gpu_tiler: gpu_t_v,
        };

        sum.rel_delta += rd;
        sum.rel_theta += rt;
        sum.rel_alpha += ra;
        sum.rel_beta += rb;
        sum.rel_gamma += rg;
        sum.relaxation += relax_v;
        sum.engagement += engage_v;
        sum.faa += faa_v;
        sum.tar += tar_v;
        sum.bar += bar_v;
        sum.dtr += dtr_v;
        sum.tbr += tbr_v;
        sum.pse += pse_v;
        sum.apf += apf_v;
        sum.bps += bps_v;
        sum.snr += snr_v;
        sum.coherence += coh_v;
        sum.mu_suppression += mu_v;
        sum.mood += mood_v;
        sum.sef95 += sef_v;
        sum.spectral_centroid += sc_v;
        sum.hjorth_activity += ha_v;
        sum.hjorth_mobility += hm_v;
        sum.hjorth_complexity += hc_v;
        sum.permutation_entropy += pe_v;
        sum.higuchi_fd += hfd_v;
        sum.dfa_exponent += dfa_v;
        sum.sample_entropy += se_v;
        sum.pac_theta_gamma += pac_v;
        sum.laterality_index += lat_v;
        sum.hr += hr_v;
        sum.rmssd += rmssd_v;
        sum.sdnn += sdnn_v;
        sum.pnn50 += pnn_v;
        sum.lf_hf_ratio += lfhf_v;
        sum.respiratory_rate += resp_v;
        sum.spo2_estimate += spo_v;
        sum.perfusion_index += perf_v;
        sum.stress_index += stress_v;
        sum.blink_count += blinks_v;
        sum.blink_rate += blink_r_v;
        sum.head_pitch += pitch_v;
        sum.head_roll += roll_v;
        sum.stillness += still_v;
        sum.nod_count += nods_v;
        sum.shake_count += shakes_v;
        sum.meditation += med_v;
        sum.cognitive_load += cog_v;
        sum.drowsiness += drow_v;

        rows.push(row);
        count += 1;
    }

    if count == 0 {
        return None;
    }

    let n = count as f64;
    sum.n_epochs = count;
    sum.rel_delta /= n;
    sum.rel_theta /= n;
    sum.rel_alpha /= n;
    sum.rel_beta /= n;
    sum.rel_gamma /= n;
    sum.relaxation /= n;
    sum.engagement /= n;
    sum.faa /= n;
    sum.tar /= n;
    sum.bar /= n;
    sum.dtr /= n;
    sum.tbr /= n;
    sum.pse /= n;
    sum.apf /= n;
    sum.bps /= n;
    sum.snr /= n;
    sum.coherence /= n;
    sum.mu_suppression /= n;
    sum.mood /= n;
    sum.sef95 /= n;
    sum.spectral_centroid /= n;
    sum.hjorth_activity /= n;
    sum.hjorth_mobility /= n;
    sum.hjorth_complexity /= n;
    sum.permutation_entropy /= n;
    sum.higuchi_fd /= n;
    sum.dfa_exponent /= n;
    sum.sample_entropy /= n;
    sum.pac_theta_gamma /= n;
    sum.laterality_index /= n;
    sum.hr /= n;
    sum.rmssd /= n;
    sum.sdnn /= n;
    sum.pnn50 /= n;
    sum.lf_hf_ratio /= n;
    sum.respiratory_rate /= n;
    sum.spo2_estimate /= n;
    sum.perfusion_index /= n;
    sum.stress_index /= n;
    sum.blink_rate /= n;
    sum.head_pitch /= n;
    sum.head_roll /= n;
    sum.stillness /= n;
    sum.meditation /= n;
    sum.cognitive_load /= n;
    sum.drowsiness /= n;

    eprintln!("[csv-metrics] loaded {} rows from {}", count, metrics_path.display());
    Some(CsvMetricsResult {
        n_rows: count,
        summary: sum,
        timeseries: rows,
    })
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
                if i >= cols.len() {
                    return 0.0;
                }
                cols[i].map_or(0.0, |c| if c.is_null(row_idx) { 0.0 } else { c.value(row_idx) })
            };

            if n_cols < x + 23 {
                continue;
            }
            let timestamp = f(0);
            if timestamp <= 0.0 {
                continue;
            }

            let avg_rel = |band_offset: usize| -> f64 {
                if ch_bases.is_empty() {
                    return 0.0;
                }
                let mut s = 0.0;
                for &base in &ch_bases {
                    s += f(base + 6 + band_offset);
                }
                s / ch_bases.len() as f64
            };

            let rd = avg_rel(0);
            let rt = avg_rel(1);
            let ra = avg_rel(2);
            let rb = avg_rel(3);
            let rg = avg_rel(4);

            let faa_v = f(x);
            let tar_v = f(x + 1);
            let bar_v = f(x + 2);
            let dtr_v = f(x + 3);
            let pse_v = f(x + 4);
            let apf_v = f(x + 5);
            let bps_v = f(x + 6);
            let snr_v = f(x + 7);
            let coh_v = f(x + 8);
            let mu_v = f(x + 9);
            let mood_v = f(x + 10);
            let tbr_v = f(x + 11);
            let sef_v = f(x + 12);
            let sc_v = f(x + 13);
            let ha_v = f(x + 14);
            let hm_v = f(x + 15);
            let hc_v = f(x + 16);
            let pe_v = f(x + 17);
            let hfd_v = f(x + 18);
            let dfa_v = f(x + 19);
            let se_v = f(x + 20);
            let pac_v = f(x + 21);
            let lat_v = f(x + 22);
            let hr_v = f(x + 23);
            let rmssd_v = f(x + 24);
            let sdnn_v = f(x + 25);
            let pnn_v = f(x + 26);
            let lfhf_v = f(x + 27);
            let resp_v = f(x + 28);
            let spo_v = f(x + 29);
            let perf_v = f(x + 30);
            let stress_v = f(x + 31);
            let blinks_v = f(x + 32);
            let blink_r_v = f(x + 33);
            let pitch_v = f(x + 34);
            let roll_v = f(x + 35);
            let still_v = f(x + 36);
            let nods_v = f(x + 37);
            let shakes_v = f(x + 38);
            let med_v = f(x + 39);
            let cog_v = f(x + 40);
            let drow_v = f(x + 41);
            let gpu_v = f(x + 43);
            let gpu_r_v = f(x + 44);
            let gpu_t_v = f(x + 45);

            let mut sr = 0.0f64;
            let mut se2 = 0.0f64;
            for &ch_base in &ch_bases {
                let a = f(ch_base + 6 + 2);
                let b = f(ch_base + 6 + 3);
                let t = f(ch_base + 6 + 1);
                let d1 = a + t;
                let d2 = b + t;
                if d1 > 1e-6 {
                    se2 += b / d1;
                }
                if d2 > 1e-6 {
                    sr += a / d2;
                }
            }
            let relax_v = sigmoid100((sr / 4.0) as f32, 2.5, 1.0) as f64;
            let engage_v = sigmoid100((se2 / 4.0) as f32, 2.0, 0.8) as f64;

            let row = EpochRow {
                t: timestamp,
                rd,
                rt,
                ra,
                rb,
                rg,
                relaxation: relax_v,
                engagement: engage_v,
                faa: faa_v,
                tar: tar_v,
                bar: bar_v,
                dtr: dtr_v,
                tbr: tbr_v,
                pse: pse_v,
                apf: apf_v,
                sef95: sef_v,
                sc: sc_v,
                bps: bps_v,
                snr: snr_v,
                coherence: coh_v,
                mu: mu_v,
                ha: ha_v,
                hm: hm_v,
                hc: hc_v,
                pe: pe_v,
                hfd: hfd_v,
                dfa: dfa_v,
                se: se_v,
                pac: pac_v,
                lat: lat_v,
                mood: mood_v,
                hr: hr_v,
                rmssd: rmssd_v,
                sdnn: sdnn_v,
                pnn50: pnn_v,
                lf_hf: lfhf_v,
                resp: resp_v,
                spo2: spo_v,
                perf: perf_v,
                stress: stress_v,
                blinks: blinks_v,
                blink_r: blink_r_v,
                pitch: pitch_v,
                roll: roll_v,
                still: still_v,
                nods: nods_v,
                shakes: shakes_v,
                med: med_v,
                cog: cog_v,
                drow: drow_v,
                gpu: gpu_v,
                gpu_render: gpu_r_v,
                gpu_tiler: gpu_t_v,
            };

            sum.rel_delta += rd;
            sum.rel_theta += rt;
            sum.rel_alpha += ra;
            sum.rel_beta += rb;
            sum.rel_gamma += rg;
            sum.relaxation += relax_v;
            sum.engagement += engage_v;
            sum.faa += faa_v;
            sum.tar += tar_v;
            sum.bar += bar_v;
            sum.dtr += dtr_v;
            sum.tbr += tbr_v;
            sum.pse += pse_v;
            sum.apf += apf_v;
            sum.bps += bps_v;
            sum.snr += snr_v;
            sum.coherence += coh_v;
            sum.mu_suppression += mu_v;
            sum.mood += mood_v;
            sum.sef95 += sef_v;
            sum.spectral_centroid += sc_v;
            sum.hjorth_activity += ha_v;
            sum.hjorth_mobility += hm_v;
            sum.hjorth_complexity += hc_v;
            sum.permutation_entropy += pe_v;
            sum.higuchi_fd += hfd_v;
            sum.dfa_exponent += dfa_v;
            sum.sample_entropy += se_v;
            sum.pac_theta_gamma += pac_v;
            sum.laterality_index += lat_v;
            sum.hr += hr_v;
            sum.rmssd += rmssd_v;
            sum.sdnn += sdnn_v;
            sum.pnn50 += pnn_v;
            sum.lf_hf_ratio += lfhf_v;
            sum.respiratory_rate += resp_v;
            sum.spo2_estimate += spo_v;
            sum.perfusion_index += perf_v;
            sum.stress_index += stress_v;
            sum.blink_count += blinks_v;
            sum.blink_rate += blink_r_v;
            sum.head_pitch += pitch_v;
            sum.head_roll += roll_v;
            sum.stillness += still_v;
            sum.nod_count += nods_v;
            sum.shake_count += shakes_v;
            sum.meditation += med_v;
            sum.cognitive_load += cog_v;
            sum.drowsiness += drow_v;

            rows.push(row);
            count += 1;
        }
    }

    if count == 0 {
        return None;
    }
    let n = count as f64;
    sum.n_epochs = count;
    sum.rel_delta /= n;
    sum.rel_theta /= n;
    sum.rel_alpha /= n;
    sum.rel_beta /= n;
    sum.rel_gamma /= n;
    sum.relaxation /= n;
    sum.engagement /= n;
    sum.faa /= n;
    sum.tar /= n;
    sum.bar /= n;
    sum.dtr /= n;
    sum.tbr /= n;
    sum.pse /= n;
    sum.apf /= n;
    sum.bps /= n;
    sum.snr /= n;
    sum.coherence /= n;
    sum.mu_suppression /= n;
    sum.mood /= n;
    sum.sef95 /= n;
    sum.spectral_centroid /= n;
    sum.hjorth_activity /= n;
    sum.hjorth_mobility /= n;
    sum.hjorth_complexity /= n;
    sum.permutation_entropy /= n;
    sum.higuchi_fd /= n;
    sum.dfa_exponent /= n;
    sum.sample_entropy /= n;
    sum.pac_theta_gamma /= n;
    sum.laterality_index /= n;
    sum.hr /= n;
    sum.rmssd /= n;
    sum.sdnn /= n;
    sum.pnn50 /= n;
    sum.lf_hf_ratio /= n;
    sum.respiratory_rate /= n;
    sum.spo2_estimate /= n;
    sum.perfusion_index /= n;
    sum.stress_index /= n;
    sum.blink_rate /= n;
    sum.head_pitch /= n;
    sum.head_roll /= n;
    sum.stillness /= n;
    sum.meditation /= n;
    sum.cognitive_load /= n;
    sum.drowsiness /= n;

    eprintln!("[parquet-metrics] loaded {} rows from {}", count, path.display());
    Some(CsvMetricsResult {
        n_rows: count,
        summary: sum,
        timeseries: rows,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::{parse_first_ts_from_bytes, parse_last_ts_from_bytes, SessionJsonMeta};

    use super::sigmoid100;

    #[test]
    fn sigmoid100_at_midpoint_is_50() {
        let result = sigmoid100(5.0, 1.0, 5.0);
        assert!(
            (result - 50.0).abs() < 0.01,
            "sigmoid at midpoint should be ~50, got {result}"
        );
    }

    #[test]
    fn sigmoid100_at_zero_is_near_zero_with_positive_mid() {
        let result = sigmoid100(0.0, 1.0, 10.0);
        assert!(result < 1.0, "sigmoid at 0 with mid=10 should be near 0, got {result}");
    }

    #[test]
    fn sigmoid100_at_large_input_approaches_100() {
        let result = sigmoid100(100.0, 1.0, 5.0);
        assert!(
            result > 99.0,
            "sigmoid at large input should approach 100, got {result}"
        );
    }

    #[test]
    fn sigmoid100_steepness_affects_slope() {
        let gentle = sigmoid100(6.0, 0.5, 5.0);
        let steep = sigmoid100(6.0, 5.0, 5.0);
        // Both above 50 since x > mid, but steep should be closer to 100
        assert!(steep > gentle, "steeper k should give higher value at same offset");
    }

    #[test]
    fn sigmoid100_monotonically_increasing() {
        let a = sigmoid100(1.0, 1.0, 5.0);
        let b = sigmoid100(5.0, 1.0, 5.0);
        let c = sigmoid100(10.0, 1.0, 5.0);
        assert!(a < b, "should be monotonically increasing: {a} < {b}");
        assert!(b < c, "should be monotonically increasing: {b} < {c}");
    }

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

    // ── load_metrics_csv ─────────────────────────────────────────────────

    #[test]
    fn load_metrics_csv_returns_none_for_missing_file() {
        let result = super::load_metrics_csv(std::path::Path::new("/nonexistent/exg_1700000000.csv"));
        assert!(result.is_none());
    }

    #[test]
    fn load_metrics_csv_with_fixture() {
        use skill_data::session_csv::METRICS_CSV_HEADER;

        let dir = tempfile::tempdir().unwrap();
        // Create the expected _metrics.csv alongside a dummy EEG csv
        let eeg_path = dir.path().join("exg_1700000000.csv");
        std::fs::write(&eeg_path, "timestamp_s\n").unwrap();

        let metrics_path = dir.path().join("exg_1700000000_metrics.csv");
        let header = METRICS_CSV_HEADER.join(",");
        // Generate 5 rows of dummy data (all zeros except timestamp)
        let mut csv_data = header.clone();
        for i in 0..5 {
            csv_data.push('\n');
            let ts = 1700000000.0 + i as f64 * 5.0;
            csv_data.push_str(&ts.to_string());
            // Fill remaining 94 columns with 0.1
            for _ in 1..METRICS_CSV_HEADER.len() {
                csv_data.push_str(",0.1");
            }
        }
        std::fs::write(&metrics_path, &csv_data).unwrap();

        let result = super::load_metrics_csv(&eeg_path);
        assert!(result.is_some(), "should parse fixture CSV");
        let result = result.unwrap();
        assert!(!result.timeseries.is_empty(), "should have timeseries rows");
        assert!(result.summary.n_epochs > 0, "should have epochs");
    }

    #[test]
    fn load_metrics_csv_with_empty_metrics_file() {
        let dir = tempfile::tempdir().unwrap();
        let eeg_path = dir.path().join("exg_1700000000.csv");
        std::fs::write(&eeg_path, "timestamp_s\n").unwrap();

        let metrics_path = dir.path().join("exg_1700000000_metrics.csv");
        // Only header, no data rows
        use skill_data::session_csv::METRICS_CSV_HEADER;
        std::fs::write(&metrics_path, METRICS_CSV_HEADER.join(",")).unwrap();

        let result = super::load_metrics_csv(&eeg_path);
        // Should return None or empty timeseries (no data rows)
        if let Some(r) = result {
            assert!(r.timeseries.is_empty());
        }
    }
}
