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
use skill_log::SkillLogger;

/// Convenience wrapper around [`skill_log!`] for code that holds an
/// `&AppHandle` but not a direct reference to the logger.
///
/// Requires `Arc<SkillLogger>` to be registered as Tauri managed state
/// (done once in `run()` → `setup`).
///
/// ```rust
/// app_log!(app, "bluetooth", "connected: {name}");
/// ```
macro_rules! app_log {
    ($app:expr, $tag:literal, $($arg:tt)*) => {{
        let _lg = $app.state::<std::sync::Arc<$crate::skill_log::SkillLogger>>();
        skill_log!(_lg, $tag, $($arg)*);
    }};
}

/// GPU stats reading is macOS-only (IOKit + CoreFoundation frameworks).
mod gpu_stats { #[allow(unused_imports)] pub use skill_data::gpu_stats::*; }

mod eeg_model_config { #[allow(unused_imports)] pub use skill_eeg::eeg_model_config::*; }

mod eeg_embeddings;
mod global_eeg_index;
use global_eeg_index::GlobalEegIndex;


mod eeg_filter { #[allow(unused_imports)] pub use skill_eeg::eeg_filter::*; }

mod eeg_bands { #[allow(unused_imports)] pub use skill_eeg::eeg_bands::*; }

mod eeg_quality { #[allow(unused_imports)] pub use skill_eeg::eeg_quality::*; }
use eeg_quality::SignalQuality;

mod session_dsp;
pub(crate) use session_dsp::SessionDsp;

mod commands;
mod job_queue;
mod label_store { #[allow(unused_imports)] pub use skill_data::label_store::*; }
mod artifact_detection { #[allow(unused_imports)] pub use skill_eeg::artifact_detection::*; }
mod head_pose { #[allow(unused_imports)] pub use skill_eeg::head_pose::*; }
mod ppg_analysis { #[allow(unused_imports)] pub use skill_data::ppg_analysis::*; }
mod ws_commands;
mod label_index;
mod ws_server;
mod api;
pub(crate) mod hooks_log { #[allow(unused_imports)] pub use skill_data::hooks_log::*; }
mod screenshot;
mod screenshot_store { #[allow(unused_imports)] pub use skill_data::screenshot_store::*; }

/// OpenAI-compatible LLM inference server — same port as WebSocket API.
/// Enabled by the `llm` Cargo feature; no-op when the feature is absent.
#[cfg(feature = "llm")]
mod llm;

use ws_server::WsBroadcaster;

// ── New extracted modules ─────────────────────────────────────────────────────

/// CSV recording (CsvState, path helpers, session-metadata sidecar).
mod session_csv;

/// Background BLE scanner and Bluetooth availability helpers.
mod ble_scanner;
pub(crate) use ble_scanner::start_background_scanner;

/// Generic device session runner (replaces per-device session modules).
mod session_runner;

/// Per-device scan / connect factories → Box<dyn DeviceAdapter>.
mod session_connect;

/// Session history listing and streaming Tauri commands.
mod history_cmds;
use history_cmds::{
    open_history_window, list_sessions, list_session_days, list_sessions_for_day,
    stream_sessions, get_history_stats, delete_session, list_embedding_sessions,
};

/// Session metrics, time-series, sleep staging, UMAP and compare commands.
mod session_analysis;
pub(crate) use session_analysis::{
    get_session_metrics_impl,
    get_sleep_stages_impl,
    compute_compare_insights, analyze_sleep_stages, analyze_search_results,
    compute_status_history,
    load_embeddings_range
};
use session_analysis::{
    get_sleep_stages, compute_umap_compare, enqueue_umap_compare, poll_job,
    get_session_metrics, get_session_timeseries, get_csv_metrics, get_day_metrics_batch,
    open_compare_window, open_compare_window_with_sessions,
};

// ── Existing extracted modules ────────────────────────────────────────────────

mod autostart;

mod tts;
pub mod device { #[allow(unused_imports)] pub use skill_data::device::*; }
use tts::{tts_init, tts_speak, tts_unload, tts_list_voices, tts_list_neutts_voices, tts_get_voice, tts_set_voice};
pub(crate) use tts::{neutts_apply_config, init_neutts_samples_dir,
                     init_espeak_bundled_data_path, tts_shutdown};

mod settings;
pub(crate) use settings::{
    CalibrationAction, CalibrationProfile, CalibrationConfig, new_profile_id,
    load_umap_config, load_settings,
    default_skill_dir,
};

mod dnd { #[allow(unused_imports)] pub use skill_data::dnd::*; }



mod tray;
pub(crate) use tray::{refresh_tray, build_menu, icon_disconnected};

// ── Linux decoration workaround (tauri-apps/tauri#11856) ─────────────────────
// On Linux (Wayland + GNOME/Mutter/KWin), window decorations (close /
// minimize / maximize buttons) become completely unresponsive when a window
// is created with `visible(false)` and later shown, or after any hide→show
// cycle.  Briefly toggling fullscreen after `show()` forces the compositor
// to re-evaluate the decoration state.  The toggle is near-instantaneous
// and visually imperceptible.  Must be called *after* `win.show()`.
#[cfg(target_os = "linux")]
pub(crate) fn linux_fix_decorations(win: &tauri::WebviewWindow) {
    eprintln!("[linux] applying decoration fix (fullscreen toggle) for {}", win.label());
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
        let handle = unsafe { libc::dlopen(c_name.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
        if !handle.is_null() {
            let _ = unsafe { libc::dlclose(handle) };
            return true;
        }
    }

    false
}

mod shortcut_cmds;
pub(crate) use shortcut_cmds::apply_all_shortcuts;
use shortcut_cmds::{
    get_label_shortcut, set_label_shortcut,
    get_search_shortcut, set_search_shortcut,
    get_settings_shortcut, set_settings_shortcut,
    get_calibration_shortcut, set_calibration_shortcut,
    get_help_shortcut, set_help_shortcut,
    get_history_shortcut, set_history_shortcut,
    get_api_shortcut, set_api_shortcut,
    get_theme_shortcut, set_theme_shortcut,
    get_focus_timer_shortcut, set_focus_timer_shortcut,
};
#[cfg(feature = "llm")]
use shortcut_cmds::{get_chat_shortcut, set_chat_shortcut};


mod active_window;

mod activity_store { #[allow(unused_imports)] pub use skill_data::activity_store::*; }

mod about;
use about::{get_about_info, open_about_window};

mod window_cmds;
pub(crate) use window_cmds::open_calibration_window_inner;
use window_cmds::{
    open_bt_settings, open_settings_window, open_updates_window, open_model_tab, open_help_window,
    open_search_window, open_session_window, open_label_window, open_labels_window,
    open_focus_timer_window, open_api_window,
    show_main_window,
    open_whats_new_window, get_whats_new_seen_version, dismiss_whats_new,
    open_onboarding_window, get_onboarding_model_download_order,
    complete_onboarding, get_onboarding_complete, close_label_window,
    check_accessibility_permission, open_accessibility_settings, open_notifications_settings,
    check_screen_recording_permission, open_screen_recording_settings,
    open_calibration_window, open_and_start_calibration, close_calibration_window,
    list_calibration_profiles, get_calibration_profile, get_active_calibration,
    set_active_calibration, create_calibration_profile, update_calibration_profile,
    delete_calibration_profile, record_calibration_completed,
    get_calibration_config, set_calibration_config,
    emit_calibration_event, quit_app, get_app_version, get_app_name,
    get_data_dir, set_data_dir, open_skill_dir, get_ws_clients, get_ws_request_log, get_ws_port,
};

mod label_cmds;
pub(crate) use label_cmds::{EmbedderState, init_embedder};
use label_cmds::{
    query_annotations, get_recent_labels, delete_label, update_label, get_queue_stats, submit_label,
    list_embedding_models, get_embedding_model, set_embedding_model,
    reembed_all_labels, get_stale_label_count,
    rebuild_label_index, search_labels_by_text, search_labels_by_eeg,
};

mod settings_cmds;
use settings_cmds::{
    subscribe_eeg, subscribe_ppg, subscribe_imu,
    get_status, get_devices, set_preferred_device, pair_device, forget_device, cancel_retry, retry_connect,
    get_filter_config, set_filter_config, set_notch_preset,
    get_latest_bands, get_embedding_overlap, set_embedding_overlap,
    get_gpu_stats, get_log_config, set_log_config,
    get_eeg_model_config, set_eeg_model_config, get_eeg_model_status,
    trigger_weights_download, cancel_weights_download,
    get_umap_config, set_umap_config, get_theme_and_language, set_theme, set_language,
    get_accent_color, set_accent_color,
    get_daily_goal, set_daily_goal, get_goal_notified_date, set_goal_notified_date,
    get_daily_recording_mins,
    get_hooks, set_hooks, get_hook_statuses, open_session_for_timestamp,
    suggest_hook_distances, suggest_hook_keywords, get_hook_log, get_hook_log_count,
    get_ws_config, set_ws_config,
    get_autostart_enabled, set_autostart_enabled,
    get_update_check_interval, set_update_check_interval,
    get_openbci_config, set_openbci_config, list_serial_ports,
    get_neutts_config, set_neutts_config, pick_ref_wav_file,
    get_tts_preload, set_tts_preload,
    get_active_window_tracking, set_active_window_tracking, get_active_window,
    get_input_activity_tracking, set_input_activity_tracking,
    get_last_input_activity,
    get_recent_active_windows, get_recent_input_activity,
    get_input_buckets,
    get_dnd_config, set_dnd_config, get_dnd_active, get_dnd_status, test_dnd, list_focus_modes,
    get_llm_config, set_llm_config, pick_gguf_file,
    get_screenshot_config, set_screenshot_config,
    estimate_screenshot_reembed, rebuild_screenshot_embeddings,
    get_screenshots_around, search_screenshots_by_vector, search_screenshots_by_image,
    search_screenshots_by_text,
    check_ocr_models_ready, download_ocr_models,
    get_screenshot_metrics, get_screenshots_dir,
};

// LLM catalog commands (feature-gated)
#[cfg(feature = "llm")]
use llm::cmds::{
    get_llm_catalog, download_llm_model, cancel_llm_download,
    pause_llm_download, resume_llm_download, get_llm_downloads,
    delete_llm_model, refresh_llm_catalog, set_llm_active_model, set_llm_active_mmproj,
    set_llm_autoload_mmproj, add_llm_model,
    get_llm_logs, start_llm_server, stop_llm_server, switch_llm_model, get_llm_server_status, open_chat_window,
    open_downloads_window,
    chat_completions_ipc, abort_llm_stream, cancel_tool_call,
    get_last_chat_session, save_chat_message, save_chat_tool_calls, new_chat_session,
    load_chat_session, list_chat_sessions, rename_chat_session, delete_chat_session,
    get_session_params, set_session_params,
    archive_chat_session, unarchive_chat_session, list_archived_chat_sessions,
    get_model_hardware_fit,
};

// ── Imports ───────────────────────────────────────────────────────────────────

use std::{
    sync::Mutex,
    time::Duration,
};

use tauri::{
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

use session_csv::new_csv_path;
use session_runner::run_device_session;
use session_connect::ConnectError;

// ── Core types (re-exported from state.rs) ────────────────────────────────────

mod state;
pub(crate) use state::*;

// ── Shared helpers (re-exported from helpers.rs) ──────────────────────────────

mod helpers;
pub(crate) use helpers::{
    unix_secs, yyyymmdd_utc,
    emit_status, emit_devices,
    send_toast, ToastLevel,
    skill_dir, read_state, mutate_and_save,
    save_settings, save_settings_handle,
    upsert_paired, upsert_discovered,
};

// ── Mutex poison recovery ─────────────────────────────────────────────────────

// Re-export MutexExt from skill-data so `crate::MutexExt` keeps working
// everywhere in src-tauri. The canonical implementation lives in
// crates/skill-data/src/util.rs.
pub(crate) use skill_data::util::MutexExt;


// ── Reconnect backoff ─────────────────────────────────────────────────────────

/// Reconnect backoff: 1 s → 2 s → 3 s → 5 s, then stays at 5 s indefinitely.
fn retry_delay_secs(attempt: u32) -> u32 {
    match attempt { 0 => 1, 1 => 2, 2 => 3, _ => 5 }
}

// ── Disconnect / retry ────────────────────────────────────────────────────────

pub(crate) fn go_disconnected(app: &AppHandle, error: Option<String>, is_bt: bool) {
    let (retry, attempt) = {
        let r = app.state::<Mutex<Box<AppState>>>();
        let s = r.lock_or_recover();
        (s.pending_reconnect && !is_bt, s.retry_attempt)
    };
    let delay = if retry { retry_delay_secs(attempt) } else { 0 };

    {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        if is_bt {
            s.pending_reconnect = false;
            s.retry_attempt     = 0;
        } else if !retry {
            s.retry_attempt = 0;
        }
        s.status.state = if retry        { "scanning".into()      }
                         else if is_bt   { "bt_off".into()        }
                         else            { "disconnected".into()  };
        s.status.device_name        = None;
        s.status.device_id          = None;
        s.status.device_kind        = "unknown".into();
        s.status.serial_number      = None;
        s.status.mac_address        = None;
        s.status.firmware_version   = None;
        s.status.hardware_version   = None;
        s.status.bootloader_version = None;
        s.status.headset_preset     = None;
        s.status.battery            = 0.0;
        s.status.eeg                = vec![f64::NAN; 4];
        s.status.ppg                = vec![0.0; 3];
        s.status.ppg_sample_count   = 0;
        s.status.bt_error           = if retry { None } else { error };
        s.status.target_name        = None;
        s.status.retry_attempt        = if retry { attempt + 1 } else { 0 };
        s.status.retry_countdown_secs = delay;
        s.stream       = None;
        s.battery_ema  = None;
        s.latest_bands = None;
        // DSP objects live in SessionDsp (session-local, lock-free).
        // They are dropped when the session task exits; the next session
        // creates a fresh set.  No reset needed here.
        s.status.channel_quality = vec![SignalQuality::default(); crate::constants::EEG_CHANNELS];
    }
    refresh_tray(app);
    emit_status(app);

    if retry {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            app_log!(app, "bluetooth",
                "[reconnect] scheduling attempt #{} in {}s (backoff schedule: 1→2→3→5s)",
                attempt + 1, delay);
            for remaining in (1..=delay).rev() {
                {
                    let r = app.state::<Mutex<Box<AppState>>>();
                    if !r.lock_or_recover().pending_reconnect { return; }
                }
                app.state::<Mutex<Box<AppState>>>().lock_or_recover()
                    .status.retry_countdown_secs = remaining;
                emit_status(&app);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            let preferred = {
                let r = app.state::<Mutex<Box<AppState>>>();
                let mut s = r.lock_or_recover();
                if !s.pending_reconnect { return; }
                s.retry_attempt += 1;
                s.status.retry_countdown_secs = 0;
                s.preferred_id.clone()
                    .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()))
            };
            app_log!(app, "bluetooth",
                "[reconnect] attempt #{} — waited {delay}s — target={preferred:?}", attempt + 1);
            start_session(&app, preferred);
        });
    }
}

// ── Session lifecycle ─────────────────────────────────────────────────────────

pub(crate) fn start_session(app: &AppHandle, preferred_id: Option<String>) {
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        if s.stream.is_some() { return; }
        s.pending_reconnect = true;
    }
    let (tx, rx) = tokio::sync::oneshot::channel();

    let target = preferred_id.or_else(|| {
        let r = app.state::<Mutex<Box<AppState>>>();
        let s = r.lock_or_recover();
        s.preferred_id.clone()
            .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()))
    });

    let target_name: Option<String> = target.as_ref().and_then(|id| {
        let r = app.state::<Mutex<Box<AppState>>>();
        let s = r.lock_or_recover();
        s.status.paired_devices.iter()
            .find(|d| &d.id == id).map(|d| d.name.clone())
            .or_else(|| s.discovered.iter().find(|d| &d.id == id).map(|d| d.name.clone()))
    });
    let target_lower = target_name.as_deref().map(|n| n.to_lowercase());
    let is_ganglion = target_lower.as_deref().map(|n| {
        n.starts_with("ganglion") || n.starts_with("simblee")
    }).unwrap_or(false);
    let is_mw75 = target_lower.as_deref().map(|n| {
        n.contains("mw75")
    }).unwrap_or(false);
    let is_hermes = target_lower.as_deref().map(|n| {
        n.starts_with("hermes")
    }).unwrap_or(false);

    app.state::<Mutex<Box<AppState>>>().lock_or_recover().stream = Some(StreamHandle { cancel_tx: tx });
    let csv  = new_csv_path(app);
    let app2 = app.clone();

    let device_kind = if is_ganglion { "ganglion" }
                      else if is_mw75 { "mw75" }
                      else if is_hermes { "hermes" }
                      else { "muse" };

    app_log!(app, "bluetooth",
        "[session] routing: target={target:?} name={target_name:?} kind={device_kind}");

    // Set scanning state with the correct device_kind.
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        s.session_start_utc = Some(unix_secs());
        s.status.reset_for_scanning(device_kind, &csv, target.as_deref());
    }
    refresh_tray(app);
    emit_status(app);

    tauri::async_runtime::spawn(async move {
        // Use a shared cancellation token so both the connect phase and the
        // session phase observe the same cancel signal.
        let cancel = tokio_util::sync::CancellationToken::new();
        let cancel2 = cancel.clone();

        // Consume the oneshot in a background task that trips the token.
        tokio::spawn(async move {
            let _ = rx.await;
            cancel2.cancel();
        });

        let connect_result = match device_kind {
            "ganglion" => session_connect::connect_ganglion(&app2, &cancel, target).await,
            "mw75"     => session_connect::connect_mw75(&app2, &cancel, target).await,
            "hermes"   => session_connect::connect_hermes(&app2, &cancel, target).await,
            _          => session_connect::connect_muse(&app2, &cancel, target).await,
        };

        match connect_result {
            Ok(adapter) => {
                run_device_session(app2, cancel, csv, adapter).await;
            }
            Err(ConnectError::Cancelled) => {
                crate::go_disconnected(&app2, None, false);
            }
            Err(ConnectError::Bluetooth(msg)) => {
                crate::go_disconnected(&app2, Some(msg), true);
            }
            Err(ConnectError::Other(msg)) => {
                crate::go_disconnected(&app2, Some(msg), false);
            }
        }
    });
}

pub(crate) fn cancel_session(app: &AppHandle) {
    let tx = app.state::<Mutex<Box<AppState>>>().lock_or_recover().stream.take().map(|sh| sh.cancel_tx);
    if let Some(tx) = tx { let _ = tx.send(()); }
}

// ── Quit confirmation dialog ──────────────────────────────────────────────────

fn confirm_and_quit(app: AppHandle) {
    let lang = {
        let s = app.state::<Mutex<Box<AppState>>>();
        let g = s.lock_or_recover();
        g.language.clone()
    };
    std::thread::spawn(move || {
        if quit_confirmed(&lang, &app) { app.exit(0); }
    });
}

#[cfg(not(target_os = "macos"))]
fn quit_confirmed(lang: &str, app: &AppHandle) -> bool {
    use tauri::Manager;
    let (title, description) = quit_dialog_strings(lang);
    let mut dialog = rfd::MessageDialog::new()
        .set_title(title)
        .set_description(description)
        .set_buttons(rfd::MessageButtons::YesNo);
    // Set the parent window so the dialog appears focused / modal
    if let Some(win) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&win);
    }
    dialog.show() == rfd::MessageDialogResult::Yes
}

#[cfg(target_os = "macos")]
fn quit_confirmed(lang: &str, _app: &AppHandle) -> bool {
    use dispatch2::DispatchQueue;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn};
    use objc2_foundation::NSString;

    let (title, description) = quit_dialog_strings(lang);
    let mut confirmed = false;
    DispatchQueue::main().exec_sync(|| {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(title));
        alert.setInformativeText(&NSString::from_str(description));
        alert.addButtonWithTitle(&NSString::from_str("Yes"));
        alert.addButtonWithTitle(&NSString::from_str("No"));
        confirmed = alert.runModal() == NSAlertFirstButtonReturn;
    });
    confirmed
}

fn quit_dialog_strings(lang: &str) -> (&'static str, &'static str) {
    match lang {
        "de" => ("NeuroSkill™ beenden", "Möchten Sie NeuroSkill™ wirklich beenden?"),
        "fr" => ("Quitter NeuroSkill™", "Voulez-vous vraiment quitter NeuroSkill™ ?"),
        "he" => ("לצאת מ-NeuroSkill™", "האם אתה בטוח שברצונך לצאת מ-NeuroSkill™?"),
        "uk" => ("Вийти з NeuroSkill™", "Ви впевнені, що хочете вийти з NeuroSkill™?"),
        _    => ("Quit NeuroSkill™",    "Are you sure you want to quit NeuroSkill™?"),
    }
}

static EXIT_SHUTDOWN_STARTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn run_blocking_exit_shutdown(app: &tauri::AppHandle) {
    if EXIT_SHUTDOWN_STARTED.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }

    #[cfg(feature = "llm")]
    {
        let cell = app
            .state::<Mutex<Box<AppState>>>()
            .lock()
            .unwrap()
            .llm
            .state_cell
            .clone();
        llm::shutdown_cell(&cell);
    }

    tts_shutdown();
}

// ── App setup (extracted to reduce `run()` stack frame) ───────────────────────

/// Extracted from the `.setup()` closure so LLVM does not merge its locals
/// into the already-huge `run()` stack frame produced by `generate_handler!`.
#[inline(never)]
fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    {
        use tauri::Manager;
        let resource_dir = app.path().resource_dir()
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
            let size = tauri::LogicalSize::new(480.0_f64, 800.0_f64);
            let _ = win.set_min_size(Some(tauri::Size::Logical(size)));
            let _ = win.set_max_size(Some(tauri::Size::Logical(size)));
        }
    }

    let app_name = app.package_info().name.to_lowercase();
    let ws_cfg = {
        let dir = app.state::<Mutex<Box<AppState>>>().lock_or_recover().skill_dir.clone();
        let s   = load_settings(&dir);
        (s.ws_host, s.ws_port)
    };

    // ── LLM server (optional, same port) ──────────────────────────────
    #[cfg(feature = "llm")]
    {
        let (llm_cfg, catalog, log_buf, cell, skill_dir) = {
            let dir = app.state::<Mutex<Box<AppState>>>().lock_or_recover().skill_dir.clone();
            let llm_cfg = load_settings(&dir).llm;
            let guard = app.state::<Mutex<Box<AppState>>>();
            let s = guard.lock().unwrap();
            (llm_cfg, s.llm.catalog.clone(), s.llm.logs.clone(), s.llm.state_cell.clone(), s.skill_dir.clone())
        };
        if llm_cfg.enabled {
            let app_handle = app.handle().clone();
            let cell2 = cell.clone();
            std::thread::spawn(move || {
                let emitter: std::sync::Arc<dyn llm::LlmEventEmitter> = std::sync::Arc::new(llm::TauriEmitter(app_handle));
                if let Some(state) = llm::init(&llm_cfg, &catalog, emitter, log_buf, &skill_dir) {
                    *cell2.lock().unwrap() = Some(state);
                }
            });
        }
    }

    #[allow(unused_mut)]
    let (broadcaster, mut serve_handle) = ws_server::bind_with(ws_cfg.0, ws_cfg.1);
    ws_server::register_mdns(&app_name, serve_handle.port);
    let ws_app = app.handle().clone();

    #[cfg(feature = "llm")]
    {
        let cell = app.state::<Mutex<Box<AppState>>>()
            .lock().unwrap().llm.state_cell.clone();
        serve_handle.set_llm(cell);
    }

    tauri::async_runtime::spawn(async move { serve_handle.serve(ws_app).await; });
    app.manage(broadcaster);

    let logger_arc = {
        let r = app.state::<Mutex<Box<AppState>>>();
        let g = r.lock_or_recover();
        g.logger.clone()
    };
    app.manage(logger_arc);

    // Route TTS and LLM log output through the unified SkillLogger.
    crate::tts::init_tts_logger(app.handle());
    crate::llm::init_llm_logger(app.handle());

    let skill_dir = {
        let r = app.state::<Mutex<Box<AppState>>>();
        let g = r.lock_or_recover();
        g.skill_dir.clone()
    };
    let data = load_settings(&skill_dir);
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        s.status.paired_devices         = data.paired.clone();
        s.preferred_id                  = data.preferred_id.clone();
        s.status.filter_config          = data.filter_config;
        s.status.embedding_overlap_secs = data.embedding_overlap_secs;
        s.label_shortcut                = data.label_shortcut;
        s.search_shortcut               = data.search_shortcut;
        s.settings_shortcut             = data.settings_shortcut;
        s.calibration_shortcut          = data.calibration_shortcut;
        s.help_shortcut                 = data.help_shortcut;
        s.history_shortcut              = data.history_shortcut;
        s.api_shortcut                  = data.api_shortcut;
        s.theme_shortcut                = data.theme_shortcut;
        s.focus_timer_shortcut          = data.focus_timer_shortcut;
        let mut profiles = data.calibration_profiles;
        if profiles.is_empty() {
            profiles.push(CalibrationProfile::from_legacy(&data.calibration));
        }
        s.calibration_profiles = profiles;
        s.active_calibration_id = if data.active_calibration_id.is_empty() {
            s.calibration_profiles.first().map(|p| p.id.clone()).unwrap_or_default()
        } else {
            data.active_calibration_id
        };
        s.onboarding_complete                = data.onboarding_complete;
        s.last_seen_whats_new_version        = data.last_seen_whats_new_version;
        s.theme                        = data.theme;
        s.language                     = data.language;
        s.daily_goal_min               = data.daily_goal_min;
        s.goal_notified_date           = data.goal_notified_date;
        s.text_embedding_model         = data.text_embedding_model.clone();
        s.hooks                        = data.hooks;
        s.ws_host                      = data.ws_host.clone();
        s.ws_port                      = data.ws_port;
        s.update_check_interval_secs   = data.update_check_interval_secs;
        s.openbci_config               = data.openbci;
        s.neutts_config                = data.neutts.clone();
        s.tts_preload                  = data.tts_preload;
        s.track_active_window          = data.track_active_window;
        s.track_input_activity         = data.track_input_activity;
        s.input_activity_enabled
            .store(data.track_input_activity, std::sync::atomic::Ordering::Relaxed);
        s.dnd_config  = data.do_not_disturb;
        s.llm.config  = data.llm;
        s.screenshot_config = data.screenshot;
        if let Some(os_active) = crate::dnd::query_os_active() {
            if !os_active { s.dnd_active = false; }
        }
        neutts_apply_config(&data.neutts);
        for pd in &data.paired {
            s.discovered.push(DiscoveredDevice {
                id: pd.id.clone(), name: pd.name.clone(),
                last_seen: pd.last_seen, last_rssi: 0,
                is_paired: true,
                is_preferred: data.preferred_id.as_deref() == Some(&pd.id),
            });
        }
    }

    if data.tts_preload {
        let app_handle = app.handle().clone();
        tauri::async_runtime::spawn(async move {
            crate::tts::tts_init(app_handle).await.ok();
        });
    }

    // Auto-start the LLM server if configured and a model is available.
    let llm_autostart = {
        let r = app.state::<Mutex<Box<AppState>>>();
        let s = r.lock_or_recover();
        s.llm.config.enabled && s.llm.config.autostart
    };
    if llm_autostart {
        #[cfg(feature = "llm")]
        {
            let has_model = {
                let r = app.state::<Mutex<Box<AppState>>>();
                let s = r.lock_or_recover();
                s.llm.config.model_path.as_ref().map(|p| p.exists()).unwrap_or(false)
            };
            if has_model {
                let app_handle = app.handle().clone();
                // Small delay so the main window can render first.
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let state = app_handle.state::<Mutex<Box<AppState>>>();
                    crate::llm::cmds::start_llm_server(app_handle.clone(), state).ok();
                });
            }
        }
    }

    {
        let model_code = {
            let r = app.state::<Mutex<Box<AppState>>>();
            let g = r.lock_or_recover();
            g.text_embedding_model.clone()
        };
        let skill_dir_emb = skill_dir.clone();
        let embedder_arc  = std::sync::Arc::clone(
            &*app.state::<std::sync::Arc<EmbedderState>>()
        );
        let logger_emb = app.state::<std::sync::Arc<SkillLogger>>().inner().clone();
        std::thread::spawn(move || {
            init_embedder(&embedder_arc, &model_code, &skill_dir_emb, &logger_emb);
        });
    }

    {
        let label_idx = std::sync::Arc::clone(
            &*app.state::<std::sync::Arc<label_index::LabelIndexState>>()
        );
        let sd = skill_dir.clone();
        std::thread::spawn(move || label_idx.load(&sd));
    }

    // ── Startup weights probe ─────────────────────────────────────────
    {
        let model_status = {
            let r = app.state::<Mutex<Box<AppState>>>();
            let g = r.lock_or_recover();
            g.model_status.clone()
        };
        let hf_repo = {
            let r = app.state::<Mutex<Box<AppState>>>();
            let g = r.lock_or_recover();
            g.model_config.hf_repo.clone()
        };
        std::thread::Builder::new()
            .name("weights-probe".into())
            .spawn(move || {
                if let Some((w, _c)) = crate::eeg_embeddings::probe_hf_weights(&hf_repo) {
                    let mut st = model_status.lock_or_recover();
                    st.weights_found  = true;
                    st.weights_path   = Some(w.display().to_string());
                    eprintln!("[embedder] startup probe: weights found at {}", w.display());
                } else {
                    eprintln!("[embedder] startup probe: weights not found in HuggingFace cache");
                }
            })
            .expect("[weights-probe] failed to spawn");
    }

    // ── Global cross-day EEG HNSW index ──────────────────────────────
    {
        let global_arc = std::sync::Arc::clone(
            &*app.state::<std::sync::Arc<GlobalEegIndex>>()
        );
        let sd = skill_dir.clone();
        std::thread::Builder::new()
            .name("global-hnsw-build".into())
            .spawn(move || {
                let idx = global_eeg_index::load_or_build(&sd);
                *global_arc.0.lock_or_recover() = Some(idx);
                eprintln!("[global_idx] ready — embed worker will insert new epochs incrementally");
            })
            .expect("[global_idx] failed to spawn build thread");
    }

    if let Err(e) = apply_all_shortcuts(app.handle()) {
        eprintln!("[shortcut] failed to register shortcuts: {e}");
    }

    #[cfg(target_os = "macos")]
    {
        use tauri::menu::{MenuBuilder, SubmenuBuilder, MenuItem, PredefinedMenuItem};
        let app_submenu = SubmenuBuilder::new(app, constants::APP_DISPLAY_NAME)
            .item(&MenuItem::with_id(
                app, "about",
                format!("About {}", constants::APP_DISPLAY_NAME),
                true, None::<&str>,
            )?)
            .separator()
            .item(&PredefinedMenuItem::hide(app, None)?)
            .item(&PredefinedMenuItem::hide_others(app, None)?)
            .item(&PredefinedMenuItem::show_all(app, None)?)
            .separator()
            .item(&MenuItem::with_id(
                app, "macos_quit",
                format!("Quit {}", constants::APP_DISPLAY_NAME),
                true, Some("Cmd+Q"),
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
        let r = app.state::<Mutex<Box<AppState>>>();
        let g = r.lock_or_recover();
        g.status.clone()
    };
    let init_menu = build_menu(app.handle(), &init_status)?;

    /// Main-window recovery helper.
    fn show_and_recover_main(app: &AppHandle) {
        let win = if let Some(win) = app.get_webview_window("main") {
            win
        } else {
            match tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("".into()),
            )
            .title(constants::APP_DISPLAY_NAME)
            .decorations(false).transparent(true)
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
                {
                    let r = app.state::<Mutex<Box<AppState>>>();
                    let mut s = r.lock_or_recover();
                    s.pending_reconnect = false;
                    s.retry_attempt = 0;
                }
                cancel_session(app);
            } else if id == "scan" || id == "retry" {
                start_session(app, None);
            } else if id == "open_bt" {
                open_bt_settings();
            } else if id == "calibrate" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_calibration_window_inner(&a, None, false).await;
                });
            } else if id == "search" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move { let _ = open_search_window(a).await; });
            } else if id == "label" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move { let _ = open_label_window(a).await; });
            } else if id == "history" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move { let _ = open_history_window(a).await; });
            } else if id == "compare" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move { let _ = open_compare_window(a).await; });
            } else if id == "settings" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move { let _ = open_settings_window(a).await; });
            } else if id == "help" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move { let _ = open_help_window(a).await; });
            } else if id == "api" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move { let _ = open_api_window(a).await; });
            } else if id == "chat" {
                #[cfg(feature = "llm")] {
                    let a = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = open_chat_window(a).await;
                    });
                }
            } else if id == "downloads" {
                #[cfg(feature = "llm")] {
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
            } else if id == "check_update" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move { let _ = open_updates_window(a).await; });
            } else if id == "quit" {
                confirm_and_quit(app.app_handle().clone());
            } else if let Some(dev_id) = id.strip_prefix("connect:") {
                start_session(app, Some(dev_id.to_owned()));
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
        start_background_scanner(&app_scan);
    });

    let app_auto = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(900)).await;
        let preferred = {
            let r = app_auto.state::<Mutex<Box<AppState>>>();
            let mut s = r.lock_or_recover();
            let pref = s.preferred_id.clone()
                .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()));
            if pref.is_some() { s.pending_reconnect = true; }
            pref
        };
        start_session(&app_auto, preferred);
    });

    let app_cal = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let auto_start_id: Option<String> = {
            let r = app_cal.state::<Mutex<Box<AppState>>>();
            let s = r.lock_or_recover();
            let active_id = &s.active_calibration_id;
            s.calibration_profiles.iter()
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
            g.onboarding_complete
        };
        if !done { let _ = open_onboarding_window(app_onboard).await; }
    });

    {
        let (act_store, kbd_ts, mouse_ts, input_flag, kbd_cnt, mouse_cnt) = {
            let state_ref = app.state::<Mutex<Box<AppState>>>();
            let s = state_ref.lock_or_recover();
            (
                s.activity_store.clone(),
                s.last_keyboard_ts.clone(),
                s.last_mouse_ts.clone(),
                s.input_activity_enabled.clone(),
                s.kbd_event_count.clone(),
                s.mouse_event_count.clone(),
            )
        };
        if let Some(store) = act_store.clone() {
            let app_win = app.handle().clone();
            std::thread::Builder::new()
                .name("active-window-poll".into())
                .spawn(move || active_window::run_poller(app_win, store))
                .expect("[active-window] failed to spawn poll thread");
        }
        if let Some(store) = act_store {
            let app_inp = app.handle().clone();
            std::thread::Builder::new()
                .name("input-monitor".into())
                .spawn(move || {
                    active_window::run_input_monitor(
                        app_inp, input_flag, kbd_ts, mouse_ts,
                        kbd_cnt, mouse_cnt, store,
                    );
                })
                .expect("[input-monitor] failed to spawn thread");
        }
    }

    // ── Screenshot store + capture worker ──────────────────────────────
    {
        let ss_store = screenshot_store::ScreenshotStore::open(&skill_dir)
            .map(std::sync::Arc::new);
        {
            let r = app.state::<Mutex<Box<AppState>>>();
            r.lock_or_recover().screenshot_store = ss_store.clone();
        }
        let app_ss = app.handle().clone();
        let sd = skill_dir.clone();
        let ss_metrics = app.state::<Mutex<Box<AppState>>>()
            .lock_or_recover().screenshot_metrics.clone();
        std::thread::Builder::new()
            .name("screenshot-worker".into())
            .spawn(move || {
                let ctx: std::sync::Arc<dyn skill_screenshots::ScreenshotContext> =
                    std::sync::Arc::new(screenshot::TauriScreenshotContext { app: app_ss });
                screenshot::run_screenshot_worker(ctx, sd, ss_store, ss_metrics)
            })
            .expect("[screenshot] failed to spawn worker thread");
    }

    setup_background_tasks(app);
    Ok(())
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
            if updater_platform_unsupported { break; }
            eprintln!("[updater] running background update check");
            match app_upd.updater() {
                Err(e) => eprintln!("[updater] cannot get updater: {e}"),
                Ok(updater) => {
                    let result = tokio::time::timeout(
                        Duration::from_secs(30), updater.check(),
                    ).await;
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
            let sleep_secs = if interval_secs == 0 { 60 } else { interval_secs };
            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
        }
    });

    // ── Background OS DND poll ────────────────────────────────────────
    let app_dnd = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        loop {
            let os_now = crate::dnd::query_os_active();

            let (prev, app_active) = {
                let r = app_dnd.state::<Mutex<Box<AppState>>>();
                let g = r.lock_or_recover();
                (g.dnd_os_active, g.dnd_active)
            };

            if os_now != prev {
                {
                    let r = app_dnd.state::<Mutex<Box<AppState>>>();
                    r.lock_or_recover().dnd_os_active = os_now;
                }

                let payload = serde_json::json!({ "os_active": os_now });
                let _ = app_dnd.emit("dnd-os-changed", &payload);
                app_dnd.state::<WsBroadcaster>().send("dnd-os-changed", &payload);

                if os_now == Some(false) && app_active {
                    eprintln!(
                        "[dnd] OS DND was externally cleared while \
                         app believed it was active — reconciling"
                    );
                    {
                        let r = app_dnd.state::<Mutex<Box<AppState>>>();
                        let mut g = r.lock_or_recover();
                        g.dnd_active      = false;
                        g.dnd_below_ticks = 0;
                        g.dnd_focus_samples.clear();
                    }
                    let _ = app_dnd.emit("dnd-state-changed", false);
                    app_dnd.state::<WsBroadcaster>()
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
        .manage(std::sync::Arc::new(EmbedderState(std::sync::Mutex::new(None))))
        .manage(std::sync::Arc::new(label_index::LabelIndexState::new()))
        .manage(std::sync::Arc::new(GlobalEegIndex::new()))
        .setup(|app| { setup_app(app) })
        .invoke_handler(tauri::generate_handler![
            subscribe_eeg, subscribe_ppg, subscribe_imu,
            get_status, get_devices,
            set_preferred_device, pair_device, forget_device, retry_connect, cancel_retry,
            open_bt_settings, open_settings_window, open_updates_window, open_model_tab, open_help_window,
            check_accessibility_permission, open_accessibility_settings, open_notifications_settings,
            check_screen_recording_permission, open_screen_recording_settings,
            get_filter_config, set_filter_config, set_notch_preset,
            get_latest_bands,
            get_embedding_overlap, set_embedding_overlap,
            get_gpu_stats,
            get_log_config, set_log_config,
            get_eeg_model_config, set_eeg_model_config, get_eeg_model_status,
            trigger_weights_download, cancel_weights_download,
            get_umap_config, set_umap_config,
            get_theme_and_language, set_theme, set_language,
            get_accent_color, set_accent_color,
            get_daily_goal, set_daily_goal,
            get_goal_notified_date, set_goal_notified_date,
            get_daily_recording_mins,
            get_hooks, set_hooks, get_hook_statuses, open_session_for_timestamp,
            suggest_hook_distances, suggest_hook_keywords, get_hook_log, get_hook_log_count,
            quit_app, open_label_window, open_labels_window, open_focus_timer_window,
            submit_label, close_label_window,
            query_annotations, get_recent_labels, delete_label, update_label, get_queue_stats,
            list_embedding_models, get_embedding_model, set_embedding_model,
            reembed_all_labels, get_stale_label_count,
            rebuild_label_index, search_labels_by_text, search_labels_by_eeg,
            open_search_window,
            open_history_window, list_sessions, list_session_days, list_sessions_for_day,
            stream_sessions, get_history_stats, delete_session,
            open_compare_window, open_compare_window_with_sessions,
            get_session_metrics, get_session_timeseries, get_csv_metrics, get_day_metrics_batch,
            list_embedding_sessions, get_sleep_stages,
            compute_umap_compare, enqueue_umap_compare, poll_job,
            get_label_shortcut, set_label_shortcut,
            get_search_shortcut, set_search_shortcut,
            get_settings_shortcut, set_settings_shortcut,
            get_calibration_shortcut, set_calibration_shortcut,
            get_help_shortcut, set_help_shortcut,
            get_history_shortcut, set_history_shortcut,
            get_api_shortcut, set_api_shortcut,
            get_theme_shortcut, set_theme_shortcut,
            get_focus_timer_shortcut, set_focus_timer_shortcut,
            open_calibration_window, open_and_start_calibration, close_calibration_window,
            list_calibration_profiles, get_calibration_profile, get_active_calibration,
            set_active_calibration, create_calibration_profile, update_calibration_profile,
            delete_calibration_profile, record_calibration_completed,
            get_calibration_config, set_calibration_config,
            emit_calibration_event,
            get_app_version, get_app_name,
            get_data_dir, set_data_dir, open_skill_dir,
            get_ws_clients, get_ws_request_log, get_ws_port,
            get_ws_config, set_ws_config,
            get_autostart_enabled, set_autostart_enabled,
            get_update_check_interval, set_update_check_interval,
            get_openbci_config, set_openbci_config, list_serial_ports,
            get_neutts_config, set_neutts_config, pick_ref_wav_file,
            get_tts_preload, set_tts_preload,
            get_active_window_tracking, set_active_window_tracking, get_active_window,
            get_input_activity_tracking, set_input_activity_tracking,
            get_last_input_activity,
            get_recent_active_windows, get_recent_input_activity,
            get_input_buckets,
            get_dnd_config, set_dnd_config, get_dnd_active, get_dnd_status, test_dnd, list_focus_modes,
            get_llm_config, set_llm_config, pick_gguf_file,
            // LLM catalog (compiled in regardless; no-op stubs when `llm` feature absent)
            #[cfg(feature = "llm")]
            get_llm_catalog,
            #[cfg(feature = "llm")]
            download_llm_model,
            #[cfg(feature = "llm")]
            cancel_llm_download,
            #[cfg(feature = "llm")]
            pause_llm_download,
            #[cfg(feature = "llm")]
            resume_llm_download,
            #[cfg(feature = "llm")]
            get_llm_downloads,
            #[cfg(feature = "llm")]
            delete_llm_model,
            #[cfg(feature = "llm")]
            refresh_llm_catalog,
            #[cfg(feature = "llm")]
            set_llm_active_model,
            #[cfg(feature = "llm")]
            set_llm_active_mmproj,
            #[cfg(feature = "llm")]
            set_llm_autoload_mmproj,
            #[cfg(feature = "llm")]
            add_llm_model,
            #[cfg(feature = "llm")]
            get_llm_logs,
            #[cfg(feature = "llm")]
            start_llm_server,
            #[cfg(feature = "llm")]
            stop_llm_server,
            #[cfg(feature = "llm")]
            switch_llm_model,
            #[cfg(feature = "llm")]
            get_llm_server_status,
            #[cfg(feature = "llm")]
            open_chat_window,
            #[cfg(feature = "llm")]
            open_downloads_window,
            #[cfg(feature = "llm")]
            get_last_chat_session,
            #[cfg(feature = "llm")]
            load_chat_session,
            #[cfg(feature = "llm")]
            list_chat_sessions,
            #[cfg(feature = "llm")]
            rename_chat_session,
            #[cfg(feature = "llm")]
            delete_chat_session,
            #[cfg(feature = "llm")]
            archive_chat_session,
            #[cfg(feature = "llm")]
            unarchive_chat_session,
            #[cfg(feature = "llm")]
            list_archived_chat_sessions,
            #[cfg(feature = "llm")]
            save_chat_message,
            #[cfg(feature = "llm")]
            save_chat_tool_calls,
            #[cfg(feature = "llm")]
            new_chat_session,
            #[cfg(feature = "llm")]
            get_chat_shortcut,
            #[cfg(feature = "llm")]
            set_chat_shortcut,
            #[cfg(feature = "llm")]
            chat_completions_ipc,
            #[cfg(feature = "llm")]
            abort_llm_stream,
            #[cfg(feature = "llm")]
            cancel_tool_call,
            #[cfg(feature = "llm")]
            get_model_hardware_fit,
            get_screenshot_config, set_screenshot_config,
            estimate_screenshot_reembed, rebuild_screenshot_embeddings,
            get_screenshots_around, search_screenshots_by_vector,
            search_screenshots_by_image,
            search_screenshots_by_text,
            check_ocr_models_ready, download_ocr_models,
            get_screenshot_metrics, get_screenshots_dir,
            tts_unload, tts_get_voice, tts_list_neutts_voices,
            session_connect::connect_openbci,
            open_api_window,
            open_whats_new_window,
            get_whats_new_seen_version, dismiss_whats_new,
            open_onboarding_window, get_onboarding_model_download_order,
            complete_onboarding, get_onboarding_complete,
            commands::search_embeddings,
            global_eeg_index::get_global_index_stats,
            global_eeg_index::rebuild_global_eeg_index,
            commands::enqueue_search_embeddings,
            commands::stream_search_embeddings,
            commands::find_session_for_timestamp,
            commands::interactive_search,
            commands::save_dot_file,
            commands::save_svg_file,
            open_session_window,
            tts_init, tts_speak, tts_list_voices, tts_set_voice,
            get_about_info, open_about_window,
            show_main_window,
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
                            eprintln!("[window-event] label={label} Resized({}x{})", size.width, size.height);
                        }
                        tauri::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                            eprintln!("[window-event] label={label} ScaleFactorChanged({scale_factor})");
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
                        eprintln!("[run-event] explicit exit requested — running blocking shutdown");
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

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::retry_delay_secs;

    #[test]
    fn backoff_schedule_1_2_3_5() {
        assert_eq!(retry_delay_secs(0), 1, "attempt 0 → 1 s");
        assert_eq!(retry_delay_secs(1), 2, "attempt 1 → 2 s");
        assert_eq!(retry_delay_secs(2), 3, "attempt 2 → 3 s");
        assert_eq!(retry_delay_secs(3), 5, "attempt 3 → 5 s");
    }

    #[test]
    fn backoff_capped_at_5s() {
        for attempt in 3u32..=100 {
            assert_eq!(retry_delay_secs(attempt), 5,
                "attempt {attempt} should be capped at 5 s");
        }
    }
}
