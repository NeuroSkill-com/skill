// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Session lifecycle — disconnect handling.
//!
//! The reconnect state machine now lives in skill-daemon.  This module
//! retains the Tauri-side disconnect handler (local state cleanup + UI
//! refresh) and the device-kind detection heuristic.

use tauri::AppHandle;

use crate::{
    helpers::{emit_status, AppStateExt},
    tray::refresh_tray,
    MutexExt,
};

// ── Disconnect ──────────────────────────────────────────────────────────────

/// Handle a device disconnect.  Cleans up local Tauri state and refreshes
/// the UI.  The daemon's reconnect loop handles retry scheduling.
#[allow(dead_code)]
pub(crate) fn go_disconnected(app: &AppHandle, error: Option<String>, is_bt: bool) {
    // Tell the daemon to cancel any active session.
    let _ = crate::daemon_cmds::cancel_session_sync();

    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();

        let new_state = if is_bt { "bt_off" } else { "disconnected" };
        s.status.reset_disconnected(new_state);
        if !is_bt {
            s.status.device_error = error;
        }
        s.stream = None;
        s.battery_ema = None;
        s.latest_bands = None;
        s.fnirs_runtime = crate::state::FnirsRuntime::default();
        s.session_start_utc = None;
    }
    refresh_tray(app);
    emit_status(app);

    // Tell daemon to disable reconnect for BT-off; otherwise the daemon's
    // reconnect loop handles it automatically.
    if is_bt {
        let _ = crate::daemon_cmds::disable_reconnect();
    }
}

// Device-kind detection logic now lives in `skill_data::device::DeviceKind::from_id_and_name`.
