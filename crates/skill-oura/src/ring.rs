// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Ring history events → [`HealthSyncPayload`].
//!
//! This is the cloud-free counterpart to [`crate::OuraSync`]: the same health
//! database, filled from the ring's own event stream over BLE instead of from
//! Oura's V2 REST API.  Pure and synchronous — the BLE drain that produces the
//! events lives in the daemon.
//!
//! # What is and is not converted
//!
//! Only events whose body layout upstream has **validated against real
//! captures** are mapped, because a plausible-looking number from a guessed
//! layout is indistinguishable from a real one once it is in the database:
//!
//! | Event | Becomes |
//! |---|---|
//! | `hrv_event` (`0x5d`) | heart rate samples + `hrv_rmssd` metrics, 5 min apart |
//! | `green_ibi_quality_event` (`0x80`) | heart rate samples (quality-gated by the decoder) |
//! | `temp_event` / `temp_period` / `sleep_temp` | `skin_temperature` metrics (°C) |
//! | `spo2_event` (`0x6f`) | `spo2` metrics (%) |
//! | `spo2_r_pi_event` (`0x8b`) | `perfusion_index` metrics (%) |
//! | `sleep_phase_*` (`0x4b`/`0x4e`/`0x5a`) | sleep stage samples, 30 s epochs |
//! | `sleep_period_information_2` (`0x6a`) | `respiratory_rate` metrics + HR samples |
//! | `activity_information` (`0x50`) | `met` metrics |
//! | `bedtime_period` (`0x76`) | an `inBed` sleep sample |
//!
//! Deliberately skipped:
//!
//! * **`ibi_and_amplitude_event` (`0x60`)** — upstream measures 7–13 % of its
//!   beats above 100 bpm during sleep on two different rings, which a Malik
//!   filter does not clean, so either the bit layout is partly wrong or the ring
//!   ships unfiltered beats. `0x80` carries a quality flag and is preferred.
//! * **The SpO2 R-ratio** from `0x8b` — turning it into a percentage needs
//!   Oura's own calibration curve, which is not in this repo. The perfusion
//!   index from the same event is already a percentage and is kept.
//! * **`real_steps` (`0x7e`/`0x7f`)** — the decoder emits
//!   `"_status": "part1_raw"`, i.e. unscaled fields, so there is no step count
//!   to store yet.
//! * Anything whose decode carries `"_status": "unvalidated"` or `"inferred"`,
//!   which [`is_trustworthy`] rejects generically so a future upstream decoder
//!   cannot silently start feeding the database guesses.
//!
//! # Dating
//!
//! Every event is stamped in ring deciseconds, which only become a wall clock
//! through [`RingClock`].  Events that cannot be dated are **dropped and
//! counted**, never stored with an approximate timestamp — see
//! [`RingSyncStats::undated`].

use serde_json::Value;

use skill_health::{HealthMetric, HealthSyncPayload, HeartRateSample, SleepSample};

use crate::ring_time::RingClock;

/// Source tag written on every row, so direct-BLE rows are distinguishable from
/// the cloud sync's and from Apple Health's.
pub const SOURCE_ID: &str = "oura_ring_ble";

/// Spacing of the samples inside an `hrv_event` body, in seconds.
///
/// The decoder reports `interval_min: 5`; this is the fallback when that field
/// is missing.
const HRV_INTERVAL_S: f64 = 300.0;

/// One sleep-hypnogram epoch, in seconds.
const SLEEP_EPOCH_S: i64 = 30;

/// `spo2_event` samples are 1 Hz.
const SPO2_INTERVAL_S: f64 = 1.0;

/// Plausibility bounds, applied before anything is stored.  A decoder that
/// mis-reads a body tends to produce values far outside human range, and this is
/// the last place to catch that before it reaches the database.
const HR_BPM_RANGE: std::ops::RangeInclusive<f64> = 25.0..=240.0;
const RMSSD_MS_RANGE: std::ops::RangeInclusive<f64> = 1.0..=300.0;
const SKIN_TEMP_C_RANGE: std::ops::RangeInclusive<f64> = 20.0..=45.0;
const SPO2_PCT_RANGE: std::ops::RangeInclusive<f64> = 50.0..=100.0;
const PERFUSION_PCT_RANGE: std::ops::RangeInclusive<f64> = 0.0..=20.0;
const RESP_RATE_RANGE: std::ops::RangeInclusive<f64> = 4.0..=40.0;
const MET_RANGE: std::ops::RangeInclusive<f64> = 0.0..=25.0;

/// What a drain produced, including what it had to throw away.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct RingSyncStats {
    /// Events seen.
    pub events: usize,
    /// Events whose body the decoders understand.
    pub decoded: usize,
    /// Events dropped because their ring timestamp could not be dated.
    pub undated: usize,
    /// Events skipped because the decode is flagged unvalidated or inferred.
    pub untrusted: usize,
    /// Clock anchors (`time_sync` / `rtc_beacon`) found in the batch.
    pub anchors: usize,
}

/// Whether a decoded body is trustworthy enough to store.
///
/// Upstream marks a decoder it has not confirmed against real bytes with
/// `"_status": "unvalidated"`, and a layout whose field *meanings* are guessed
/// with `"inferred"`. Both are useful for research and neither belongs in a
/// health database, so this rejects any `_status` other than an absent one.
fn is_trustworthy(decoded: &Value) -> bool {
    // Any `_status` at all is a disclaimer, so the test is simply its absence.
    // Deliberately not an allowlist of known-bad values: a new upstream
    // decoder arrives flagged, and it must stay out of the health database
    // until someone has looked at it here.
    decoded.get("_status").is_none()
}

/// Read an array of numbers under `key`, if present.
fn num_array(decoded: &Value, key: &str) -> Vec<f64> {
    decoded
        .get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default()
}

fn metric(metric_type: &str, timestamp: i64, value: f64, unit: &str) -> HealthMetric {
    HealthMetric {
        source_id: SOURCE_ID.to_string(),
        metric_type: metric_type.to_string(),
        timestamp,
        value,
        unit: unit.to_string(),
        metadata: None,
    }
}

fn heart_rate(timestamp: i64, bpm: f64, context: &str) -> HeartRateSample {
    HeartRateSample {
        source_id: SOURCE_ID.to_string(),
        timestamp,
        bpm,
        context: Some(context.to_string()),
    }
}

/// Convert a batch of ring events into a health payload.
///
/// `events` pairs each event with the Unix second at which it was drained, and
/// must be in **capture order** — [`RingClock`] relies on that to see reboots.
pub fn events_to_payload(events: &[(oura_protocol::events::RingEvent, i64)]) -> (HealthSyncPayload, RingSyncStats) {
    let clock = RingClock::build(events);
    let mut payload = HealthSyncPayload::default();
    let mut stats = RingSyncStats {
        events: events.len(),
        anchors: clock.anchor_count(),
        ..Default::default()
    };

    for (ev, captured_unix) in events {
        let Some(decoded) = ev.decoded.as_ref() else {
            continue;
        };
        stats.decoded += 1;

        if !is_trustworthy(decoded) {
            stats.untrusted += 1;
            continue;
        }

        // Everything below needs a wall clock. An event we cannot date is
        // dropped rather than stored at the time we happened to download it.
        let Some(base) = clock.dated_unix_s(i64::from(ev.timestamp), *captured_unix) else {
            stats.undated += 1;
            continue;
        };

        match ev.tag {
            // hrv_event: parallel HR / RMSSD arrays, one pair per window.
            0x5d => {
                let interval = decoded
                    .get("interval_min")
                    .and_then(Value::as_f64)
                    .map_or(HRV_INTERVAL_S, |m| m * 60.0);
                let hr = num_array(decoded, "hr_bpm");
                let rmssd = num_array(decoded, "rmssd_ms");
                for (i, bpm) in hr.iter().enumerate() {
                    let ts = (base + i as f64 * interval) as i64;
                    if HR_BPM_RANGE.contains(bpm) {
                        payload.heart_rate.push(heart_rate(ts, *bpm, "hrv_window"));
                    }
                }
                for (i, ms) in rmssd.iter().enumerate() {
                    let ts = (base + i as f64 * interval) as i64;
                    if RMSSD_MS_RANGE.contains(ms) {
                        payload.metrics.push(metric("hrv_rmssd", ts, *ms, "ms"));
                    }
                }
            }

            // green_ibi_quality_event: the decoder already drops poor-quality and
            // implausible beats, and reports one hr_bpm per surviving beat.
            0x80 => {
                for (i, bpm) in num_array(decoded, "hr_bpm").iter().enumerate() {
                    if HR_BPM_RANGE.contains(bpm) {
                        // Beats are irregular by nature; the event timestamp is
                        // the only time the ring gives for the group.
                        payload.heart_rate.push(heart_rate(base as i64 + i as i64, *bpm, "ibi"));
                    }
                }
            }

            // temp_event / temp_period / sleep_temp_event.
            0x46 | 0x69 | 0x75 => {
                for (i, c) in num_array(decoded, "temps_c").iter().enumerate() {
                    if SKIN_TEMP_C_RANGE.contains(c) {
                        payload
                            .metrics
                            .push(metric("skin_temperature", base as i64 + i as i64, *c, "degC"));
                    }
                }
            }

            // spo2_event: one percentage per second.
            0x6f => {
                for (i, pct) in num_array(decoded, "spo2_percent").iter().enumerate() {
                    let ts = (base + i as f64 * SPO2_INTERVAL_S) as i64;
                    if SPO2_PCT_RANGE.contains(pct) {
                        payload.metrics.push(metric("spo2", ts, *pct, "%"));
                    }
                }
            }

            // spo2_r_pi_event: the R-ratio needs Oura's calibration, the
            // perfusion index does not.
            0x8b => {
                if let Some(pi) = decoded.get("perfusion_index").and_then(Value::as_f64) {
                    if PERFUSION_PCT_RANGE.contains(&pi) {
                        payload.metrics.push(metric("perfusion_index", base as i64, pi, "%"));
                    }
                }
            }

            // sleep_phase_*: 2-bit hypnogram codes, one per 30 s epoch.
            // Consecutive equal stages are merged into one sample, which is what
            // the health schema wants (a stage with a duration, not per-epoch rows).
            0x4b | 0x4e | 0x5a => {
                let phases: Vec<String> = decoded
                    .get("phases")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                    .unwrap_or_default();
                payload.sleep.extend(merge_sleep_stages(&phases, base as i64));
            }

            // sleep_period_information_2: one per-window sleep sample.
            0x6a => {
                if let Some(breath) = decoded.get("breath").and_then(Value::as_f64) {
                    if RESP_RATE_RANGE.contains(&breath) {
                        payload
                            .metrics
                            .push(metric("respiratory_rate", base as i64, breath, "count/min"));
                    }
                }
                if let Some(hr) = decoded.get("average_hr").and_then(Value::as_f64) {
                    if HR_BPM_RANGE.contains(&hr) {
                        payload.heart_rate.push(heart_rate(base as i64, hr, "sleep"));
                    }
                }
            }

            // activity_information: per-bin MET levels.
            0x50 => {
                for (i, met) in num_array(decoded, "met").iter().enumerate() {
                    if MET_RANGE.contains(met) {
                        payload.metrics.push(metric("met", base as i64 + i as i64, *met, "MET"));
                    }
                }
            }

            // bedtime_period: the ring's own detected sleep window, as two ring
            // timestamps that each need dating in their own right.
            0x76 => {
                let start_ds = decoded.get("bedtime_start_ds").and_then(Value::as_i64);
                let end_ds = decoded.get("bedtime_end_ds").and_then(Value::as_i64);
                if let (Some(s), Some(e)) = (start_ds, end_ds) {
                    let start = clock.dated_unix_s(s, *captured_unix);
                    let end = clock.dated_unix_s(e, *captured_unix);
                    if let (Some(start), Some(end)) = (start, end) {
                        if end > start {
                            payload.sleep.push(SleepSample {
                                source_id: SOURCE_ID.to_string(),
                                start_utc: start as i64,
                                end_utc: end as i64,
                                value: "inBed".to_string(),
                            });
                        }
                    }
                }
            }

            _ => {}
        }
    }

    (payload, stats)
}

/// Collapse a per-epoch hypnogram into one sample per contiguous stage run.
fn merge_sleep_stages(phases: &[String], base_utc: i64) -> Vec<SleepSample> {
    let mut out: Vec<SleepSample> = Vec::new();
    let mut run_start = 0usize;

    for i in 0..phases.len() {
        let is_last = i + 1 == phases.len();
        if is_last || phases[i] != phases[i + 1] {
            out.push(SleepSample {
                source_id: SOURCE_ID.to_string(),
                start_utc: base_utc + run_start as i64 * SLEEP_EPOCH_S,
                end_utc: base_utc + (i as i64 + 1) * SLEEP_EPOCH_S,
                value: phases[i].clone(),
            });
            run_start = i + 1;
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use oura_protocol::events::RingEvent;

    const CAPTURED: i64 = 1_700_010_000;
    const ANCHOR_UNIX: i64 = 1_700_000_000;

    fn ev(tag: u8, ds: u32, decoded: Value) -> RingEvent {
        RingEvent {
            tag,
            name: oura_protocol::events::event_name(tag),
            timestamp: ds,
            body: Vec::new(),
            decoded: Some(decoded),
        }
    }

    /// Every batch needs an anchor or nothing in it can be dated.  Anchor at
    /// counter 0 == ANCHOR_UNIX, so counter `n` deciseconds == ANCHOR_UNIX + n/10.
    fn with_anchor(mut events: Vec<(RingEvent, i64)>) -> Vec<(RingEvent, i64)> {
        let anchor = ev(0x42, 0, serde_json::json!({ "unix_time": ANCHOR_UNIX }));
        events.insert(0, (anchor, CAPTURED));
        events
    }

    #[test]
    fn hrv_event_becomes_heart_rate_and_rmssd_five_minutes_apart() {
        let events = with_anchor(vec![(
            ev(
                0x5d,
                0,
                serde_json::json!({ "hr_bpm": [50, 52], "rmssd_ms": [80, 95], "interval_min": 5 }),
            ),
            CAPTURED,
        )]);
        let (payload, stats) = events_to_payload(&events);

        assert_eq!(payload.heart_rate.len(), 2);
        assert_eq!(payload.heart_rate[0].bpm, 50.0);
        assert_eq!(payload.heart_rate[0].timestamp, ANCHOR_UNIX);
        // Second sample is one 5-minute window later.
        assert_eq!(payload.heart_rate[1].timestamp, ANCHOR_UNIX + 300);
        assert_eq!(payload.heart_rate[0].source_id, SOURCE_ID);

        let rmssd: Vec<f64> = payload
            .metrics
            .iter()
            .filter(|m| m.metric_type == "hrv_rmssd")
            .map(|m| m.value)
            .collect();
        assert_eq!(rmssd, vec![80.0, 95.0]);
        assert_eq!(stats.undated, 0);
    }

    #[test]
    fn temperatures_become_skin_temperature_metrics() {
        let events = with_anchor(vec![(
            ev(0x46, 100, serde_json::json!({ "temps_c": [33.12, 33.25] })),
            CAPTURED,
        )]);
        let (payload, _) = events_to_payload(&events);

        let temps: Vec<f64> = payload
            .metrics
            .iter()
            .filter(|m| m.metric_type == "skin_temperature")
            .map(|m| m.value)
            .collect();
        assert_eq!(temps, vec![33.12, 33.25]);
        assert_eq!(payload.metrics[0].unit, "degC");
        assert_eq!(payload.metrics[0].timestamp, ANCHOR_UNIX + 10);
    }

    /// A hypnogram is stored as stage runs with durations, not one row per epoch.
    #[test]
    fn sleep_phases_merge_into_contiguous_stage_runs() {
        let events = with_anchor(vec![(
            ev(
                0x5a,
                0,
                serde_json::json!({ "header": 0, "phases": ["deep", "deep", "light", "rem"] }),
            ),
            CAPTURED,
        )]);
        let (payload, _) = events_to_payload(&events);

        assert_eq!(payload.sleep.len(), 3, "three runs from four epochs");
        assert_eq!(payload.sleep[0].value, "deep");
        assert_eq!(payload.sleep[0].start_utc, ANCHOR_UNIX);
        assert_eq!(payload.sleep[0].end_utc, ANCHOR_UNIX + 60, "two 30 s epochs");
        assert_eq!(payload.sleep[1].value, "light");
        assert_eq!(payload.sleep[1].start_utc, ANCHOR_UNIX + 60);
        assert_eq!(payload.sleep[2].value, "rem");
        assert_eq!(payload.sleep[2].end_utc, ANCHOR_UNIX + 120);
    }

    #[test]
    fn sleep_period_info_yields_respiratory_rate_and_heart_rate() {
        let events = with_anchor(vec![(
            ev(
                0x6a,
                0,
                serde_json::json!({ "average_hr": 48.5, "breath": 14.25, "sleep_state": 1 }),
            ),
            CAPTURED,
        )]);
        let (payload, _) = events_to_payload(&events);

        assert_eq!(payload.heart_rate.len(), 1);
        assert_eq!(payload.heart_rate[0].bpm, 48.5);
        assert_eq!(payload.heart_rate[0].context.as_deref(), Some("sleep"));
        let resp = payload.metrics.iter().find(|m| m.metric_type == "respiratory_rate");
        assert_eq!(resp.map(|m| m.value), Some(14.25));
    }

    #[test]
    fn bedtime_period_becomes_an_in_bed_sample() {
        let events = with_anchor(vec![(
            ev(
                0x76,
                0,
                serde_json::json!({ "bedtime_start_ds": 1000, "bedtime_end_ds": 4000 }),
            ),
            CAPTURED,
        )]);
        let (payload, _) = events_to_payload(&events);

        let in_bed = payload.sleep.iter().find(|s| s.value == "inBed").unwrap();
        assert_eq!(in_bed.start_utc, ANCHOR_UNIX + 100);
        assert_eq!(in_bed.end_utc, ANCHOR_UNIX + 400);
    }

    /// The noisy IBI event must not reach the database — see the module docs.
    #[test]
    fn ibi_and_amplitude_event_is_skipped() {
        let events = with_anchor(vec![(
            ev(
                0x60,
                0,
                serde_json::json!({ "hr_bpm": [41, 139], "ibi_ms": [1463, 431] }),
            ),
            CAPTURED,
        )]);
        let (payload, _) = events_to_payload(&events);
        assert!(payload.heart_rate.is_empty(), "0x60 must not be stored");
    }

    /// An unvalidated or inferred decode is research data, not health data.
    #[test]
    fn flagged_decodes_are_rejected() {
        for status in ["unvalidated", "inferred", "part1_raw"] {
            let events = with_anchor(vec![(
                ev(
                    0x5d,
                    0,
                    serde_json::json!({ "hr_bpm": [60], "rmssd_ms": [50], "_status": status }),
                ),
                CAPTURED,
            )]);
            let (payload, stats) = events_to_payload(&events);
            assert!(payload.heart_rate.is_empty(), "status {status} must be rejected");
            assert_eq!(stats.untrusted, 1);
        }
    }

    /// A `_status` that is not a string is still a disclaimer.  Checking only
    /// string values would let a numeric or structured status through.
    #[test]
    fn a_non_string_status_is_also_rejected() {
        let events = with_anchor(vec![(
            ev(0x5d, 0, serde_json::json!({ "hr_bpm": [60], "_status": 3 })),
            CAPTURED,
        )]);
        let (payload, stats) = events_to_payload(&events);
        assert!(payload.heart_rate.is_empty());
        assert_eq!(stats.untrusted, 1);
    }

    /// Implausible values mean a mis-read body; they must not be stored.
    #[test]
    fn out_of_range_values_are_dropped() {
        let events = with_anchor(vec![
            (
                ev(
                    0x5d,
                    0,
                    serde_json::json!({ "hr_bpm": [0, 300], "rmssd_ms": [0, 9000] }),
                ),
                CAPTURED,
            ),
            (ev(0x46, 0, serde_json::json!({ "temps_c": [-30.0, 70.0] })), CAPTURED),
            (ev(0x6f, 0, serde_json::json!({ "spo2_percent": [0, 150] })), CAPTURED),
        ]);
        let (payload, _) = events_to_payload(&events);
        assert!(payload.heart_rate.is_empty());
        assert!(payload.metrics.is_empty(), "got {:?}", payload.metrics);
    }

    /// Without an anchor nothing can be dated, and nothing may be stored.
    #[test]
    fn a_batch_without_an_anchor_stores_nothing_and_says_so() {
        let events = vec![(
            ev(0x5d, 500, serde_json::json!({ "hr_bpm": [60], "rmssd_ms": [50] })),
            CAPTURED,
        )];
        let (payload, stats) = events_to_payload(&events);

        assert!(payload.heart_rate.is_empty());
        assert!(payload.metrics.is_empty());
        assert_eq!(stats.undated, 1);
        assert_eq!(stats.anchors, 0);
        assert_eq!(stats.events, 1);
    }

    #[test]
    fn an_empty_batch_is_an_empty_payload() {
        let (payload, stats) = events_to_payload(&[]);
        assert!(payload.heart_rate.is_empty());
        assert!(payload.sleep.is_empty());
        assert!(payload.metrics.is_empty());
        assert_eq!(stats, RingSyncStats::default());
    }

    #[test]
    fn spo2_samples_are_one_second_apart() {
        let events = with_anchor(vec![(
            ev(0x6f, 0, serde_json::json!({ "spo2_percent": [97, 96, 98] })),
            CAPTURED,
        )]);
        let (payload, _) = events_to_payload(&events);
        let spo2: Vec<(i64, f64)> = payload
            .metrics
            .iter()
            .filter(|m| m.metric_type == "spo2")
            .map(|m| (m.timestamp, m.value))
            .collect();
        assert_eq!(
            spo2,
            vec![(ANCHOR_UNIX, 97.0), (ANCHOR_UNIX + 1, 96.0), (ANCHOR_UNIX + 2, 98.0)]
        );
    }
}
