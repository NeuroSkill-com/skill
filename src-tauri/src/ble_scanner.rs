// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Background BLE scanner (runs independently of device sessions) and
// Bluetooth availability helpers.

use std::{sync::Mutex, time::Duration};

use btleplug::api::{
    Central, CentralEvent, CentralState, Manager as BtManager,
    Peripheral as BtPeripheral, ScanFilter,
};
use btleplug::platform::{Adapter as BtPlatformAdapter, Manager as BtPlatformManager};
use futures_util::StreamExt;
use tauri::{AppHandle, Manager};

use crate::{
    AppState, MutexExt, ScannerHandle,
    emit_devices, emit_status, refresh_tray, send_toast, start_session, upsert_discovered,
    ToastLevel,
};

// ── Bluetooth availability ────────────────────────────────────────────────────

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

// ── Background scanner ────────────────────────────────────────────────────────

/// Emit the `bt_off` UI state once per outage (edge-triggered via `emitted` flag).
fn scanner_bt_off(app: &AppHandle, emitted: &mut bool) {
    if *emitted { return; }
    *emitted = true;
    app_log!(app, "bluetooth", "off");
    send_toast(app, ToastLevel::Error, "Bluetooth Off",
        "Bluetooth is unavailable — turn it on to connect.");
    let do_emit = {
        let s = app.state::<Mutex<Box<AppState>>>();
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
/// No-ops if `emitted` is `false` (BT was never off during this run).
async fn scanner_bt_on(
    app:      &AppHandle,
    emitted:  &mut bool,
    scanning: &mut bool,
    adapter:  &BtPlatformAdapter,
) {
    if !*emitted { return; }
    *emitted = false;
    app_log!(app, "bluetooth", "on");
    send_toast(app, ToastLevel::Info, "Bluetooth Restored",
        "Bluetooth is back — reconnecting…");

    let (do_emit, preferred_id) = {
        let s = app.state::<Mutex<Box<AppState>>>();
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
        app_log!(app, "bluetooth", "[scanner] BLE scan started");
        *scanning = true;
    }
}

async fn run_background_scanner(app: AppHandle, stop_rx: tokio::sync::oneshot::Receiver<()>) {
    tokio::pin!(stop_rx);
    let mut bt_off_emitted = false;

    'outer: loop {
        // Acquire the BT adapter.  On macOS (CoreBluetooth) this always
        // succeeds; on Linux (BlueZ) `bluetoothd` must be running.
        let adapter = loop {
            if let Ok(mgr) = BtPlatformManager::new().await {
                match mgr.adapters().await {
                    Ok(mut v) if !v.is_empty() => break v.remove(0),
                    _ => {}
                }
            }
            tokio::select! {
                biased;
                _ = &mut stop_rx => return,
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            }
        };

        let mut events = match adapter.events().await {
            Ok(s) => s,
            Err(e) => {
                app_log!(app, "bluetooth", "[scanner] events() failed: {e}");
                tokio::select! {
                    biased;
                    _ = &mut stop_rx => return,
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
                    app_log!(app, "bluetooth", "[scanner] BLE scan started");
                    scanning = true;
                }
            }
            _ => scanner_bt_off(&app, &mut bt_off_emitted),
        }

        let mut poll_tick = tokio::time::interval(Duration::from_secs(3));
        loop {
            tokio::select! {
                biased;

                _ = &mut stop_rx => {
                    if scanning { let _ = adapter.stop_scan().await; }
                    app_log!(app, "bluetooth", "[scanner] stopped");
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
                                    if let Some(ref name) = props.local_name {
                                        let n = name.to_lowercase();
                                        let is_known = n.starts_with("muse")
                                            || n.starts_with("ganglion")
                                            || n.starts_with("simblee")
                                            || n.contains("mw75");
                                        if is_known {
                                            let id   = p.id().to_string();
                                            let rssi = props.rssi.unwrap_or(0);
                                            upsert_discovered(&app, &id, name, rssi);
                                            app_log!(app, "bluetooth",
                                                "[scanner] {name} id={id} rssi={rssi} dBm"
                                            );
                                        }
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

/// Start the background BLE scanner if it is not already running.
/// Idempotent — safe to call multiple times.
pub(crate) fn start_background_scanner(app: &AppHandle) {
    let s_ref = app.state::<Mutex<Box<AppState>>>();
    let already = { let g = s_ref.lock_or_recover(); g.scanner.is_some() };
    if already { return; }
    let (tx, rx) = tokio::sync::oneshot::channel();
    s_ref.lock_or_recover().scanner = Some(ScannerHandle { cancel_tx: tx });
    let clone = app.clone();
    tauri::async_runtime::spawn(async move { run_background_scanner(clone, rx).await; });
}
