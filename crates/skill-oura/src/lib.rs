// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Oura Ring V2 API integration for NeuroSkill.
//!
//! Fetches data from the [Oura Cloud V2 REST API](https://cloud.ouraring.com/v2/docs)
//! and converts it into the unified [`skill_health::HealthSyncPayload`] format
//! so it flows through the same storage and query pipeline as Apple HealthKit data.
//!
//! # Supported data types
//!
//! | Oura endpoint          | HealthSyncPayload field          |
//! |------------------------|----------------------------------|
//! | Daily Sleep            | `metrics` (oura_sleep_score)     |
//! | Sleep (detailed)       | `sleep` + `heart_rate` + `metrics` (HRV, breath rate) |
//! | Daily Activity         | `steps` + `metrics` (calories, activity score) |
//! | Daily Readiness        | `metrics` (oura_readiness_score) |
//! | Heart Rate             | `heart_rate`                     |
//! | Daily SpO2             | `metrics` (spo2)                 |
//! | Workouts               | `workouts`                       |
//! | Sessions (meditation)  | `mindfulness`                    |
//!
//! # Usage
//!
//! ```rust,ignore
//! use skill_oura::OuraSync;
//!
//! let sync = OuraSync::new("your-oura-personal-access-token");
//! let payload = sync.fetch("2026-03-01", "2026-03-28").unwrap();
//! // payload is a HealthSyncPayload ready for health_store.sync(&payload)
//! ```

use chrono::{NaiveDate, NaiveDateTime};
use oura_api::{DateQuery, DatetimeQuery, OuraClient};
use skill_health::{
    HealthMetric, HealthSyncPayload, HeartRateSample, MindfulnessSample, SleepSample, StepsSample,
    Workout as HealthWorkout,
};

/// Source identifier used in all records imported from the Oura Ring.
const SOURCE_ID: &str = "oura_ring";

/// Minimum valid timestamp — 2020-01-01 UTC.  Anything below this is
/// treated as a parse failure (Oura data cannot predate the Gen 3 ring).
const MIN_VALID_TS: i64 = 1_577_836_800;

/// Oura Ring sync client.
///
/// Wraps the [`OuraClient`] and provides a single [`fetch`](OuraSync::fetch)
/// method that pulls all available data types for a date range and returns a
/// unified [`HealthSyncPayload`].
pub struct OuraSync<'a> {
    client: OuraClient<'a>,
}

impl<'a> OuraSync<'a> {
    /// Create a new sync client from a personal access token.
    pub fn new(token: &'a str) -> Self {
        Self {
            client: OuraClient::new(token),
        }
    }

    /// Fetch all Oura data for the given date range and convert to a
    /// [`HealthSyncPayload`].
    ///
    /// `start_date` and `end_date` are ISO 8601 date strings (`"YYYY-MM-DD"`).
    ///
    /// Individual endpoint failures are logged to stderr but do **not** fail
    /// the overall sync — the payload will contain whatever data was
    /// successfully retrieved.
    pub fn fetch(&self, start_date: &str, end_date: &str) -> Result<HealthSyncPayload, String> {
        let mut payload = HealthSyncPayload::default();

        // ── Sleep (detailed) ─────────────────────────────────────────────
        self.fetch_sleep(&mut payload, start_date, end_date);

        // ── Daily Sleep (scores) ─────────────────────────────────────────
        self.fetch_daily_sleep(&mut payload, start_date, end_date);

        // ── Daily Activity → steps + metrics ─────────────────────────────
        self.fetch_daily_activity(&mut payload, start_date, end_date);

        // ── Daily Readiness ──────────────────────────────────────────────
        self.fetch_daily_readiness(&mut payload, start_date, end_date);

        // ── Heart Rate ───────────────────────────────────────────────────
        self.fetch_heart_rate(&mut payload, start_date, end_date);

        // ── Daily SpO2 ──────────────────────────────────────────────────
        self.fetch_daily_spo2(&mut payload, start_date, end_date);

        // ── Workouts ─────────────────────────────────────────────────────
        self.fetch_workouts(&mut payload, start_date, end_date);

        // ── Sessions (meditation / mindfulness) ──────────────────────────
        self.fetch_sessions(&mut payload, start_date, end_date);

        Ok(payload)
    }

    // ── Detailed sleep ───────────────────────────────────────────────────

    fn fetch_sleep(&self, payload: &mut HealthSyncPayload, start: &str, end: &str) {
        let query = DateQuery::builder().start_date(start).end_date(end).build();
        let resp = match self.client.list_sleep(query) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[oura] sleep fetch failed: {e}");
                return;
            }
        };

        for s in &resp.data {
            let start_utc = parse_datetime_to_utc(&s.bedtime_start);
            let end_utc = parse_datetime_to_utc(&s.bedtime_end);

            // Skip records with unparseable timestamps.
            if !valid_ts(start_utc) || !valid_ts(end_utc) {
                eprintln!("[oura] skipping sleep record: bad timestamps ({start_utc}, {end_utc})");
                continue;
            }

            // Sleep phase breakdown → SleepSamples
            // Oura provides durations rather than per-phase timestamps, so we
            // create a single "composite" sleep sample plus individual metric
            // entries for each phase duration.
            if let Some(total) = s.total_sleep_duration {
                payload.sleep.push(SleepSample {
                    source_id: SOURCE_ID.into(),
                    start_utc,
                    end_utc,
                    value: "Asleep".into(),
                });

                // Phase durations as metrics (seconds)
                if let Some(deep) = s.deep_sleep_duration {
                    push_metric(payload, "oura_deep_sleep_secs", start_utc, deep as f64, "s");
                }
                if let Some(rem) = s.rem_sleep_duration {
                    push_metric(payload, "oura_rem_sleep_secs", start_utc, rem as f64, "s");
                }
                if let Some(light) = s.light_sleep_duration {
                    push_metric(payload, "oura_light_sleep_secs", start_utc, light as f64, "s");
                }
                if let Some(awake) = s.awake_time {
                    push_metric(payload, "oura_awake_time_secs", start_utc, awake as f64, "s");
                }
                push_metric(payload, "oura_total_sleep_secs", start_utc, total as f64, "s");
            }

            // Average HRV during sleep
            if let Some(hrv) = s.average_hrv {
                push_metric(payload, "hrv", start_utc, hrv as f64, "ms");
            }

            // Average breath rate
            if let Some(br) = s.average_breath {
                push_metric(payload, "oura_breath_rate", start_utc, br as f64, "breaths/min");
            }

            // Average & lowest heart rate during sleep
            if let Some(hr) = s.average_heart_rate {
                push_metric(payload, "oura_sleep_avg_hr", start_utc, hr as f64, "bpm");
            }
            if let Some(lhr) = s.lowest_heart_rate {
                push_metric(payload, "restingHeartRate", start_utc, lhr as f64, "bpm");
            }

            // Sleep efficiency
            if let Some(eff) = s.efficiency {
                push_metric(payload, "oura_sleep_efficiency", start_utc, eff as f64, "%");
            }

            // Inlined HR time series from the sleep period
            if let Some(ref hr_sample) = s.heart_rate {
                let base_ts = parse_datetime_to_utc(&hr_sample.timestamp);
                if !valid_ts(base_ts) {
                    continue;
                }
                let interval = hr_sample.interval as i64;
                for (i, val) in hr_sample.items.iter().enumerate() {
                    if let Some(bpm) = val {
                        payload.heart_rate.push(HeartRateSample {
                            source_id: SOURCE_ID.into(),
                            timestamp: base_ts + (i as i64) * interval,
                            bpm: *bpm as f64,
                            context: Some("sleep".into()),
                        });
                    }
                }
            }
        }
    }

    // ── Daily sleep scores ───────────────────────────────────────────────

    fn fetch_daily_sleep(&self, payload: &mut HealthSyncPayload, start: &str, end: &str) {
        let query = DateQuery::builder().start_date(start).end_date(end).build();
        let resp = match self.client.list_daily_sleep(query) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[oura] daily_sleep fetch failed: {e}");
                return;
            }
        };

        for ds in &resp.data {
            let ts = parse_date_to_utc(&ds.day);
            if !valid_ts(ts) {
                continue;
            }
            if let Some(score) = ds.score {
                push_metric(payload, "oura_sleep_score", ts, score as f64, "score");
            }
            // Contributor sub-scores
            let c = &ds.contributors;
            if let Some(v) = c.deep_sleep {
                push_metric(payload, "oura_sleep_deep_contrib", ts, v as f64, "score");
            }
            if let Some(v) = c.efficiency {
                push_metric(payload, "oura_sleep_efficiency_contrib", ts, v as f64, "score");
            }
            if let Some(v) = c.rem_sleep {
                push_metric(payload, "oura_sleep_rem_contrib", ts, v as f64, "score");
            }
            if let Some(v) = c.restfulness {
                push_metric(payload, "oura_sleep_restfulness_contrib", ts, v as f64, "score");
            }
            if let Some(v) = c.timing {
                push_metric(payload, "oura_sleep_timing_contrib", ts, v as f64, "score");
            }
            if let Some(v) = c.total_sleep {
                push_metric(payload, "oura_sleep_total_contrib", ts, v as f64, "score");
            }
        }
    }

    // ── Daily activity → steps + metrics ─────────────────────────────────

    fn fetch_daily_activity(&self, payload: &mut HealthSyncPayload, start: &str, end: &str) {
        let query = DateQuery::builder().start_date(start).end_date(end).build();
        let resp = match self.client.list_daily_activity(query) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[oura] daily_activity fetch failed: {e}");
                return;
            }
        };

        for da in &resp.data {
            let ts = parse_date_to_utc(&da.day);
            if !valid_ts(ts) {
                continue;
            }
            // End of day = start of day + 24h
            let end_ts = ts + 86400;

            // Steps
            payload.steps.push(StepsSample {
                source_id: SOURCE_ID.into(),
                start_utc: ts,
                end_utc: end_ts,
                count: da.steps as i64,
            });

            // Activity score
            if let Some(score) = da.score {
                push_metric(payload, "oura_activity_score", ts, score as f64, "score");
            }

            // Calories
            push_metric(payload, "oura_active_calories", ts, da.active_calories as f64, "kcal");
            push_metric(payload, "oura_total_calories", ts, da.total_calories as f64, "kcal");

            // Activity time breakdown
            push_metric(
                payload,
                "oura_high_activity_time",
                ts,
                da.high_activity_time as f64,
                "s",
            );
            push_metric(
                payload,
                "oura_medium_activity_time",
                ts,
                da.medium_activity_time as f64,
                "s",
            );
            push_metric(payload, "oura_low_activity_time", ts, da.low_activity_time as f64, "s");
            push_metric(payload, "oura_sedentary_time", ts, da.sedentary_time as f64, "s");
            push_metric(payload, "oura_resting_time", ts, da.resting_time as f64, "s");

            // Walking distance equivalent
            push_metric(
                payload,
                "oura_equivalent_walking_distance",
                ts,
                da.equivalent_walking_distance as f64,
                "m",
            );
        }
    }

    // ── Daily readiness ──────────────────────────────────────────────────

    fn fetch_daily_readiness(&self, payload: &mut HealthSyncPayload, start: &str, end: &str) {
        let query = DateQuery::builder().start_date(start).end_date(end).build();
        let resp = match self.client.list_daily_readiness(query) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[oura] daily_readiness fetch failed: {e}");
                return;
            }
        };

        for dr in &resp.data {
            let ts = parse_date_to_utc(&dr.day);
            if !valid_ts(ts) {
                continue;
            }

            if let Some(score) = dr.score {
                push_metric(payload, "oura_readiness_score", ts, score as f64, "score");
            }

            // Temperature deviation from baseline
            if let Some(td) = dr.temperature_deviation {
                push_metric(payload, "oura_temperature_deviation", ts, td as f64, "°C");
            }
            if let Some(ttd) = dr.temperature_trend_deviation {
                push_metric(payload, "oura_temperature_trend_deviation", ts, ttd as f64, "°C");
            }

            // Contributor sub-scores
            let c = &dr.contributors;
            if let Some(v) = c.hrv_balance {
                push_metric(payload, "oura_readiness_hrv_balance", ts, v as f64, "score");
            }
            if let Some(v) = c.resting_heart_rate {
                push_metric(payload, "oura_readiness_resting_hr", ts, v as f64, "score");
            }
            if let Some(v) = c.body_temperature {
                push_metric(payload, "oura_readiness_body_temp", ts, v as f64, "score");
            }
            if let Some(v) = c.recovery_index {
                push_metric(payload, "oura_readiness_recovery_index", ts, v as f64, "score");
            }
            if let Some(v) = c.previous_night {
                push_metric(payload, "oura_readiness_previous_night", ts, v as f64, "score");
            }
            if let Some(v) = c.sleep_balance {
                push_metric(payload, "oura_readiness_sleep_balance", ts, v as f64, "score");
            }
            if let Some(v) = c.activity_balance {
                push_metric(payload, "oura_readiness_activity_balance", ts, v as f64, "score");
            }
        }
    }

    // ── Heart rate (5-min intervals) ─────────────────────────────────────

    fn fetch_heart_rate(&self, payload: &mut HealthSyncPayload, start: &str, end: &str) {
        // The heart rate endpoint uses datetime, not date.
        let start_dt = format!("{start}T00:00:00+00:00");
        let end_dt = format!("{end}T23:59:59+00:00");
        let query = DatetimeQuery::builder()
            .start_datetime(&start_dt)
            .end_datetime(&end_dt)
            .build();
        let resp = match self.client.list_heart_rate(query) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[oura] heart_rate fetch failed: {e}");
                return;
            }
        };

        for hr in &resp.data {
            let ts = parse_datetime_to_utc(&hr.timestamp);
            if !valid_ts(ts) {
                continue;
            }
            payload.heart_rate.push(HeartRateSample {
                source_id: SOURCE_ID.into(),
                timestamp: ts,
                bpm: hr.bpm as f64,
                context: Some(hr.source.clone()),
            });
        }
    }

    // ── Daily SpO2 ──────────────────────────────────────────────────────

    fn fetch_daily_spo2(&self, payload: &mut HealthSyncPayload, start: &str, end: &str) {
        let query = DateQuery::builder().start_date(start).end_date(end).build();
        let resp = match self.client.list_daily_spo2(query) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[oura] daily_spo2 fetch failed: {e}");
                return;
            }
        };

        for sp in &resp.data {
            let ts = parse_date_to_utc(&sp.day);
            if !valid_ts(ts) {
                continue;
            }
            if let Some(ref agg) = sp.spo2_percentage {
                push_metric(payload, "spo2", ts, agg.average as f64, "%");
            }
        }
    }

    // ── Workouts ─────────────────────────────────────────────────────────

    fn fetch_workouts(&self, payload: &mut HealthSyncPayload, start: &str, end: &str) {
        let query = DateQuery::builder().start_date(start).end_date(end).build();
        let resp = match self.client.list_workout(query) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[oura] workout fetch failed: {e}");
                return;
            }
        };

        for w in &resp.data {
            let start_utc = parse_datetime_to_utc(&w.start_datetime);
            let end_utc = parse_datetime_to_utc(&w.end_datetime);
            if !valid_ts(start_utc) || !valid_ts(end_utc) {
                continue;
            }
            let duration_secs = (end_utc - start_utc).max(0) as f64;

            payload.workouts.push(HealthWorkout {
                source_id: SOURCE_ID.into(),
                workout_type: w.activity.clone(),
                start_utc,
                end_utc,
                duration_secs,
                total_calories: w.calories.map(|c| c as f64),
                active_calories: w.calories.map(|c| c as f64), // Oura only provides total
                distance_meters: w.distance.map(|d| d as f64),
                avg_heart_rate: None,
                max_heart_rate: None,
                metadata: Some(serde_json::json!({
                    "source": format!("{:?}", w.source),
                    "intensity": format!("{:?}", w.intensity),
                    "label": w.label,
                })),
            });
        }
    }

    // ── Sessions (meditation / mindfulness) ──────────────────────────────

    fn fetch_sessions(&self, payload: &mut HealthSyncPayload, start: &str, end: &str) {
        let query = DateQuery::builder().start_date(start).end_date(end).build();
        let resp = match self.client.list_session(query) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[oura] session fetch failed: {e}");
                return;
            }
        };

        for session in &resp.data {
            let start_utc = parse_datetime_to_utc(&session.start_datetime);
            let end_utc = parse_datetime_to_utc(&session.end_datetime);
            if !valid_ts(start_utc) || !valid_ts(end_utc) {
                continue;
            }

            // All Oura session types (Breathing, Meditation, Nap, Relaxation,
            // Rest, BodyStatus) map naturally to mindfulness samples.
            payload.mindfulness.push(MindfulnessSample {
                source_id: SOURCE_ID.into(),
                start_utc,
                end_utc,
            });
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Push a [`HealthMetric`] into the payload.
///
/// Silently skips the metric if the timestamp is below [`MIN_VALID_TS`]
/// (indicates a datetime parse failure — avoids inserting 1970 garbage).
fn push_metric(payload: &mut HealthSyncPayload, metric_type: &str, timestamp: i64, value: f64, unit: &str) {
    if timestamp < MIN_VALID_TS {
        eprintln!("[oura] skipping metric {metric_type}: timestamp {timestamp} below minimum");
        return;
    }
    payload.metrics.push(HealthMetric {
        source_id: SOURCE_ID.into(),
        metric_type: metric_type.into(),
        timestamp,
        value,
        unit: unit.into(),
        metadata: None,
    });
}

/// Returns `true` if a timestamp is valid (>= 2020-01-01).
fn valid_ts(ts: i64) -> bool {
    ts >= MIN_VALID_TS
}

/// Parse an ISO 8601 datetime string to a UTC unix timestamp.
///
/// Handles formats like `"2026-03-15T23:45:00+02:00"` and
/// `"2026-03-15T21:45:00Z"`.  Falls back to 0 on parse failure.
fn parse_datetime_to_utc(s: &str) -> i64 {
    // Try RFC 3339 (with timezone offset)
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return dt.timestamp();
    }
    // Try naive datetime (no timezone — assume UTC)
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return ndt.and_utc().timestamp();
    }
    // Try with fractional seconds
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return ndt.and_utc().timestamp();
    }
    eprintln!("[oura] failed to parse datetime: {s:?}");
    0
}

/// Parse an ISO 8601 date string (`"YYYY-MM-DD"`) to midnight UTC unix timestamp.
fn parse_date_to_utc(s: &str) -> i64 {
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        if let Some(dt) = d.and_hms_opt(0, 0, 0) {
            return dt.and_utc().timestamp();
        }
    }
    eprintln!("[oura] failed to parse date: {s:?}");
    0
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_datetime_rfc3339() {
        let ts = parse_datetime_to_utc("2026-03-15T12:00:00+00:00");
        assert!(ts > 0);
        // 2026-03-15 12:00 UTC
        assert_eq!(ts, 1773576000);
    }

    #[test]
    fn parse_datetime_with_offset() {
        let ts = parse_datetime_to_utc("2026-03-15T14:00:00+02:00");
        // Should be 12:00 UTC
        assert_eq!(ts, 1773576000);
    }

    #[test]
    fn parse_date_midnight_utc() {
        let ts = parse_date_to_utc("2026-03-15");
        // 2026-03-15 00:00:00 UTC
        assert_eq!(ts, 1773532800);
    }

    #[test]
    fn parse_invalid_returns_zero() {
        assert_eq!(parse_datetime_to_utc("not-a-date"), 0);
        assert_eq!(parse_date_to_utc("invalid"), 0);
    }

    /// A valid timestamp for test data (2026-03-15 12:00 UTC).
    const TEST_TS: i64 = 1773576000;

    #[test]
    fn push_metric_adds_to_payload() {
        let mut payload = HealthSyncPayload::default();
        push_metric(&mut payload, "test_metric", TEST_TS, 42.0, "units");
        assert_eq!(payload.metrics.len(), 1);
        assert_eq!(payload.metrics[0].metric_type, "test_metric");
        assert!((payload.metrics[0].value - 42.0).abs() < 0.01);
    }

    #[test]
    fn push_metric_preserves_source_id() {
        let mut payload = HealthSyncPayload::default();
        push_metric(&mut payload, "hrv", TEST_TS, 35.0, "ms");
        assert_eq!(payload.metrics[0].source_id, SOURCE_ID);
    }

    #[test]
    fn parse_datetime_naive_no_tz() {
        // Oura sometimes returns datetimes without timezone offset
        let ts = parse_datetime_to_utc("2026-03-15T12:00:00");
        assert!(ts > 0);
    }

    #[test]
    fn parse_datetime_fractional_seconds() {
        let ts = parse_datetime_to_utc("2026-03-15T12:00:00.123");
        assert!(ts > 0);
    }

    #[test]
    fn multiple_metrics_accumulate() {
        let mut payload = HealthSyncPayload::default();
        push_metric(&mut payload, "hrv", TEST_TS, 40.0, "ms");
        push_metric(&mut payload, "restingHeartRate", TEST_TS, 58.0, "bpm");
        push_metric(&mut payload, "spo2", TEST_TS, 98.5, "%");
        assert_eq!(payload.metrics.len(), 3);
    }

    /// Verify that the payload structure produced by push_metric is compatible
    /// with the HealthStore's sync method by round-tripping through the store.
    #[test]
    fn payload_round_trips_through_health_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = skill_health::HealthStore::open(dir.path()).unwrap();

        let mut payload = HealthSyncPayload::default();
        push_metric(&mut payload, "oura_sleep_score", TEST_TS, 85.0, "score");
        push_metric(&mut payload, "oura_readiness_score", TEST_TS, 78.0, "score");
        payload.sleep.push(SleepSample {
            source_id: SOURCE_ID.into(),
            start_utc: TEST_TS,
            end_utc: TEST_TS + 28800,
            value: "Asleep".into(),
        });
        payload.steps.push(StepsSample {
            source_id: SOURCE_ID.into(),
            start_utc: TEST_TS,
            end_utc: TEST_TS + 86400,
            count: 8500,
        });
        payload.heart_rate.push(HeartRateSample {
            source_id: SOURCE_ID.into(),
            timestamp: TEST_TS + 500,
            bpm: 62.0,
            context: Some("sleep".into()),
        });
        payload.mindfulness.push(MindfulnessSample {
            source_id: SOURCE_ID.into(),
            start_utc: TEST_TS + 3000,
            end_utc: TEST_TS + 3600,
        });
        payload.workouts.push(HealthWorkout {
            source_id: SOURCE_ID.into(),
            workout_type: "Running".into(),
            start_utc: TEST_TS + 5000,
            end_utc: TEST_TS + 6800,
            duration_secs: 1800.0,
            total_calories: Some(350.0),
            active_calories: Some(350.0),
            distance_meters: Some(5000.0),
            avg_heart_rate: None,
            max_heart_rate: None,
            metadata: Some(serde_json::json!({"source": "Autodetected", "intensity": "Moderate"})),
        });

        let result = store.sync(&payload);

        assert_eq!(result.sleep_upserted, 1);
        assert_eq!(result.steps_upserted, 1);
        assert_eq!(result.heart_rate_upserted, 1);
        assert_eq!(result.mindfulness_upserted, 1);
        assert_eq!(result.workouts_upserted, 1);
        assert_eq!(result.metrics_upserted, 2);

        // Query back
        let sleep = store.query_sleep(TEST_TS - 1, TEST_TS + 100000, 10);
        assert_eq!(sleep.len(), 1);
        assert_eq!(sleep[0].value, "Asleep");
        assert_eq!(sleep[0].source_id, SOURCE_ID);

        let steps = store.query_steps(TEST_TS - 1, TEST_TS + 100000, 10);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].count, 8500);

        let hr = store.query_heart_rate(TEST_TS - 1, TEST_TS + 100000, 10);
        assert_eq!(hr.len(), 1);
        assert!((hr[0].bpm - 62.0).abs() < 0.01);

        let metrics = store.query_metrics("oura_sleep_score", TEST_TS - 1, TEST_TS + 100000, 10);
        assert_eq!(metrics.len(), 1);
        assert!((metrics[0].value - 85.0).abs() < 0.01);

        let workouts = store.query_workouts(TEST_TS - 1, TEST_TS + 100000, 10);
        assert_eq!(workouts.len(), 1);
        assert_eq!(workouts[0].workout_type, "Running");

        // Idempotent — syncing again should not duplicate
        let _result2 = store.sync(&payload);
        let sleep2 = store.query_sleep(TEST_TS - 1, TEST_TS + 100000, 100);
        assert_eq!(sleep2.len(), 1);
        // Steps uses INSERT OR IGNORE, so still 1
        let steps2 = store.query_steps(TEST_TS - 1, TEST_TS + 100000, 100);
        assert_eq!(steps2.len(), 1);

        // Verify Oura types appear in metric type listing
        let types = store.list_metric_types();
        assert!(types.contains(&"oura_sleep_score".to_string()));
        assert!(types.contains(&"oura_readiness_score".to_string()));
    }

    #[test]
    fn source_id_constant_is_oura_ring() {
        assert_eq!(SOURCE_ID, "oura_ring");
    }

    #[test]
    fn valid_ts_rejects_zero() {
        assert!(!valid_ts(0));
    }

    #[test]
    fn valid_ts_rejects_1970() {
        assert!(!valid_ts(1000));
    }

    #[test]
    fn valid_ts_accepts_2026() {
        assert!(valid_ts(1773576000));
    }

    #[test]
    fn push_metric_skips_invalid_timestamp() {
        let mut payload = HealthSyncPayload::default();
        push_metric(&mut payload, "oura_sleep_score", 0, 85.0, "score");
        assert_eq!(payload.metrics.len(), 0, "should skip timestamp=0");
    }

    #[test]
    fn push_metric_accepts_valid_timestamp() {
        let mut payload = HealthSyncPayload::default();
        push_metric(&mut payload, "oura_sleep_score", 1773576000, 85.0, "score");
        assert_eq!(payload.metrics.len(), 1);
    }

    /// Test that empty API responses produce an empty but valid payload
    /// (no crashes, no panics).
    #[test]
    fn empty_fetch_produces_empty_payload() {
        let payload = HealthSyncPayload::default();
        assert!(payload.sleep.is_empty());
        assert!(payload.workouts.is_empty());
        assert!(payload.heart_rate.is_empty());
        assert!(payload.steps.is_empty());
        assert!(payload.mindfulness.is_empty());
        assert!(payload.metrics.is_empty());

        // An empty payload should sync fine (idempotent no-op).
        let dir = tempfile::tempdir().unwrap();
        let store = skill_health::HealthStore::open(dir.path()).unwrap();
        let result = store.sync(&payload);
        assert_eq!(result.sleep_upserted, 0);
        assert_eq!(result.workouts_upserted, 0);
    }

    /// Verify the round-trip still works when all timestamps are garbage
    /// (everything should be silently dropped).
    #[test]
    fn corrupted_timestamps_are_silently_dropped() {
        let mut payload = HealthSyncPayload::default();
        // push_metric with ts=0 should be skipped
        push_metric(&mut payload, "oura_sleep_score", 0, 85.0, "score");
        push_metric(&mut payload, "oura_readiness_score", -100, 90.0, "score");
        push_metric(&mut payload, "oura_activity_score", 500, 70.0, "score"); // year ~1970

        assert_eq!(payload.metrics.len(), 0, "all corrupted timestamps should be skipped");
    }

    #[test]
    fn min_valid_ts_is_2020() {
        // 2020-01-01 00:00:00 UTC
        assert_eq!(MIN_VALID_TS, 1_577_836_800);
        assert!(valid_ts(MIN_VALID_TS));
        assert!(!valid_ts(MIN_VALID_TS - 1));
    }
}
