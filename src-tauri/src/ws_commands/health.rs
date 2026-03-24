// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! WebSocket HealthKit sync/query commands.

use serde_json::Value;
use tauri::AppHandle;

use crate::AppStateExt;
use crate::MutexExt;

/// `health_sync` — upsert Apple HealthKit data from the iOS companion app.
pub fn health_sync(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let payload: skill_data::health_store::HealthSyncPayload = serde_json::from_value(msg.clone())
        .map_err(|e| format!("invalid health_sync payload: {e}"))?;

    let st = app.app_state();
    let store = {
        let s = st.lock_or_recover();
        s.health_store.clone()
    };
    let store = store.ok_or_else(|| "health store not available".to_string())?;
    let result = store.sync(&payload);
    eprintln!(
        "[health] sync: sleep={} workouts={} hr={} steps={} mindful={} metrics={}",
        result.sleep_upserted,
        result.workouts_upserted,
        result.heart_rate_upserted,
        result.steps_upserted,
        result.mindfulness_upserted,
        result.metrics_upserted,
    );
    serde_json::to_value(&result).map_err(|e| e.to_string())
}

/// `health_query` — query stored HealthKit data by type and time range.
pub fn health_query(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let data_type = msg.get("type").and_then(|v| v.as_str()).ok_or_else(|| {
        "missing required field: \"type\" (sleep|workouts|heart_rate|steps|metrics)".to_string()
    })?;
    let start_utc = msg
        .get("start_utc")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let end_utc = msg
        .get("end_utc")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64
        });
    let limit = msg
        .get("limit")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(500)
        .clamp(1, 10_000);

    let st = app.app_state();
    let store = {
        let s = st.lock_or_recover();
        s.health_store.clone()
    };
    let store = store.ok_or_else(|| "health store not available".to_string())?;

    match data_type {
        "sleep" => {
            let rows = store.query_sleep(start_utc, end_utc, limit);
            Ok(serde_json::json!({ "type": "sleep", "count": rows.len(), "results": rows }))
        }
        "workouts" => {
            let rows = store.query_workouts(start_utc, end_utc, limit);
            Ok(serde_json::json!({ "type": "workouts", "count": rows.len(), "results": rows }))
        }
        "heart_rate" => {
            let rows = store.query_heart_rate(start_utc, end_utc, limit);
            Ok(serde_json::json!({ "type": "heart_rate", "count": rows.len(), "results": rows }))
        }
        "steps" => {
            let rows = store.query_steps(start_utc, end_utc, limit);
            Ok(serde_json::json!({ "type": "steps", "count": rows.len(), "results": rows }))
        }
        "metrics" => {
            let metric_type = msg.get("metric_type").and_then(|v| v.as_str())
                .ok_or_else(|| "\"metric_type\" required when type=\"metrics\" (e.g. \"restingHeartRate\")".to_string())?;
            let rows = store.query_metrics(metric_type, start_utc, end_utc, limit);
            Ok(serde_json::json!({ "type": "metrics", "metric_type": metric_type, "count": rows.len(), "results": rows }))
        }
        other => Err(format!("invalid health data type: \"{other}\" — must be sleep|workouts|heart_rate|steps|metrics")),
    }
}

/// `health_summary` — aggregate counts for a time range.
pub fn health_summary(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let start_utc = msg
        .get("start_utc")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let end_utc = msg
        .get("end_utc")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64
        });

    let st = app.app_state();
    let store = {
        let s = st.lock_or_recover();
        s.health_store.clone()
    };
    let store = store.ok_or_else(|| "health store not available".to_string())?;
    Ok(store.summary(start_utc, end_utc))
}

/// `health_metric_types` — list all distinct metric types in the database.
pub fn health_metric_types(app: &AppHandle) -> Result<Value, String> {
    let st = app.app_state();
    let store = {
        let s = st.lock_or_recover();
        s.health_store.clone()
    };
    let store = store.ok_or_else(|| "health store not available".to_string())?;
    let types = store.list_metric_types();
    Ok(serde_json::json!({ "metric_types": types }))
}
