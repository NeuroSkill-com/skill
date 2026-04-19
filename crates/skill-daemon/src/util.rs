// Re-export all util functions from skill-daemon-state.
pub(crate) use skill_daemon_state::util::*;

use crate::state::AppState;

/// Spawn the appropriate session runner for the given target device.
/// Cancels any existing session first.
pub(crate) fn spawn_session_for_target(state: &AppState, target: Option<&str>) {
    let Some(t) = target else { return };

    // Idempotency guard: if we're already connecting/connected to the same
    // target and have an active session handle, do not cancel/restart.
    // Also prevent reconnect from killing an active session to a *different*
    // device (e.g. user manually connected device B while reconnect keeps
    // retrying device A).
    let (same_target_active, other_target_connected) = {
        let (status_same, status_connected) = state
            .status
            .lock()
            .ok()
            .map(|s| {
                let same = (s.state == "connecting" || s.state == "connected")
                    && (s.target_id.as_deref() == Some(t)
                        || s.target_name.as_deref() == Some(t)
                        || s.target_display_name.as_deref() == Some(t));
                let connected_other =
                    s.state == "connected" && !same && (s.target_id.is_some() || s.target_name.is_some());
                (same, connected_other)
            })
            .unwrap_or((false, false));
        let handle_active = state
            .session_handle
            .lock()
            .ok()
            .map(|slot| slot.is_some())
            .unwrap_or(false);
        (status_same && handle_active, status_connected && handle_active)
    };
    if same_target_active {
        push_device_log(
            state,
            "session",
            &format!("spawn_session_for_target noop: already active target={t}"),
        );
        return;
    }
    if other_target_connected {
        push_device_log(
            state,
            "session",
            &format!("spawn_session_for_target noop: another device is connected, won't cancel for target={t}"),
        );
        return;
    }

    // Cancel any existing session.
    if let Ok(mut slot) = state.session_handle.lock() {
        if let Some(handle) = slot.take() {
            let _ = handle.cancel_tx.send(());
        }
    }

    // All devices route through the generic adapter session runner.
    let handle = crate::session::spawn_device_session(state.clone(), t.to_string());

    if let Some(h) = handle {
        if let Ok(mut slot) = state.session_handle.lock() {
            *slot = Some(h);
        }
    }
}
