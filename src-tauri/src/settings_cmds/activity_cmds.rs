// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Active-window and input-activity tracking Tauri commands.

use std::sync::Mutex;
use crate::MutexExt;
use tauri::AppHandle;

use crate::AppState;
use crate::active_window::ActiveWindowInfo;
use skill_data::activity_store::{ActiveWindowRow, InputActivityRow, InputBucketRow};

// ── Activity tracking ──────────────────────────────────────────────────────────

/// Return whether active-window tracking is currently enabled.
#[tauri::command]
pub fn get_active_window_tracking(state: tauri::State<'_, Mutex<Box<AppState>>>) -> bool {
    state.lock_or_recover().track_active_window
}

/// Enable or disable active-window tracking and persist the change.
#[tauri::command]
pub fn set_active_window_tracking(
    enabled: bool,
    app:     AppHandle,
    state:   tauri::State<'_, Mutex<Box<AppState>>>,
) {
    state.lock_or_recover().track_active_window = enabled;
    crate::save_settings(&app);
}

/// Return the most recently detected active window, or `None` when tracking
/// is disabled or no window has been observed yet.
#[tauri::command]
pub fn get_active_window(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Option<ActiveWindowInfo> {
    state.lock_or_recover().current_active_window.clone()
}

/// Return whether keyboard/mouse input tracking is currently enabled.
#[tauri::command]
pub fn get_input_activity_tracking(state: tauri::State<'_, Mutex<Box<AppState>>>) -> bool {
    state.lock_or_recover().track_input_activity
}

/// Enable or disable keyboard/mouse input tracking and persist the change.
/// Also flips the `AtomicBool` read by the input-monitor loop so the change
/// takes effect immediately without a restart.
#[tauri::command]
pub fn set_input_activity_tracking(
    enabled: bool,
    app:     AppHandle,
    state:   tauri::State<'_, Mutex<Box<AppState>>>,
) {
    use std::sync::atomic::Ordering;
    let s = state.lock_or_recover();
    s.input_activity_enabled.store(enabled, Ordering::Relaxed);
    drop(s);
    state.lock_or_recover().track_input_activity = enabled;
    crate::save_settings(&app);
}

/// Return `(last_keyboard_unix_secs, last_mouse_unix_secs)`.
/// A value of `0` means the device type has not been seen since the app started.
#[tauri::command]
pub fn get_last_input_activity(state: tauri::State<'_, Mutex<Box<AppState>>>) -> (u64, u64) {
    use std::sync::atomic::Ordering;
    let s = state.lock_or_recover();
    (
        s.last_keyboard_ts.load(Ordering::Relaxed),
        s.last_mouse_ts.load(Ordering::Relaxed),
    )
}

/// Return up to `limit` recent active-window records from `activity.sqlite`,
/// newest first.
#[tauri::command]
pub fn get_recent_active_windows(
    limit: Option<u32>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<ActiveWindowRow> {
    let n = limit.unwrap_or(50).min(500);
    state
        .lock_or_recover()
        .activity_store
        .as_ref()
        .map(|s| s.get_recent_windows(n))
        .unwrap_or_default()
}

/// Return up to `limit` recent input-activity samples from `activity.sqlite`,
/// newest first.
#[tauri::command]
pub fn get_recent_input_activity(
    limit: Option<u32>,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<InputActivityRow> {
    let n = limit.unwrap_or(50).min(500);
    state
        .lock_or_recover()
        .activity_store
        .as_ref()
        .map(|s| s.get_recent_input(n))
        .unwrap_or_default()
}

/// Return per-minute input-event buckets for `[from_ts, to_ts]` (Unix seconds).
///
/// Both timestamps must be rounded to 60-second boundaries by the caller,
/// though SQLite will accept any value.  Results are ordered oldest-first
/// (ascending `minute_ts`) which is what charting libraries expect.
///
/// A convenience default: if `from_ts` is `None` the query covers the last
/// 24 hours.  If `to_ts` is `None` it defaults to now.
#[tauri::command]
pub fn get_input_buckets(
    from_ts: Option<u64>,
    to_ts:   Option<u64>,
    state:   tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<InputBucketRow> {
    let now   = crate::unix_secs();
    let end   = to_ts.unwrap_or(now);
    let start = from_ts.unwrap_or_else(|| end.saturating_sub(24 * 3600));
    state
        .lock_or_recover()
        .activity_store
        .as_ref()
        .map(|s| s.get_input_buckets(start, end))
        .unwrap_or_default()
}

