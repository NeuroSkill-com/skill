// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! WebSocket DND and sleep schedule commands.

use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::AppStateExt;
use crate::MutexExt;

// ── dnd ───────────────────────────────────────────────────────────────────────

/// `dnd` — return the current Do Not Disturb automation status.
pub fn dnd_status(app: &AppHandle) -> Result<Value, String> {
    let s = app.app_state();
    let guard = s.lock_or_recover();
    let dnd = guard.dnd.lock_or_recover();
    let enabled = dnd.config.enabled;
    let threshold = dnd.config.focus_threshold;
    let duration_secs = dnd.config.duration_secs;
    let mode_id = dnd.config.focus_mode_identifier.clone();
    let dnd_active = dnd.active;
    let window_size = (duration_secs as usize * 4).max(8);
    let sample_count = dnd.focus_samples.len();
    let avg_score = if sample_count > 0 {
        dnd.focus_samples.iter().sum::<f64>() / sample_count as f64
    } else {
        0.0
    };
    let os_active = dnd.os_active;
    let last_error = dnd.last_error.clone();
    drop(dnd);
    drop(guard);

    Ok(serde_json::json!({
        "enabled":          enabled,
        "avg_score":        avg_score,
        "threshold":        threshold,
        "sample_count":     sample_count,
        "window_size":      window_size,
        "duration_secs":    duration_secs,
        "mode_identifier":  mode_id,
        "dnd_active":       dnd_active,
        "os_active":        os_active,
        "last_error":       last_error,
    }))
}

/// `dnd_set { "enabled": bool }` — force-enable or disable DND immediately.
pub fn dnd_set(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let enabled = msg
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "missing required field: \"enabled\" (boolean)".to_string())?;

    let mode_id = {
        let dnd_arc = app.app_state().lock_or_recover().dnd.clone();
        let r = dnd_arc
            .lock_or_recover()
            .config
            .focus_mode_identifier
            .clone();
        r
    };

    let ok = skill_data::dnd::set_dnd(enabled, &mode_id);
    if ok {
        let s = app.app_state();
        let g = s.lock_or_recover();
        let mut dnd = g.dnd.lock_or_recover();
        dnd.active = enabled;
        dnd.last_error = None;
        if !enabled {
            dnd.focus_samples.clear();
        }
        drop(dnd);
        drop(g);
        let _ = app.emit("dnd-state-changed", enabled);
    } else {
        let msg = if enabled {
            "Couldn’t enable Focus mode. macOS blocked access to Do Not Disturb settings (permission or sandbox restriction)."
        } else {
            "Couldn’t disable Focus mode. macOS blocked access to Do Not Disturb settings (permission or sandbox restriction)."
        };
        let s = app.app_state();
        let g = s.lock_or_recover();
        g.dnd.lock_or_recover().last_error = Some(msg.to_owned());
        drop(g);
        let _ = app.emit("dnd-error", msg);
    }

    Ok(serde_json::json!({ "enabled": enabled, "ok": ok }))
}

// ── sleep schedule ────────────────────────────────────────────────────────────

/// `sleep_schedule` — return the current sleep schedule configuration.
pub fn sleep_schedule(app: &AppHandle) -> Result<Value, String> {
    let s = app.app_state();
    let guard = s.lock_or_recover();
    let cfg = &guard.sleep_config;
    let dur = cfg.duration_minutes();
    let alarm = guard.alarm_config.as_ref();
    Ok(serde_json::json!({
        "bedtime":              cfg.bedtime,
        "wake_time":            cfg.wake_time,
        "preset":               cfg.preset,
        "duration_minutes":     dur,
        "alarm_enabled":        alarm.map(|a| a.enabled).unwrap_or(false),
        "alarm_smart_window":   alarm.map(|a| a.smart_window).unwrap_or(30),
        "alarm_smart_wake":     alarm.map(|a| a.smart_wake_enabled).unwrap_or(true),
    }))
}

/// `sleep_schedule_set` — update the sleep schedule.
pub fn sleep_schedule_set(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    use crate::settings::SleepPreset;

    let s = app.app_state();
    let mut guard = s.lock_or_recover();

    if let Some(v) = msg.get("bedtime").and_then(|v| v.as_str()) {
        guard.sleep_config.bedtime = v.to_string();
    }
    if let Some(v) = msg.get("wake_time").and_then(|v| v.as_str()) {
        guard.sleep_config.wake_time = v.to_string();
    }
    if let Some(v) = msg.get("preset").and_then(|v| v.as_str()) {
        guard.sleep_config.preset = match v {
            "default" => SleepPreset::Default,
            "early_bird" => SleepPreset::EarlyBird,
            "night_owl" => SleepPreset::NightOwl,
            "short_sleeper" => SleepPreset::ShortSleeper,
            "long_sleeper" => SleepPreset::LongSleeper,
            _ => SleepPreset::Custom,
        };
    }

    // Sync wake_time into the alarm config if one exists
    let (bh, bm) = {
        let parts: Vec<&str> = guard.sleep_config.wake_time.split(':').collect();
        (parts.first().and_then(|s| s.parse::<i32>().ok()).unwrap_or(7),
         parts.get(1).and_then(|s| s.parse::<i32>().ok()).unwrap_or(0))
    };
    if let Some(ref mut alarm) = guard.alarm_config {
        alarm.wake_time_minutes = bh * 60 + bm;
        alarm.smart_wake_sent = false; // reset for new schedule
    }

    let cfg = guard.sleep_config.clone();
    let alarm = guard.alarm_config.clone();
    let dur = cfg.duration_minutes();
    drop(guard);
    crate::save_settings(app);

    Ok(serde_json::json!({
        "ok":                   true,
        "bedtime":              cfg.bedtime,
        "wake_time":            cfg.wake_time,
        "preset":               cfg.preset,
        "duration_minutes":     dur,
        "alarm_enabled":        alarm.as_ref().map(|a| a.enabled).unwrap_or(false),
        "alarm_smart_window":   alarm.as_ref().map(|a| a.smart_window).unwrap_or(30),
        "alarm_smart_wake":     alarm.as_ref().map(|a| a.smart_wake_enabled).unwrap_or(true),
    }))
}

// ── Alarm ─────────────────────────────────────────────────────────────────────

/// `alarm_config` — receive alarm configuration from the iOS client.
///
/// The desktop stores it and starts monitoring the real-time sleep stage
/// to trigger a `smart_wake` broadcast when the user enters light sleep
/// within the alarm window.
pub fn alarm_config(app: &AppHandle, msg: &Value) -> Result<Value, String> {
    let enabled = msg.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let wake_time_minutes = msg.get("wake_time_minutes").and_then(|v| v.as_i64()).unwrap_or(420) as i32;
    let smart_window = msg.get("smart_window").and_then(|v| v.as_i64()).unwrap_or(30) as i32;
    let smart_wake_enabled = msg.get("smart_wake_enabled").and_then(|v| v.as_bool()).unwrap_or(true);
    let next_alarm_utc = msg.get("next_alarm_utc").and_then(|v| v.as_f64());

    // If bedtime is provided, update the sleep schedule too
    let bedtime = msg.get("bedtime").and_then(|v| v.as_str()).map(|s| s.to_string());

    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();

        // Store alarm config
        s.alarm_config = Some(AlarmConfig {
            enabled,
            wake_time_minutes,
            smart_window,
            smart_wake_enabled,
            next_alarm_utc,
            smart_wake_sent: false,
        });

        // Sync wake_time into the sleep schedule so both stay consistent
        let wake_h = wake_time_minutes / 60;
        let wake_m = wake_time_minutes % 60;
        s.sleep_config.wake_time = format!("{wake_h:02}:{wake_m:02}");
        if let Some(ref bt) = bedtime {
            s.sleep_config.bedtime = bt.clone();
        }
        s.sleep_config.preset = crate::settings::SleepPreset::Custom;
    }
    crate::save_settings(app);

    // Return the merged sleep + alarm config
    let r = app.app_state();
    let g = r.lock_or_recover();
    let sc = &g.sleep_config;

    eprintln!(
        "[alarm] config received: enabled={enabled} wake={wake_time_minutes}m window={smart_window}m bedtime={} smart={smart_wake_enabled}",
        sc.bedtime
    );

    Ok(serde_json::json!({
        "ok": true,
        "command": "alarm_config",
        "bedtime": sc.bedtime,
        "wake_time": sc.wake_time,
        "duration_minutes": sc.duration_minutes(),
    }))
}

/// Alarm configuration stored in AppState.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AlarmConfig {
    pub enabled: bool,
    pub wake_time_minutes: i32,
    pub smart_window: i32,
    pub smart_wake_enabled: bool,
    pub next_alarm_utc: Option<f64>,
    /// Whether we already sent the smart_wake for this alarm cycle.
    pub smart_wake_sent: bool,
}

/// Called from the band snapshot emitter (every ~5s) to check if we should
/// trigger a smart wake alarm.
///
/// Returns `true` if a `smart_wake` broadcast should be sent.
pub fn check_smart_wake(
    alarm: &mut AlarmConfig,
    rel_delta: f32,
    rel_theta: f32,
    rel_alpha: f32,
    rel_beta: f32,
) -> bool {
    if !alarm.enabled || !alarm.smart_wake_enabled || alarm.smart_wake_sent {
        return false;
    }

    let Some(target_utc) = alarm.next_alarm_utc else {
        return false;
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let window_start = target_utc - (alarm.smart_window as f64 * 60.0);
    let window_end = target_utc;

    // Are we within the smart wake window?
    if now < window_start || now > window_end {
        return false;
    }

    // Classify current sleep stage (same as skill-history)
    // 0=Wake, 1=N1(light), 2=N2, 3=N3(deep), 5=REM
    let stage = if rel_alpha > 0.30 || rel_beta > 0.30 {
        0 // Wake
    } else if rel_theta > 0.30 && rel_alpha < 0.15 && rel_delta < 0.45 {
        5 // REM
    } else if rel_delta > 0.50 {
        3 // N3 deep
    } else if rel_theta > 0.25 && rel_delta < 0.50 {
        1 // N1 light
    } else {
        2 // N2
    };

    // Fire on Wake or N1 (light sleep) — best time to wake up
    if stage == 0 || stage == 1 {
        alarm.smart_wake_sent = true;
        eprintln!("[alarm] smart wake triggered! stage={stage} (within {:.0}s of target)", window_end - now);
        return true;
    }

    false
}
