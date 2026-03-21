// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Unified background device scanner.
//!
//! Runs multiple transport-specific scan tasks in parallel:
//!
//! * **BLE** — discovers Muse, MW75, Hermes, Ganglion, IDUN devices
//! * **USB serial** — detects OpenBCI Cyton / CytonDaisy dongles
//! * **Cortex WebSocket** — checks for Emotiv headsets via the local Launcher
//!
//! Each backend calls [`on_device_discovered`] when a device is found,
//! which upserts it into the shared `discovered` list and triggers
//! auto-connect if the device is paired and the app is idle.

use std::time::Duration;

use btleplug::api::{
    Central, CentralEvent, CentralState, Manager as BtManager,
    Peripheral as BtPeripheral, ScanFilter,
};
use btleplug::platform::{Adapter as BtPlatformAdapter, Manager as BtPlatformManager};
use futures_util::StreamExt;
use serde::Serialize;
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::AppStateExt;
use crate::{
    MutexExt, ScannerHandle,
    emit_devices, emit_status, refresh_tray, send_toast, start_session, upsert_discovered,
    ToastLevel,
};

// ── Device log ring buffer ────────────────────────────────────────────────────

use std::sync::Mutex;

/// Thread-safe ring buffer holding the most recent device/scanner log lines
/// for the frontend log viewer.
pub(crate) static DEVICE_LOG: std::sync::LazyLock<Mutex<DeviceLogRing>> =
    std::sync::LazyLock::new(|| Mutex::new(DeviceLogRing::new(200)));

pub(crate) struct DeviceLogRing {
    entries: std::collections::VecDeque<DeviceLogEntry>,
    capacity: usize,
}

#[derive(Clone, serde::Serialize)]
pub(crate) struct DeviceLogEntry {
    /// UTC timestamp in milliseconds.
    pub ts: u64,
    /// Short tag: "ble", "usb", "cortex", "session", …
    pub tag: String,
    /// Human-readable message.
    pub msg: String,
}

impl DeviceLogRing {
    fn new(capacity: usize) -> Self {
        Self { entries: std::collections::VecDeque::with_capacity(capacity), capacity }
    }

    pub fn push(&mut self, tag: &str, msg: &str) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(DeviceLogEntry {
            ts,
            tag: tag.to_owned(),
            msg: msg.to_owned(),
        });
    }

    pub fn entries(&self) -> Vec<DeviceLogEntry> {
        self.entries.iter().cloned().collect()
    }
}

/// Push a device log entry (called from scanner backends and session runner).
pub(crate) fn device_log(tag: &str, msg: &str) {
    if let Ok(mut ring) = DEVICE_LOG.lock() {
        ring.push(tag, msg);
    }
}

/// MW75 GATT service UUID — used to discover paired MW75 devices on macOS
/// where `local_name` is often `None` for already-paired Classic BT devices.
use skill_devices::mw75::protocol::MW75_SERVICE_UUID;

// ── Transport tag ─────────────────────────────────────────────────────────────

/// How a device was discovered.  Serialised to the frontend so the UI can
/// show a transport icon / badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub(crate) enum Transport {
    /// Bluetooth Low Energy (Muse, MW75, Hermes, Ganglion, IDUN).
    Ble,
    /// USB serial port (OpenBCI Cyton / CytonDaisy dongle).
    UsbSerial,
    /// WiFi / LAN (OpenBCI WiFi Shield, Galea).
    Wifi,
    /// Emotiv Cortex WebSocket API (EPOC, Insight, Flex, MN8).
    Cortex,
}

// ── Bluetooth helpers (re-exported for session_connect) ───────────────────────

/// Classify a raw btleplug error string into a user-visible message and a flag
/// indicating whether the fault is BT-level (radio off / permission denied) vs
/// a transient connection error.
pub(crate) fn classify_device_error(raw: &str) -> (String, bool) {
    let lo = raw.to_lowercase();
    let is_bt = lo.contains("adapter")       || lo.contains("powered")
             || lo.contains("bluetooth")     || lo.contains("permission")
             || lo.contains("access denied") || lo.contains("org.bluez")
             || lo.contains("dbus");
    let msg = if is_bt {
        "Bluetooth is off or unavailable.\n\
         \n\
         • Enable Bluetooth in System Settings\n\
         • macOS: System Settings → Privacy & Security → Bluetooth\n\
         • Linux: make sure bluetoothd is running"
    } else {
        "Connection failed. Make sure the headset is powered on and in range."
    };
    (msg.into(), is_bt)
}

/// Quick sanity-check that returns `Err` with a user message if no BT adapter
/// is present or accessible.  Called at the start of every session attempt.
pub(crate) async fn bluetooth_ok() -> Result<(), (String, bool)> {
    let mgr = BtPlatformManager::new().await
        .map_err(|e| classify_device_error(&e.to_string()))?;
    let adapters = mgr.adapters().await
        .map_err(|e| classify_device_error(&e.to_string()))?;
    if adapters.is_empty() {
        return Err((
            "No Bluetooth adapter detected.\n\
             \n\
             • Enable Bluetooth in System Settings\n\
             • Linux: sudo systemctl start bluetooth".into(),
            true,
        ));
    }
    Ok(())
}

// ── Shared auto-connect helper ────────────────────────────────────────────────

/// Check whether a discovered device should trigger an automatic session start.
///
/// Conditions:
/// * App is idle (`disconnected`, no active stream, no pending reconnect).
/// * The device is paired, **OR** it was discovered via a trusted transport
///   (Cortex WebSocket, USB serial) where the device identity is reliable.
///
/// `start_session()` immediately sets `stream + pending_reconnect`, so
/// `is_idle` becomes false and this guard cannot fire again while a
/// connection attempt is in flight.
fn try_auto_connect(app: &AppHandle, id: &str, display_name: &str) {
    // Only auto-connect devices the user has explicitly paired.
    // Exception: legacy "cortex:emotiv" paired entries match any cortex
    // headset (migration compat from before individual IDs were tracked).
    //
    // Special case: when the paired devices list is completely empty (fresh
    // install / all devices unpaired), automatically adopt the first
    // discovered device so the user gets a seamless first-run experience
    // without having to manually pair.
    let should_auto = {
        let r = app.app_state();
        let g = r.lock_or_recover();
        let is_idle = g.stream.is_none()
            && !g.pending_reconnect
            && matches!(g.status.state.as_str(), "disconnected");
        let no_paired = g.status.paired_devices.is_empty();
        let is_paired = g.status.paired_devices.iter().any(|d| d.id == id);
        let legacy_cortex_paired = id.starts_with("cortex:")
            && g.status.paired_devices.iter().any(|d| d.id == "cortex:emotiv");
        is_idle && (is_paired || legacy_cortex_paired || no_paired)
    };
    if should_auto {
        // If paired list was empty, adopt this device first so reconnect
        // logic and the rest of the app treat it as a known device.
        {
            let r = app.app_state();
            let g = r.lock_or_recover();
            if g.status.paired_devices.is_empty() {
                drop(g);
                let msg = format!(
                    "No paired devices — auto-pairing first discovered device {display_name}"
                );
                app_log!(app, "scanner", "{msg}");
                device_log("session", &msg);
                crate::upsert_paired(app, id, display_name);
                crate::emit_devices(app);
            }
        }
        let msg = format!("Auto-connecting to paired device {display_name}");
        app_log!(app, "scanner", "{msg}");
        device_log("session", &msg);
        start_session(app, Some(id.to_owned()));
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// BLE scan backend
// ══════════════════════════════════════════════════════════════════════════════

/// Emit the `bt_off` UI state once per outage (edge-triggered).
fn scanner_bt_off(app: &AppHandle, emitted: &mut bool) {
    if *emitted { return; }
    *emitted = true;
    app_log!(app, "scanner", "[ble] bluetooth off");
    device_log("ble", "Bluetooth off");
    send_toast(app, ToastLevel::Error, "Bluetooth Off",
        "Bluetooth is unavailable — turn it on to connect.");
    let do_emit = {
        let s = app.app_state();
        let mut g = s.lock_or_recover();
        let idle = matches!(g.status.state.as_str(), "disconnected" | "scanning");
        if idle {
            g.status.state      = "bt_off".into();
            g.status.device_error   = Some(
                "Bluetooth is off — turn it on to connect to your BCI device.".into()
            );
            g.pending_reconnect = false;
            true
        } else { false }
    };
    if do_emit { refresh_tray(app); emit_status(app); }
}

/// Clear the `bt_off` state, trigger auto-reconnect, and (re)start scanning.
async fn scanner_bt_on(
    app:      &AppHandle,
    emitted:  &mut bool,
    scanning: &mut bool,
    adapter:  &BtPlatformAdapter,
) {
    if !*emitted { return; }
    *emitted = false;
    app_log!(app, "scanner", "[ble] bluetooth on");
    device_log("ble", "Bluetooth restored");
    send_toast(app, ToastLevel::Info, "Bluetooth Restored",
        "Bluetooth is back — reconnecting…");

    let (do_emit, preferred_id) = {
        let s = app.app_state();
        let mut g = s.lock_or_recover();
        if g.status.state == "bt_off" {
            g.status.state      = "disconnected".into();
            g.status.device_error   = None;
            g.pending_reconnect = true;
            (true, g.preferred_id.clone())
        } else { (false, None) }
    };
    if do_emit {
        refresh_tray(app);
        emit_status(app);
        if preferred_id.is_some() {
            start_session(app, preferred_id);
        }
    }

    if !*scanning && adapter.start_scan(ScanFilter::default()).await.is_ok() {
        app_log!(app, "scanner", "[ble] scan started");
        *scanning = true;
    }
}

async fn run_ble_scanner(app: AppHandle, cancel: CancellationToken) {
    let mut bt_off_emitted = false;

    'outer: loop {
        if cancel.is_cancelled() { return; }

        // Acquire the BT adapter.
        let adapter = loop {
            if let Ok(mgr) = BtPlatformManager::new().await {
                match mgr.adapters().await {
                    Ok(mut v) if !v.is_empty() => break v.remove(0),
                    _ => {}
                }
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            }
        };

        let mut events = match adapter.events().await {
            Ok(s) => s,
            Err(e) => {
                app_log!(app, "scanner", "[ble] events() failed: {e}");
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                }
                continue 'outer;
            }
        };

        let mut scanning = false;
        match adapter.adapter_state().await.unwrap_or(CentralState::Unknown) {
            CentralState::PoweredOn => {
                scanner_bt_on(&app, &mut bt_off_emitted, &mut scanning, &adapter).await;
                if !scanning && adapter.start_scan(ScanFilter::default()).await.is_ok() {
                    app_log!(app, "scanner", "[ble] scan started");
                    scanning = true;
                }
            }
            _ => scanner_bt_off(&app, &mut bt_off_emitted),
        }

        let mut poll_tick = tokio::time::interval(Duration::from_secs(3));
        loop {
            tokio::select! {
                biased;

                _ = cancel.cancelled() => {
                    if scanning { let _ = adapter.stop_scan().await; }
                    app_log!(app, "scanner", "[ble] stopped");
                    return;
                }

                maybe_event = events.next() => {
                    let Some(event) = maybe_event else { continue 'outer; };
                    match event {
                        CentralEvent::StateUpdate(CentralState::PoweredOn) => {
                            scanner_bt_on(&app, &mut bt_off_emitted, &mut scanning, &adapter).await;
                        }
                        CentralEvent::StateUpdate(_) => {
                            if scanning {
                                let _ = adapter.stop_scan().await;
                                scanning = false;
                            }
                            scanner_bt_off(&app, &mut bt_off_emitted);
                        }
                        _ => {}
                    }
                }

                _ = poll_tick.tick(), if scanning => {
                    match adapter.peripherals().await {
                        Err(_) => { let _ = adapter.stop_scan().await; continue 'outer; }
                        Ok(peripherals) => {
                            for p in peripherals {
                                if let Ok(Some(props)) = p.properties().await {
                                    let name_lower = props.local_name.as_deref()
                                        .map(|n| n.to_lowercase());

                                    let name_match = name_lower.as_deref().map(|n| {
                                        skill_data::device::DeviceKind::from_name(Some(n))
                                            != skill_data::device::DeviceKind::Unknown
                                    }).unwrap_or(false);

                                    let uuid_match = props.services.contains(&MW75_SERVICE_UUID);

                                    if name_match || uuid_match {
                                        let id   = p.id().to_string();
                                        let rssi = props.rssi.unwrap_or(0);
                                        let display_name = props.local_name.as_deref()
                                            .unwrap_or(if uuid_match { "MW75 Neuro" } else { "Unknown" });
                                        upsert_discovered(&app, &id, display_name, rssi);
                                        let msg = format!("{display_name} id={id} rssi={rssi} dBm");
                                        app_log!(app, "scanner", "[ble] {msg}");
                                        device_log("ble", &msg);
                                        try_auto_connect(&app, &id, display_name);
                                    }
                                }
                            }
                            emit_devices(&app);
                        }
                    }
                }
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// USB serial scan backend  (OpenBCI Cyton / CytonDaisy dongles)
// ══════════════════════════════════════════════════════════════════════════════

/// Detect OpenBCI USB dongles by scanning serial ports.
///
/// The FTDI chip on the Cyton dongle typically reports as `FT231X` or
/// contains `usbserial` / `ttyUSB` in the path.  We accept any serial port
/// whose product string or path matches known OpenBCI identifiers.
fn detect_openbci_serial_ports() -> Vec<(String, String)> {
    let ports = match serialport::available_ports() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for port in ports {
        let name = port.port_name.clone();
        let lower = name.to_lowercase();

        let is_openbci = match &port.port_type {
            serialport::SerialPortType::UsbPort(usb) => {
                // OpenBCI Cyton dongle: FTDI FT231X (VID 0403, PID 6015)
                let vid_match = usb.vid == 0x0403 && usb.pid == 0x6015;
                let product_match = usb.product.as_deref()
                    .map(|p| {
                        let pl = p.to_lowercase();
                        pl.contains("ft231x") || pl.contains("openbci") || pl.contains("ftdi")
                    })
                    .unwrap_or(false);
                vid_match || product_match
            }
            _ => {
                // Fallback: accept ttyUSB / usbserial paths (Linux / macOS)
                lower.contains("ttyusb") || lower.contains("usbserial")
            }
        };

        if is_openbci {
            let display = format!("OpenBCI ({})", name);
            results.push((name, display));
        }
    }
    results
}

async fn run_usb_scanner(app: AppHandle, cancel: CancellationToken) {
    let mut poll_tick = tokio::time::interval(Duration::from_secs(5));
    let mut known_ports: std::collections::HashSet<String> = std::collections::HashSet::new();

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                app_log!(app, "scanner", "[usb] stopped");
                return;
            }
            _ = poll_tick.tick() => {
                let ports = tokio::task::spawn_blocking(detect_openbci_serial_ports)
                    .await
                    .unwrap_or_default();

                let mut changed = false;
                for (port_name, display_name) in &ports {
                    // Use "usb:<port>" as a stable device ID.
                    let id = format!("usb:{port_name}");
                    if known_ports.insert(id.clone()) {
                        let msg = format!("{display_name} port={port_name}");
                        app_log!(app, "scanner", "[usb] {msg}");
                        device_log("usb", &msg);
                        changed = true;
                    }
                    upsert_discovered(&app, &id, display_name, 0);
                    try_auto_connect(&app, &id, display_name);
                }

                // Remove stale entries for ports that disappeared.
                let current_ids: std::collections::HashSet<String> = ports.iter()
                    .map(|(p, _)| format!("usb:{p}"))
                    .collect();
                known_ports.retain(|id| current_ids.contains(id));

                if changed { emit_devices(&app); }
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Emotiv Cortex scan backend
// ══════════════════════════════════════════════════════════════════════════════

/// Poll the local Emotiv Cortex service (`wss://localhost:6868`) for available
/// headsets.  The service is only present when the EMOTIV Launcher is running.
///
/// This task is lightweight: it attempts a quick WebSocket handshake every 10 s.
/// If the service is unreachable, it silently retries.  When headsets are found,
/// each one appears as a separate discovered device (`cortex:<headset_id>`) so
/// the user can choose which one to connect to.
async fn run_cortex_scanner(app: AppHandle, cancel: CancellationToken) {
    use skill_devices::emotiv::prelude::*;

    let mut poll_tick = tokio::time::interval(Duration::from_secs(10));
    // Track which headset IDs we've already logged to avoid spam.
    let mut known_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Skip the session_active guard on the very first tick so the scanner
    // discovers headsets before the startup auto-connect (at +900 ms) fires.
    let mut first_tick = true;

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                app_log!(app, "scanner", "[cortex] stopped");
                return;
            }
            _ = poll_tick.tick() => {
                // Skip the Cortex API probe while a session is active,
                // connecting, or retrying.  Each `authorize` call returns a
                // new cortex token that **invalidates** the previous one for
                // the same client_id.  Probing from the scanner while a
                // session is running would kill the session's token and stop
                // EEG data flow.
                //
                // Exception: the very first tick runs unconditionally so that
                // headsets are discovered before the startup auto-connect fires.
                // Previously discovered headsets remain in the `discovered`
                // list (kept by `known_ids` / `upsert_discovered`) so the
                // UI still shows them even when polling is paused.
                if !first_tick {
                    let session_active = {
                        let r = app.app_state();
                        let g = r.lock_or_recover();
                        g.stream.is_some()
                            || g.pending_reconnect
                            || !matches!(g.status.state.as_str(), "disconnected" | "bt_off")
                    };
                    if session_active {
                        continue;
                    }
                }
                first_tick = false;

                // Only poll if Emotiv credentials are configured.
                let (client_id, client_secret) = {
                    let r = app.app_state();
                    let s = r.lock_or_recover();
                    let cfg = &s.device_api_config;
                    let cid = if cfg.emotiv_client_id.trim().is_empty() {
                        std::env::var("EMOTIV_CLIENT_ID").unwrap_or_default()
                    } else {
                        cfg.emotiv_client_id.clone()
                    };
                    let csec = if cfg.emotiv_client_secret.trim().is_empty() {
                        std::env::var("EMOTIV_CLIENT_SECRET").unwrap_or_default()
                    } else {
                        cfg.emotiv_client_secret.clone()
                    };
                    (cid, csec)
                };
                if client_id.is_empty() || client_secret.is_empty() {
                    continue; // No credentials — skip this poll.
                }

                // Probe the Cortex service with auto_create_session disabled
                // so queryHeadsets returns the headset list without triggering
                // connect_headset / create_session side effects.
                let config = CortexClientConfig {
                    client_id,
                    client_secret,
                    auto_create_session: false,
                    ..Default::default()
                };
                let client = CortexClient::new(config);

                let result = tokio::time::timeout(
                    Duration::from_secs(12),
                    cortex_probe_headsets(&client),
                ).await;

                let headsets = match result {
                    Ok(Ok(list)) => list,
                    _ => continue, // Launcher not running or auth/query failed.
                };

                if headsets.is_empty() {
                    // Launcher is running but no headsets are paired/visible.
                    // Register a generic fallback so the user sees "Emotiv" in
                    // the device list and can still attempt a connection.
                    let id = "cortex:emotiv".to_owned();
                    let display_name = "Emotiv (Cortex)";
                    if known_ids.insert(id.clone()) {
                        let msg = "Emotiv Cortex reachable, no headsets paired";
                        app_log!(app, "scanner", "[cortex] {msg}");
                        device_log("cortex", msg);
                    }
                    upsert_discovered(&app, &id, display_name, 0);
                    emit_devices(&app);
                    continue;
                }

                // Phase 1: register ALL headsets as discovered devices so the
                // UI shows the complete list before any auto-connect fires.
                for hs in &headsets {
                    let id = format!("cortex:{}", hs.id);
                    let display_name = &hs.id; // e.g. "EPOCX-A1B2C3D4"
                    if known_ids.insert(id.clone()) {
                        let msg = format!("{} status={}", hs.id, hs.status);
                        app_log!(app, "scanner", "[cortex] {msg}");
                        device_log("cortex", &msg);
                    }
                    upsert_discovered(&app, &id, display_name, 0);
                }
                // Emit the device list so the frontend sees all headsets.
                // We emit on every poll (not just first discovery) because
                // upsert_discovered updates last_seen timestamps.
                emit_devices(&app);

                // Phase 2: attempt auto-connect for any paired headset.
                // (session_active was already checked above — we only reach
                // here when idle.)
                for hs in &headsets {
                    let id = format!("cortex:{}", hs.id);
                    try_auto_connect(&app, &id, &hs.id);
                }
            }
        }
    }
}

/// Probe the Cortex service: authorize, then query available headsets.
///
/// Uses `auto_create_session: false` so the crate does NOT automatically
/// send `queryHeadsets` after authorization.  We call `query_headsets()`
/// manually and receive the list via the `HeadsetsQueried` event without
/// triggering `connect_headset` / `create_session`.
async fn cortex_probe_headsets(
    client: &skill_devices::emotiv::client::CortexClient,
) -> Result<Vec<skill_devices::emotiv::types::HeadsetInfo>, String> {
    let (mut rx, handle) = client.connect().await.map_err(|e| e.to_string())?;

    use skill_devices::emotiv::types::CortexEvent;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

    // Phase 1: wait for Authorized.
    loop {
        let ev = tokio::time::timeout_at(deadline, rx.recv()).await;
        match ev {
            Ok(Some(CortexEvent::Authorized)) => break,
            Ok(Some(CortexEvent::Error(e))) => return Err(e),
            Ok(None) => return Err("Channel closed before authorized".into()),
            Err(_) => return Err("Timed out waiting for authorization".into()),
            _ => continue,
        }
    }

    // Phase 2: send queryHeadsets and wait for HeadsetsQueried.
    handle.query_headsets().await.map_err(|e| e.to_string())?;

    loop {
        let ev = tokio::time::timeout_at(deadline, rx.recv()).await;
        match ev {
            Ok(Some(CortexEvent::HeadsetsQueried(list))) => return Ok(list),
            Ok(Some(CortexEvent::Error(e))) => return Err(e),
            Ok(None) => return Err("Channel closed before headset query".into()),
            Err(_) => return Err("Timed out waiting for headset query".into()),
            _ => continue,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Orchestrator
// ══════════════════════════════════════════════════════════════════════════════

/// Run all scanner backends until cancellation.
///
/// Each backend is started only if the corresponding toggle is enabled in
/// the user's `ScannerConfig`.
async fn run_device_scanner(app: AppHandle, stop_rx: tokio::sync::oneshot::Receiver<()>) {
    let cancel = CancellationToken::new();

    // Convert the oneshot into a cancellation.
    let cancel2 = cancel.clone();
    tokio::spawn(async move {
        let _ = stop_rx.await;
        cancel2.cancel();
    });

    // Read scanner config once at startup.
    let cfg = {
        let r = app.app_state();
        let g = r.lock_or_recover();
        g.scanner_config.clone()
    };

    let mut tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    if cfg.ble {
        device_log("scanner", "BLE backend enabled");
        tasks.push(tokio::spawn(run_ble_scanner(app.clone(), cancel.clone())));
    } else {
        device_log("scanner", "BLE backend disabled by settings");
    }
    if cfg.usb_serial {
        device_log("scanner", "USB serial backend enabled");
        tasks.push(tokio::spawn(run_usb_scanner(app.clone(), cancel.clone())));
    } else {
        device_log("scanner", "USB serial backend disabled by settings");
    }
    if cfg.cortex {
        device_log("scanner", "Cortex backend enabled");
        tasks.push(tokio::spawn(run_cortex_scanner(app.clone(), cancel.clone())));
    } else {
        device_log("scanner", "Cortex backend disabled by settings");
    }

    futures_util::future::join_all(tasks).await;
}

/// Stop the background device scanner if it is running.
#[allow(dead_code)]
pub(crate) fn stop_background_scanner(app: &AppHandle) {
    let s_ref = app.app_state();
    let tx = s_ref.lock_or_recover().scanner.take().map(|sh| sh.cancel_tx);
    if let Some(tx) = tx {
        let _ = tx.send(());
        app_log!(app, "scanner", "background scanner stopped");
    }
}

/// Start the background device scanner if it is not already running.
/// Idempotent — safe to call multiple times.
pub(crate) fn start_background_scanner(app: &AppHandle) {
    let s_ref = app.app_state();
    let already = { let g = s_ref.lock_or_recover(); g.scanner.is_some() };
    if already { return; }
    let (tx, rx) = tokio::sync::oneshot::channel();
    s_ref.lock_or_recover().scanner = Some(ScannerHandle { cancel_tx: tx });
    let clone = app.clone();
    tauri::async_runtime::spawn(async move { run_device_scanner(clone, rx).await; });
}
