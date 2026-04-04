// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Device discovery, pairing, and connection Tauri commands.

use crate::MutexExt;
use tauri::AppHandle;

use crate::tray::refresh_tray;
use crate::{emit_devices, emit_status, AppStateExt, DeviceStatus, DiscoveredDevice};

// ── Device commands ────────────────────────────────────────────────────────────

fn map_daemon_device(d: skill_daemon_common::DiscoveredDeviceResponse) -> DiscoveredDevice {
    DiscoveredDevice {
        id: d.id,
        name: d.name,
        last_seen: d.last_seen,
        last_rssi: d.last_rssi,
        is_paired: d.is_paired,
        is_preferred: d.is_preferred,
        transport: skill_daemon_common::DeviceTransport::from_wire(&d.transport),
    }
}

fn apply_daemon_devices_to_local(
    app: &AppHandle,
    daemon_devices: Vec<skill_daemon_common::DiscoveredDeviceResponse>,
) {
    let mapped: Vec<DiscoveredDevice> = daemon_devices.into_iter().map(map_daemon_device).collect();
    let preferred = mapped.iter().find(|d| d.is_preferred).map(|d| d.id.clone());

    let r = app.app_state();
    let mut s = r.lock_or_recover();
    s.discovered = mapped;
    s.preferred_id = preferred;
}

fn apply_daemon_status_to_local(app: &AppHandle, daemon: skill_daemon_common::StatusResponse) {
    let r = app.app_state();
    let mut s = r.lock_or_recover();
    s.status.state = daemon.state;
    s.status.device_name = daemon.device_name;
    s.status.sample_count = daemon.sample_count;
    s.status.battery = daemon.battery;
    s.status.device_error = daemon.device_error;
    s.status.target_name = daemon.target_name;
    s.status.retry_attempt = daemon.retry_attempt;
    s.status.retry_countdown_secs = daemon.retry_countdown_secs;
    s.status.paired_devices = daemon
        .paired_devices
        .into_iter()
        .map(|d| crate::PairedDevice {
            id: d.id,
            name: d.name,
            last_seen: d.last_seen,
        })
        .collect();
}

fn mark_daemon_unavailable(app: &AppHandle, err: &str) {
    let r = app.app_state();
    let mut s = r.lock_or_recover();
    s.status.state = "disconnected".into();
    s.status.device_error = Some(format!("daemon unavailable: {err}"));
}

#[tauri::command]
pub fn get_supported_companies() -> Vec<skill_data::device::SupportedCompany> {
    skill_data::device::supported_companies()
}

#[tauri::command]
pub fn get_device_capabilities(
    device_name: Option<String>,
) -> skill_data::device::DeviceCapabilities {
    let kind = skill_data::device::DeviceKind::from_name(device_name.as_deref());
    kind.capabilities()
}

#[tauri::command]
pub fn set_preferred_device(id: String, app: AppHandle) -> Vec<DiscoveredDevice> {
    let daemon_devices = match crate::daemon_cmds::set_preferred_device(id) {
        Ok(v) => v,
        Err(err) => {
            mark_daemon_unavailable(&app, &err);
            emit_status(&app);
            return app.app_state().lock_or_recover().discovered.clone();
        }
    };

    apply_daemon_devices_to_local(&app, daemon_devices);
    if let Ok(status) = crate::daemon_cmds::fetch_daemon_status() {
        apply_daemon_status_to_local(&app, status);
    }

    emit_devices(&app);
    app.app_state().lock_or_recover().discovered.clone()
}

#[tauri::command]
pub fn forget_device(id: String, app: AppHandle) -> DeviceStatus {
    let daemon_devices = match crate::daemon_cmds::forget_device(id) {
        Ok(v) => v,
        Err(err) => {
            mark_daemon_unavailable(&app, &err);
            emit_status(&app);
            return app.app_state().lock_or_recover().status.clone();
        }
    };

    apply_daemon_devices_to_local(&app, daemon_devices);
    if let Ok(status) = crate::daemon_cmds::fetch_daemon_status() {
        apply_daemon_status_to_local(&app, status);
    }

    refresh_tray(&app);
    emit_status(&app);
    emit_devices(&app);
    app.app_state().lock_or_recover().status.clone()
}

#[tauri::command]
pub fn cancel_retry(app: AppHandle) {
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.pending_reconnect = false;
    }

    match crate::daemon_cmds::cancel_retry() {
        Ok(daemon_status) => apply_daemon_status_to_local(&app, daemon_status),
        Err(err) => {
            mark_daemon_unavailable(&app, &err);
            emit_status(&app);
            return;
        }
    }

    emit_status(&app);
}

#[tauri::command]
pub fn retry_connect(app: AppHandle) {
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.pending_reconnect = true;
    }

    match crate::daemon_cmds::retry_connect() {
        Ok(daemon_status) => apply_daemon_status_to_local(&app, daemon_status),
        Err(err) => {
            mark_daemon_unavailable(&app, &err);
            emit_status(&app);
            return;
        }
    }

    emit_status(&app);
}
