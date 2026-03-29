// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Download / cancel / pause / resume / delete commands.

use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use super::{save_catalog, save_catalog_locked};
use crate::llm::catalog::{DownloadProgress, DownloadState};
use crate::tray::refresh_tray;
use crate::AppState;
use crate::MutexExt;

// ── Download ──────────────────────────────────────────────────────────────────

/// Start downloading a GGUF file by filename.
///
/// Spawns a blocking task so the UI stays responsive.  The download progress
/// can be observed by polling `get_llm_catalog()` every few seconds.
///
/// If a download for this file is already in progress, this is a no-op.
#[tauri::command]
pub fn download_llm_model(
    filename: String,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let (entry_snapshot, prog_arc) = {
        let s = state.lock_or_recover();
        let __llm_arc = s.llm.clone();
        let mut llm = __llm_arc.lock_or_recover();

        // Find the entry.
        let entry = match llm.catalog.entries.iter().find(|e| e.filename == filename) {
            Some(e) => e.clone(),
            None => {
                eprintln!("[llm] download_llm_model: unknown filename '{filename}'");
                return;
            }
        };

        // Skip if already downloading.
        if llm.downloads.contains_key(&filename) {
            if let Some(prog) = llm.downloads.get(&filename) {
                if prog
                    .lock()
                    .is_ok_and(|p| p.state == DownloadState::Downloading)
                {
                    return;
                }
            }
        }

        // Mark as downloading in the catalog immediately so the UI updates.
        if let Some(e) = llm
            .catalog
            .entries
            .iter_mut()
            .find(|e| e.filename == filename)
        {
            e.state = DownloadState::Downloading;
            e.status_msg = Some(format!("Queued: {}…", filename));
            e.progress = 0.0;
            if e.initiated_at_unix.is_none() {
                e.initiated_at_unix = Some(crate::unix_secs());
            }
        }

        // Create a shared progress object.
        let prog = std::sync::Arc::new(Mutex::new(DownloadProgress {
            filename: filename.clone(),
            state: DownloadState::Downloading,
            status_msg: Some(format!("Queued: {}…", filename)),
            progress: 0.0,
            cancelled: false,
            pause_requested: false,
            current_shard: 0,
            total_shards: entry.shard_count() as u16,
        }));

        llm.downloads.insert(filename.clone(), prog.clone());

        (entry, prog)
    };

    refresh_tray(&app);

    // Spawn tray-refresh watcher.
    let watch_app = app.clone();
    let watch_prog = prog_arc.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_bucket: Option<u8> = None;
        let mut last_state = DownloadState::NotDownloaded;

        loop {
            let Some((state, bucket)) = watch_prog.lock().ok().map(|prog| {
                (
                    prog.state.clone(),
                    ((prog.progress.clamp(0.0, 1.0) * 20.0).round() as u8).min(20),
                )
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
    let app2 = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let result = crate::llm::catalog::download_model(&entry_snapshot, &prog_arc);

        // After completion / failure, refresh the catalog entry.
        if let Some(state_handle) = app2.try_state::<Mutex<Box<AppState>>>() {
            let s = state_handle.lock_or_recover();
            let __llm_arc = s.llm.clone();
            let mut llm = __llm_arc.lock_or_recover();
            if let Some(entry) = llm
                .catalog
                .entries
                .iter_mut()
                .find(|e| e.filename == filename2)
            {
                match result {
                    Ok(path) => {
                        entry.state = DownloadState::Downloaded;
                        entry.local_path = Some(path);
                        entry.status_msg = None;
                        entry.progress = 1.0;
                        if !entry.is_mmproj() {
                            let should_activate = llm.catalog.active_model.is_empty()
                                || llm.catalog.active_model_path().is_none_or(|p| !p.exists());
                            if should_activate {
                                llm.catalog.active_model = filename2.clone();
                            }
                        }
                    }
                    Err(ref e) if e.to_string() == "cancelled" => {
                        entry.state = DownloadState::Cancelled;
                        entry.status_msg = Some("Cancelled.".into());
                        entry.progress = 0.0;
                    }
                    Err(ref e) if e.to_string() == "paused" => {
                        entry.state = DownloadState::Paused;
                        entry.status_msg = Some("Paused.".into());
                    }
                    Err(e) => {
                        entry.state = DownloadState::Failed;
                        entry.status_msg = Some(e.to_string());
                        entry.progress = 0.0;
                    }
                }
            }
            let model_path = llm.catalog.active_model_path();
            let mmproj_path = llm.catalog.active_mmproj_path();
            llm.config.model_path = model_path;
            llm.config.mmproj = mmproj_path;
            if !matches!(
                llm.catalog
                    .entries
                    .iter()
                    .find(|e| e.filename == filename2)
                    .map(|e| e.state.clone()),
                Some(DownloadState::Paused)
            ) {
                llm.downloads.remove(&filename2);
            }
            save_catalog_locked(&app2, &s.skill_dir, &llm);
            drop(llm);
            drop(s);
            crate::save_settings_handle(&app2);
            refresh_tray(&app2);
        }
    });
}

// ── Cancel ────────────────────────────────────────────────────────────────────

fn cancel_llm_download_inner(
    filename: String,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
    app: Option<&AppHandle>,
) {
    let s = state.lock_or_recover();
    let __llm_arc = s.llm.clone();
    let mut llm = __llm_arc.lock_or_recover();
    let mut was_paused = false;
    if let Some(prog) = llm.downloads.get(&filename) {
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
        llm.downloads.remove(&filename);
        if let Some(entry) = llm
            .catalog
            .entries
            .iter_mut()
            .find(|e| e.filename == filename)
        {
            entry.state = DownloadState::Cancelled;
            entry.status_msg = Some("Cancelled.".into());
            entry.progress = 0.0;
        }
        if let Some(app) = app {
            drop(llm);
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
pub fn cancel_llm_download(filename: String, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    cancel_llm_download_inner(filename, state, None);
}

pub fn cancel_llm_download_with_app(
    filename: String,
    app: &AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    cancel_llm_download_inner(filename, state, Some(app));
}

// ── Pause / Resume ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn pause_llm_download(filename: String, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let s = state.lock_or_recover();
    let __llm_arc = s.llm.clone();
    let mut llm = __llm_arc.lock_or_recover();
    if let Some(prog) = llm.downloads.get(&filename) {
        if let Ok(mut p) = prog.lock() {
            p.cancelled = true;
            p.pause_requested = true;
            p.status_msg = Some("Pausing…".into());
        }
    }
    if let Some(entry) = llm
        .catalog
        .entries
        .iter_mut()
        .find(|e| e.filename == filename)
    {
        entry.state = DownloadState::Paused;
        entry.status_msg = Some("Pausing…".into());
    }
}

#[tauri::command]
pub fn resume_llm_download(
    filename: String,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    {
        let s = state.lock_or_recover();
        let __llm_arc = s.llm.clone();
        let mut llm = __llm_arc.lock_or_recover();
        if let Some(entry) = llm
            .catalog
            .entries
            .iter_mut()
            .find(|e| e.filename == filename)
        {
            entry.state = DownloadState::NotDownloaded;
            entry.status_msg = None;
        }
        llm.downloads.remove(&filename);
        drop(llm);
        save_catalog(&app, &s);
    }
    download_llm_model(filename, app, state);
}

// ── Delete ────────────────────────────────────────────────────────────────────

/// Delete a locally-cached model file and reset its catalog entry.
///
/// Uses the HuggingFace Hub cache layout to locate the file, then removes it.
#[tauri::command]
pub fn delete_llm_model(
    filename: String,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let s = state.lock_or_recover();
    {
        let __llm_arc = s.llm.clone();
        let mut llm = __llm_arc.lock_or_recover();
        if let Some(entry) = llm
            .catalog
            .entries
            .iter_mut()
            .find(|e| e.filename == filename)
        {
            if entry.is_split() {
                let (cached_paths, _) = entry.resolve_cached_shards();
                for path in cached_paths {
                    if path.exists() {
                        if let Err(e) = std::fs::remove_file(&path) {
                            eprintln!("[llm] delete shard failed for {}: {e}", path.display());
                        }
                    }
                }
            } else if let Some(ref path) = entry.local_path {
                if path.exists() {
                    if let Err(e) = std::fs::remove_file(path) {
                        eprintln!("[llm] delete failed for {}: {e}", path.display());
                    }
                }
            }
            entry.local_path = None;
            entry.state = DownloadState::NotDownloaded;
            entry.status_msg = None;
            entry.progress = 0.0;
            entry.initiated_at_unix = None;

            if llm.catalog.active_model == filename {
                llm.catalog.active_model = String::new();
                llm.config.model_path = None;
            }
            if llm.catalog.active_mmproj == filename {
                llm.catalog.active_mmproj = String::new();
                llm.config.mmproj = None;
            }
        }
    }
    save_catalog(&app, &s);
    drop(s);
    crate::save_settings_handle(&app);
    refresh_tray(&app);
}

// ── Downloads window ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn open_downloads_window(app: AppHandle) -> Result<(), String> {
    crate::window_cmds::focus_or_create(
        &app,
        crate::window_cmds::WindowSpec {
            label: "downloads",
            route: "downloads",
            title: "NeuroSkill™ – Downloads",
            inner_size: (760.0, 640.0),
            min_inner_size: Some((560.0, 420.0)),
            ..Default::default()
        },
    )
}
