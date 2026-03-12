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

// ── Tray-update deduplication ─────────────────────────────────────────────────
//
// `tray.set_menu()` replaces the live native menu object. On macOS and Windows
// this dismisses the menu if the user currently has it open, which produces the
// "menu disappears on hover" symptom. The same applies to `set_icon` — an
// unnecessary icon swap can cause a brief flicker on some platforms.
//
// We avoid both problems by caching a fingerprint of the last value we actually
// pushed to the OS. `refresh_tray` is a no-op for the fields that haven't
// changed since the previous call.

/// Fingerprint of the last icon state pushed to the OS tray.
static LAST_ICON_STATE: Mutex<&'static str> = Mutex::new("");

/// Fingerprint of the last menu content pushed to the OS tray.
/// Encodes every piece of data that `build_menu` uses so a rebuild is
/// triggered if and only if the visible menu would differ.
static LAST_MENU_KEY: Mutex<String> = Mutex::new(String::new());

/// Compute a compact string key from all state that `build_menu` renders.
/// Two identical keys guarantee identical menus; a different key means the
/// menu must be rebuilt.
fn menu_key(st: &MuseStatus, app: &AppHandle) -> String {
    let r = app.state::<Mutex<AppState>>();
    let g = r.lock_or_recover();
    let ls   = g.label_shortcut.clone();
    let ss   = g.search_shortcut.clone();
    let sets = g.settings_shortcut.clone();
    let cs   = g.calibration_shortcut.clone();
    let hs   = g.help_shortcut.clone();
    let hist = g.history_shortcut.clone();
    let api  = g.api_shortcut.clone();
    let ts   = g.theme_shortcut.clone();
    let ft   = g.focus_timer_shortcut.clone();
    #[cfg(feature = "llm")]
    let chat = g.chat_shortcut.clone();
    #[cfg(not(feature = "llm"))]
    let chat = String::new();
    drop(g);

    let mut pair_parts = st.paired_devices
        .iter()
        .map(|d| format!("{}:{}", d.id, d.name))
        .collect::<Vec<_>>();
    pair_parts.sort_unstable();
    let pairs = pair_parts.join(",");

    // Battery can jitter between frequent telemetry samples.
    // Bucket to 10% steps so tiny fluctuations do not constantly invalidate
    // the tray menu fingerprint.
    let batt_bucket = if st.battery <= 0.0 {
        0u32
    } else {
        (((st.battery as u32) / 10) * 10).min(100)
    };
    let state = st.state.as_str();
    let name  = st.device_name.as_deref().unwrap_or("");
    let tgt   = if state == "scanning" {
        st.target_name.as_deref().unwrap_or("")
    } else {
        ""
    };

    format!("{state}|{name}|{batt_bucket}|{tgt}|{pairs}|{ls}|{ss}|{sets}|{cs}|{hs}|{hist}|{api}|{ts}|{ft}|{chat}")
}

fn shortcut_suffix(shortcut: &str) -> String {
    if shortcut.trim().is_empty() {
        return String::new();
    }

    let mut s = shortcut.trim().replace("CmdOrCtrl", if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" });
    s = s.replace("Command", "Cmd");
    s = s.replace("Meta", "Cmd");
    s = s.replace("Option", "Alt");
    s = s.replace("Plus", "+");
    s = s.replace("Arrow", "");
    format!("  ({s})")
}

fn with_shortcut(label: &str, shortcut: &str) -> String {
    format!("{label}{}", shortcut_suffix(shortcut))
}

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
    #[cfg(feature = "llm")]
    let chat_shortcut = {
        let r = app.state::<Mutex<AppState>>();
        let s = r.lock_or_recover().chat_shortcut.clone();
        s
    };

    let menu = Menu::new(app)?;
    let open_skill_label = with_shortcut("Open NeuroSkill™", "CmdOrCtrl+Shift+O");
    menu.append(&MenuItem::with_id(app, "open_skill", &open_skill_label, true, None::<&str>)?)?;
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
                let mut paired = st.paired_devices.clone();
                paired.sort_unstable_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));

                for dev in &paired {
                    menu.append(&MenuItem::with_id(app, format!("connect:{}", dev.id),
                        format!("Connect to {}", dev.name), true, None::<&str>)?)?;
                }
                menu.append(&PredefinedMenuItem::separator(app)?)?;
                menu.append(&MenuItem::with_id(app, "scan", "Scan for New Device", true, None::<&str>)?)?;
                menu.append(&PredefinedMenuItem::separator(app)?)?;
                let fsub = Submenu::with_id(app, "forget_sub", "Forget Device", true)?;
                for dev in &paired {
                    fsub.append(&MenuItem::with_id(app, format!("forget:{}", dev.id),
                        format!("Forget {}", dev.name), true, None::<&str>)?)?;
                }
                menu.append(&fsub)?;
            }
        }
    }

    let is_streaming = st.state == "connected";
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "focus_timer", &with_shortcut("Focus Timer…", &focus_timer_shortcut), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "calibrate",   &with_shortcut("Calibrate…", &calibration_shortcut), is_streaming, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "search",      &with_shortcut("Search…", &search_shortcut), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "label",       &with_shortcut("Add Label…", &label_shortcut), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "history",     &with_shortcut("History…", &history_shortcut), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "compare",     &with_shortcut("Compare…", "CmdOrCtrl+Shift+M"), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "settings",    &with_shortcut("Settings…", &settings_shortcut), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "help",        &with_shortcut("Help…", &help_shortcut), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "api",         &with_shortcut("API Status…", &api_shortcut), true, None::<&str>)?)?;
    #[cfg(feature = "llm")]
    {
        menu.append(&MenuItem::with_id(app, "chat", &with_shortcut("Chat…", &chat_shortcut), true, None::<&str>)?)?;
    }

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

    let Some(tray) = app.tray_by_id("main") else { return };

    // ── Icon + tooltip (only update when the state bucket changes) ────────────
    let icon_state: &'static str = match st.state.as_str() {
        "connected" => "connected",
        "scanning"  => "scanning",
        "bt_off"    => "bt_off",
        _           => "disconnected",
    };
    {
        let mut last = LAST_ICON_STATE.lock().unwrap_or_else(|p| p.into_inner());
        if *last != icon_state {
            let (icon, tip) = match icon_state {
                "connected"    => (icon_connected(),    "NeuroSkill™ – Connected"),
                "scanning"     => (icon_scanning(),     "NeuroSkill™ – Scanning…"),
                "bt_off"       => (icon_bt_off(),       "NeuroSkill™ – Bluetooth Off"),
                _              => (icon_disconnected(), "NeuroSkill™ – Disconnected"),
            };
            let _ = tray.set_icon(Some(icon));
            let _ = tray.set_tooltip(Some(tip));
            *last = icon_state;
        }
    }

    // ── Menu (only rebuild when content would actually differ) ────────────────
    //
    // `tray.set_menu()` replaces the native menu object, which on macOS/Windows
    // dismisses the menu if the user currently has it open. Skipping the call
    // when the fingerprint is unchanged prevents the "menu disappears on hover"
    // symptom that occurs when status events arrive while the user is reading
    // the open menu.
    let key = menu_key(&st, app);
    {
        let mut last = LAST_MENU_KEY.lock().unwrap_or_else(|p| p.into_inner());
        if *last != key {
            if let Ok(m) = build_menu(app, &st) {
                let _ = tray.set_menu(Some(m));
                *last = key;
            } else {
                eprintln!("[tray] menu rebuild failed; preserving previous native menu");
            }
        }
    }
}
