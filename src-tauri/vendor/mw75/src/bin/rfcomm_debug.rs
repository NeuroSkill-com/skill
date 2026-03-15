//! Diagnostic tool for debugging macOS RFCOMM connection to MW75 Neuro.
//!
//! Usage: cargo run --bin rfcomm-debug --features rfcomm
//!
//! This binary runs a series of diagnostic probes against the MW75 headphones:
//!
//! 1. BLE activation (enable EEG + raw mode)
//! 2. Enumerate all paired/recent IOBluetooth devices
//! 3. SDP service record enumeration (list all RFCOMM channels)
//! 4. Try opening RFCOMM on discovered channels
//! 5. Try alternative connection strategies (keep BLE alive, etc.)

use std::sync::Arc;

use anyhow::Result;
use log::info;

use mw75::mw75_client::{Mw75Client, Mw75ClientConfig};
use mw75::types::Mw75Event;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = Mw75ClientConfig {
        scan_timeout_secs: 10,
        name_pattern: "MW75".into(),
    };
    let client = Mw75Client::new(config);

    // ── Phase 1: BLE activation ───────────────────────────────────────────────
    info!("═══════════════════════════════════════════════════════════════");
    info!("Phase 1: BLE activation");
    info!("═══════════════════════════════════════════════════════════════");

    let (mut rx, handle) = client.connect().await?;
    let handle = Arc::new(handle);
    handle.start().await?;

    // Drain events
    while let Ok(event) = rx.try_recv() {
        match event {
            Mw75Event::Connected(name) => info!("  Connected: {name}"),
            Mw75Event::Battery(b) => info!("  Battery: {}%", b.level),
            Mw75Event::Activated(s) => info!("  Activated: EEG={}, Raw={}", s.eeg_enabled, s.raw_mode_enabled),
            _ => {}
        }
    }

    let device_name = handle.device_name().to_string();
    let peripheral_id = handle.peripheral_id();
    info!("  Device name: {device_name}");
    info!("  Peripheral ID (CoreBluetooth UUID): {peripheral_id}");

    // ── Phase 2: IOBluetooth diagnostics (on a dedicated thread) ──────────────
    info!("");
    info!("═══════════════════════════════════════════════════════════════");
    info!("Phase 2: IOBluetooth device enumeration & SDP probe");
    info!("═══════════════════════════════════════════════════════════════");

    // Strategy A: Try RFCOMM while BLE is still connected
    info!("");
    info!("─── Strategy A: RFCOMM with BLE still connected ───");
    let name_a = device_name.clone();
    let result_a = tokio::task::spawn_blocking(move || {
        run_rfcomm_diagnostics(&name_a, "A (BLE connected)")
    }).await?;
    info!("Strategy A result: {result_a:?}");

    // Strategy B: Disconnect BLE first, then try RFCOMM
    info!("");
    info!("─── Strategy B: disconnect BLE, then RFCOMM ───");
    handle.disconnect_ble().await.ok();

    // Various settle times
    for settle_ms in [500, 2000, 5000] {
        info!("");
        info!("  Settle time: {settle_ms} ms …");
        tokio::time::sleep(std::time::Duration::from_millis(settle_ms)).await;

        let name_b = device_name.clone();
        let label = format!("B (BLE disconnected, settle={settle_ms}ms)");
        let result_b = tokio::task::spawn_blocking(move || {
            run_rfcomm_diagnostics(&name_b, &label)
        }).await?;
        info!("  Result: {result_b:?}");

        if result_b.is_ok() {
            info!("  ✅ SUCCESS — RFCOMM connected!");
            break;
        }
    }

    info!("");
    info!("═══════════════════════════════════════════════════════════════");
    info!("Diagnostics complete.");
    info!("═══════════════════════════════════════════════════════════════");

    Ok(())
}

#[cfg(target_os = "macos")]
fn run_rfcomm_diagnostics(device_name: &str, label: &str) -> Result<String> {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, ClassType};
    use objc2_foundation::{NSArray, NSDate, NSRunLoop, NSString};
    use objc2_io_bluetooth::IOBluetoothDevice;

    info!("[{label}] Enumerating IOBluetooth devices …");

    // ── List ALL paired devices ───────────────────────────────────────────────
    let mut all_devices: Vec<(String, String, bool)> = Vec::new();

    unsafe {
        let paired: Option<Retained<NSArray<IOBluetoothDevice>>> =
            msg_send![IOBluetoothDevice::class(), pairedDevices];
        if let Some(ref devices) = paired {
            let count: usize = msg_send![devices, count];
            info!("[{label}] Paired devices: {count}");
            for i in 0..count {
                let dev: Retained<IOBluetoothDevice> = msg_send![devices, objectAtIndex: i];
                let name_ptr: *const NSString = msg_send![&*dev, name];
                let addr_ptr: *const NSString = msg_send![&*dev, addressString];
                let connected: bool = msg_send![&*dev, isConnected];
                let name = if !name_ptr.is_null() { (*name_ptr).to_string() } else { "<null>".into() };
                let addr = if !addr_ptr.is_null() { (*addr_ptr).to_string() } else { "<null>".into() };
                info!("[{label}]   [{i}] name={name:30} addr={addr}  connected={connected}");
                all_devices.push((name, addr, connected));
            }
        } else {
            info!("[{label}] No paired devices returned");
        }

        let recent: Option<Retained<NSArray<IOBluetoothDevice>>> =
            msg_send![IOBluetoothDevice::class(), recentDevices: 20usize];
        if let Some(ref devices) = recent {
            let count: usize = msg_send![devices, count];
            info!("[{label}] Recent devices: {count}");
            for i in 0..count {
                let dev: Retained<IOBluetoothDevice> = msg_send![devices, objectAtIndex: i];
                let name_ptr: *const NSString = msg_send![&*dev, name];
                let addr_ptr: *const NSString = msg_send![&*dev, addressString];
                let connected: bool = msg_send![&*dev, isConnected];
                let name = if !name_ptr.is_null() { (*name_ptr).to_string() } else { "<null>".into() };
                let addr = if !addr_ptr.is_null() { (*addr_ptr).to_string() } else { "<null>".into() };
                info!("[{label}]   [{i}] name={name:30} addr={addr}  connected={connected}");
            }
        }
    }

    // ── Find the MW75 device ──────────────────────────────────────────────────
    let device: Retained<IOBluetoothDevice> = unsafe {
        let mut found: Option<Retained<IOBluetoothDevice>> = None;
        let paired: Option<Retained<NSArray<IOBluetoothDevice>>> =
            msg_send![IOBluetoothDevice::class(), pairedDevices];
        if let Some(ref devices) = paired {
            let count: usize = msg_send![devices, count];
            for i in 0..count {
                let dev: Retained<IOBluetoothDevice> = msg_send![devices, objectAtIndex: i];
                let name_ptr: *const NSString = msg_send![&*dev, name];
                if !name_ptr.is_null() {
                    let name_str = (*name_ptr).to_string();
                    if name_str == device_name {
                        found = Some(dev);
                        break;
                    }
                }
            }
        }
        match found {
            Some(d) => d,
            None => return Err(anyhow::anyhow!("Device '{device_name}' not found in paired devices")),
        }
    };

    let runloop = NSRunLoop::currentRunLoop();

    // ── Check connection state ────────────────────────────────────────────────
    let is_connected: bool = unsafe { msg_send![&*device, isConnected] };
    info!("[{label}] Device isConnected = {is_connected}");

    // ── Try openConnection ────────────────────────────────────────────────────
    if !is_connected {
        info!("[{label}] Calling openConnection …");
        let result: i32 = unsafe { msg_send![&*device, openConnection] };
        info!("[{label}] openConnection returned: 0x{result:08x} ({})",
            if result == 0 { "success" } else { "error" });

        // Pump run loop up to 8 seconds, checking isConnected
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        loop {
            let date = NSDate::dateWithTimeIntervalSinceNow(0.25);
            runloop.runUntilDate(&date);

            let connected: bool = unsafe { msg_send![&*device, isConnected] };
            if connected {
                info!("[{label}] Baseband connected after {} ms",
                    (std::time::Instant::now() - (deadline - std::time::Duration::from_secs(8))).as_millis());
                break;
            }
            if std::time::Instant::now() >= deadline {
                info!("[{label}] ⚠ Baseband connection timeout (8 s) — isConnected still false");
                break;
            }
        }
    }

    let is_connected: bool = unsafe { msg_send![&*device, isConnected] };
    info!("[{label}] After openConnection: isConnected = {is_connected}");

    // ── SDP query ─────────────────────────────────────────────────────────────
    info!("[{label}] Performing SDP query …");
    let sdp_result: i32 = unsafe {
        msg_send![&*device, performSDPQuery: std::ptr::null::<AnyObject>()]
    };
    info!("[{label}] performSDPQuery returned: 0x{sdp_result:08x}");

    // Pump run loop for SDP to complete
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let date = NSDate::dateWithTimeIntervalSinceNow(0.25);
        runloop.runUntilDate(&date);

        let services: *const AnyObject = unsafe { msg_send![&*device, services] };
        if !services.is_null() {
            let count: usize = unsafe { msg_send![services, count] };
            if count > 0 {
                info!("[{label}] SDP returned {count} service record(s)");
                break;
            }
        }
        if std::time::Instant::now() >= deadline {
            info!("[{label}] ⚠ SDP query timeout (8 s)");
            break;
        }
    }

    // ── Enumerate SDP service records ─────────────────────────────────────────
    let mut rfcomm_channels: Vec<u8> = Vec::new();

    unsafe {
        let services: *const AnyObject = msg_send![&*device, services];
        if !services.is_null() {
            let count: usize = msg_send![services, count];
            info!("[{label}] ── SDP Service Records ({count}) ──");
            for i in 0..count {
                let record: *const AnyObject = msg_send![services, objectAtIndex: i];

                // Get service name
                // IOBluetoothSDPServiceRecord getServiceName
                let svc_name: *const NSString = msg_send![record, getServiceName];
                let name = if !svc_name.is_null() {
                    (*svc_name).to_string()
                } else {
                    "<unnamed>".into()
                };

                // Try to get RFCOMM channel
                // getRFCOMMChannelID: takes a pointer to u8
                let mut ch_id: u8 = 0;
                let ch_result: i32 = msg_send![record, getRFCOMMChannelID: &mut ch_id as *mut u8];

                if ch_result == 0 {
                    info!("[{label}]   [{i}] {name:40}  RFCOMM channel = {ch_id}");
                    rfcomm_channels.push(ch_id);
                } else {
                    // Try L2CAP PSM
                    let mut psm: u16 = 0;
                    let psm_result: i32 = msg_send![record, getL2CAPPSM: &mut psm as *mut u16];
                    if psm_result == 0 {
                        info!("[{label}]   [{i}] {name:40}  L2CAP PSM = {psm}");
                    } else {
                        info!("[{label}]   [{i}] {name:40}  (no RFCOMM/L2CAP)");
                    }
                }
            }
        } else {
            info!("[{label}] No SDP service records available");
        }
    }

    info!("[{label}] Available RFCOMM channels: {rfcomm_channels:?}");

    if rfcomm_channels.is_empty() {
        return Err(anyhow::anyhow!("No RFCOMM channels found in SDP records"));
    }

    // ── Try opening each RFCOMM channel ───────────────────────────────────────
    // Try channel 25 first (if present), then all others
    if !rfcomm_channels.contains(&25) {
        info!("[{label}] ⚠ Channel 25 NOT in SDP records! Will try discovered channels.");
    }

    // Put channel 25 first if it exists, then the rest
    let mut try_channels = Vec::new();
    if rfcomm_channels.contains(&25) {
        try_channels.push(25u8);
    }
    for &ch in &rfcomm_channels {
        if ch != 25 {
            try_channels.push(ch);
        }
    }

    for &channel in &try_channels {
        info!("[{label}] ── Trying RFCOMM channel {channel} ──");

        // Re-check connection state before each attempt
        let is_conn: bool = unsafe { msg_send![&*device, isConnected] };
        info!("[{label}]   isConnected = {is_conn}");

        let mut channel_ptr: *mut AnyObject = std::ptr::null_mut();
        let result: i32 = unsafe {
            msg_send![
                &*device,
                openRFCOMMChannelSync: &mut channel_ptr,
                withChannelID: channel,
                delegate: std::ptr::null::<AnyObject>()
            ]
        };

        if result == 0 && !channel_ptr.is_null() {
            let is_open: bool = unsafe { msg_send![channel_ptr, isOpen] };
            let mtu: u16 = unsafe { msg_send![channel_ptr, getMTU] };
            info!("[{label}]   ✅ Channel {channel} OPENED! isOpen={is_open} MTU={mtu}");

            // Close it
            let _: () = unsafe { msg_send![channel_ptr, closeChannel] };
            info!("[{label}]   Channel {channel} closed.");

            return Ok(format!("Channel {channel} opened successfully"));
        } else {
            let error_name = match result as u32 {
                0xe00002bc => "kIOReturnNotPermitted",
                0xe00002c0 => "kIOReturnNotReady",
                0xe00002c2 => "kIOReturnNoDevice",
                0xe00002d8 => "kIOReturnTimeout",
                0xe00002eb => "kIOReturnNotOpen",
                _ => "unknown",
            };
            info!("[{label}]   ❌ Channel {channel} failed: 0x{result:08x} ({error_name})");
        }

        // Pump runloop between attempts
        let date = NSDate::dateWithTimeIntervalSinceNow(1.0);
        runloop.runUntilDate(&date);
    }

    // ── Try openRFCOMMChannelAsync as alternative ─────────────────────────────
    info!("[{label}] ── Trying openRFCOMMChannelAsync (channel 25) ──");
    unsafe {
        let mut channel_ptr: *mut AnyObject = std::ptr::null_mut();
        let result: i32 = msg_send![
            &*device,
            openRFCOMMChannelAsync: &mut channel_ptr,
            withChannelID: 25u8,
            delegate: std::ptr::null::<AnyObject>()
        ];
        info!("[{label}]   openRFCOMMChannelAsync returned: 0x{result:08x}");

        if result == 0 {
            // Pump run loop to let the async open complete
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                let date = NSDate::dateWithTimeIntervalSinceNow(0.25);
                runloop.runUntilDate(&date);

                if !channel_ptr.is_null() {
                    let is_open: bool = msg_send![channel_ptr, isOpen];
                    if is_open {
                        info!("[{label}]   ✅ Async channel 25 opened!");
                        let _: () = msg_send![channel_ptr, closeChannel];
                        return Ok("Async channel 25 opened".into());
                    }
                }
                if std::time::Instant::now() >= deadline {
                    info!("[{label}]   ⚠ Async open timeout (5 s)");
                    break;
                }
            }
        }
    }

    Err(anyhow::anyhow!("All RFCOMM channel open attempts failed"))
}

#[cfg(not(target_os = "macos"))]
fn run_rfcomm_diagnostics(_device_name: &str, label: &str) -> Result<String> {
    info!("[{label}] RFCOMM diagnostics only supported on macOS");
    Ok("skipped (not macOS)".into())
}
