// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Server lifecycle commands: start, stop, switch, status, logs.

use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::MutexExt;
use crate::AppState;
use super::save_catalog;
use crate::llm::{LlmLogEntry, LlmStatus, cell_status, push_log};
use crate::llm::catalog::DownloadState;

// ── Logs ──────────────────────────────────────────────────────────────────────

/// Return all buffered LLM server log entries (up to 500 most recent).
#[tauri::command]
pub fn get_llm_logs(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<LlmLogEntry> {
    let s      = state.lock_or_recover();
    let log    = s.llm.logs.lock().expect("lock poisoned");
    let result: Vec<LlmLogEntry> = log.iter().cloned().collect();
    result
}

// ── Start ─────────────────────────────────────────────────────────────────────

/// Start the LLM inference server.
///
/// Immediately returns `"starting"` and loads the model on a background
/// thread so the UI is never blocked.  The frontend should poll
/// `get_llm_server_status` to detect when `status` transitions from
/// `Loading` → `Running` or when `start_error` is non-null.
///
/// No-ops (returns `"already_running"`) if the server is already up or a
/// load is already in progress.
#[tauri::command]
pub fn start_llm_server(
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<String, String> {
    use std::sync::atomic::Ordering;

    let (mut config, catalog, log_buf, cell, skill_dir, loading, start_error) = {
        let mut s = state.lock_or_recover();

        // Auto-select the first downloaded model if none is active or the
        // active model doesn't exist on disk (e.g. deleted).
        let needs_model = s.llm.catalog.active_model.is_empty()
            || s.llm.catalog.active_model_path().is_none_or(|p| !p.exists());
        if needs_model {
            if let Some(entry) = s.llm.catalog.entries.iter()
                .find(|e| !e.is_mmproj && e.state == DownloadState::Downloaded
                          && e.local_path.as_ref().is_some_and(|p| p.exists()))
            {
                s.llm.catalog.active_model = entry.filename.clone();
                s.llm.config.model_path = s.llm.catalog.active_model_path();
                s.llm.config.enabled = true;
                save_catalog(&app, &s);
            }
        }

        (
            s.llm.config.clone(),
            s.llm.catalog.clone(),
            s.llm.logs.clone(),
            s.llm.state_cell.clone(),
            s.skill_dir.clone(),
            s.llm.loading.clone(),
            s.llm.start_error.clone(),
        )
    };

    if cell.lock().expect("lock poisoned").is_some() {
        return Ok("already_running".to_string());
    }
    if loading.load(Ordering::Relaxed) {
        return Ok("already_loading".to_string());
    }

    // Clear any previous error and mark loading.
    *start_error.lock().expect("lock poisoned") = None;
    loading.store(true, Ordering::Relaxed);

    let emitter = crate::llm::TauriEmitter(app.clone());
    push_log(&emitter, &log_buf, "info", "start_llm_server: spawning background load");

    // If no mmproj is explicitly set but autoload is on, resolve the best one.
    if config.mmproj.is_none() {
        config.mmproj = catalog.resolve_mmproj_path(config.autoload_mmproj);
        if let Some(ref p) = config.mmproj {
            push_log(&emitter, &log_buf, "info",
                &format!("autoload_mmproj: selected {}", p.display()));
        }
    }

    // Spawn a background task — load the model without blocking the UI thread.
    tauri::async_runtime::spawn(async move {
        let emitter_arc: std::sync::Arc<dyn crate::llm::LlmEventEmitter> =
            std::sync::Arc::new(crate::llm::TauriEmitter(app));
        let result = tokio::task::spawn_blocking(move || {
            crate::llm::init(&config, &catalog, emitter_arc, log_buf, &skill_dir)
        }).await;

        loading.store(false, Ordering::Relaxed);

        match result {
            Ok(Some(s))  => { *cell.lock().expect("lock poisoned") = Some(s); }
            Ok(None)     => {
                *start_error.lock().expect("lock poisoned") = Some(
                    "Failed to start LLM server. \
                     Check that a model is downloaded and selected in Settings → LLM."
                    .to_string()
                );
            }
            Err(e) => {
                *start_error.lock().expect("lock poisoned") = Some(format!("Load task panicked: {e}"));
            }
        }
    });

    Ok("starting".to_string())
}

// ── Stop ──────────────────────────────────────────────────────────────────────

/// Stop the LLM inference server gracefully.
///
/// Takes the server state out of the cell (so new inference requests are
/// immediately rejected) and joins the actor thread on a background thread
/// so the UI is never blocked waiting for GPU resources to free up.
#[tauri::command]
pub fn stop_llm_server(
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let (cell, log_buf, loading, start_error) = {
        let s = state.lock_or_recover();
        (
            s.llm.state_cell.clone(),
            s.llm.logs.clone(),
            s.llm.loading.clone(),
            s.llm.start_error.clone(),
        )
    };

    // Cancel any in-progress load as well.
    loading.store(false, std::sync::atomic::Ordering::Relaxed);
    *start_error.lock().expect("lock poisoned") = None;

    // Take the Arc out of the cell so the server is immediately "Stopped"
    // from the UI's perspective before the actor thread finishes joining.
    let server_state = { cell.lock().expect("lock poisoned").take() };
    if let Some(server_state) = server_state {
        let emitter = crate::llm::TauriEmitter(app);
        push_log(&emitter, &log_buf, "info", "stopping LLM server — freeing resources in background…");
        // Join the actor thread on a blocking thread so the caller returns
        // immediately without freezing the UI or the Tauri IPC channel.
        tauri::async_runtime::spawn(async move {
            tokio::task::spawn_blocking(move || {
                match std::sync::Arc::try_unwrap(server_state) {
                    Ok(owned) => owned.shutdown(),
                    Err(arc)  => drop(arc),
                }
                push_log(&emitter, &log_buf, "info", "LLM server stopped");
            }).await.ok();
        });
    }
}

// ── Switch ────────────────────────────────────────────────────────────────────

/// Atomically switch to a different model: stop the running server (if any),
/// wait for full shutdown, set the new active model, then start again.
///
/// Returns immediately — the frontend should poll `get_llm_server_status` to
/// track the `Stopped → Loading → Running` transition.
#[tauri::command]
pub fn switch_llm_model(
    filename: String,
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<String, String> {
    use std::sync::atomic::Ordering;

    let (cell, log_buf, loading, start_error) = {
        let mut s = state.lock_or_recover();
        // Update the active model in the catalog immediately.
        s.llm.catalog.active_model = filename.clone();
        if !s.llm.catalog.active_mmproj_matches_active_model() {
            s.llm.catalog.active_mmproj.clear();
        }
        // Mirror into LlmConfig so the server picks the updated pair.
        s.llm.config.model_path = s.llm.catalog.active_model_path();
        s.llm.config.mmproj     = s.llm.catalog.active_mmproj_path();
        s.llm.config.enabled    = true;
        save_catalog(&app, &s);
        (
            s.llm.state_cell.clone(),
            s.llm.logs.clone(),
            s.llm.loading.clone(),
            s.llm.start_error.clone(),
        )
    };

    crate::save_settings(&app);

    // Clear any previous error.
    *start_error.lock().expect("lock poisoned") = None;

    // Take the running server out of the cell (if any).
    let server_state = { cell.lock().expect("lock poisoned").take() };

    let app2  = app.clone();

    // Mark loading right away so the UI sees the transition.
    loading.store(true, Ordering::Relaxed);

    let emitter = crate::llm::TauriEmitter(app.clone());
    push_log(&emitter, &log_buf, "info",
        &format!("switch_llm_model: switching to {filename}"));

    tauri::async_runtime::spawn(async move {
        // 1. Shut down the old server (if running).
        if let Some(old) = server_state {
            let log_buf2 = log_buf.clone();
            let emitter2 = crate::llm::TauriEmitter(app2.clone());
            tokio::task::spawn_blocking(move || {
                match std::sync::Arc::try_unwrap(old) {
                    Ok(owned) => owned.shutdown(),
                    Err(arc)  => drop(arc),
                }
                push_log(&emitter2, &log_buf2, "info", "old model unloaded");
            }).await.ok();
        }

        // 2. Start the new model.
        let (mut config, catalog, skill_dir) = {
            let r = app2.state::<Mutex<Box<AppState>>>();
            let s = r.lock_or_recover();
            (s.llm.config.clone(), s.llm.catalog.clone(), s.skill_dir.clone())
        };

        // Resolve mmproj if needed.
        if config.mmproj.is_none() {
            config.mmproj = catalog.resolve_mmproj_path(config.autoload_mmproj);
        }

        let emitter_arc: std::sync::Arc<dyn crate::llm::LlmEventEmitter> =
            std::sync::Arc::new(crate::llm::TauriEmitter(app2));
        let result = tokio::task::spawn_blocking(move || {
            crate::llm::init(&config, &catalog, emitter_arc, log_buf, &skill_dir)
        }).await;

        loading.store(false, std::sync::atomic::Ordering::Relaxed);

        match result {
            Ok(Some(s)) => { *cell.lock().expect("lock poisoned") = Some(s); }
            Ok(None) => {
                *start_error.lock().expect("lock poisoned") = Some(
                    "Failed to start LLM server after model switch.".to_string()
                );
            }
            Err(e) => {
                *start_error.lock().expect("lock poisoned") = Some(format!("Load task panicked: {e}"));
            }
        }
    });

    Ok("switching".to_string())
}

// ── Status ────────────────────────────────────────────────────────────────────

/// Return the current server status: `Stopped | Loading | Running`.
#[derive(serde::Serialize)]
pub struct LlmServerStatusResponse {
    pub status:         LlmStatus,
    pub model_name:     String,
    /// Context window size in tokens (0 = model not yet loaded).
    pub n_ctx:          usize,
    /// True when a vision projector is loaded and image input is supported.
    pub supports_vision: bool,
    /// True when the model is running and built-in tools are available.
    pub supports_tools:  bool,
    /// Non-null when the most recent background start attempt failed.
    /// Cleared when a new start is requested.
    pub start_error:    Option<String>,
}

#[tauri::command]
pub fn get_llm_server_status(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> LlmServerStatusResponse {
    use std::sync::atomic::Ordering;
    let s = state.lock_or_recover();
    // If the background task is loading but the cell is still empty, report Loading.
    let (mut status, model_name) = cell_status(&s.llm.state_cell);
    if matches!(status, LlmStatus::Stopped) && s.llm.loading.load(Ordering::Relaxed) {
        status = LlmStatus::Loading;
    }
    let (n_ctx, supports_vision, supports_tools) = s.llm.state_cell.lock().expect("lock poisoned")
        .as_ref()
        .map(|srv| (
            srv.n_ctx.load(Ordering::Relaxed),
            srv.vision_ready.load(Ordering::Relaxed),
            srv.is_ready(),
        ))
        .unwrap_or((0, false, false));
    let start_error = s.llm.start_error.lock().expect("lock poisoned").clone();
    LlmServerStatusResponse { status, model_name, n_ctx, supports_vision, supports_tools, start_error }
}
