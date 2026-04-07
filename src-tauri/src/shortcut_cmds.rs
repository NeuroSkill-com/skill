// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Global keyboard shortcuts — registration and Tauri command get/set pairs.

use crate::MutexExt;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::history_cmds::open_history_window;
#[cfg(feature = "llm")]
use crate::llm::cmds::open_chat_window;
use crate::tray::refresh_tray;
use crate::window_cmds::{
    open_api_window, open_calibration_window_inner, open_focus_timer_window, open_help_window,
    open_label_window, open_search_window, open_settings_window,
};
use crate::AppStateExt;
use crate::{save_settings, AppState};

// ── Internal helpers ──────────────────────────────────────────────────────────

fn register_one<F>(app: &AppHandle, accel: &str, action: F) -> Result<(), String>
where
    F: Fn(AppHandle) + Send + Sync + 'static,
{
    if accel.is_empty() {
        return Ok(());
    }
    let shortcut: tauri_plugin_global_shortcut::Shortcut = accel
        .parse()
        .map_err(|e| format!("invalid shortcut '{accel}': {e}"))?;
    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_, _, event| {
            if event.state() == ShortcutState::Pressed {
                action(handle.clone());
            }
        })
        .map_err(|e| e.to_string())
}

/// Unregister every global shortcut and re-register all configured ones.
pub(crate) fn apply_all_shortcuts(app: &AppHandle) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;

    let (label, search, settings, calibration, help, history, api, theme, focus_timer) = {
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
            g.shortcuts.theme_shortcut.clone(),
            g.shortcuts.focus_timer_shortcut.clone(),
        )
    };

    if let Err(e) = register_one(app, &label, |a| {
        tauri::async_runtime::spawn(async move {
            let _ = open_label_window(a).await;
        });
    }) {
        eprintln!("[shortcut] label: {e}");
    }

    if let Err(e) = register_one(app, &search, |a| {
        tauri::async_runtime::spawn(async move {
            let _ = open_search_window(a).await;
        });
    }) {
        eprintln!("[shortcut] search: {e}");
    }

    if let Err(e) = register_one(app, &settings, |a| {
        tauri::async_runtime::spawn(async move {
            let _ = open_settings_window(a).await;
        });
    }) {
        eprintln!("[shortcut] settings: {e}");
    }

    if let Err(e) = register_one(app, &calibration, |a| {
        tauri::async_runtime::spawn(async move {
            let _ = open_calibration_window_inner(&a, None, false).await;
        });
    }) {
        eprintln!("[shortcut] calibration: {e}");
    }

    if let Err(e) = register_one(app, &help, |a| {
        tauri::async_runtime::spawn(async move {
            let _ = open_help_window(a).await;
        });
    }) {
        eprintln!("[shortcut] help: {e}");
    }

    if let Err(e) = register_one(app, &history, |a| {
        tauri::async_runtime::spawn(async move {
            let _ = open_history_window(a).await;
        });
    }) {
        eprintln!("[shortcut] history: {e}");
    }

    if let Err(e) = register_one(app, &api, |a| {
        tauri::async_runtime::spawn(async move {
            let _ = open_api_window(a).await;
        });
    }) {
        eprintln!("[shortcut] api: {e}");
    }

    if let Err(e) = register_one(app, &theme, |a| {
        let _ = a.emit("toggle-theme", ());
    }) {
        eprintln!("[shortcut] theme: {e}");
    }

    if let Err(e) = register_one(app, &focus_timer, |a| {
        tauri::async_runtime::spawn(async move {
            let _ = open_focus_timer_window(a).await;
        });
    }) {
        eprintln!("[shortcut] focus_timer: {e}");
    }

    #[cfg(feature = "llm")]
    {
        let chat = {
            let r = app.app_state();
            let s = r.lock_or_recover().shortcuts.chat_shortcut.clone();
            s
        };
        if let Err(e) = register_one(app, &chat, |a| {
            tauri::async_runtime::spawn(async move {
                let _ = open_chat_window(a).await;
            });
        }) {
            eprintln!("[shortcut] chat: {e}");
        }
    }

    // "Open NeuroSkill™" — always CmdOrCtrl+Shift+O (not user-configurable)
    if let Err(e) = register_one(app, "CmdOrCtrl+Shift+O", |a| {
        if let Some(win) = a.get_webview_window("main") {
            let _ = win.show();
            let _ = win.set_focus();
        }
    }) {
        eprintln!("[shortcut] open_skill: {e}");
    }

    // Cmd+, (macOS "Preferences" convention) is intentionally NOT registered here as a
    // global hotkey.  macOS reserves Cmd+, for the native app-menu Preferences item and
    // the Carbon RegisterEventHotKey API rejects it with "RegisterEventHotKey failed for
    // Comma".  Instead, Cmd+, is handled as a window-level keyboard event in the Svelte
    // layout (+layout.svelte) so it works whenever the app window is focused — which is
    // the only context where the macOS convention applies.

    Ok(())
}

// ── Tauri commands ─────────────────────────────────────────────────────────────

macro_rules! shortcut_pair {
    ($get:ident, $set:ident, $field:ident, $name:literal) => {
        #[tauri::command]
        pub fn $get(state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
            state.lock_or_recover().shortcuts.$field.clone()
        }
        #[tauri::command]
        pub fn $set(shortcut: String, app: AppHandle) -> Result<(), String> {
            app.app_state().lock_or_recover().shortcuts.$field = shortcut;
            apply_all_shortcuts(&app)?;
            save_settings(&app);
            refresh_tray(&app);
            Ok(())
        }
    };
}

shortcut_pair!(
    get_label_shortcut,
    set_label_shortcut,
    label_shortcut,
    "label"
);
shortcut_pair!(
    get_search_shortcut,
    set_search_shortcut,
    search_shortcut,
    "search"
);
shortcut_pair!(
    get_settings_shortcut,
    set_settings_shortcut,
    settings_shortcut,
    "settings"
);
shortcut_pair!(
    get_calibration_shortcut,
    set_calibration_shortcut,
    calibration_shortcut,
    "calibration"
);
shortcut_pair!(get_help_shortcut, set_help_shortcut, help_shortcut, "help");
shortcut_pair!(
    get_history_shortcut,
    set_history_shortcut,
    history_shortcut,
    "history"
);
shortcut_pair!(get_api_shortcut, set_api_shortcut, api_shortcut, "api");
shortcut_pair!(
    get_theme_shortcut,
    set_theme_shortcut,
    theme_shortcut,
    "theme"
);
shortcut_pair!(
    get_focus_timer_shortcut,
    set_focus_timer_shortcut,
    focus_timer_shortcut,
    "focus_timer"
);

#[cfg(feature = "llm")]
shortcut_pair!(get_chat_shortcut, set_chat_shortcut, chat_shortcut, "chat");
