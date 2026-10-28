// SPDX-License-Identifier: GPL-3.0-only
//! Status monitor — device health (battery, electrodes, data flow) warnings.
//!
//! Previously lived in Tauri's `background.rs` (`spawn_daemon_status_poll`).
//! Now runs daemon-side: every second it assesses the connected device
//! (`skill_daemon_common::device_health`), keeps the verdict in
//! `AppState::device_health` for chat, broadcasts a `DeviceHealth` event when
//! the set of problems changes, and raises a toast with concrete advice for
//! each new problem. Nothing here stops a recording.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Serialize;
use skill_daemon_common::device_health::{DeviceHealth, HealthIssue, HealthTracker, Severity};
use tracing::info;

use crate::state::AppState;

/// Don't toast the same problem again within this window (a flickering
/// electrode would otherwise toast every few seconds).
const TOAST_REPEAT_AFTER: Duration = Duration::from_secs(10 * 60);

#[derive(Serialize)]
struct ToastEvent {
    level: &'static str,
    title: String,
    message: String,
}

/// Decides which issues deserve a toast right now.
#[derive(Default)]
pub(crate) struct ToastGate {
    last: HashMap<String, Instant>,
}

impl ToastGate {
    /// Issues that are new, or last toasted longer than [`TOAST_REPEAT_AFTER`] ago.
    pub(crate) fn due<'a>(&mut self, health: &'a DeviceHealth, now: Instant) -> Vec<&'a HealthIssue> {
        let mut out = Vec::new();
        for issue in &health.issues {
            let key = format!("{}:{}", issue.code, issue.channels.join(","));
            let fresh = self
                .last
                .get(&key)
                .is_none_or(|t| now.saturating_duration_since(*t) >= TOAST_REPEAT_AFTER);
            if fresh {
                self.last.insert(key, now);
                out.push(issue);
            }
        }
        out
    }
}

fn toast_title(code: &str) -> &'static str {
    match code {
        "battery_low" => "Low battery",
        "battery_critical" => "Battery critically low",
        "no_signal" => "No signal on an electrode",
        "poor_contact" => "Poor electrode contact",
        "faulty_electrodes" => "Faulty electrodes",
        "intermittent_contact" => "Electrode keeps cutting out",
        "fair_contact" => "Electrode contact could be better",
        "no_usable_channels" => "No usable EEG signal",
        "reference_suspect" => "Check the reference electrode",
        "data_stalled" => "EEG data stopped",
        "device_error" => "Device error",
        _ => "Device check",
    }
}

fn toast_for(issue: &HealthIssue) -> ToastEvent {
    ToastEvent {
        level: match issue.severity {
            Severity::Critical => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        },
        title: toast_title(&issue.code).into(),
        message: format!("{} {}", issue.message, issue.advice).trim().to_string(),
    }
}

/// The virtual LSL source can simulate a battery; report it as the device's.
/// With no simulated battery it reports 0 (unknown), like any LSL stream —
/// otherwise removing the simulation left the last reading stuck.
fn apply_virtual_battery(state: &AppState) {
    let Ok(src) = state.lsl_virtual_source.lock() else {
        return;
    };
    let Some(src) = src.as_ref() else { return };
    let pct = src.battery_percent().unwrap_or(0.0);
    if let Ok(mut s) = state.status.lock() {
        if s.state == "connected" && s.device_name.as_deref() == Some(skill_lsl::VIRTUAL_STREAM_NAME) {
            s.battery = pct as f32;
        }
    }
}

// ── Background loop ─────────────────────────────────────────────────────────

/// Spawn a background task that assesses device health and broadcasts
/// `DeviceHealth` events and advice toasts.
pub fn spawn_status_monitor(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;

        // SKILL_PERSISTENT_FAULT_SECS shortens the "persistent fault" window
        // for scenario tests (default 5 min).
        let tracker = HealthTracker::new().with_analysis_limit(skill_constants::EEG_CHANNELS);
        let mut tracker = match std::env::var("SKILL_PERSISTENT_FAULT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            Some(secs) => tracker.with_persistent_after(Duration::from_secs(secs)),
            None => tracker,
        };
        let mut toasts = ToastGate::default();

        // Health is assessed every second (a loose electrode drops out for a
        // second or two at a time); the status broadcast stays at every 3 s.
        let mut tick: u64 = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            tick += 1;
            state.record_task_heartbeat("status-monitor", 0);

            apply_virtual_battery(&state);
            let status = match state.status.lock() {
                Ok(s) => s.clone(),
                Err(_) => continue,
            };

            let now = Instant::now();
            let health = tracker.observe(&status, now);
            let changed = match state.device_health.lock() {
                Ok(mut cur) => {
                    let changed = cur.fingerprint() != health.fingerprint() || cur.connected != health.connected;
                    *cur = health.clone();
                    changed
                }
                Err(_) => false,
            };
            if changed {
                state.broadcast("DeviceHealth", &health);
            }
            for issue in toasts.due(&health, now) {
                info!("[monitor] device health: {} — {}", issue.code, issue.message);
                state.broadcast("toast", toast_for(issue));
            }

            if status.state == "connected" && tick.is_multiple_of(3) {
                // Broadcast periodic status so WS clients stay up to date.
                state.broadcast("status", &status);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn health(issues: &[(&str, &[&str])]) -> DeviceHealth {
        DeviceHealth {
            connected: true,
            issues: issues
                .iter()
                .map(|(code, ch)| HealthIssue {
                    code: (*code).into(),
                    severity: Severity::Warning,
                    message: "m".into(),
                    advice: "a".into(),
                    channels: ch.iter().map(|c| c.to_string()).collect(),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn toasts_once_per_problem_then_again_after_the_repeat_window() {
        let mut gate = ToastGate::default();
        let t0 = Instant::now();
        let h = health(&[("no_signal", &["AF7"])]);
        assert_eq!(gate.due(&h, t0).len(), 1);
        assert!(gate.due(&h, t0 + Duration::from_secs(30)).is_empty());
        assert_eq!(gate.due(&h, t0 + TOAST_REPEAT_AFTER).len(), 1);
    }

    #[test]
    fn a_different_electrode_or_problem_toasts_immediately() {
        let mut gate = ToastGate::default();
        let t0 = Instant::now();
        gate.due(&health(&[("no_signal", &["AF7"])]), t0);
        assert_eq!(gate.due(&health(&[("no_signal", &["AF7", "TP9"])]), t0).len(), 1);
        assert_eq!(gate.due(&health(&[("battery_low", &[])]), t0).len(), 1);
    }

    #[test]
    fn toast_carries_message_and_advice() {
        let t = toast_for(&HealthIssue {
            code: "battery_critical".into(),
            severity: Severity::Critical,
            message: "Battery is critically low (8%).".into(),
            advice: "Charge it now.".into(),
            channels: vec![],
        });
        assert_eq!(t.level, "error");
        assert_eq!(t.title, "Battery critically low");
        assert_eq!(t.message, "Battery is critically low (8%). Charge it now.");
    }
}
