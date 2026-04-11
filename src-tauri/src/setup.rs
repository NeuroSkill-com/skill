// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! App initialisation: one-shot setup, settings hydration, cache migration.

use tauri::Manager;

#[cfg(target_os = "macos")]
use crate::constants;
use crate::helpers::AppStateExt;
use crate::settings::{load_settings, CalibrationProfile};
use crate::shortcut_cmds::apply_all_shortcuts;
use crate::state::DiscoveredDevice;
use crate::tray::build_menu;
use crate::tts::{init_espeak_bundled_data_path, init_neutts_samples_dir, neutts_apply_config};
use crate::ws_server;
use crate::MutexExt;

// ── External renderer for macOS headless webview ─────────────────────────────

#[cfg(target_os = "macos")]
mod external_renderer_bridge {
    pub(crate) fn setup(app: &mut tauri::App) {
        crate::external_renderer::setup(app);
    }
}

// ── One-time migration: fastembed_cache → HuggingFace hub cache ──────────────

/// Move model directories from `~/.skill/fastembed_cache/` into the shared
/// HuggingFace hub cache (`~/.cache/huggingface/hub/`).  Idempotent —
/// skips entries that already exist in the destination.
fn migrate_fastembed_cache(skill_dir: &std::path::Path) {
    let src = skill_dir.join("fastembed_cache");
    let dst = skill_data::util::hf_cache_root();
    migrate_cache_dir(&src, &dst);
}

/// Move all entries from `src` into `dst`.  Skips entries that already exist
/// in `dst` (removes the stale source copy instead).  Removes `src` when empty.
fn migrate_cache_dir(src: &std::path::Path, dst: &std::path::Path) {
    if !src.is_dir() {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(dst) {
        eprintln!("[migrate] cannot create HF cache dir: {e}");
        return;
    }
    let Ok(entries) = std::fs::read_dir(src) else {
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
    // Remove the now-empty source dir (best-effort).
    if moved > 0 {
        let _ = std::fs::remove_dir(src);
        eprintln!(
            "[migrate] moved {moved} model(s) from {} → {}",
            src.display(),
            dst.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_noop_when_src_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("fastembed_cache");
        let dst = tmp.path().join("hf_hub");
        // src does not exist — should be a no-op
        migrate_cache_dir(&src, &dst);
        assert!(!dst.exists());
    }

    #[test]
    fn migrate_moves_entries_to_dst() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("fastembed_cache");
        let dst = tmp.path().join("hf_hub");
        std::fs::create_dir_all(src.join("model_a")).unwrap();
        std::fs::write(src.join("model_a/weights.bin"), b"data").unwrap();

        migrate_cache_dir(&src, &dst);

        assert!(dst.join("model_a/weights.bin").exists());
        // src should be removed after successful migration
        assert!(!src.exists());
    }

    #[test]
    fn migrate_skips_existing_and_removes_stale_src() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("fastembed_cache");
        let dst = tmp.path().join("hf_hub");
        std::fs::create_dir_all(src.join("model_b")).unwrap();
        std::fs::write(src.join("model_b/old.bin"), b"old").unwrap();
        // Pre-existing entry in dst
        std::fs::create_dir_all(dst.join("model_b")).unwrap();
        std::fs::write(dst.join("model_b/new.bin"), b"new").unwrap();

        migrate_cache_dir(&src, &dst);

        // dst entry should be untouched
        assert_eq!(std::fs::read(dst.join("model_b/new.bin")).unwrap(), b"new");
        // src copy should be removed
        assert!(!src.join("model_b").exists());
    }

    #[test]
    fn migrate_idempotent_on_empty_src() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("fastembed_cache");
        let dst = tmp.path().join("hf_hub");
        std::fs::create_dir_all(&src).unwrap();
        // src exists but is empty — should be a no-op (no moved count)
        migrate_cache_dir(&src, &dst);
        // src stays because moved == 0
        assert!(src.exists());
    }
}

// ── App setup ────────────────────────────────────────────────────────────────

/// Extracted from the `.setup()` closure so LLVM does not merge its locals
/// into the already-huge `run()` stack frame produced by `generate_handler!`.
#[inline(never)]
pub(crate) fn setup_app(app: &mut tauri::App) -> anyhow::Result<()> {
    // On macOS, the headless browser (tao) cannot create a second event loop
    // because Tauri already owns the main thread.  Disable the standalone
    // browser and register an external renderer that reuses Tauri's webview.
    #[cfg(target_os = "macos")]
    {
        skill_headless::Browser::set_unavailable();
        external_renderer_bridge::setup(app);
    }

    {
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

    let broadcaster = ws_server::WsBroadcaster::new();

    #[cfg(feature = "llm")]
    {
        // LLM inference server ownership moved to daemon.
    }

    // ── Daemon runtime readiness (spawn → protocol gate → service repair) ──
    crate::daemon_cmds::ensure_daemon_runtime_ready();

    app.manage(broadcaster);

    let (logger_arc, skill_dir) = {
        let r = app.app_state();
        let g = r.lock_or_recover();
        (g.logger.clone(), g.skill_dir.clone())
    };
    app.manage(logger_arc);

    // Route TTS and LLM log output through the unified SkillLogger.
    crate::tts::init_tts_logger(app.handle());
    #[cfg(feature = "llm")]
    {
        crate::llm::init_llm_logger(app.handle());
        crate::llm::init_tool_logger(app.handle());
    }

    load_and_apply_settings(app, &skill_dir);

    // ── Gather values from AppState in a single lock acquisition ─────
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
            use crate::state::AppState;
            use std::sync::Mutex;
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let state = app_handle.state::<Mutex<Box<AppState>>>();
                crate::llm::cmds::start_llm_server(app_handle.clone(), state).ok();
            });
        }
    }

    // Migrate fastembed_cache → HuggingFace hub cache (one-time, idempotent).
    migrate_fastembed_cache(&skill_dir);

    // EXG weight probing now runs in the daemon; the Tauri app queries
    // the daemon's /v1/embedding/status endpoint instead.
    let _ = (model_status, hf_repo);

    if let Err(e) = apply_all_shortcuts(app.handle()) {
        eprintln!("[shortcut] failed to register shortcuts: {e}");
    }

    setup_macos_menu(app)?;

    app.on_menu_event(|app, event| {
        if event.id().as_ref() == "about" {
            let a = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::about::open_about_window(a).await;
            });
        } else if event.id().as_ref() == "macos_quit" {
            crate::confirm_and_quit(app.clone());
        }
    });

    let init_status = {
        let r = app.app_state();
        let g = r.lock_or_recover();
        g.status.clone()
    };
    let init_menu = build_menu(app.handle(), &init_status)?;

    #[cfg(target_os = "linux")]
    if !crate::platform::linux_has_appindicator_runtime() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "System tray is required but Linux appindicator runtime is missing. \
             Install libayatana-appindicator3 or libappindicator3.",
        )
        .into());
    }

    crate::tray_setup::build_tray(app, &init_menu)?;

    // Screenshot capture worker is daemon-owned in the thin-client architecture.

    crate::background::spawn_all(app);
    Ok(())
}

/// Build the macOS application menu bar (App / Edit / Window).
#[cfg(target_os = "macos")]
fn setup_macos_menu(app: &mut tauri::App) -> anyhow::Result<()> {
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
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn setup_macos_menu(_app: &mut tauri::App) -> anyhow::Result<()> {
    Ok(())
}

// ── Settings hydration ───────────────────────────────────────────────────────

/// Load persisted settings from disk and apply them to `AppState`.
#[inline(never)]
fn load_and_apply_settings(app: &mut tauri::App, skill_dir: &std::path::Path) {
    let data = load_settings(skill_dir);
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        // Prefer paired_devices.json (daemon-authoritative) over settings.json.
        let paired_devices_path = skill_dir.join(skill_constants::PAIRED_DEVICES_FILE);
        let paired: Vec<skill_data::device::PairedDevice> = if paired_devices_path.exists() {
            skill_data::util::load_json_or_default(&paired_devices_path)
        } else {
            data.paired.clone()
        };
        s.status.paired_devices = paired;
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
