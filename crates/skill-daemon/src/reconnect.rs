// SPDX-License-Identifier: GPL-3.0-only
//! Reconnect state machine and background loop.
//!
//! Previously lived in the Tauri `lifecycle.rs` / `background.rs` files.
//! Now daemon-authoritative: the daemon owns the reconnect countdown, backoff
//! schedule, and retry trigger.  The Tauri UI subscribes to `"reconnect-state"`
//! events via WebSocket.

use std::sync::{Arc, Mutex};

use tracing::info;

use crate::state::AppState;
pub use skill_daemon_state::reconnect_state::{ReconnectState, MAX_RETRY_ATTEMPTS};

/// Continuous reconnect cadence (seconds) used by the auto-reconnect loop.
const AUTO_RECONNECT_CADENCE_SECS: u32 = 3;

// ── Pure decision logic ─────────────────────────────────────────────────────

/// Reconnect backoff: 1 s → 2 s → 3 s → 5 s, then stays at 5 s.
#[allow(dead_code)]
pub fn retry_delay_secs(attempt: u32) -> u32 {
    match attempt {
        0 => 1,
        1 => 2,
        2 => 3,
        _ => 5,
    }
}

/// Outcome of one reconnect-tick evaluation.
#[derive(Debug, PartialEq)]
pub struct ReconnectAction {
    pub countdown: u32,
    pub attempt: u32,
    pub should_emit: bool,
    pub trigger_retry: bool,
}

/// Pure state-machine step for the auto-reconnect countdown.
pub fn eval_reconnect_tick(pending: bool, state: &str, countdown: u32, attempt: u32) -> ReconnectAction {
    if !pending {
        let dirty = countdown != 0 || attempt != 0;
        return ReconnectAction {
            countdown: 0,
            attempt: 0,
            should_emit: dirty,
            trigger_retry: false,
        };
    }

    if state == "connected" {
        let dirty = countdown != 0 || attempt != 0;
        return ReconnectAction {
            countdown: 0,
            attempt: 0,
            should_emit: dirty,
            trigger_retry: false,
        };
    }

    if state == "bt_off" || state == "connecting" || state == "scanning" {
        return ReconnectAction {
            countdown,
            attempt,
            should_emit: false,
            trigger_retry: false,
        };
    }

    // Disconnected + reconnect enabled: run the countdown.
    let new_countdown = if countdown == 0 {
        AUTO_RECONNECT_CADENCE_SECS
    } else {
        countdown.saturating_sub(1)
    };

    let trigger = new_countdown == 0;
    let new_attempt = if trigger { attempt.saturating_add(1) } else { attempt };

    ReconnectAction {
        countdown: new_countdown,
        attempt: new_attempt,
        should_emit: true,
        trigger_retry: trigger,
    }
}

// ── Background loop ─────────────────────────────────────────────────────────

/// Spawn the reconnect background loop that ticks every second.
pub fn spawn_reconnect_loop(state: AppState, reconnect: Arc<Mutex<ReconnectState>>) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            let (device_state, action) = {
                let rc = reconnect.lock().unwrap_or_else(|e| e.into_inner());
                let device_state = state.status.lock().map(|s| s.state.clone()).unwrap_or_default();
                let action = eval_reconnect_tick(rc.pending, &device_state, rc.countdown, rc.attempt);
                (device_state, action)
            };

            // Give up after MAX_RETRY_ATTEMPTS.
            if action.attempt >= MAX_RETRY_ATTEMPTS && action.trigger_retry {
                info!("[reconnect] giving up after {} attempts", action.attempt);
                let mut rc = reconnect.lock().unwrap_or_else(|e| e.into_inner());
                rc.pending = false;
                rc.attempt = 0;
                rc.countdown = 0;
                drop(rc);

                // Update status to reflect giving up.
                if let Ok(mut s) = state.status.lock() {
                    s.retry_attempt = 0;
                    s.retry_countdown_secs = 0;
                    s.state = "disconnected".to_string();
                    s.device_error = Some("Reconnect failed after multiple attempts".to_string());
                }
                state.broadcast("status", &*state.status.lock().unwrap_or_else(|e| e.into_inner()));
                state.broadcast("reconnect-state", ReconnectState::default());
                continue;
            }

            // Apply the tick result.
            {
                let mut rc = reconnect.lock().unwrap_or_else(|e| e.into_inner());
                rc.countdown = action.countdown;
                rc.attempt = action.attempt;
            }

            // Update status fields so polling clients also see countdown.
            if let Ok(mut s) = state.status.lock() {
                s.retry_countdown_secs = action.countdown;
                s.retry_attempt = action.attempt;
            }

            if action.should_emit {
                let rc = reconnect.lock().unwrap_or_else(|e| e.into_inner()).clone();
                state.broadcast("reconnect-state", &rc);
                state.broadcast("status", &*state.status.lock().unwrap_or_else(|e| e.into_inner()));
            }

            if action.trigger_retry {
                info!(
                    "[reconnect] attempt #{} — triggering retry (state={device_state})",
                    action.attempt
                );
                // Trigger a session start via the existing session runner.
                let preferred = state
                    .status
                    .lock()
                    .ok()
                    .and_then(|s| s.paired_devices.first().map(|d| d.id.clone()));
                if let Some(target_id) = preferred {
                    crate::util::spawn_session_for_target(&state, Some(&target_id));
                }
            }
        }
    });
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_schedule_1_2_3_5() {
        assert_eq!(retry_delay_secs(0), 1);
        assert_eq!(retry_delay_secs(1), 2);
        assert_eq!(retry_delay_secs(2), 3);
        assert_eq!(retry_delay_secs(3), 5);
    }

    #[test]
    fn backoff_capped_at_5s() {
        for attempt in 3u32..=100 {
            assert_eq!(retry_delay_secs(attempt), 5);
        }
    }

    #[test]
    fn reconnect_disabled_clears_counters() {
        let a = eval_reconnect_tick(false, "disconnected", 2, 5);
        assert_eq!(a.countdown, 0);
        assert_eq!(a.attempt, 0);
        assert!(a.should_emit);
        assert!(!a.trigger_retry);
    }

    #[test]
    fn reconnect_disabled_no_emit_when_already_zero() {
        let a = eval_reconnect_tick(false, "disconnected", 0, 0);
        assert!(!a.should_emit);
        assert!(!a.trigger_retry);
    }

    #[test]
    fn reconnect_connected_clears_counters() {
        let a = eval_reconnect_tick(true, "connected", 2, 3);
        assert_eq!(a.countdown, 0);
        assert_eq!(a.attempt, 0);
        assert!(a.should_emit);
        assert!(!a.trigger_retry);
    }

    #[test]
    fn reconnect_bt_off_passthrough() {
        let a = eval_reconnect_tick(true, "bt_off", 2, 1);
        assert_eq!(a.countdown, 2);
        assert_eq!(a.attempt, 1);
        assert!(!a.should_emit);
        assert!(!a.trigger_retry);
    }

    #[test]
    fn reconnect_disconnected_starts_countdown() {
        let a = eval_reconnect_tick(true, "disconnected", 0, 0);
        assert_eq!(a.countdown, AUTO_RECONNECT_CADENCE_SECS);
        assert!(!a.trigger_retry);
    }

    #[test]
    fn reconnect_fires_at_zero() {
        let a = eval_reconnect_tick(true, "disconnected", 1, 0);
        assert_eq!(a.countdown, 0);
        assert_eq!(a.attempt, 1);
        assert!(a.trigger_retry);
    }

    #[test]
    fn max_retry_attempts_is_reasonable() {
        let total: u32 = (0..MAX_RETRY_ATTEMPTS).map(retry_delay_secs).sum();
        assert!(total >= 30);
        assert!(total <= 120);
    }
}
