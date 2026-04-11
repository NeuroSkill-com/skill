// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Background async tasks spawned during app setup.
//!
//! Each function spawns one long-lived tokio task.  Grouping them here keeps
//! `setup.rs` focused on one-shot initialisation and makes it easy to see
//! every recurring background loop at a glance.

use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::helpers::{apply_daemon_status, emit_status_from_daemon};
use crate::state::AppState;
use crate::MutexExt;

// Battery/signal warning logic lives in skill-daemon::monitor.

/// Adaptive poll delay: 5 s when connected, 2 s otherwise.
fn poll_delay_secs(state: &str) -> u64 {
    if state == "connected" {
        5
    } else {
        2
    }
}

// ── Public entry-point ───────────────────────────────────────────────────────

/// Spawn every background task.  Called once from `setup_app`.
pub(crate) fn spawn_all(app: &mut tauri::App) {
    // Scanner start and auto-connect now run in skill-daemon (background.rs).

    spawn_daemon_status_poll(app.handle());
    // Reconnect loop now runs in skill-daemon; no Tauri-side loop needed.
    spawn_daemon_log_tail();
    spawn_onboarding_check(app.handle());
    spawn_updater_poll(app.handle());
    // Skills sync and DND polling run in skill-daemon (background.rs).
}

// ── Individual tasks ─────────────────────────────────────────────────────────

fn spawn_daemon_status_poll(handle: &AppHandle) {
    let app = handle.clone();
    tauri::async_runtime::spawn(async move {
        // Wait for daemon to be ready before polling.
        tokio::time::sleep(Duration::from_secs(2)).await;

        // One-shot: check if daemon wants us to auto-start calibration.
        if let Ok(Ok(val)) = tokio::task::spawn_blocking(|| {
            crate::daemon_cmds::fetch_json_value_with_auth("/v1/calibration/auto-start-pending")
        })
        .await
        {
            if let Some(id) = val.get("profile_id").and_then(|v| v.as_str()) {
                let id = id.to_owned();
                let a = app.clone();
                tokio::spawn(async move {
                    let _ = crate::window_cmds::open_calibration_window_inner(&a, Some(id), false)
                        .await;
                });
            }
        }

        // Battery/signal warnings now run daemon-side (monitor.rs).
        // This loop only syncs daemon status into local Tauri state for the UI.
        // Watchdog: if the daemon is unreachable for several consecutive ticks,
        // attempt to restart it so the app doesn't stay in a degraded state.
        let mut consecutive_failures: u32 = 0;
        const WATCHDOG_THRESHOLD: u32 = 3;
        loop {
            let poll_result = tokio::task::spawn_blocking(crate::daemon_cmds::fetch_daemon_status)
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            match poll_result {
                Ok(daemon_status) => {
                    consecutive_failures = 0;
                    let changed = {
                        let r = app.state::<Mutex<Box<AppState>>>();
                        let s = r.lock_or_recover();
                        s.status.state != daemon_status.state
                            || s.status.device_name != daemon_status.device_name
                            || s.status.sample_count != daemon_status.sample_count
                            || s.status.device_error != daemon_status.device_error
                            || s.status.iroh_client_name != daemon_status.iroh_client_name
                            || s.status.phone_info != daemon_status.phone_info
                            || s.status.iroh_tunnel_online != daemon_status.iroh_tunnel_online
                            || s.status.iroh_connected_peers != daemon_status.iroh_connected_peers
                            || s.status.iroh_remote_device_connected
                                != daemon_status.iroh_remote_device_connected
                            || s.status.iroh_streaming_active != daemon_status.iroh_streaming_active
                            || s.status.iroh_eeg_streaming_active
                                != daemon_status.iroh_eeg_streaming_active
                    };
                    if changed {
                        {
                            let r = app.state::<Mutex<Box<AppState>>>();
                            let mut s = r.lock_or_recover();
                            apply_daemon_status(&mut s.status, daemon_status);
                        }
                        emit_status_from_daemon(&app);
                    }
                }
                Err(_) => {
                    consecutive_failures += 1;
                    if consecutive_failures == WATCHDOG_THRESHOLD {
                        eprintln!(
                            "[watchdog] daemon unreachable for {WATCHDOG_THRESHOLD} consecutive polls — restarting"
                        );
                        let _ = tokio::task::spawn_blocking(|| {
                            crate::daemon_cmds::ensure_daemon_running();
                        })
                        .await;
                    }
                }
            }
            let delay = {
                let r = app.state::<Mutex<Box<AppState>>>();
                let s = r.lock_or_recover();
                poll_delay_secs(&s.status.state)
            };
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
    });
}

fn spawn_daemon_log_tail() {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let mut since: u64 = 0;
        // On first call, skip any backlog — only show lines from now on.
        if let Ok((next, _)) = tokio::task::spawn_blocking(move || {
            crate::daemon_cmds::fetch_daemon_log_recent(u64::MAX)
        })
        .await
        .unwrap_or(Err(String::new()))
        {
            since = next;
        }
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let since_copy = since;
            if let Ok(Ok((next_seq, lines))) = tokio::task::spawn_blocking(move || {
                crate::daemon_cmds::fetch_daemon_log_recent(since_copy)
            })
            .await
            {
                for line in &lines {
                    eprintln!("[daemon] {line}");
                }
                since = next_seq;
            }
        }
    });
}

// Calibration auto-start decision now lives in skill-daemon (background.rs).
// The daemon broadcasts a "calibration-auto-start" event with { profile_id }.
// The WS event listener (spawn_daemon_ws_listener) opens the calibration window
// in response.

fn spawn_onboarding_check(handle: &AppHandle) {
    let app = handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        let done = {
            let r = app.state::<Mutex<Box<AppState>>>();
            let g = r.lock_or_recover();
            g.ui.onboarding_complete
        };
        if !done {
            let _ = crate::window_cmds::open_onboarding_window(app).await;
        }
    });
}

fn spawn_updater_poll(handle: &AppHandle) {
    let app = handle.clone();
    tauri::async_runtime::spawn(async move {
        use tauri_plugin_updater::UpdaterExt;
        let mut updater_platform_unsupported = false;
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            if updater_platform_unsupported {
                break;
            }
            eprintln!("[updater] running background update check");
            match app.updater() {
                Err(e) => eprintln!("[updater] cannot get updater: {e}"),
                Ok(updater) => {
                    let result =
                        tokio::time::timeout(Duration::from_secs(30), updater.check()).await;
                    match result {
                        Err(_) => eprintln!("[updater] check timed out after 30 s"),
                        Ok(Ok(Some(update))) => {
                            eprintln!("[updater] update available: {}", update.version);
                            let payload = serde_json::json!({
                                "version": update.version,
                                "date":    update.date,
                                "body":    update.body,
                            });
                            let _ = app.emit("update-available", payload);
                        }
                        Ok(Ok(None)) => {
                            eprintln!("[updater] up to date");
                            let _ = app.emit("update-checked", ());
                        }
                        Ok(Err(e)) => {
                            let msg = e.to_string();
                            if msg.contains("None of the fallback platforms")
                                || msg.contains("were found in the response `platforms` object")
                            {
                                eprintln!(
                                    "[updater] no release artifacts for this platform; \
                                     disabling background update checks"
                                );
                                updater_platform_unsupported = true;
                            } else {
                                eprintln!("[updater] check failed: {e}");
                            }
                        }
                    }
                }
            }

            let interval_secs = {
                let r = app.state::<Mutex<Box<AppState>>>();
                let g = r.lock_or_recover();
                g.update_check_interval_secs
            };
            let sleep_secs = if interval_secs == 0 {
                60
            } else {
                interval_secs
            };
            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_delay_connected() {
        assert_eq!(poll_delay_secs("connected"), 5);
    }

    #[test]
    fn poll_delay_disconnected() {
        assert_eq!(poll_delay_secs("disconnected"), 2);
    }

    #[test]
    fn poll_delay_scanning() {
        assert_eq!(poll_delay_secs("scanning"), 2);
    }
}
