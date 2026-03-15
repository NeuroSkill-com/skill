//! RFCOMM data probe for MW75 Neuro — connects via RFCOMM and reads EEG data.
//!
//! Usage: cargo run --bin rfcomm-probe --features rfcomm
//!
//! Tries multiple connection strategies:
//!   A) RFCOMM while BLE is still connected
//!   B) RFCOMM after BLE disconnect (various settle times)
//!   C) RFCOMM via openConnection with target delegate

use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use btleplug::api::{
    Central, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use log::{info, warn};
use uuid::Uuid;

const CMD_1101: Uuid = Uuid::from_u128(0x00001101_d102_11e1_9b23_00025b00a5a5);
const STATUS_1102: Uuid = Uuid::from_u128(0x00001102_d102_11e1_9b23_00025b00a5a5);

const ENABLE_EEG: [u8; 5] = [0x09, 0x9A, 0x03, 0x60, 0x01];
const ENABLE_RAW: [u8; 5] = [0x09, 0x9A, 0x03, 0x41, 0x01];

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

async fn find_mw75(adapter: &Adapter) -> Result<Peripheral> {
    adapter.start_scan(ScanFilter::default()).await?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        for p in adapter.peripherals().await? {
            if let Ok(Some(props)) = p.properties().await {
                let name = props.local_name.unwrap_or_default();
                if name.to_uppercase().contains("MW75") {
                    info!("Found: {name} ({})", p.id());
                    adapter.stop_scan().await.ok();
                    return Ok(p);
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(anyhow!("Timeout scanning for MW75"));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("═══════════════════════════════════════════════════════════════");
    info!("MW75 RFCOMM Data Probe v2");
    info!("═══════════════════════════════════════════════════════════════");

    let manager = Manager::new().await?;
    let adapter = manager.adapters().await?.into_iter().next()
        .ok_or_else(|| anyhow!("No Bluetooth adapter"))?;

    #[cfg(target_os = "macos")]
    {
        use btleplug::api::CentralState;
        let dl = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            match adapter.adapter_state().await {
                Ok(CentralState::PoweredOn) => { info!("Adapter: PoweredOn"); break; }
                Ok(s) if tokio::time::Instant::now() >= dl => { warn!("Adapter: {s:?}"); break; }
                Err(e) => { warn!("Adapter error: {e}"); break; }
                _ => {}
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    let peripheral = find_mw75(&adapter).await?;

    // ── Phase 1: BLE activation ───────────────────────────────────────────────
    info!("\n═══ Phase 1: BLE Activation ═══");
    peripheral.connect().await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    peripheral.discover_services().await?;
    info!("Connected.");

    let chars = peripheral.characteristics();
    let cmd_char = chars.iter().find(|c| c.uuid == CMD_1101)
        .ok_or_else(|| anyhow!("CMD_1101 not found"))?;
    let status_char = chars.iter().find(|c| c.uuid == STATUS_1102)
        .ok_or_else(|| anyhow!("STATUS_1102 not found"))?;

    peripheral.subscribe(status_char).await?;
    let mut notifications = peripheral.notifications().await?;

    info!("Sending ENABLE_EEG…");
    peripheral.write(cmd_char, &ENABLE_EEG, WriteType::WithResponse).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    info!("Sending ENABLE_RAW…");
    peripheral.write(cmd_char, &ENABLE_RAW, WriteType::WithResponse).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Collect responses
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() { break; }
        match tokio::time::timeout(remaining, notifications.next()).await {
            Ok(Some(n)) => info!("  STATUS: {}", hex(&n.value)),
            _ => break,
        }
    }

    let props = peripheral.properties().await?.unwrap_or_default();
    let device_name = props.local_name.unwrap_or_else(|| "MW75 Neuro".to_string());
    info!("Device name: {device_name}");

    // ── Strategy A: RFCOMM while BLE is still connected ───────────────────────
    #[cfg(target_os = "macos")]
    {
        info!("\n═══ Strategy A: RFCOMM with BLE still connected ═══");
        let name = device_name.clone();
        let result = tokio::task::spawn_blocking(move || {
            try_rfcomm_macos(&name, "A-BLE-connected")
        }).await?;
        match result {
            Ok(channel_info) => {
                info!("  ✅ Strategy A worked: {channel_info}");
                // If we got a channel, run the data phase
                info!("\n═══ Phase 4: RFCOMM Data Stream ═══");
                let name2 = device_name.clone();
                rfcomm_data_phase_macos(&name2).await?;
                peripheral.disconnect().await.ok();
                info!("\n═══ DONE ═══");
                return Ok(());
            }
            Err(e) => info!("  ❌ Strategy A failed: {e}"),
        }

        // ── Strategy B: Disconnect BLE, try RFCOMM with various settle times ──
        info!("\n═══ Strategy B: Disconnect BLE first ═══");
        peripheral.disconnect().await?;
        info!("BLE disconnected.");
        drop(notifications);

        for settle_ms in [1000u64, 2000, 3000, 5000] {
            info!("\n  Settle {settle_ms}ms…");
            tokio::time::sleep(Duration::from_millis(settle_ms)).await;

            let name = device_name.clone();
            let label = format!("B-settle-{settle_ms}ms");
            let result = tokio::task::spawn_blocking(move || {
                try_rfcomm_macos(&name, &label)
            }).await?;
            match result {
                Ok(channel_info) => {
                    info!("  ✅ Strategy B ({settle_ms}ms) worked: {channel_info}");
                    info!("\n═══ Phase 4: RFCOMM Data Stream ═══");
                    let name2 = device_name.clone();
                    rfcomm_data_phase_macos(&name2).await?;
                    info!("\n═══ DONE ═══");
                    return Ok(());
                }
                Err(e) => info!("  ❌ Strategy B ({settle_ms}ms) failed: {e}"),
            }
        }

        // ── Strategy C: Re-activate BLE, then try RFCOMM with BLE alive ───────
        info!("\n═══ Strategy C: Re-activate BLE, keep alive, try RFCOMM ═══");
        let peripheral2 = find_mw75(&adapter).await?;
        peripheral2.connect().await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        peripheral2.discover_services().await?;
        let chars2 = peripheral2.characteristics();
        if let Some(cmd) = chars2.iter().find(|c| c.uuid == CMD_1101) {
            peripheral2.write(cmd, &ENABLE_EEG, WriteType::WithResponse).await?;
            tokio::time::sleep(Duration::from_millis(200)).await;
            peripheral2.write(cmd, &ENABLE_RAW, WriteType::WithResponse).await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            info!("  BLE re-activated, EEG+RAW enabled");
        }

        // Wait for RFCOMM status 0x88 0x01 (connected)
        info!("  Waiting 5s for device to be ready…");
        tokio::time::sleep(Duration::from_secs(5)).await;

        let name = device_name.clone();
        let result = tokio::task::spawn_blocking(move || {
            try_rfcomm_macos(&name, "C-BLE-reactivated")
        }).await?;
        match result {
            Ok(channel_info) => {
                info!("  ✅ Strategy C worked: {channel_info}");
                info!("\n═══ Phase 4: RFCOMM Data Stream ═══");
                let name2 = device_name.clone();
                rfcomm_data_phase_macos(&name2).await?;
            }
            Err(e) => info!("  ❌ Strategy C failed: {e}"),
        }

        peripheral2.disconnect().await.ok();
    }

    #[cfg(target_os = "linux")]
    {
        peripheral.disconnect().await?;
        tokio::time::sleep(Duration::from_millis(1500)).await;
        rfcomm_data_probe_linux(&device_name).await?;
    }

    info!("\n═══ DONE ═══");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── macOS: Try RFCOMM connection (returns success/failure) ────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
fn try_rfcomm_macos(device_name: &str, label: &str) -> Result<String> {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, ClassType};
    use objc2_foundation::{NSArray, NSDate, NSRunLoop, NSString};
    use objc2_io_bluetooth::IOBluetoothDevice;

    info!("[{label}] Looking up '{device_name}' …");

    let device: Retained<IOBluetoothDevice> = unsafe {
        let mut found: Option<Retained<IOBluetoothDevice>> = None;
        let paired: Option<Retained<NSArray<IOBluetoothDevice>>> =
            msg_send![IOBluetoothDevice::class(), pairedDevices];
        if let Some(ref devices) = paired {
            let count: usize = msg_send![devices, count];
            for i in 0..count {
                let dev: Retained<IOBluetoothDevice> = msg_send![devices, objectAtIndex: i];
                let name_ptr: *const NSString = msg_send![&*dev, name];
                if !name_ptr.is_null() && (*name_ptr).to_string() == device_name {
                    found = Some(dev);
                    break;
                }
            }
        }
        found.ok_or_else(|| anyhow!("Device not found in paired devices"))?
    };

    let runloop = NSRunLoop::currentRunLoop();

    // Check/establish baseband
    let is_connected: bool = unsafe { msg_send![&*device, isConnected] };
    info!("[{label}] isConnected = {is_connected}");

    if !is_connected {
        info!("[{label}] Opening baseband…");
        let r: i32 = unsafe { msg_send![&*device, openConnection] };
        info!("[{label}] openConnection = 0x{r:08x}");

        for i in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let date = NSDate::dateWithTimeIntervalSinceNow(0.05);
            runloop.runUntilDate(&date);
            let c: bool = unsafe { msg_send![&*device, isConnected] };
            if c {
                info!("[{label}] Baseband connected after {:.1}s", (i + 1) as f64 * 0.5);
                break;
            }
        }
    }

    let is_connected: bool = unsafe { msg_send![&*device, isConnected] };
    info!("[{label}] isConnected (post-wait) = {is_connected}");

    // SDP query
    info!("[{label}] SDP query…");
    let _: i32 = unsafe { msg_send![&*device, performSDPQuery: std::ptr::null::<AnyObject>()] };
    std::thread::sleep(std::time::Duration::from_secs(2));
    let date = NSDate::dateWithTimeIntervalSinceNow(0.1);
    runloop.runUntilDate(&date);

    // List SDP records
    let mut rfcomm_channels: Vec<u8> = Vec::new();
    unsafe {
        let services: *const AnyObject = msg_send![&*device, services];
        if !services.is_null() {
            let count: usize = msg_send![services, count];
            info!("[{label}] SDP: {count} records");
            for i in 0..count {
                let rec: *const AnyObject = msg_send![services, objectAtIndex: i];
                let svc_name_ptr: *const NSString = msg_send![rec, getServiceName];
                let svc_name = if !svc_name_ptr.is_null() { (*svc_name_ptr).to_string() } else { "<unnamed>".into() };
                let mut ch: u8 = 0;
                let ch_r: i32 = msg_send![rec, getRFCOMMChannelID: &mut ch as *mut u8];
                let mut psm: u16 = 0;
                let psm_r: i32 = msg_send![rec, getL2CAPPSM: &mut psm as *mut u16];
                let rfcomm_s = if ch_r == 0 { rfcomm_channels.push(ch); format!("RFCOMM={ch}") } else { String::new() };
                let l2cap_s = if psm_r == 0 { format!("L2CAP={psm}") } else { String::new() };
                info!("[{label}]   [{i}] {svc_name:35} {rfcomm_s:12} {l2cap_s}");
            }
        } else {
            info!("[{label}] No SDP records");
        }
    }

    // Try channels: 25 first, then others
    let mut try_order = vec![];
    if rfcomm_channels.contains(&25) { try_order.push(25u8); }
    for &ch in &rfcomm_channels { if ch != 25 { try_order.push(ch); } }
    // Also try 25 even if not in SDP
    if !try_order.contains(&25) { try_order.insert(0, 25); }

    for &ch in &try_order {
        info!("[{label}] Trying RFCOMM channel {ch}…");
        let mut channel_ptr: *mut AnyObject = std::ptr::null_mut();
        let r: i32 = unsafe {
            msg_send![
                &*device,
                openRFCOMMChannelSync: &mut channel_ptr,
                withChannelID: ch,
                delegate: std::ptr::null::<AnyObject>()
            ]
        };
        if r == 0 && !channel_ptr.is_null() {
            let is_open: bool = unsafe { msg_send![channel_ptr, isOpen] };
            let mtu: u16 = unsafe { msg_send![channel_ptr, getMTU] };
            info!("[{label}] ✅ Channel {ch} opened! isOpen={is_open} MTU={mtu}");
            let _: () = unsafe { msg_send![channel_ptr, closeChannel] };
            return Ok(format!("RFCOMM channel {ch} (MTU={mtu})"));
        }
        let err = match r as u32 {
            0xe00002bc => "kIOReturnNotPermitted",
            0xe00002c0 => "kIOReturnNotReady",
            0xe00002c2 => "kIOReturnNoDevice",
            0xe00002d8 => "kIOReturnTimeout",
            _ => "unknown",
        };
        info!("[{label}] Channel {ch}: 0x{r:08x} ({err})");
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    Err(anyhow!("All RFCOMM attempts failed (channels tried: {try_order:?})"))
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── macOS: RFCOMM data reading phase ──────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
async fn rfcomm_data_phase_macos(device_name: &str) -> Result<()> {
    use std::sync::mpsc as std_mpsc;

    let name = device_name.to_string();
    let (data_tx, data_rx) = std_mpsc::channel::<Vec<u8>>();
    let (status_tx, mut status_rx) = tokio::sync::mpsc::channel::<Result<()>>(1);

    std::thread::spawn(move || {
        macos_rfcomm_with_delegate(&name, data_tx, status_tx);
    });

    match status_rx.recv().await {
        Some(Ok(())) => info!("RFCOMM connected!"),
        Some(Err(e)) => return Err(e),
        None => return Err(anyhow!("RFCOMM thread exited")),
    }

    info!("Reading RFCOMM data for 30 seconds…");
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut total_bytes: u64 = 0;
    let mut packet_count: u64 = 0;
    let mut processor = mw75::parse::PacketProcessor::new(false);

    loop {
        if Instant::now() >= deadline { break; }
        match data_rx.try_recv() {
            Ok(data) => {
                if data.is_empty() { info!("RFCOMM closed"); break; }
                total_bytes += data.len() as u64;
                if packet_count < 10 {
                    let h = if data.len() <= 64 { hex(&data) } else { format!("{} … ({} B)", hex(&data[..32]), data.len()) };
                    info!("  RFCOMM chunk ({} B): {h}", data.len());
                }
                let events = processor.process_data(&data);
                for event in &events {
                    if let mw75::types::Mw75Event::Eeg(pkt) = event {
                        if packet_count < 20 {
                            info!("  ✅ EEG: counter={} ref={:.2} drl={:.2} ch1={:.2}µV",
                                pkt.counter, pkt.ref_value, pkt.drl,
                                pkt.channels.first().unwrap_or(&0.0));
                        }
                        packet_count += 1;
                    }
                }
                if packet_count > 0 && packet_count % 500 == 0 {
                    let stats = processor.get_stats();
                    let elapsed = 30.0 - deadline.saturating_duration_since(Instant::now()).as_secs_f64();
                    info!("  📊 {packet_count} pkts, {total_bytes} B, {:.1} Hz, err={:.1}%",
                        packet_count as f64 / elapsed, stats.error_rate());
                }
            }
            Err(std_mpsc::TryRecvError::Empty) => {
                tokio::time::sleep(Duration::from_micros(100)).await;
            }
            Err(std_mpsc::TryRecvError::Disconnected) => { info!("RFCOMM channel closed"); break; }
        }
    }

    let stats = processor.get_stats();
    info!("\n═══ Summary ═══");
    info!("  Bytes: {total_bytes}  EEG packets: {packet_count}");
    info!("  Valid: {}  Invalid: {}  Error: {:.1}%",
        stats.valid_packets, stats.invalid_packets, stats.error_rate());
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_rfcomm_with_delegate(
    device_name: &str,
    data_tx: std::sync::mpsc::Sender<Vec<u8>>,
    status_tx: tokio::sync::mpsc::Sender<Result<()>>,
) {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::sync::mpsc::Sender;

    use objc2::sel;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
    use objc2::{msg_send, ClassType};
    use objc2_foundation::{NSArray, NSDate, NSObject, NSRunLoop, NSString};
    use objc2_io_bluetooth::IOBluetoothDevice;

    const RFCOMM_CHANNEL: u8 = 25;

    // Find device
    let device: Retained<IOBluetoothDevice> = unsafe {
        let mut found: Option<Retained<IOBluetoothDevice>> = None;
        let paired: Option<Retained<NSArray<IOBluetoothDevice>>> =
            msg_send![IOBluetoothDevice::class(), pairedDevices];
        if let Some(ref devices) = paired {
            let count: usize = msg_send![devices, count];
            for i in 0..count {
                let dev: Retained<IOBluetoothDevice> = msg_send![devices, objectAtIndex: i];
                let name_ptr: *const NSString = msg_send![&*dev, name];
                if !name_ptr.is_null() && (*name_ptr).to_string() == device_name {
                    found = Some(dev);
                    break;
                }
            }
        }
        match found {
            Some(d) => d,
            None => {
                let _ = status_tx.blocking_send(Err(anyhow!("Device not found")));
                return;
            }
        }
    };

    let runloop = NSRunLoop::currentRunLoop();

    // Baseband
    let is_connected: bool = unsafe { msg_send![&*device, isConnected] };
    if !is_connected {
        let _: i32 = unsafe { msg_send![&*device, openConnection] };
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let date = NSDate::dateWithTimeIntervalSinceNow(0.05);
            runloop.runUntilDate(&date);
            let c: bool = unsafe { msg_send![&*device, isConnected] };
            if c { break; }
        }
    }

    // Delegate setup
    thread_local! {
        static DATA_SENDER: RefCell<Option<Sender<Vec<u8>>>> = RefCell::new(None);
    }
    DATA_SENDER.with(|cell| { *cell.borrow_mut() = Some(data_tx.clone()); });

    let delegate_class = unsafe {
        let class_name = c"RFCOMMProbeDelegate";
        let existing = objc2::runtime::AnyClass::get(class_name);
        if let Some(cls) = existing {
            cls as *const AnyClass
        } else {
            let mut builder = ClassBuilder::new(class_name, NSObject::class())
                .expect("Failed to create delegate class");

            extern "C" fn rfcomm_data_cb(
                _this: *mut AnyObject, _sel: Sel,
                _channel: *const AnyObject, data: *const c_void, length: usize,
            ) {
                if data.is_null() || length == 0 { return; }
                let bytes = unsafe { std::slice::from_raw_parts(data as *const u8, length) };
                DATA_SENDER.with(|cell| {
                    if let Some(ref tx) = *cell.borrow() { let _ = tx.send(bytes.to_vec()); }
                });
            }

            extern "C" fn rfcomm_open_cb(
                _this: *mut AnyObject, _sel: Sel,
                _channel: *const AnyObject, status: i32,
            ) {
                info!("macOS delegate: openComplete status=0x{status:08x}");
            }

            extern "C" fn rfcomm_closed_cb(
                _this: *mut AnyObject, _sel: Sel, _channel: *const AnyObject,
            ) {
                info!("macOS delegate: channel closed");
                DATA_SENDER.with(|cell| {
                    if let Some(ref tx) = *cell.borrow() { let _ = tx.send(Vec::new()); }
                });
            }

            builder.add_method(
                sel!(rfcommChannelData:data:length:),
                rfcomm_data_cb as extern "C" fn(*mut AnyObject, Sel, *const AnyObject, *const c_void, usize),
            );
            builder.add_method(
                sel!(rfcommChannelOpenComplete:status:),
                rfcomm_open_cb as extern "C" fn(*mut AnyObject, Sel, *const AnyObject, i32),
            );
            builder.add_method(
                sel!(rfcommChannelClosed:),
                rfcomm_closed_cb as extern "C" fn(*mut AnyObject, Sel, *const AnyObject),
            );

            builder.register() as *const AnyClass
        }
    };

    let delegate: Retained<NSObject> = unsafe {
        let alloc: *mut AnyObject = msg_send![delegate_class, alloc];
        Retained::from_raw(msg_send![alloc, init]).expect("delegate alloc failed")
    };

    // Try RFCOMM channels: 25, then all discovered
    let channels_to_try = [RFCOMM_CHANNEL, 2, 10];
    let mut channel_ptr: *mut AnyObject = std::ptr::null_mut();
    let mut opened = false;

    for &ch in &channels_to_try {
        info!("macOS: trying RFCOMM channel {ch} with delegate…");
        channel_ptr = std::ptr::null_mut();
        let r: i32 = unsafe {
            msg_send![
                &*device,
                openRFCOMMChannelSync: &mut channel_ptr,
                withChannelID: ch,
                delegate: &*delegate
            ]
        };
        if r == 0 && !channel_ptr.is_null() {
            let is_open: bool = unsafe { msg_send![channel_ptr, isOpen] };
            let mtu: u16 = unsafe { msg_send![channel_ptr, getMTU] };
            info!("macOS: ✅ RFCOMM channel {ch} opened! isOpen={is_open} MTU={mtu}");
            let _: () = unsafe { msg_send![channel_ptr, setDelegate: &*delegate] };
            opened = true;
            break;
        }
        let err = match r as u32 {
            0xe00002bc => "kIOReturnNotPermitted",
            0xe00002c0 => "kIOReturnNotReady",
            _ => "unknown",
        };
        info!("macOS: channel {ch}: 0x{r:08x} ({err})");
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    if !opened {
        let _ = status_tx.blocking_send(Err(anyhow!("All RFCOMM channels failed")));
        return;
    }

    let _ = status_tx.blocking_send(Ok(()));

    // Pump run loop
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(35);
    loop {
        let date = NSDate::dateWithTimeIntervalSinceNow(0.01);
        runloop.runUntilDate(&date);
        let is_open: bool = unsafe { msg_send![channel_ptr, isOpen] };
        if !is_open {
            let _ = data_tx.send(Vec::new());
            break;
        }
        if start.elapsed() > timeout {
            let _: () = unsafe { msg_send![channel_ptr, closeChannel] };
            let _ = data_tx.send(Vec::new());
            break;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Linux: RFCOMM via BlueZ ───────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "linux")]
async fn rfcomm_data_probe_linux(_device_name: &str) -> Result<()> {
    use bluer::rfcomm::{SocketAddr, Stream};
    use bluer::Address;
    use tokio::io::AsyncReadExt;

    const RFCOMM_CHANNEL: u8 = 25;

    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    let mut address: Option<Address> = None;
    for addr in adapter.device_addresses().await? {
        if let Ok(dev) = adapter.device(addr) {
            if let Ok(Some(name)) = dev.name().await {
                if name.to_uppercase().contains("MW75") { address = Some(addr); }
            }
        }
    }
    let addr = address.ok_or_else(|| anyhow!("MW75 not found"))?;
    info!("Linux: connecting RFCOMM to {addr} ch {RFCOMM_CHANNEL}…");

    let mut stream = tokio::time::timeout(Duration::from_secs(10),
        Stream::connect(SocketAddr::new(addr, RFCOMM_CHANNEL))
    ).await.map_err(|_| anyhow!("timeout"))??;
    info!("Linux: ✅ connected!");

    let mut buf = [0u8; 1024];
    let mut total_bytes: u64 = 0;
    let mut packet_count: u64 = 0;
    let mut processor = mw75::parse::PacketProcessor::new(false);
    let deadline = Instant::now() + Duration::from_secs(30);

    loop {
        if Instant::now() >= deadline { break; }
        match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => {
                total_bytes += n as u64;
                if packet_count < 10 { info!("  chunk ({n} B): {}", hex(&buf[..n.min(64)])); }
                for event in processor.process_data(&buf[..n]) {
                    if let mw75::types::Mw75Event::Eeg(pkt) = &event {
                        if packet_count < 20 {
                            info!("  ✅ EEG: counter={} ch1={:.2}µV", pkt.counter, pkt.channels.first().unwrap_or(&0.0));
                        }
                        packet_count += 1;
                    }
                }
            }
            Ok(Err(e)) => { info!("read error: {e}"); break; }
            Err(_) => { info!("no data 5s"); break; }
        }
    }

    let stats = processor.get_stats();
    info!("\n═══ Summary ═══");
    info!("  Bytes: {total_bytes}  Packets: {packet_count}  Error: {:.1}%", stats.error_rate());
    Ok(())
}
