// SPDX-License-Identifier: GPL-3.0-only
//! `GET /v1/activity` — describe what the daemon is doing in the background.
//!
//! Users sometimes notice the daemon using CPU and have no way to see what's
//! running or why. This endpoint returns a manifest of every recurring
//! background task, what it does, why it's necessary, how often it
//! wakes up, plus live heartbeat data (`last_tick_unix_ms`,
//! `last_duration_ms`, `tick_count`) read from the central registry on
//! `AppState`. The Tauri UI surfaces this in an "Activity" panel so users
//! can understand (and challenge) the daemon's CPU footprint.
//!
//! The static metadata (name/does/why/cost/interval) lives in `MANIFEST`
//! below. Background workers call `state.record_task_heartbeat(id, ms)`
//! once per tick; that both updates the registry and broadcasts an
//! `activity-state` WebSocket event so the UI updates without polling.

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct BackgroundTask {
    pub id: &'static str,
    pub name: &'static str,
    /// What it does, in one sentence.
    pub does: &'static str,
    /// Why this work has to happen — so users can decide whether they care.
    pub why: &'static str,
    /// Wake-up cadence in seconds. `0` means event-driven.
    pub interval_secs: u64,
    /// Approximate CPU cost per tick: "low" (<5ms), "medium" (<50ms), "high" (>50ms).
    pub cost: &'static str,
    /// Whether the user can disable this from settings.
    pub user_toggleable: bool,
    /// Live heartbeat from the central registry (zeroed if the loop has
    /// not ticked yet).
    pub heartbeat: Heartbeat,
    /// Live state, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<TaskState>,
}

#[derive(Debug, Default, Serialize)]
pub struct Heartbeat {
    pub last_tick_unix_ms: u64,
    pub last_duration_ms: u64,
    pub tick_count: u64,
}

#[derive(Debug, Serialize)]
pub struct TaskState {
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ActivityResponse {
    pub tasks: Vec<BackgroundTask>,
}

/// Static metadata for every background task. Adding a new entry here is
/// what makes a new worker visible in the panel — pair this with a
/// `state.record_task_heartbeat(id, ms)` call inside the worker's loop.
struct StaticEntry {
    id: &'static str,
    name: &'static str,
    does: &'static str,
    why: &'static str,
    interval_secs: u64,
    cost: &'static str,
    user_toggleable: bool,
}

const MANIFEST: &[StaticEntry] = &[
    StaticEntry {
        id: "device-scanner",
        name: "Device scanner",
        does: "Probes USB serial ports, BLE adapters, Cortex, NeuroField, BrainBit, g.tec, ANT Neuro, BrainMaster.",
        why: "So when you plug in or power on a device, it shows up automatically without you opening a menu.",
        interval_secs: 5,
        cost: "medium",
        user_toggleable: true,
    },
    StaticEntry {
        id: "status-monitor",
        name: "Device status monitor",
        does: "Reads device battery and signal quality; warns at low battery / poor signal.",
        why: "Required to keep the connection indicator live and to flash a toast before a recording dies.",
        interval_secs: 3,
        cost: "low",
        user_toggleable: false,
    },
    StaticEntry {
        id: "idle-reembed",
        name: "Idle re-embedding",
        does: "When the device has been idle for 30 min, embeds older epochs in the background.",
        why: "Keeps embedding search current after a model upgrade. Pauses immediately when a device reconnects.",
        interval_secs: 10,
        cost: "high",
        user_toggleable: true,
    },
    StaticEntry {
        id: "active-window-poll",
        name: "Active window tracker",
        does: "Records which app/window is in focus and detects file/build/meeting changes.",
        why: "Powers the activity timeline and focus-session reports. Off by default.",
        interval_secs: 3,
        cost: "low",
        user_toggleable: true,
    },
    StaticEntry {
        id: "input-monitor",
        name: "Input activity monitor",
        does: "Detects keyboard / mouse activity to mark you as 'active'.",
        why: "Distinguishes idle time from real work for focus reports.",
        interval_secs: 0,
        cost: "low",
        user_toggleable: true,
    },
    StaticEntry {
        id: "clipboard-monitor",
        name: "Clipboard monitor (macOS)",
        does: "Watches NSPasteboard.changeCount and records copy events. Captures clipboard images when enabled.",
        why: "Lets you find 'that thing I copied an hour ago'. The native change-count check is ~free when you aren't copying.",
        interval_secs: 2,
        cost: "low",
        user_toggleable: true,
    },
    StaticEntry {
        id: "tty-embedder",
        name: "Terminal output embedder",
        does: "Embeds finalized terminal session text so it can be searched.",
        why: "Powers terminal search. Runs in batches of 32 every 30s.",
        interval_secs: 30,
        cost: "medium",
        user_toggleable: true,
    },
    StaticEntry {
        id: "reconnect",
        name: "Reconnect state machine",
        does: "Counts down a retry timer when a device disconnects unexpectedly.",
        why: "Required to auto-reconnect after a brief BLE/USB hiccup.",
        interval_secs: 1,
        cost: "low",
        user_toggleable: false,
    },
    StaticEntry {
        id: "skills-sync",
        name: "Skills sync",
        does: "Pulls remote skill manifest updates.",
        why: "Keeps the skill catalog current.",
        interval_secs: 0,
        cost: "low",
        user_toggleable: true,
    },
];

async fn get_activity(State(state): State<AppState>) -> Json<ActivityResponse> {
    let scanner_running = state.scanner_running.lock().map(|g| *g).unwrap_or(false);
    let (idle_active, idle_detail) = match state.idle_reembed_state.lock() {
        Ok(s) => {
            let detail = if s.active {
                Some(format!("processing {}/{} epochs", s.done, s.total))
            } else if s.delay_secs > 0 {
                Some(format!(
                    "waiting — idle for {}s of {}s before starting",
                    s.idle_secs, s.delay_secs
                ))
            } else {
                None
            };
            (s.active, detail)
        }
        Err(_) => (false, None),
    };

    let heartbeats = state.task_heartbeats.lock().ok().map(|m| m.clone()).unwrap_or_default();

    let mut tasks = Vec::with_capacity(MANIFEST.len());
    for entry in MANIFEST {
        let hb = heartbeats.get(entry.id).cloned().unwrap_or_default();
        let live_state = match entry.id {
            "device-scanner" => Some(TaskState {
                running: scanner_running,
                detail: Some("backs off to every 30s after 5 minutes of no devices and none paired".into()),
            }),
            "idle-reembed" => Some(TaskState {
                running: idle_active,
                detail: idle_detail.clone(),
            }),
            "active-window-poll" => Some(TaskState {
                running: state.track_active_window.load(std::sync::atomic::Ordering::Relaxed),
                detail: None,
            }),
            "input-monitor" => Some(TaskState {
                running: state.track_input_activity.load(std::sync::atomic::Ordering::Relaxed),
                detail: Some("event-driven via the OS input stack".into()),
            }),
            "skills-sync" => Some(TaskState {
                running: false,
                detail: Some("interval set in settings (skills_refresh_interval_secs)".into()),
            }),
            _ => None,
        };
        tasks.push(BackgroundTask {
            id: entry.id,
            name: entry.name,
            does: entry.does,
            why: entry.why,
            interval_secs: entry.interval_secs,
            cost: entry.cost,
            user_toggleable: entry.user_toggleable,
            heartbeat: Heartbeat {
                last_tick_unix_ms: hb.last_tick_unix_ms,
                last_duration_ms: hb.last_duration_ms,
                tick_count: hb.tick_count,
            },
            state: live_state,
        });
    }

    Json(ActivityResponse { tasks })
}

pub fn router() -> Router<AppState> {
    Router::new().route("/activity", get(get_activity))
}
