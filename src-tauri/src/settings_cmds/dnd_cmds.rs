// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Do Not Disturb automation Tauri commands.

use std::sync::Mutex;
use crate::MutexExt;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;
use crate::settings::DoNotDisturbConfig;

// ── Do Not Disturb automation ─────────────────────────────────────────────────

/// Return all Focus modes configured on this Mac as `{ identifier, name }` pairs.
///
/// On macOS 12+ this reads `ModeConfigurations.json`; falls back to the
/// well-known first-party list if the file is unavailable.  Returns an empty
/// array on non-macOS platforms.
#[tauri::command]
pub fn list_focus_modes() -> Vec<skill_data::dnd::FocusModeOption> {
    skill_data::dnd::list_focus_modes()
}

/// Force-disable the active Focus mode.
///
/// This is a one-direction safety escape hatch: it can only **deactivate**
/// Focus mode, never activate it.  Activation is exclusively controlled by the
/// EEG scoring pipeline in the session loop.
///
/// `enabled` is accepted as a parameter for API symmetry, but any call with
/// `enabled = true` is rejected immediately (returns `false`) so that no code
/// path other than live EEG data can turn on Focus mode.
///
/// Returns `true` if the OS call succeeded.
#[tauri::command]
pub fn test_dnd(
    enabled: bool,
    app:     AppHandle,
    state:   tauri::State<'_, Mutex<Box<AppState>>>,
) -> bool {
    // Guard: only allow disabling, never enabling.
    if enabled { return false; }

    let ok = skill_data::dnd::set_dnd(false, "");
    if ok {
        state.lock_or_recover().dnd_active = false;
        let _ = app.emit("dnd-state-changed", false);
        app.state::<crate::ws_server::WsBroadcaster>()
            .send("dnd-state-changed", &false);
    }
    ok
}

/// Return whether DND is currently active (i.e. the app has enabled it).
#[tauri::command]
pub fn get_dnd_active(state: tauri::State<'_, Mutex<Box<AppState>>>) -> bool {
    state.lock_or_recover().dnd_active
}

/// Return the current Do Not Disturb automation configuration.
#[tauri::command]
pub fn get_dnd_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> DoNotDisturbConfig {
    state.lock_or_recover().dnd_config.clone()
}

/// Persist new Do Not Disturb automation configuration.
///
/// If the feature is disabled and DND is currently active, DND is cleared
/// immediately so the user is not left in an unintended DND state.
#[tauri::command]
pub fn set_dnd_config(
    config: DoNotDisturbConfig,
    app:    AppHandle,
    state:  tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let was_active = {
        let mut s = state.lock_or_recover();
        let active = s.dnd_active;
        s.dnd_config           = config.clone();
        s.dnd_focus_samples.clear();   // reset activation window on any config change
        s.dnd_below_ticks      = 0;    // reset exit counter on any config change
        s.dnd_score_history.clear();   // reset lookback history on any config change
        s.dnd_snr_low_ticks    = 0;    // reset SNR low counter on any config change
        if !config.enabled && s.dnd_active {
            s.dnd_active = false;
        }
        active && !config.enabled
    };

    // If we just disabled the feature while DND was on, clear it:
    // (1) Exit system Focus first, (2) then notify the user if configured.
    if was_active {
        let ok = skill_data::dnd::set_dnd(false, "");
        let payload = false;
        let _ = app.emit("dnd-state-changed", payload);
        app.state::<crate::ws_server::WsBroadcaster>()
            .send("dnd-state-changed", &payload);
        if ok && config.exit_notification {
            crate::send_toast(
                &app,
                crate::ToastLevel::Info,
                "Focus mode exited",
                "Do Not Disturb automation was disabled. Focus mode deactivated.",
            );
        }
    }

    crate::save_settings(&app);
}

/// Live snapshot of the Do Not Disturb automation pipeline.
///
/// Returned by [`get_dnd_status`] and mirrored by the `dnd-eligibility`
/// broadcast event (emitted ~4 Hz).
#[derive(serde::Serialize, Clone, Debug)]
pub struct DndStatus {
    /// Whether the DND automation feature is enabled in settings.
    pub enabled: bool,
    /// Rolling-average focus score over the current sample window (0–100).
    /// The live per-tick value is only available via the `dnd-eligibility` event.
    pub avg_score: f64,
    /// Score (0–100) that the rolling average must reach to activate DND.
    pub threshold: f64,
    /// Number of samples currently in the rolling window.
    pub sample_count: usize,
    /// Target window size in samples (≈ duration_secs × 4 Hz).
    pub window_size: usize,
    /// Duration (seconds) that defines the rolling window length.
    pub duration_secs: u32,
    /// Whether the app has currently activated DND.
    pub dnd_active: bool,
    /// Whether the OS reports DND / Focus as active right now (`null` on non-macOS).
    pub os_active: Option<bool>,
    /// Seconds the score must remain below the threshold before DND clears.
    pub exit_duration_secs: u32,
    /// Consecutive ticks for which the score has been below threshold while
    /// DND is active.  Used to show the exit countdown in the UI.
    pub below_ticks: u32,
    /// Total ticks required for the exit window (≈ exit_duration_secs × 4 Hz).
    pub exit_window_size: usize,
    /// Approximate seconds remaining until DND exits (0 if not counting down).
    pub exit_secs_remaining: f64,
    /// Lookback window in seconds: if any tick in this window was above the
    /// threshold the exit counter resets (recent focus delays deactivation).
    pub focus_lookback_secs: u32,
    /// `true` when DND is active, score is below threshold, but the lookback
    /// window still contains a focus peak so exit is being delayed.
    pub exit_held_by_lookback: bool,
}

/// Return a snapshot of the DND automation pipeline state.
#[tauri::command]
pub fn get_dnd_status(state: tauri::State<'_, Mutex<Box<AppState>>>) -> DndStatus {
    let s                    = state.lock_or_recover();
    let enabled              = s.dnd_config.enabled;
    let threshold            = s.dnd_config.focus_threshold as f64;
    let duration_secs        = s.dnd_config.duration_secs;
    let exit_duration_secs   = s.dnd_config.exit_duration_secs;
    let focus_lookback_secs  = s.dnd_config.focus_lookback_secs;
    let window_size          = (duration_secs as usize * 4).max(8);
    let exit_window_size     = (exit_duration_secs as usize * 4).max(4);
    let sample_count         = s.dnd_focus_samples.len();
    let avg_score            = if sample_count > 0 {
        s.dnd_focus_samples.iter().sum::<f64>() / sample_count as f64
    } else { 0.0 };
    let dnd_active           = s.dnd_active;
    let below_ticks          = s.dnd_below_ticks;
    let exit_held_by_lookback = dnd_active
        && avg_score < threshold
        && s.dnd_score_history.iter().any(|&v| v >= threshold);
    // Use the cached OS state (refreshed every 5 s by the background poll)
    // rather than reading the file on every UI request.
    let os_active            = s.dnd_os_active;
    drop(s);

    let exit_secs_remaining =
        if dnd_active && avg_score < threshold && !exit_held_by_lookback {
            let remaining = exit_window_size.saturating_sub(below_ticks as usize);
            remaining as f64 / 4.0
        } else { 0.0 };

    DndStatus {
        enabled, avg_score, threshold, sample_count, window_size,
        duration_secs, dnd_active, os_active,
        exit_duration_secs, below_ticks, exit_window_size, exit_secs_remaining,
        focus_lookback_secs, exit_held_by_lookback,
    }
}

/// Open a native file-picker dialog and return the selected WAV file path.
///
/// Returns `None` if the user cancels.  The dialog is opened on a blocking
/// thread so it does not hold the Tauri async executor.
#[tauri::command]
pub async fn pick_ref_wav_file() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("WAV audio", &["wav"])
            .set_title("Select reference WAV for voice cloning")
            .pick_file()
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .ok()
    .flatten()
}

