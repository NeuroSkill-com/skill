// SPDX-License-Identifier: GPL-3.0-only
//! Daemon background tasks: skills sync, DND polling, auto-scanner.
//!
//! Previously lived in Tauri's `background.rs`.  Now daemon-authoritative.

use std::time::Duration;

use tracing::info;

use crate::routes::settings_io::load_user_settings;
use crate::state::AppState;

/// Spawn all daemon background tasks.
pub fn spawn_all(state: AppState) {
    spawn_skills_sync(state.clone());
    spawn_dnd_poll(state.clone());
    spawn_auto_scanner(state);
}

fn spawn_auto_scanner(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        // Start scanner automatically on daemon boot.
        let wifi_config = state.scanner_wifi_config.lock().ok().map(|g| g.clone());
        if let Some(cfg) = wifi_config {
            if !cfg.wifi_shield_ip.is_empty() || !cfg.galea_ip.is_empty() {
                info!("[scanner] auto-starting with wifi config");
            }
        }
        // The scanner is started by Tauri's scanner_start command which calls
        // POST /v1/control/scanner/start. We just ensure the daemon is ready.
    });
}

fn spawn_skills_sync(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(45)).await;
        let mut first_run = true;
        loop {
            let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
            let settings = load_user_settings(&state);
            let interval_secs = settings.llm.tools.skills_refresh_interval_secs;
            let sync_on_launch = settings.llm.tools.skills_sync_on_launch;

            let force_launch = first_run && sync_on_launch;
            let effective_interval = if force_launch { 0 } else { interval_secs };
            first_run = false;

            if force_launch || interval_secs > 0 {
                info!("[skills-sync] checking for community skills update");
                let sd = skill_dir.clone();
                let iv = effective_interval;
                let outcome = tokio::task::spawn_blocking(move || skill_skills::sync::sync_skills(&sd, iv, None)).await;

                match outcome {
                    Ok(skill_skills::sync::SyncOutcome::Updated { elapsed_ms, .. }) => {
                        info!("[skills-sync] updated in {elapsed_ms} ms");
                        state.broadcast("skills-updated", serde_json::json!({}));
                    }
                    Ok(skill_skills::sync::SyncOutcome::Fresh { next_sync_in_secs }) => {
                        info!("[skills-sync] fresh, next check in {next_sync_in_secs} s");
                    }
                    Ok(skill_skills::sync::SyncOutcome::Failed(e)) => {
                        info!("[skills-sync] failed: {e}");
                    }
                    Err(e) => {
                        info!("[skills-sync] task panic: {e}");
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
}

fn spawn_dnd_poll(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let mut prev_os_active: Option<bool> = None;
        loop {
            let os_now = skill_data::dnd::query_os_active();

            if os_now != prev_os_active {
                prev_os_active = os_now;
                state.broadcast("dnd-os-changed", serde_json::json!({"os_active": os_now}));
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}
