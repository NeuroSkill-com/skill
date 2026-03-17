// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Persistent Apple HealthKit data store — `~/.skill/health.sqlite`.
//!
//! Stores data pushed from a companion iOS app over the HTTP/WS API:
//!
//! * **`sleep_samples`** — sleep analysis entries (asleep, awake, REM, core, deep)
//! * **`workouts`** — workout sessions (type, duration, calories, distance, HR)
//! * **`heart_rate_samples`** — discrete heart-rate readings (bpm + context)
//! * **`steps_samples`** — step-count aggregates over time ranges
//! * **`mindfulness_samples`** — mindful minutes / meditation sessions
//! * **`health_metrics`** — catch-all for any scalar HealthKit quantity
//!   (resting HR, HRV, VO2max, body mass, blood pressure, etc.)
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

use crate::util::MutexExt;

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
    pub value:     String,  // InBed, Asleep, Awake, REM, Core, Deep
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
        let conn = self.conn.lock_or_recover();
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

        // ── Sleep ─────────────────────────────────────────────────────────
        if !payload.sleep.is_empty() {
            let mut stmt = conn.prepare_cached(
                "INSERT OR IGNORE INTO sleep_samples (source_id, start_utc, end_utc, value, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)"
            ).unwrap();
            for s in &payload.sleep {
                if stmt.execute(params![s.source_id, s.start_utc, s.end_utc, s.value, now]).is_ok() {
                    result.sleep_upserted += 1;
                }
            }
        }

        // ── Workouts ──────────────────────────────────────────────────────
        if !payload.workouts.is_empty() {
            let mut stmt = conn.prepare_cached(
                "INSERT OR REPLACE INTO workouts
                 (source_id, workout_type, start_utc, end_utc, duration_secs,
                  total_calories, active_calories, distance_meters,
                  avg_heart_rate, max_heart_rate, metadata, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
            ).unwrap();
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

        // ── Heart rate ────────────────────────────────────────────────────
        if !payload.heart_rate.is_empty() {
            let mut stmt = conn.prepare_cached(
                "INSERT OR IGNORE INTO heart_rate_samples (source_id, timestamp, bpm, context, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)"
            ).unwrap();
            for hr in &payload.heart_rate {
                if stmt.execute(params![hr.source_id, hr.timestamp, hr.bpm, hr.context, now]).is_ok() {
                    result.heart_rate_upserted += 1;
                }
            }
        }

        // ── Steps ─────────────────────────────────────────────────────────
        if !payload.steps.is_empty() {
            let mut stmt = conn.prepare_cached(
                "INSERT OR IGNORE INTO steps_samples (source_id, start_utc, end_utc, count, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)"
            ).unwrap();
            for s in &payload.steps {
                if stmt.execute(params![s.source_id, s.start_utc, s.end_utc, s.count, now]).is_ok() {
                    result.steps_upserted += 1;
                }
            }
        }

        // ── Mindfulness ───────────────────────────────────────────────────
        if !payload.mindfulness.is_empty() {
            let mut stmt = conn.prepare_cached(
                "INSERT OR IGNORE INTO mindfulness_samples (source_id, start_utc, end_utc, created_at)
                 VALUES (?1, ?2, ?3, ?4)"
            ).unwrap();
            for m in &payload.mindfulness {
                if stmt.execute(params![m.source_id, m.start_utc, m.end_utc, now]).is_ok() {
                    result.mindfulness_upserted += 1;
                }
            }
        }

        // ── Generic metrics ───────────────────────────────────────────────
        if !payload.metrics.is_empty() {
            let mut stmt = conn.prepare_cached(
                "INSERT OR REPLACE INTO health_metrics
                 (source_id, metric_type, timestamp, value, unit, metadata, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
            ).unwrap();
            for m in &payload.metrics {
                let meta = m.metadata.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
                if stmt.execute(params![
                    m.source_id, m.metric_type, m.timestamp, m.value, m.unit, meta, now
                ]).is_ok() {
                    result.metrics_upserted += 1;
                }
            }
        }

        result
    }

    // ── Query helpers ─────────────────────────────────────────────────────────

    /// Query sleep samples in a time range.
    pub fn query_sleep(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<SleepRow> {
        let conn = self.conn.lock_or_recover();
        let mut stmt = conn.prepare(
            "SELECT id, source_id, start_utc, end_utc, value, created_at
             FROM sleep_samples WHERE start_utc >= ?1 AND start_utc <= ?2
             ORDER BY start_utc DESC LIMIT ?3"
        ).unwrap();
        stmt.query_map(params![start_utc, end_utc, limit], |row| Ok(SleepRow {
            id:         row.get(0)?,
            source_id:  row.get(1)?,
            start_utc:  row.get(2)?,
            end_utc:    row.get(3)?,
            value:      row.get(4)?,
            created_at: row.get(5)?,
        })).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    /// Query workouts in a time range.
    pub fn query_workouts(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<WorkoutRow> {
        let conn = self.conn.lock_or_recover();
        let mut stmt = conn.prepare(
            "SELECT id, source_id, workout_type, start_utc, end_utc, duration_secs,
                    total_calories, active_calories, distance_meters,
                    avg_heart_rate, max_heart_rate, metadata, created_at
             FROM workouts WHERE start_utc >= ?1 AND start_utc <= ?2
             ORDER BY start_utc DESC LIMIT ?3"
        ).unwrap();
        stmt.query_map(params![start_utc, end_utc, limit], |row| Ok(WorkoutRow {
            id:              row.get(0)?,
            source_id:       row.get(1)?,
            workout_type:    row.get(2)?,
            start_utc:       row.get(3)?,
            end_utc:         row.get(4)?,
            duration_secs:   row.get(5)?,
            total_calories:  row.get(6)?,
            active_calories: row.get(7)?,
            distance_meters: row.get(8)?,
            avg_heart_rate:  row.get(9)?,
            max_heart_rate:  row.get(10)?,
            metadata:        row.get(11)?,
            created_at:      row.get(12)?,
        })).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    /// Query heart rate samples in a time range.
    pub fn query_heart_rate(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<HeartRateRow> {
        let conn = self.conn.lock_or_recover();
        let mut stmt = conn.prepare(
            "SELECT id, source_id, timestamp, bpm, context, created_at
             FROM heart_rate_samples WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp DESC LIMIT ?3"
        ).unwrap();
        stmt.query_map(params![start_utc, end_utc, limit], |row| Ok(HeartRateRow {
            id:         row.get(0)?,
            source_id:  row.get(1)?,
            timestamp:  row.get(2)?,
            bpm:        row.get(3)?,
            context:    row.get(4)?,
            created_at: row.get(5)?,
        })).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    /// Query step samples in a time range.
    pub fn query_steps(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<StepsRow> {
        let conn = self.conn.lock_or_recover();
        let mut stmt = conn.prepare(
            "SELECT id, source_id, start_utc, end_utc, count, created_at
             FROM steps_samples WHERE start_utc >= ?1 AND start_utc <= ?2
             ORDER BY start_utc DESC LIMIT ?3"
        ).unwrap();
        stmt.query_map(params![start_utc, end_utc, limit], |row| Ok(StepsRow {
            id:         row.get(0)?,
            source_id:  row.get(1)?,
            start_utc:  row.get(2)?,
            end_utc:    row.get(3)?,
            count:      row.get(4)?,
            created_at: row.get(5)?,
        })).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    /// Query generic health metrics by type and time range.
    pub fn query_metrics(&self, metric_type: &str, start_utc: i64, end_utc: i64, limit: i64) -> Vec<HealthMetricRow> {
        let conn = self.conn.lock_or_recover();
        let mut stmt = conn.prepare(
            "SELECT id, source_id, metric_type, timestamp, value, unit, metadata, created_at
             FROM health_metrics WHERE metric_type = ?1 AND timestamp >= ?2 AND timestamp <= ?3
             ORDER BY timestamp DESC LIMIT ?4"
        ).unwrap();
        stmt.query_map(params![metric_type, start_utc, end_utc, limit], |row| Ok(HealthMetricRow {
            id:          row.get(0)?,
            source_id:   row.get(1)?,
            metric_type: row.get(2)?,
            timestamp:   row.get(3)?,
            value:       row.get(4)?,
            unit:        row.get(5)?,
            metadata:    row.get(6)?,
            created_at:  row.get(7)?,
        })).map(|rows| rows.flatten().collect()).unwrap_or_default()
    }

    /// List distinct metric types stored in the database.
    pub fn list_metric_types(&self) -> Vec<String> {
        let conn = self.conn.lock_or_recover();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT metric_type FROM health_metrics ORDER BY metric_type"
        ).unwrap();
        stmt.query_map([], |row| row.get(0))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    /// Summary statistics for a time range.
    pub fn summary(&self, start_utc: i64, end_utc: i64) -> serde_json::Value {
        let conn = self.conn.lock_or_recover();

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
            "start_utc":        start_utc,
            "end_utc":          end_utc,
            "sleep_samples":    sleep_count,
            "workouts":         workout_count,
            "heart_rate_samples": hr_count,
            "total_steps":      total_steps,
            "mindfulness_sessions": mindful_count,
            "metric_entries":   metric_count,
        })
    }
}
