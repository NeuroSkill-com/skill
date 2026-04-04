// SPDX-License-Identifier: GPL-3.0-only
//! LSL auto-scanner bootstrap (ownership moved to daemon).

use tauri::AppHandle;

/// No-op — LSL auto-connect scanner ownership moved to daemon.
pub(crate) fn maybe_start_lsl_auto_scanner(_app: &AppHandle) {
    // Daemon owns the auto-connect scanner; nothing to do here.
}
