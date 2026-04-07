// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//
// lib.rs — crate root.
//
// Responsibilities:
//   • Module declarations and public re-exports
//   • Core shared types: AppState, DeviceStatus, data-packet structs, handles
//   • Shared helpers: settings I/O, device upsert, emit helpers, toast, retry
//   • Session lifecycle: start_session / cancel_session / go_disconnected
//   • App entry-point: run()

// ── Existing modules ──────────────────────────────────────────────────────────

mod constants;

#[macro_use]
mod skill_log;

/// Convenience wrapper around [`skill_log!`] for code that holds an
/// `&AppHandle` but not a direct reference to the logger.
///
/// Requires `Arc<SkillLogger>` to be registered as Tauri managed state
/// (done once in `run()` → `setup`).
///
/// ```ignore
/// app_log!(app, "devices", "connected: {name}");
/// ```
macro_rules! app_log {
    ($app:expr, $tag:literal, $($arg:tt)*) => {{
        use tauri::Manager as _;
        let _lg = $app.state::<std::sync::Arc<$crate::skill_log::SkillLogger>>();
        skill_log!(_lg, $tag, $($arg)*);
    }};
}

mod lifecycle;

mod quit;
pub(crate) use quit::confirm_and_quit;

mod job_queue;

mod label_index;
mod ws_server;

#[allow(dead_code, unused_imports)]
/// OpenAI-compatible LLM inference server — same port as WebSocket API.
/// Enabled by the `llm` Cargo feature; no-op when the feature is absent.
#[cfg(feature = "llm")]
mod llm;

use ws_server::WsBroadcaster;

// ── New extracted modules ─────────────────────────────────────────────────────

/// CSV recording (CsvState, path helpers, session-metadata sidecar).
mod session_csv;

/// Generic device session runner (replaces per-device session modules).
/// Per-device scan / connect factories → `Box<dyn DeviceAdapter>`.
mod session_connect;

/// Session history listing and streaming Tauri commands.
mod history_cmds;
use history_cmds::{
    list_session_days, list_sessions_for_day, open_history_window, stream_sessions,
};

/// Session metrics, time-series, sleep staging, UMAP and compare commands.
mod session_analysis;
use session_analysis::{
    get_day_metrics_batch, open_compare_window, open_compare_window_with_sessions,
};

// ── Existing extracted modules ────────────────────────────────────────────────

mod autostart;

mod tts;

pub(crate) use tts::{
    init_espeak_bundled_data_path, init_neutts_samples_dir, neutts_apply_config, tts_shutdown,
};
use tts::{
    tts_get_voice, tts_init, tts_list_neutts_voices, tts_list_voices, tts_set_voice, tts_speak,
    tts_unload,
};

mod settings;
pub(crate) use settings::{
    default_skill_dir, load_settings, new_profile_id, CalibrationConfig, CalibrationProfile,
};

mod tray;
pub(crate) use tray::{build_menu, icon_disconnected};

// ── Linux decoration workaround (tauri-apps/tauri#11856) ─────────────────────
// On Linux (Wayland + GNOME/Mutter/KWin), window decorations (close /
// minimize / maximize buttons) become completely unresponsive when a window
// is created with `visible(false)` and later shown, or after any hide→show
// cycle.  Briefly toggling fullscreen after `show()` forces the compositor
// to re-evaluate the decoration state.  The toggle is near-instantaneous
// and visually imperceptible.  Must be called *after* `win.show()`.
#[cfg(target_os = "linux")]
pub(crate) fn linux_fix_decorations(win: &tauri::WebviewWindow) {
    eprintln!(
        "[linux] applying decoration fix (fullscreen toggle) for {}",
        win.label()
    );
    let _ = win.set_fullscreen(true);
    let _ = win.set_fullscreen(false);
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn linux_fix_decorations(_win: &tauri::WebviewWindow) {}

#[cfg(target_os = "linux")]
fn linux_has_appindicator_runtime() -> bool {
    let candidates = [
        "libayatana-appindicator3.so.1",
        "libappindicator3.so.1",
        "libayatana-appindicator3.so",
        "libappindicator3.so",
    ];

    for name in candidates {
        let Ok(c_name) = std::ffi::CString::new(name) else {
            continue;
        };
        // SAFETY: `c_name` is a valid NUL-terminated C string that outlives the call.
        // `dlopen` with RTLD_LAZY|RTLD_LOCAL is safe for probing library availability.
        let handle = unsafe { libc::dlopen(c_name.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
        if !handle.is_null() {
            // SAFETY: `handle` is a non-null pointer returned by `dlopen` above.
            let _ = unsafe { libc::dlclose(handle) };
            return true;
        }
    }

    false
}

mod shortcut_cmds;
pub(crate) use shortcut_cmds::apply_all_shortcuts;
use shortcut_cmds::{
    get_api_shortcut, get_calibration_shortcut, get_focus_timer_shortcut, get_help_shortcut,
    get_history_shortcut, get_label_shortcut, get_search_shortcut, get_settings_shortcut,
    get_theme_shortcut, set_api_shortcut, set_calibration_shortcut, set_focus_timer_shortcut,
    set_help_shortcut, set_history_shortcut, set_label_shortcut, set_search_shortcut,
    set_settings_shortcut, set_theme_shortcut,
};
#[cfg(feature = "llm")]
use shortcut_cmds::{get_chat_shortcut, set_chat_shortcut};

mod active_window;

mod about;
use about::{get_about_info, open_about_window};

mod calibration_service;
mod window_cmds;
pub(crate) use window_cmds::open_calibration_window_inner;
use window_cmds::{
    autosize_main_window, check_accessibility_permission, check_bluetooth_power,
    check_screen_recording_permission, close_calibration_window, close_label_window,
    complete_onboarding, create_calibration_profile, delete_calibration_profile, dismiss_whats_new,
    emit_calibration_event, get_active_calibration, get_app_name, get_app_version,
    get_calendar_events, get_calendar_permission_status, get_calibration_config,
    get_calibration_profile, get_data_dir, get_location_permission_status, get_onboarding_complete,
    get_onboarding_model_download_order, get_whats_new_seen_version, is_session_live,
    list_calibration_profiles, open_accessibility_settings, open_and_start_calibration,
    open_api_window, open_bt_settings, open_calendar_settings, open_calibration_window,
    open_focus_settings, open_focus_timer_window, open_help_window, open_input_monitoring_settings,
    open_label_window, open_label_window_at, open_labels_window, open_latest_log,
    open_location_settings, open_model_tab, open_notifications_settings, open_onboarding_window,
    open_screen_recording_settings, open_search_window, open_session_window, open_settings_window,
    open_skill_dir, open_updates_window, open_whats_new_window, quit_app,
    record_calibration_completed, request_calendar_permission, request_location_permission,
    set_active_calibration, set_calibration_config, set_data_dir, set_update_ready,
    show_main_window, update_calibration_profile,
};

mod label_cmds;
use label_cmds::{get_queue_stats, rebuild_label_index, search_labels_by_eeg};

mod daemon_cmds;
use daemon_cmds::{
    cancel_session,
    cancel_weights_download,
    daemon_install_service,
    daemon_uninstall_service,
    estimate_reembed,
    get_daemon_bootstrap,
    get_daemon_service_status,
    get_daemon_status,
    get_daemon_token_path,
    get_eeg_model_config,
    get_eeg_model_status,
    // EXG model daemon proxies
    get_exg_catalog,
    // LSL daemon proxies
    lsl_discover,
    lsl_get_config,
    lsl_get_idle_timeout,
    lsl_iroh_start,
    lsl_iroh_status,
    lsl_iroh_stop,
    lsl_pair_stream,
    lsl_set_auto_connect,
    lsl_set_idle_timeout,
    lsl_unpair_stream,
    lsl_virtual_source_running,
    lsl_virtual_source_start,
    lsl_virtual_source_stop,
    set_eeg_model_config,
    start_daemon_dev,
    start_session,
    switch_session,
    trigger_reembed,
    trigger_weights_download,
};

mod settings_cmds;
use settings_cmds::*;

// LLM catalog commands (feature-gated)
#[cfg(feature = "llm")]
use llm::cmds::{open_chat_window, open_downloads_window};

// ── Imports ───────────────────────────────────────────────────────────────────

use std::{sync::Mutex, time::Duration};

use tauri::{tray::TrayIconBuilder, AppHandle, Emitter, Manager};

// ── Core types (re-exported from state.rs) ────────────────────────────────────

mod state;
pub(crate) use state::*;

// ── Shared helpers (re-exported from helpers.rs) ──────────────────────────────

mod helpers;
pub(crate) use helpers::{
    apply_daemon_status, emit_devices, emit_status, emit_status_from_daemon, mutate_and_save,
    save_settings, save_settings_now, send_toast, unix_secs, yyyymmdd_utc, AppStateExt, ToastLevel,
};

// ── Mutex poison recovery ─────────────────────────────────────────────────────

// Re-export MutexExt from skill-data so `crate::MutexExt` keeps working
// everywhere in src-tauri. The canonical implementation lives in
// crates/skill-data/src/util.rs.
pub(crate) use skill_data::util::MutexExt;

// ── One-time migration: fastembed_cache → HuggingFace hub cache ──────────────

/// Move model directories from `~/.skill/fastembed_cache/` into the shared
/// HuggingFace hub cache (`~/.cache/huggingface/hub/`).  Idempotent —
/// skips entries that already exist in the destination.
fn migrate_fastembed_cache(skill_dir: &std::path::Path) {
    let src = skill_dir.join("fastembed_cache");
    if !src.is_dir() {
        return;
    }
    let dst = skill_data::util::hf_cache_root();
    if let Err(e) = std::fs::create_dir_all(&dst) {
        eprintln!("[migrate] cannot create HF cache dir: {e}");
        return;
    }
    let Ok(entries) = std::fs::read_dir(&src) else {
        return;
    };
    let mut moved = 0usize;
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let target = dst.join(&name);
        if target.exists() {
            // Already migrated or downloaded separately — remove the old copy.
            let _ = std::fs::remove_dir_all(entry.path());
            moved += 1;
            continue;
        }
        if let Err(e) = std::fs::rename(entry.path(), &target) {
            // rename fails across mount points; fall back to leaving in place
            eprintln!("[migrate] cannot move {}: {e}", name.to_string_lossy());
        } else {
            moved += 1;
        }
    }
    // Remove the now-empty fastembed_cache dir (best-effort).
    if moved > 0 {
        let _ = std::fs::remove_dir(&src);
        eprintln!(
            "[migrate] moved {moved} model(s) from fastembed_cache → {}",
            dst.display()
        );
    }
}

// ── Quit confirmation dialog ──────────────────────────────────────────────────

static EXIT_SHUTDOWN_STARTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn run_blocking_exit_shutdown(app: &tauri::AppHandle) {
    if EXIT_SHUTDOWN_STARTED.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }

    // Flush any pending debounced settings to disk before exit.
    save_settings_now(app);

    #[cfg(feature = "llm")]
    {
        let _ = crate::daemon_cmds::llm_server_stop();
    }

    tts_shutdown();
}

// ── External renderer for macOS headless webview ──────────────────────────────

#[cfg(target_os = "macos")]
mod external_renderer;

/// Delegate to the extracted module.
#[cfg(target_os = "macos")]
fn setup_external_renderer(app: &mut tauri::App) {
    external_renderer::setup(app);
}

// ── App setup (extracted to reduce `run()` stack frame) ───────────────────────

/// Extracted from the `.setup()` closure so LLVM does not merge its locals
/// into the already-huge `run()` stack frame produced by `generate_handler!`.
#[inline(never)]
fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // On macOS, the headless browser (tao) cannot create a second event loop
    // because Tauri already owns the main thread.  Disable the standalone
    // browser and register an external renderer that reuses Tauri's webview.
    #[cfg(target_os = "macos")]
    {
        skill_headless::Browser::set_unavailable();
        setup_external_renderer(app);
    }

    {
        use tauri::Manager;
        let resource_dir = app
            .path()
            .resource_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("resources"));
        init_espeak_bundled_data_path(&resource_dir);
        let samples_dir = resource_dir.join("neutts-samples");
        init_neutts_samples_dir(samples_dir);
    }

    // ── Linux: fix main-window property overrides ─────────────────────
    #[cfg(target_os = "linux")]
    {
        use tauri::Manager;
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.set_skip_taskbar(false);
            let _ = win.set_resizable(true);
            let _ = win.set_closable(true);
            let _ = win.set_minimizable(true);
            let min_size = tauri::LogicalSize::new(480.0_f64, 560.0_f64);
            let _ = win.set_min_size(Some(tauri::Size::Logical(min_size)));
            let _ = win.set_max_size(Option::<tauri::Size>::None);
        }
    }

    let skill_dir_for_iroh = app.app_state().lock_or_recover().skill_dir.clone();

    // ── LLM server ownership moved to daemon ───────────────────────────

    let broadcaster = ws_server::WsBroadcaster;

    #[cfg(feature = "llm")]
    {
        // LLM inference server ownership moved to daemon.
    }

    // ── Auto-start daemon if not already running ───────────────────────
    crate::daemon_cmds::ensure_daemon_running();

    let ws_port = crate::daemon_cmds::fetch_daemon_ws_port().unwrap_or(18444);

    // NAT-traversing P2P bridge — proxies iroh peers to the single API port.
    // The peer map lets the axum server identify which iroh client is on each
    // TCP connection so it can enforce per-command permissions.
    let iroh_auth = std::sync::Arc::new(std::sync::Mutex::new(skill_iroh::IrohAuthStore::open(
        &skill_dir_for_iroh,
    )));
    let iroh_runtime = std::sync::Arc::new(std::sync::Mutex::new(
        skill_iroh::IrohRuntimeState::default(),
    ));
    let iroh_peer_map = skill_iroh::new_peer_map();
    let (iroh_eeg_tx, iroh_eeg_rx) = skill_iroh::event_channel();
    let shared_device_tx: skill_iroh::SharedDeviceEventTx =
        std::sync::Arc::new(std::sync::Mutex::new(Some(iroh_eeg_tx)));
    skill_iroh::spawn(
        skill_dir_for_iroh.clone(),
        ws_port,
        iroh_auth.clone(),
        iroh_runtime.clone(),
        iroh_peer_map.clone(),
        shared_device_tx.clone(),
    );

    app.manage(iroh_auth);
    app.manage(iroh_runtime);
    app.manage(iroh_peer_map);
    app.manage(shared_device_tx);
    app.manage(std::sync::Arc::new(tokio::sync::Mutex::new(Some(
        iroh_eeg_rx,
    ))));
    app.manage(broadcaster);

    let (logger_arc, skill_dir) = {
        let r = app.app_state();
        let g = r.lock_or_recover();
        (g.logger.clone(), g.skill_dir.clone())
    };
    app.manage(logger_arc);

    // Route TTS and LLM log output through the unified SkillLogger.
    crate::tts::init_tts_logger(app.handle());
    crate::llm::init_llm_logger(app.handle());
    crate::llm::init_tool_logger(app.handle());

    load_and_apply_settings(app, &skill_dir);

    // ── Gather values from AppState in a single lock acquisition ─────
    // Avoids 4 separate lock/unlock cycles that were here previously.
    let (llm_autostart, llm_has_model, model_status, hf_repo) = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        let __llm_arc = s.llm.clone();
        let llm = __llm_arc.lock_or_recover();
        let autostart = llm.config.enabled && llm.config.autostart;
        let has_model = llm
            .config
            .model_path
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false);
        drop(llm);
        (
            autostart,
            has_model,
            s.embedding.model_status.clone(),
            s.embedding.model_config.hf_repo.clone(),
        )
    };

    // Auto-start the LLM server if configured and a model is available.
    if llm_autostart && llm_has_model {
        #[cfg(feature = "llm")]
        {
            let app_handle = app.handle().clone();
            // Small delay so the main window can render first.
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let state = app_handle.state::<Mutex<Box<AppState>>>();
                crate::llm::cmds::start_llm_server(app_handle.clone(), state).ok();
            });
        }
    }

    // Migrate fastembed_cache → HuggingFace hub cache (one-time, idempotent).
    migrate_fastembed_cache(&skill_dir);

    // Label HNSW indices are now owned by the daemon — no Tauri-side load.

    // ── Startup weights probe ─────────────────────────────────────────
    std::thread::Builder::new()
        .name("weights-probe".into())
        .spawn(move || {
            if let Some((w, _c)) = skill_exg::probe_hf_weights(&hf_repo) {
                let mut st = model_status.lock_or_recover();
                st.weights_found = true;
                st.weights_path = Some(w.display().to_string());
                eprintln!("[embedder] startup probe: weights found at {}", w.display());
            } else {
                eprintln!("[embedder] startup probe: weights not found in HuggingFace cache");
            }
        })
        .expect("[weights-probe] failed to spawn");

    if let Err(e) = apply_all_shortcuts(app.handle()) {
        eprintln!("[shortcut] failed to register shortcuts: {e}");
    }

    #[cfg(target_os = "macos")]
    {
        use tauri::menu::{MenuBuilder, MenuItem, PredefinedMenuItem, SubmenuBuilder};
        let app_submenu = SubmenuBuilder::new(app, constants::APP_DISPLAY_NAME)
            .item(&MenuItem::with_id(
                app,
                "about",
                format!("About {}", constants::APP_DISPLAY_NAME),
                true,
                None::<&str>,
            )?)
            .separator()
            .item(&PredefinedMenuItem::hide(app, None)?)
            .item(&PredefinedMenuItem::hide_others(app, None)?)
            .item(&PredefinedMenuItem::show_all(app, None)?)
            .separator()
            .item(&MenuItem::with_id(
                app,
                "macos_quit",
                format!("Quit {}", constants::APP_DISPLAY_NAME),
                true,
                Some("Cmd+Q"),
            )?)
            .build()?;
        let edit_submenu = SubmenuBuilder::new(app, "Edit")
            .item(&PredefinedMenuItem::undo(app, None)?)
            .item(&PredefinedMenuItem::redo(app, None)?)
            .separator()
            .item(&PredefinedMenuItem::cut(app, None)?)
            .item(&PredefinedMenuItem::copy(app, None)?)
            .item(&PredefinedMenuItem::paste(app, None)?)
            .item(&PredefinedMenuItem::select_all(app, None)?)
            .build()?;
        let window_submenu = SubmenuBuilder::new(app, "Window")
            .item(&PredefinedMenuItem::minimize(app, None)?)
            .item(&PredefinedMenuItem::maximize(app, None)?)
            .separator()
            .item(&PredefinedMenuItem::close_window(app, None)?)
            .build()?;
        let app_menu = MenuBuilder::new(app)
            .item(&app_submenu)
            .item(&edit_submenu)
            .item(&window_submenu)
            .build()?;
        app.set_menu(app_menu).ok();
    }

    app.on_menu_event(|app, event| {
        if event.id().as_ref() == "about" {
            let a = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = open_about_window(a).await;
            });
        } else if event.id().as_ref() == "macos_quit" {
            confirm_and_quit(app.clone());
        }
    });

    let init_status = {
        let r = app.app_state();
        let g = r.lock_or_recover();
        g.status.clone()
    };
    let init_menu = build_menu(app.handle(), &init_status)?;

    /// Main-window recovery helper.
    fn show_and_recover_main(app: &AppHandle) {
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

    #[cfg(target_os = "linux")]
    if !linux_has_appindicator_runtime() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "System tray is required but Linux appindicator runtime is missing. \
             Install libayatana-appindicator3 or libappindicator3.",
        )
        .into());
    }

    TrayIconBuilder::with_id("main")
        .icon(icon_disconnected())
        .tooltip("NeuroSkill™ – Disconnected")
        .menu(&init_menu)
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
                open_bt_settings();
            } else if id == "calibrate" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_calibration_window_inner(&a, None, false).await;
                });
            } else if id == "search" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_search_window(a).await;
                });
            } else if id == "label" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_label_window(a).await;
                });
            } else if id == "history" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_history_window(a).await;
                });
            } else if id == "compare" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_compare_window(a).await;
                });
            } else if id == "settings" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_settings_window(a).await;
                });
            } else if id == "help" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_help_window(a).await;
                });
            } else if id == "api" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_api_window(a).await;
                });
            } else if id == "chat" {
                #[cfg(feature = "llm")]
                {
                    let a = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = open_chat_window(a).await;
                    });
                }
            } else if id == "downloads" {
                #[cfg(feature = "llm")]
                {
                    let a = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = open_downloads_window(a).await;
                    });
                }
            } else if id == "focus_timer" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_focus_timer_window(a).await;
                });
            } else if id == "show_logs" {
                open_latest_log();
            } else if id == "check_update" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_updates_window(a).await;
                });
            } else if id == "quit" {
                confirm_and_quit(app.app_handle().clone());
            } else if let Some(dev_id) = id.strip_prefix("connect:") {
                let _ = set_preferred_device(dev_id.to_owned(), app.clone());
                retry_connect(app.clone());
            } else if let Some(dev_id) = id.strip_prefix("forget:") {
                let dev_id = dev_id.to_owned();
                forget_device(dev_id, app.clone());
            }
        })
        .on_tray_icon_event(|_tray, _event| {})
        .build(app)?;

    let app_scan = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let (wifi_shield_ip, galea_ip) = {
            let r = app_scan.state::<Mutex<Box<AppState>>>();
            let s = r.lock_or_recover();
            (
                s.openbci_config.wifi_shield_ip.clone(),
                s.openbci_config.galea_ip.clone(),
            )
        };
        let _ = crate::daemon_cmds::scanner_set_wifi_config(wifi_shield_ip, galea_ip);
        let _ = crate::daemon_cmds::scanner_start();
        // Start LSL auto-scanner if enabled in settings
        settings_cmds::lsl_cmds::maybe_start_lsl_auto_scanner(&app_scan);
    });

    // iroh remote-session auto-start is daemon-owned.

    let app_auto = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(900)).await;
        let preferred = {
            let r = app_auto.state::<Mutex<Box<AppState>>>();
            let mut s = r.lock_or_recover();
            let pref = s
                .preferred_id
                .clone()
                .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()));
            if pref.is_some() {
                s.pending_reconnect = true;
            }
            pref
        };
        // Only auto-connect if there's a paired device.  On first launch
        // (no paired devices) the user must discover and pair manually —
        // except that the first successful connection auto-pairs as a
        // convenience (handled in on_connected).
        if let Some(preferred) = preferred {
            let _ = set_preferred_device(preferred, app_auto.clone());
            retry_connect(app_auto.clone());
        }
    });

    // ── Daemon status poll ───────────────────────────────────────────────
    // The daemon's session runner updates status asynchronously (e.g.
    // connecting → connected).  Poll the daemon every 2 s and mirror
    // changes into the local Tauri AppState so the frontend receives
    // real-time status events.
    let app_poll = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        // Wait for daemon to be ready before polling.
        tokio::time::sleep(Duration::from_secs(2)).await;
        loop {
            let poll_result = tokio::task::spawn_blocking(crate::daemon_cmds::fetch_daemon_status)
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            match poll_result {
                Ok(daemon_status) => {
                    let changed = {
                        let r = app_poll.state::<Mutex<Box<AppState>>>();
                        let s = r.lock_or_recover();
                        s.status.state != daemon_status.state
                            || s.status.device_name != daemon_status.device_name
                            || s.status.sample_count != daemon_status.sample_count
                            || s.status.device_error != daemon_status.device_error
                    };
                    if changed {
                        {
                            let r = app_poll.state::<Mutex<Box<AppState>>>();
                            let mut s = r.lock_or_recover();
                            apply_daemon_status(&mut s.status, daemon_status);
                        }
                        emit_status_from_daemon(&app_poll);
                    }
                }
                Err(_) => { /* daemon unreachable — skip this tick */ }
            }
            // Adaptive poll: 2 s when disconnected (catch transitions fast),
            // 5 s when connected (just sample count / battery updates).
            let delay = {
                let r = app_poll.state::<Mutex<Box<AppState>>>();
                let s = r.lock_or_recover();
                if s.status.state == "connected" {
                    5
                } else {
                    2
                }
            };
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
    });

    let app_cal = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let auto_start_id: Option<String> = {
            let r = app_cal.state::<Mutex<Box<AppState>>>();
            let s = r.lock_or_recover();
            let active_id = &s.active_calibration_id;
            s.calibration_profiles
                .iter()
                .find(|p| &p.id == active_id)
                .filter(|p| p.auto_start)
                .map(|p| p.id.clone())
        };
        if let Some(id) = auto_start_id {
            let _ = open_calibration_window_inner(&app_cal, Some(id), false).await;
        }
    });

    let app_onboard = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        let done = {
            let r = app_onboard.state::<Mutex<Box<AppState>>>();
            let g = r.lock_or_recover();
            g.ui.onboarding_complete
        };
        if !done {
            let _ = open_onboarding_window(app_onboard).await;
        }
    });

    // Screenshot capture worker is daemon-owned in the thin-client architecture.

    setup_background_tasks(app);
    Ok(())
}

/// Load persisted settings from disk and apply them to `AppState`.
///
/// Extracted from `setup_app` to keep the setup function under 300 lines.
/// Reads `settings.json`, populates every `AppState` field, and pre-warms
/// TTS if configured.
#[inline(never)]
fn load_and_apply_settings(app: &mut tauri::App, skill_dir: &std::path::Path) {
    let data = load_settings(skill_dir);
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.status.paired_devices = data.paired.clone();
        s.preferred_id = data.preferred_id.clone();
        s.status.filter_config = data.filter_config;
        s.status.embedding_overlap_secs = data.embedding_overlap_secs;
        s.shortcuts.label_shortcut = data.label_shortcut;
        s.shortcuts.search_shortcut = data.search_shortcut;
        s.shortcuts.settings_shortcut = data.settings_shortcut;
        s.shortcuts.calibration_shortcut = data.calibration_shortcut;
        s.shortcuts.help_shortcut = data.help_shortcut;
        s.shortcuts.history_shortcut = data.history_shortcut;
        s.shortcuts.api_shortcut = data.api_shortcut;
        s.shortcuts.theme_shortcut = data.theme_shortcut;
        s.shortcuts.focus_timer_shortcut = data.focus_timer_shortcut;
        let mut profiles = data.calibration_profiles;
        if profiles.is_empty() {
            profiles.push(CalibrationProfile::from_legacy(&data.calibration));
        }
        s.calibration_profiles = profiles;
        s.active_calibration_id = if data.active_calibration_id.is_empty() {
            s.calibration_profiles
                .first()
                .map(|p| p.id.clone())
                .unwrap_or_default()
        } else {
            data.active_calibration_id
        };
        s.ui.onboarding_complete = data.onboarding_complete;
        s.ui.last_seen_whats_new_version = data.last_seen_whats_new_version;
        s.ui.theme = data.theme;
        s.ui.language = data.language;
        s.ui.daily_goal_min = data.daily_goal_min;
        s.ui.goal_notified_date = data.goal_notified_date;
        s.ui.text_embedding_model = data.text_embedding_model.clone();
        s.hooks = data.hooks;
        s.ws_host = data.ws_host.clone();
        s.ws_port = data.ws_port;
        s.api_token = data.api_token.clone();
        s.hf_endpoint = data.hf_endpoint.clone();
        s.update_check_interval_secs = data.update_check_interval_secs;
        s.openbci_config = data.openbci;
        s.device_api_config = data.device_api;
        s.scanner_config = data.scanner;
        s.location_enabled = data.location_enabled;
        s.inference_device = data.inference_device.clone();
        s.llm_gpu_layers_saved = data.llm_gpu_layers_saved;
        s.exg_inference_device = data.exg_inference_device.clone();
        s.neutts_config = data.neutts.clone();
        s.tts_preload = data.tts_preload;
        s.input.track_active_window = data.track_active_window;
        s.input.track_input_activity = data.track_input_activity;
        s.ui.main_window_auto_fit = data.main_window_auto_fit;
        s.input.input_activity_enabled.store(
            data.track_input_activity,
            std::sync::atomic::Ordering::Relaxed,
        );
        s.dnd.lock_or_recover().config = data.do_not_disturb;
        {
            let __a = s.llm.clone();
            __a.lock_or_recover().config = data.llm;
        }
        s.settings_storage_format = data.storage_format;
        s.sleep_config = data.sleep;
        s.screenshot_config = data.screenshot;
        // Ensure all HF-backed download paths use the persisted endpoint.
        std::env::set_var("HF_ENDPOINT", &s.hf_endpoint);

        if let Some(os_active) = skill_data::dnd::query_os_active() {
            if !os_active {
                s.dnd.lock_or_recover().active = false;
            }
        }
        neutts_apply_config(&data.neutts);
        for pd in &data.paired {
            let transport = crate::helpers::transport_from_id(&pd.id);
            s.discovered.push(DiscoveredDevice {
                id: pd.id.clone(),
                name: pd.name.clone(),
                last_seen: pd.last_seen,
                last_rssi: 0,
                is_paired: true,
                is_preferred: data.preferred_id.as_deref() == Some(&pd.id),
                transport,
            });
        }
    }

    if data.tts_preload {
        let app_handle = app.handle().clone();
        tauri::async_runtime::spawn(async move {
            crate::tts::tts_init(app_handle).await.ok();
        });
    }
}

/// Long-running background async tasks (updater poll, DND OS poll).
/// Extracted into its own `#[inline(never)]` function to keep `setup_app`
/// frame smaller.
#[inline(never)]
fn setup_background_tasks(app: &mut tauri::App) {
    let app_upd = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        use tauri_plugin_updater::UpdaterExt;
        let mut updater_platform_unsupported = false;
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            if updater_platform_unsupported {
                break;
            }
            eprintln!("[updater] running background update check");
            match app_upd.updater() {
                Err(e) => eprintln!("[updater] cannot get updater: {e}"),
                Ok(updater) => {
                    let result =
                        tokio::time::timeout(Duration::from_secs(30), updater.check()).await;
                    match result {
                        Err(_) => eprintln!("[updater] check timed out after 30 s"),
                        Ok(Ok(Some(update))) => {
                            eprintln!("[updater] update available: {}", update.version);
                            let payload = serde_json::json!({
                                "version": update.version,
                                "date":    update.date,
                                "body":    update.body,
                            });
                            let _ = app_upd.emit("update-available", payload);
                        }
                        Ok(Ok(None)) => {
                            eprintln!("[updater] up to date");
                            let _ = app_upd.emit("update-checked", ());
                        }
                        Ok(Err(e)) => {
                            let msg = e.to_string();
                            if msg.contains("None of the fallback platforms")
                                || msg.contains("were found in the response `platforms` object")
                            {
                                eprintln!(
                                    "[updater] no release artifacts for this platform; \
                                     disabling background update checks"
                                );
                                updater_platform_unsupported = true;
                            } else {
                                eprintln!("[updater] check failed: {e}");
                            }
                        }
                    }
                }
            }

            let interval_secs = {
                let r = app_upd.state::<Mutex<Box<AppState>>>();
                let g = r.lock_or_recover();
                g.update_check_interval_secs
            };
            let sleep_secs = if interval_secs == 0 {
                60
            } else {
                interval_secs
            };
            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
        }
    });

    // ── Background community-skills sync ─────────────────────────────
    let app_skills = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        // Wait a bit after startup before first sync attempt.
        tokio::time::sleep(Duration::from_secs(45)).await;
        let mut first_run = true;
        loop {
            let (skill_dir, interval_secs, sync_on_launch) = {
                let r = app_skills.state::<Mutex<Box<AppState>>>();
                let (sd, llm_arc) = {
                    let g = r.lock_or_recover();
                    (g.skill_dir.clone(), g.llm.clone())
                };
                let tools = &llm_arc.lock_or_recover().config.tools;
                let iv = tools.skills_refresh_interval_secs;
                let sol = tools.skills_sync_on_launch;
                (sd, iv, sol)
            };

            // On first run, force sync if sync_on_launch is enabled;
            // otherwise respect the normal interval.
            let force_launch = first_run && sync_on_launch;
            let effective_interval = if force_launch { 0 } else { interval_secs };
            first_run = false;

            if force_launch || interval_secs > 0 {
                eprintln!("[skills-sync] checking for community skills update");
                let sd = skill_dir.clone();
                let iv = effective_interval;
                let outcome = tokio::task::spawn_blocking(move || {
                    skill_skills::sync::sync_skills(&sd, iv, None)
                })
                .await;

                match outcome {
                    Ok(skill_skills::sync::SyncOutcome::Updated { elapsed_ms, .. }) => {
                        eprintln!("[skills-sync] updated in {elapsed_ms} ms");
                        let _ = app_skills.emit("skills-updated", ());
                    }
                    Ok(skill_skills::sync::SyncOutcome::Fresh { next_sync_in_secs }) => {
                        eprintln!("[skills-sync] fresh, next check in {next_sync_in_secs} s");
                    }
                    Ok(skill_skills::sync::SyncOutcome::Failed(e)) => {
                        eprintln!("[skills-sync] failed: {e}");
                    }
                    Err(e) => {
                        eprintln!("[skills-sync] task panic: {e}");
                    }
                }
            }

            let sleep_secs = if interval_secs == 0 {
                300
            } else {
                interval_secs.min(3600)
            };
            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
        }
    });

    // ── Background OS DND poll ────────────────────────────────────────
    let app_dnd = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        loop {
            let os_now = skill_data::dnd::query_os_active();

            // DND state is behind its own lock — no AppState lock needed.
            let dnd_arc = app_dnd
                .state::<Mutex<Box<AppState>>>()
                .lock_or_recover()
                .dnd_arc();
            let (prev, app_active) = {
                let d = dnd_arc.lock_or_recover();
                (d.os_active, d.active)
            };

            if os_now != prev {
                dnd_arc.lock_or_recover().os_active = os_now;

                let payload = serde_json::json!({ "os_active": os_now });
                let _ = app_dnd.emit("dnd-os-changed", &payload);
                app_dnd
                    .state::<WsBroadcaster>()
                    .send("dnd-os-changed", &payload);

                if os_now == Some(false) && app_active {
                    eprintln!(
                        "[dnd] OS DND was externally cleared while \
                         app believed it was active — reconciling"
                    );
                    {
                        let mut d = dnd_arc.lock_or_recover();
                        d.active = false;
                        d.below_ticks = 0;
                        d.focus_samples.clear();
                    }
                    let _ = app_dnd.emit("dnd-state-changed", false);
                    app_dnd
                        .state::<WsBroadcaster>()
                        .send("dnd-state-changed", &false);
                }
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

// ── App entry-point ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ── Windows: install a vectored exception handler for crash diagnostics ──
    #[cfg(target_os = "windows")]
    // SAFETY: Raw Win32 FFI for crash diagnostics. The handler only reads
    // exception pointers provided by the OS after a null check, and the
    // `AddVectoredExceptionHandler` call is safe with a valid function pointer.
    unsafe {
        // Raw Win32 FFI — no windows-sys dependency needed.
        #[repr(C)]
        struct ExceptionRecord {
            exception_code: u32,
            exception_flags: u32,
            exception_record: *mut ExceptionRecord,
            exception_address: *mut core::ffi::c_void,
            number_parameters: u32,
            exception_information: [usize; 15], // EXCEPTION_MAXIMUM_PARAMETERS
        }
        #[repr(C)]
        struct ExceptionPointers {
            exception_record: *mut ExceptionRecord,
            context_record: *mut core::ffi::c_void,
        }
        type VectoredHandler = unsafe extern "system" fn(*mut ExceptionPointers) -> i32;

        extern "system" {
            fn AddVectoredExceptionHandler(
                first: u32,
                handler: VectoredHandler,
            ) -> *mut core::ffi::c_void;
        }

        const EXCEPTION_ACCESS_VIOLATION: u32 = 0xC000_0005;
        const EXCEPTION_CONTINUE_SEARCH: i32 = 0;

        unsafe extern "system" fn crash_handler(info: *mut ExceptionPointers) -> i32 {
            if info.is_null() {
                return EXCEPTION_CONTINUE_SEARCH;
            }
            // SAFETY: `info` is non-null (checked above) and points to a valid
            // OS-provided `ExceptionPointers` structure for the duration of this call.
            let record = unsafe { (*info).exception_record };
            if record.is_null() {
                return EXCEPTION_CONTINUE_SEARCH;
            }
            // SAFETY: `record` is non-null (checked above) and points to a valid
            // OS-provided `ExceptionRecord` structure.
            let code = unsafe { (*record).exception_code };
            if code == EXCEPTION_ACCESS_VIOLATION {
                // SAFETY: Same as above — reading fields from a valid ExceptionRecord.
                let addr = unsafe { (*record).exception_address as usize };
                // SAFETY: Same as above — reading fields from a valid ExceptionRecord.
                let info0 = unsafe { (*record).exception_information[0] }; // 0=read, 1=write, 8=DEP
                                                                           // SAFETY: Same as above — reading fields from a valid ExceptionRecord.
                let info1 = unsafe { (*record).exception_information[1] }; // target address
                let op = match info0 {
                    0 => "reading",
                    1 => "writing",
                    8 => "DEP violation at",
                    _ => "accessing",
                };
                eprintln!("\n=== STATUS_ACCESS_VIOLATION ===");
                eprintln!("Faulting instruction: 0x{addr:016x}");
                eprintln!("Operation: {op} address 0x{info1:016x}");
                eprintln!(
                    "Thread: {:?}",
                    std::thread::current().name().unwrap_or("unnamed")
                );
                eprintln!("\nBacktrace:");
                eprintln!("{}", std::backtrace::Backtrace::force_capture());
                eprintln!("=== END CRASH INFO ===\n");
            }
            EXCEPTION_CONTINUE_SEARCH
        }

        AddVectoredExceptionHandler(0, crash_handler);
    }

    // ── rustls CryptoProvider ─────────────────────────────────────────────
    // Multiple transitive deps activate both the `ring` and `aws-lc-rs`
    // features of rustls 0.23, so it cannot auto-select a provider.
    // Install `ring` explicitly before any TLS connection is attempted.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls CryptoProvider");

    // ── Vulkan: disable validation layers in debug builds ──────────────
    //
    // The VulkanSDK (installed by build.rs for shader compilation) registers
    // VK_LAYER_KHRONOS_validation as an implicit Vulkan layer.  In debug
    // builds this validation layer is loaded by every Vulkan client —
    // including llama.cpp (ggml-vulkan) and wgpu/cubecl.
    //
    // The validation layer can trigger STATUS_ACCESS_VIOLATION (0xc0000005)
    // on certain Windows GPU drivers during vkEnumeratePhysicalDevices /
    // shader compilation.  Disable it via env vars that both the Vulkan
    // loader and wgpu respect.
    //
    // Must be set before any Vulkan code runs (llm-actor, eeg-embed, etc.).
    if cfg!(debug_assertions) {
        // Vulkan loader: disable the validation implicit layer.
        // VK_LOADER_LAYERS_DISABLE is supported by Vulkan Loader ≥ 1.3.234.
        if std::env::var("VK_LOADER_LAYERS_DISABLE").is_err() {
            std::env::set_var("VK_LOADER_LAYERS_DISABLE", "VK_LAYER_KHRONOS_validation");
        }
        // Older Vulkan loaders: override the implicit layer list to empty.
        if std::env::var("VK_INSTANCE_LAYERS").is_err() {
            std::env::set_var("VK_INSTANCE_LAYERS", "");
        }
        // wgpu-specific: disable wgpu's own validation flags.
        if std::env::var("WGPU_VALIDATION").is_err() {
            std::env::set_var("WGPU_VALIDATION", "0");
        }
        if std::env::var("WGPU_GPU_BASED_VALIDATION").is_err() {
            std::env::set_var("WGPU_GPU_BASED_VALIDATION", "0");
        }
    }

    // ── Linux: suppress noisy libEGL / DRI2 warnings ──────────────────────
    // WebKitGTK probes for DRI2/DMABuf GPU rendering at startup.  On systems
    // without full DRI2 support (VMs, Wayland-only, missing Mesa drivers)
    // this produces harmless but noisy warnings on stderr:
    //   "libEGL warning: egl: failed to create dri2 screen"
    //   "libEGL warning: DRI2: failed to create screen"
    // WebKit falls back to software rendering automatically; the warnings
    // are purely cosmetic.  Suppress them:
    //   • WEBKIT_DISABLE_DMABUF_RENDERER — skip the DMABuf/DRI2 probe path
    //     entirely so the warnings are never emitted.
    //   • EGL_LOG_LEVEL=fatal — tell Mesa's EGL loader to only print fatal
    //     errors, not warnings.
    // Both must be set before Tauri/GTK creates the WebView.
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        if std::env::var("EGL_LOG_LEVEL").is_err() {
            std::env::set_var("EGL_LOG_LEVEL", "fatal");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unminimize();
                let _ = win.show();
                let _ = win.set_focus();
                crate::linux_fix_decorations(&win);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .manage(Mutex::new(AppState::new_boxed()))
        .manage(job_queue::JobQueue::new())
        .setup(|app| setup_app(app))
        .invoke_handler(tauri::generate_handler![
            get_supported_companies,
            get_device_capabilities,
            open_bt_settings,
            check_bluetooth_power,
            open_settings_window,
            open_updates_window,
            open_model_tab,
            open_help_window,
            check_accessibility_permission,
            open_accessibility_settings,
            open_notifications_settings,
            check_screen_recording_permission,
            open_screen_recording_settings,
            get_calendar_permission_status,
            request_calendar_permission,
            get_calendar_events,
            open_calendar_settings,
            get_location_permission_status,
            request_location_permission,
            open_location_settings,
            open_input_monitoring_settings,
            open_focus_settings,
            set_notch_preset,
            get_log_config,
            set_log_config,
            get_theme_and_language,
            set_theme,
            set_language,
            get_accent_color,
            set_accent_color,
            open_session_for_timestamp,
            quit_app,
            open_label_window,
            open_label_window_at,
            open_labels_window,
            open_focus_timer_window,
            close_label_window,
            get_queue_stats,
            rebuild_label_index,
            search_labels_by_eeg,
            open_search_window,
            open_history_window,
            list_session_days,
            list_sessions_for_day,
            stream_sessions,
            open_compare_window,
            open_compare_window_with_sessions,
            get_day_metrics_batch,
            get_label_shortcut,
            set_label_shortcut,
            get_search_shortcut,
            set_search_shortcut,
            get_settings_shortcut,
            set_settings_shortcut,
            get_calibration_shortcut,
            set_calibration_shortcut,
            get_help_shortcut,
            set_help_shortcut,
            get_history_shortcut,
            set_history_shortcut,
            get_api_shortcut,
            set_api_shortcut,
            get_theme_shortcut,
            set_theme_shortcut,
            get_focus_timer_shortcut,
            set_focus_timer_shortcut,
            open_calibration_window,
            open_and_start_calibration,
            close_calibration_window,
            list_calibration_profiles,
            get_calibration_profile,
            get_active_calibration,
            set_active_calibration,
            create_calibration_profile,
            update_calibration_profile,
            delete_calibration_profile,
            record_calibration_completed,
            get_calibration_config,
            set_calibration_config,
            emit_calibration_event,
            get_app_version,
            get_app_name,
            is_session_live,
            set_update_ready,
            get_data_dir,
            set_data_dir,
            open_skill_dir,
            open_latest_log,
            get_daemon_status,
            get_daemon_token_path,
            get_daemon_bootstrap,
            start_daemon_dev,
            daemon_install_service,
            daemon_uninstall_service,
            get_daemon_service_status,
            get_autostart_enabled,
            set_autostart_enabled,
            get_update_check_interval,
            set_update_check_interval,
            pick_ref_wav_file,
            get_recent_active_windows,
            get_recent_input_activity,
            get_input_buckets,
            test_location,
            get_exg_catalog,
            get_eeg_model_config,
            get_eeg_model_status,
            set_eeg_model_config,
            trigger_weights_download,
            cancel_weights_download,
            estimate_reembed,
            trigger_reembed,
            lsl_discover,
            lsl_get_config,
            lsl_set_auto_connect,
            lsl_pair_stream,
            lsl_unpair_stream,
            lsl_get_idle_timeout,
            lsl_set_idle_timeout,
            lsl_virtual_source_running,
            lsl_virtual_source_start,
            lsl_virtual_source_stop,
            lsl_iroh_start,
            lsl_iroh_stop,
            lsl_iroh_status,
            start_session,
            switch_session,
            cancel_session,
            pick_exg_weights_file,
            pick_gguf_file,
            // LLM catalog (compiled in regardless; no-op stubs when `llm` feature absent)
            #[cfg(feature = "llm")]
            open_chat_window,
            #[cfg(feature = "llm")]
            open_downloads_window,
            #[cfg(feature = "llm")]
            get_chat_shortcut,
            #[cfg(feature = "llm")]
            set_chat_shortcut,
            tts_unload,
            tts_get_voice,
            tts_list_neutts_voices,
            session_connect::connect_openbci,
            open_api_window,
            open_whats_new_window,
            get_whats_new_seen_version,
            dismiss_whats_new,
            open_onboarding_window,
            get_onboarding_model_download_order,
            complete_onboarding,
            get_onboarding_complete,
            open_session_window,
            open_api_window,
            open_whats_new_window,
            get_whats_new_seen_version,
            dismiss_whats_new,
            open_onboarding_window,
            get_onboarding_model_download_order,
            complete_onboarding,
            get_onboarding_complete,
            session_connect::connect_openbci,
            tts_init,
            tts_speak,
            tts_list_voices,
            tts_set_voice,
            tts_unload,
            tts_get_voice,
            tts_list_neutts_voices,
            get_about_info,
            open_about_window,
            show_main_window,
            autosize_main_window,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match event {
                tauri::RunEvent::WindowEvent { label, event, .. } => {
                    match &event {
                        tauri::WindowEvent::CloseRequested { .. } => {
                            eprintln!("[window-event] label={label} CloseRequested");
                        }
                        tauri::WindowEvent::Destroyed => {
                            eprintln!("[window-event] label={label} Destroyed");
                        }
                        tauri::WindowEvent::Focused(focused) => {
                            eprintln!("[window-event] label={label} Focused({focused})");
                        }
                        tauri::WindowEvent::Moved(pos) => {
                            eprintln!("[window-event] label={label} Moved({},{})", pos.x, pos.y);
                        }
                        tauri::WindowEvent::Resized(size) => {
                            eprintln!(
                                "[window-event] label={label} Resized({}x{})",
                                size.width, size.height
                            );
                        }
                        tauri::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                            eprintln!(
                                "[window-event] label={label} ScaleFactorChanged({scale_factor})"
                            );
                        }
                        _ => {}
                    }
                    if label == "main" {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                            // Always prevent close and hide the window instead.
                            // User must click "Quit" in the tray menu to exit the app.
                            eprintln!("[window-event] main: preventing close, hiding window");
                            api.prevent_close();
                            if let Some(win) = app.get_webview_window("main") {
                                let _ = win.hide();
                            }
                        }
                    }
                }
                #[allow(unused_variables)]
                tauri::RunEvent::ExitRequested { api, code, .. } => {
                    eprintln!("[run-event] ExitRequested code={code:?}");
                    if code.is_none() {
                        eprintln!("[run-event] preventing exit, hiding main window");
                        api.prevent_exit();
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.hide();
                        }
                    } else {
                        eprintln!(
                            "[run-event] explicit exit requested — running blocking shutdown"
                        );
                        run_blocking_exit_shutdown(app);
                    }
                }
                // macOS: user clicks the Dock icon while the app is running
                // with no visible windows (all hidden in the tray).
                // Without this handler the click is silently ignored.
                // show_and_recover_main() also handles the blank-page case
                // that can occur after the window has been hidden for a day.
                // RunEvent::Reopen is a macOS-only variant; the #[cfg] attr
                // prevents a compile error on Linux and Windows.
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => {
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.set_focus();
                        if win
                            .eval("window.__skill_loaded||(window.location.reload(),false)")
                            .is_err()
                        {
                            if let Ok(url) = "tauri://localhost".parse() {
                                let _ = win.navigate(url);
                            }
                        }
                    }
                }
                tauri::RunEvent::Exit => {
                    run_blocking_exit_shutdown(app);
                }
                _ => {}
            }
        });
}
