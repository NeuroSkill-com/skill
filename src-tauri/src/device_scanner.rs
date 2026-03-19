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
pub(crate) fn classify_bt_error(raw: &str) -> (String, bool) {
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
        .map_err(|e| classify_bt_error(&e.to_string()))?;
    let adapters = mgr.adapters().await
        .map_err(|e| classify_bt_error(&e.to_string()))?;
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
    // Cortex and USB devices are discovered via trusted transports —
    // auto-connect even if not explicitly paired.
    let trusted_transport = id.starts_with("cortex:") || id.starts_with("usb:");
    let should_auto = {
        let r = app.app_state();
        let g = r.lock_or_recover();
        let is_idle = g.stream.is_none()
            && !g.pending_reconnect
            && matches!(g.status.state.as_str(), "disconnected");
        let is_paired = g.status.paired_devices.iter().any(|d| d.id == id);
        is_idle && (is_paired || trusted_transport)
    };
    if should_auto {
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
            g.status.bt_error   = Some(
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
            g.status.bt_error   = None;
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
/// they appear in the discovered list and can be auto-connected.
async fn run_cortex_scanner(app: AppHandle, cancel: CancellationToken) {
    use skill_devices::emotiv::prelude::*;

    let mut poll_tick = tokio::time::interval(Duration::from_secs(10));
    // Track which headset IDs we've already logged to avoid spam.
    let mut known_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                app_log!(app, "scanner", "[cortex] stopped");
                return;
            }
            _ = poll_tick.tick() => {
                // Skip polling while a session is already active — creating
                // a second Cortex WS connection would interfere with the
                // running session (the Cortex service stops streams on the
                // first session when a second is created).
                {
                    let r = app.app_state();
                    let g = r.lock_or_recover();
                    if g.stream.is_some() || g.pending_reconnect {
                        continue;
                    }
                }

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

                // Probe the Cortex service — authorize + getCortexInfo only.
                // We NEVER send queryHeadsets from the scanner because the
                // emotiv crate's response handler for that call automatically
                // triggers connect_headset / create_session, which would
                // interfere with any active session.
                let config = CortexClientConfig {
                    client_id,
                    client_secret,
                    auto_create_session: false,
                    ..Default::default()
                };
                let client = CortexClient::new(config);

                let result = tokio::time::timeout(
                    Duration::from_secs(8),
                    cortex_probe(&client),
                ).await;

                match result {
                    Ok(Ok(true)) => {} // Authorized — Launcher is running.
                    _ => continue,      // Launcher not running or auth failed.
                }

                // The Cortex service is up and we're authorized.
                // Use a stable ID so the discovered entry persists across polls.
                let id = "cortex:emotiv".to_owned();
                let display_name = "Emotiv (Cortex)";

                if known_ids.insert(id.clone()) {
                    let msg = "Emotiv Cortex service reachable";
                    app_log!(app, "scanner", "[cortex] {msg}");
                    device_log("cortex", msg);
                    emit_devices(&app);
                }
                upsert_discovered(&app, &id, display_name, 0);
                try_auto_connect(&app, &id, display_name);
            }
        }
    }
}

/// Probe whether the Cortex service is reachable and we can authorize.
///
/// We intentionally do NOT send `queryHeadsets` or `getCortexInfo` — the
/// emotiv crate's internal handler for `queryHeadsets` automatically calls
/// `connect_headset` / `create_session`, which would interfere with any
/// active session.
///
/// A successful authorization proves the EMOTIV Launcher is running and
/// credentials are valid.  `connect_emotiv` will do the real headset
/// discovery and session creation when a connection is actually initiated.
async fn cortex_probe(
    client: &skill_devices::emotiv::client::CortexClient,
) -> Result<bool, String> {
    let (mut rx, _handle) = client.connect().await.map_err(|e| e.to_string())?;

    use skill_devices::emotiv::types::CortexEvent;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);

    while tokio::time::Instant::now() < deadline {
        let ev = tokio::time::timeout_at(deadline, rx.recv()).await;
        match ev {
            Ok(Some(CortexEvent::Authorized)) => return Ok(true),
            Ok(Some(CortexEvent::Error(e))) => return Err(e),
            Ok(None) => break,
            Err(_) => break,
            _ => {}
        }
    }

    Err("Not authorized".into())
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
