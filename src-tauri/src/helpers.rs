// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Shared helpers: time, status/device emitters, toast, device upsert,
//! settings persistence, state access shortcuts.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::settings::{save_secrets_from_settings, settings_path};
use crate::state::*;
use crate::ws_server::WsBroadcaster;
use crate::MutexExt;

// ── Time helpers ──────────────────────────────────────────────────────────────

pub(crate) fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Returns today's date as `YYYYMMDD` (UTC).
/// Delegates to [`skill_data::util::yyyymmdd_utc`] — the canonical implementation.
pub(crate) fn yyyymmdd_utc() -> String {
    skill_data::util::yyyymmdd_utc()
}

// ── Status / device emitters ──────────────────────────────────────────────────

/// Apply all daemon `StatusResponse` fields onto the local `DeviceStatus`.
///
/// The daemon is the source of truth for device state; the Tauri UI is a thin
/// client.  This helper ensures every field is copied (no cherry-picking).
pub(crate) fn apply_daemon_status(
    local: &mut crate::DeviceStatus,
    ds: skill_daemon_common::StatusResponse,
) {
    local.state = ds.state;
    local.device_name = ds.device_name;
    local.device_kind = ds.device_kind;
    local.device_id = ds.device_id;
    local.sample_count = ds.sample_count;
    local.battery = ds.battery;
    local.device_error = ds.device_error;
    local.target_name = ds.target_name;
    local.target_id = ds.target_id;
    local.target_display_name = ds.target_display_name;
    local.retry_attempt = ds.retry_attempt;
    local.retry_countdown_secs = ds.retry_countdown_secs;
    local.paired_devices = ds
        .paired_devices
        .into_iter()
        .map(|d| crate::PairedDevice {
            id: d.id,
            name: d.name,
            last_seen: d.last_seen,
        })
        .collect();
    local.csv_path = ds.csv_path;
    local.channel_names = ds.channel_names;
    local.ppg_channel_names = ds.ppg_channel_names;
    local.imu_channel_names = ds.imu_channel_names;
    local.fnirs_channel_names = ds.fnirs_channel_names;
    local.eeg_channel_count = ds.eeg_channel_count;
    local.eeg_sample_rate_hz = ds.eeg_sample_rate_hz;
    local.channel_quality = ds
        .channel_quality
        .into_iter()
        .map(|q| match q.as_str() {
            "good" => skill_eeg::eeg_quality::SignalQuality::Good,
            "fair" => skill_eeg::eeg_quality::SignalQuality::Fair,
            "poor" => skill_eeg::eeg_quality::SignalQuality::Poor,
            _ => skill_eeg::eeg_quality::SignalQuality::NoSignal,
        })
        .collect();
    local.serial_number = ds.serial_number;
    local.mac_address = ds.mac_address;
    local.firmware_version = ds.firmware_version;
    local.hardware_version = ds.hardware_version;
    local.has_ppg = ds.has_ppg;
    local.has_imu = ds.has_imu;
    local.has_central_electrodes = ds.has_central_electrodes;
    local.has_full_montage = ds.has_full_montage;
    local.ppg_sample_count = ds.ppg_sample_count;
    local.phone_info = ds.phone_info;
    local.iroh_client_name = ds.iroh_client_name;
    local.iroh_tunnel_online = ds.iroh_tunnel_online;
    local.iroh_connected_peers = ds.iroh_connected_peers;
    local.iroh_remote_device_connected = ds.iroh_remote_device_connected;
    local.iroh_streaming_active = ds.iroh_streaming_active;
    local.iroh_eeg_streaming_active = ds.iroh_eeg_streaming_active;
}

/// Emit status to the frontend and local WS clients.
/// The daemon is the sole authority — no mirroring back.
pub(crate) fn emit_status(app: &AppHandle) {
    let s_ref = app.app_state();
    let st = {
        let g = s_ref.lock_or_recover();
        g.status.clone()
    };
    let _ = app.emit("status", &st);
    app.state::<WsBroadcaster>().send("status", &st);
    crate::daemon_cmds::push_event_to_daemon("status", &st);
    crate::tray::refresh_tray(app);
}

/// Alias kept for call-site clarity — both paths now behave identically.
pub(crate) fn emit_status_from_daemon(app: &AppHandle) {
    emit_status(app);
}

pub(crate) fn emit_devices(app: &AppHandle) {
    let s_ref = app.app_state();
    let d = {
        let g = s_ref.lock_or_recover();
        g.discovered.clone()
    };
    let _ = app.emit("devices-updated", &d);
}

// Cortex WS state management moved to skill-daemon scanner backend.

// ── Toast / notification helpers ──────────────────────────────────────────────

/// Toast severity levels — serialised to the frontend as lowercase strings.
#[allow(dead_code)]
#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Serialize)]
struct ToastPayload {
    level: ToastLevel,
    title: String,
    message: String,
}

/// Send an in-app toast event AND a native OS notification.
pub(crate) fn send_toast(app: &AppHandle, level: ToastLevel, title: &str, message: &str) {
    let payload = ToastPayload {
        level,
        title: title.to_owned(),
        message: message.to_owned(),
    };
    let _ = app.emit("toast", &payload);
    app.state::<WsBroadcaster>().send("toast", &payload);
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(message)
        .show();
}

// ── State access helpers ──────────────────────────────────────────────────────

/// Extension trait that reduces the verbose
/// `app.state::<Mutex<Box<AppState>>>()` pattern (137+ call sites).
///
/// Implemented as a blanket impl for anything that implements `Manager<Wry>`,
/// so it works on `AppHandle`, `&AppHandle`, `App`, `WebviewWindow`, etc.
pub(crate) trait AppStateExt: Manager<tauri::Wry> {
    /// Obtain a reference to the `Mutex<Box<AppState>>` managed state.
    fn app_state(&self) -> tauri::State<'_, Mutex<Box<AppState>>> {
        self.state::<Mutex<Box<AppState>>>()
    }
}

impl<T: Manager<tauri::Wry>> AppStateExt for T {}

/// Mutate `AppState` and auto-persist settings afterwards.
pub(crate) fn mutate_and_save(app: &AppHandle, f: impl FnOnce(&mut AppState)) {
    {
        let r = app.app_state();
        let mut g = r.lock_or_recover();
        f(&mut g);
    }
    save_settings(app);
}

// ── Settings persistence (debounced) ──────────────────────────────────────────

/// Debounce interval for settings persistence.  Multiple `save_settings` calls
/// within this window are collapsed into a single disk write.
const SETTINGS_DEBOUNCE_MS: u64 = 500;

/// Atomic flag: `true` while a debounce timer is already pending.
static SAVE_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Schedule a settings save.  The actual disk write is debounced: if another
/// `save_settings` call arrives within [`SETTINGS_DEBOUNCE_MS`], only one
/// write is performed.  This prevents I/O storms when the user rapidly
/// toggles multiple settings.
pub(crate) fn save_settings(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    // If a timer is already pending, skip — it will pick up the latest state.
    if SAVE_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(SETTINGS_DEBOUNCE_MS)).await;
        SAVE_PENDING.store(false, Ordering::Release);
        save_settings_now(&handle);
    });
}

/// Immediately flush settings to disk (no debounce).  Called by the
/// debounce timer and by shutdown paths that must persist before exit.
///
/// Uses read-modify-write: loads the existing settings from disk (which
/// includes daemon-owned fields), overlays Tauri-owned UI/preference
/// fields, and writes the result back.  This avoids clobbering fields
/// that the daemon writes directly (hooks, LSL config, skills sync, etc.).
pub(crate) fn save_settings_now(app: &AppHandle) {
    let s_ref = app.app_state();
    let s = s_ref.lock_or_recover();
    let skill_dir = s.skill_dir.clone();
    let path = settings_path(&skill_dir);

    // Start from the on-disk state so daemon-owned fields are preserved.
    let mut data = skill_settings::load_settings(&skill_dir);

    // ── Overlay Tauri-owned fields ───────────────────────────────────────
    // Device / session state (daemon also reads these at startup)
    data.paired = s.status.paired_devices.clone();
    data.preferred_id = s.preferred_id.clone();
    data.filter_config = s.status.filter_config;
    data.embedding_overlap_secs = s.status.embedding_overlap_secs;

    // Keyboard shortcuts
    data.label_shortcut = s.shortcuts.label_shortcut.clone();
    data.search_shortcut = s.shortcuts.search_shortcut.clone();
    data.settings_shortcut = s.shortcuts.settings_shortcut.clone();
    data.calibration_shortcut = s.shortcuts.calibration_shortcut.clone();
    data.help_shortcut = s.shortcuts.help_shortcut.clone();
    data.history_shortcut = s.shortcuts.history_shortcut.clone();
    data.api_shortcut = s.shortcuts.api_shortcut.clone();
    data.theme_shortcut = s.shortcuts.theme_shortcut.clone();
    data.focus_timer_shortcut = s.shortcuts.focus_timer_shortcut.clone();
    #[cfg(feature = "llm")]
    {
        data.chat_shortcut = s.shortcuts.chat_shortcut.clone();
    }

    // Calibration
    data.calibration_profiles = s.calibration_profiles.clone();
    data.active_calibration_id = s.active_calibration_id.clone();

    // UI preferences
    data.onboarding_complete = s.ui.onboarding_complete;
    data.last_seen_whats_new_version = s.ui.last_seen_whats_new_version.clone();
    data.theme = s.ui.theme.clone();
    data.language = s.ui.language.clone();
    data.accent_color = s.ui.accent_color.clone();
    data.daily_goal_min = s.ui.daily_goal_min;
    data.goal_notified_date = s.ui.goal_notified_date.clone();
    data.text_embedding_model = s.ui.text_embedding_model.clone();
    data.main_window_auto_fit = s.ui.main_window_auto_fit;

    // Infrastructure / server config
    data.ws_host = s.ws_host.clone();
    data.ws_port = s.ws_port;
    data.api_token = s.api_token.clone();
    data.hf_endpoint = s.hf_endpoint.clone();
    data.update_check_interval_secs = s.update_check_interval_secs;

    // Hardware / device config
    data.openbci = s.openbci_config.clone();
    data.device_api = s.device_api_config.clone();
    data.neutts = s.neutts_config.clone();
    data.tts_preload = s.tts_preload;
    data.screenshot = s.screenshot_config.clone();
    data.sleep = s.sleep_config.clone();
    data.storage_format = s.settings_storage_format.clone();
    data.scanner = s.scanner_config.clone();
    data.location_enabled = s.location_enabled;
    data.inference_device = s.inference_device.clone();
    data.llm_gpu_layers_saved = s.llm_gpu_layers_saved;
    data.exg_inference_device = s.exg_inference_device.clone();

    // Input tracking
    data.track_active_window = s.input.track_active_window;
    data.track_input_activity = s.input.track_input_activity;

    // DND
    data.do_not_disturb = s.dnd.lock_or_recover().config.clone();

    // LLM config: overlay everything except `tools` (daemon-owned).
    {
        let llm_guard = s.llm.lock_or_recover();
        let tools_backup = data.llm.tools.clone();
        data.llm = llm_guard.config.clone();
        data.llm.tools = tools_backup;
    }

    drop(s);

    // Persist secrets to the system keychain (encrypted, survives updates).
    save_secrets_from_settings(&data);

    if let Ok(json) = serde_json::to_string_pretty(&data) {
        if let Err(e) = std::fs::write(&path, &json) {
            eprintln!("[settings] save error: {e}");
        }
    }
}

// ── Transport inference ───────────────────────────────────────────────────────

/// Infer the transport type from a device ID.
///
/// Device IDs are prefixed by the scanner backend that discovered them:
/// * `usb:<port>`   → USB serial
/// * `cortex:<id>`  → Emotiv Cortex WebSocket
/// * anything else  → BLE (the default / legacy format)
pub(crate) fn transport_from_id(id: &str) -> skill_daemon_common::DeviceTransport {
    use skill_daemon_common::DeviceTransport;
    if id.starts_with("usb:") {
        DeviceTransport::UsbSerial
    } else if id.starts_with("cortex:") {
        DeviceTransport::Cortex
    } else {
        DeviceTransport::Ble
    }
}

// Device upsert helpers removed — device discovery/pairing is daemon-owned.
