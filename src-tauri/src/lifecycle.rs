// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Session lifecycle — start, cancel, disconnect, reconnect backoff.

use std::time::Duration;

use tauri::AppHandle;

use crate::{
    helpers::{emit_status, AppStateExt},
    tray::refresh_tray,
    MutexExt,
};

// ── Reconnect backoff ────────────────────────────────────────────────────────

/// Maximum number of automatic reconnect attempts before giving up and
/// staying in the "disconnected" state.  After this many consecutive
/// failures the user must manually re-connect.
///
/// 12 attempts ≈ 1 + 2 + 3 + 5×9 = 51 seconds of total backoff, which is
/// enough to survive brief radio interference while not burning battery on
/// a headset that was intentionally turned off.
#[allow(dead_code)]
pub(crate) const MAX_RETRY_ATTEMPTS: u32 = 12;

/// Reconnect backoff: 1 s → 2 s → 3 s → 5 s, then stays at 5 s indefinitely.
#[allow(dead_code)]
pub(crate) fn retry_delay_secs(attempt: u32) -> u32 {
    match attempt {
        0 => 1,
        1 => 2,
        2 => 3,
        _ => 5,
    }
}

// ── Disconnect / retry ────────────────────────────────────────────────────────

#[allow(dead_code)]
pub(crate) fn go_disconnected(app: &AppHandle, error: Option<String>, is_bt: bool) {
    // Mirror disconnected state to daemon first
    let state = if is_bt { "bt_off" } else { "disconnected" };
    let _ = crate::daemon_cmds::post_json_with_auth_pub(
        "/v1/status",
        &skill_daemon_common::StatusResponse {
            state: state.to_string(),
            device_name: None,
            sample_count: 0,
            battery: 0.0,
            device_error: error.clone(),
            target_name: None,
            retry_attempt: 0,
            retry_countdown_secs: 0,
            paired_devices: Vec::new(),
        },
    );

    let (mut retry, attempt) = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        (s.pending_reconnect && !is_bt, s.retry_attempt)
    };

    // Give up after MAX_RETRY_ATTEMPTS consecutive failures.
    if retry && attempt >= MAX_RETRY_ATTEMPTS {
        app_log!(
            app,
            "devices",
            "[reconnect] giving up after {attempt} consecutive attempts"
        );
        crate::send_toast(
            app,
            crate::ToastLevel::Error,
            "Reconnect Failed",
            "Could not reconnect after multiple attempts. Please reconnect manually.",
        );
        retry = false;
    }

    let delay = if retry { retry_delay_secs(attempt) } else { 0 };

    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        if is_bt {
            s.pending_reconnect = false;
            s.retry_attempt = 0;
        } else if !retry {
            s.retry_attempt = 0;
        }

        // Reset all device identity / telemetry fields in one call.
        let new_state = if retry {
            "scanning"
        } else if is_bt {
            "bt_off"
        } else {
            "disconnected"
        };
        s.status.reset_disconnected(new_state);

        // Override the defaults set by reset_disconnected for retry-specific values.
        if !retry {
            s.status.device_error = error;
        }
        s.status.retry_attempt = if retry { attempt + 1 } else { 0 };
        s.status.retry_countdown_secs = delay;
        s.status.channel_quality = Vec::new();

        s.stream = None;
        s.battery_ema = None;
        s.latest_bands = None;
        s.fnirs_runtime = crate::state::FnirsRuntime::default();
        // Reset session timestamp so screenshot "sessions only" gate works.
        // Even during auto-reconnect the device is not streaming data,
        // so this is not an active session.
        s.session_start_utc = None;
        // DSP objects live in SessionDsp (session-local, lock-free).
        // They are dropped when the session task exits; the next session
        // creates a fresh set.  No reset needed here.
    }
    refresh_tray(app);
    emit_status(app);

    if retry {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            app_log!(
                app,
                "devices",
                "[reconnect] scheduling attempt #{} in {}s (backoff schedule: 1→2→3→5s)",
                attempt + 1,
                delay
            );
            for remaining in (1..=delay).rev() {
                {
                    let r = app.app_state();
                    if !r.lock_or_recover().pending_reconnect {
                        return;
                    }
                }
                app.app_state()
                    .lock_or_recover()
                    .status
                    .retry_countdown_secs = remaining;
                emit_status(&app);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            let preferred = {
                let r = app.app_state();
                let mut s = r.lock_or_recover();
                if !s.pending_reconnect {
                    return;
                }
                s.retry_attempt += 1;
                s.status.retry_countdown_secs = 0;
                s.preferred_id
                    .clone()
                    .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()))
            };
            app_log!(
                app,
                "devices",
                "[reconnect] attempt #{} — waited {delay}s — target={preferred:?}",
                attempt + 1
            );

            match crate::daemon_cmds::start_session(preferred) {
                Ok(daemon_status) => {
                    let r = app.app_state();
                    let mut s = r.lock_or_recover();
                    s.status.state = daemon_status.state;
                    s.status.device_name = daemon_status.device_name;
                    s.status.sample_count = daemon_status.sample_count;
                    s.status.battery = daemon_status.battery;
                    s.status.device_error = daemon_status.device_error;
                    s.status.target_name = daemon_status.target_name;
                    s.status.retry_attempt = daemon_status.retry_attempt;
                    s.status.retry_countdown_secs = daemon_status.retry_countdown_secs;
                    s.status.paired_devices = daemon_status
                        .paired_devices
                        .into_iter()
                        .map(|d| crate::PairedDevice {
                            id: d.id,
                            name: d.name,
                            last_seen: d.last_seen,
                        })
                        .collect();
                    drop(s);
                    emit_status(&app);
                }
                Err(err) => {
                    let r = app.app_state();
                    let mut s = r.lock_or_recover();
                    s.status.state = "disconnected".into();
                    s.status.device_error = Some(format!("daemon unavailable: {err}"));
                    drop(s);
                    emit_status(&app);
                }
            }
        });
    }
}

// ── Session lifecycle ─────────────────────────────────────────────────────────

// ── Secondary session management ─────────────────────────────────────────────

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_schedule_1_2_3_5() {
        assert_eq!(retry_delay_secs(0), 1, "attempt 0 → 1 s");
        assert_eq!(retry_delay_secs(1), 2, "attempt 1 → 2 s");
        assert_eq!(retry_delay_secs(2), 3, "attempt 2 → 3 s");
        assert_eq!(retry_delay_secs(3), 5, "attempt 3 → 5 s");
    }

    #[test]
    fn backoff_capped_at_5s() {
        for attempt in 3u32..=100 {
            assert_eq!(
                retry_delay_secs(attempt),
                5,
                "attempt {attempt} should be capped at 5 s"
            );
        }
    }

    #[test]
    fn detect_device_kind_ganglion() {
        assert_eq!(detect_device_kind(None, Some("ganglion-1234")), "ganglion");
        assert_eq!(detect_device_kind(None, Some("simblee-001")), "ganglion");
    }

    #[test]
    fn detect_device_kind_mw75() {
        assert_eq!(detect_device_kind(None, Some("headphones-mw75-v2")), "mw75");
        assert_eq!(detect_device_kind(None, Some("neurable-xyz")), "mw75");
    }

    #[test]
    fn detect_device_kind_hermes() {
        assert_eq!(detect_device_kind(None, Some("hermes-abc")), "hermes");
    }

    #[test]
    fn detect_device_kind_emotiv() {
        assert_eq!(detect_device_kind(None, Some("emotiv-epoc-x")), "emotiv");
        assert_eq!(detect_device_kind(None, Some("epoc-x-1234")), "emotiv");
        assert_eq!(detect_device_kind(None, Some("insight-5ch")), "emotiv");
        assert_eq!(detect_device_kind(None, Some("flex-saline")), "emotiv");
        assert_eq!(detect_device_kind(None, Some("mn8-earbuds")), "emotiv");
    }

    #[test]
    fn detect_device_kind_idun() {
        assert_eq!(detect_device_kind(None, Some("idun-guardian")), "idun");
        assert_eq!(detect_device_kind(None, Some("guardian-001")), "idun");
        assert_eq!(detect_device_kind(None, Some("ige-1234")), "idun");
    }

    #[test]
    fn detect_device_kind_mendi() {
        assert_eq!(detect_device_kind(None, Some("mendi")), "mendi");
        assert_eq!(detect_device_kind(None, Some("mendi-1234")), "mendi");
    }

    #[test]
    fn detect_device_kind_cognionics() {
        assert_eq!(
            detect_device_kind(Some("cgx:/dev/ttyUSB0"), None),
            "cognionics"
        );
        assert_eq!(
            detect_device_kind(None, Some("cgx quick-20r")),
            "cognionics"
        );
        assert_eq!(
            detect_device_kind(None, Some("cognionics-device")),
            "cognionics"
        );
        assert_eq!(detect_device_kind(None, Some("quick-20r")), "cognionics");
    }

    #[test]
    fn detect_device_kind_muse_fallback() {
        assert_eq!(detect_device_kind(None, Some("muse-2")), "muse");
        assert_eq!(detect_device_kind(None, None), "muse");
        assert_eq!(detect_device_kind(None, Some("unknown-device")), "muse");
    }

    #[test]
    fn detect_device_kind_by_id_prefix() {
        // Cortex prefix → emotiv regardless of name.
        assert_eq!(
            detect_device_kind(Some("cortex:EPOCX-1234"), None),
            "emotiv"
        );
        assert_eq!(
            detect_device_kind(Some("cortex:EPOCX-1234"), Some("unknown")),
            "emotiv"
        );
        // USB prefix → ganglion (OpenBCI serial).
        assert_eq!(
            detect_device_kind(Some("usb:/dev/ttyUSB0"), None),
            "ganglion"
        );
    }

    #[test]
    fn max_retry_attempts_is_reasonable() {
        // Total backoff time for MAX_RETRY_ATTEMPTS should be < 2 minutes
        // so the user doesn't wait too long, but > 30 s to survive glitches.
        let total: u32 = (0..MAX_RETRY_ATTEMPTS).map(retry_delay_secs).sum();
        assert!(total >= 30, "total backoff {total}s should be >= 30s");
        assert!(total <= 120, "total backoff {total}s should be <= 120s");
    }
}
