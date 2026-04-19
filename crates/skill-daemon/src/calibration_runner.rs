// SPDX-License-Identifier: GPL-3.0-only
//! Daemon-authoritative calibration session runner.
//!
//! The timing loop runs entirely in the daemon.  The UI subscribes to
//! WebSocket events to display countdown / phase changes and to trigger
//! TTS announcements.
//!
//! ## TTS flow
//!
//! Before each action/break countdown the daemon broadcasts a
//! `calibration-tts` event with the text to speak.  It then waits
//! `TTS_ANNOUNCE_SECS` seconds so the frontend can play the audio
//! before the timer starts ticking.  This ensures the user always
//! hears the full cue ("Eyes Open", "Break — next: Eyes Closed")
//! before the countdown begins.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::oneshot;
use tracing::info;

use crate::routes::labels;
use crate::routes::settings_io::{load_user_settings, save_user_settings};
use crate::state::AppState;
use skill_daemon_state::CalibrationPhaseSnapshot;
use skill_settings::CalibrationProfile;

/// Seconds reserved for TTS playback before each phase countdown starts.
const TTS_ANNOUNCE_SECS: u64 = 4;

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Spawn a calibration session for the given profile.
///
/// Returns `Err` if a session is already running or the profile is not found.
pub fn spawn_session(state: &AppState, profile_id: &str) -> Result<(), String> {
    let mut cancel_guard = state.calibration_cancel.lock().unwrap();
    if cancel_guard.is_some() {
        return Err("A calibration session is already running".into());
    }

    let settings = load_user_settings(state);
    let profile = settings
        .calibration_profiles
        .iter()
        .find(|p| p.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("Profile '{profile_id}' not found"))?;

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    *cancel_guard = Some(cancel_tx);
    drop(cancel_guard);

    let st = state.clone();
    tokio::spawn(async move {
        run_session(st, profile, cancel_rx).await;
    });

    Ok(())
}

/// Cancel the running calibration session, if any.
pub fn cancel_session(state: &AppState) -> bool {
    let tx = state.calibration_cancel.lock().unwrap().take();
    if let Some(tx) = tx {
        let _ = tx.send(());
        set_phase(state, CalibrationPhaseSnapshot::default());
        state.broadcast("calibration-tts", serde_json::json!({"text": "Calibration cancelled."}));
        state.broadcast("calibration-cancelled", serde_json::json!({"source": "daemon"}));
        true
    } else {
        false
    }
}

fn set_phase(state: &AppState, snap: CalibrationPhaseSnapshot) {
    *state.calibration_phase.lock().unwrap() = snap;
}

fn phase_snapshot(
    kind: &str,
    profile: &CalibrationProfile,
    action_index: usize,
    loop_number: u32,
    countdown: u32,
    total_secs: u32,
) -> CalibrationPhaseSnapshot {
    CalibrationPhaseSnapshot {
        kind: kind.to_string(),
        action_index,
        loop_number,
        countdown,
        total_secs,
        running: true,
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
    }
}

/// Broadcast a TTS cue and wait `TTS_ANNOUNCE_SECS` for the frontend to
/// play it.  Returns `false` if cancelled during the wait.
async fn announce_tts(state: &AppState, cancel_rx: &mut oneshot::Receiver<()>, text: &str) -> bool {
    state.broadcast("calibration-tts", serde_json::json!({"text": text}));
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(TTS_ANNOUNCE_SECS)) => true,
        _ = &mut *cancel_rx => false,
    }
}

/// Run a wall-clock-aligned countdown, broadcasting tick events every second.
///
/// Returns `false` if cancelled, `true` if completed normally.
async fn run_countdown(
    state: &AppState,
    cancel_rx: &mut oneshot::Receiver<()>,
    profile: &CalibrationProfile,
    kind: &str,
    action_index: usize,
    loop_number: u32,
    secs: u32,
) -> bool {
    let end = SystemTime::now() + Duration::from_secs(secs as u64);

    let snap = phase_snapshot(kind, profile, action_index, loop_number, secs, secs);
    set_phase(state, snap.clone());
    state.broadcast("calibration-phase", &snap);

    for remaining in (0..secs).rev() {
        let now = SystemTime::now();
        let until = end - Duration::from_secs(remaining as u64);
        if let Ok(d) = until.duration_since(now) {
            tokio::select! {
                _ = tokio::time::sleep(d) => {}
                _ = &mut *cancel_rx => { return false; }
            }
        }

        let snap = phase_snapshot(kind, profile, action_index, loop_number, remaining, secs);
        set_phase(state, snap.clone());
        state.broadcast("calibration-phase", &snap);
    }

    true
}

async fn run_session(state: AppState, profile: CalibrationProfile, mut cancel_rx: oneshot::Receiver<()>) {
    info!(
        "[calibration] session start: profile={} actions={} loops={}",
        profile.name,
        profile.actions.len(),
        profile.loop_count
    );

    // ── Opening TTS announcement ─────────────────────────────────────────
    // Give the user a spoken overview before the first action timer starts.
    let intro = format!(
        "Calibration starting. {} actions, {} loops.",
        profile.actions.len(),
        profile.loop_count
    );
    if !announce_tts(&state, &mut cancel_rx, &intro).await {
        cleanup_cancelled(&state);
        return;
    }

    state.broadcast(
        "calibration-started",
        serde_json::json!({
            "profile_id": profile.id,
            "profile_name": profile.name,
            "actions": profile.actions.iter().map(|a| &a.label).collect::<Vec<_>>(),
            "loop_count": profile.loop_count,
        }),
    );

    let mut completed = true;

    'outer: for loop_num in 1..=profile.loop_count {
        for (ai, action) in profile.actions.iter().enumerate() {
            // ── TTS: announce action label before countdown ───────────
            if !announce_tts(&state, &mut cancel_rx, &action.label).await {
                completed = false;
                break 'outer;
            }

            // ── Action countdown ─────────────────────────────────────
            state.broadcast(
                "calibration-action",
                serde_json::json!({
                    "action": action.label,
                    "action_index": ai,
                    "loop": loop_num,
                    "duration_secs": action.duration_secs,
                }),
            );

            let action_start = unix_secs();

            if !run_countdown(
                &state,
                &mut cancel_rx,
                &profile,
                "action",
                ai,
                loop_num,
                action.duration_secs,
            )
            .await
            {
                completed = false;
                break 'outer;
            }

            let action_end = unix_secs();

            // Submit label
            if let Err(e) = labels::submit_label_internal(&state, &action.label, action_start, action_end).await {
                info!("[calibration] label submission failed: {e}");
                state.broadcast(
                    "calibration-error",
                    serde_json::json!({"error": "label_failed", "detail": e}),
                );
                completed = false;
                break 'outer;
            }

            // ── Break phase ──────────────────────────────────────────
            let is_last = loop_num == profile.loop_count && ai == profile.actions.len() - 1;
            if !is_last {
                let next_label = &profile.actions[(ai + 1) % profile.actions.len()].label;

                // TTS: announce break and preview next action
                let break_text = format!("Break. Next: {}.", next_label);
                if !announce_tts(&state, &mut cancel_rx, &break_text).await {
                    completed = false;
                    break 'outer;
                }

                state.broadcast(
                    "calibration-break",
                    serde_json::json!({
                        "after_action": action.label,
                        "next_action": next_label,
                        "loop": loop_num,
                        "duration_secs": profile.break_duration_secs,
                    }),
                );

                if !run_countdown(
                    &state,
                    &mut cancel_rx,
                    &profile,
                    "break",
                    ai,
                    loop_num,
                    profile.break_duration_secs,
                )
                .await
                {
                    completed = false;
                    break 'outer;
                }
            }
        }
    }

    // Clear the cancel handle
    let _ = state.calibration_cancel.lock().unwrap().take();

    if completed {
        info!("[calibration] session complete: {} loops", profile.loop_count);

        // Update last_calibration_utc in settings
        let mut settings = load_user_settings(&state);
        if let Some(p) = settings.calibration_profiles.iter_mut().find(|p| p.id == profile.id) {
            p.last_calibration_utc = Some(unix_secs());
        }
        save_user_settings(&state, &settings);

        let snap = CalibrationPhaseSnapshot {
            kind: "done".into(),
            loop_number: profile.loop_count,
            running: false,
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            ..Default::default()
        };
        set_phase(&state, snap);

        // TTS: completion announcement
        let done_text = format!("Calibration complete. {} loops recorded.", profile.loop_count);
        state.broadcast("calibration-tts", serde_json::json!({"text": done_text}));
        state.broadcast(
            "calibration-completed",
            serde_json::json!({
                "profile_id": profile.id,
                "loop_count": profile.loop_count,
            }),
        );
    } else {
        cleanup_cancelled(&state);
    }
}

fn cleanup_cancelled(state: &AppState) {
    let _ = state.calibration_cancel.lock().unwrap().take();
    set_phase(state, CalibrationPhaseSnapshot::default());
    state.broadcast("calibration-tts", serde_json::json!({"text": "Calibration cancelled."}));
    state.broadcast("calibration-cancelled", serde_json::json!({"source": "daemon"}));
}
