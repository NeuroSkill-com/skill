// SPDX-License-Identifier: GPL-3.0-only
//! Daemon background tasks: skills sync, DND polling, auto-scanner.
//!
//! Previously lived in Tauri's `background.rs`.  Now daemon-authoritative.

use std::time::Duration;

use chrono::{Datelike, Timelike};
use tracing::info;

use crate::routes::settings_io::load_user_settings;
use crate::state::AppState;

/// Spawn all daemon background tasks.
pub fn spawn_all(state: AppState) {
    spawn_skills_sync(state.clone());
    spawn_dnd_poll(state.clone());
    spawn_auto_scanner(state.clone());
    spawn_auto_connect(state.clone());
    spawn_calibration_auto_start(state.clone());
    spawn_screenshot_worker(state.clone());
    spawn_weekly_digest(state.clone());
    spawn_fatigue_monitor(state.clone());
    spawn_daily_brain_report(state);
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

        let preferred_id = match preferred {
            Some(id) => id,
            None => {
                // No paired/preferred device — wait for the scanner to discover one
                // and auto-pair it.  Prefer BLE EEG devices over other transports.
                info!("[auto-connect] no paired device found, waiting for discovery…");
                const MAX_ATTEMPTS: u32 = 30; // ~60 seconds
                let mut attempts = 0u32;
                let found = loop {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    attempts += 1;
                    let candidate = state.devices.lock().ok().and_then(|devs| {
                        // Prefer a known EEG BLE device; fall back to any eligible device.
                        let eligible = devs.iter().filter(|d| crate::scanner::is_auto_pair_eligible(d));
                        eligible
                            .clone()
                            .find(|d| crate::scanner::is_known_eeg_ble_name(&d.name))
                            .or_else(|| eligible.clone().next())
                            .cloned()
                    });
                    if let Some(dev) = candidate {
                        break dev;
                    }
                    if attempts >= MAX_ATTEMPTS {
                        info!("[auto-connect] no device discovered after {MAX_ATTEMPTS} attempts, giving up");
                        return;
                    }
                };

                info!(
                    "[auto-connect] auto-pairing first discovered device: {} ({})",
                    found.name, found.id
                );

                // Pair the device.
                if let Ok(mut guard) = state.devices.lock() {
                    if let Some(d) = guard.iter_mut().find(|d| d.id == found.id) {
                        d.is_paired = true;
                        d.is_preferred = true;
                    }
                }
                if let Ok(mut status) = state.status.lock() {
                    if !status.paired_devices.iter().any(|d| d.id == found.id) {
                        status.paired_devices.push(skill_daemon_common::PairedDeviceResponse {
                            id: found.id.clone(),
                            name: found.name.clone(),
                            last_seen: skill_daemon_state::util::now_unix_secs(),
                        });
                    }
                }
                skill_daemon_state::util::persist_paired_devices(&state);

                // Set as preferred.
                let mut settings = load_user_settings(&state);
                settings.preferred_id = Some(found.id.clone());
                crate::routes::settings_io::save_user_settings(&state, &settings);

                info!("[auto-connect] device {} set as preferred", found.id);

                // Notify the UI so the device list updates immediately.
                state.broadcast("devices-updated", serde_json::json!({ "auto_paired": found.id }));

                found.id
            }
        };

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

fn spawn_screenshot_worker(state: AppState) {
    use std::sync::Arc;
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let settings = skill_settings::load_settings(&skill_dir);
    let ctx = Arc::new(crate::routes::settings_screenshots::DaemonScreenshotContext {
        config: settings.screenshot,
        state: Some(state.clone()),
        events_tx: state.events_tx.clone(),
        text_embedder: state.text_embedder.clone(),
    });
    let metrics = Arc::new(skill_screenshots::capture::ScreenshotMetrics::default());
    let dir = skill_dir.clone();
    std::thread::Builder::new()
        .name("screenshot-capture".into())
        .spawn(move || {
            skill_screenshots::capture::run_screenshot_worker(ctx, dir, None, metrics);
        })
        .unwrap_or_else(|e| {
            eprintln!("[screenshot] failed to spawn worker: {e}");
            panic!("screenshot worker spawn failed");
        });
    info!("[screenshot] worker spawned");
}

/// Weekly digest notification — fires once per week (Monday 9am local).
/// Sends a summary of the past week's activity as an OS notification.
fn spawn_weekly_digest(state: AppState) {
    tokio::spawn(async move {
        // Wait 30 seconds after startup to avoid competing with other init work.
        tokio::time::sleep(Duration::from_secs(30)).await;
        let mut last_digest_week: i32 = -1; // ISO week number of last sent digest
        loop {
            // Check once per hour if it's time to send the digest.
            tokio::time::sleep(Duration::from_secs(3600)).await;

            let now = chrono::Local::now();
            // Fire on Monday between 9:00-9:59 AM.
            if now.weekday() != chrono::Weekday::Mon || now.hour() != 9 {
                continue;
            }
            // Dedup: only fire once per ISO week (prevents re-fire on daemon restart).
            let iso_week = now.iso_week().week() as i32;
            if iso_week == last_digest_week {
                continue;
            }
            last_digest_week = iso_week;

            let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
            let week_start = {
                let today = now.date_naive();
                let monday = today - chrono::Duration::days(7);
                monday
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_local_timezone(chrono::Local)
                    .earliest()
                    .map(|t| t.timestamp() as u64)
                    .unwrap_or(0)
            };

            let digest = tokio::task::spawn_blocking(move || {
                skill_data::activity_store::ActivityStore::open(&skill_dir).map(|s| s.weekly_digest(week_start))
            })
            .await
            .ok()
            .flatten();

            if let Some(d) = digest {
                if d.total_edits == 0 {
                    continue; // No activity — skip.
                }
                let hours = d.total_secs / 3600;
                let body = format!(
                    "{}h coding, {} edits, {} files, {} meetings. Peak: {}:00.",
                    hours, d.total_edits, d.total_interactions, d.meeting_count, d.peak_hour
                );
                state.broadcast(
                    "weekly-digest",
                    serde_json::json!({
                        "title": "Weekly Activity Digest",
                        "body": body,
                        "week_start": week_start,
                    }),
                );
                // Also send as OS notification if the app supports it.
                if let Err(e) = skill_data::dnd::send_notification("Weekly Activity Digest", &body) {
                    eprintln!("[weekly-digest] notification failed: {e}");
                }
            }

            // Dedup handled by ISO week check above — no extra sleep needed.
        }
    });
}

/// Insert an auto-generated EEG label for significant activity state transitions.
/// Labels are stored in `labels.sqlite` alongside manual labels, enabling
/// EEG search to find brain states correlated with coding events.
fn auto_label(skill_dir: &std::path::Path, text: &str, context: &str) {
    let db_path = skill_dir.join(skill_constants::LABELS_FILE);
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let _ = conn.execute(
        "INSERT INTO labels (text, context, eeg_start, eeg_end, wall_start, wall_end, created_at)
         VALUES (?1, ?2, ?3, ?3, ?3, ?3, ?3)",
        rusqlite::params![text, context, now as i64],
    );
}

/// Fatigue monitor — checks every 15 minutes for declining focus trend.
/// Broadcasts `fatigue-alert` event and sends OS notification when fatigued.
fn spawn_fatigue_monitor(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        loop {
            tokio::time::sleep(Duration::from_secs(900)).await; // 15 minutes

            let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
            let alert = tokio::task::spawn_blocking(move || {
                skill_data::activity_store::ActivityStore::open(&skill_dir).map(|s| s.fatigue_check())
            })
            .await
            .ok()
            .flatten();

            if let Some(a) = alert {
                if a.fatigued {
                    state.broadcast(
                        "fatigue-alert",
                        serde_json::json!({
                            "fatigued": true,
                            "decline_pct": a.focus_decline_pct,
                            "continuous_work_mins": a.continuous_work_mins,
                            "suggestion": a.suggestion,
                        }),
                    );
                    let _ = skill_data::dnd::send_notification("Focus declining", &a.suggestion);
                    // Auto-label for EEG correlation.
                    let sd = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
                    auto_label(
                        &sd,
                        "fatigue detected",
                        &format!(
                            "decline {}%, work {}m",
                            a.focus_decline_pct as i32, a.continuous_work_mins
                        ),
                    );
                }
            }
        }
    });
}

/// Daily brain report — fires at 6pm local, sends summary as notification.
fn spawn_daily_brain_report(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let mut last_report_day: i32 = -1;
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;

            let now = chrono::Local::now();
            if now.hour() != 18 {
                continue;
            }
            let day_of_year = now.ordinal() as i32;
            if day_of_year == last_report_day {
                continue;
            }
            last_report_day = day_of_year;

            let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
            let today_start = {
                let d = now.date_naive();
                d.and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_local_timezone(chrono::Local)
                    .earliest()
                    .map(|t| t.timestamp() as u64)
                    .unwrap_or(0)
            };

            let report = tokio::task::spawn_blocking(move || {
                skill_data::activity_store::ActivityStore::open(&skill_dir).map(|s| s.daily_brain_report(today_start))
            })
            .await
            .ok()
            .flatten();

            if let Some(r) = report {
                let body = format!(
                    "Best: {}. Score: {:.0}. Focus: {:.0}.",
                    r.best_period,
                    r.productivity_score,
                    r.overall_focus.unwrap_or(0.0)
                );
                state.broadcast(
                    "daily-brain-report",
                    serde_json::json!({
                        "day_start": r.day_start,
                        "best_period": r.best_period,
                        "productivity_score": r.productivity_score,
                        "overall_focus": r.overall_focus,
                    }),
                );
                let _ = skill_data::dnd::send_notification("Daily Brain Report", &body);
            }
        }
    });
}
