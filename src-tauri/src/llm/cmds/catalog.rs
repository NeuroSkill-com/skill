// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Catalog query and external-model registration commands.

use serde::Serialize;
use std::sync::Mutex;
use tauri::AppHandle;

use super::{ensure_catalog_entry, save_catalog};
use crate::llm::catalog::{DownloadState, LlmCatalog};
use crate::AppState;
use crate::MutexExt;

#[derive(Debug, Clone, Serialize)]
pub struct LlmDownloadItem {
    pub repo: String,
    pub filename: String,
    pub quant: String,
    pub size_gb: f32,
    pub description: String,
    pub is_mmproj: bool,
    pub state: DownloadState,
    pub status_msg: Option<String>,
    pub progress: f32,
    pub initiated_at_unix: Option<u64>,
    pub local_path: Option<std::path::PathBuf>,
    pub shard_count: u16,
    pub current_shard: u16,
}

// ── Catalog query ──────────────────────────────────────────────────────────────

/// Return the current LLM model catalog (all entries, their download states,
/// and the active model / mmproj selections).
///
/// The frontend polls this every ~2 s while the LLM tab is visible.
#[tauri::command]
pub fn get_llm_catalog(state: tauri::State<'_, Mutex<Box<AppState>>>) -> LlmCatalog {
    let mut s = state.lock_or_recover();
    sync_download_progress(&mut s);
    {
        let __a = s.llm.clone();
        let __r = __a.lock_or_recover().catalog.clone();
        __r
    }
}

#[tauri::command]
pub fn get_llm_downloads(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<LlmDownloadItem> {
    let mut s = state.lock_or_recover();
    sync_download_progress(&mut s);
    let __llm_arc = s.llm.clone();
    let llm = __llm_arc.lock_or_recover();

    let mut items: Vec<LlmDownloadItem> = llm
        .catalog
        .entries
        .iter()
        .filter(|e| {
            e.state == DownloadState::Downloading
                || e.state == DownloadState::Paused
                || e.state == DownloadState::Failed
                || e.state == DownloadState::Cancelled
                || e.state == DownloadState::Downloaded
        })
        .map(|e| {
            // Read shard progress from the in-flight download if available.
            let (current_shard, _total_shards) = llm
                .downloads
                .get(&e.filename)
                .and_then(|prog| prog.lock().ok().map(|p| (p.current_shard, p.total_shards)))
                .unwrap_or((0, 0));
            LlmDownloadItem {
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
                shard_count: e.shard_count() as u16,
                current_shard,
            }
        })
        .collect();

    items.sort_by(|a, b| {
        b.initiated_at_unix
            .unwrap_or(0)
            .cmp(&a.initiated_at_unix.unwrap_or(0))
    });
    items
}

/// Sync in-flight download progress into the catalog entries so the UI sees
/// the latest state.
fn sync_download_progress(s: &mut AppState) {
    let __llm_arc = s.llm.clone();
    let mut llm = __llm_arc.lock_or_recover();
    let downloads = llm.downloads.clone();
    for (filename, prog_arc) in &downloads {
        if let Ok(prog) = prog_arc.lock() {
            if let Some(entry) = llm
                .catalog
                .entries
                .iter_mut()
                .find(|e| &e.filename == filename)
            {
                entry.state = prog.state.clone();
                entry.status_msg = prog.status_msg.clone();
                entry.progress = prog.progress;
                if prog.state == DownloadState::Downloaded {
                    entry.local_path = entry.resolve_cached();
                }
            }
        }
    }
}

// ── Add external model ────────────────────────────────────────────────────────

/// Add an external model from any HuggingFace repo to the catalog and
/// optionally start downloading it.
///
/// If the `filename` already exists in the catalog, returns its existing entry
/// without creating a duplicate.  The `size_gb` can be 0.0 — the actual size
/// will be discovered during download from the HF API.
///
/// Returns the created (or existing) entry's filename.
#[tauri::command]
pub fn add_llm_model(
    repo: String,
    filename: String,
    size_gb: Option<f32>,
    mmproj: Option<String>,
    download: Option<bool>,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<String, String> {
    let should_download = download.unwrap_or(true);

    {
        let mut s = state.lock_or_recover();
        ensure_catalog_entry(&mut s, &repo, &filename, size_gb, Some(false));
        if let Some(ref mm) = mmproj {
            ensure_catalog_entry(&mut s, &repo, mm, None, Some(true));
        }
        save_catalog(&app, &s);
    }

    if should_download {
        super::downloads::download_llm_model(filename.clone(), app.clone(), state.clone());
        if let Some(ref mm) = mmproj {
            super::downloads::download_llm_model(mm.clone(), app, state);
        }
    }

    Ok(filename)
}

// ── Refresh catalog ───────────────────────────────────────────────────────────

/// Force-refresh the catalog by re-probing the HuggingFace Hub disk cache.
/// Useful after the user downloads a file externally.
#[tauri::command]
pub fn refresh_llm_catalog(app: AppHandle, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let s = state.lock_or_recover();
    {
        let __llm_arc = s.llm.clone();
        let mut llm = __llm_arc.lock_or_recover();
        llm.catalog.refresh_cache();
        llm.catalog.auto_select();
        let model_path = llm.catalog.active_model_path();
        let mmproj_path = llm.catalog.active_mmproj_path();
        llm.config.model_path = model_path;
        llm.config.mmproj = mmproj_path;
    }
    save_catalog(&app, &s);
    drop(s);
    crate::save_settings_handle(&app);
    crate::tray::refresh_tray(&app);
}
