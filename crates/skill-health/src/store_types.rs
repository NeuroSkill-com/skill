// SPDX-License-Identifier: GPL-3.0-only
// Data types and rows for HealthStore

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SleepSample {
    #[serde(default)]
    pub source_id: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Workout {
    #[serde(default)]
    pub source_id: String,
    pub workout_type: String,
    pub start_utc: i64,
    pub end_utc: i64,
    #[serde(default)]
    pub duration_secs: f64,
    pub total_calories: Option<f64>,
    pub active_calories: Option<f64>,
    pub distance_meters: Option<f64>,
    pub avg_heart_rate: Option<f64>,
    pub max_heart_rate: Option<f64>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeartRateSample {
    #[serde(default)]
    pub source_id: String,
    pub timestamp: i64,
    pub bpm: f64,
    pub context: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StepsSample {
    #[serde(default)]
    pub source_id: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MindfulnessSample {
    #[serde(default)]
    pub source_id: String,
    pub start_utc: i64,
    pub end_utc: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthMetric {
    #[serde(default)]
    pub source_id: String,
    pub metric_type: String,
    pub timestamp: i64,
    pub value: f64,
    #[serde(default)]
    pub unit: String,
    pub metadata: Option<serde_json::Value>,
}

#[cfg(feature = "gps")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocationSample {
    #[serde(default)]
    pub source_id: String,
    pub timestamp: i64,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
    pub horizontal_accuracy: Option<f64>,
    pub vertical_accuracy: Option<f64>,
    pub speed: Option<f64>,
    pub course: Option<f64>,
}

#[cfg(feature = "gps")]
#[derive(Clone, Debug, Serialize)]
pub struct LocationRow {
    pub id: i64,
    pub source_id: String,
    pub timestamp: i64,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
    pub horizontal_accuracy: Option<f64>,
    pub vertical_accuracy: Option<f64>,
    pub speed: Option<f64>,
    pub course: Option<f64>,
    pub created_at: i64,
}

#[cfg(feature = "gps")]
impl LocationSample {
    pub fn is_valid(&self) -> bool {
        self.latitude.is_finite()
            && self.longitude.is_finite()
            && (-90.0..=90.0).contains(&self.latitude)
            && (-180.0..=180.0).contains(&self.longitude)
            && self.timestamp > 0
            && self.altitude.is_none_or(f64::is_finite)
            && self.horizontal_accuracy.is_none_or(f64::is_finite)
            && self.vertical_accuracy.is_none_or(f64::is_finite)
            && self.speed.is_none_or(f64::is_finite)
            && self.course.is_none_or(f64::is_finite)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HealthSyncPayload {
    #[serde(default)]
    pub sleep: Vec<SleepSample>,
    #[serde(default)]
    pub workouts: Vec<Workout>,
    #[serde(default)]
    pub heart_rate: Vec<HeartRateSample>,
    #[serde(default)]
    pub steps: Vec<StepsSample>,
    #[serde(default)]
    pub mindfulness: Vec<MindfulnessSample>,
    #[serde(default)]
    pub metrics: Vec<HealthMetric>,
    #[cfg(feature = "gps")]
    #[serde(default)]
    pub location: Vec<LocationSample>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SyncResult {
    pub sleep_upserted: usize,
    pub workouts_upserted: usize,
    pub heart_rate_upserted: usize,
    pub steps_upserted: usize,
    pub mindfulness_upserted: usize,
    pub metrics_upserted: usize,
    #[cfg(feature = "gps")]
    pub location_upserted: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct SleepRow {
    pub id: i64,
    pub source_id: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub value: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkoutRow {
    pub id: i64,
    pub source_id: String,
    pub workout_type: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub duration_secs: f64,
    pub total_calories: Option<f64>,
    pub active_calories: Option<f64>,
    pub distance_meters: Option<f64>,
    pub avg_heart_rate: Option<f64>,
    pub max_heart_rate: Option<f64>,
    pub metadata: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct HeartRateRow {
    pub id: i64,
    pub source_id: String,
    pub timestamp: i64,
    pub bpm: f64,
    pub context: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct StepsRow {
    pub id: i64,
    pub source_id: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub count: i64,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct HealthMetricRow {
    pub id: i64,
    pub source_id: String,
    pub metric_type: String,
    pub timestamp: i64,
    pub value: f64,
    pub unit: String,
    pub metadata: Option<String>,
    pub created_at: i64,
}
