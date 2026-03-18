// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Session lifecycle — start, cancel, disconnect, reconnect backoff.

use std::time::Duration;

use tauri::AppHandle;

use skill_eeg::eeg_quality::SignalQuality;

use crate::{
    helpers::{emit_status, unix_secs, AppStateExt},
    session_connect::ConnectError,
    session_csv::new_csv_path,
    session_runner::run_device_session,
    state::StreamHandle,
    tray::refresh_tray,
    MutexExt,
};

// ── Device kind constants ─────────────────────────────────────────────────────

/// Map a lowercased device advertising name to a device-kind routing key.
///
/// Delegates to [`DeviceKind::from_name`] so detection logic is defined in
/// one place.  `OpenBci` and `Unknown` both route to `"muse"` (the default
/// BLE-scan connect path; OpenBCI serial/WiFi uses a separate command).
fn detect_device_kind(name_lower: Option<&str>) -> &'static str {
    use skill_data::device::DeviceKind;
    match DeviceKind::from_name(name_lower) {
        DeviceKind::OpenBci  => "muse", // serial/WiFi boards use connect_openbci command
        DeviceKind::Unknown  => "muse",
        other                => other.as_str(),
    }
}

// ── Reconnect backoff ─────────────────────────────────────────────────────────

/// Reconnect backoff: 1 s → 2 s → 3 s → 5 s, then stays at 5 s indefinitely.
pub(crate) fn retry_delay_secs(attempt: u32) -> u32 {
    match attempt { 0 => 1, 1 => 2, 2 => 3, _ => 5 }
}

// ── Disconnect / retry ────────────────────────────────────────────────────────

pub(crate) fn go_disconnected(app: &AppHandle, error: Option<String>, is_bt: bool) {
    let (retry, attempt) = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        (s.pending_reconnect && !is_bt, s.retry_attempt)
    };
    let delay = if retry { retry_delay_secs(attempt) } else { 0 };

    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        if is_bt {
            s.pending_reconnect = false;
            s.retry_attempt     = 0;
        } else if !retry {
            s.retry_attempt = 0;
        }

        // Reset all device identity / telemetry fields in one call.
        let new_state = if retry { "scanning" }
                        else if is_bt { "bt_off" }
                        else { "disconnected" };
        s.status.reset_disconnected(new_state);

        // Override the defaults set by reset_disconnected for retry-specific values.
        if !retry { s.status.bt_error = error; }
        s.status.retry_attempt        = if retry { attempt + 1 } else { 0 };
        s.status.retry_countdown_secs = delay;
        s.status.channel_quality = vec![SignalQuality::default(); crate::constants::EEG_CHANNELS];

        s.stream       = None;
        s.battery_ema  = None;
        s.latest_bands = None;
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
            app_log!(app, "bluetooth",
                "[reconnect] scheduling attempt #{} in {}s (backoff schedule: 1→2→3→5s)",
                attempt + 1, delay);
            for remaining in (1..=delay).rev() {
                {
                    let r = app.app_state();
                    if !r.lock_or_recover().pending_reconnect { return; }
                }
                app.app_state().lock_or_recover()
                    .status.retry_countdown_secs = remaining;
                emit_status(&app);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            let preferred = {
                let r = app.app_state();
                let mut s = r.lock_or_recover();
                if !s.pending_reconnect { return; }
                s.retry_attempt += 1;
                s.status.retry_countdown_secs = 0;
                s.preferred_id.clone()
                    .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()))
            };
            app_log!(app, "bluetooth",
                "[reconnect] attempt #{} — waited {delay}s — target={preferred:?}", attempt + 1);
            start_session(&app, preferred);
        });
    }
}

// ── Session lifecycle ─────────────────────────────────────────────────────────

pub(crate) fn start_session(app: &AppHandle, preferred_id: Option<String>) {
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        if s.stream.is_some() { return; }
        s.pending_reconnect = true;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();

    let target = preferred_id.or_else(|| {
        let r = app.app_state();
        let s = r.lock_or_recover();
        s.preferred_id.clone()
            .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()))
    });

    let target_name: Option<String> = target.as_ref().and_then(|id| {
        let r = app.app_state();
        let s = r.lock_or_recover();
        s.status.paired_devices.iter()
            .find(|d| &d.id == id).map(|d| d.name.clone())
            .or_else(|| s.discovered.iter().find(|d| &d.id == id).map(|d| d.name.clone()))
    });
    let target_lower = target_name.as_deref().map(|n| n.to_lowercase());
    let device_kind = detect_device_kind(target_lower.as_deref());

    app.app_state().lock_or_recover().stream = Some(StreamHandle { cancel_tx: tx });
    let csv  = new_csv_path(app);
    let app2 = app.clone();

    app_log!(app, "bluetooth",
        "[session] routing: target={target:?} name={target_name:?} kind={device_kind}");

    // Set scanning state with the correct device_kind.
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.session_start_utc = Some(unix_secs());
        s.status.reset_for_scanning(device_kind, &csv, target.as_deref());
    }
    refresh_tray(app);
    emit_status(app);

    tauri::async_runtime::spawn(async move {
        // Use a shared cancellation token so both the connect phase and the
        // session phase observe the same cancel signal.
        let cancel = tokio_util::sync::CancellationToken::new();
        let cancel2 = cancel.clone();

        // Consume the oneshot in a background task that trips the token.
        tokio::spawn(async move {
            let _ = rx.await;
            cancel2.cancel();
        });

        let connect_result = match device_kind {
            "ganglion" => crate::session_connect::connect_ganglion(&app2, &cancel, target).await,
            "mw75"     => crate::session_connect::connect_mw75(&app2, &cancel, target).await,
            "hermes"   => crate::session_connect::connect_hermes(&app2, &cancel, target).await,
            "emotiv"   => crate::session_connect::connect_emotiv(&app2, &cancel).await,
            "idun"     => crate::session_connect::connect_idun(&app2, &cancel).await,
            _          => crate::session_connect::connect_muse(&app2, &cancel, target).await,
        };

        match connect_result {
            Ok(adapter) => {
                run_device_session(app2, cancel, csv, adapter).await;
            }
            Err(ConnectError::Cancelled) => {
                go_disconnected(&app2, None, false);
            }
            Err(ConnectError::Bluetooth(msg)) => {
                go_disconnected(&app2, Some(msg), true);
            }
            Err(ConnectError::Other(msg)) => {
                go_disconnected(&app2, Some(msg), false);
            }
        }
    });
}

pub(crate) fn cancel_session(app: &AppHandle) {
    let tx = app.app_state().lock_or_recover().stream.take().map(|sh| sh.cancel_tx);
    if let Some(tx) = tx { let _ = tx.send(()); }
}

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
            assert_eq!(retry_delay_secs(attempt), 5,
                "attempt {attempt} should be capped at 5 s");
        }
    }

    #[test]
    fn detect_device_kind_ganglion() {
        assert_eq!(detect_device_kind(Some("ganglion-1234")), "ganglion");
        assert_eq!(detect_device_kind(Some("simblee-001")), "ganglion");
    }

    #[test]
    fn detect_device_kind_mw75() {
        assert_eq!(detect_device_kind(Some("headphones-mw75-v2")), "mw75");
        assert_eq!(detect_device_kind(Some("neurable-xyz")), "mw75");
    }

    #[test]
    fn detect_device_kind_hermes() {
        assert_eq!(detect_device_kind(Some("hermes-abc")), "hermes");
    }

    #[test]
    fn detect_device_kind_emotiv() {
        assert_eq!(detect_device_kind(Some("emotiv-epoc-x")), "emotiv");
        assert_eq!(detect_device_kind(Some("epoc-x-1234")), "emotiv");
        assert_eq!(detect_device_kind(Some("insight-5ch")), "emotiv");
        assert_eq!(detect_device_kind(Some("flex-saline")), "emotiv");
        assert_eq!(detect_device_kind(Some("mn8-earbuds")), "emotiv");
    }

    #[test]
    fn detect_device_kind_idun() {
        assert_eq!(detect_device_kind(Some("idun-guardian")), "idun");
        assert_eq!(detect_device_kind(Some("guardian-001")), "idun");
        assert_eq!(detect_device_kind(Some("ige-1234")), "idun");
    }

    #[test]
    fn detect_device_kind_muse_fallback() {
        assert_eq!(detect_device_kind(Some("muse-2")), "muse");
        assert_eq!(detect_device_kind(None), "muse");
        assert_eq!(detect_device_kind(Some("unknown-device")), "muse");
    }
}
