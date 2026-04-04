// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Do Not Disturb automation Tauri commands.

// ── Do Not Disturb automation ─────────────────────────────────────────────────

/// Open a native file-picker dialog and return the selected WAV file path.
///
/// Returns `None` if the user cancels.  The dialog is opened on a blocking
/// thread so it does not hold the Tauri async executor.
#[tauri::command]
pub async fn pick_ref_wav_file() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("WAV audio", &["wav"])
            .set_title("Select reference WAV for voice cloning")
            .pick_file()
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .ok()
    .flatten()
}
