// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! System tray icon, tooltip and context menu.

use std::sync::Mutex;
use crate::MutexExt;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Manager,
};

use crate::{AppState, MuseStatus};

// ── Embedded icons ────────────────────────────────────────────────────────────

const ICON_CONNECTED:    &[u8] = include_bytes!("../icons/tray-connected.png");
const ICON_DISCONNECTED: &[u8] = include_bytes!("../icons/tray-disconnected.png");
const ICON_SCANNING:     &[u8] = include_bytes!("../icons/tray-scanning.png");
const ICON_BT_OFF:       &[u8] = include_bytes!("../icons/tray-bt-off.png");

fn icon_connected()             -> Image<'static> { Image::from_bytes(ICON_CONNECTED).unwrap() }
pub(crate) fn icon_disconnected() -> Image<'static> { Image::from_bytes(ICON_DISCONNECTED).unwrap() }
fn icon_scanning()              -> Image<'static> { Image::from_bytes(ICON_SCANNING).unwrap() }
fn icon_bt_off()                -> Image<'static> { Image::from_bytes(ICON_BT_OFF).unwrap() }

// ── Menu builder ──────────────────────────────────────────────────────────────

pub(crate) fn build_menu(app: &AppHandle, st: &MuseStatus) -> tauri::Result<Menu<tauri::Wry>> {
    let (label_shortcut, search_shortcut, settings_shortcut, calibration_shortcut,
         help_shortcut, history_shortcut, api_shortcut, focus_timer_shortcut) = {
        let r = app.state::<Mutex<AppState>>();
        let g = r.lock_or_recover();
        (
            g.label_shortcut.clone(),
            g.search_shortcut.clone(),
            g.settings_shortcut.clone(),
            g.calibration_shortcut.clone(),
            g.help_shortcut.clone(),
            g.history_shortcut.clone(),
            g.api_shortcut.clone(),
            g.focus_timer_shortcut.clone(),
        )
    };

    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "open_skill", "Open NeuroSkill™", true, Some("CmdOrCtrl+Shift+O"))?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    match st.state.as_str() {
        "connected" => {
            let name = st.device_name.as_deref().unwrap_or("BCI device");
            menu.append(&MenuItem::with_id(app, "info", format!("● {name}"), false, None::<&str>)?)?;
            if st.battery > 0.0 {
                menu.append(&MenuItem::with_id(app, "battery_info",
                    format!("🔋 {:.0}%", st.battery), false, None::<&str>)?)?;
            }
            menu.append(&PredefinedMenuItem::separator(app)?)?;
            menu.append(&MenuItem::with_id(app, "disconnect", "Disconnect", true, None::<&str>)?)?;
        }
        "scanning" => {
            let lbl = match &st.target_name {
                Some(n) => format!("Searching for {n}…"),
                None    => "Scanning for BCI device…".into(),
            };
            menu.append(&MenuItem::with_id(app, "scan_info", &lbl, false, None::<&str>)?)?;
            menu.append(&PredefinedMenuItem::separator(app)?)?;
            menu.append(&MenuItem::with_id(app, "cancel", "Cancel", true, None::<&str>)?)?;
        }
        "bt_off" => {
            menu.append(&MenuItem::with_id(app, "bt_info", "⚠ Bluetooth Unavailable", false, None::<&str>)?)?;
            menu.append(&PredefinedMenuItem::separator(app)?)?;
            menu.append(&MenuItem::with_id(app, "retry",   "Retry Connection",         true, None::<&str>)?)?;
            menu.append(&MenuItem::with_id(app, "open_bt", "Open Bluetooth Settings…", true, None::<&str>)?)?;
        }
        _ => { // disconnected
            if st.paired_devices.is_empty() {
                menu.append(&MenuItem::with_id(app, "scan", "Scan for BCI Device", true, None::<&str>)?)?;
            } else {
                for dev in &st.paired_devices {
                    menu.append(&MenuItem::with_id(app, format!("connect:{}", dev.id),
                        format!("Connect to {}", dev.name), true, None::<&str>)?)?;
                }
                menu.append(&PredefinedMenuItem::separator(app)?)?;
                menu.append(&MenuItem::with_id(app, "scan", "Scan for New Device", true, None::<&str>)?)?;
                menu.append(&PredefinedMenuItem::separator(app)?)?;
                let fsub = Submenu::with_id(app, "forget_sub", "Forget Device", true)?;
                for dev in &st.paired_devices {
                    fsub.append(&MenuItem::with_id(app, format!("forget:{}", dev.id),
                        format!("Forget {}", dev.name), true, None::<&str>)?)?;
                }
                menu.append(&fsub)?;
            }
        }
    }

    let label_accel:       Option<&str> = if label_shortcut.is_empty()       { None } else { Some(&label_shortcut) };
    let search_accel:      Option<&str> = if search_shortcut.is_empty()      { None } else { Some(&search_shortcut) };
    let settings_accel:    Option<&str> = if settings_shortcut.is_empty()    { None } else { Some(&settings_shortcut) };
    let calibration_accel: Option<&str> = if calibration_shortcut.is_empty() { None } else { Some(&calibration_shortcut) };
    let help_accel:        Option<&str> = if help_shortcut.is_empty()        { None } else { Some(&help_shortcut) };
    let history_accel:     Option<&str> = if history_shortcut.is_empty()     { None } else { Some(&history_shortcut) };
    let api_accel:         Option<&str> = if api_shortcut.is_empty()         { None } else { Some(&api_shortcut) };
    let focus_timer_accel: Option<&str> = if focus_timer_shortcut.is_empty() { None } else { Some(&focus_timer_shortcut) };

    let is_streaming = st.state == "connected";
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "focus_timer", "Focus Timer…",        true, focus_timer_accel)?)?;
    menu.append(&MenuItem::with_id(app, "calibrate",   "Calibrate…",          is_streaming, calibration_accel)?)?;
    menu.append(&MenuItem::with_id(app, "search",      "Search…",  true, search_accel)?)?;
    menu.append(&MenuItem::with_id(app, "label",       "Add Label…",          true, label_accel)?)?;
    menu.append(&MenuItem::with_id(app, "history",     "History…",            true, history_accel)?)?;
    menu.append(&MenuItem::with_id(app, "compare",     "Compare…",            true, Some("CmdOrCtrl+Shift+M"))?)?;
    menu.append(&MenuItem::with_id(app, "settings",    "Settings…",           true, settings_accel)?)?;
    menu.append(&MenuItem::with_id(app, "help",        "Help…",               true, help_accel)?)?;
    menu.append(&MenuItem::with_id(app, "api",         "API Status…",         true, api_accel)?)?;

    {
        let queue  = app.state::<std::sync::Arc<crate::job_queue::JobQueue>>();
        let stats  = queue.stats();
        let total: i64 = stats["total_active"].as_i64().unwrap_or(0);
        if total > 0 {
            let est: u64  = stats["est_secs"].as_u64().unwrap_or(0);
            let running   = stats["running"].as_bool().unwrap_or(false);
            let label = if running {
                format!("⏳ {total} task{} in queue (~{est}s)", if total == 1 { "" } else { "s" })
            } else {
                format!("⏳ {total} task{} queued (~{est}s)", if total == 1 { "" } else { "s" })
            };
            menu.append(&PredefinedMenuItem::separator(app)?)?;
            menu.append(&MenuItem::with_id(app, "queue_info", &label, false, None::<&str>)?)?;
        }
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "check_update", "Check for Updates…", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "about", format!("About {}…", crate::constants::APP_DISPLAY_NAME), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok(menu)
}

pub(crate) fn refresh_tray(app: &AppHandle) {
    let s_ref = app.state::<Mutex<AppState>>();
    let st = { let g = s_ref.lock_or_recover(); g.status.clone() };
    if let Some(tray) = app.tray_by_id("main") {
        let (icon, tip) = match st.state.as_str() {
            "connected" => (icon_connected(),    "NeuroSkill™ – Connected"),
            "scanning"  => (icon_scanning(),     "NeuroSkill™ – Scanning…"),
            "bt_off"    => (icon_bt_off(),       "NeuroSkill™ – Bluetooth Off"),
            _           => (icon_disconnected(), "NeuroSkill™ – Disconnected"),
        };
        let _ = tray.set_icon(Some(icon));
        let _ = tray.set_tooltip(Some(tip));
        if let Ok(m) = build_menu(app, &st) { let _ = tray.set_menu(Some(m)); }
    }
}
