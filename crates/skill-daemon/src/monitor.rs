// SPDX-License-Identifier: GPL-3.0-only
//! Status monitor — battery and signal quality warnings.
//!
//! Previously lived in Tauri's `background.rs` (`spawn_daemon_status_poll`).
//! Now runs daemon-side, broadcasting toast events to connected clients.

use std::time::{Duration, Instant};

use serde::Serialize;
use tracing::info;

use crate::state::AppState;

// ── Pure decision logic ─────────────────────────────────────────────────────

/// Whether a low-battery toast should fire.
pub fn should_warn_battery(batt: f32, dev_id: &str, warned_for: Option<&str>) -> bool {
    !dev_id.is_empty() && batt > 0.0 && batt <= 15.0 && warned_for != Some(dev_id)
}

/// Whether the battery-warning latch should be cleared (device recharged).
pub fn should_clear_battery_warning(batt: f32, dev_id: &str, warned_for: Option<&str>) -> bool {
    batt >= 25.0 && warned_for == Some(dev_id)
}

/// Count good / bad channels from a quality slice.
pub fn count_signal_quality(quality: &[String]) -> (usize, usize) {
    let good = quality.iter().filter(|x| x.as_str() == "good").count();
    let bad = quality
        .iter()
        .filter(|x| x.as_str() == "poor" || x.as_str() == "no_signal")
        .count();
    (good, bad)
}

#[derive(Serialize)]
struct ToastEvent {
    level: &'static str,
    title: String,
    message: String,
}

// ── Background loop ─────────────────────────────────────────────────────────

/// Spawn a background task that monitors device status and broadcasts
/// battery/signal warnings as `"toast"` events.
pub fn spawn_status_monitor(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;

        let mut batt_warned_for: Option<String> = None;
        let mut had_good_signal = false;
        let mut bad_signal_since: Option<Instant> = None;
        let mut signal_warned_for: Option<String> = None;

        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;

            let status = match state.status.lock() {
                Ok(s) => s.clone(),
                Err(_) => continue,
            };

            if status.state != "connected" {
                had_good_signal = false;
                bad_signal_since = None;
                continue;
            }

            let dev_id = status
                .device_id
                .clone()
                .or(status.target_id.clone())
                .or(status.device_name.clone())
                .unwrap_or_default();

            // Low battery warning.
            let batt = status.battery;
            if should_warn_battery(batt, &dev_id, batt_warned_for.as_deref()) {
                batt_warned_for = Some(dev_id.clone());
                let name = status.device_name.clone().unwrap_or_else(|| "Device".into());
                state.broadcast(
                    "toast",
                    &ToastEvent {
                        level: "warning",
                        title: "Low battery".into(),
                        message: format!("{name} battery is at {batt:.0}%."),
                    },
                );
                info!("[monitor] low battery warning: {name} at {batt:.0}%");
            }
            if should_clear_battery_warning(batt, &dev_id, batt_warned_for.as_deref()) {
                batt_warned_for = None;
            }

            // Signal degradation warning.
            let (good, bad) = count_signal_quality(&status.channel_quality);
            if good >= 2 {
                had_good_signal = true;
            }
            if had_good_signal && bad >= 2 && status.sample_count > 0 {
                if bad_signal_since.is_none() {
                    bad_signal_since = Some(Instant::now());
                }
                if let Some(since) = bad_signal_since {
                    if since.elapsed() >= Duration::from_secs(20) && signal_warned_for.as_deref() != Some(&dev_id) {
                        signal_warned_for = Some(dev_id.clone());
                        state.broadcast(
                            "toast",
                            &ToastEvent {
                                level: "warning",
                                title: "Signal quality dropped".into(),
                                message: "EEG signal became poor during recording. Re-seat electrodes / adjust fit."
                                    .into(),
                            },
                        );
                        info!("[monitor] signal quality warning for {dev_id}");
                    }
                }
            } else {
                bad_signal_since = None;
                if good >= 2 && signal_warned_for.as_deref() == Some(&dev_id) {
                    signal_warned_for = None;
                }
            }

            // Broadcast periodic status so WS clients stay up to date.
            state.broadcast("status", &status);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_warn_at_15_percent() {
        assert!(should_warn_battery(15.0, "dev1", None));
    }

    #[test]
    fn battery_no_warn_at_16_percent() {
        assert!(!should_warn_battery(16.0, "dev1", None));
    }

    #[test]
    fn battery_no_warn_at_zero() {
        assert!(!should_warn_battery(0.0, "dev1", None));
    }

    #[test]
    fn battery_no_warn_if_already_warned() {
        assert!(!should_warn_battery(10.0, "dev1", Some("dev1")));
    }

    #[test]
    fn battery_clear_at_25_percent() {
        assert!(should_clear_battery_warning(25.0, "dev1", Some("dev1")));
    }

    #[test]
    fn battery_no_clear_at_24_percent() {
        assert!(!should_clear_battery_warning(24.0, "dev1", Some("dev1")));
    }

    #[test]
    fn signal_quality_counts() {
        let q: Vec<String> = vec![
            "good".into(),
            "good".into(),
            "poor".into(),
            "no_signal".into(),
            "fair".into(),
        ];
        assert_eq!(count_signal_quality(&q), (2, 2));
    }

    #[test]
    fn signal_quality_empty() {
        assert_eq!(count_signal_quality(&[]), (0, 0));
    }
}
