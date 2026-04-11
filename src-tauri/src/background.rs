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

use crate::helpers::{
    apply_daemon_status, emit_status, emit_status_from_daemon, send_toast, ToastLevel,
};
use crate::state::AppState;
use crate::ws_server::WsBroadcaster;
use crate::MutexExt;

/// Continuous reconnect cadence (seconds) used by the startup auto-reconnect loop.
/// Single source of truth for countdown + retry trigger interval.
const AUTO_RECONNECT_CADENCE_SECS: u32 = 3;

// ── Pure decision logic (extracted for testability) ──────────────────────────

/// Whether a low-battery toast should fire.
///
/// Returns `true` when battery is in (0, 15] and we haven't already warned
/// for this device.
fn should_warn_battery(batt: f32, dev_id: &str, warned_for: Option<&str>) -> bool {
    !dev_id.is_empty() && batt > 0.0 && batt <= 15.0 && warned_for != Some(dev_id)
}

/// Whether the battery-warning latch should be cleared (device recharged).
fn should_clear_battery_warning(batt: f32, dev_id: &str, warned_for: Option<&str>) -> bool {
    batt >= 25.0 && warned_for == Some(dev_id)
}

/// Count good / bad channels from a quality slice.
fn count_signal_quality(quality: &[String]) -> (usize, usize) {
    let good = quality.iter().filter(|x| x.as_str() == "good").count();
    let bad = quality
        .iter()
        .filter(|x| x.as_str() == "poor" || x.as_str() == "no_signal")
        .count();
    (good, bad)
}

/// Outcome of one reconnect-tick evaluation.
#[derive(Debug, PartialEq)]
struct ReconnectAction {
    /// New countdown value to write back.
    countdown: u32,
    /// New attempt counter to write back.
    attempt: u32,
    /// Whether to emit a status update to the frontend.
    should_emit: bool,
    /// Whether to actually fire a retry RPC to the daemon.
    trigger_retry: bool,
}

/// Pure state-machine step for the auto-reconnect countdown.
///
/// `state` is the current connection state string ("connected", "disconnected",
/// "bt_off", "connecting", "scanning", …).
fn eval_reconnect_tick(
    pending_reconnect: bool,
    state: &str,
    countdown: u32,
    attempt: u32,
) -> ReconnectAction {
    if !pending_reconnect {
        // Reconnect disabled — clear counters if non-zero.
        let dirty = countdown != 0 || attempt != 0;
        return ReconnectAction {
            countdown: 0,
            attempt: 0,
            should_emit: dirty,
            trigger_retry: false,
        };
    }

    if state == "connected" {
        let dirty = countdown != 0 || attempt != 0;
        return ReconnectAction {
            countdown: 0,
            attempt: 0,
            should_emit: dirty,
            trigger_retry: false,
        };
    }

    if state == "bt_off" || state == "connecting" || state == "scanning" {
        // Active flow in progress — don't interfere.
        return ReconnectAction {
            countdown,
            attempt,
            should_emit: false,
            trigger_retry: false,
        };
    }

    // Disconnected + reconnect enabled: run the countdown.
    let new_countdown = if countdown == 0 {
        AUTO_RECONNECT_CADENCE_SECS
    } else {
        countdown.saturating_sub(1)
    };

    let trigger = new_countdown == 0;
    let new_attempt = if trigger {
        attempt.saturating_add(1)
    } else {
        attempt
    };

    ReconnectAction {
        countdown: new_countdown,
        attempt: new_attempt,
        should_emit: true,
        trigger_retry: trigger,
    }
}

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
    spawn_scanner_start(app.handle());
    spawn_auto_connect(app.handle());
    spawn_daemon_status_poll(app.handle());
    spawn_auto_reconnect(app.handle());
    spawn_daemon_log_tail();
    spawn_calibration_auto_start(app.handle());
    spawn_onboarding_check(app.handle());
    spawn_updater_poll(app.handle());
    spawn_skills_sync(app.handle());
    spawn_dnd_poll(app.handle());
}

// ── Individual tasks ─────────────────────────────────────────────────────────

fn spawn_scanner_start(handle: &AppHandle) {
    let app = handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let (wifi_shield_ip, galea_ip) = {
            let r = app.state::<Mutex<Box<AppState>>>();
            let s = r.lock_or_recover();
            (
                s.openbci_config.wifi_shield_ip.clone(),
                s.openbci_config.galea_ip.clone(),
            )
        };
        let _ = crate::daemon_cmds::scanner_set_wifi_config(wifi_shield_ip, galea_ip);
        let _ = crate::daemon_cmds::scanner_start();
        // Start LSL auto-scanner if enabled in settings
        crate::settings_cmds::lsl_cmds::maybe_start_lsl_auto_scanner(&app);
    });
}

fn spawn_auto_connect(handle: &AppHandle) {
    let app = handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(900)).await;
        let preferred = {
            let r = app.state::<Mutex<Box<AppState>>>();
            let mut s = r.lock_or_recover();
            let pref = s
                .preferred_id
                .clone()
                .or_else(|| s.status.paired_devices.first().map(|d| d.id.clone()));
            if pref.is_some() {
                s.pending_reconnect = true;
            }
            pref
        };
        // Only auto-connect if there's a paired device.  On first launch
        // (no paired devices) the user must discover and pair manually —
        // except that the first successful connection auto-pairs as a
        // convenience (handled in on_connected).
        if let Some(preferred) = preferred {
            let _ = crate::settings_cmds::device_cmds::set_preferred_device(preferred, app.clone());
            crate::settings_cmds::device_cmds::retry_connect(app.clone());
        }
    });
}

fn spawn_daemon_status_poll(handle: &AppHandle) {
    let app = handle.clone();
    tauri::async_runtime::spawn(async move {
        // Wait for daemon to be ready before polling.
        tokio::time::sleep(Duration::from_secs(2)).await;
        let mut batt_warned_for: Option<String> = None;
        let mut had_good_signal = false;
        let mut bad_signal_since: Option<std::time::Instant> = None;
        let mut signal_warned_for: Option<String> = None;
        loop {
            let poll_result = tokio::task::spawn_blocking(crate::daemon_cmds::fetch_daemon_status)
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            match poll_result {
                Ok(daemon_status) => {
                    // Backend-level safety alerts so notifications still fire
                    // even when the dashboard window is hidden/closed.
                    if daemon_status.state == "connected" {
                        let dev_id = daemon_status
                            .device_id
                            .clone()
                            .or(daemon_status.target_id.clone())
                            .or(daemon_status.device_name.clone())
                            .unwrap_or_default();

                        // Low battery warning (once per device until recharge).
                        let batt = daemon_status.battery;
                        if should_warn_battery(batt, &dev_id, batt_warned_for.as_deref()) {
                            batt_warned_for = Some(dev_id.clone());
                            send_toast(
                                &app,
                                ToastLevel::Warning,
                                "Low battery",
                                &format!(
                                    "{} battery is at {:.0}%.",
                                    daemon_status
                                        .device_name
                                        .clone()
                                        .unwrap_or_else(|| "Device".into()),
                                    batt
                                ),
                            );
                        }
                        if should_clear_battery_warning(batt, &dev_id, batt_warned_for.as_deref()) {
                            batt_warned_for = None;
                        }

                        // Signal degradation warning after a previously good state.
                        let (good, bad) = count_signal_quality(&daemon_status.channel_quality);
                        if good >= 2 {
                            had_good_signal = true;
                        }
                        if had_good_signal && bad >= 2 && daemon_status.sample_count > 0 {
                            if bad_signal_since.is_none() {
                                bad_signal_since = Some(std::time::Instant::now());
                            }
                            if let Some(since) = bad_signal_since {
                                if since.elapsed() >= Duration::from_secs(20)
                                    && signal_warned_for.as_deref() != Some(&dev_id)
                                {
                                    signal_warned_for = Some(dev_id.clone());
                                    send_toast(
                                        &app,
                                        ToastLevel::Warning,
                                        "Signal quality dropped",
                                        "EEG signal became poor during recording. Re-seat electrodes / adjust fit.",
                                    );
                                }
                            }
                        } else {
                            bad_signal_since = None;
                            if good >= 2 && signal_warned_for.as_deref() == Some(&dev_id) {
                                signal_warned_for = None;
                            }
                        }
                    } else {
                        had_good_signal = false;
                        bad_signal_since = None;
                    }

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
                Err(_) => { /* daemon unreachable — skip this tick */ }
            }
            // Adaptive poll: 5 s when connected, 2 s otherwise.
            let delay = {
                let r = app.state::<Mutex<Box<AppState>>>();
                let s = r.lock_or_recover();
                poll_delay_secs(&s.status.state)
            };
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
    });
}

fn spawn_auto_reconnect(handle: &AppHandle) {
    let app = handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let action = {
                let r = app.state::<Mutex<Box<AppState>>>();
                let s = r.lock_or_recover();
                eval_reconnect_tick(
                    s.pending_reconnect,
                    &s.status.state,
                    s.status.retry_countdown_secs,
                    s.status.retry_attempt,
                )
            };

            {
                let r = app.state::<Mutex<Box<AppState>>>();
                let mut s = r.lock_or_recover();
                s.status.retry_countdown_secs = action.countdown;
                s.status.retry_attempt = action.attempt;
            }

            let should_emit = action.should_emit;
            let trigger_retry = action.trigger_retry;

            if should_emit {
                emit_status(&app);
            }

            if trigger_retry {
                let _ = tokio::task::spawn_blocking(crate::daemon_cmds::retry_connect)
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .map(|daemon_status| {
                        let r = app.state::<Mutex<Box<AppState>>>();
                        let mut s = r.lock_or_recover();
                        apply_daemon_status(&mut s.status, daemon_status);
                    });
                emit_status_from_daemon(&app);
            }
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

fn spawn_calibration_auto_start(handle: &AppHandle) {
    let app = handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let auto_start_id: Option<String> = {
            let r = app.state::<Mutex<Box<AppState>>>();
            let s = r.lock_or_recover();
            let active_id = &s.active_calibration_id;
            s.calibration_profiles
                .iter()
                .find(|p| &p.id == active_id)
                .filter(|p| p.auto_start)
                .map(|p| p.id.clone())
        };
        if let Some(id) = auto_start_id {
            let _ = crate::window_cmds::open_calibration_window_inner(&app, Some(id), false).await;
        }
    });
}

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

fn spawn_skills_sync(handle: &AppHandle) {
    let app = handle.clone();
    tauri::async_runtime::spawn(async move {
        // Wait a bit after startup before first sync attempt.
        tokio::time::sleep(Duration::from_secs(45)).await;
        let mut first_run = true;
        loop {
            let (skill_dir, interval_secs, sync_on_launch) = {
                let r = app.state::<Mutex<Box<AppState>>>();
                let (sd, llm_arc) = {
                    let g = r.lock_or_recover();
                    (g.skill_dir.clone(), g.llm.clone())
                };
                let tools = &llm_arc.lock_or_recover().config.tools;
                let iv = tools.skills_refresh_interval_secs;
                let sol = tools.skills_sync_on_launch;
                (sd, iv, sol)
            };

            // On first run, force sync if sync_on_launch is enabled;
            // otherwise respect the normal interval.
            let force_launch = first_run && sync_on_launch;
            let effective_interval = if force_launch { 0 } else { interval_secs };
            first_run = false;

            if force_launch || interval_secs > 0 {
                eprintln!("[skills-sync] checking for community skills update");
                let sd = skill_dir.clone();
                let iv = effective_interval;
                let outcome = tokio::task::spawn_blocking(move || {
                    skill_skills::sync::sync_skills(&sd, iv, None)
                })
                .await;

                match outcome {
                    Ok(skill_skills::sync::SyncOutcome::Updated { elapsed_ms, .. }) => {
                        eprintln!("[skills-sync] updated in {elapsed_ms} ms");
                        let _ = app.emit("skills-updated", ());
                    }
                    Ok(skill_skills::sync::SyncOutcome::Fresh { next_sync_in_secs }) => {
                        eprintln!("[skills-sync] fresh, next check in {next_sync_in_secs} s");
                    }
                    Ok(skill_skills::sync::SyncOutcome::Failed(e)) => {
                        eprintln!("[skills-sync] failed: {e}");
                    }
                    Err(e) => {
                        eprintln!("[skills-sync] task panic: {e}");
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

fn spawn_dnd_poll(handle: &AppHandle) {
    let app = handle.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        loop {
            let os_now = skill_data::dnd::query_os_active();

            // DND state is behind its own lock — no AppState lock needed.
            let dnd_arc = app
                .state::<Mutex<Box<AppState>>>()
                .lock_or_recover()
                .dnd_arc();
            let (prev, app_active) = {
                let d = dnd_arc.lock_or_recover();
                (d.os_active, d.active)
            };

            if os_now != prev {
                dnd_arc.lock_or_recover().os_active = os_now;

                let payload = serde_json::json!({ "os_active": os_now });
                let _ = app.emit("dnd-os-changed", &payload);
                app.state::<WsBroadcaster>()
                    .send("dnd-os-changed", &payload);

                if os_now == Some(false) && app_active {
                    eprintln!(
                        "[dnd] OS DND was externally cleared while \
                         app believed it was active — reconciling"
                    );
                    {
                        let mut d = dnd_arc.lock_or_recover();
                        d.active = false;
                        d.below_ticks = 0;
                        d.focus_samples.clear();
                    }
                    let _ = app.emit("dnd-state-changed", false);
                    app.state::<WsBroadcaster>()
                        .send("dnd-state-changed", &false);
                }
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Constants ────────────────────────────────────────────────────────

    #[test]
    fn auto_reconnect_cadence_is_nonzero() {
        assert!(AUTO_RECONNECT_CADENCE_SECS > 0);
    }

    #[test]
    fn auto_reconnect_cadence_reasonable_range() {
        assert!((1..=30).contains(&AUTO_RECONNECT_CADENCE_SECS));
    }

    // ── Battery warnings ─────────────────────────────────────────────────

    #[test]
    fn battery_warn_at_15_percent() {
        assert!(should_warn_battery(15.0, "dev1", None));
    }

    #[test]
    fn battery_warn_at_1_percent() {
        assert!(should_warn_battery(1.0, "dev1", None));
    }

    #[test]
    fn battery_no_warn_at_16_percent() {
        assert!(!should_warn_battery(16.0, "dev1", None));
    }

    #[test]
    fn battery_no_warn_at_zero() {
        // battery == 0 means "unknown" — don't warn.
        assert!(!should_warn_battery(0.0, "dev1", None));
    }

    #[test]
    fn battery_no_warn_negative() {
        assert!(!should_warn_battery(-1.0, "dev1", None));
    }

    #[test]
    fn battery_no_warn_empty_device_id() {
        assert!(!should_warn_battery(10.0, "", None));
    }

    #[test]
    fn battery_no_warn_if_already_warned_same_device() {
        assert!(!should_warn_battery(10.0, "dev1", Some("dev1")));
    }

    #[test]
    fn battery_warn_if_warned_different_device() {
        assert!(should_warn_battery(10.0, "dev2", Some("dev1")));
    }

    #[test]
    fn battery_clear_at_25_percent() {
        assert!(should_clear_battery_warning(25.0, "dev1", Some("dev1")));
    }

    #[test]
    fn battery_no_clear_at_24_percent() {
        assert!(!should_clear_battery_warning(24.0, "dev1", Some("dev1")));
    }

    #[test]
    fn battery_no_clear_different_device() {
        assert!(!should_clear_battery_warning(30.0, "dev2", Some("dev1")));
    }

    #[test]
    fn battery_no_clear_when_not_warned() {
        assert!(!should_clear_battery_warning(30.0, "dev1", None));
    }

    // ── Signal quality counting ──────────────────────────────────────────

    #[test]
    fn signal_quality_counts_good_and_bad() {
        let q: Vec<String> = vec![
            "good".into(),
            "good".into(),
            "poor".into(),
            "no_signal".into(),
            "fair".into(),
        ];
        assert_eq!(count_signal_quality(&q), (2, 2));
    }

    #[test]
    fn signal_quality_empty() {
        assert_eq!(count_signal_quality(&[]), (0, 0));
    }

    #[test]
    fn signal_quality_all_good() {
        let q: Vec<String> = vec!["good".into(); 4];
        assert_eq!(count_signal_quality(&q), (4, 0));
    }

    #[test]
    fn signal_quality_all_poor() {
        let q: Vec<String> = vec!["poor".into(); 3];
        assert_eq!(count_signal_quality(&q), (0, 3));
    }

    // ── Poll delay ───────────────────────────────────────────────────────

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

    // ── Reconnect state machine ──────────────────────────────────────────

    #[test]
    fn reconnect_disabled_clears_counters() {
        let a = eval_reconnect_tick(false, "disconnected", 2, 5);
        assert_eq!(
            a,
            ReconnectAction {
                countdown: 0,
                attempt: 0,
                should_emit: true,
                trigger_retry: false,
            }
        );
    }

    #[test]
    fn reconnect_disabled_no_emit_when_already_zero() {
        let a = eval_reconnect_tick(false, "disconnected", 0, 0);
        assert!(!a.should_emit);
        assert!(!a.trigger_retry);
    }

    #[test]
    fn reconnect_connected_clears_counters() {
        let a = eval_reconnect_tick(true, "connected", 2, 3);
        assert_eq!(a.countdown, 0);
        assert_eq!(a.attempt, 0);
        assert!(a.should_emit);
        assert!(!a.trigger_retry);
    }

    #[test]
    fn reconnect_connected_no_emit_when_clean() {
        let a = eval_reconnect_tick(true, "connected", 0, 0);
        assert!(!a.should_emit);
    }

    #[test]
    fn reconnect_bt_off_passthrough() {
        let a = eval_reconnect_tick(true, "bt_off", 2, 1);
        assert_eq!(a.countdown, 2);
        assert_eq!(a.attempt, 1);
        assert!(!a.should_emit);
        assert!(!a.trigger_retry);
    }

    #[test]
    fn reconnect_connecting_passthrough() {
        let a = eval_reconnect_tick(true, "connecting", 0, 0);
        assert!(!a.should_emit);
        assert!(!a.trigger_retry);
    }

    #[test]
    fn reconnect_scanning_passthrough() {
        let a = eval_reconnect_tick(true, "scanning", 1, 2);
        assert!(!a.should_emit);
        assert!(!a.trigger_retry);
    }

    #[test]
    fn reconnect_disconnected_starts_countdown() {
        let a = eval_reconnect_tick(true, "disconnected", 0, 0);
        assert_eq!(a.countdown, AUTO_RECONNECT_CADENCE_SECS);
        assert_eq!(a.attempt, 0);
        assert!(a.should_emit);
        assert!(!a.trigger_retry);
    }

    #[test]
    fn reconnect_countdown_decrements() {
        let a = eval_reconnect_tick(true, "disconnected", 3, 0);
        assert_eq!(a.countdown, 2);
        assert!(a.should_emit);
        assert!(!a.trigger_retry);
    }

    #[test]
    fn reconnect_fires_at_zero() {
        // countdown=1 → sub(1)=0 → trigger
        let a = eval_reconnect_tick(true, "disconnected", 1, 0);
        assert_eq!(a.countdown, 0);
        assert_eq!(a.attempt, 1);
        assert!(a.should_emit);
        assert!(a.trigger_retry);
    }

    #[test]
    fn reconnect_attempt_increments() {
        let a = eval_reconnect_tick(true, "disconnected", 1, 5);
        assert_eq!(a.attempt, 6);
        assert!(a.trigger_retry);
    }

    #[test]
    fn reconnect_full_cycle() {
        // Simulate a full cadence cycle: start → count down → fire.
        let a0 = eval_reconnect_tick(true, "disconnected", 0, 0);
        assert_eq!(a0.countdown, AUTO_RECONNECT_CADENCE_SECS);
        assert!(!a0.trigger_retry);

        let mut cd = a0.countdown;
        let mut att = a0.attempt;
        // Tick down until retry fires.
        loop {
            let a = eval_reconnect_tick(true, "disconnected", cd, att);
            cd = a.countdown;
            att = a.attempt;
            if a.trigger_retry {
                break;
            }
        }
        assert_eq!(att, 1);
        assert_eq!(cd, 0);
    }
}
