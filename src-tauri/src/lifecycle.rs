// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Session lifecycle — start, cancel, disconnect, reconnect backoff.

use std::time::Duration;

use tauri::{AppHandle, Manager};

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
fn detect_device_kind(id: Option<&str>, name_lower: Option<&str>) -> &'static str {
    // Check ID prefix first — Cortex and USB scanner prefix device IDs.
    if let Some(id) = id {
        if id.starts_with("cortex:") {
            return "emotiv";
        }
        if id.starts_with("usb:") {
            return "ganglion";
        }
        if id.starts_with("cgx:") {
            return "cognionics";
        }
        if id.starts_with("lsl:") {
            return "lsl";
        }
        if id == "lsl-iroh" {
            return "lsl-iroh";
        }
    }
    use skill_data::device::DeviceKind;
    match DeviceKind::from_name(name_lower) {
        DeviceKind::OpenBci => "muse", // serial/WiFi boards use connect_openbci command
        DeviceKind::Unknown => "muse",
        other => other.as_str(),
    }
}

// ── Reconnect backoff ─────────────────────────────────────────────────────────

/// Maximum number of automatic reconnect attempts before giving up and
/// staying in the "disconnected" state.  After this many consecutive
/// failures the user must manually re-connect.
///
/// 12 attempts ≈ 1 + 2 + 3 + 5×9 = 51 seconds of total backoff, which is
/// enough to survive brief radio interference while not burning battery on
/// a headset that was intentionally turned off.
pub(crate) const MAX_RETRY_ATTEMPTS: u32 = 12;

/// Reconnect backoff: 1 s → 2 s → 3 s → 5 s, then stays at 5 s indefinitely.
pub(crate) fn retry_delay_secs(attempt: u32) -> u32 {
    match attempt {
        0 => 1,
        1 => 2,
        2 => 3,
        _ => 5,
    }
}

// ── Disconnect / retry ────────────────────────────────────────────────────────

pub(crate) fn go_disconnected(app: &AppHandle, error: Option<String>, is_bt: bool) {
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
            start_session(&app, preferred);
        });
    }
}

// ── Session lifecycle ─────────────────────────────────────────────────────────

/// Try to start a session.  Returns `true` if the session was started,
/// `false` if another session is already active (with a toast + event).
pub(crate) fn start_session(app: &AppHandle, preferred_id: Option<String>) -> bool {
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        if s.stream.is_some() {
            let current = s
                .status
                .device_name
                .clone()
                .unwrap_or_else(|| s.status.device_kind.clone());
            let requested = preferred_id
                .as_deref()
                .unwrap_or("another device")
                .to_string();
            drop(s);
            crate::send_toast(
                app,
                crate::ToastLevel::Warning,
                "Session Active",
                &format!("Disconnect {current} before connecting to {requested}, or use Switch."),
            );
            let _ = tauri::Emitter::emit(
                app,
                "session-blocked",
                serde_json::json!({
                    "current_device": current,
                    "requested_device": requested,
                }),
            );
            return false;
        }
        s.pending_reconnect = true;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();

    let target = preferred_id.or_else(|| {
        let r = app.app_state();
        let s = r.lock_or_recover();
        s.preferred_id
            .clone()
            .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()))
    });

    let target_name: Option<String> = target.as_ref().and_then(|id| {
        let r = app.app_state();
        let s = r.lock_or_recover();
        s.status
            .paired_devices
            .iter()
            .find(|d| &d.id == id)
            .map(|d| d.name.clone())
            .or_else(|| {
                s.discovered
                    .iter()
                    .find(|d| &d.id == id)
                    .map(|d| d.name.clone())
            })
    });
    let target_lower = target_name.as_deref().map(str::to_lowercase);
    let device_kind = detect_device_kind(target.as_deref(), target_lower.as_deref());

    // For Cortex devices without a resolved name, set a user-visible name
    // so the UI shows something meaningful during the scanning/connecting phase.
    let target_name = target_name.or_else(|| {
        if device_kind == "emotiv" {
            Some("Emotiv Headset".into())
        } else {
            None
        }
    });

    app.app_state().lock_or_recover().stream = Some(StreamHandle { cancel_tx: tx });
    let csv = new_csv_path(app);
    let app2 = app.clone();

    app_log!(
        app,
        "devices",
        "[session] routing: target={target:?} name={target_name:?} kind={device_kind}"
    );

    // Set scanning state with the correct device_kind.
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.session_start_utc = Some(unix_secs());
        s.snr_sum = 0.0;
        s.snr_count = 0;
        s.status
            .reset_for_scanning(device_kind, &csv, target.as_deref());
        // Pin the scanner-level device ID so on_connected can use it
        // for pairing (instead of the adapter's internal session ID).
        if let Some(ref id) = target {
            s.status.device_id = Some(id.clone());
        }
        // Override target_name if we resolved one (handles Cortex devices
        // whose paired ID may not match the current scanner ID).
        if let Some(ref name) = target_name {
            s.status.target_name = Some(name.clone());
        }
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
            "mw75" => crate::session_connect::connect_mw75(&app2, &cancel, target).await,
            "hermes" => crate::session_connect::connect_hermes(&app2, &cancel, target).await,
            "emotiv" => crate::session_connect::connect_emotiv(&app2, &cancel, target).await,
            "idun" => crate::session_connect::connect_idun(&app2, &cancel).await,
            "mendi" => crate::session_connect::connect_mendi(&app2, &cancel, target).await,
            "cognionics" => {
                crate::session_connect::connect_cognionics(&app2, &cancel, target).await
            }
            "lsl" => connect_lsl(target).await,
            "lsl-iroh" => connect_lsl_iroh(&app2).await,
            _ => crate::session_connect::connect_muse(&app2, &cancel, target).await,
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
    true
}

// ── Secondary session management ─────────────────────────────────────────────

/// Start a secondary (background) recording session.
///
/// The secondary session writes its own CSV but does not drive the dashboard
/// or embeddings.  Returns `false` if a session with the same ID already exists.
pub(crate) fn start_secondary_session(
    app: &AppHandle,
    session_id: String,
    adapter: Box<dyn skill_devices::session::DeviceAdapter>,
) -> bool {
    let cancel = tokio_util::sync::CancellationToken::new();
    let csv = new_csv_path(app);
    let desc = adapter.descriptor();

    let info = crate::state::SecondarySessionInfo {
        id: session_id.clone(),
        device_name: desc.kind.to_string(),
        device_kind: detect_device_kind(Some(&session_id), None).to_string(),
        channels: desc.eeg_channels,
        sample_rate: desc.eeg_sample_rate,
        sample_count: 0,
        csv_path: csv.to_string_lossy().to_string(),
        started_at: unix_secs(),
        battery: 0.0,
    };

    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        if s.secondary_sessions.contains_key(&session_id) {
            return false;
        }
        s.secondary_sessions.insert(
            session_id.clone(),
            crate::state::SecondarySessionHandle {
                cancel: cancel.clone(),
                info,
            },
        );
    }
    crate::secondary_session::emit_secondary_status(app);

    let app2 = app.clone();
    let csv2 = csv;
    tauri::async_runtime::spawn(async move {
        crate::secondary_session::run_secondary_session(app2, session_id, cancel, csv2, adapter)
            .await;
    });
    true
}

/// Cancel a specific secondary session by ID.
pub(crate) fn cancel_secondary_session(app: &AppHandle, session_id: &str) {
    let r = app.app_state();
    let s = r.lock_or_recover();
    if let Some(handle) = s.secondary_sessions.get(session_id) {
        handle.cancel.cancel();
    }
}

/// Cancel ALL secondary sessions.
#[allow(dead_code)]
pub(crate) fn cancel_all_secondary_sessions(app: &AppHandle) {
    let r = app.app_state();
    let s = r.lock_or_recover();
    for handle in s.secondary_sessions.values() {
        handle.cancel.cancel();
    }
}

/// Get the list of active secondary sessions (for the frontend).
pub(crate) fn list_secondary_sessions(app: &AppHandle) -> Vec<crate::state::SecondarySessionInfo> {
    let r = app.app_state();
    let s = r.lock_or_recover();
    s.secondary_sessions
        .values()
        .map(|h| h.info.clone())
        .collect()
}

/// Cancel the current session and start a new one in its place.
///
/// This is the "quick switch" action: the user wants to swap from the
/// current device to a different one without two manual steps.
pub(crate) fn switch_session(app: &AppHandle, target: Option<String>) {
    cancel_session(app);
    // The cancel is asynchronous (oneshot → CancellationToken → session loop
    // exit → go_disconnected clears the stream handle).  We poll briefly
    // until the slot is free, then start the new session.
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        // Wait up to 3 s for the old session to clear
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let free = app2.app_state().lock_or_recover().stream.is_none();
            if free {
                break;
            }
        }
        start_session(&app2, target);
    });
}

/// Start a session driven by a remote device streaming EEG over iroh.
///
/// Takes ownership of the `EegChunkRx` from the managed Tauri state and
/// wraps it in an [`IrohRemoteAdapter`] so the standard session runner
/// pipeline (DSP → CSV → embeddings → broadcast → DND → hooks) processes
/// the remote data identically to a local BLE device.
pub(crate) fn start_iroh_remote_session(app: &AppHandle, peer_id: String) {
    use skill_devices::session::iroh_remote::IrohRemoteAdapter;
    use skill_iroh::RemoteEventRx;

    let app2 = app.clone();

    // Take the EegChunkRx from Tauri state (only one remote session at a time).
    let rx_arc = app.state::<std::sync::Arc<tokio::sync::Mutex<Option<RemoteEventRx>>>>();
    let rx_arc2 = rx_arc.inner().clone();

    let (tx, rx_cancel) = tokio::sync::oneshot::channel::<()>();
    app.app_state().lock_or_recover().stream = Some(StreamHandle { cancel_tx: tx });
    let csv = new_csv_path(app);

    // Set scanning / connecting state
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.session_start_utc = Some(unix_secs());
        s.snr_sum = 0.0;
        s.snr_count = 0;

        // Remote iroh sessions are source-driven by incoming tunnel data.
        // Disable BLE-style auto-reconnect loops so pending retry tasks from
        // a previously preferred local device (e.g. AttentivU) stop
        // immediately and don't race with iroh session startup.
        s.pending_reconnect = false;
        s.retry_attempt = 0;

        s.status
            .reset_for_scanning("iroh-remote", &csv, Some(&peer_id));
        s.status.device_id = Some(peer_id.clone());
        s.status.retry_attempt = 0;
        s.status.retry_countdown_secs = 0;

        // Look up the registered client name for this peer so the dashboard
        // can display it (e.g. "Mario's iPhone").
        let client_name = app
            .try_state::<skill_iroh::SharedIrohAuth>()
            .and_then(|auth| skill_iroh::lock_or_recover(&auth).client_name_for_endpoint(&peer_id));
        s.status.iroh_client_name = client_name;
    }
    refresh_tray(app);
    emit_status(app);

    tauri::async_runtime::spawn(async move {
        // Take the receiver out of the mutex
        let chunk_rx = {
            let mut guard = rx_arc2.lock().await;
            match guard.take() {
                Some(rx) => rx,
                None => {
                    eprintln!("[iroh-remote] no EegChunkRx available — another session active?");
                    crate::go_disconnected(
                        &app2,
                        Some("No remote EEG channel available".into()),
                        false,
                    );
                    return;
                }
            }
        };

        let cancel = tokio_util::sync::CancellationToken::new();
        let cancel2 = cancel.clone();
        tokio::spawn(async move {
            let _ = rx_cancel.await;
            cancel2.cancel();
        });

        // Default to Muse configuration (4ch @ 256 Hz with PPG) since that's
        // the device the iOS SkillClient connects to.  The adapter dynamically
        // adjusts if the first chunk header reports different parameters.
        // The adapter starts with default Muse config but will update
        // when it receives a DeviceConnected event from the remote.
        let adapter = IrohRemoteAdapter::new(chunk_rx, peer_id);

        run_device_session(app2.clone(), cancel, csv, Box::new(adapter)).await;

        // Session ended — the adapter's rx was consumed.  Create a fresh
        // tx/rx pair: swap the tunnel's shared tx so new iroh connections
        // send into the new channel, and store the new rx in managed state.
        let (new_tx, new_rx) = skill_iroh::event_channel();
        {
            let shared_tx = app2.state::<skill_iroh::SharedDeviceEventTx>();
            let mut guard = shared_tx
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *guard = Some(new_tx);
        }
        {
            let mut guard = rx_arc2.lock().await;
            *guard = Some(new_rx);
        }
    });
}

/// Spawn a background task that watches for incoming EEG data from the iroh
/// tunnel and automatically starts a remote session when the first chunk
/// arrives.
///
/// Call once at app startup (after `skill_iroh::spawn`).
pub(crate) fn spawn_iroh_eeg_watcher(app: &AppHandle) {
    let app2 = app.clone();
    let rx_arc =
        app.state::<std::sync::Arc<tokio::sync::Mutex<Option<skill_iroh::RemoteEventRx>>>>();
    let rx_arc2 = rx_arc.inner().clone();

    tauri::async_runtime::spawn(async move {
        loop {
            // Peek at the channel: wait until a chunk is available, then
            // start a session to consume it.  We don't actually take the
            // chunk — start_iroh_remote_session will take the whole Rx.
            //
            // We need to detect that the Rx has data waiting.  The simplest
            // approach: take the Rx, recv one chunk, put it back through a
            // new channel that has the chunk pre-loaded, then start the session.
            let has_data = {
                let guard = rx_arc2.lock().await;
                guard.is_some()
            };
            if !has_data {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }

            // Try to peek — take the rx, recv with timeout, put back
            let chunk: Option<skill_iroh::RemoteDeviceEvent> = {
                let mut guard = rx_arc2.lock().await;
                let Some(mut rx) = guard.take() else {
                    drop(guard);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                };
                match tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await {
                    Ok(Some(chunk)) => {
                        // Got a chunk — we need to create a new channel with this
                        // chunk pre-loaded + the remaining rx
                        let (new_tx, new_rx): (
                            skill_iroh::RemoteEventTx,
                            skill_iroh::RemoteEventRx,
                        ) = skill_iroh::event_channel();
                        let _ = new_tx.send(chunk.clone()).await;
                        // Spawn a forwarder to keep piping the old rx into new_tx
                        tokio::spawn(async move {
                            while let Some(c) = rx.recv().await {
                                if new_tx.send(c).await.is_err() {
                                    break;
                                }
                            }
                        });
                        *guard = Some(new_rx);
                        Some(chunk)
                    }
                    Ok(None) => {
                        // Channel closed — iroh tunnel shut down
                        *guard = None;
                        None
                    }
                    Err(_) => {
                        // Timeout — no data yet, put rx back
                        *guard = Some(rx);
                        None
                    }
                }
            };

            if let Some(chunk) = chunk {
                // Check if a session is already running
                let session_active = {
                    let r = app2.app_state();
                    let s = r.lock_or_recover();
                    s.stream.is_some()
                };
                if session_active {
                    // A local BLE session is already running.  The iroh
                    // remote data will be consumed when that session ends
                    // and a new one starts.  Don't block — just wait.
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }

                let peer_id = match &chunk {
                    skill_iroh::RemoteDeviceEvent::DeviceConnected {
                        descriptor_json, ..
                    } => {
                        // Extract device ID from the JSON if available
                        serde_json::from_str::<serde_json::Value>(descriptor_json)
                            .ok()
                            .and_then(|v| v["id"].as_str().map(str::to_owned))
                            .unwrap_or_else(|| "iroh-remote".into())
                    }
                    _ => "iroh-remote".into(),
                };
                eprintln!("[iroh-remote] auto-starting session from peer={peer_id}");
                start_iroh_remote_session(&app2, peer_id);

                // Wait for this session to end before watching again
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let still_active = {
                        let r = app2.app_state();
                        let s = r.lock_or_recover();
                        s.stream.is_some()
                    };
                    if !still_active {
                        break;
                    }
                }
            } else {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    });
}

/// Connect to a local LSL stream.  `target` is an optional stream name filter.
async fn connect_lsl(
    target: Option<String>,
) -> Result<Box<dyn skill_devices::session::DeviceAdapter>, crate::session_connect::ConnectError> {
    // Strip "lsl:" prefix if present
    let query_target = target
        .as_ref()
        .map(|t| t.strip_prefix("lsl:").unwrap_or(t).to_string())
        .filter(|s| !s.is_empty());
    let streams = tokio::task::spawn_blocking(move || skill_lsl::resolve_eeg_streams(5.0))
        .await
        .map_err(|e| crate::session_connect::ConnectError::Other(format!("LSL resolve: {e}")))?;

    if streams.is_empty() {
        return Err(crate::session_connect::ConnectError::Other(
            "No LSL EEG streams found on the network".into(),
        ));
    }

    // If target specified, find matching stream; otherwise use first
    let info = if let Some(ref name) = query_target {
        streams
            .iter()
            .find(|s| s.name().contains(name.as_str()))
            .or(streams.first())
            .cloned()
            .ok_or_else(|| {
                crate::session_connect::ConnectError::Other(format!(
                    "No LSL stream matching '{name}'"
                ))
            })?
    } else {
        streams
            .into_iter()
            .next()
            .expect("streams verified non-empty above")
    };

    eprintln!(
        "[lsl] connecting to '{}' ({}ch @ {}Hz)",
        info.name(),
        info.channel_count(),
        info.nominal_srate()
    );
    let adapter = skill_lsl::LslAdapter::new(&info);
    Ok(Box::new(adapter))
}

/// Start an rlsl-iroh sink and wait for a remote LSL stream.
async fn connect_lsl_iroh(
    app: &AppHandle,
) -> Result<Box<dyn skill_devices::session::DeviceAdapter>, crate::session_connect::ConnectError> {
    let (adapter, endpoint_id) = skill_lsl::IrohLslAdapter::start_sink()
        .await
        .map_err(|e| crate::session_connect::ConnectError::Other(format!("rlsl-iroh sink: {e}")))?;
    eprintln!("[lsl-iroh] sink started, endpoint_id={endpoint_id}");
    // Store the endpoint ID so lsl_iroh_status can report it.
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.lsl_iroh_endpoint_id = Some(endpoint_id);
    }
    Ok(Box::new(adapter))
}

pub(crate) fn cancel_session(app: &AppHandle) {
    let tx = app
        .app_state()
        .lock_or_recover()
        .stream
        .take()
        .map(|sh| sh.cancel_tx);
    if let Some(tx) = tx {
        let _ = tx.send(());
    }
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
