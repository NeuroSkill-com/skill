// SPDX-License-Identifier: GPL-3.0-only
//! Session connect commands (daemon-controlled).

use tauri::AppHandle;

use crate::{
    helpers::{apply_daemon_status, emit_status_from_daemon},
    AppStateExt, MutexExt,
};

/// Connect to OpenBCI via daemon session control.
#[tauri::command]
pub(crate) async fn connect_openbci(app: AppHandle) -> Result<(), String> {
    let daemon_status = crate::daemon_cmds::start_session_sync(Some("openbci".to_string()))?;

    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        apply_daemon_status(&mut s.status, daemon_status);
    }

    emit_status_from_daemon(&app);
    Ok(())
}
