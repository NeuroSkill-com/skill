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
    spawn_auto_scanner(state.clone());
    spawn_auto_connect(state.clone());
    spawn_calibration_auto_start(state);
}

fn spawn_calibration_auto_start(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let settings = load_user_settings(&state);
        let active_id = &settings.active_calibration_id;
        let auto_start_id = settings
            .calibration_profiles
            .iter()
            .find(|p| p.id == *active_id && p.auto_start)
            .map(|p| p.id.clone());
        if let Some(id) = auto_start_id {
            info!("[calibration] auto-start profile: {id}");
            state.broadcast("calibration-auto-start", serde_json::json!({"profile_id": id}));
        }
    });
}

fn spawn_auto_connect(state: AppState) {
    tokio::spawn(async move {
        // Wait for scanner to start discovering devices.
        tokio::time::sleep(Duration::from_millis(900)).await;

        let settings = load_user_settings(&state);
        let preferred = settings
            .preferred_id
            .or_else(|| settings.paired.first().map(|d| d.id.clone()));

        if let Some(preferred_id) = preferred {
            info!("[auto-connect] preferred device: {preferred_id}");

            // Set preferred in discovered devices list.
            if let Ok(mut guard) = state.devices.lock() {
                for d in guard.iter_mut() {
                    d.is_preferred = d.id == preferred_id;
                }
            }

            // Set target in status so retry-connect knows where to connect.
            if let Ok(mut status) = state.status.lock() {
                status.target_id = Some(preferred_id);
            }

            // Enable reconnect.
            if let Ok(mut rc) = state.reconnect.lock() {
                rc.pending = true;
            }

            // Trigger connect via the handler.
            use axum::extract::State;
            let _ = crate::handlers::control_retry_connect(State(state.clone())).await;
            info!("[auto-connect] connect triggered");
        }
    });
}

fn spawn_auto_scanner(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Load wifi config from settings and apply it before starting.
        let settings = load_user_settings(&state);
        if !settings.openbci.wifi_shield_ip.is_empty() || !settings.openbci.galea_ip.is_empty() {
            if let Ok(mut guard) = state.scanner_wifi_config.lock() {
                guard.wifi_shield_ip = settings.openbci.wifi_shield_ip.clone();
                guard.galea_ip = settings.openbci.galea_ip.clone();
            }
            info!("[scanner] applied wifi config from settings");
        }

        // Auto-start the scanner so devices are discoverable immediately.
        info!("[scanner] auto-starting on daemon boot");
        crate::handlers::start_scanner_inner(&state);
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
