// SPDX-License-Identifier: GPL-3.0-only
//! Per-device connection logic — transport-specific setup (BLE, serial,
//! Cortex WS, PCAN) → `Box<dyn DeviceAdapter>` for the generic runner.

use std::time::Duration;

use skill_daemon_common::DeviceLogEntry;
use skill_devices::session::DeviceAdapter;
use tokio::sync::oneshot;
use tracing::{error, info};

use super::connect_ble;
use super::connect_wired;
use super::runner::run_adapter_session;
use crate::session_runner::SessionHandle;
use crate::state::AppState;

/// Spawn a device session for the given target.  Returns a cancel handle.
pub fn spawn_device_session(state: AppState, target: String) -> Option<SessionHandle> {
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let state2 = state.clone();

    tokio::task::spawn(async move {
        if let Ok(mut s) = state2.status.lock() {
            let target_id = if target.contains(':') {
                Some(target.clone())
            } else {
                s.paired_devices.iter().find(|d| d.name == target).map(|d| d.id.clone())
            };
            let target_display_name = if target.contains(':') {
                s.paired_devices
                    .iter()
                    .find(|d| d.id == target)
                    .map(|d| d.name.clone())
                    .or_else(|| Some(target.clone()))
            } else {
                Some(target.clone())
            };
            s.state = "connecting".into();
            s.target_name = Some(target.clone());
            s.target_id = target_id;
            s.target_display_name = target_display_name;
            s.device_error = None;
        }

        // ── Routing log ──────────────────────────────────────────────────
        // Produces: [devices] [session] routing: target=… kind=…
        // Visible in the device log and tracing output so connection
        // failures are easy to diagnose.
        let routed_kind = if target.starts_with("ble:") {
            paired_name_for(&state2, &target)
                .map(|name| infer_kind_from_target(&name))
                .unwrap_or("ble-unknown")
        } else {
            infer_kind_from_target(&target)
        };
        push_device_log_static(
            &state2,
            "session",
            &format!("routing: target={target:?} kind={routed_kind}"),
        );
        info!(target = %target, kind = %routed_kind, "session routing");

        match connect_device(&state2, &target).await {
            Ok(adapter) => {
                run_adapter_session(state2.clone(), cancel_rx, adapter).await;
            }
            Err(e) => {
                error!(%e, %target, "device connect failed");
                push_device_log_static(
                    &state2,
                    "session",
                    &format!("connect failed: target={target:?} err={e}"),
                );
                // Only update state if this session is still the current one.
                // If the target changed (user connected a different device
                // while this connect was running), don't clobber their session.
                let still_current = state2
                    .status
                    .lock()
                    .ok()
                    .map(|s| s.target_id.as_deref() == Some(&target) || s.target_name.as_deref() == Some(&target))
                    .unwrap_or(true);
                if still_current {
                    if let Ok(mut s) = state2.status.lock() {
                        s.state = "disconnected".into();
                        s.device_error = Some(e.to_string());
                    }
                }
            }
        }
        if let Ok(mut slot) = state2.session_handle.lock() {
            *slot = None;
        }
    });

    Some(SessionHandle { cancel_tx })
}

fn requires_pairing(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    // LSL streams are logical network sources, not pairable hardware.
    // Iroh remote peers are pre-authenticated by the iroh tunnel (TOTP-paired).
    !(lower == "lsl" || lower.starts_with("lsl:") || lower.starts_with("peer:"))
}

fn is_paired(state: &AppState, target: &str) -> bool {
    state
        .status
        .lock()
        .ok()
        .map(|s| s.paired_devices.iter().any(|d| d.id == target || d.name == target))
        .unwrap_or(false)
}

async fn connect_device(state: &AppState, target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    let lower = target.to_lowercase();

    // Defense-in-depth: session control endpoints already enforce pairing for
    // scanner/device targets. Keep the same invariant here in case connect
    // paths are called from future internal entry points.
    if requires_pairing(target) && !is_paired(state, target) {
        anyhow::bail!("Target device is not paired. Pair it first in Settings → Devices.");
    }

    // Devices that use their own BLE scanner (btleplug CBCentralManager) need
    // the background BLE listener scan to be stopped first.  On macOS, two
    // concurrent CBCentralManager.scanForPeripherals() calls suppress the
    // centralManager(_:didConnect:) delegate callback, so peripheral.connect()
    // hangs forever.  We pause here once for every BLE-scanning connect path
    // rather than duplicating the logic in each individual function.
    let needs_ble_pause = lower == "ganglion"
        || lower.contains("mw75")
        || lower.contains("neurable")
        || lower.contains("hermes")
        || lower.contains("mendi")
        || lower.contains("idun")
        || lower.contains("guardian")
        || lower.contains("awear")
        || lower.starts_with("luca")
        || lower.starts_with("ige")
        || lower.starts_with("ble:")
        // catch generic Muse targets (device name used as target)
        || lower.starts_with("muse");

    if needs_ble_pause {
        state.ble_scan_paused.store(true, std::sync::atomic::Ordering::Relaxed);
        // Allow up to 400 ms for the listener task to detect the flag and
        // call stop_scan().  The event loop now has a 300 ms timeout so the
        // listener notices the flag within 300 ms; stop_scan() is near-instant.
        tokio::time::sleep(Duration::from_millis(400)).await;
    }

    let result = connect_device_inner(state, target, &lower).await;

    if needs_ble_pause {
        state.ble_scan_paused.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    result
}

/// Look up the human-readable name for a paired device ID from the daemon's
/// in-memory paired list.  Used to give BLE clients a specific name prefix so
/// they can use the fast event-driven `connect()` path (~250 ms) instead of
/// the fixed-sleep `scan_all()` path (3-5 s).
fn paired_name_for(state: &AppState, target: &str) -> Option<String> {
    state
        .status
        .lock()
        .ok()
        .and_then(|s| s.paired_devices.iter().find(|d| d.id == target).map(|d| d.name.clone()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectRoute {
    OpenBci,
    Cognionics,
    Emotiv,
    Ganglion,
    Lsl,
    Brainmaster,
    Brainbit,
    Neurosky,
    Neurosity,
    Brainvision,
    Neurofield,
    Gtec,
    Mw75,
    Hermes,
    Idun,
    Awear,
    Mendi,
    IrohRemote,
    AntNeuro,
    Muse,
}

type ConnectPredicate = fn(&str) -> bool;

fn is_openbci(s: &str) -> bool {
    s == "openbci" || s.starts_with("usb:")
}
fn is_cognionics(s: &str) -> bool {
    s.starts_with("cgx:")
}
fn is_emotiv(s: &str) -> bool {
    s.starts_with("cortex:")
}
fn is_ganglion(s: &str) -> bool {
    s == "ganglion"
}
fn is_lsl(s: &str) -> bool {
    s.starts_with("lsl:") || s == "lsl"
}
fn is_brainmaster(s: &str) -> bool {
    s.starts_with("brainmaster:") || s.contains("brainmaster")
}
fn is_brainbit(s: &str) -> bool {
    s.starts_with("brainbit:") || s.contains("brainbit")
}
fn is_neurosky(s: &str) -> bool {
    s.starts_with("neurosky:") || s == "neurosky" || s.contains("mindwave")
}
fn is_neurosity(s: &str) -> bool {
    s.starts_with("neurosity:") || s == "neurosity" || s.contains("crown") || s.contains("notion")
}
fn is_brainvision(s: &str) -> bool {
    s.starts_with("brainvision:") || s == "brainvision" || s.starts_with("rda:")
}
fn is_neurofield(s: &str) -> bool {
    s.starts_with("neurofield:") || s.contains("neurofield")
}
fn is_gtec(s: &str) -> bool {
    s.starts_with("gtec:") || s.contains("unicorn")
}
fn is_mw75(s: &str) -> bool {
    s.contains("mw75") || s.contains("neurable")
}
fn is_hermes(s: &str) -> bool {
    s.contains("hermes")
}
fn is_idun(s: &str) -> bool {
    s.contains("idun") || s.contains("guardian")
}
fn is_awear(s: &str) -> bool {
    s.contains("awear") || s.starts_with("luca")
}
fn is_mendi(s: &str) -> bool {
    s.contains("mendi")
}
fn is_iroh_remote(s: &str) -> bool {
    s.starts_with("peer:")
}
fn is_antneuro(s: &str) -> bool {
    s.starts_with("antneuro:") || s.contains("antneuro") || s.contains("eego")
}

const CONNECT_ROUTE_RULES: &[(ConnectPredicate, ConnectRoute)] = &[
    (is_openbci, ConnectRoute::OpenBci),
    (is_cognionics, ConnectRoute::Cognionics),
    (is_emotiv, ConnectRoute::Emotiv),
    (is_ganglion, ConnectRoute::Ganglion),
    (is_lsl, ConnectRoute::Lsl),
    (is_brainmaster, ConnectRoute::Brainmaster),
    (is_brainbit, ConnectRoute::Brainbit),
    (is_neurosky, ConnectRoute::Neurosky),
    (is_neurosity, ConnectRoute::Neurosity),
    (is_brainvision, ConnectRoute::Brainvision),
    (is_neurofield, ConnectRoute::Neurofield),
    (is_gtec, ConnectRoute::Gtec),
    (is_mw75, ConnectRoute::Mw75),
    (is_hermes, ConnectRoute::Hermes),
    (is_idun, ConnectRoute::Idun),
    (is_awear, ConnectRoute::Awear),
    (is_mendi, ConnectRoute::Mendi),
    (is_antneuro, ConnectRoute::AntNeuro),
    (is_iroh_remote, ConnectRoute::IrohRemote),
];

fn matching_connect_routes(lower: &str) -> Vec<ConnectRoute> {
    CONNECT_ROUTE_RULES
        .iter()
        .filter_map(|(pred, route)| pred(lower).then_some(*route))
        .collect()
}

fn select_connect_route(lower: &str) -> ConnectRoute {
    matching_connect_routes(lower)
        .into_iter()
        .next()
        .unwrap_or(ConnectRoute::Muse)
}

async fn connect_device_inner(state: &AppState, target: &str, lower: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    // For `ble:<uuid>` targets the UUID alone carries no device-kind
    // information.  Look up the human-readable name from the paired
    // devices list and route on that instead.
    let route = if lower.starts_with("ble:") {
        paired_name_for(state, target)
            .map(|name| select_connect_route(&name.to_lowercase()))
            .unwrap_or(ConnectRoute::Muse)
    } else {
        select_connect_route(lower)
    };
    match route {
        ConnectRoute::OpenBci => connect_wired::connect_openbci(state, target).await,
        ConnectRoute::Cognionics => connect_wired::connect_cognionics(target).await,
        ConnectRoute::Emotiv => connect_wired::connect_emotiv(state).await,
        ConnectRoute::Ganglion => connect_ble::connect_ganglion(state).await,
        ConnectRoute::Lsl => connect_wired::connect_lsl(target).await,
        ConnectRoute::Brainmaster => connect_wired::connect_brainmaster(state, target).await,
        ConnectRoute::Brainbit => connect_ble::connect_brainbit(target).await,
        ConnectRoute::Neurosky => connect_wired::connect_neurosky(target).await,
        ConnectRoute::Neurosity => connect_wired::connect_neurosity(state, target).await,
        ConnectRoute::Brainvision => connect_wired::connect_brainvision(target).await,
        ConnectRoute::Neurofield => connect_wired::connect_neurofield(target).await,
        ConnectRoute::Gtec => connect_ble::connect_gtec(target).await,
        ConnectRoute::Mw75 => connect_ble::connect_mw75(paired_name_for(state, target)).await,
        ConnectRoute::Hermes => connect_ble::connect_hermes(paired_name_for(state, target)).await,
        ConnectRoute::Idun => connect_ble::connect_idun(state, paired_name_for(state, target)).await,
        ConnectRoute::Awear => connect_ble::connect_awear(paired_name_for(state, target)).await,
        ConnectRoute::Mendi => connect_ble::connect_mendi(paired_name_for(state, target)).await,
        ConnectRoute::AntNeuro => connect_wired::connect_antneuro(state, target).await,
        ConnectRoute::IrohRemote => connect_wired::connect_iroh_remote(state, target).await,
        ConnectRoute::Muse => connect_ble::connect_muse(target, paired_name_for(state, target)).await,
    }
}

// ── Routing helpers ────────────────────────────────────────────────────────────────

/// Infer a human-readable device kind string from the raw target identifier.
///
/// Used only for diagnostic logging in [`spawn_device_session`]; not used for
/// actual routing decisions (that remains in [`connect_device`]).
fn infer_kind_from_target(target: &str) -> &'static str {
    let lower = target.to_lowercase();
    if lower.starts_with("neurofield:") {
        return "neurofield";
    }
    if lower.starts_with("neurosky:") || lower == "neurosky" {
        return "neurosky";
    }
    if lower.starts_with("neurosity:") {
        return "neurosity";
    }
    if lower.starts_with("brainvision:") {
        return "brainvision";
    }
    if lower.starts_with("brainbit:") {
        return "brainbit";
    }
    if lower.starts_with("gtec:") {
        return "gtec";
    }
    if lower.starts_with("brainmaster:") {
        return "brainmaster";
    }
    if lower.starts_with("cortex:") {
        return "emotiv";
    }
    if lower.starts_with("cgx:") {
        return "cognionics";
    }
    if lower.starts_with("lsl:") || lower == "lsl" {
        return "lsl";
    }
    if lower.starts_with("peer:") {
        return "iroh-remote";
    }
    if lower.starts_with("usb:") {
        return "openbci/cyton";
    } // serial → Cyton, not Ganglion
    if lower == "ganglion" {
        return "ganglion";
    }
    if lower == "openbci" {
        return "openbci";
    }
    if lower.contains("mw75") || lower.contains("neurable") {
        return "mw75";
    }
    if lower.contains("hermes") {
        return "hermes";
    }
    if lower.contains("idun") || lower.contains("guardian") {
        return "idun";
    }
    if lower.contains("awear") || lower.starts_with("luca") {
        return "awear";
    }
    if lower.contains("mendi") {
        return "mendi";
    }
    if lower.starts_with("antneuro:") || lower.contains("antneuro") || lower.contains("eego") {
        return "antneuro";
    }
    "muse"
}

/// Append an entry to the state device log (used by `spawn_device_session`).
///
/// Mirrors the `push_device_log` helper in `main.rs` without requiring a
/// shared reference to it across the module boundary.
fn push_device_log_static(state: &AppState, tag: &str, msg: &str) {
    let entry = DeviceLogEntry {
        ts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        tag: tag.to_string(),
        msg: msg.to_string(),
    };
    if let Ok(mut guard) = state.device_log.lock() {
        if guard.len() >= 256 {
            guard.pop_front();
        }
        guard.push_back(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_kind_covers_supported_device_targets() {
        let cases = [
            ("muse", "muse"),
            ("MW75-ABCD", "mw75"),
            ("Hermes-001", "hermes"),
            ("Idun-Guardian", "idun"),
            ("AWEAR-E04A8471", "awear"),
            ("Mendi-XY", "mendi"),
            ("ganglion", "ganglion"),
            ("openbci", "openbci"),
            ("usb:COM3", "openbci/cyton"),
            ("lsl", "lsl"),
            ("lsl:SkillVirtualEEG", "lsl"),
            ("neurofield:USB1:5", "neurofield"),
            ("brainbit:AA:BB", "brainbit"),
            ("gtec:UN-123", "gtec"),
            ("brainmaster:/dev/ttyUSB0", "brainmaster"),
            ("cortex:emotiv", "emotiv"),
            ("cgx:/dev/ttyUSB1", "cognionics"),
            ("neurosky:/dev/ttyUSB0", "neurosky"),
            ("neurosity:device123", "neurosity"),
            ("brainvision:127.0.0.1:51244", "brainvision"),
            ("antneuro:0", "antneuro"),
        ];

        for (target, expected) in cases {
            assert_eq!(infer_kind_from_target(target), expected, "target={target}");
        }
    }

    #[test]
    fn lsl_target_query_parsing_configurations() {
        assert_eq!(connect_wired::lsl_query_from_target("lsl"), "");
        assert_eq!(connect_wired::lsl_query_from_target("lsl:"), "");
        assert_eq!(
            connect_wired::lsl_query_from_target("lsl:SkillVirtualEEG"),
            "SkillVirtualEEG"
        );
        assert_eq!(
            connect_wired::lsl_query_from_target("lsl:EEG-32ch@1kHz"),
            "EEG-32ch@1kHz"
        );
        assert_eq!(
            connect_wired::lsl_query_from_target("LSL:SkillVirtualEEG"),
            "SkillVirtualEEG"
        );
        assert_eq!(connect_wired::lsl_query_from_target("LsL:MixedCase"), "MixedCase");
        assert_eq!(connect_wired::lsl_query_from_target("not-lsl"), "");
    }

    #[tokio::test]
    async fn connect_lsl_missing_named_stream_returns_error() {
        let t0 = std::time::Instant::now();
        let res = connect_wired::connect_lsl("lsl:THIS_STREAM_SHOULD_NOT_EXIST_987654321").await;
        let elapsed = t0.elapsed();
        assert!(res.is_err(), "missing LSL stream should error");
        let msg = res.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("No LSL stream matching"), "unexpected error: {msg}");
        assert!(
            elapsed < std::time::Duration::from_secs(8),
            "LSL missing-stream failure too slow: {elapsed:?}"
        );
    }

    #[test]
    fn infer_kind_unknown_defaults_to_muse() {
        assert_eq!(infer_kind_from_target("totally-unknown-device"), "muse");
    }

    #[test]
    fn infer_kind_prefixes_are_case_insensitive() {
        assert_eq!(infer_kind_from_target("NEUROFIELD:USB1:1"), "neurofield");
        assert_eq!(infer_kind_from_target("BRAINBIT:AA:BB"), "brainbit");
        assert_eq!(infer_kind_from_target("GTEC:UN-1"), "gtec");
        assert_eq!(infer_kind_from_target("BRAINMASTER:COM3"), "brainmaster");
        assert_eq!(infer_kind_from_target("CORTEX:EMOTIV"), "emotiv");
        assert_eq!(infer_kind_from_target("CGX:/dev/ttyUSB0"), "cognionics");
        assert_eq!(infer_kind_from_target("LSL:MyStream"), "lsl");
        assert_eq!(infer_kind_from_target("USB:COM4"), "openbci/cyton");
    }

    #[test]
    fn select_connect_route_covers_aliases_and_prefixes() {
        let cases = [
            ("openbci", ConnectRoute::OpenBci),
            ("usb:COM3", ConnectRoute::OpenBci),
            ("cgx:/dev/ttyUSB1", ConnectRoute::Cognionics),
            ("cortex:emotiv", ConnectRoute::Emotiv),
            ("ganglion", ConnectRoute::Ganglion),
            ("lsl", ConnectRoute::Lsl),
            ("lsl:SkillVirtualEEG", ConnectRoute::Lsl),
            ("brainmaster:/dev/ttyUSB0", ConnectRoute::Brainmaster),
            ("brainbit:AA:BB", ConnectRoute::Brainbit),
            ("neurosky:/dev/ttyUSB0", ConnectRoute::Neurosky),
            ("neurosity:device123", ConnectRoute::Neurosity),
            ("brainvision:127.0.0.1:51244", ConnectRoute::Brainvision),
            ("neurofield:USB1:5", ConnectRoute::Neurofield),
            ("gtec:UN-123", ConnectRoute::Gtec),
            ("MW75-ABCD", ConnectRoute::Mw75),
            ("Hermes-001", ConnectRoute::Hermes),
            ("Idun-Guardian", ConnectRoute::Idun),
            ("AWEAR-E04A8471", ConnectRoute::Awear),
            ("Mendi-XY", ConnectRoute::Mendi),
            ("antneuro:0", ConnectRoute::AntNeuro),
            ("totally-unknown-device", ConnectRoute::Muse),
        ];

        for (target, expected) in cases {
            let lower = target.to_ascii_lowercase();
            assert_eq!(select_connect_route(&lower), expected, "target={target}");
        }
    }

    #[test]
    fn connect_route_rules_do_not_overlap_for_known_targets() {
        let targets = [
            "openbci",
            "usb:COM3",
            "cgx:/dev/ttyUSB1",
            "cortex:emotiv",
            "ganglion",
            "lsl:SkillVirtualEEG",
            "brainmaster:/dev/ttyUSB0",
            "brainbit:AA:BB",
            "neurosky:/dev/ttyUSB0",
            "neurosity:device123",
            "brainvision:127.0.0.1:51244",
            "neurofield:USB1:5",
            "gtec:UN-123",
            "MW75-ABCD",
            "Hermes-001",
            "Idun-Guardian",
            "AWEAR-E04A8471",
            "Mendi-XY",
            "antneuro:0",
        ];

        for target in targets {
            let lower = target.to_ascii_lowercase();
            let matches = matching_connect_routes(&lower);
            assert_eq!(
                matches.len(),
                1,
                "ambiguous route rules for target={target}: {matches:?}"
            );
        }
    }

    #[test]
    fn select_connect_route_is_deterministic_for_random_targets() {
        use rand::{RngExt, SeedableRng};

        let mut rng = rand::rngs::StdRng::seed_from_u64(0x5EED_BAAD_F00D);
        for _ in 0..512 {
            let len = rng.random_range(0..64);
            let s: String = (0..len)
                .map(|_| {
                    let c = rng.random_range(0x20u8..0x7Eu8);
                    c as char
                })
                .collect();
            let lower = s.to_ascii_lowercase();
            let a = select_connect_route(&lower);
            let b = select_connect_route(&lower);
            assert_eq!(a, b, "non-deterministic route for input={s:?}");
        }
    }

    #[test]
    fn paired_name_lookup_uses_status_paired_devices() {
        let td = tempfile::tempdir().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        if let Ok(mut s) = state.status.lock() {
            s.paired_devices.push(skill_daemon_common::PairedDeviceResponse {
                id: "ble:abc".into(),
                name: "Muse S Alice".into(),
                last_seen: 0,
            });
        }

        assert_eq!(paired_name_for(&state, "ble:abc").as_deref(), Some("Muse S Alice"));
        assert_eq!(paired_name_for(&state, "ble:missing"), None);
    }

    #[test]
    fn push_device_log_static_caps_to_256() {
        let td = tempfile::tempdir().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        for i in 0..300 {
            push_device_log_static(&state, "session", &format!("m{i}"));
        }

        let log = state.device_log.lock().unwrap();
        assert_eq!(log.len(), 256);
        assert_eq!(log.front().map(|e| e.msg.clone()), Some("m44".into()));
        assert_eq!(log.back().map(|e| e.msg.clone()), Some("m299".into()));
    }
}
