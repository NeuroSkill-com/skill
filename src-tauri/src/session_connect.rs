// SPDX-License-Identifier: GPL-3.0-only
//! Session connect commands (daemon-controlled).

use tauri::AppHandle;

use crate::{emit_status, AppStateExt, MutexExt};

/// Connect to OpenBCI via daemon session control.
#[tauri::command]
pub(crate) async fn connect_openbci(app: AppHandle) -> Result<(), String> {
    let status = crate::daemon_cmds::start_session(Some("openbci".to_string()))?;

    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.status.state = status.state;
        s.status.device_name = status.device_name;
        s.status.sample_count = status.sample_count;
        s.status.battery = status.battery;
        s.status.device_error = status.device_error;
        s.status.target_name = status.target_name;
        s.status.retry_attempt = status.retry_attempt;
        s.status.retry_countdown_secs = status.retry_countdown_secs;
        s.status.paired_devices = status
            .paired_devices
            .into_iter()
            .map(|d| crate::PairedDevice {
                id: d.id,
                name: d.name,
                last_seen: d.last_seen,
            })
            .collect();
    }

    emit_status(&app);
    Ok(())
}
