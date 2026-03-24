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
        if !enabled {
            dnd.focus_samples.clear();
        }
        drop(dnd);
        drop(g);
        let _ = app.emit("dnd-state-changed", enabled);
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
    Ok(serde_json::json!({
        "bedtime":          cfg.bedtime,
        "wake_time":        cfg.wake_time,
        "preset":           cfg.preset,
        "duration_minutes": dur,
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

    let cfg = guard.sleep_config.clone();
    let dur = cfg.duration_minutes();
    drop(guard);
    crate::save_settings(app);

    Ok(serde_json::json!({
        "ok":               true,
        "bedtime":          cfg.bedtime,
        "wake_time":        cfg.wake_time,
        "preset":           cfg.preset,
        "duration_minutes": dur,
    }))
}
