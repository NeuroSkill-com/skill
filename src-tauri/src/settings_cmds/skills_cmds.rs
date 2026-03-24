// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Community skills sync, listing, and disable-list Tauri commands.

use std::sync::Mutex;
use crate::MutexExt;
use tauri::{AppHandle, Emitter};

use crate::AppState;

// ── Skills refresh ─────────────────────────────────────────────────────────────

/// Return the current community-skills refresh interval in seconds (0 = disabled).
#[tauri::command]
pub fn get_skills_refresh_interval(state: tauri::State<'_, Mutex<Box<AppState>>>) -> u64 {
    { let __a = state.lock_or_recover().llm.clone(); let __r = __a.lock_or_recover().config.tools.skills_refresh_interval_secs; __r }
}

/// Persist a new skills refresh interval.  `secs` = 0 disables auto-refresh.
#[tauri::command]
pub fn set_skills_refresh_interval(
    secs:  u64,
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    { let __a = state.lock_or_recover().llm.clone(); __a.lock_or_recover().config.tools.skills_refresh_interval_secs = secs; }
    crate::save_settings(&app);
}

/// Return whether skills should be synced on every app launch.
#[tauri::command]
pub fn get_skills_sync_on_launch(state: tauri::State<'_, Mutex<Box<AppState>>>) -> bool {
    { let __a = state.lock_or_recover().llm.clone(); let __r = __a.lock_or_recover().config.tools.skills_sync_on_launch; __r }
}

/// Persist the sync-on-launch flag.
#[tauri::command]
pub fn set_skills_sync_on_launch(
    enabled: bool,
    app:     AppHandle,
    state:   tauri::State<'_, Mutex<Box<AppState>>>,
) {
    { let __a = state.lock_or_recover().llm.clone(); __a.lock_or_recover().config.tools.skills_sync_on_launch = enabled; }
    crate::save_settings(&app);
}

/// Return the Unix timestamp of the last successful skills sync, or `null`.
#[tauri::command]
pub fn get_skills_last_sync(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Option<u64> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    skill_skills::sync::last_sync_ts(&skill_dir)
}

/// Trigger a manual skills sync (ignores the interval, forces re-download).
#[tauri::command]
pub async fn sync_skills_now(
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<String, String> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        skill_skills::sync::sync_skills(&skill_dir, 0, None)
    })
    .await
    .map_err(|e| format!("task panic: {e}"))?;

    match outcome {
        skill_skills::sync::SyncOutcome::Updated { elapsed_ms, .. } => {
            let _ = app.emit("skills-updated", ());
            Ok(format!("updated in {elapsed_ms} ms"))
        }
        skill_skills::sync::SyncOutcome::Fresh { .. } => {
            Ok("already up to date".into())
        }
        skill_skills::sync::SyncOutcome::Failed(e) => Err(e),
    }
}

// ── Discovered skills listing ──────────────────────────────────────────────────

/// A lightweight description of a discovered skill, sent to the frontend.
#[derive(serde::Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub source: String,
    pub enabled: bool,
}

/// List all discovered skills from the user's skill_dir (and bundled/project
/// directories).  Each entry includes whether it is currently enabled based on
/// the `disabled_skills` list in settings.
#[tauri::command]
pub fn list_skills(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<SkillInfo> {
    let (skill_dir, disabled) = {
        let g = state.lock_or_recover();
        let sd = g.skill_dir.clone();
        let __a = g.llm.clone();
        drop(g);
        let __r = __a.lock_or_recover().config.tools.disabled_skills.clone();
        (sd, __r)
    };

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let bundled_dir = exe_dir.as_ref()
        .map(|d| d.join(skill_constants::SKILLS_SUBDIR))
        .filter(|d| d.is_dir())
        .or_else(|| {
            let cwd = std::env::current_dir().ok()?;
            let p = cwd.join(skill_constants::SKILLS_SUBDIR);
            if p.is_dir() { Some(p) } else { None }
        });

    let result = skill_skills::load_skills(&skill_skills::LoadSkillsOptions {
        cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        skill_dir: skill_dir.to_path_buf(),
        bundled_dir,
        skill_paths: Vec::new(),
        include_defaults: true,
    });

    result.skills.into_iter().map(|s| {
        let enabled = !disabled.iter().any(|d| d == &s.name);
        SkillInfo {
            name: s.name,
            description: s.description,
            source: s.source,
            enabled,
        }
    }).collect()
}

/// Read the LICENSE file from the user's skills directory, if present.
#[tauri::command]
pub fn get_skills_license(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Option<String> {
    let skill_dir = state.lock_or_recover().skill_dir.clone();
    let license_path = skill_dir.join(skill_constants::SKILLS_SUBDIR).join("LICENSE");
    std::fs::read_to_string(&license_path).ok()
}

/// Return the list of disabled skill names.
#[tauri::command]
pub fn get_disabled_skills(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Vec<String> {
    { let __a = state.lock_or_recover().llm.clone(); let __r = __a.lock_or_recover().config.tools.disabled_skills.clone(); __r }
}

/// Persist the disabled-skills list.
#[tauri::command]
pub fn set_disabled_skills(
    names: Vec<String>,
    app:   AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    { let __a = state.lock_or_recover().llm.clone(); __a.lock_or_recover().config.tools.disabled_skills = names; }
    crate::save_settings(&app);

    // Live-update the running LLM server's tool config so the change takes
    // effect without a restart.
    #[cfg(feature = "llm")]
    {
        let (cell, tools) = {
            let g = state.lock_or_recover();
            { let __llm_arc = g.llm.clone(); let llm = __llm_arc.lock_or_recover(); (llm.state_cell.clone(), llm.config.tools.clone()) }
        };
        let server = cell.lock_or_recover().as_ref().cloned();
        if let Some(server) = server {
            server.set_allowed_tools(tools);
        }
    }
}
