// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Auto-update opt-out preference.
//!
//! When enabled (the default), the frontend automatically downloads and
//! installs an update as soon as the background poller emits
//! `update-available`. When disabled, the same event surfaces a notice in
//! the Updates tab and the user must click "Install" to proceed.
//!
//! Storage mirrors `update_channel.rs`: a single ASCII line in
//! `<app_local_data>/auto-update.txt` containing `true` or `false`. A
//! missing or unreadable file is treated as `true` so first-run users get
//! today's behavior.

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const PREF_FILE: &str = "auto-update.txt";

fn pref_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_local_data_dir()
        .ok()
        .map(|d| d.join(PREF_FILE))
}

pub fn read_auto_update_enabled(app: &AppHandle) -> bool {
    let Some(path) = pref_path(app) else {
        return true;
    };
    match std::fs::read_to_string(&path) {
        Ok(s) => match s.trim().to_ascii_lowercase().as_str() {
            "false" => false,
            "true" => true,
            _ => true,
        },
        Err(_) => true,
    }
}

#[tauri::command]
pub fn get_auto_update_enabled(app: AppHandle) -> bool {
    read_auto_update_enabled(&app)
}

#[tauri::command]
pub fn set_auto_update_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let path = pref_path(&app).ok_or_else(|| "app_local_data_dir unavailable".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, if enabled { "true" } else { "false" }).map_err(|e| e.to_string())?;
    Ok(())
}
