// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Active-window and input-activity tracking Tauri commands.

use std::sync::Mutex;

use crate::AppState;
use skill_data::activity_store::{ActiveWindowRow, InputActivityRow, InputBucketRow};

// ── Activity tracking ──────────────────────────────────────────────────────────

/// Return up to `limit` recent active-window records from `activity.sqlite`,
/// newest first.
#[tauri::command]
pub fn get_recent_active_windows(
    limit: Option<u32>,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<ActiveWindowRow> {
    crate::daemon_cmds::fetch_recent_active_windows(limit).unwrap_or_default()
}

/// Return up to `limit` recent input-activity samples from `activity.sqlite`,
/// newest first.
#[tauri::command]
pub fn get_recent_input_activity(
    limit: Option<u32>,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<InputActivityRow> {
    crate::daemon_cmds::fetch_recent_input_activity(limit).unwrap_or_default()
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
    to_ts: Option<u64>,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<InputBucketRow> {
    let now = crate::unix_secs();
    let end = to_ts.unwrap_or(now);
    let start = from_ts.unwrap_or_else(|| end.saturating_sub(24 * 3600));
    crate::daemon_cmds::fetch_input_buckets(Some(start), Some(end)).unwrap_or_default()
}
