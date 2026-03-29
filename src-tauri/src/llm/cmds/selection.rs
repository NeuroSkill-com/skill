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
    let s = state.lock_or_recover();
    let __llm_arc = s.llm.clone();
    let mut llm = __llm_arc.lock_or_recover();
    if filename.is_empty() {
        llm.catalog.active_mmproj.clear();
    } else {
        let current_matches = llm
            .catalog
            .active_model_entry()
            .zip(
                llm.catalog
                    .entries
                    .iter()
                    .find(|e| e.is_mmproj() && e.filename == filename),
            )
            .is_some_and(|(model, mmproj)| model.repo == mmproj.repo);

        if !current_matches {
            if let Some(model_filename) = llm
                .catalog
                .best_model_for_mmproj(&filename)
                .map(|entry| entry.filename.clone())
            {
                llm.catalog.active_model = model_filename;
            }
        }

        if llm
            .catalog
            .active_model_entry()
            .zip(
                llm.catalog
                    .entries
                    .iter()
                    .find(|e| e.is_mmproj() && e.filename == filename),
            )
            .is_some_and(|(model, mmproj)| model.repo == mmproj.repo)
        {
            llm.catalog.active_mmproj = filename;
        } else {
            llm.catalog.active_mmproj.clear();
        }
    }

    llm.config.model_path = llm.catalog.active_model_path();
    llm.config.mmproj = llm.catalog.active_mmproj_path();
    save_catalog_locked(&app, &s.skill_dir, &llm);
    drop(llm);
    drop(s);
    crate::save_settings_handle(&app);
}
