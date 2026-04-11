// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Session lifecycle — disconnect handling.
//!
//! The reconnect state machine now lives in skill-daemon.  This module
//! retains the Tauri-side disconnect handler (local state cleanup + UI
//! refresh) and the device-kind detection heuristic.

use tauri::AppHandle;

use crate::{
    helpers::{emit_status, AppStateExt},
    tray::refresh_tray,
    MutexExt,
};

// ── Disconnect ──────────────────────────────────────────────────────────────

/// Handle a device disconnect.  Cleans up local Tauri state and refreshes
/// the UI.  The daemon's reconnect loop handles retry scheduling.
#[allow(dead_code)]
pub(crate) fn go_disconnected(app: &AppHandle, error: Option<String>, is_bt: bool) {
    // Tell the daemon to cancel any active session.
    let _ = crate::daemon_cmds::cancel_session_sync();

    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();

        let new_state = if is_bt { "bt_off" } else { "disconnected" };
        s.status.reset_disconnected(new_state);
        if !is_bt {
            s.status.device_error = error;
        }
        s.stream = None;
        s.battery_ema = None;
        s.latest_bands = None;
        s.fnirs_runtime = crate::state::FnirsRuntime::default();
        s.session_start_utc = None;
    }
    refresh_tray(app);
    emit_status(app);

    // Tell daemon to disable reconnect for BT-off; otherwise the daemon's
    // reconnect loop handles it automatically.
    if is_bt {
        let _ = crate::daemon_cmds::disable_reconnect();
    }
}

// ── Session lifecycle ───────────────────────────────────────────────────────

/// Best-effort device-kind detection from daemon identifier and/or display name.
#[allow(dead_code)]
pub(crate) fn detect_device_kind(
    device_id: Option<&str>,
    device_name: Option<&str>,
) -> &'static str {
    if let Some(id) = device_id.map(str::to_ascii_lowercase) {
        if id.starts_with("neurofield:") {
            return "neurofield";
        }
        if id.starts_with("brainbit:") {
            return "brainbit";
        }
        if id.starts_with("gtec:") {
            return "gtec";
        }
        if id.starts_with("brainmaster:") {
            return "brainmaster";
        }
        if id.starts_with("cortex:") {
            return "emotiv";
        }
        if id.starts_with("usb:") {
            let n = device_name.map(str::to_ascii_lowercase).unwrap_or_default();
            if n.contains("cyton") {
                return "cyton";
            }
            if n.contains("ganglion") || n.contains("simblee") {
                return "ganglion";
            }
            return "openbci";
        }
        if id.starts_with("cgx:") {
            return "cognionics";
        }
    }

    let name = device_name.map(str::to_ascii_lowercase).unwrap_or_default();

    if name.starts_with("ganglion") || name.starts_with("simblee") {
        return "ganglion";
    }
    if name.contains("cyton") {
        return "cyton";
    }
    if name.contains("openbci") {
        return "openbci";
    }
    if name.contains("mw75") || name.contains("neurable") {
        return "mw75";
    }
    if name.starts_with("hermes") {
        return "hermes";
    }
    if name.starts_with("emotiv")
        || name.starts_with("epoc-x")
        || name.starts_with("insight")
        || name.starts_with("flex")
        || name.starts_with("mn8")
    {
        return "emotiv";
    }
    if name.starts_with("idun") || name.starts_with("guardian") || name.starts_with("ige") {
        return "idun";
    }
    if name.starts_with("mendi") {
        return "mendi";
    }
    if name.contains("cgx") || name.contains("cognionics") || name.contains("quick-20r") {
        return "cognionics";
    }
    if name.contains("neurofield") || name.contains("q21") {
        return "neurofield";
    }
    if name.contains("brainbit") {
        return "brainbit";
    }
    if name.contains("unicorn") || name.contains("g.tec") || name.contains("gtec") {
        return "gtec";
    }
    if name.contains("brainmaster") || name.contains("atlantis") || name.contains("discovery") {
        return "brainmaster";
    }

    "muse"
}

// ── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_device_kind_ganglion() {
        assert_eq!(detect_device_kind(None, Some("ganglion-1234")), "ganglion");
        assert_eq!(detect_device_kind(None, Some("simblee-001")), "ganglion");
    }

    #[test]
    fn detect_device_kind_mw75() {
        assert_eq!(detect_device_kind(None, Some("headphones-mw75-v2")), "mw75");
        assert_eq!(detect_device_kind(None, Some("neurable-xyz")), "mw75");
    }

    #[test]
    fn detect_device_kind_hermes() {
        assert_eq!(detect_device_kind(None, Some("hermes-abc")), "hermes");
    }

    #[test]
    fn detect_device_kind_emotiv() {
        assert_eq!(detect_device_kind(None, Some("emotiv-epoc-x")), "emotiv");
        assert_eq!(detect_device_kind(None, Some("epoc-x-1234")), "emotiv");
        assert_eq!(detect_device_kind(None, Some("insight-5ch")), "emotiv");
        assert_eq!(detect_device_kind(None, Some("flex-saline")), "emotiv");
        assert_eq!(detect_device_kind(None, Some("mn8-earbuds")), "emotiv");
    }

    #[test]
    fn detect_device_kind_idun() {
        assert_eq!(detect_device_kind(None, Some("idun-guardian")), "idun");
        assert_eq!(detect_device_kind(None, Some("guardian-001")), "idun");
        assert_eq!(detect_device_kind(None, Some("ige-1234")), "idun");
    }

    #[test]
    fn detect_device_kind_mendi() {
        assert_eq!(detect_device_kind(None, Some("mendi")), "mendi");
        assert_eq!(detect_device_kind(None, Some("mendi-1234")), "mendi");
    }

    #[test]
    fn detect_device_kind_cognionics() {
        assert_eq!(
            detect_device_kind(Some("cgx:/dev/ttyUSB0"), None),
            "cognionics"
        );
        assert_eq!(
            detect_device_kind(None, Some("cgx quick-20r")),
            "cognionics"
        );
        assert_eq!(
            detect_device_kind(None, Some("cognionics-device")),
            "cognionics"
        );
        assert_eq!(detect_device_kind(None, Some("quick-20r")), "cognionics");
    }

    #[test]
    fn detect_device_kind_muse_fallback() {
        assert_eq!(detect_device_kind(None, Some("muse-2")), "muse");
        assert_eq!(detect_device_kind(None, None), "muse");
        assert_eq!(detect_device_kind(None, Some("unknown-device")), "muse");
    }

    #[test]
    fn detect_device_kind_by_id_prefix() {
        assert_eq!(
            detect_device_kind(Some("cortex:EPOCX-1234"), None),
            "emotiv"
        );
        assert_eq!(
            detect_device_kind(Some("cortex:EPOCX-1234"), Some("unknown")),
            "emotiv"
        );
        assert_eq!(
            detect_device_kind(Some("usb:/dev/ttyUSB0"), None),
            "openbci"
        );
        assert_eq!(
            detect_device_kind(Some("usb:COM3"), Some("OpenBCI (COM3)")),
            "openbci"
        );
        assert_eq!(
            detect_device_kind(Some("usb:/dev/ttyUSB0"), Some("Cyton-1234")),
            "cyton"
        );
        assert_eq!(
            detect_device_kind(Some("usb:/dev/ttyUSB0"), Some("Ganglion-5678")),
            "ganglion"
        );
    }

    #[test]
    fn detect_device_kind_cyton_by_name() {
        assert_eq!(detect_device_kind(None, Some("Cyton-1234")), "cyton");
        assert_eq!(detect_device_kind(None, Some("cyton_daisy")), "cyton");
        assert_eq!(detect_device_kind(None, Some("My Cyton Board")), "cyton");
    }

    #[test]
    fn detect_device_kind_openbci_generic_name() {
        assert_eq!(detect_device_kind(None, Some("OpenBCI (COM3)")), "openbci");
        assert_eq!(detect_device_kind(None, Some("OpenBCI Device")), "openbci");
    }

    #[test]
    fn detect_device_kind_usb_cyton_name() {
        assert_eq!(
            detect_device_kind(Some("usb:COM3"), Some("Cyton-1234")),
            "cyton"
        );
        assert_eq!(
            detect_device_kind(Some("usb:COM5"), Some("CytonDaisy Board")),
            "cyton"
        );
    }

    #[test]
    fn detect_device_kind_usb_no_name_returns_openbci() {
        assert_eq!(detect_device_kind(Some("usb:COM3"), None), "openbci");
        assert_eq!(detect_device_kind(Some("usb:COM5"), Some("")), "openbci");
    }

    #[test]
    fn detect_device_kind_usb_openbci_display_name() {
        assert_eq!(
            detect_device_kind(Some("usb:COM3"), Some("OpenBCI (COM3)")),
            "openbci"
        );
    }

    #[test]
    fn detect_device_kind_usb_ganglion_name() {
        assert_eq!(
            detect_device_kind(Some("usb:/dev/ttyUSB0"), Some("Ganglion")),
            "ganglion"
        );
        assert_eq!(
            detect_device_kind(Some("usb:COM4"), Some("Simblee-1234")),
            "ganglion"
        );
    }

    #[test]
    fn detect_device_kind_windows_com_port() {
        assert_eq!(detect_device_kind(Some("usb:COM3"), None), "openbci");
        assert_eq!(detect_device_kind(Some("usb:COM10"), None), "openbci");
    }
}
