// SPDX-License-Identifier: GPL-3.0-only
// Database constants and DDL for HealthStore

pub const HEALTH_SQLITE: &str = "health.sqlite";

pub const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS sleep_samples (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id  TEXT    NOT NULL DEFAULT '',
    start_utc  INTEGER NOT NULL,
    end_utc    INTEGER NOT NULL,
    value      TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(source_id, start_utc, end_utc, value)
);
CREATE INDEX IF NOT EXISTS idx_sleep_start ON sleep_samples (start_utc);

CREATE TABLE IF NOT EXISTS workouts (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id         TEXT    NOT NULL DEFAULT '',
    workout_type      TEXT    NOT NULL,
    start_utc         INTEGER NOT NULL,
    end_utc           INTEGER NOT NULL,
    duration_secs     REAL    NOT NULL DEFAULT 0,
    total_calories    REAL,
    active_calories   REAL,
    distance_meters   REAL,
    avg_heart_rate    REAL,
    max_heart_rate    REAL,
    metadata          TEXT,
    created_at        INTEGER NOT NULL,
    UNIQUE(source_id, start_utc, end_utc, workout_type)
);
CREATE INDEX IF NOT EXISTS idx_workouts_start ON workouts (start_utc);

CREATE TABLE IF NOT EXISTS heart_rate_samples (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id  TEXT    NOT NULL DEFAULT '',
    timestamp  INTEGER NOT NULL,
    bpm        REAL    NOT NULL,
    context    TEXT,
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
    metric_type   TEXT    NOT NULL,
    timestamp     INTEGER NOT NULL,
    value         REAL    NOT NULL,
    unit          TEXT    NOT NULL DEFAULT '',
    metadata      TEXT,
    created_at    INTEGER NOT NULL,
    UNIQUE(source_id, metric_type, timestamp)
);
CREATE INDEX IF NOT EXISTS idx_hm_type_ts ON health_metrics (metric_type, timestamp);
"#;

#[cfg(feature = "gps")]
pub const DDL_GPS: &str = r#"
CREATE TABLE IF NOT EXISTS location_samples (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id           TEXT    NOT NULL DEFAULT '',
    timestamp           INTEGER NOT NULL,
    latitude            REAL    NOT NULL,
    longitude           REAL    NOT NULL,
    altitude            REAL,
    horizontal_accuracy REAL,
    vertical_accuracy   REAL,
    speed               REAL,
    course              REAL,
    created_at          INTEGER NOT NULL,
    UNIQUE(source_id, timestamp)
);
CREATE INDEX IF NOT EXISTS idx_loc_ts ON location_samples (timestamp);
"#;
