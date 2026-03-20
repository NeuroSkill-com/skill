// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Device discovery, pairing, and connection Tauri commands.

use std::sync::Mutex;
use crate::MutexExt;
use tauri::AppHandle;

use crate::{
    AppState, DeviceStatus, DiscoveredDevice,
    emit_status, emit_devices, save_settings,
    start_session, cancel_session,
    AppStateExt,
};
use crate::tray::refresh_tray;

// ── Device commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_status(state: tauri::State<'_, Mutex<Box<AppState>>>) -> DeviceStatus {
    state.lock_or_recover().status.clone()
}

#[tauri::command]
pub fn get_devices(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<DiscoveredDevice> {
    state.lock_or_recover().discovered.clone()
}

#[tauri::command]
pub fn get_supported_companies() -> Vec<skill_data::device::SupportedCompany> {
    skill_data::device::supported_companies()
}

#[tauri::command]
pub fn get_device_capabilities(device_name: Option<String>) -> skill_data::device::DeviceCapabilities {
    let kind = skill_data::device::DeviceKind::from_name(device_name.as_deref());
    kind.capabilities()
}

#[tauri::command]
pub fn set_preferred_device(id: String, app: AppHandle) -> Vec<DiscoveredDevice> {
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.preferred_id = if id.is_empty() { None } else { Some(id.clone()) };
        let pref = s.preferred_id.clone();
        for d in s.discovered.iter_mut() { d.is_preferred = pref.as_deref() == Some(&d.id); }
    }
    save_settings(&app);
    emit_devices(&app);
    app.app_state().lock_or_recover().discovered.clone()
}

/// Explicitly pair a discovered device so it is trusted for future connections.
///
/// Adds the device to `paired_devices`, marks it as `is_paired` in the
/// discovered list, persists settings, and broadcasts updated state.
#[tauri::command]
pub fn pair_device(id: String, app: AppHandle) -> Vec<DiscoveredDevice> {
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        // Look up the name from the discovered list.
        let name = s.discovered.iter()
            .find(|d| d.id == id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| id.clone());
        let now = crate::unix_secs();
        // Insert into paired list if not already there.
        if !s.status.paired_devices.iter().any(|d| d.id == id) {
            s.status.paired_devices.push(crate::PairedDevice {
                id:        id.clone(),
                name:      name.clone(),
                last_seen: now,
            });
        }
        // Mark as paired in the discovered/settings list.
        for d in s.discovered.iter_mut() {
            if d.id == id {
                d.is_paired = true;
                d.name      = name.clone();
            }
        }
    }
    save_settings(&app);
    refresh_tray(&app);
    emit_status(&app);
    emit_devices(&app);
    app.app_state().lock_or_recover().discovered.clone()
}

#[tauri::command]
pub fn forget_device(id: String, app: AppHandle) -> DeviceStatus {
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.status.paired_devices.retain(|d| d.id != id);
        for d in s.discovered.iter_mut() { if d.id == id { d.is_paired = false; } }
        drop(s);
        save_settings(&app);
    }
    refresh_tray(&app); emit_status(&app); emit_devices(&app);
    app.app_state().lock_or_recover().status.clone()
}

#[tauri::command]
pub fn cancel_retry(app: AppHandle) {
    let r = app.app_state();
    let mut s = r.lock_or_recover();
    s.pending_reconnect           = false;
    s.retry_attempt               = 0;
    s.status.retry_attempt        = 0;
    s.status.retry_countdown_secs = 0;
    s.status.state                = "disconnected".into();
    s.status.device_error             = None;
    drop(s);
    cancel_session(&app);
    emit_status(&app);
}

#[tauri::command]
pub fn retry_connect(app: AppHandle) {
    let preferred = {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.pending_reconnect = true;
        s.retry_attempt     = 0;
        s.status.retry_attempt        = 0;
        s.status.retry_countdown_secs = 0;
        s.preferred_id.clone()
            .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()))
    };
    start_session(&app, preferred);
}

