// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tauri commands for the LLM settings tab.
//!
//! All commands are gated behind the `llm` feature flag.  When the feature
//! is absent these stubs are compiled but simply return an error / empty value,
//! keeping the frontend command calls valid regardless of the build config.

use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::MutexExt;
use crate::AppState;
use super::catalog::{DownloadProgress, DownloadState, LlmCatalog};
use super::{LlmLogEntry, LlmStatus, cell_status, push_log};

// ── Catalog query ──────────────────────────────────────────────────────────────

/// Return the current LLM model catalog (all entries, their download states,
/// and the active model / mmproj selections).
///
/// The frontend polls this every ~2 s while the LLM tab is visible.
#[tauri::command]
pub fn get_llm_catalog(
    state: tauri::State<'_, Mutex<AppState>>,
) -> LlmCatalog {
    let mut s = state.lock_or_recover();
    // Sync in-flight download progress into the catalog entries before
    // returning so the UI always sees the latest state.
    let downloads = s.llm_downloads.clone();
    for (filename, prog_arc) in &downloads {
        if let Ok(prog) = prog_arc.lock() {
            if let Some(entry) = s.llm_catalog.entries
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
    s.llm_catalog.clone()
}

/// Persist the catalog to disk (called after state changes).
fn save_catalog(app: &AppHandle, state: &std::sync::MutexGuard<'_, AppState>) {
    state.llm_catalog.save(&state.skill_dir);
    let _ = app; // suppress unused warning
}

// ── Active model selection ────────────────────────────────────────────────────

/// Set the active LLM model (by filename).
/// The selection is persisted to `llm_catalog.json` immediately.
#[tauri::command]
pub fn set_llm_active_model(
    filename: String,
    app:      AppHandle,
    state:    tauri::State<'_, Mutex<AppState>>,
) {
    let mut s = state.lock_or_recover();
    s.llm_catalog.active_model = filename;
    // Mirror into LlmConfig.model_path so the server picks it up on restart.
    let path = s.llm_catalog.active_model_path();
    s.llm_config.model_path = path;
    save_catalog(&app, &s);
    crate::save_settings_handle(&app);
}

/// Toggle whether the vision projector is auto-loaded when the server starts.
#[tauri::command]
pub fn set_llm_autoload_mmproj(
    enabled: bool,
    app:     AppHandle,
    state:   tauri::State<'_, Mutex<AppState>>,
) {
    let mut s = state.lock_or_recover();
    s.llm_config.autoload_mmproj = enabled;
    drop(s);
    crate::save_settings_handle(&app);
}

/// Set the active mmproj projector (by filename, or empty to disable).
#[tauri::command]
pub fn set_llm_active_mmproj(
    filename: String,
    app:      AppHandle,
    state:    tauri::State<'_, Mutex<AppState>>,
) {
    let mut s = state.lock_or_recover();
    s.llm_catalog.active_mmproj = filename.clone();
    // Mirror into LlmConfig.mmproj
    let path = if filename.is_empty() {
        None
    } else {
        s.llm_catalog.active_mmproj_path()
    };
    s.llm_config.mmproj = path;
    save_catalog(&app, &s);
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
    state:    tauri::State<'_, Mutex<AppState>>,
) {
    let (repo, _skill_dir, prog_arc) = {
        let mut s = state.lock_or_recover();

        // Find the entry.
        let entry = match s.llm_catalog.entries
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
        if s.llm_downloads.contains_key(&filename) {
            if let Some(prog) = s.llm_downloads.get(&filename) {
                if prog.lock().is_ok_and(|p| p.state == DownloadState::Downloading) {
                    return;
                }
            }
        }

        // Mark as downloading in the catalog immediately so the UI updates.
        if let Some(e) = s.llm_catalog.entries.iter_mut().find(|e| e.filename == filename) {
            e.state      = DownloadState::Downloading;
            e.status_msg = Some(format!("Queued: {}…", filename));
            e.progress   = 0.0;
        }

        // Create a shared progress object.
        let prog = std::sync::Arc::new(Mutex::new(DownloadProgress {
            filename:   filename.clone(),
            state:      DownloadState::Downloading,
            status_msg: Some(format!("Queued: {}…", filename)),
            progress:   0.0,
            cancelled:  false,
        }));

        s.llm_downloads.insert(filename.clone(), prog.clone());

        (entry.repo.clone(), s.skill_dir.clone(), prog)
    };

    let filename2 = filename.clone();
    let app2      = app.clone();

    tokio::task::spawn_blocking(move || {
        let result = super::catalog::download_file(&repo, &filename2, &prog_arc);

        // After completion / failure, refresh the catalog entry.
        if let Some(state_handle) = app2.try_state::<Mutex<AppState>>() {
            let mut s = state_handle.lock_or_recover();
            if let Some(entry) = s.llm_catalog.entries
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
                        if !entry.is_mmproj && s.llm_catalog.active_model.is_empty() {
                            s.llm_catalog.active_model = filename2.clone();
                        }
                    }
                    Err(ref e) if e == "cancelled" => {
                        entry.state      = DownloadState::Cancelled;
                        entry.status_msg = Some("Cancelled.".into());
                        entry.progress   = 0.0;
                    }
                    Err(e) => {
                        entry.state      = DownloadState::Failed;
                        entry.status_msg = Some(e);
                        entry.progress   = 0.0;
                    }
                }
            }
            // Mirror active model path to LlmConfig.
            let model_path  = s.llm_catalog.active_model_path();
            let mmproj_path = s.llm_catalog.active_mmproj_path();
            s.llm_config.model_path = model_path;
            s.llm_config.mmproj     = mmproj_path;
            // Remove the in-flight progress entry.
            s.llm_downloads.remove(&filename2);
            save_catalog(&app2, &s);
            drop(s);
            crate::save_settings_handle(&app2);
        }
    });
}

/// Cancel an in-progress download by filename.
#[tauri::command]
pub fn cancel_llm_download(
    filename: String,
    state:    tauri::State<'_, Mutex<AppState>>,
) {
    let s = state.lock_or_recover();
    if let Some(prog) = s.llm_downloads.get(&filename) {
        if let Ok(mut p) = prog.lock() {
            p.cancelled = true;
        }
    }
}

/// Delete a locally-cached model file and reset its catalog entry.
///
/// Uses the HuggingFace Hub cache layout to locate the file, then removes it.
#[tauri::command]
pub fn delete_llm_model(
    filename: String,
    app:      AppHandle,
    state:    tauri::State<'_, Mutex<AppState>>,
) {
    let mut s = state.lock_or_recover();
    if let Some(entry) = s.llm_catalog.entries
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

        // Clear active selection if this was the active model/mmproj.
        if s.llm_catalog.active_model == filename {
            s.llm_catalog.active_model = String::new();
            s.llm_config.model_path    = None;
        }
        if s.llm_catalog.active_mmproj == filename {
            s.llm_catalog.active_mmproj = String::new();
            s.llm_config.mmproj         = None;
        }
    }
    save_catalog(&app, &s);
    drop(s);
    crate::save_settings_handle(&app);
}

/// Force-refresh the catalog by re-probing the HuggingFace Hub disk cache.
/// Useful after the user downloads a file externally.
#[tauri::command]
pub fn refresh_llm_catalog(
    app:   AppHandle,
    state: tauri::State<'_, Mutex<AppState>>,
) {
    let mut s = state.lock_or_recover();
    s.llm_catalog.refresh_cache();
    s.llm_catalog.auto_select();
    let model_path  = s.llm_catalog.active_model_path();
    let mmproj_path = s.llm_catalog.active_mmproj_path();
    s.llm_config.model_path = model_path;
    s.llm_config.mmproj     = mmproj_path;
    save_catalog(&app, &s);
    drop(s);
    crate::save_settings_handle(&app);
}

/// Return all buffered LLM server log entries (up to 500 most recent).
#[tauri::command]
pub fn get_llm_logs(state: tauri::State<'_, Mutex<AppState>>) -> Vec<LlmLogEntry> {
    let s      = state.lock_or_recover();
    let log    = s.llm_logs.lock().unwrap();
    let result: Vec<LlmLogEntry> = log.iter().cloned().collect();
    result
}

// ── Server lifecycle ───────────────────────────────────────────────────────────

/// Start the LLM inference server.
///
/// Returns `"started"` on success or an error string on failure.
/// No-ops (returns `"already_running"`) if the server is already up.
#[tauri::command]
pub async fn start_llm_server(
    app:   AppHandle,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<String, String> {
    let (mut config, catalog, log_buf, cell, skill_dir) = {
        let s = state.lock_or_recover();
        (
            s.llm_config.clone(),
            s.llm_catalog.clone(),
            s.llm_logs.clone(),
            s.llm_state_cell.clone(),
            s.skill_dir.clone(),
        )
    };

    if cell.lock().unwrap().is_some() {
        return Ok("already_running".to_string());
    }

    push_log(&app, &log_buf, "info", "start_llm_server command received");

    // If no mmproj is explicitly set but autoload is on, resolve the best one.
    if config.mmproj.is_none() {
        config.mmproj = catalog.resolve_mmproj_path(config.autoload_mmproj);
        if let Some(ref p) = config.mmproj {
            push_log(&app, &log_buf, "info",
                &format!("autoload_mmproj: selected {}", p.display()));
        }
    }

    // Run init on a blocking thread — it loads the model which can take seconds.
    let new_state = tokio::task::spawn_blocking(move || {
        crate::llm::init(&config, &catalog, app, log_buf, &skill_dir)
    }).await.map_err(|e| e.to_string())?;

    match new_state {
        Some(s) => {
            *cell.lock().unwrap() = Some(s);
            Ok("started".to_string())
        }
        None => Err(
            "Failed to start LLM server. \
             Check that a model is downloaded and selected in Settings → LLM.".to_string()
        ),
    }
}

/// Stop the LLM inference server gracefully.
///
/// Drops the actor's send channel (causing `blocking_recv` to return `None`),
/// then **blocks until the actor thread has fully exited** — ensuring all GPU
/// resources (Metal/CUDA command encoders, KV cache, model weights) are
/// released before this function returns.
#[tauri::command]
pub fn stop_llm_server(
    app:   AppHandle,
    state: tauri::State<'_, Mutex<AppState>>,
) {
    let (cell, log_buf) = {
        let s = state.lock_or_recover();
        (s.llm_state_cell.clone(), s.llm_logs.clone())
    };
    // Take the Arc out of the cell.  If this is the last Arc (it always
    // should be — only the cell holds a long-lived reference), dropping it
    // closes req_tx, the actor exits its loop, and shutdown() joins the thread.
    // Lock, take, and immediately release the guard so `cell` can be dropped.
    let server_state = { cell.lock().unwrap().take() };
    if let Some(server_state) = server_state {
        push_log(&app, &log_buf, "info", "stopping LLM server — waiting for actor to exit…");
        match std::sync::Arc::try_unwrap(server_state) {
            Ok(owned) => owned.shutdown(),
            Err(arc)  => drop(arc), // in-flight handler; actor exits when last Arc drops
        }
        push_log(&app, &log_buf, "info", "LLM server stopped");
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
}

#[tauri::command]
pub fn get_llm_server_status(
    state: tauri::State<'_, Mutex<AppState>>,
) -> LlmServerStatusResponse {
    let s = state.lock_or_recover();
    let (status, model_name) = cell_status(&s.llm_state_cell);
    let (n_ctx, supports_vision) = s.llm_state_cell.lock().unwrap()
        .as_ref()
        .map(|srv| (
            srv.n_ctx.load(std::sync::atomic::Ordering::Relaxed),
            srv.vision_ready.load(std::sync::atomic::Ordering::Relaxed),
        ))
        .unwrap_or((0, false));
    LlmServerStatusResponse { status, model_name, n_ctx, supports_vision }
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
    state: tauri::State<'_, Mutex<AppState>>,
) -> ChatSessionResponse {
    let mut s = state.lock_or_recover();
    let Some(store) = s.chat_store.as_mut() else {
        return ChatSessionResponse { session_id: 0, messages: vec![] };
    };
    let session_id = store.get_or_create_last_session();
    let messages   = store.load_session(session_id);
    ChatSessionResponse { session_id, messages }
}

/// Append a single message to a chat session.
/// Returns the new message row id, or 0 if the store is unavailable.
#[tauri::command]
pub fn save_chat_message(
    session_id: i64,
    role:       String,
    content:    String,
    thinking:   Option<String>,
    state:      tauri::State<'_, Mutex<AppState>>,
) -> i64 {
    let mut s = state.lock_or_recover();
    let Some(store) = s.chat_store.as_mut() else { return 0; };
    store.save_message(session_id, &role, &content, thinking.as_deref())
}

/// Create a fresh chat session and return its id.
/// Called when the user clicks "New Chat".
#[tauri::command]
pub fn new_chat_session(
    state: tauri::State<'_, Mutex<AppState>>,
) -> i64 {
    let mut s = state.lock_or_recover();
    let Some(store) = s.chat_store.as_mut() else { return 0; };
    store.new_session()
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
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}
