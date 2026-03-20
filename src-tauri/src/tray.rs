// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! System tray icon, tooltip and context menu.
//!
//! Pure helpers (progress-ring overlay, shortcut formatting, bucketing) live
//! in the `skill-tray` crate.  This module wires them to the Tauri runtime.

use std::sync::Mutex;
use crate::AppStateExt;
use crate::MutexExt;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Manager,
};

use crate::DeviceStatus;

// ── Re-exports from skill-tray ────────────────────────────────────────────────
pub use skill_tray::{
    progress_bucket, progress_percent,
    ellipsize_middle, with_shortcut,
};

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
static LAST_ICON_STATE: Mutex<String> = Mutex::new(String::new());

/// Timestamp (ms) of the last actual menu rebuild.  Used to debounce rapid
/// `refresh_tray` calls — if a rebuild happened less than `MENU_REBUILD_MIN_MS`
/// ago, the menu update is deferred to a short async timer instead of running
/// synchronously.
static LAST_MENU_REBUILD_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

use skill_tray::MENU_REBUILD_MIN_MS;

/// Fingerprint of the last menu content pushed to the OS tray.
/// Encodes every piece of data that `build_menu` uses so a rebuild is
/// triggered if and only if the visible menu would differ.
static LAST_MENU_KEY: Mutex<String> = Mutex::new(String::new());

/// Fingerprint of the last structural key — when only the status key changes,
/// we update existing menu items in-place instead of rebuilding.
static LAST_STRUCTURE_KEY: Mutex<String> = Mutex::new(String::new());

/// Cached reference to the current tray `Menu` so we can call `menu.get(id)`
/// for in-place item updates without `TrayIcon::menu()` (which doesn't exist).
static CURRENT_MENU: Mutex<Option<Menu<tauri::Wry>>> = Mutex::new(None);

#[cfg(feature = "llm")]
#[derive(Clone)]
struct TrayDownloadItem {
    filename:   String,
    progress:   f32,
    status_msg: Option<String>,
}

#[cfg(feature = "llm")]
fn tray_download_items(app: &AppHandle) -> Vec<TrayDownloadItem> {
    use crate::llm::catalog::DownloadState;

    let downloads = {
        let r = app.app_state();
        let g = r.lock_or_recover();
        { let __a = g.llm.clone(); let __r = __a.lock_or_recover().downloads.clone(); __r }
    };

    let mut items = downloads
        .into_iter()
        .filter_map(|(filename, prog_arc)| {
            let prog = prog_arc.lock().ok()?;
            if prog.state != DownloadState::Downloading {
                return None;
            }
            Some(TrayDownloadItem {
                filename,
                progress: prog.progress.clamp(0.0, 1.0),
                status_msg: prog.status_msg.clone(),
            })
        })
        .collect::<Vec<_>>();
    items.sort_unstable_by(|a, b| a.filename.cmp(&b.filename));
    items
}

#[cfg(feature = "llm")]
fn tray_download_fingerprint(app: &AppHandle) -> String {
    let items = tray_download_items(app);
    items
        .into_iter()
        .map(|item| format!(
            "{}:{}:{}",
            item.filename,
            progress_bucket(item.progress),
            item.status_msg.unwrap_or_default()
        ))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(feature = "llm")]
fn tray_download_icon_progress(app: &AppHandle) -> Option<(usize, f32)> {
    let items = tray_download_items(app);
    if items.is_empty() {
        return None;
    }

    let avg = items.iter().map(|item| item.progress).sum::<f32>() / items.len() as f32;
    Some((items.len(), avg))
}

#[cfg(not(feature = "llm"))]
fn tray_download_fingerprint(_app: &AppHandle) -> String {
    String::new()
}

#[cfg(not(feature = "llm"))]
fn tray_download_icon_progress(_app: &AppHandle) -> Option<(usize, f32)> {
    None
}

/// Structural key — things that change the number / identity of menu items.
/// When this changes, the entire menu must be rebuilt via `set_menu()`.
fn structure_key(st: &DeviceStatus, app: &AppHandle) -> String {
    let r = app.app_state();
    let g = r.lock_or_recover();
    let ls   = g.shortcuts.label_shortcut.clone();
    let ss   = g.shortcuts.search_shortcut.clone();
    let sets = g.shortcuts.settings_shortcut.clone();
    let cs   = g.shortcuts.calibration_shortcut.clone();
    let hs   = g.shortcuts.help_shortcut.clone();
    let hist = g.shortcuts.history_shortcut.clone();
    let api  = g.shortcuts.api_shortcut.clone();
    let ts   = g.shortcuts.theme_shortcut.clone();
    let ft   = g.shortcuts.focus_timer_shortcut.clone();
    #[cfg(feature = "llm")]
    let chat = g.shortcuts.chat_shortcut.clone();
    #[cfg(not(feature = "llm"))]
    let chat = String::new();
    drop(g);

    let llm_downloads = tray_download_fingerprint(app);

    let mut pair_parts = st.paired_devices
        .iter()
        .map(|d| format!("{}:{}", d.id, d.name))
        .collect::<Vec<_>>();
    pair_parts.sort_unstable();
    let pairs = pair_parts.join(",");

    // Structural: the BT state determines which action items are shown
    // (disconnect vs cancel vs connect-to-X vs scan).
    let state = st.state.as_str();

    format!("{state}|{pairs}|{ls}|{ss}|{sets}|{cs}|{hs}|{hist}|{api}|{ts}|{ft}|{chat}|{llm_downloads}")
}

/// Status key — things that only change text/enabled state of existing items.
/// When this changes (but structure_key doesn't), we update items in-place
/// via `set_text()` / `set_enabled()` — no `set_menu()` call needed.
fn status_key(st: &DeviceStatus) -> String {
    let batt_bucket = if st.battery <= 0.0 {
        0u32
    } else {
        (((st.battery as u32) / 10) * 10).min(100)
    };
    let name = st.device_name.as_deref().unwrap_or("");
    let tgt  = if st.state == "scanning" {
        st.target_name.as_deref().unwrap_or("")
    } else {
        ""
    };
    let is_streaming = st.state == "connected";
    format!("{name}|{batt_bucket}|{tgt}|{is_streaming}")
}

/// Combined key for full dedup (used by LAST_MENU_KEY to track whether
/// *anything* changed at all).
fn menu_key(st: &DeviceStatus, app: &AppHandle) -> String {
    format!("{}|{}", structure_key(st, app), status_key(st))
}

// shortcut_suffix, with_shortcut — re-exported from skill_tray above.

// ── Embedded icons ────────────────────────────────────────────────────────────

const ICON_CONNECTED:    &[u8] = include_bytes!("../icons/tray-connected.png");
const ICON_DISCONNECTED: &[u8] = include_bytes!("../icons/tray-disconnected.png");
const ICON_SCANNING:     &[u8] = include_bytes!("../icons/tray-scanning.png");
const ICON_BT_OFF:       &[u8] = include_bytes!("../icons/tray-bt-off.png");

fn icon_connected()             -> Image<'static> { Image::from_bytes(ICON_CONNECTED).expect("embedded tray icon") }
pub(crate) fn icon_disconnected() -> Image<'static> { Image::from_bytes(ICON_DISCONNECTED).expect("embedded tray icon") }
fn icon_scanning()              -> Image<'static> { Image::from_bytes(ICON_SCANNING).expect("embedded tray icon") }
fn icon_bt_off()                -> Image<'static> { Image::from_bytes(ICON_BT_OFF).expect("embedded tray icon") }

fn overlay_progress_bar(base: Image<'static>, progress: f32) -> Image<'static> {
    let width = base.width();
    let height = base.height();
    let rgba = skill_tray::overlay_progress_bar(base.rgba(), width, height, progress);
    Image::new_owned(rgba, width, height)
}

fn icon_with_progress(icon_state: &str, progress: Option<f32>) -> Image<'static> {
    let base = match icon_state {
        "connected" => icon_connected(),
        "scanning" => icon_scanning(),
        "bt_off" => icon_bt_off(),
        _ => icon_disconnected(),
    };

    match progress {
        Some(value) => overlay_progress_bar(base, value),
        None => base,
    }
}

// ── Menu builder ──────────────────────────────────────────────────────────────

pub(crate) fn build_menu(app: &AppHandle, st: &DeviceStatus) -> tauri::Result<Menu<tauri::Wry>> {
    let (label_shortcut, search_shortcut, settings_shortcut, calibration_shortcut,
         help_shortcut, history_shortcut, api_shortcut, focus_timer_shortcut) = {
        let r = app.app_state();
        let g = r.lock_or_recover();
        (
            g.shortcuts.label_shortcut.clone(),
            g.shortcuts.search_shortcut.clone(),
            g.shortcuts.settings_shortcut.clone(),
            g.shortcuts.calibration_shortcut.clone(),
            g.shortcuts.help_shortcut.clone(),
            g.shortcuts.history_shortcut.clone(),
            g.shortcuts.api_shortcut.clone(),
            g.shortcuts.focus_timer_shortcut.clone(),
        )
    };
    #[cfg(feature = "llm")]
    let chat_shortcut = {
        let r = app.app_state();
        let s = r.lock_or_recover().shortcuts.chat_shortcut.clone();
        s
    };

    let menu = Menu::new(app)?;
    let open_skill_label = with_shortcut("Open NeuroSkill™", "CmdOrCtrl+Shift+O");
    menu.append(&MenuItem::with_id(app, "open_skill", &open_skill_label, true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // ── Status info (always present — updated in-place by update_status_items) ──
    let info_text = match st.state.as_str() {
        "connected" => format!("● {}", st.device_name.as_deref().unwrap_or("BCI device")),
        "scanning"  => match &st.target_name {
            Some(n) => format!("Searching for {n}…"),
            None    => "Scanning for BCI device…".into(),
        },
        "bt_off"    => "⚠ Bluetooth Unavailable".into(),
        _           => "○ Disconnected".into(),
    };
    menu.append(&MenuItem::with_id(app, "info", &info_text, false, None::<&str>)?)?;

    // Battery line (always present; hidden when no battery data)
    let has_batt = st.state == "connected" && st.battery > 0.0;
    let batt_text = if has_batt { format!("🔋 {:.0}%", st.battery) } else { String::new() };
    let batt_item = MenuItem::with_id(app, "battery_info", &batt_text, false, None::<&str>)?;
    if !has_batt { let _ = batt_item.set_enabled(false); }
    menu.append(&batt_item)?;

    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // ── Action items (state-dependent — trigger full rebuild when they change) ──
    match st.state.as_str() {
        "connected" => {
            menu.append(&MenuItem::with_id(app, "disconnect", "Disconnect", true, None::<&str>)?)?;
        }
        "scanning" => {
            menu.append(&MenuItem::with_id(app, "cancel", "Cancel", true, None::<&str>)?)?;
        }
        "bt_off" => {
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

    #[cfg(feature = "llm")]
    {
        let downloads = tray_download_items(app);
        if !downloads.is_empty() {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
            let avg = downloads.iter().map(|item| item.progress).sum::<f32>() / downloads.len() as f32;
            let heading = if downloads.len() == 1 {
                format!("⬇ LLM download {}", progress_percent(avg))
            } else {
                format!("⬇ {} LLM downloads {}", downloads.len(), progress_percent(avg))
            };
            menu.append(&MenuItem::with_id(app, "llm_download_info", &heading, false, None::<&str>)?)?;

            for (index, item) in downloads.iter().take(3).enumerate() {
                let label = format!(
                    "{} {}",
                    ellipsize_middle(&item.filename, 38),
                    progress_percent(item.progress)
                );
                menu.append(&MenuItem::with_id(app, format!("llm_download_item_{index}"), &label, false, None::<&str>)?)?;
                if let Some(status) = item.status_msg.as_deref() {
                    if !status.trim().is_empty() {
                        menu.append(&MenuItem::with_id(
                            app,
                            format!("llm_download_status_{index}"),
                            ellipsize_middle(status, 46),
                            false,
                            None::<&str>,
                        )?)?;
                    }
                }
            }

            if downloads.len() > 3 {
                menu.append(&MenuItem::with_id(
                    app,
                    "llm_download_more",
                    format!("+{} more download{}", downloads.len() - 3, if downloads.len() == 4 { "" } else { "s" }),
                    false,
                    None::<&str>,
                )?)?;
            }
        }
    }

    let is_streaming = st.state == "connected";
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "focus_timer", with_shortcut("Focus Timer…", &focus_timer_shortcut), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "calibrate",   with_shortcut("Calibrate…", &calibration_shortcut), is_streaming, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "search",      with_shortcut("Search…", &search_shortcut), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "label",       with_shortcut("Add Label…", &label_shortcut), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "history",     with_shortcut("History…", &history_shortcut), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "compare",     with_shortcut("Compare…", "CmdOrCtrl+Shift+M"), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "settings",    with_shortcut("Settings…", &settings_shortcut), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "help",        with_shortcut("Help…", &help_shortcut), true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "api",         with_shortcut("API Status…", &api_shortcut), true, None::<&str>)?)?;
    #[cfg(feature = "llm")]
    {
        menu.append(&MenuItem::with_id(app, "downloads", "Downloads…", true, None::<&str>)?)?;
        menu.append(&MenuItem::with_id(app, "chat", with_shortcut("Chat…", &chat_shortcut), true, None::<&str>)?)?;
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
    let s_ref = app.app_state();
    let st = { let g = s_ref.lock_or_recover(); g.status.clone() };

    let Some(tray) = app.tray_by_id("main") else { return };

    // ── Icon + tooltip (only update when the state bucket changes) ────────────
    let icon_state: &'static str = match st.state.as_str() {
        "connected" => "connected",
        "scanning"  => "scanning",
        "bt_off"    => "bt_off",
        _           => "disconnected",
    };
    let download_progress = tray_download_icon_progress(app);
    let icon_key = match download_progress {
        Some((count, progress)) => format!("{icon_state}|{}|{count}", progress_bucket(progress)),
        None => icon_state.to_string(),
    };
    {
        let mut last = LAST_ICON_STATE.lock().unwrap_or_else(|p| p.into_inner());
        if *last != icon_key {
            let base_tip = match icon_state {
                "connected"    => "NeuroSkill™ – Connected",
                "scanning"     => "NeuroSkill™ – Scanning…",
                "bt_off"       => "NeuroSkill™ – Bluetooth Off",
                _              => "NeuroSkill™ – Disconnected",
            };
            let icon = icon_with_progress(icon_state, download_progress.map(|(_, progress)| progress));
            let tip = match download_progress {
                Some((1, progress)) => format!("{base_tip} • LLM download {}", progress_percent(progress)),
                Some((count, progress)) => format!("{base_tip} • {count} LLM downloads {}", progress_percent(progress)),
                None => base_tip.to_string(),
            };
            let _ = tray.set_icon(Some(icon));
            let _ = tray.set_tooltip(Some(&tip));
            *last = icon_key;
        }
    }

    // ── Menu ───────────────────────────────────────────────────────────────────
    //
    // Strategy: split updates into *structural* (add/remove items → set_menu)
    // and *status-only* (change text/enabled on existing items → set_text).
    // Status-only updates are O(1) and never dismiss an open menu.

    let full_key = menu_key(&st, app);
    {
        let last = LAST_MENU_KEY.lock().unwrap_or_else(|p| p.into_inner());
        if *last == full_key {
            return; // nothing changed at all
        }
    }

    let s_key = structure_key(&st, app);
    let structure_changed = {
        let last = LAST_STRUCTURE_KEY.lock().unwrap_or_else(|p| p.into_inner());
        *last != s_key
    };

    if structure_changed {
        // Full rebuild — menu item count or identity changed.
        // First do a fast in-place status patch so the user sees the new
        // status text instantly, then rebuild the full menu asynchronously.
        if let Some(ref menu) = *CURRENT_MENU.lock().unwrap_or_else(|p| p.into_inner()) {
            update_status_items(menu, &st);
        }
        // Debounce rapid structural rebuilds.
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let prev = LAST_MENU_REBUILD_MS.load(std::sync::atomic::Ordering::Relaxed);
        if now_ms.saturating_sub(prev) < MENU_REBUILD_MIN_MS {
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(MENU_REBUILD_MIN_MS)).await;
                refresh_tray(&app2);
            });
            return;
        }
        if let Ok(m) = build_menu(app, &st) {
            let _ = tray.set_menu(Some(m.clone()));
            *CURRENT_MENU.lock().unwrap_or_else(|p| p.into_inner()) = Some(m);
            *LAST_STRUCTURE_KEY.lock().unwrap_or_else(|p| p.into_inner()) = s_key;
            *LAST_MENU_KEY.lock().unwrap_or_else(|p| p.into_inner()) = full_key;
            LAST_MENU_REBUILD_MS.store(now_ms, std::sync::atomic::Ordering::Relaxed);
        } else {
            eprintln!("[tray] menu rebuild failed; preserving previous native menu");
        }
    } else {
        // Status-only update — patch existing items in-place (no set_menu).
        if let Some(ref menu) = *CURRENT_MENU.lock().unwrap_or_else(|p| p.into_inner()) {
            update_status_items(menu, &st);
        }
        *LAST_MENU_KEY.lock().unwrap_or_else(|p| p.into_inner()) = full_key;
    }
}

/// Patch text/enabled on existing menu items without replacing the menu.
/// This avoids the expensive native menu teardown/rebuild cycle for
/// state transitions like connected↔disconnected that don't change
/// which items exist.
fn update_status_items(menu: &Menu<tauri::Wry>, st: &DeviceStatus) {
    // Status info line
    if let Some(item) = menu.get("info").and_then(|k| k.as_menuitem().cloned()) {
        let text = match st.state.as_str() {
            "connected" => format!("● {}", st.device_name.as_deref().unwrap_or("BCI device")),
            "scanning"  => match &st.target_name {
                Some(n) => format!("Searching for {n}…"),
                None    => "Scanning for BCI device…".into(),
            },
            "bt_off"    => "⚠ Bluetooth Unavailable".into(),
            _           => "○ Disconnected".into(),
        };
        let _ = item.set_text(text);
    }

    // Battery line
    if let Some(item) = menu.get("battery_info").and_then(|k| k.as_menuitem().cloned()) {
        if st.battery > 0.0 {
            let _ = item.set_text(format!("🔋 {:.0}%", st.battery));
            let _ = item.set_enabled(true);  // make visible
        } else {
            let _ = item.set_text("");
            let _ = item.set_enabled(false); // hide
        }
    }

    // Calibrate is only enabled while streaming
    let is_streaming = st.state == "connected";
    if let Some(item) = menu.get("calibrate").and_then(|k| k.as_menuitem().cloned()) {
        let _ = item.set_enabled(is_streaming);
    }
}
