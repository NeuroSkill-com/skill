// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Location services settings Tauri commands.

/// Test the location API without changing any setting.
///
/// Returns a JSON object with the location fix or error.
#[tauri::command]
pub async fn test_location() -> Result<serde_json::Value, String> {
    crate::daemon_cmds::test_location()
}
