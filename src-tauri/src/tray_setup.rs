// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! System-tray icon construction and menu-event routing.

use tauri::{tray::TrayIconBuilder, AppHandle, Manager};

use crate::constants;
use crate::platform::linux_fix_decorations;
use crate::settings_cmds::device_cmds::{cancel_retry, forget_device, retry_connect};
use crate::tray::icon_disconnected;
use crate::window_cmds::open_calibration_window_inner;

/// Main-window recovery helper.
pub(crate) fn show_and_recover_main(app: &AppHandle) {
    let win = if let Some(win) = app.get_webview_window("main") {
        win
    } else {
        match tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("".into()))
            .title(constants::APP_DISPLAY_NAME)
            .decorations(false)
            .transparent(true)
            .build()
        {
            Ok(win) => win,
            Err(_) => return,
        }
    };
    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
    linux_fix_decorations(&win);
    if win
        .eval("window.__skill_loaded||(window.location.reload(),false)")
        .is_err()
    {
        if let Ok(url) = "tauri://localhost".parse() {
            let _ = win.navigate(url);
        }
    }
}

/// Build the system-tray icon and wire up all menu-event handlers.
pub(crate) fn build_tray(
    app: &mut tauri::App,
    init_menu: &tauri::menu::Menu<tauri::Wry>,
) -> anyhow::Result<()> {
    TrayIconBuilder::with_id("main")
        .icon(icon_disconnected())
        .tooltip("NeuroSkill™ – Disconnected")
        .menu(init_menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            if id == "open_skill" {
                show_and_recover_main(app);
            } else if id == "disconnect" || id == "cancel" {
                cancel_retry(app.clone());
            } else if id == "scan" || id == "retry" {
                retry_connect(app.clone());
            } else if id == "open_bt" {
                crate::window_cmds::open_bt_settings();
            } else if id == "calibrate" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_calibration_window_inner(&a, None, false).await;
                });
            } else if id == "search" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::window_cmds::open_search_window(a).await;
                });
            } else if id == "label" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::window_cmds::open_label_window(a).await;
                });
            } else if id == "history" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::history_cmds::open_history_window(a).await;
                });
            } else if id == "compare" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::session_analysis::open_compare_window(a).await;
                });
            } else if id == "settings" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::window_cmds::open_settings_window(a).await;
                });
            } else if id == "help" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::window_cmds::open_help_window(a).await;
                });
            } else if id == "api" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::window_cmds::open_api_window(a).await;
                });
            } else if id == "virtual_devices" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::window_cmds::open_virtual_devices_window(a).await;
                });
            } else if id == "chat" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::llm::cmds::open_chat_window(a, None).await;
                });
            } else if id == "downloads" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::llm::cmds::open_downloads_window(a).await;
                });
            } else if id == "focus_timer" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::window_cmds::open_focus_timer_window(a).await;
                });
            } else if id == "show_logs" {
                crate::window_cmds::open_latest_log();
            } else if id == "check_update" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::window_cmds::open_updates_window(a).await;
                });
            } else if id == "quit" {
                crate::confirm_and_quit(app.app_handle().clone());
            } else if let Some(dev_id) = id.strip_prefix("connect:") {
                let _ = crate::settings_cmds::device_cmds::set_preferred_device(
                    dev_id.to_owned(),
                    app.clone(),
                );
                retry_connect(app.clone());
            } else if let Some(dev_id) = id.strip_prefix("forget:") {
                let dev_id = dev_id.to_owned();
                forget_device(dev_id, app.clone());
            }
        })
        .on_tray_icon_event(|_tray, _event| {})
        .build(app)?;

    Ok(())
}
