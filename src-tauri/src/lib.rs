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

mod exg_embeddings;
mod global_eeg_index;
use global_eeg_index::GlobalEegIndex;

mod session_dsp;
pub(crate) use session_dsp::SessionDsp;

mod lifecycle;
pub(crate) use lifecycle::{cancel_session, go_disconnected, start_session};

mod quit;
pub(crate) use quit::confirm_and_quit;

mod commands;
mod job_queue;

mod api;
mod label_index;
mod ws_commands;
mod ws_server;

mod screenshot;

/// OpenAI-compatible LLM inference server — same port as WebSocket API.
/// Enabled by the `llm` Cargo feature; no-op when the feature is absent.
#[cfg(feature = "llm")]
mod llm;

use ws_server::WsBroadcaster;

// ── New extracted modules ─────────────────────────────────────────────────────

/// CSV recording (CsvState, path helpers, session-metadata sidecar).
mod session_csv;

/// Background device scanner — BLE, USB serial, Emotiv Cortex, WiFi.
mod device_scanner;
pub(crate) use device_scanner::start_background_scanner;

/// Generic device session runner (replaces per-device session modules).
mod secondary_session;
mod session_runner;

/// Per-device scan / connect factories → `Box<dyn DeviceAdapter>`.
mod session_connect;

/// Session history listing and streaming Tauri commands.
mod history_cmds;
use history_cmds::{
    delete_session, get_history_stats, list_all_sessions, list_embedding_sessions,
    list_local_session_days, list_session_days, list_sessions, list_sessions_for_day,
    list_sessions_for_local_day, open_history_window, stream_sessions,
};

/// Session metrics, time-series, sleep staging, UMAP and compare commands.
mod session_analysis;
pub(crate) use session_analysis::{
    analyze_search_results, analyze_sleep_stages, compute_compare_insights, compute_status_history,
    get_session_metrics_impl, get_sleep_stages_impl, load_embeddings_range,
};
use session_analysis::{
    compute_umap_compare, enqueue_umap_compare, get_csv_metrics, get_day_metrics_batch,
    get_session_embedding_count, get_session_location, get_session_metrics, get_session_timeseries,
    get_sleep_stages, open_compare_window, open_compare_window_with_sessions, poll_job,
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
    default_skill_dir, load_settings, load_umap_config, new_profile_id, CalibrationAction,
    CalibrationConfig, CalibrationProfile,
};

mod tray;
pub(crate) use tray::{build_menu, icon_disconnected, refresh_tray};

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
    get_onboarding_model_download_order, get_whats_new_seen_version, get_ws_clients, get_ws_port,
    get_ws_request_log, is_session_live, list_calibration_profiles, open_accessibility_settings,
    open_and_start_calibration, open_api_window, open_bt_settings, open_calendar_settings,
    open_calibration_window, open_focus_settings, open_focus_timer_window, open_help_window,
    open_input_monitoring_settings, open_label_window, open_label_window_at, open_labels_window,
    open_location_settings, open_model_tab, open_notifications_settings, open_onboarding_window,
    open_screen_recording_settings, open_search_window, open_session_window, open_settings_window,
    open_skill_dir, open_updates_window, open_whats_new_window, quit_app,
    record_calibration_completed, request_calendar_permission, request_location_permission,
    set_active_calibration, set_calibration_config, set_data_dir, set_update_ready,
    show_main_window, update_calibration_profile,
};

mod label_cmds;
use label_cmds::{
    delete_label, get_embedding_model, get_queue_stats, get_recent_labels, get_stale_label_count,
    list_embedding_models, query_annotations, rebuild_label_index, reembed_all_labels,
    search_labels_by_eeg, search_labels_by_text, set_embedding_model, submit_label, update_label,
};
pub(crate) use label_cmds::{init_embedder, EmbedderState};

mod settings_cmds;
use settings_cmds::{
    cancel_retry, cancel_weights_download, check_ocr_models_ready, download_ocr_models,
    estimate_reembed, estimate_screenshot_reembed, forget_device, get_accent_color,
    get_active_window, get_active_window_tracking, get_api_token, get_autostart_enabled,
    get_cortex_ws_state, get_daily_goal, get_daily_recording_mins, get_device_api_config,
    get_device_capabilities, get_device_log, get_devices, get_disabled_skills, get_dnd_active,
    get_dnd_config, get_dnd_status, get_eeg_model_config, get_eeg_model_status,
    get_embedding_overlap, get_exg_catalog, get_exg_inference_device, get_filter_config,
    get_goal_notified_date, get_gpu_stats, get_hook_log, get_hook_log_count, get_hook_statuses,
    get_hooks, get_inference_device, get_input_activity_tracking, get_input_buckets,
    get_last_input_activity, get_latest_bands, get_llm_config, get_location_enabled,
    get_log_config, get_main_window_auto_fit, get_neutts_config, get_openbci_config,
    get_recent_active_windows, get_recent_input_activity, get_scanner_config,
    get_screenshot_config, get_screenshot_metrics, get_screenshots_around, get_screenshots_dir,
    get_skills_last_sync, get_skills_license, get_skills_refresh_interval,
    get_skills_sync_on_launch, get_sleep_config, get_status, get_storage_format,
    get_supported_companies, get_theme_and_language, get_tts_preload, get_umap_config,
    get_update_check_interval, get_ws_config, list_focus_modes, list_serial_ports, list_skills,
    open_session_for_timestamp, pair_device, pick_exg_weights_file, pick_gguf_file,
    pick_ref_wav_file, rebuild_screenshot_embeddings, retry_connect, search_screenshots_by_image,
    search_screenshots_by_text, search_screenshots_by_vector, set_accent_color,
    set_active_window_tracking, set_api_token, set_autostart_enabled, set_daily_goal,
    set_device_api_config, set_disabled_skills, set_dnd_config, set_eeg_model_config,
    set_embedding_overlap, set_exg_inference_device, set_filter_config, set_goal_notified_date,
    set_hooks, set_inference_device, set_input_activity_tracking, set_language, set_llm_config,
    set_location_enabled, set_log_config, set_main_window_auto_fit, set_neutts_config,
    set_notch_preset, set_openbci_config, set_preferred_device, set_scanner_config,
    set_screenshot_config, set_skills_refresh_interval, set_skills_sync_on_launch,
    set_sleep_config, set_storage_format, set_theme, set_tts_preload, set_umap_config,
    set_update_check_interval, set_ws_config, subscribe_eeg, subscribe_imu, subscribe_ppg,
    suggest_hook_distances, suggest_hook_keywords, sync_skills_now, test_dnd, test_location,
    trigger_reembed, trigger_weights_download, web_cache_clear, web_cache_list,
    web_cache_remove_domain, web_cache_remove_entry, web_cache_stats,
};

// LLM catalog commands (feature-gated)
#[cfg(feature = "llm")]
use llm::cmds::{
    abort_llm_stream, add_llm_model, archive_chat_session, cancel_llm_download, cancel_tool_call,
    chat_completions_ipc, delete_chat_session, delete_llm_model, download_llm_model,
    get_last_chat_session, get_llm_catalog, get_llm_downloads, get_llm_logs, get_llm_server_status,
    get_model_hardware_fit, get_session_params, list_archived_chat_sessions, list_chat_sessions,
    load_chat_session, new_chat_session, open_chat_window, open_downloads_window,
    pause_llm_download, refresh_llm_catalog, rename_chat_session, resume_llm_download,
    save_chat_message, save_chat_tool_calls, set_llm_active_mmproj, set_llm_active_model,
    set_llm_autoload_mmproj, set_session_params, start_llm_server, stop_llm_server,
    switch_llm_mmproj, switch_llm_model, unarchive_chat_session,
};

// ── Imports ───────────────────────────────────────────────────────────────────

use std::{sync::Mutex, time::Duration};

use tauri::{tray::TrayIconBuilder, AppHandle, Emitter, Manager};

// ── Core types (re-exported from state.rs) ────────────────────────────────────

mod state;
pub(crate) use state::*;

// ── Shared helpers (re-exported from helpers.rs) ──────────────────────────────

mod helpers;
pub(crate) use helpers::{
    emit_devices, emit_status, mutate_and_save, read_state, save_settings, save_settings_handle,
    save_settings_now, send_toast, set_cortex_ws_state, skill_dir, unix_secs, upsert_discovered,
    upsert_paired, yyyymmdd_utc, AppStateExt, ToastLevel,
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
        let cell = app
            .state::<Mutex<Box<AppState>>>()
            .lock_or_recover()
            .llm
            .lock_or_recover()
            .state_cell
            .clone();
        llm::shutdown_cell(&cell);
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

    let app_name = app.package_info().name.to_lowercase();
    let (ws_cfg, skill_dir_for_iroh) = {
        let dir = app.app_state().lock_or_recover().skill_dir.clone();
        let s = load_settings(&dir);
        ((s.ws_host, s.ws_port), dir)
    };

    // ── LLM server (optional, same port) ──────────────────────────────
    #[cfg(feature = "llm")]
    {
        let (llm_cfg, catalog, log_buf, cell, skill_dir) = {
            let dir = app.app_state().lock_or_recover().skill_dir.clone();
            let llm_cfg = load_settings(&dir).llm;
            let llm_arc = app.app_state().lock_or_recover().llm_arc();
            let llm = llm_arc.lock_or_recover();
            let sd = app.app_state().lock_or_recover().skill_dir.clone();
            (
                llm_cfg,
                llm.catalog.clone(),
                llm.logs.clone(),
                llm.state_cell.clone(),
                sd,
            )
        };
        if llm_cfg.enabled {
            let app_handle = app.handle().clone();
            let cell2 = cell.clone();
            std::thread::spawn(move || {
                let emitter: std::sync::Arc<dyn llm::LlmEventEmitter> =
                    std::sync::Arc::new(llm::TauriEmitter(app_handle));
                if let Some(state) = llm::init(&llm_cfg, &catalog, emitter, log_buf, &skill_dir) {
                    *cell2.lock_or_recover() = Some(state);
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
        let cell = app
            .app_state()
            .lock_or_recover()
            .llm
            .lock_or_recover()
            .state_cell
            .clone();
        serve_handle.set_llm(cell.clone());

        // Propagate the actual WS port to the Skill API tool so the LLM
        // can call back into the server via HTTP.
        let ws_port = serve_handle.port;
        let location_enabled = app.app_state().lock_or_recover().location_enabled;
        std::thread::spawn(move || {
            // Wait briefly for the LLM server to initialise, then set the port.
            for _ in 0..60 {
                if let Some(ref server) = *cell.lock_or_recover() {
                    let mut tools = server.allowed_tools.lock_or_recover();
                    tools.skill_api_port = ws_port;
                    // Gate location tool on location_enabled setting.
                    if !location_enabled {
                        tools.location = false;
                    }
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        });
    }

    let ws_port = serve_handle.port;
    let (ws_shutdown_tx, ws_shutdown_rx) = tokio::sync::watch::channel(false);
    let ws_task = tauri::async_runtime::spawn(async move {
        serve_handle
            .serve_with_mode(ws_app, false, Some(ws_shutdown_rx))
            .await;
    });
    let ws_control: ws_server::SharedWsControl = std::sync::Arc::new(std::sync::Mutex::new(Some(
        ws_server::WsServerControl::new(ws_shutdown_tx, ws_task),
    )));
    app.manage(ws_control);

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
    let (llm_autostart, llm_has_model, model_code, model_status, hf_repo) = {
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
            s.ui.text_embedding_model.clone(),
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

    {
        let skill_dir_emb = skill_dir.clone();
        let embedder_arc = std::sync::Arc::clone(&*app.state::<std::sync::Arc<EmbedderState>>());
        let logger_emb = app.state::<std::sync::Arc<SkillLogger>>().inner().clone();
        std::thread::spawn(move || {
            init_embedder(&embedder_arc, &model_code, &skill_dir_emb, &logger_emb);
        });
    }

    {
        let label_idx =
            std::sync::Arc::clone(&*app.state::<std::sync::Arc<label_index::LabelIndexState>>());
        let sd = skill_dir.clone();
        std::thread::spawn(move || label_idx.load(&sd));
    }

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

    // ── Global cross-day EEG HNSW index ──────────────────────────────
    {
        let global_arc = std::sync::Arc::clone(&*app.state::<std::sync::Arc<GlobalEegIndex>>());
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
                {
                    let r = app.app_state();
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
            } else if id == "check_update" {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = open_updates_window(a).await;
                });
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
        // Start LSL auto-scanner if enabled in settings
        settings_cmds::lsl_cmds::maybe_start_lsl_auto_scanner(&app_scan);
    });

    // Watch for incoming EEG data from remote devices over iroh
    // and auto-start a recording session when data arrives.
    let app_iroh_eeg = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        crate::lifecycle::spawn_iroh_eeg_watcher(&app_iroh_eeg);
    });

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
        if preferred.is_some() {
            start_session(&app_auto, preferred);
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

    {
        let (act_store, kbd_ts, mouse_ts, input_flag, kbd_cnt, mouse_cnt) = {
            let state_ref = app.app_state();
            let s = state_ref.lock_or_recover();
            (
                s.input.activity_store.clone(),
                s.input.last_keyboard_ts.clone(),
                s.input.last_mouse_ts.clone(),
                s.input.input_activity_enabled.clone(),
                s.input.kbd_event_count.clone(),
                s.input.mouse_event_count.clone(),
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
                        app_inp, input_flag, kbd_ts, mouse_ts, kbd_cnt, mouse_cnt, store,
                    );
                })
                .expect("[input-monitor] failed to spawn thread");
        }
    }

    // ── Screenshot store + capture worker ──────────────────────────────
    {
        let ss_store = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir)
            .map(std::sync::Arc::new);
        {
            let r = app.app_state();
            r.lock_or_recover().screenshot_store = ss_store.clone();
        }
        let app_ss = app.handle().clone();
        let sd = skill_dir.clone();
        let ss_metrics = app.app_state().lock_or_recover().screenshot_metrics.clone();
        std::thread::Builder::new()
            .name("screenshot-worker".into())
            .spawn(move || {
                let ctx: std::sync::Arc<dyn skill_screenshots::ScreenshotContext> =
                    std::sync::Arc::new(screenshot::TauriScreenshotContext { app: app_ss });
                screenshot::run_screenshot_worker(ctx, sd, ss_store, ss_metrics);
            })
            .expect("[screenshot] failed to spawn worker thread");
    }

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
        s.update_check_interval_secs = data.update_check_interval_secs;
        s.openbci_config = data.openbci;
        s.device_api_config = data.device_api;
        s.scanner_config = data.scanner;
        s.location_enabled = data.location_enabled;
        s.lsl_auto_connect = data.lsl_auto_connect;
        s.lsl_paired_streams = data.lsl_paired_streams;
        s.lsl_idle_timeout_secs = data.lsl_idle_timeout_secs;
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
        .manage(std::sync::Arc::new(EmbedderState(std::sync::Mutex::new(
            None,
        ))))
        .manage(std::sync::Arc::new(label_index::LabelIndexState::new()))
        .manage(std::sync::Arc::new(GlobalEegIndex::new()))
        .setup(|app| setup_app(app))
        .invoke_handler(tauri::generate_handler![
            subscribe_eeg,
            subscribe_ppg,
            subscribe_imu,
            get_status,
            get_devices,
            get_supported_companies,
            get_device_capabilities,
            set_preferred_device,
            pair_device,
            forget_device,
            retry_connect,
            cancel_retry,
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
            get_filter_config,
            set_filter_config,
            set_notch_preset,
            get_storage_format,
            set_storage_format,
            get_latest_bands,
            get_embedding_overlap,
            set_embedding_overlap,
            get_gpu_stats,
            get_log_config,
            set_log_config,
            get_eeg_model_config,
            set_eeg_model_config,
            get_eeg_model_status,
            get_exg_catalog,
            trigger_weights_download,
            cancel_weights_download,
            estimate_reembed,
            trigger_reembed,
            get_umap_config,
            set_umap_config,
            get_theme_and_language,
            set_theme,
            set_language,
            get_accent_color,
            set_accent_color,
            get_daily_goal,
            set_daily_goal,
            get_goal_notified_date,
            set_goal_notified_date,
            get_main_window_auto_fit,
            set_main_window_auto_fit,
            get_daily_recording_mins,
            get_hooks,
            set_hooks,
            get_hook_statuses,
            open_session_for_timestamp,
            suggest_hook_distances,
            suggest_hook_keywords,
            get_hook_log,
            get_hook_log_count,
            quit_app,
            open_label_window,
            open_label_window_at,
            open_labels_window,
            open_focus_timer_window,
            submit_label,
            close_label_window,
            query_annotations,
            get_recent_labels,
            delete_label,
            update_label,
            get_queue_stats,
            list_embedding_models,
            get_embedding_model,
            set_embedding_model,
            reembed_all_labels,
            get_stale_label_count,
            rebuild_label_index,
            search_labels_by_text,
            search_labels_by_eeg,
            open_search_window,
            open_history_window,
            list_sessions,
            list_session_days,
            list_sessions_for_day,
            list_all_sessions,
            list_local_session_days,
            list_sessions_for_local_day,
            stream_sessions,
            get_history_stats,
            delete_session,
            open_compare_window,
            open_compare_window_with_sessions,
            get_session_metrics,
            get_session_timeseries,
            get_session_location,
            get_session_embedding_count,
            get_csv_metrics,
            get_day_metrics_batch,
            list_embedding_sessions,
            get_sleep_stages,
            compute_umap_compare,
            enqueue_umap_compare,
            poll_job,
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
            get_ws_clients,
            get_ws_request_log,
            get_ws_port,
            get_ws_config,
            set_ws_config,
            get_api_token,
            set_api_token,
            get_autostart_enabled,
            set_autostart_enabled,
            get_update_check_interval,
            set_update_check_interval,
            get_skills_refresh_interval,
            set_skills_refresh_interval,
            get_skills_sync_on_launch,
            set_skills_sync_on_launch,
            get_skills_last_sync,
            sync_skills_now,
            list_skills,
            get_disabled_skills,
            set_disabled_skills,
            get_skills_license,
            get_openbci_config,
            set_openbci_config,
            list_serial_ports,
            get_device_api_config,
            set_device_api_config,
            get_scanner_config,
            set_scanner_config,
            get_device_log,
            get_cortex_ws_state,
            get_neutts_config,
            set_neutts_config,
            pick_ref_wav_file,
            get_tts_preload,
            set_tts_preload,
            get_active_window_tracking,
            set_active_window_tracking,
            get_active_window,
            get_input_activity_tracking,
            set_input_activity_tracking,
            get_last_input_activity,
            get_recent_active_windows,
            get_recent_input_activity,
            get_input_buckets,
            get_location_enabled,
            set_location_enabled,
            test_location,
            get_dnd_config,
            set_dnd_config,
            get_dnd_active,
            get_dnd_status,
            test_dnd,
            list_focus_modes,
            get_sleep_config,
            set_sleep_config,
            get_llm_config,
            set_llm_config,
            get_inference_device,
            set_inference_device,
            get_exg_inference_device,
            set_exg_inference_device,
            pick_exg_weights_file,
            pick_gguf_file,
            web_cache_stats,
            web_cache_list,
            web_cache_clear,
            web_cache_remove_domain,
            web_cache_remove_entry,
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
            switch_llm_mmproj,
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
            get_session_params,
            #[cfg(feature = "llm")]
            set_session_params,
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
            get_screenshot_config,
            set_screenshot_config,
            estimate_screenshot_reembed,
            rebuild_screenshot_embeddings,
            get_screenshots_around,
            search_screenshots_by_vector,
            search_screenshots_by_image,
            search_screenshots_by_text,
            check_ocr_models_ready,
            download_ocr_models,
            get_screenshot_metrics,
            get_screenshots_dir,
            tts_unload,
            tts_get_voice,
            tts_list_neutts_voices,
            session_connect::connect_openbci,
            settings_cmds::lsl_cmds::lsl_discover,
            settings_cmds::lsl_cmds::lsl_connect,
            settings_cmds::lsl_cmds::lsl_pair_stream,
            settings_cmds::lsl_cmds::lsl_unpair_stream,
            settings_cmds::lsl_cmds::lsl_get_config,
            settings_cmds::lsl_cmds::lsl_set_auto_connect,
            settings_cmds::lsl_cmds::lsl_get_idle_timeout,
            settings_cmds::lsl_cmds::lsl_set_idle_timeout,
            settings_cmds::lsl_cmds::lsl_switch_session,
            settings_cmds::lsl_cmds::lsl_start_secondary,
            settings_cmds::lsl_cmds::lsl_cancel_secondary,
            settings_cmds::lsl_cmds::list_secondary_sessions,
            settings_cmds::lsl_cmds::lsl_iroh_start,
            settings_cmds::lsl_cmds::lsl_iroh_status,
            settings_cmds::lsl_cmds::lsl_iroh_stop,
            open_api_window,
            open_whats_new_window,
            get_whats_new_seen_version,
            dismiss_whats_new,
            open_onboarding_window,
            get_onboarding_model_download_order,
            complete_onboarding,
            get_onboarding_complete,
            commands::search_embeddings,
            global_eeg_index::get_global_index_stats,
            global_eeg_index::rebuild_global_eeg_index,
            commands::enqueue_search_embeddings,
            commands::stream_search_embeddings,
            commands::find_session_for_timestamp,
            commands::interactive_search,
            commands::regenerate_interactive_svg,
            commands::regenerate_interactive_dot,
            commands::save_dot_file,
            commands::save_svg_file,
            open_session_window,
            tts_init,
            tts_speak,
            tts_list_voices,
            tts_set_voice,
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
