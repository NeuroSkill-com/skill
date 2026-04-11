// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//
// lib.rs — crate root.
//
// Thin UI client: module declarations, shared macros, re-exports,
// and the Tauri `run()` entry-point (generate_handler! must live here).
// All business logic lives in skill-daemon.

// ── Logging ──────────────────────────────────────────────────────────────────

mod constants;

#[macro_use]
mod skill_log;

#[allow(unused_macros)]
/// Convenience wrapper around [`skill_log!`] for code that holds an
/// `&AppHandle` but not a direct reference to the logger.
macro_rules! app_log {
    ($app:expr, $tag:literal, $($arg:tt)*) => {{
        use tauri::Manager as _;
        let _lg = $app.state::<std::sync::Arc<$crate::skill_log::SkillLogger>>();
        skill_log!(_lg, $tag, $($arg)*);
    }};
}

// ── Core types & helpers ─────────────────────────────────────────────────────

mod state;
pub(crate) use state::*;

mod helpers;
pub(crate) use helpers::{
    emit_devices, emit_status, mutate_and_save, save_settings, send_toast, unix_secs, yyyymmdd_utc,
    AppStateExt, ToastLevel,
};

pub(crate) use skill_data::util::MutexExt;

// ── Platform helpers ─────────────────────────────────────────────────────────

mod platform;
pub(crate) use platform::linux_fix_decorations;

mod shutdown;

// ── Feature modules ──────────────────────────────────────────────────────────

mod lifecycle;

mod quit;
pub(crate) use quit::confirm_and_quit;

mod job_queue;
mod label_index;
pub mod ws_server;
pub use ws_server::WsBroadcaster;

#[allow(dead_code, unused_imports)]
mod llm;

mod session_connect;
mod session_csv;

mod history_cmds;
mod session_analysis;

mod autostart;
mod settings;
mod tts;
pub(crate) use settings::{default_skill_dir, CalibrationConfig, CalibrationProfile};

mod tray;

mod about;
mod active_window;
mod calibration_service;
mod shortcut_cmds;

mod window_cmds;

mod daemon_cmds;
mod label_cmds;
mod settings_cmds;

#[cfg(target_os = "macos")]
mod external_renderer;

// ── App lifecycle ────────────────────────────────────────────────────────────

mod background;
mod setup;
mod tray_setup;

// ── Imports for run() / generate_handler! ────────────────────────────────────

use std::sync::Mutex;

use tauri::Manager;

use about::{get_about_info, open_about_window};
use daemon_cmds::{
    cancel_session, cancel_weights_download, daemon_install_service, daemon_uninstall_service,
    estimate_reembed, get_daemon_bootstrap, get_daemon_service_status, get_daemon_status,
    get_daemon_token_path, get_eeg_model_config, get_eeg_model_status, get_exg_catalog,
    lsl_discover, lsl_get_config, lsl_get_idle_timeout, lsl_iroh_start, lsl_iroh_status,
    lsl_iroh_stop, lsl_pair_stream, lsl_set_auto_connect, lsl_set_idle_timeout, lsl_unpair_stream,
    lsl_virtual_source_running, lsl_virtual_source_start, lsl_virtual_source_start_configured,
    lsl_virtual_source_stop, set_eeg_model_config, start_daemon_dev, start_session, switch_session,
    trigger_reembed, trigger_weights_download,
};
use history_cmds::{
    list_session_days, list_sessions_for_day, open_history_window, stream_sessions,
};
use label_cmds::{get_queue_stats, rebuild_label_index, search_labels_by_eeg};
#[cfg(feature = "llm")]
use llm::cmds::{open_chat_window, open_downloads_window};
use session_analysis::{
    get_day_metrics_batch, open_compare_window, open_compare_window_with_sessions,
};
use settings_cmds::*;
use shortcut_cmds::{
    get_api_shortcut, get_calibration_shortcut, get_focus_timer_shortcut, get_help_shortcut,
    get_history_shortcut, get_label_shortcut, get_search_shortcut, get_settings_shortcut,
    get_theme_shortcut, set_api_shortcut, set_calibration_shortcut, set_focus_timer_shortcut,
    set_help_shortcut, set_history_shortcut, set_label_shortcut, set_search_shortcut,
    set_settings_shortcut, set_theme_shortcut,
};
#[cfg(feature = "llm")]
use shortcut_cmds::{get_chat_shortcut, set_chat_shortcut};
use shutdown::run_blocking_exit_shutdown;
use tts::{
    tts_get_voice, tts_init, tts_list_neutts_voices, tts_list_voices, tts_set_voice, tts_speak,
    tts_unload,
};
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
    open_skill_dir, open_updates_window, open_virtual_devices_window, open_whats_new_window,
    quit_app, record_calibration_completed, request_calendar_permission,
    request_location_permission, set_active_calibration, set_calibration_config, set_data_dir,
    set_update_ready, show_main_window, update_calibration_profile,
};

// ── App entry-point ──────────────────────────────────────────────────────────
//
// `run()` must live in the crate root because `tauri::generate_handler!`
// expands companion macros (`__cmd__*`) that require crate-root visibility.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "windows")]
    platform::install_windows_crash_handler();

    platform::setup_env();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unminimize();
                let _ = win.show();
                let _ = win.set_focus();
                linux_fix_decorations(&win);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .manage(Mutex::new(AppState::new_boxed()))
        .manage(job_queue::JobQueue::new())
        .setup(|app| setup::setup_app(app).map_err(Into::into))
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
            lsl_virtual_source_start_configured,
            lsl_virtual_source_stop,
            lsl_iroh_start,
            lsl_iroh_stop,
            lsl_iroh_status,
            start_session,
            switch_session,
            cancel_session,
            pick_exg_weights_file,
            pick_gguf_file,
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
            open_virtual_devices_window,
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
        .run(|app, event| match event {
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
                    eprintln!("[run-event] explicit exit requested — running blocking shutdown");
                    run_blocking_exit_shutdown(app);
                }
            }
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
        });
}
