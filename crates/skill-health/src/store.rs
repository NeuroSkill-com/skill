use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

use super::store_db::*;
use super::store_types::*;
use super::store_util::*;

pub struct HealthStore {
    conn: Mutex<Connection>,
}

/// Upsert a batch of HealthKit samples (idempotent).
impl HealthStore {
    /// Open (or create) the health database inside `skill_dir`.
    pub fn open(skill_dir: &Path) -> Option<Self> {
        let path = skill_dir.join(HEALTH_SQLITE);
        let conn = match Connection::open(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[health] open {}: {e}", path.display());
                return None;
            }
        };
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .ok()?;
        conn.execute_batch(DDL).ok()?;
        #[cfg(feature = "gps")]
        if let Err(e) = conn.execute_batch(DDL_GPS) {
            eprintln!("[health] GPS DDL failed — location data will not be stored: {e}");
        }
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
            sleep_upserted: 0,
            workouts_upserted: 0,
            heart_rate_upserted: 0,
            steps_upserted: 0,
            mindfulness_upserted: 0,
            metrics_upserted: 0,
            #[cfg(feature = "gps")]
            location_upserted: 0,
        };

        if !payload.sleep.is_empty() {
            if let Ok(mut stmt) = conn.prepare_cached(
                "INSERT OR IGNORE INTO sleep_samples (source_id, start_utc, end_utc, value, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            ) {
                for s in &payload.sleep {
                    if stmt
                        .execute(params![s.source_id, s.start_utc, s.end_utc, s.value, now])
                        .is_ok()
                    {
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
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            ) {
                for w in &payload.workouts {
                    let meta = w
                        .metadata
                        .as_ref()
                        .map(|m| serde_json::to_string(m).unwrap_or_default());
                    if stmt
                        .execute(params![
                            w.source_id,
                            w.workout_type,
                            w.start_utc,
                            w.end_utc,
                            w.duration_secs,
                            w.total_calories,
                            w.active_calories,
                            w.distance_meters,
                            w.avg_heart_rate,
                            w.max_heart_rate,
                            meta,
                            now
                        ])
                        .is_ok()
                    {
                        result.workouts_upserted += 1;
                    }
                }
            }
        }

        if !payload.heart_rate.is_empty() {
            if let Ok(mut stmt) = conn.prepare_cached(
                "INSERT OR IGNORE INTO heart_rate_samples (source_id, timestamp, bpm, context, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            ) {
                for hr in &payload.heart_rate {
                    if stmt
                        .execute(params![hr.source_id, hr.timestamp, hr.bpm, hr.context, now])
                        .is_ok()
                    {
                        result.heart_rate_upserted += 1;
                    }
                }
            }
        }

        if !payload.steps.is_empty() {
            if let Ok(mut stmt) = conn.prepare_cached(
                "INSERT OR IGNORE INTO steps_samples (source_id, start_utc, end_utc, count, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            ) {
                for s in &payload.steps {
                    if stmt
                        .execute(params![s.source_id, s.start_utc, s.end_utc, s.count, now])
                        .is_ok()
                    {
                        result.steps_upserted += 1;
                    }
                }
            }
        }

        if !payload.mindfulness.is_empty() {
            if let Ok(mut stmt) = conn.prepare_cached(
                "INSERT OR IGNORE INTO mindfulness_samples (source_id, start_utc, end_utc, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
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
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            ) {
                for m in &payload.metrics {
                    let meta = m
                        .metadata
                        .as_ref()
                        .map(|v| serde_json::to_string(v).unwrap_or_default());
                    if stmt
                        .execute(params![
                            m.source_id,
                            m.metric_type,
                            m.timestamp,
                            m.value,
                            m.unit,
                            meta,
                            now
                        ])
                        .is_ok()
                    {
                        result.metrics_upserted += 1;
                    }
                }
            }
        }

        #[cfg(feature = "gps")]
        if !payload.location.is_empty() {
            if let Ok(mut stmt) = conn.prepare_cached(
                "INSERT OR IGNORE INTO location_samples
                 (source_id, timestamp, latitude, longitude, altitude,
                  horizontal_accuracy, vertical_accuracy, speed, course, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            ) {
                for loc in &payload.location {
                    if !loc.is_valid() {
                        continue;
                    }
                    if stmt
                        .execute(params![
                            loc.source_id,
                            loc.timestamp,
                            loc.latitude,
                            loc.longitude,
                            loc.altitude,
                            loc.horizontal_accuracy,
                            loc.vertical_accuracy,
                            loc.speed,
                            loc.course,
                            now
                        ])
                        .is_ok()
                    {
                        result.location_upserted += 1;
                    }
                }
            }
        }

        result
    }

    pub fn query_sleep(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<SleepRow> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, source_id, start_utc, end_utc, value, created_at
             FROM sleep_samples WHERE start_utc >= ?1 AND start_utc <= ?2
             ORDER BY start_utc DESC LIMIT ?3",
        ) else {
            return vec![];
        };
        stmt.query_map(params![start_utc, end_utc, limit], |row| {
            Ok(SleepRow {
                id: row.get(0)?,
                source_id: row.get(1)?,
                start_utc: row.get(2)?,
                end_utc: row.get(3)?,
                value: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    pub fn query_workouts(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<WorkoutRow> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, source_id, workout_type, start_utc, end_utc, duration_secs,
                    total_calories, active_calories, distance_meters,
                    avg_heart_rate, max_heart_rate, metadata, created_at
             FROM workouts WHERE start_utc >= ?1 AND start_utc <= ?2
             ORDER BY start_utc DESC LIMIT ?3",
        ) else {
            return vec![];
        };
        stmt.query_map(params![start_utc, end_utc, limit], |row| {
            Ok(WorkoutRow {
                id: row.get(0)?,
                source_id: row.get(1)?,
                workout_type: row.get(2)?,
                start_utc: row.get(3)?,
                end_utc: row.get(4)?,
                duration_secs: row.get(5)?,
                total_calories: row.get(6)?,
                active_calories: row.get(7)?,
                distance_meters: row.get(8)?,
                avg_heart_rate: row.get(9)?,
                max_heart_rate: row.get(10)?,
                metadata: row.get(11)?,
                created_at: row.get(12)?,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    pub fn query_heart_rate(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<HeartRateRow> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, source_id, timestamp, bpm, context, created_at
             FROM heart_rate_samples WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp DESC LIMIT ?3",
        ) else {
            return vec![];
        };
        stmt.query_map(params![start_utc, end_utc, limit], |row| {
            Ok(HeartRateRow {
                id: row.get(0)?,
                source_id: row.get(1)?,
                timestamp: row.get(2)?,
                bpm: row.get(3)?,
                context: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    pub fn query_steps(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<StepsRow> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, source_id, start_utc, end_utc, count, created_at
             FROM steps_samples WHERE start_utc >= ?1 AND start_utc <= ?2
             ORDER BY start_utc DESC LIMIT ?3",
        ) else {
            return vec![];
        };
        stmt.query_map(params![start_utc, end_utc, limit], |row| {
            Ok(StepsRow {
                id: row.get(0)?,
                source_id: row.get(1)?,
                start_utc: row.get(2)?,
                end_utc: row.get(3)?,
                count: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    pub fn query_metrics(&self, metric_type: &str, start_utc: i64, end_utc: i64, limit: i64) -> Vec<HealthMetricRow> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, source_id, metric_type, timestamp, value, unit, metadata, created_at
             FROM health_metrics WHERE metric_type = ?1 AND timestamp >= ?2 AND timestamp <= ?3
             ORDER BY timestamp DESC LIMIT ?4",
        ) else {
            return vec![];
        };
        stmt.query_map(params![metric_type, start_utc, end_utc, limit], |row| {
            Ok(HealthMetricRow {
                id: row.get(0)?,
                source_id: row.get(1)?,
                metric_type: row.get(2)?,
                timestamp: row.get(3)?,
                value: row.get(4)?,
                unit: row.get(5)?,
                metadata: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    #[cfg(feature = "gps")]
    pub fn query_location(&self, start_utc: i64, end_utc: i64, limit: i64) -> Vec<LocationRow> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, source_id, timestamp, latitude, longitude, altitude,
                    horizontal_accuracy, vertical_accuracy, speed, course, created_at
             FROM location_samples WHERE timestamp >= ?1 AND timestamp <= ?2
             ORDER BY timestamp DESC LIMIT ?3",
        ) else {
            return vec![];
        };
        stmt.query_map(params![start_utc, end_utc, limit], |row| {
            Ok(LocationRow {
                id: row.get(0)?,
                source_id: row.get(1)?,
                timestamp: row.get(2)?,
                latitude: row.get(3)?,
                longitude: row.get(4)?,
                altitude: row.get(5)?,
                horizontal_accuracy: row.get(6)?,
                vertical_accuracy: row.get(7)?,
                speed: row.get(8)?,
                course: row.get(9)?,
                created_at: row.get(10)?,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    pub fn list_metric_types(&self) -> Vec<String> {
        let conn = lock_or_recover(&self.conn);
        let Ok(mut stmt) = conn.prepare("SELECT DISTINCT metric_type FROM health_metrics ORDER BY metric_type") else {
            return vec![];
        };
        stmt.query_map([], |row| row.get(0))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    pub fn summary(&self, start_utc: i64, end_utc: i64) -> serde_json::Value {
        let conn = lock_or_recover(&self.conn);

        let sleep_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sleep_samples WHERE start_utc >= ?1 AND start_utc <= ?2",
                params![start_utc, end_utc],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let workout_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM workouts WHERE start_utc >= ?1 AND start_utc <= ?2",
                params![start_utc, end_utc],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let hr_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM heart_rate_samples WHERE timestamp >= ?1 AND timestamp <= ?2",
                params![start_utc, end_utc],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let total_steps: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(count), 0) FROM steps_samples WHERE start_utc >= ?1 AND start_utc <= ?2",
                params![start_utc, end_utc],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let mindful_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM mindfulness_samples WHERE start_utc >= ?1 AND start_utc <= ?2",
                params![start_utc, end_utc],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let metric_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM health_metrics WHERE timestamp >= ?1 AND timestamp <= ?2",
                params![start_utc, end_utc],
                |r| r.get(0),
            )
            .unwrap_or(0);

        #[cfg(not(feature = "gps"))]
        {
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
        #[cfg(feature = "gps")]
        {
            let location_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM location_samples WHERE timestamp >= ?1 AND timestamp <= ?2",
                    params![start_utc, end_utc],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            serde_json::json!({
                "start_utc":            start_utc,
                "end_utc":              end_utc,
                "sleep_samples":        sleep_count,
                "workouts":             workout_count,
                "heart_rate_samples":   hr_count,
                "total_steps":          total_steps,
                "mindfulness_sessions": mindful_count,
                "metric_entries":       metric_count,
                "location_fixes":       location_count,
            })
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
        #[cfg(feature = "gps")]
        assert_eq!(result.location_upserted, 0);
    }

    #[test]
    fn sync_sleep_and_query() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            sleep: vec![SleepSample {
                source_id: "watch".into(),
                start_utc: 1000,
                end_utc: 2000,
                value: "REM".into(),
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
                end_utc: 2000,
                value: "Deep".into(),
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
                end_utc: 2000,
                count: 9500,
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
                HealthMetric {
                    source_id: "".into(),
                    metric_type: "hrv".into(),
                    timestamp: 1,
                    value: 40.0,
                    unit: "ms".into(),
                    metadata: None,
                },
                HealthMetric {
                    source_id: "".into(),
                    metric_type: "restingHeartRate".into(),
                    timestamp: 1,
                    value: 60.0,
                    unit: "bpm".into(),
                    metadata: None,
                },
                HealthMetric {
                    source_id: "".into(),
                    metric_type: "hrv".into(),
                    timestamp: 2,
                    value: 42.0,
                    unit: "ms".into(),
                    metadata: None,
                },
            ],
            ..Default::default()
        };
        store.sync(&payload);
        let types = store.list_metric_types();
        assert_eq!(types, vec!["hrv", "restingHeartRate"]);
    }

    #[cfg(feature = "gps")]
    #[test]
    fn sync_location_and_query() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            location: vec![LocationSample {
                source_id: "iphone".into(),
                timestamp: 7000,
                latitude: 37.3317,
                longitude: -122.0307,
                altitude: Some(25.0),
                horizontal_accuracy: Some(5.0),
                vertical_accuracy: Some(10.0),
                speed: Some(1.4),
                course: Some(270.0),
            }],
            ..Default::default()
        };
        let result = store.sync(&payload);
        assert_eq!(result.location_upserted, 1);

        let rows = store.query_location(0, 10000, 10);
        assert_eq!(rows.len(), 1);
        assert!((rows[0].latitude - 37.3317).abs() < 1e-6);
        assert!((rows[0].longitude - -122.0307).abs() < 1e-6);
        assert_eq!(rows[0].source_id, "iphone");
    }

    #[cfg(feature = "gps")]
    #[test]
    fn sync_location_is_idempotent() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            location: vec![LocationSample {
                source_id: "iphone".into(),
                timestamp: 7000,
                latitude: 48.8566,
                longitude: 2.3522,
                altitude: None,
                horizontal_accuracy: Some(8.0),
                vertical_accuracy: None,
                speed: None,
                course: None,
            }],
            ..Default::default()
        };
        store.sync(&payload);
        store.sync(&payload);
        let rows = store.query_location(0, 10000, 100);
        assert_eq!(rows.len(), 1);
    }

    #[cfg(feature = "gps")]
    #[test]
    fn sync_location_optional_fields_nullable() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            location: vec![LocationSample {
                source_id: "".into(),
                timestamp: 9000,
                latitude: 51.5074,
                longitude: -0.1278,
                altitude: None,
                horizontal_accuracy: None,
                vertical_accuracy: None,
                speed: None,
                course: None,
            }],
            ..Default::default()
        };
        store.sync(&payload);
        let rows = store.query_location(0, 10000, 10);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].altitude.is_none());
        assert!(rows[0].speed.is_none());
    }

    #[cfg(feature = "gps")]
    #[test]
    fn summary_includes_location_count() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            location: vec![
                LocationSample {
                    source_id: "".into(),
                    timestamp: 100,
                    latitude: 0.0,
                    longitude: 0.0,
                    altitude: None,
                    horizontal_accuracy: None,
                    vertical_accuracy: None,
                    speed: None,
                    course: None,
                },
                LocationSample {
                    source_id: "".into(),
                    timestamp: 200,
                    latitude: 1.0,
                    longitude: 1.0,
                    altitude: None,
                    horizontal_accuracy: None,
                    vertical_accuracy: None,
                    speed: None,
                    course: None,
                },
            ],
            ..Default::default()
        };
        store.sync(&payload);
        let s = store.summary(0, 500);
        assert_eq!(s["location_fixes"], 2);
    }

    #[test]
    fn summary_aggregates_correctly() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            sleep: vec![
                SleepSample {
                    source_id: "".into(),
                    start_utc: 100,
                    end_utc: 200,
                    value: "REM".into(),
                },
                SleepSample {
                    source_id: "".into(),
                    start_utc: 300,
                    end_utc: 400,
                    value: "Deep".into(),
                },
            ],
            steps: vec![
                StepsSample {
                    source_id: "".into(),
                    start_utc: 100,
                    end_utc: 200,
                    count: 5000,
                },
                StepsSample {
                    source_id: "".into(),
                    start_utc: 300,
                    end_utc: 400,
                    count: 4500,
                },
            ],
            ..Default::default()
        };
        store.sync(&payload);
        let s = store.summary(0, 500);
        assert_eq!(s["sleep_samples"], 2);
        assert_eq!(s["total_steps"], 9500);
        // When gps is enabled, summary must include location_fixes (0 here).
        #[cfg(feature = "gps")]
        assert_eq!(s["location_fixes"], 0);
    }

    // ── GPS tests ───────────────────────────────────────────────────────────────

    /// Helper: build a minimal valid `LocationSample`.
    #[cfg(feature = "gps")]
    fn loc(timestamp: i64, lat: f64, lon: f64) -> LocationSample {
        LocationSample {
            source_id: String::new(),
            timestamp,
            latitude: lat,
            longitude: lon,
            altitude: None,
            horizontal_accuracy: None,
            vertical_accuracy: None,
            speed: None,
            course: None,
        }
    }

    #[cfg(feature = "gps")]
    #[test]
    fn sync_location_time_range_filtering() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            location: vec![loc(100, 0.0, 0.0), loc(500, 1.0, 1.0), loc(1000, 2.0, 2.0)],
            ..Default::default()
        };
        store.sync(&payload);
        // Only the fix at t=500 falls within [200, 800].
        let rows = store.query_location(200, 800, 100);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].timestamp, 500);
    }

    #[cfg(feature = "gps")]
    #[test]
    fn sync_location_limit_respected() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            location: (1..=10).map(|i| loc(i, i as f64 * 0.1, i as f64 * 0.1)).collect(),
            ..Default::default()
        };
        store.sync(&payload);
        let rows = store.query_location(0, 100, 3);
        assert_eq!(rows.len(), 3);
    }

    #[cfg(feature = "gps")]
    #[test]
    fn query_location_ordered_newest_first() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            location: vec![loc(100, 0.0, 0.0), loc(300, 0.1, 0.1), loc(200, 0.2, 0.2)],
            ..Default::default()
        };
        store.sync(&payload);
        let rows = store.query_location(0, 400, 10);
        assert_eq!(rows.len(), 3);
        assert!(rows[0].timestamp >= rows[1].timestamp);
        assert!(rows[1].timestamp >= rows[2].timestamp);
    }

    #[cfg(feature = "gps")]
    #[test]
    fn query_location_empty_range_returns_empty() {
        let (_dir, store) = temp_store();
        store.sync(&HealthSyncPayload {
            location: vec![loc(5000, 0.0, 0.0)],
            ..Default::default()
        });
        assert!(store.query_location(0, 1000, 10).is_empty());
    }

    #[cfg(feature = "gps")]
    #[test]
    fn sync_location_multiple_source_ids() {
        let (_dir, store) = temp_store();
        // Same timestamp, different source_ids — both must be stored.
        let payload = HealthSyncPayload {
            location: vec![
                LocationSample {
                    source_id: "iphone".into(),
                    ..loc(1000, 10.0, 20.0)
                },
                LocationSample {
                    source_id: "ipad".into(),
                    ..loc(1000, 11.0, 21.0)
                },
            ],
            ..Default::default()
        };
        let result = store.sync(&payload);
        assert_eq!(result.location_upserted, 2);
        assert_eq!(store.query_location(0, 2000, 10).len(), 2);
    }

    // ── LocationSample::is_valid ─────────────────────────────────────────────

    #[cfg(feature = "gps")]
    #[test]
    fn is_valid_accepts_boundary_coordinates() {
        assert!(loc(1, 0.0, 0.0).is_valid());
        assert!(loc(1, 90.0, 180.0).is_valid());
        assert!(loc(1, -90.0, -180.0).is_valid());
    }

    #[cfg(feature = "gps")]
    #[test]
    fn is_valid_rejects_out_of_range_coordinates() {
        assert!(!LocationSample {
            latitude: 90.001,
            ..loc(1, 0.0, 0.0)
        }
        .is_valid());
        assert!(!LocationSample {
            latitude: -90.001,
            ..loc(1, 0.0, 0.0)
        }
        .is_valid());
        assert!(!LocationSample {
            longitude: 180.001,
            ..loc(1, 0.0, 0.0)
        }
        .is_valid());
        assert!(!LocationSample {
            longitude: -180.001,
            ..loc(1, 0.0, 0.0)
        }
        .is_valid());
    }

    #[cfg(feature = "gps")]
    #[test]
    fn is_valid_rejects_nan_and_inf_coordinates() {
        assert!(!LocationSample {
            latitude: f64::NAN,
            ..loc(1, 0.0, 0.0)
        }
        .is_valid());
        assert!(!LocationSample {
            longitude: f64::NAN,
            ..loc(1, 0.0, 0.0)
        }
        .is_valid());
        assert!(!LocationSample {
            latitude: f64::INFINITY,
            ..loc(1, 0.0, 0.0)
        }
        .is_valid());
        assert!(!LocationSample {
            longitude: f64::NEG_INFINITY,
            ..loc(1, 0.0, 0.0)
        }
        .is_valid());
    }

    #[cfg(feature = "gps")]
    #[test]
    fn is_valid_rejects_nonpositive_timestamp() {
        assert!(!loc(0, 0.0, 0.0).is_valid());
        assert!(!loc(-1, 0.0, 0.0).is_valid());
        assert!(loc(1, 0.0, 0.0).is_valid());
    }

    #[cfg(feature = "gps")]
    #[test]
    fn is_valid_rejects_non_finite_optional_fields() {
        let base = loc(1, 10.0, 20.0);
        assert!(!LocationSample {
            altitude: Some(f64::INFINITY),
            ..base.clone()
        }
        .is_valid());
        assert!(!LocationSample {
            horizontal_accuracy: Some(f64::NAN),
            ..base.clone()
        }
        .is_valid());
        assert!(!LocationSample {
            vertical_accuracy: Some(f64::NEG_INFINITY),
            ..base.clone()
        }
        .is_valid());
        assert!(!LocationSample {
            speed: Some(f64::NAN),
            ..base.clone()
        }
        .is_valid());
        assert!(!LocationSample {
            course: Some(f64::INFINITY),
            ..base.clone()
        }
        .is_valid());
    }

    #[cfg(feature = "gps")]
    #[test]
    fn is_valid_accepts_negative_sentinel_values() {
        // CoreLocation reports -1 for unavailable accuracy / speed / course.
        let fix = LocationSample {
            horizontal_accuracy: Some(-1.0),
            vertical_accuracy: Some(-1.0),
            speed: Some(-1.0),
            course: Some(-1.0),
            altitude: Some(-50.0), // below sea level — valid
            ..loc(1, 10.0, 20.0)
        };
        assert!(fix.is_valid());
    }

    #[cfg(feature = "gps")]
    #[test]
    fn sync_location_skips_invalid_counts_valid() {
        let (_dir, store) = temp_store();
        let payload = HealthSyncPayload {
            location: vec![
                loc(1000, 10.0, 20.0), // valid
                LocationSample {
                    latitude: f64::NAN,
                    ..loc(1001, 0.0, 0.0)
                }, // ✕ NaN lat
                LocationSample {
                    latitude: 91.0,
                    ..loc(1002, 0.0, 0.0)
                }, // ✕ lat > 90
                LocationSample {
                    longitude: -181.0,
                    ..loc(1003, 0.0, 0.0)
                }, // ✕ lon < -180
                loc(0, 0.0, 0.0),      // ✕ zero ts
                loc(-5, 0.0, 0.0),     // ✕ negative ts
                loc(1004, -45.0, 170.0), // valid
            ],
            ..Default::default()
        };
        let result = store.sync(&payload);
        assert_eq!(result.location_upserted, 2);
        assert_eq!(store.query_location(0, 2000, 10).len(), 2);
    }
}
