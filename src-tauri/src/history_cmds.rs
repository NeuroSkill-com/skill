// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Thin Tauri IPC wrappers for session history listing, streaming, stats,
// deletion, and embedding-session discovery.  All heavy I/O is delegated
// to the `skill-history` crate and runs on Tokio blocking threads.

use std::sync::Mutex;

use serde::Serialize;
use tauri::AppHandle;

use crate::{AppState, MutexExt};

// Re-export the core type for callers.
pub(crate) use skill_history::SessionEntry;

#[tauri::command]
pub(crate) async fn open_history_window(app: AppHandle) -> Result<(), String> {
    crate::window_cmds::focus_or_create(&app, crate::window_cmds::WindowSpec {
        label: "history", route: "history", title: "NeuroSkill™ – History",
        inner_size: (920.0, 780.0), min_inner_size: Some((700.0, 560.0)),
        ..Default::default()
    })
}

/// Scan all `~/.skill/*/muse_*.json` sidecar files and return session entries
/// sorted by start time descending (newest first).
#[tauri::command]
pub(crate) fn list_sessions(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<SessionEntry> {
    let (skill_dir, logger) = {
        let s = state.lock_or_recover();
        (s.skill_dir.clone(), s.logger.clone())
    };

    skill_log!(logger, "history", "scanning {:?}", skill_dir);

    // Collect sessions from all day directories.
    let days = skill_history::list_session_days(&skill_dir);
    let label_store = skill_history::label_store::LabelStore::open(&skill_dir);

    let mut sessions = Vec::new();
    for day in &days {
        let mut day_sessions = skill_history::list_sessions_for_day(day, &skill_dir, label_store.as_ref());
        sessions.append(&mut day_sessions);
    }

    sessions.sort_by(|a, b| b.session_start_utc.cmp(&a.session_start_utc));
    skill_log!(logger, "history", "returning {} sessions", sessions.len());
    sessions
}

#[tauri::command]
pub(crate) async fn list_session_days(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Result<Vec<String>, String> {
    let skill_dir = crate::skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        skill_history::list_session_days(&skill_dir)
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn list_sessions_for_day(
    day: String,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<SessionEntry>, String> {
    let skill_dir = crate::skill_dir(&state);
    tokio::task::spawn_blocking(move || {
        let label_store = skill_history::label_store::LabelStore::open(&skill_dir);
        skill_history::list_sessions_for_day(&day, &skill_dir, label_store.as_ref())
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn delete_session(csv_path: String) -> Result<(), String> {
    skill_history::delete_session(&csv_path)
}

// ── Streaming session list ────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub(crate) struct SessionStreamEvent {
    kind:           String,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_days:     Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    day:            Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sessions:       Option<Vec<SessionEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_sessions: Option<usize>,
}

#[tauri::command]
pub(crate) async fn stream_sessions(
    on_event: tauri::ipc::Channel<SessionStreamEvent>,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    let skill_dir = crate::skill_dir(&state);

    tokio::task::spawn_blocking(move || {
        let days = skill_history::list_session_days(&skill_dir);

        let _ = on_event.send(SessionStreamEvent {
            kind: "started".into(), total_days: Some(days.len()),
            day: None, sessions: None, total_sessions: None,
        });

        let label_store = skill_history::label_store::LabelStore::open(&skill_dir);
        let mut total_sessions = 0usize;

        for day in &days {
            let sessions = skill_history::list_sessions_for_day(day, &skill_dir, label_store.as_ref());
            total_sessions += sessions.len();

            let _ = on_event.send(SessionStreamEvent {
                kind: "day".into(), total_days: None,
                day: Some(day.clone()), sessions: Some(sessions),
                total_sessions: None,
            });
        }

        let _ = on_event.send(SessionStreamEvent {
            kind: "done".into(), total_days: None,
            day: None, sessions: None, total_sessions: Some(total_sessions),
        });
    })
    .await
    .map_err(|e| e.to_string())
}

/// Aggregate history stats.
#[tauri::command]
pub(crate) async fn get_history_stats(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<skill_history::HistoryStats, ()> {
    let skill_dir = crate::skill_dir(&state);
    Ok(tokio::task::spawn_blocking(move || {
        skill_history::get_history_stats(&skill_dir)
    }).await.unwrap_or(skill_history::HistoryStats {
        total_sessions: 0, total_secs: 0, this_week_secs: 0, last_week_secs: 0,
    }))
}

/// List embedding sessions for the compare picker.
#[tauri::command]
pub(crate) fn list_embedding_sessions(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<skill_history::EmbeddingSession> {
    let skill_dir = crate::skill_dir(&state);
    skill_history::list_embedding_sessions(&skill_dir)
}

/// Find a session CSV path for a given timestamp — used by settings_cmds.
pub(crate) fn find_session_csv_for_timestamp(skill_dir: &std::path::Path, ts_utc: u64) -> Option<String> {
    skill_history::find_session_csv_for_timestamp(skill_dir, ts_utc)
}
