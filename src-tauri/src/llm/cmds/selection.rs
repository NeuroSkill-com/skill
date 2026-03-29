// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Active model & mmproj selection commands.

use std::sync::Mutex;
use tauri::AppHandle;

use super::save_catalog_locked;
use crate::AppState;
use crate::MutexExt;

/// Set the active LLM model (by filename).
/// The selection is persisted to `llm_catalog.json` immediately.
#[tauri::command]
pub fn set_llm_active_model(
    filename: String,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let s = state.lock_or_recover();
    let __llm_arc = s.llm.clone();
    let mut llm = __llm_arc.lock_or_recover();
    llm.catalog.active_model = filename;
    if !llm.catalog.active_mmproj_matches_active_model() {
        llm.catalog.active_mmproj.clear();
    }
    // Mirror into LlmConfig so the server picks the updated pair up on restart.
    llm.config.model_path = llm.catalog.active_model_path();
    llm.config.mmproj = llm.catalog.active_mmproj_path();
    save_catalog_locked(&app, &s.skill_dir, &llm);
    drop(llm);
    drop(s);
    crate::save_settings_handle(&app);
}

/// Toggle whether the vision projector is auto-loaded when the server starts.
#[tauri::command]
pub fn set_llm_autoload_mmproj(
    enabled: bool,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let s = state.lock_or_recover();
    let __llm_arc = s.llm.clone();
    let mut llm = __llm_arc.lock_or_recover();
    llm.config.autoload_mmproj = enabled;
    drop(s);
    crate::save_settings_handle(&app);
}

/// Set the active mmproj projector (by filename, or empty to disable).
#[tauri::command]
pub fn set_llm_active_mmproj(
    filename: String,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    log::info!("[llm] set_llm_active_mmproj called with filename={filename:?}");
    let s = state.lock_or_recover();
    let __llm_arc = s.llm.clone();
    let mut llm = __llm_arc.lock_or_recover();
    if filename.is_empty() {
        log::info!("[llm] clearing active_mmproj (empty filename)");
        llm.catalog.active_mmproj.clear();
    } else {
        let mmproj_entry_found = llm
            .catalog
            .entries
            .iter()
            .find(|e| e.is_mmproj() && e.filename == filename);
        let active_model = llm.catalog.active_model_entry().map(|e| e.repo.as_str());

        log::info!(
            "[llm] mmproj entry found={}, active_model_repo={:?}, mmproj_entry is_mmproj_field={:?} state={:?}",
            mmproj_entry_found.is_some(),
            active_model,
            mmproj_entry_found.map(|e| e.is_mmproj),
            mmproj_entry_found.map(|e| &e.state),
        );

        let current_matches = llm
            .catalog
            .active_model_entry()
            .zip(mmproj_entry_found)
            .is_some_and(|(model, mmproj)| model.repo == mmproj.repo);

        log::info!("[llm] current_matches={current_matches}");

        if !current_matches {
            if let Some(model_filename) = llm
                .catalog
                .best_model_for_mmproj(&filename)
                .map(|entry| entry.filename.clone())
            {
                log::info!(
                    "[llm] switching active model to {model_filename} for mmproj compatibility"
                );
                llm.catalog.active_model = model_filename;
            } else {
                log::warn!("[llm] no compatible downloaded model found for mmproj {filename}");
            }
        }

        // Re-check after potential model switch.
        let final_match = llm
            .catalog
            .active_model_entry()
            .zip(
                llm.catalog
                    .entries
                    .iter()
                    .find(|e| e.is_mmproj() && e.filename == filename),
            )
            .is_some_and(|(model, mmproj)| model.repo == mmproj.repo);

        if final_match {
            log::info!("[llm] setting active_mmproj={filename}");
            llm.catalog.active_mmproj = filename;
        } else {
            log::warn!("[llm] repos still don't match after model switch — clearing active_mmproj");
            llm.catalog.active_mmproj.clear();
        }
    }

    let model_path = llm.catalog.active_model_path();
    let mmproj_path = llm.catalog.active_mmproj_path();
    log::info!(
        "[llm] after set_llm_active_mmproj: active_mmproj={:?} model_path={:?} mmproj_path={:?}",
        llm.catalog.active_mmproj,
        model_path,
        mmproj_path,
    );
    llm.config.model_path = model_path;
    llm.config.mmproj = mmproj_path;
    save_catalog_locked(&app, &s.skill_dir, &llm);
    drop(llm);
    drop(s);
    crate::save_settings_handle(&app);
}
