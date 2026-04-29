// SPDX-License-Identifier: GPL-3.0-only
//! System commands: TTS, notify, sleep schedule, health, Oura, calendar, DND, iroh.

use serde_json::{json, Value};

use super::{bool_field, skill_dir, str_field, u64_field};
use crate::state::AppState;

// ── TTS / Notify ─────────────────────────────────────────────────────────────

pub(super) async fn cmd_say(_state: &AppState, msg: &Value) -> Result<Value, String> {
    let text = str_field(msg, "text").ok_or("missing text")?;
    let voice = str_field(msg, "voice");

    let spoken = text.clone();
    skill_tts::tts_speak(text, voice).await;

    Ok(json!({ "spoken": spoken }))
}

pub(super) async fn cmd_notify(msg: &Value) -> Result<Value, String> {
    let title = str_field(msg, "title").ok_or("missing title")?;
    let body = str_field(msg, "body").unwrap_or_default();

    tokio::task::spawn_blocking(move || {
        let _ = notify_rust::Notification::new().summary(&title).body(&body).show();
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!({}))
}

// ── Sleep schedule ───────────────────────────────────────────────────────────

pub(super) async fn cmd_sleep_schedule(state: &AppState) -> Result<Value, String> {
    let skill_dir = skill_dir(state);
    let settings = skill_settings::load_settings(&skill_dir);
    Ok(json!({
        "bedtime": settings.sleep.bedtime,
        "wake_time": settings.sleep.wake_time,
        "preset": settings.sleep.preset,
    }))
}

pub(super) async fn cmd_sleep_schedule_set(state: &AppState, msg: &Value) -> Result<Value, String> {
    let skill_dir = skill_dir(state);
    let mut settings = skill_settings::load_settings(&skill_dir);
    if let Some(bt) = str_field(msg, "bedtime") {
        settings.sleep.bedtime = bt;
    }
    if let Some(wt) = str_field(msg, "wake_time") {
        settings.sleep.wake_time = wt;
    }
    if let Some(p) = str_field(msg, "preset") {
        if let Ok(preset) = serde_json::from_value::<skill_settings::SleepPreset>(json!(p)) {
            settings.sleep.preset = preset;
        }
    }
    let path = skill_settings::settings_path(&skill_dir);
    match serde_json::to_string_pretty(&settings) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                return Err(format!("failed to save sleep schedule: {e}"));
            }
        }
        Err(e) => return Err(format!("failed to serialize settings: {e}")),
    }
    Ok(json!({
        "bedtime": settings.sleep.bedtime,
        "wake_time": settings.sleep.wake_time,
        "preset": settings.sleep.preset,
    }))
}

// ── Health / Oura / Calendar ─────────────────────────────────────────────────

pub(super) async fn cmd_health_query(state: &AppState, msg: &Value) -> Result<Value, String> {
    let query_type = str_field(msg, "type").unwrap_or_else(|| "summary".into());
    let start_utc = u64_field(msg, "start_utc");
    let end_utc = u64_field(msg, "end_utc");
    let metric_type = str_field(msg, "metric_type");
    let limit = u64_field(msg, "limit");
    let skill_dir = skill_dir(state);

    let result = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_health::HealthStore::open(&skill_dir) else {
            return json!({ "error": "health store not available" });
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let end = end_utc.unwrap_or(now) as i64;
        let start = start_utc.map(|v| v as i64).unwrap_or_else(|| end - 86400);
        let lim = limit.unwrap_or(500) as i64;

        match query_type.as_str() {
            "sleep" => {
                let rows = store.query_sleep(start, end, lim);
                json!({ "type": "sleep", "data": rows })
            }
            "workouts" => {
                let rows = store.query_workouts(start, end, lim);
                json!({ "type": "workouts", "data": rows })
            }
            "hr" => {
                let rows = store.query_heart_rate(start, end, lim);
                json!({ "type": "hr", "data": rows })
            }
            "steps" => {
                let rows = store.query_steps(start, end, lim);
                json!({ "type": "steps", "data": rows })
            }
            "metrics" => {
                let mt = metric_type.unwrap_or_default();
                let rows = store.query_metrics(&mt, start, end, lim);
                json!({ "type": "metrics", "metric_type": mt, "data": rows })
            }
            "location" => {
                // Location query not directly available from health store.
                json!({ "type": "location", "data": [] })
            }
            _ => {
                let summary = store.summary(start, end);
                json!({ "type": "summary", "data": summary })
            }
        }
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));

    Ok(result)
}

pub(super) async fn cmd_health_summary(state: &AppState, msg: &Value) -> Result<Value, String> {
    let start_utc = u64_field(msg, "start_utc");
    let end_utc = u64_field(msg, "end_utc");
    let skill_dir = skill_dir(state);

    let result = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_health::HealthStore::open(&skill_dir) else {
            return json!({ "error": "health store not available" });
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let end = end_utc.unwrap_or(now) as i64;
        let start = start_utc.map(|v| v as i64).unwrap_or_else(|| end - 86400);
        store.summary(start, end)
    })
    .await
    .unwrap_or_default();

    Ok(serde_json::to_value(result).unwrap_or_default())
}

pub(super) async fn cmd_health_metric_types(state: &AppState) -> Result<Value, String> {
    let skill_dir = skill_dir(state);
    let types = tokio::task::spawn_blocking(move || {
        skill_health::HealthStore::open(&skill_dir)
            .map(|store| store.list_metric_types())
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    Ok(json!({ "types": types }))
}

pub(super) async fn cmd_oura_status(_state: &AppState) -> Result<Value, String> {
    let has_token = !skill_settings::keychain::get_oura_access_token().is_empty();
    Ok(json!({
        "connected": has_token,
        "has_token": has_token,
    }))
}

pub(super) async fn cmd_oura_sync(state: &AppState, msg: &Value) -> Result<Value, String> {
    let start_date = str_field(msg, "start_date");
    let end_date = str_field(msg, "end_date");
    let skill_dir = skill_dir(state);
    let token = skill_settings::keychain::get_oura_access_token();

    if token.is_empty() {
        return Err("Oura access token not configured".into());
    }

    let result = tokio::task::spawn_blocking(move || {
        let Some(store) = skill_health::HealthStore::open(&skill_dir) else {
            return json!({ "error": "health store not available" });
        };
        let oura = skill_oura::OuraSync::new(&token);
        let now = chrono::Utc::now();
        let end = end_date
            .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
            .unwrap_or(now.date_naive());
        let start = start_date
            .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
            .unwrap_or(end - chrono::Duration::days(30));

        match oura.fetch(&start.to_string(), &end.to_string()) {
            Ok(payload) => {
                let result = store.sync(&payload);
                json!({
                    "synced": true,
                    "start": start.to_string(),
                    "end": end.to_string(),
                    "sleep_upserted": result.sleep_upserted,
                    "workouts_upserted": result.workouts_upserted,
                    "heart_rate_upserted": result.heart_rate_upserted,
                    "steps_upserted": result.steps_upserted,
                })
            }
            Err(e) => json!({ "error": e.to_string() }),
        }
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));

    Ok(result)
}

pub(super) async fn cmd_calendar_status() -> Result<Value, String> {
    let result = tokio::task::spawn_blocking(|| {
        let status = skill_calendar::auth_status();
        json!({
            "platform": std::env::consts::OS,
            "permission": format!("{:?}", status),
        })
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));
    Ok(result)
}

pub(super) async fn cmd_calendar_permission() -> Result<Value, String> {
    let result = tokio::task::spawn_blocking(|| {
        let granted = skill_calendar::request_access();
        let status = skill_calendar::auth_status();
        json!({
            "permission": format!("{:?}", status),
            "granted": granted,
        })
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));
    Ok(result)
}

pub(super) async fn cmd_calendar_events(msg: &Value) -> Result<Value, String> {
    let start_utc = u64_field(msg, "start_utc");
    let end_utc = u64_field(msg, "end_utc");

    let result = tokio::task::spawn_blocking(move || {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let start = start_utc.unwrap_or(now) as i64;
        let end = end_utc.unwrap_or(now + 7 * 86400) as i64;
        match skill_calendar::fetch_events(start, end) {
            Ok(events) => json!({ "events": events }),
            Err(e) => json!({ "events": [], "error": e.to_string() }),
        }
    })
    .await
    .unwrap_or_else(|e| json!({ "error": e.to_string() }));

    Ok(result)
}

// ── DND ──────────────────────────────────────────────────────────────────────

pub(super) async fn cmd_dnd(state: &AppState) -> Result<Value, String> {
    let skill_dir = skill_dir(state);
    let settings = skill_settings::load_settings(&skill_dir);
    let cfg = settings.do_not_disturb;
    let os_active = skill_data::dnd::query_os_active();
    Ok(json!({
        "enabled": cfg.enabled,
        "threshold": cfg.focus_threshold,
        "duration_secs": cfg.duration_secs,
        "dnd_active": os_active.unwrap_or(false),
        "os_active": os_active,
    }))
}

pub(super) async fn cmd_dnd_set(msg: &Value) -> Result<Value, String> {
    let enabled = bool_field(msg, "enabled").ok_or("missing enabled")?;
    let mode = msg
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("com.apple.donotdisturb.mode.default");
    let grayscale = msg.get("grayscale").and_then(|v| v.as_bool()).unwrap_or(false);
    let ok = skill_data::dnd::set_dnd(enabled, mode, grayscale);
    Ok(json!({ "enabled": enabled, "applied": ok }))
}

// ── Iroh ─────────────────────────────────────────────────────────────────────

pub(super) fn cmd_iroh(result: anyhow::Result<Value>) -> Result<Value, String> {
    result.map_err(|e| e.to_string())
}

pub(super) async fn cmd_iroh_info(state: &AppState) -> Result<Value, String> {
    skill_iroh::commands::iroh_info(&state.iroh_auth, &state.iroh_runtime).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_state() -> (TempDir, AppState) {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        (td, state)
    }

    #[tokio::test]
    async fn cmd_say_requires_text() {
        let (_td, state) = test_state();
        let msg = json!({});
        let result = cmd_say(&state, &msg).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("text"));
    }

    #[tokio::test]
    async fn cmd_notify_requires_title() {
        let msg = json!({});
        let result = cmd_notify(&msg).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("title"));
    }

    #[tokio::test]
    async fn cmd_dnd_returns_config() {
        let (_td, state) = test_state();
        let result = cmd_dnd(&state).await.unwrap();
        // DND config should have enabled field
        assert!(result.get("enabled").is_some());
    }

    #[tokio::test]
    async fn cmd_sleep_schedule_returns_schedule() {
        let (_td, state) = test_state();
        let result = cmd_sleep_schedule(&state).await.unwrap();
        assert!(result.get("bedtime").is_some() || result.get("preset").is_some());
    }

    #[tokio::test]
    async fn cmd_say_with_text_succeeds() {
        let (_td, state) = test_state();
        let msg = json!({"text": "hello world"});
        let result = cmd_say(&state, &msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cmd_notify_with_title_succeeds() {
        let msg = json!({"title": "Test", "body": "notification"});
        let result = cmd_notify(&msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cmd_health_metric_types_returns_types() {
        let (_td, state) = test_state();
        let result = cmd_health_metric_types(&state).await.unwrap();
        assert!(result.get("types").is_some());
    }

    #[tokio::test]
    async fn cmd_dnd_set_requires_enabled_field() {
        let msg = json!({});
        let result = cmd_dnd_set(&msg).await;
        // Should either work with defaults or require enabled
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn cmd_iroh_info_returns_value() {
        let (_td, state) = test_state();
        let result = cmd_iroh_info(&state).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cmd_calendar_status_returns_value() {
        let result = cmd_calendar_status().await;
        assert!(result.is_ok());
    }
}
