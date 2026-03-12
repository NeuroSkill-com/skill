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
static LAST_ICON_STATE: Mutex<String> = Mutex::new(String::new());

/// Fingerprint of the last menu content pushed to the OS tray.
/// Encodes every piece of data that `build_menu` uses so a rebuild is
/// triggered if and only if the visible menu would differ.
static LAST_MENU_KEY: Mutex<String> = Mutex::new(String::new());

#[cfg(feature = "llm")]
#[derive(Clone)]
struct TrayDownloadItem {
    filename:   String,
    progress:   f32,
    status_msg: Option<String>,
}

fn progress_bucket(progress: f32) -> u8 {
    ((progress.clamp(0.0, 1.0) * 20.0).round() as u8).min(20)
}

fn progress_percent(progress: f32) -> u8 {
    ((progress.clamp(0.0, 1.0) * 100.0).round() as u8).min(100)
}

#[cfg(feature = "llm")]
fn ellipsize_middle(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return "...".to_string();
    }

    let head = (max_chars - 3) / 2;
    let tail = max_chars - 3 - head;
    format!(
        "{}...{}",
        chars[..head].iter().collect::<String>(),
        chars[chars.len() - tail..].iter().collect::<String>(),
    )
}

#[cfg(feature = "llm")]
fn tray_download_items(app: &AppHandle) -> Vec<TrayDownloadItem> {
    use crate::llm::catalog::DownloadState;

    let downloads = {
        let r = app.state::<Mutex<AppState>>();
        let g = r.lock_or_recover();
        g.llm_downloads.clone()
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

    let llm_downloads = tray_download_fingerprint(app);

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

    format!("{state}|{name}|{batt_bucket}|{tgt}|{pairs}|{ls}|{ss}|{sets}|{cs}|{hs}|{hist}|{api}|{ts}|{ft}|{chat}|{llm_downloads}")
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

fn overlay_progress_bar(base: Image<'static>, progress: f32) -> Image<'static> {
    let width = base.width();
    let height = base.height();
    let mut rgba = base.rgba().to_vec();
    let progress = progress.clamp(0.0, 1.0);

    if width < 8 || height < 8 {
        return base;
    }

    let cx = (width as f32 - 1.0) * 0.5;
    let cy = (height as f32 - 1.0) * 0.5;
    let outer = (width.min(height) as f32 * 0.5) - 0.75;
    let thickness = ((width.min(height) as f32) * 0.24).clamp(2.0, 5.0);
    let inner = (outer - thickness).max(0.0);
    let start_angle = -std::f32::consts::FRAC_PI_2;
    let end_angle = start_angle + progress * std::f32::consts::TAU;

    fn blend(rgba: &mut [u8], idx: usize, color: [u8; 4]) {
        let alpha = color[3] as u16;
        let inv = 255u16.saturating_sub(alpha);
        rgba[idx] = (((rgba[idx] as u16 * inv) + (color[0] as u16 * alpha)) / 255) as u8;
        rgba[idx + 1] = (((rgba[idx + 1] as u16 * inv) + (color[1] as u16 * alpha)) / 255) as u8;
        rgba[idx + 2] = (((rgba[idx + 2] as u16 * inv) + (color[2] as u16 * alpha)) / 255) as u8;
        rgba[idx + 3] = rgba[idx + 3].max(color[3]);
    }

    fn angle_in_arc(angle: f32, start: f32, end: f32) -> bool {
        if end >= std::f32::consts::TAU + start {
            return true;
        }
        if end <= start {
            return false;
        }
        if angle >= start {
            angle <= end
        } else {
            angle + std::f32::consts::TAU <= end
        }
    }

    // Draw a high-contrast circular progress ring around the icon:
    // dark track + bright filled arc, clockwise from top.
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < inner || dist > outer {
                continue;
            }

            let mut angle = dy.atan2(dx);
            if angle < start_angle {
                angle += std::f32::consts::TAU;
            }
            let in_progress_arc = angle_in_arc(angle, start_angle, end_angle);

            let idx = ((y * width + x) * 4) as usize;

            // Ring track.
            blend(&mut rgba, idx, [12, 16, 22, 220]);

            // Bright progress arc.
            if in_progress_arc && progress > 0.0 {
                blend(&mut rgba, idx, [255, 255, 255, 245]);
            }

            // Outer halo for prominence.
            if dist > outer - 0.8 {
                if in_progress_arc && progress > 0.0 {
                    blend(&mut rgba, idx, [255, 255, 255, 210]);
                } else {
                    blend(&mut rgba, idx, [0, 0, 0, 190]);
                }
            }
        }
    }

    // Subtle dim of the unfinished interior sector for extra visibility.
    if progress < 1.0 {
        let interior_radius = (inner - 0.8).max(0.0);
        for y in 0..height {
            for x in 0..width {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > interior_radius {
                    continue;
                }
                let mut angle = dy.atan2(dx);
                if angle < start_angle {
                    angle += std::f32::consts::TAU;
                }
                if !angle_in_arc(angle, start_angle, end_angle) {
                    let idx = ((y * width + x) * 4) as usize;
                    rgba[idx] = ((rgba[idx] as u16 * 72) / 100) as u8;
                    rgba[idx + 1] = ((rgba[idx + 1] as u16 * 72) / 100) as u8;
                    rgba[idx + 2] = ((rgba[idx + 2] as u16 * 72) / 100) as u8;
                }
            }
        }
    }

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
