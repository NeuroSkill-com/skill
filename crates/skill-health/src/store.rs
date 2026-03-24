// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Persistent Apple HealthKit data store — `~/.skill/health.sqlite`.
//!
//! All timestamps are stored as UTC unix seconds (i64).  The iOS app is
//! expected to convert `HKSample.startDate`/`endDate` before sending.
//!
//! # Sync protocol
//!
//! The iOS companion app calls `POST /v1/health/sync` with a JSON body
//! containing arrays of typed samples.  The server upserts by
//! `(source_id, start_utc, end_utc)` so the same payload can be sent
//! repeatedly without creating duplicates (idempotent sync).

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

// ── Mutex helper ──────────────────────────────────────────────────────────────

/// Acquire a Mutex lock, recovering from poison.
fn lock_or_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ── Constants ─────────────────────────────────────────────────────────────────

/// Filename for the health database inside `~/.skill/`.
pub const HEALTH_SQLITE: &str = "health.sqlite";

// ── DDL ───────────────────────────────────────────────────────────────────────

const DDL: &str = "
CREATE TABLE IF NOT EXISTS sleep_samples (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id  TEXT    NOT NULL DEFAULT '',
    start_utc  INTEGER NOT NULL,
    end_utc    INTEGER NOT NULL,
    value      TEXT    NOT NULL,  -- InBed, Asleep, Awake, REM, Core, Deep
    created_at INTEGER NOT NULL,
    UNIQUE(source_id, start_utc, end_utc, value)
);
CREATE INDEX IF NOT EXISTS idx_sleep_start ON sleep_samples (start_utc);

CREATE TABLE IF NOT EXISTS workouts (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id         TEXT    NOT NULL DEFAULT '',
    workout_type      TEXT    NOT NULL,  -- e.g. Running, Walking, Cycling, HIIT, Yoga
    start_utc         INTEGER NOT NULL,
    end_utc           INTEGER NOT NULL,
    duration_secs     REAL    NOT NULL DEFAULT 0,
    total_calories    REAL,             -- kcal (active + basal)
    active_calories   REAL,             -- kcal (active energy only)
    distance_meters   REAL,
    avg_heart_rate    REAL,
    max_heart_rate    REAL,
    metadata          TEXT,             -- arbitrary JSON from HealthKit
    created_at        INTEGER NOT NULL,
    UNIQUE(source_id, start_utc, end_utc, workout_type)
);
CREATE INDEX IF NOT EXISTS idx_workouts_start ON workouts (start_utc);

CREATE TABLE IF NOT EXISTS heart_rate_samples (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id  TEXT    NOT NULL DEFAULT '',
    timestamp  INTEGER NOT NULL,
    bpm        REAL    NOT NULL,
    context    TEXT,                    -- e.g. sedentary, active, workout
    created_at INTEGER NOT NULL,
    UNIQUE(source_id, timestamp, context)
);
CREATE INDEX IF NOT EXISTS idx_hr_ts ON heart_rate_samples (timestamp);

CREATE TABLE IF NOT EXISTS steps_samples (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id  TEXT    NOT NULL DEFAULT '',
    start_utc  INTEGER NOT NULL,
    end_utc    INTEGER NOT NULL,
    count      INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(source_id, start_utc, end_utc)
);
CREATE INDEX IF NOT EXISTS idx_steps_start ON steps_samples (start_utc);

CREATE TABLE IF NOT EXISTS mindfulness_samples (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id  TEXT    NOT NULL DEFAULT '',
    start_utc  INTEGER NOT NULL,
    end_utc    INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(source_id, start_utc, end_utc)
);
CREATE INDEX IF NOT EXISTS idx_mindful_start ON mindfulness_samples (start_utc);

CREATE TABLE IF NOT EXISTS health_metrics (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id     TEXT    NOT NULL DEFAULT '',
    metric_type   TEXT    NOT NULL,  -- e.g. restingHeartRate, hrv, vo2Max, bodyMass, bloodPressureSystolic, ...
    timestamp     INTEGER NOT NULL,
    value         REAL    NOT NULL,
    unit          TEXT    NOT NULL DEFAULT '',
    metadata      TEXT,
    created_at    INTEGER NOT NULL,
    UNIQUE(source_id, metric_type, timestamp)
);
CREATE INDEX IF NOT EXISTS idx_hm_type_ts ON health_metrics (metric_type, timestamp);
";

// ── Data types ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SleepSample {
    #[serde(default)]
    pub source_id: String,
    pub start_utc: i64,
    pub end_utc:   i64,
    pub value:     String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Workout {
    #[serde(default)]
    pub source_id:       String,
    pub workout_type:    String,
    pub start_utc:       i64,
    pub end_utc:         i64,
    #[serde(default)]
    pub duration_secs:   f64,
    pub total_calories:  Option<f64>,
    pub active_calories: Option<f64>,
    pub distance_meters: Option<f64>,
    pub avg_heart_rate:  Option<f64>,
    pub max_heart_rate:  Option<f64>,
    pub metadata:        Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeartRateSample {
    #[serde(default)]
    pub source_id: String,
    pub timestamp: i64,
    pub bpm:       f64,
    pub context:   Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepsSample {
    #[serde(default)]
    pub source_id: String,
    pub start_utc: i64,
    pub end_utc:   i64,
    pub count:     i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MindfulnessSample {
    #[serde(default)]
    pub source_id: String,
    pub start_utc: i64,
    pub end_utc:   i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthMetric {
    #[serde(default)]
    pub source_id:   String,
    pub metric_type: String,
    pub timestamp:   i64,
    pub value:       f64,
    #[serde(default)]
    pub unit:        String,
    pub metadata:    Option<serde_json::Value>,
}

/// Batch sync payload sent by the iOS companion app.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HealthSyncPayload {
    #[serde(default)]
    pub sleep:       Vec<SleepSample>,
    #[serde(default)]
    pub workouts:    Vec<Workout>,
    #[serde(default)]
    pub heart_rate:  Vec<HeartRateSample>,
    #[serde(default)]
    pub steps:       Vec<StepsSample>,
    #[serde(default)]
    pub mindfulness: Vec<MindfulnessSample>,
    #[serde(default)]
    pub metrics:     Vec<HealthMetric>,
}

/// Summary returned after a sync.
#[derive(Clone, Debug, Serialize)]
pub struct SyncResult {
    pub sleep_upserted:       usize,
    pub workouts_upserted:    usize,
    pub heart_rate_upserted:  usize,
    pub steps_upserted:       usize,
    pub mindfulness_upserted: usize,
    pub metrics_upserted:     usize,
}

/// Row returned by query endpoints (includes the DB id).
#[derive(Clone, Debug, Serialize)]
pub struct SleepRow {
    pub id:         i64,
    pub source_id:  String,
    pub start_utc:  i64,
    pub end_utc:    i64,
    pub value:      String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkoutRow {
    pub id:              i64,
    pub source_id:       String,
    pub workout_type:    String,
    pub start_utc:       i64,
    pub end_utc:         i64,
    pub duration_secs:   f64,
    pub total_calories:  Option<f64>,
    pub active_calories: Option<f64>,
    pub distance_meters: Option<f64>,
    pub avg_heart_rate:  Option<f64>,
    pub max_heart_rate:  Option<f64>,
    pub metadata:        Option<String>,
    pub created_at:      i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct HeartRateRow {
    pub id:         i64,
    pub source_id:  String,
    pub timestamp:  i64,
    pub bpm:        f64,
    pub context:    Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct StepsRow {
    pub id:         i64,
    pub source_id:  String,
    pub start_utc:  i64,
    pub end_utc:    i64,
    pub count:      i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct HealthMetricRow {
    pub id:          i64,
    pub source_id:   String,
    pub metric_type: String,
    pub timestamp:   i64,
    pub value:       f64,
    pub unit:        String,
    pub metadata:    Option<String>,
    pub created_at:  i64,
}

// ── Store ─────────────────────────────────────────────────────────────────────

pub struct HealthStore {
    conn: Mutex<Connection>,
}

impl HealthStore {
    /// Open (or create) the health database inside `skill_dir`.
    pub fn open(skill_dir: &Path) -> Option<Self> {
        let path = skill_dir.join(HEALTH_SQLITE);
        let conn = match Connection::open(&path) {
            Ok(c)  => c,
            Err(e) => { eprintln!("[health] open {}: {e}", path.display()); return None; }
        };
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;").ok()?;
        conn.execute_batch(DDL).ok()?;
        Some(Self { conn: Mutex::new(conn) })
    }

    /// Upsert a batch of HealthKit samples (idempotent).
    pub fn sync(&self, payload: &HealthSyncPayload) -> SyncResult {
        let conn = lock_or_recover(&self.conn);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let mut result = SyncResult {
            sleep_upserted:       0,
            workouts_upserted:    0,
            heart_rate_upserted:  0,
            steps_upserted:       0,
            mindfulness_upserted: 0,
            metrics_upserted:     0,
        };

        if !payload.sleep.is_empty() {
            if let Ok(mut stmt) = conn.prepare_cached(
                "INSERT OR IGNORE INTO sleep_samples (source_id, start_utc, end_utc, value, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)"
            ) {
                for s in &payload.sleep {
                    if stmt.execute(params![s.source_id, s.start_utc, s.end_utc, s.value, now]).is_ok() {
                        result.sleep_upserted += 1;
                    }
                }
            }
        }

        if !payload.workouts.is_empty() {
            if let Ok(mut stmt) = conn.prepare_cached(
                "INSERT OR REPLACE INTO workouts
                 (source_id, workout_type, start_utc, end_utc, duration_secs,
                  total_calories, active_calories, distance_meters,
                  avg_heart_rate, max_heart_rate, metadata, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
            ) {
                for w in &payload.workouts {
                    let meta = w.metadata.as_ref().map(|m| serde_json::to_string(m).unwrap_or_default());
                    if stmt.execute(params![
                        w.source_id, w.workout_type, w.start_utc, w.end_utc, w.duration_secs,
                        w.total_calories, w.active_calories, w.distance_meters,
                        w.avg_heart_rate, w.max_heart_rate, meta, now
                    ]).is_ok() {
                        result.workouts_upserted += 1;
                    }
                }
            }
        }

        if !payload.heart_rate.is_empty() {
            if let Ok(mut stmt) = conn.prepare_cached(
                "INSERT OR IGNORE INTO heart_rate_samples (source_id, timestamp, bpm, context, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)"
            ) {
                for hr in &payload.heart_rate {
                    if stmt.execute(params![hr.source_id, hr.timestamp, hr.bpm, hr.context, now]).is_ok() {
                        result.heart_rate_upserted += 1;
                    }
                }
            }
        }

        if !payload.steps.is_empty() {
            if let Ok(mut stmt) = conn.prepare_cached(
                "INSERT OR IGNORE INTO steps_samples (source_id, start_utc, end_utc, count, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)"
            ) {
                for s in &payload.steps {
                    if stmt.execute(params![s.source_id, s.start_utc, s.end_utc, s.count, now]).is_ok() {
                        result.steps_upserted += 1;
                    }
                }
            }
        }

        if !payload.mindfulness.is_empty() {
            if let Ok(mut stmt) = conn.prepare_cached(
                "INSERT OR IGNORE INTO mindfulness_samples (source_id, start_utc, end_utc, created_at)
                 VALUES (?1, ?2, ?3, ?4)"
            ) {
                for m in &payload.mindfulness {
                    if stmt.execute(params![m.source_id, m.start_utc, m.end_utc, now]).is_ok() {
                        result.mindfulness_upserted += 1;
                    }
                }
            }
        }

        if !payload.metrics.is_empty() {
            if let Ok(mut stmt) = conn.prepare_cached(
                "INSERT OR REPLACE INTO health_metrics
                 (source_id, metric_type, timestamp, value, unit, metadata, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
            ) {
                for m in &payload.metrics {
                    let meta = m.metadata.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
                    if stmt.execute(params![
                        m.source_id, m.metric_type, m.timestamp, m.value, m.unit, meta, now
                    ]).is_ok() {
                        result.metrics_upserted += 1;
                    }
                }
            }
        }

        result
    }

    // ── Query helpers ─────────────────────────────────────────────────────────

    pub fn query_sleep(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<SleepRow> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, source_id, start_utc, end_utc, value, created_at
             FROM sleep_samples WHERE start_utc >= ?1 AND start_utc <= ?2
             ORDER BY start_utc DESC LIMIT ?3"
        ) else { return vec![] };
        stmt.query_map(params![start_utc, end_utc, limit], |row| Ok(SleepRow {
            id: row.get(0)?, source_id: row.get(1)?, start_utc: row.get(2)?,
            end_utc: row.get(3)?, value: row.get(4)?, created_at: row.get(5)?,
        })).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    pub fn query_workouts(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<WorkoutRow> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, source_id, workout_type, start_utc, end_utc, duration_secs,
                    total_calories, active_calories, distance_meters,
                    avg_heart_rate, max_heart_rate, metadata, created_at
             FROM workouts WHERE start_utc >= ?1 AND start_utc <= ?2
             ORDER BY start_utc DESC LIMIT ?3"
        ) else { return vec![] };
        stmt.query_map(params![start_utc, end_utc, limit], |row| Ok(WorkoutRow {
            id: row.get(0)?, source_id: row.get(1)?, workout_type: row.get(2)?,
            start_utc: row.get(3)?, end_utc: row.get(4)?, duration_secs: row.get(5)?,
            total_calories: row.get(6)?, active_calories: row.get(7)?,
            distance_meters: row.get(8)?, avg_heart_rate: row.get(9)?,
            max_heart_rate: row.get(10)?, metadata: row.get(11)?, created_at: row.get(12)?,
        })).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    pub fn query_heart_rate(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<HeartRateRow> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, source_id, timestamp, bpm, context, created_at
             FROM heart_rate_samples WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp DESC LIMIT ?3"
        ) else { return vec![] };
        stmt.query_map(params![start_utc, end_utc, limit], |row| Ok(HeartRateRow {
            id: row.get(0)?, source_id: row.get(1)?, timestamp: row.get(2)?,
            bpm: row.get(3)?, context: row.get(4)?, created_at: row.get(5)?,
        })).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    pub fn query_steps(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<StepsRow> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, source_id, start_utc, end_utc, count, created_at
             FROM steps_samples WHERE start_utc >= ?1 AND start_utc <= ?2
             ORDER BY start_utc DESC LIMIT ?3"
        ) else { return vec![] };
        stmt.query_map(params![start_utc, end_utc, limit], |row| Ok(StepsRow {
            id: row.get(0)?, source_id: row.get(1)?, start_utc: row.get(2)?,
            end_utc: row.get(3)?, count: row.get(4)?, created_at: row.get(5)?,
        })).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    pub fn query_metrics(&self, metric_type: &str, start_utc: i64, end_utc: i64, limit: i64) -> Vec<HealthMetricRow> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, source_id, metric_type, timestamp, value, unit, metadata, created_at
             FROM health_metrics WHERE metric_type = ?1 AND timestamp >= ?2 AND timestamp <= ?3
             ORDER BY timestamp DESC LIMIT ?4"
        ) else { return vec![] };
        stmt.query_map(params![metric_type, start_utc, end_utc, limit], |row| Ok(HealthMetricRow {
            id: row.get(0)?, source_id: row.get(1)?, metric_type: row.get(2)?,
            timestamp: row.get(3)?, value: row.get(4)?, unit: row.get(5)?,
            metadata: row.get(6)?, created_at: row.get(7)?,
        })).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    pub fn list_metric_types(&self) -> Vec<String> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare(
            "SELECT DISTINCT metric_type FROM health_metrics ORDER BY metric_type"
        ) else { return vec![] };
        stmt.query_map([], |row| row.get(0))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    pub fn summary(&self, start_utc: i64, end_utc: i64) -> serde_json::Value {
        let conn = lock_or_recover(&self.conn);

        let sleep_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sleep_samples WHERE start_utc >= ?1 AND start_utc <= ?2",
            params![start_utc, end_utc], |r| r.get(0),
        ).unwrap_or(0);

        let workout_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM workouts WHERE start_utc >= ?1 AND start_utc <= ?2",
            params![start_utc, end_utc], |r| r.get(0),
        ).unwrap_or(0);

        let hr_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM heart_rate_samples WHERE timestamp >= ?1 AND timestamp <= ?2",
            params![start_utc, end_utc], |r| r.get(0),
        ).unwrap_or(0);

        let total_steps: i64 = conn.query_row(
            "SELECT COALESCE(SUM(count), 0) FROM steps_samples WHERE start_utc >= ?1 AND start_utc <= ?2",
            params![start_utc, end_utc], |r| r.get(0),
        ).unwrap_or(0);

        let mindful_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mindfulness_samples WHERE start_utc >= ?1 AND start_utc <= ?2",
            params![start_utc, end_utc], |r| r.get(0),
        ).unwrap_or(0);

        let metric_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM health_metrics WHERE timestamp >= ?1 AND timestamp <= ?2",
            params![start_utc, end_utc], |r| r.get(0),
        ).unwrap_or(0);

        serde_json::json!({
            "start_utc":            start_utc,
            "end_utc":              end_utc,
            "sleep_samples":        sleep_count,
            "workouts":             workout_count,
            "heart_rate_samples":   hr_count,
            "total_steps":          total_steps,
            "mindfulness_sessions": mindful_count,
            "metric_entries":       metric_count,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, HealthStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = HealthStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn open_creates_database() {
        let (_dir, _store) = temp_store();
    }

    #[test]
    fn sync_empty_payload_is_noop() {
        let (_dir, store) = temp_store();
        let result = store.sync(&HealthSyncPayload::default());
        assert_eq!(result.sleep_upserted, 0);
        assert_eq!(result.workouts_upserted, 0);
    }

    #[test]
    fn sync_sleep_and_query() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            sleep: vec![SleepSample {
                source_id: "watch".into(),
                start_utc: 1000,
                end_utc:   2000,
                value:     "REM".into(),
            }],
            ..Default::default()
        };
        let result = store.sync(&payload);
        assert_eq!(result.sleep_upserted, 1);

        let rows = store.query_sleep(0, 3000, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].value, "REM");
    }

    #[test]
    fn sync_is_idempotent() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            sleep: vec![SleepSample {
                source_id: "watch".into(),
                start_utc: 1000,
                end_utc:   2000,
                value:     "Deep".into(),
            }],
            ..Default::default()
        };
        store.sync(&payload);
        store.sync(&payload);
        let rows = store.query_sleep(0, 3000, 100);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn sync_heart_rate_and_query() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            heart_rate: vec![HeartRateSample {
                source_id: "watch".into(),
                timestamp: 5000,
                bpm: 72.0,
                context: Some("sedentary".into()),
            }],
            ..Default::default()
        };
        store.sync(&payload);
        let rows = store.query_heart_rate(0, 10000, 10);
        assert_eq!(rows.len(), 1);
        assert!((rows[0].bpm - 72.0).abs() < 0.01);
    }

    #[test]
    fn sync_steps_and_query() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            steps: vec![StepsSample {
                source_id: "phone".into(),
                start_utc: 1000,
                end_utc:   2000,
                count:     9500,
            }],
            ..Default::default()
        };
        store.sync(&payload);
        let rows = store.query_steps(0, 3000, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].count, 9500);
    }

    #[test]
    fn sync_metrics_and_query() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            metrics: vec![HealthMetric {
                source_id: "watch".into(),
                metric_type: "restingHeartRate".into(),
                timestamp: 3000,
                value: 58.0,
                unit: "bpm".into(),
                metadata: None,
            }],
            ..Default::default()
        };
        store.sync(&payload);
        let rows = store.query_metrics("restingHeartRate", 0, 5000, 10);
        assert_eq!(rows.len(), 1);
        assert!((rows[0].value - 58.0).abs() < 0.01);
    }

    #[test]
    fn list_metric_types_returns_distinct() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            metrics: vec![
                HealthMetric { source_id: "".into(), metric_type: "hrv".into(), timestamp: 1, value: 40.0, unit: "ms".into(), metadata: None },
                HealthMetric { source_id: "".into(), metric_type: "restingHeartRate".into(), timestamp: 1, value: 60.0, unit: "bpm".into(), metadata: None },
                HealthMetric { source_id: "".into(), metric_type: "hrv".into(), timestamp: 2, value: 42.0, unit: "ms".into(), metadata: None },
            ],
            ..Default::default()
        };
        store.sync(&payload);
        let types = store.list_metric_types();
        assert_eq!(types, vec!["hrv", "restingHeartRate"]);
    }

    #[test]
    fn summary_aggregates_correctly() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            sleep: vec![
                SleepSample { source_id: "".into(), start_utc: 100, end_utc: 200, value: "REM".into() },
                SleepSample { source_id: "".into(), start_utc: 300, end_utc: 400, value: "Deep".into() },
            ],
            steps: vec![
                StepsSample { source_id: "".into(), start_utc: 100, end_utc: 200, count: 5000 },
                StepsSample { source_id: "".into(), start_utc: 300, end_utc: 400, count: 4500 },
            ],
            ..Default::default()
        };
        store.sync(&payload);
        let s = store.summary(0, 500);
        assert_eq!(s["sleep_samples"], 2);
        assert_eq!(s["total_steps"], 9500);
    }
}
