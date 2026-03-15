// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tauri commands for the LLM settings tab.
//!
//! All commands are gated behind the `llm` feature flag.  When the feature
//! is absent these stubs are compiled but simply return an error / empty value,
//! keeping the frontend command calls valid regardless of the build config.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use serde::Serialize;

use crate::MutexExt;
use crate::AppState;
use crate::tray::refresh_tray;
use super::catalog::{DownloadProgress, DownloadState, LlmCatalog};
use super::{LlmLogEntry, LlmStatus, cell_status, push_log};

#[derive(Debug, Clone, Serialize)]
pub struct LlmDownloadItem {
    pub repo:              String,
    pub filename:          String,
    pub quant:             String,
    pub size_gb:           f32,
    pub description:       String,
    pub is_mmproj:         bool,
    pub state:             DownloadState,
    pub status_msg:        Option<String>,
    pub progress:          f32,
    pub initiated_at_unix: Option<u64>,
    pub local_path:        Option<std::path::PathBuf>,
}

// ── Catalog query ──────────────────────────────────────────────────────────────

/// Return the current LLM model catalog (all entries, their download states,
/// and the active model / mmproj selections).
///
/// The frontend polls this every ~2 s while the LLM tab is visible.
#[tauri::command]
pub fn get_llm_catalog(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> LlmCatalog {
    let mut s = state.lock_or_recover();
    // Sync in-flight download progress into the catalog entries before
    // returning so the UI always sees the latest state.
    let downloads = s.llm.downloads.clone();
    for (filename, prog_arc) in &downloads {
        if let Ok(prog) = prog_arc.lock() {
            if let Some(entry) = s.llm.catalog.entries
                .iter_mut()
                .find(|e| &e.filename == filename)
            {
                entry.state      = prog.state.clone();
                entry.status_msg = prog.status_msg.clone();
                entry.progress   = prog.progress;
                if prog.state == DownloadState::Downloaded {
                    entry.local_path = entry.resolve_cached();
                }
            }
        }
    }
    s.llm.catalog.clone()
}

#[tauri::command]
pub fn get_llm_downloads(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<LlmDownloadItem> {
    let mut s = state.lock_or_recover();

    let downloads = s.llm.downloads.clone();
    for (filename, prog_arc) in &downloads {
        if let Ok(prog) = prog_arc.lock() {
            if let Some(entry) = s.llm.catalog.entries
                .iter_mut()
                .find(|e| &e.filename == filename)
            {
                entry.state      = prog.state.clone();
                entry.status_msg = prog.status_msg.clone();
                entry.progress   = prog.progress;
                if prog.state == DownloadState::Downloaded {
                    entry.local_path = entry.resolve_cached();
                }
            }
        }
    }

    let mut items: Vec<LlmDownloadItem> = s.llm.catalog.entries.iter()
        .filter(|e| {
            e.state == DownloadState::Downloading
                || e.state == DownloadState::Paused
                || e.state == DownloadState::Failed
                || e.state == DownloadState::Cancelled
                || e.state == DownloadState::Downloaded
        })
        .map(|e| LlmDownloadItem {
            repo: e.repo.clone(),
            filename: e.filename.clone(),
            quant: e.quant.clone(),
            size_gb: e.size_gb,
            description: e.description.clone(),
            is_mmproj: e.is_mmproj,
            state: e.state.clone(),
            status_msg: e.status_msg.clone(),
            progress: e.progress,
            initiated_at_unix: e.initiated_at_unix,
            local_path: e.local_path.clone(),
        })
        .collect();

    items.sort_by(|a, b| b.initiated_at_unix.unwrap_or(0).cmp(&a.initiated_at_unix.unwrap_or(0)));
    items
}

/// Persist the catalog to disk (called after state changes).
fn save_catalog(app: &AppHandle, state: &AppState) {
    state.llm.catalog.save(&state.skill_dir);
    let _ = app; // suppress unused warning
}

// ── Active model selection ────────────────────────────────────────────────────

/// Set the active LLM model (by filename).
/// The selection is persisted to `llm_catalog.json` immediately.
#[tauri::command]
pub fn set_llm_active_model(
    filename: String,
    app:      AppHandle,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    s.llm.catalog.active_model = filename;
    if !s.llm.catalog.active_mmproj_matches_active_model() {
        s.llm.catalog.active_mmproj.clear();
    }
    // Mirror into LlmConfig so the server picks the updated pair up on restart.
    s.llm.config.model_path = s.llm.catalog.active_model_path();
    s.llm.config.mmproj     = s.llm.catalog.active_mmproj_path();
    save_catalog(&app, &s);
    drop(s);
    crate::save_settings_handle(&app);
}

/// Toggle whether the vision projector is auto-loaded when the server starts.
#[tauri::command]
pub fn set_llm_autoload_mmproj(
    enabled: bool,
    app:     AppHandle,
    state:   tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    s.llm.config.autoload_mmproj = enabled;
    drop(s);
    crate::save_settings_handle(&app);
}

/// Set the active mmproj projector (by filename, or empty to disable).
#[tauri::command]
pub fn set_llm_active_mmproj(
    filename: String,
    app:      AppHandle,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    if filename.is_empty() {
        s.llm.catalog.active_mmproj.clear();
    } else {
        let current_matches = s.llm.catalog.active_model_entry()
            .zip(s.llm.catalog.entries.iter().find(|e| e.is_mmproj && e.filename == filename))
            .is_some_and(|(model, mmproj)| model.repo == mmproj.repo);

        if !current_matches {
            if let Some(model_filename) = s.llm.catalog
                .best_model_for_mmproj(&filename)
                .map(|entry| entry.filename.clone())
            {
                s.llm.catalog.active_model = model_filename;
            }
        }

        if s.llm.catalog.active_model_entry()
            .zip(s.llm.catalog.entries.iter().find(|e| e.is_mmproj && e.filename == filename))
            .is_some_and(|(model, mmproj)| model.repo == mmproj.repo)
        {
            s.llm.catalog.active_mmproj = filename;
        } else {
            s.llm.catalog.active_mmproj.clear();
        }
    }

    s.llm.config.model_path = s.llm.catalog.active_model_path();
    s.llm.config.mmproj     = s.llm.catalog.active_mmproj_path();
    save_catalog(&app, &s);
    drop(s);
    crate::save_settings_handle(&app);
}

// ── Download / cancel / delete ────────────────────────────────────────────────

/// Start downloading a GGUF file by filename.
///
/// Spawns a blocking task so the UI stays responsive.  The download progress
/// can be observed by polling `get_llm_catalog()` every few seconds.
///
/// If a download for this file is already in progress, this is a no-op.
#[tauri::command]
pub fn download_llm_model(
    filename: String,
    app:      AppHandle,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let (repo, _skill_dir, prog_arc, size_bytes) = {
        let mut s = state.lock_or_recover();

        // Find the entry.
        let entry = match s.llm.catalog.entries
            .iter()
            .find(|e| e.filename == filename)
        {
            Some(e) => e.clone(),
            None => {
                eprintln!("[llm] download_llm_model: unknown filename '{filename}'");
                return;
            }
        };

        // Skip if already downloading.
        if s.llm.downloads.contains_key(&filename) {
            if let Some(prog) = s.llm.downloads.get(&filename) {
                if prog.lock().is_ok_and(|p| p.state == DownloadState::Downloading) {
                    return;
                }
            }
        }

        // Catalog size in bytes — used by the progress monitor thread.
        let size_bytes = (entry.size_gb * 1_073_741_824.0) as u64;

        // Mark as downloading in the catalog immediately so the UI updates.
        if let Some(e) = s.llm.catalog.entries.iter_mut().find(|e| e.filename == filename) {
            e.state      = DownloadState::Downloading;
            e.status_msg = Some(format!("Queued: {}…", filename));
            e.progress   = 0.0;
            if e.initiated_at_unix.is_none() {
                e.initiated_at_unix = Some(crate::unix_secs());
            }
        }

        // Create a shared progress object.
        let prog = std::sync::Arc::new(Mutex::new(DownloadProgress {
            filename:   filename.clone(),
            state:      DownloadState::Downloading,
            status_msg: Some(format!("Queued: {}…", filename)),
            progress:   0.0,
            cancelled:  false,
            pause_requested: false,
        }));

        s.llm.downloads.insert(filename.clone(), prog.clone());

        (entry.repo.clone(), s.skill_dir.clone(), prog, size_bytes)
    };

    refresh_tray(&app);

    let watch_app = app.clone();
    let watch_prog = prog_arc.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_bucket: Option<u8> = None;
        let mut last_state = DownloadState::NotDownloaded;

        loop {
            let Some((state, bucket)) = watch_prog.lock().ok().map(|prog| {
                (prog.state.clone(), ((prog.progress.clamp(0.0, 1.0) * 20.0).round() as u8).min(20))
            }) else {
                break;
            };

            if last_bucket != Some(bucket) || last_state != state {
                refresh_tray(&watch_app);
                last_bucket = Some(bucket);
                last_state = state.clone();
            }

            if state != DownloadState::Downloading {
                break;
            }

            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    });

    let filename2 = filename.clone();
    let app2      = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let result = super::catalog::download_file(&repo, &filename2, &prog_arc, size_bytes);

        // After completion / failure, refresh the catalog entry.
        if let Some(state_handle) = app2.try_state::<Mutex<Box<AppState>>>() {
            let mut s = state_handle.lock_or_recover();
            if let Some(entry) = s.llm.catalog.entries
                .iter_mut()
                .find(|e| e.filename == filename2)
            {
                match result {
                    Ok(path) => {
                        entry.state      = DownloadState::Downloaded;
                        entry.local_path = Some(path);
                        entry.status_msg = None;
                        entry.progress   = 1.0;
                        // Auto-select if this is the first downloaded main model.
                        if !entry.is_mmproj && s.llm.catalog.active_model.is_empty() {
                            s.llm.catalog.active_model = filename2.clone();
                        }
                    }
                    Err(ref e) if e == "cancelled" => {
                        entry.state      = DownloadState::Cancelled;
                        entry.status_msg = Some("Cancelled.".into());
                        entry.progress   = 0.0;
                    }
                    Err(ref e) if e == "paused" => {
                        entry.state      = DownloadState::Paused;
                        entry.status_msg = Some("Paused.".into());
                    }
                    Err(e) => {
                        entry.state      = DownloadState::Failed;
                        entry.status_msg = Some(e);
                        entry.progress   = 0.0;
                    }
                }
            }
            // Mirror active model path to LlmConfig.
            let model_path  = s.llm.catalog.active_model_path();
            let mmproj_path = s.llm.catalog.active_mmproj_path();
            s.llm.config.model_path = model_path;
            s.llm.config.mmproj     = mmproj_path;
            // Remove the in-flight progress entry when finished (not paused).
            if !matches!(
                s.llm.catalog.entries
                    .iter()
                    .find(|e| e.filename == filename2)
                    .map(|e| e.state.clone()),
                Some(DownloadState::Paused)
            ) {
                s.llm.downloads.remove(&filename2);
            }
            save_catalog(&app2, &s);
            drop(s);
            crate::save_settings_handle(&app2);
            refresh_tray(&app2);
        }
    });
}

fn cancel_llm_download_inner(
    filename: String,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
    app:      Option<&AppHandle>,
) {
    let mut s = state.lock_or_recover();
    let mut was_paused = false;
    if let Some(prog) = s.llm.downloads.get(&filename) {
        if let Ok(mut p) = prog.lock() {
            if p.state == DownloadState::Paused {
                was_paused = true;
            }
            p.cancelled = true;
            p.status_msg = Some("Cancelling…".into());
            if was_paused {
                p.state = DownloadState::Cancelled;
            }
        }
    }
    if was_paused {
        s.llm.downloads.remove(&filename);
        if let Some(entry) = s.llm.catalog.entries.iter_mut().find(|e| e.filename == filename) {
            entry.state = DownloadState::Cancelled;
            entry.status_msg = Some("Cancelled.".into());
            entry.progress = 0.0;
        }
        if let Some(app) = app {
            save_catalog(app, &s);
        }
    }
    drop(s);
    if let Some(app) = app {
        refresh_tray(app);
    }
}

/// Cancel an in-progress download by filename.
#[tauri::command]
pub fn cancel_llm_download(
    filename: String,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) {
    cancel_llm_download_inner(filename, state, None);
}

#[tauri::command]
pub fn pause_llm_download(
    filename: String,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    if let Some(prog) = s.llm.downloads.get(&filename) {
        if let Ok(mut p) = prog.lock() {
            p.cancelled = true;
            p.pause_requested = true;
            p.status_msg = Some("Pausing…".into());
        }
    }
    if let Some(entry) = s.llm.catalog.entries.iter_mut().find(|e| e.filename == filename) {
        entry.state = DownloadState::Paused;
        entry.status_msg = Some("Pausing…".into());
    }
}

#[tauri::command]
pub fn resume_llm_download(
    filename: String,
    app:      AppHandle,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) {
    {
        let mut s = state.lock_or_recover();
        if let Some(entry) = s.llm.catalog.entries.iter_mut().find(|e| e.filename == filename) {
            entry.state = DownloadState::NotDownloaded;
            entry.status_msg = None;
        }
        s.llm.downloads.remove(&filename);
        save_catalog(&app, &s);
    }
    download_llm_model(filename, app, state);
}

pub fn cancel_llm_download_with_app(
    filename: String,
    app:      &AppHandle,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) {
    cancel_llm_download_inner(filename, state, Some(app));
}

/// Delete a locally-cached model file and reset its catalog entry.
///
/// Uses the HuggingFace Hub cache layout to locate the file, then removes it.
#[tauri::command]
pub fn delete_llm_model(
    filename: String,
    app:      AppHandle,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    if let Some(entry) = s.llm.catalog.entries
        .iter_mut()
        .find(|e| e.filename == filename)
    {
        if let Some(path) = entry.local_path.take() {
            if path.exists() {
                if let Err(e) = std::fs::remove_file(&path) {
                    eprintln!("[llm] delete failed for {}: {e}", path.display());
                }
            }
        }
        entry.state      = DownloadState::NotDownloaded;
        entry.status_msg = None;
        entry.progress   = 0.0;
        entry.initiated_at_unix = None;

        // Clear active selection if this was the active model/mmproj.
        if s.llm.catalog.active_model == filename {
            s.llm.catalog.active_model = String::new();
            s.llm.config.model_path    = None;
        }
        if s.llm.catalog.active_mmproj == filename {
            s.llm.catalog.active_mmproj = String::new();
            s.llm.config.mmproj         = None;
        }
    }
    save_catalog(&app, &s);
    drop(s);
    crate::save_settings_handle(&app);
    refresh_tray(&app);
}

#[tauri::command]
pub async fn open_downloads_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("downloads") {
        win.show().ok();
        win.set_focus().ok();
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        "downloads",
        tauri::WebviewUrl::App("downloads".into()),
    )
    .title("NeuroSkill™ – Downloads")
    .inner_size(760.0, 640.0)
    .min_inner_size(560.0, 420.0)
    .resizable(true)
    .center()
    .decorations(false)
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Force-refresh the catalog by re-probing the HuggingFace Hub disk cache.
/// Useful after the user downloads a file externally.
#[tauri::command]
pub fn refresh_llm_catalog(
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    s.llm.catalog.refresh_cache();
    s.llm.catalog.auto_select();
    let model_path  = s.llm.catalog.active_model_path();
    let mmproj_path = s.llm.catalog.active_mmproj_path();
    s.llm.config.model_path = model_path;
    s.llm.config.mmproj     = mmproj_path;
    save_catalog(&app, &s);
    drop(s);
    crate::save_settings_handle(&app);
    refresh_tray(&app);
}

/// Return all buffered LLM server log entries (up to 500 most recent).
#[tauri::command]
pub fn get_llm_logs(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<LlmLogEntry> {
    let s      = state.lock_or_recover();
    let log    = s.llm.logs.lock().unwrap();
    let result: Vec<LlmLogEntry> = log.iter().cloned().collect();
    result
}

// ── Server lifecycle ───────────────────────────────────────────────────────────

/// Start the LLM inference server.
///
/// Immediately returns `"starting"` and loads the model on a background
/// thread so the UI is never blocked.  The frontend should poll
/// `get_llm_server_status` (which already happens on a 2-second timer) to
/// detect when `status` transitions from `Loading` → `Running` or when
/// `start_error` is non-null after a failed load.
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
        let s = state.lock_or_recover();
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

    if cell.lock().unwrap().is_some() {
        return Ok("already_running".to_string());
    }
    if loading.load(Ordering::Relaxed) {
        return Ok("already_loading".to_string());
    }

    // Clear any previous error and mark loading.
    *start_error.lock().unwrap() = None;
    loading.store(true, Ordering::Relaxed);

    push_log(&app, &log_buf, "info", "start_llm_server: spawning background load");

    // If no mmproj is explicitly set but autoload is on, resolve the best one.
    if config.mmproj.is_none() {
        config.mmproj = catalog.resolve_mmproj_path(config.autoload_mmproj);
        if let Some(ref p) = config.mmproj {
            push_log(&app, &log_buf, "info",
                &format!("autoload_mmproj: selected {}", p.display()));
        }
    }

    // Spawn a background task — load the model without blocking the UI thread.
    tauri::async_runtime::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            crate::llm::init(&config, &catalog, app, log_buf, &skill_dir)
        }).await;

        loading.store(false, Ordering::Relaxed);

        match result {
            Ok(Some(s))  => { *cell.lock().unwrap() = Some(s); }
            Ok(None)     => {
                *start_error.lock().unwrap() = Some(
                    "Failed to start LLM server. \
                     Check that a model is downloaded and selected in Settings → LLM."
                    .to_string()
                );
            }
            Err(e) => {
                *start_error.lock().unwrap() = Some(format!("Load task panicked: {e}"));
            }
        }
    });

    Ok("starting".to_string())
}

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
    *start_error.lock().unwrap() = None;

    // Take the Arc out of the cell so the server is immediately "Stopped"
    // from the UI's perspective before the actor thread finishes joining.
    let server_state = { cell.lock().unwrap().take() };
    if let Some(server_state) = server_state {
        push_log(&app, &log_buf, "info", "stopping LLM server — freeing resources in background…");
        // Join the actor thread on a blocking thread so the caller returns
        // immediately without freezing the UI or the Tauri IPC channel.
        tauri::async_runtime::spawn(async move {
            tokio::task::spawn_blocking(move || {
                match std::sync::Arc::try_unwrap(server_state) {
                    Ok(owned) => owned.shutdown(),
                    Err(arc)  => drop(arc),
                }
                push_log(&app, &log_buf, "info", "LLM server stopped");
            }).await.ok();
        });
    }
}

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
    let (n_ctx, supports_vision, supports_tools) = s.llm.state_cell.lock().unwrap()
        .as_ref()
        .map(|srv| (
            srv.n_ctx.load(Ordering::Relaxed),
            srv.vision_ready.load(Ordering::Relaxed),
            srv.is_ready(),
        ))
        .unwrap_or((0, false, false));
    let start_error = s.llm.start_error.lock().unwrap().clone();
    LlmServerStatusResponse { status, model_name, n_ctx, supports_vision, supports_tools, start_error }
}

// ── Chat history persistence ───────────────────────────────────────────────────

/// Payload returned by `get_last_chat_session`.
#[derive(serde::Serialize)]
pub struct ChatSessionResponse {
    pub session_id: i64,
    pub messages:   Vec<super::chat_store::StoredMessage>,
}

/// Return the most recent chat session and all its messages.
/// Creates a fresh empty session if none exists yet.
/// Returns an empty response if the chat store is unavailable.
#[tauri::command]
pub fn get_last_chat_session(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> ChatSessionResponse {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else {
        return ChatSessionResponse { session_id: 0, messages: vec![] };
    };
    let session_id = store.get_or_create_last_session();
    let messages   = store.load_session(session_id);
    ChatSessionResponse { session_id, messages }
}

/// Load a specific chat session by id.
#[tauri::command]
pub fn load_chat_session(
    id:    i64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> ChatSessionResponse {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else {
        return ChatSessionResponse { session_id: id, messages: vec![] };
    };
    let messages = store.load_session(id);
    ChatSessionResponse { session_id: id, messages }
}

/// Return all sessions (newest-first) for the sidebar.
#[tauri::command]
pub fn list_chat_sessions(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<super::chat_store::SessionSummary> {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return vec![]; };
    store.list_sessions()
}

/// Set a custom title for a session (called after auto-title or inline rename).
#[tauri::command]
pub fn rename_chat_session(
    id:    i64,
    title: String,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return; };
    store.rename_session(id, &title);
}

/// Delete a session and all its messages.
#[tauri::command]
pub fn delete_chat_session(
    id:    i64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return; };
    store.delete_session(id);
}

/// Archive a session (soft-delete — keeps data but hides from main list).
#[tauri::command]
pub fn archive_chat_session(
    id:    i64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return; };
    store.archive_session(id);
}

/// Restore an archived session back to the main list.
#[tauri::command]
pub fn unarchive_chat_session(
    id:    i64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return; };
    store.unarchive_session(id);
}

/// Return all archived sessions.
#[tauri::command]
pub fn list_archived_chat_sessions(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<super::chat_store::SessionSummary> {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return vec![]; };
    store.list_archived_sessions()
}

/// Append a single message to a chat session.
/// Returns the new message row id, or 0 if the store is unavailable.
#[tauri::command]
pub fn save_chat_message(
    session_id: i64,
    role:       String,
    content:    String,
    thinking:   Option<String>,
    state:      tauri::State<'_, Mutex<Box<AppState>>>,
) -> i64 {
    eprintln!("[save_chat_message] called: session_id={session_id} role={role} content_len={}", content.len());
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else {
        eprintln!("[save_chat_message] chat_store is None!");
        return 0;
    };
    store.save_message(session_id, &role, &content, thinking.as_deref())
}

/// Create a fresh chat session and return its id.
/// Called when the user clicks "New Chat".
#[tauri::command]
pub fn new_chat_session(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> i64 {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return 0; };
    store.new_session()
}

/// Save tool calls associated with a chat message.
///
/// `message_id` must be the row id returned by `save_chat_message`.
/// `tool_calls` is a JSON array of objects matching `StoredToolCall` fields.
#[tauri::command]
pub fn save_chat_tool_calls(
    message_id: i64,
    tool_calls: Vec<super::chat_store::StoredToolCall>,
    state:      tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return; };
    store.save_tool_calls(message_id, &tool_calls);
}

// ── IPC chat streaming ────────────────────────────────────────────────────────

/// One message delivered through the Tauri IPC `Channel` for `chat_completions_ipc`.
///
/// Serialised as a tagged-union JSON object, e.g.:
/// ```json
/// {"type":"delta","content":"Hello"}
/// {"type":"done","finish_reason":"stop","prompt_tokens":42,"completion_tokens":18,"n_ctx":4096}
/// {"type":"error","message":"decode error"}
/// ```
/// An `"error"` with `message == "aborted"` means the caller invoked
/// `abort_llm_stream` — the frontend should treat partial content as the
/// final answer rather than showing an error.
#[derive(serde::Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatChunk {
    Delta    { content: String },
    /// Legacy event — still emitted for backwards compatibility.
    ToolUse  { tool: String, status: String, detail: Option<String> },
    /// Rich tool-execution lifecycle events (pi-mono style).
    ToolExecutionStart  { tool_call_id: String, tool_name: String, args: serde_json::Value },
    ToolExecutionEnd    { tool_call_id: String, tool_name: String, result: serde_json::Value, is_error: bool },
    /// A tool call was cancelled by the user.
    ToolCancelled       { tool_call_id: String, tool_name: String },
    Done     { finish_reason: String, prompt_tokens: usize, completion_tokens: usize, n_ctx: usize },
    Error    { message: String },
}

/// Stream a chat completion directly through Tauri IPC, bypassing the HTTP
/// server entirely — no CORS, no port lookup, no WebSocket required.
///
/// Tokens arrive on `channel` as `ChatChunk` messages in order:
/// zero or more `Delta`, then exactly one `Done` **or** one `Error`.
/// An `Error { message: "aborted" }` is sent when `abort_llm_stream` is called.
///
/// The command blocks (async-awaits) until generation finishes, is aborted,
/// or the channel is closed by the JS side.
#[tauri::command]
pub async fn chat_completions_ipc(
    messages: Vec<serde_json::Value>,
    params:   super::GenParams,
    channel:  tauri::ipc::Channel<ChatChunk>,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    let cell = state.lock_or_recover().llm.state_cell.clone();
    let srv  = cell.lock().unwrap().clone()
        .ok_or_else(|| "LLM server not running — start it in Settings → LLM".to_string())?;

    // Subscribe to the abort watch and mark the current value as "seen" so
    // that only a *new* increment (from `abort_llm_stream`) wakes us up.
    let mut abort_rx = srv.abort_tx.subscribe();
    abort_rx.borrow_and_update();

    let tool_channel = channel.clone();
    let gen_fut = super::run_chat_with_builtin_tools(&srv, messages, params, Vec::new(), |delta| {
        let _ = channel.send(ChatChunk::Delta { content: delta.to_string() });
    }, move |event: super::ToolEvent| {
        match event {
            super::ToolEvent::Status { tool_name, status, detail } => {
                if status.as_str() == "cancelled" {
                    let _ = tool_channel.send(ChatChunk::ToolCancelled {
                        tool_call_id: String::new(),
                        tool_name: tool_name.clone(),
                    });
                }
                let _ = tool_channel.send(ChatChunk::ToolUse {
                    tool:   tool_name,
                    status,
                    detail,
                });
            }
            super::ToolEvent::ExecutionStart { tool_call_id, tool_name, args } => {
                let _ = tool_channel.send(ChatChunk::ToolExecutionStart {
                    tool_call_id,
                    tool_name,
                    args,
                });
            }
            super::ToolEvent::ExecutionEnd { tool_call_id, tool_name, result, is_error } => {
                let _ = tool_channel.send(ChatChunk::ToolExecutionEnd {
                    tool_call_id,
                    tool_name,
                    result,
                    is_error,
                });
            }
        }
    });
    tokio::pin!(gen_fut);

    tokio::select! {
        biased;

        // Abort signal — higher priority than completion so we stop fast.
        Ok(()) = abort_rx.changed() => {
            let _ = channel.send(ChatChunk::Error { message: "aborted".into() });
        }

        result = &mut gen_fut => {
            match result {
                Ok((_text, finish_reason, prompt_tokens, completion_tokens, n_ctx)) => {
                    let _ = channel.send(ChatChunk::Done {
                        finish_reason,
                        prompt_tokens,
                        completion_tokens,
                        n_ctx,
                    });
                }
                Err(msg) => {
                    let _ = channel.send(ChatChunk::Error { message: msg });
                }
            }
        }
    }

    // If this command is aborted or dropped while generation is in-flight,
    // dropping the pinned future releases its internal token receiver; the
    // actor observes the closed channel on the next send and exits that request.
    Ok(())
}

/// Cancel a running `chat_completions_ipc` stream.
///
/// Increments the abort watch in `LlmServerState`; the streaming command
/// detects the change via `watch::Receiver::changed()` and returns early,
/// sending `ChatChunk::Error { message: "aborted" }` to the frontend first.
///
/// Safe to call even when no generation is in progress — it is a no-op if
/// the server is stopped or idle.
#[tauri::command]
pub fn abort_llm_stream(state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let cell = { let g = state.lock_or_recover(); g.llm.state_cell.clone() };
    let guard = cell.lock().unwrap();
    if let Some(srv) = guard.as_ref() {
        srv.abort_tx.send_modify(|v| *v = v.wrapping_add(1));
    }
}

/// Cancel a specific tool call by its `tool_call_id`.
///
/// Adds the ID to the server's cancelled-tool-call set. The tool execution
/// functions check this set before and during execution. If the tool is
/// already running (e.g. a long bash command), the cancellation takes effect
/// the next time the runner checks; for tools that haven't started yet,
/// execution is skipped entirely.
///
/// Safe to call even when no generation is in progress — it is a no-op if
/// the server is stopped or the ID doesn't match any pending call.
#[tauri::command]
pub fn cancel_tool_call(
    tool_call_id: String,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let cell = { let g = state.lock_or_recover(); g.llm.state_cell.clone() };
    let guard = cell.lock().unwrap();
    if let Some(srv) = guard.as_ref() {
        srv.cancelled_tool_calls.lock().unwrap().insert(tool_call_id);
    }
}

// ── Chat window ───────────────────────────────────────────────────────────────

/// Open (or focus) the floating Chat window.
#[tauri::command]
pub async fn open_chat_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("chat") {
        win.show().ok();
        win.set_focus().ok();
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        "chat",
        tauri::WebviewUrl::App("chat".into()),
    )
    .title("NeuroSkill™ – Chat")
    .inner_size(760.0, 680.0)
    .min_inner_size(480.0, 400.0)
    .resizable(true)
    .center()
    .decorations(false)
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

// ── Hardware fit prediction ───────────────────────────────────────────────────

/// Cached `SystemSpecs` detection — detect once, reuse forever.
/// `SystemSpecs::detect()` spawns child processes (nvidia-smi, rocm-smi, …)
/// and reads sysfs/WMI, so we must not call it on every Tauri poll.
static SYSTEM_SPECS: OnceLock<llmfit_core::hardware::SystemSpecs> = OnceLock::new();

fn cached_system_specs() -> &'static llmfit_core::hardware::SystemSpecs {
    SYSTEM_SPECS.get_or_init(llmfit_core::hardware::SystemSpecs::detect)
}

/// Per-model hardware fit prediction returned to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelHardwareFit {
    /// Catalog filename (join key for the frontend).
    pub filename: String,
    /// `"perfect"` | `"good"` | `"marginal"` | `"too_tight"`
    pub fit_level: String,
    /// `"gpu"` | `"moe"` | `"cpu_gpu"` | `"cpu"`
    pub run_mode: String,
    /// Estimated memory required (GB).
    pub memory_required_gb: f64,
    /// Memory pool being used (GB).
    pub memory_available_gb: f64,
    /// Estimated tokens per second.
    pub estimated_tps: f64,
    /// Composite score 0–100.
    pub score: f64,
    /// Human-readable notes from the analyzer.
    pub notes: Vec<String>,
}

/// Parse a parameter count string from a family name like "4B", "27B", "270M".
fn parse_param_count(family_name: &str) -> (String, Option<u64>) {
    // Try to find a param pattern like "4B", "27B", "270M"
    let mut best_label = String::from("7B");
    let mut best_raw: Option<u64> = None;

    let bytes = family_name.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        // Find start of a number
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let num_str = &family_name[start..i];
            // Check for B or M suffix
            if i < len && (bytes[i] == b'B' || bytes[i] == b'b' || bytes[i] == b'M' || bytes[i] == b'm') {
                let unit = (bytes[i] as char).to_uppercase().next().unwrap();
                if let Ok(num) = num_str.parse::<f64>() {
                    let raw = match unit {
                        'B' => (num * 1_000_000_000.0) as u64,
                        'M' => (num * 1_000_000.0) as u64,
                        _ => 0,
                    };
                    if best_raw.is_none() {
                        best_label = format!("{}{}", num_str, unit);
                        best_raw = Some(raw);
                    }
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    (best_label, best_raw)
}

/// Convert a catalog `LlmModelEntry` into an `llmfit_core::models::LlmModel`
/// for hardware-fit analysis.
fn catalog_entry_to_llm_model(entry: &super::catalog::LlmModelEntry) -> llmfit_core::models::LlmModel {
    let (param_count_str, parameters_raw) = parse_param_count(&entry.family_name);

    // Estimate min RAM from file size (GGUF size ≈ model weights; add ~0.5 GB overhead)
    let min_ram = (entry.size_gb as f64) + 0.5;
    let recommended_ram = min_ram * 1.3;

    // Detect MoE from family name (e.g. "35B-A3B")
    let name_lower = entry.family_name.to_lowercase();
    let is_moe = name_lower.contains("-a") && {
        // Check for pattern like "35B-A3B"
        let parts: Vec<&str> = entry.family_name.split('-').collect();
        parts.iter().any(|p| {
            let lower = p.to_lowercase();
            lower.starts_with('a') && lower.len() > 1 && lower[1..].ends_with('b')
                && lower[1..lower.len()-1].parse::<f64>().is_ok()
        })
    };

    // Infer use_case from tags
    let use_case = if entry.tags.iter().any(|t| t == "coding") {
        "Coding"
    } else if entry.tags.iter().any(|t| t == "reasoning") {
        "Reasoning"
    } else if entry.tags.iter().any(|t| t == "vision" || t == "multimodal") {
        "Multimodal"
    } else {
        "Chat"
    };

    llmfit_core::models::LlmModel {
        name: entry.family_name.clone(),
        provider: entry.repo.split('/').next().unwrap_or("unknown").to_string(),
        parameter_count: param_count_str,
        parameters_raw,
        min_ram_gb: min_ram,
        recommended_ram_gb: recommended_ram,
        min_vram_gb: Some(min_ram),
        quantization: entry.quant.clone(),
        context_length: 4096,
        use_case: use_case.to_string(),
        is_moe,
        num_experts: None,
        active_experts: None,
        active_parameters: None,
        release_date: None,
        gguf_sources: vec![],
        capabilities: vec![],
    }
}

/// Predict hardware fit for all non-mmproj catalog entries.
///
/// Returns a list of `ModelHardwareFit` objects, one per model file, containing
/// fit level, run mode, estimated TPS, memory requirements, and notes from
/// `llmfit-core`'s `ModelFit::analyze`.
#[tauri::command]
pub fn get_model_hardware_fit(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<ModelHardwareFit> {
    let specs = cached_system_specs();
    let s = state.lock_or_recover();

    s.llm.catalog.entries.iter()
        .filter(|e| !e.is_mmproj)
        .map(|entry| {
            let model = catalog_entry_to_llm_model(entry);
            let fit = llmfit_core::fit::ModelFit::analyze(&model, specs);

            let fit_level = match fit.fit_level {
                llmfit_core::fit::FitLevel::Perfect  => "perfect",
                llmfit_core::fit::FitLevel::Good     => "good",
                llmfit_core::fit::FitLevel::Marginal => "marginal",
                llmfit_core::fit::FitLevel::TooTight => "too_tight",
            };
            let run_mode = match fit.run_mode {
                llmfit_core::fit::RunMode::Gpu        => "gpu",
                llmfit_core::fit::RunMode::MoeOffload => "moe",
                llmfit_core::fit::RunMode::CpuOffload => "cpu_gpu",
                llmfit_core::fit::RunMode::CpuOnly    => "cpu",
            };

            ModelHardwareFit {
                filename: entry.filename.clone(),
                fit_level: fit_level.to_string(),
                run_mode: run_mode.to_string(),
                memory_required_gb: (fit.memory_required_gb * 10.0).round() / 10.0,
                memory_available_gb: (fit.memory_available_gb * 10.0).round() / 10.0,
                estimated_tps: (fit.estimated_tps * 10.0).round() / 10.0,
                score: fit.score,
                notes: fit.notes,
            }
        })
        .collect()
}
