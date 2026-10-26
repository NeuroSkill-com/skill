// SPDX-License-Identifier: GPL-3.0-only
// Database constants and schema for HealthStore

/// The health database (DuckDB). The constant keeps its historical name.
pub const HEALTH_SQLITE: &str = "health.duckdb";

pub const SCHEMA: skill_db::StoreSchema<'static> = skill_db::StoreSchema {
    ddl: r#"
CREATE TABLE IF NOT EXISTS sleep_samples (
    id         BIGINT  PRIMARY KEY DEFAULT nextval('sleep_samples_id_seq'),
    source_id  VARCHAR    NOT NULL DEFAULT '',
    start_utc  BIGINT  NOT NULL,
    end_utc    BIGINT  NOT NULL,
    value      VARCHAR    NOT NULL,
    created_at BIGINT  NOT NULL,
    UNIQUE(source_id, start_utc, end_utc, value)
);
CREATE INDEX IF NOT EXISTS idx_sleep_start ON sleep_samples (start_utc);

CREATE TABLE IF NOT EXISTS workouts (
    id                BIGINT  PRIMARY KEY DEFAULT nextval('workouts_id_seq'),
    source_id         VARCHAR    NOT NULL DEFAULT '',
    workout_type      VARCHAR    NOT NULL,
    start_utc         BIGINT  NOT NULL,
    end_utc           BIGINT  NOT NULL,
    duration_secs     DOUBLE    NOT NULL DEFAULT 0,
    total_calories    DOUBLE,
    active_calories   DOUBLE,
    distance_meters   DOUBLE,
    avg_heart_rate    DOUBLE,
    max_heart_rate    DOUBLE,
    metadata          VARCHAR,
    created_at        BIGINT  NOT NULL,
    UNIQUE(source_id, start_utc, end_utc, workout_type)
);
CREATE INDEX IF NOT EXISTS idx_workouts_start ON workouts (start_utc);

CREATE TABLE IF NOT EXISTS heart_rate_samples (
    id         BIGINT  PRIMARY KEY DEFAULT nextval('heart_rate_samples_id_seq'),
    source_id  VARCHAR    NOT NULL DEFAULT '',
    timestamp  BIGINT  NOT NULL,
    bpm        DOUBLE    NOT NULL,
    context    VARCHAR,
    created_at BIGINT  NOT NULL,
    UNIQUE(source_id, timestamp, context)
);
CREATE INDEX IF NOT EXISTS idx_hr_ts ON heart_rate_samples (timestamp);

CREATE TABLE IF NOT EXISTS steps_samples (
    id         BIGINT  PRIMARY KEY DEFAULT nextval('steps_samples_id_seq'),
    source_id  VARCHAR    NOT NULL DEFAULT '',
    start_utc  BIGINT  NOT NULL,
    end_utc    BIGINT  NOT NULL,
    count      BIGINT  NOT NULL,
    created_at BIGINT  NOT NULL,
    UNIQUE(source_id, start_utc, end_utc)
);
CREATE INDEX IF NOT EXISTS idx_steps_start ON steps_samples (start_utc);

CREATE TABLE IF NOT EXISTS mindfulness_samples (
    id         BIGINT  PRIMARY KEY DEFAULT nextval('mindfulness_samples_id_seq'),
    source_id  VARCHAR    NOT NULL DEFAULT '',
    start_utc  BIGINT  NOT NULL,
    end_utc    BIGINT  NOT NULL,
    created_at BIGINT  NOT NULL,
    UNIQUE(source_id, start_utc, end_utc)
);
CREATE INDEX IF NOT EXISTS idx_mindful_start ON mindfulness_samples (start_utc);

CREATE TABLE IF NOT EXISTS health_metrics (
    id            BIGINT  PRIMARY KEY DEFAULT nextval('health_metrics_id_seq'),
    source_id     VARCHAR    NOT NULL DEFAULT '',
    metric_type   VARCHAR    NOT NULL,
    timestamp     BIGINT  NOT NULL,
    value         DOUBLE    NOT NULL,
    unit          VARCHAR    NOT NULL DEFAULT '',
    metadata      VARCHAR,
    created_at    BIGINT  NOT NULL,
    UNIQUE(source_id, metric_type, timestamp)
);
CREATE INDEX IF NOT EXISTS idx_hm_type_ts ON health_metrics (metric_type, timestamp);

-- Location fixes (only written when built with the `gps` feature).
CREATE TABLE IF NOT EXISTS location_samples (
    id                  BIGINT  PRIMARY KEY DEFAULT nextval('location_samples_id_seq'),
    source_id           VARCHAR    NOT NULL DEFAULT '',
    timestamp           BIGINT  NOT NULL,
    latitude            DOUBLE    NOT NULL,
    longitude           DOUBLE    NOT NULL,
    altitude            DOUBLE,
    horizontal_accuracy DOUBLE,
    vertical_accuracy   DOUBLE,
    speed               DOUBLE,
    course              DOUBLE,
    created_at          BIGINT  NOT NULL,
    UNIQUE(source_id, timestamp)
);
CREATE INDEX IF NOT EXISTS idx_loc_ts ON location_samples (timestamp);
"#,
    import_tables: &[
        "sleep_samples",
        "workouts",
        "heart_rate_samples",
        "steps_samples",
        "mindfulness_samples",
        "health_metrics",
        "location_samples",
    ],
    sequences: &[
        ("sleep_samples_id_seq", "sleep_samples", "id"),
        ("workouts_id_seq", "workouts", "id"),
        ("heart_rate_samples_id_seq", "heart_rate_samples", "id"),
        ("steps_samples_id_seq", "steps_samples", "id"),
        ("mindfulness_samples_id_seq", "mindfulness_samples", "id"),
        ("health_metrics_id_seq", "health_metrics", "id"),
        ("location_samples_id_seq", "location_samples", "id"),
    ],
};
