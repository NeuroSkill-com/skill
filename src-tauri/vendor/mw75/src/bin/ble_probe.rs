//! BLE probe for MW75 Neuro — dumps everything the device sends.
//!
//! Usage: cargo run --bin ble-probe --features rfcomm
//!
//! This tool:
//! 1. Connects via BLE, enumerates ALL services & characteristics
//! 2. Reads every readable characteristic (raw hex dump)
//! 3. Subscribes to ALL notify/indicate characteristics
//! 4. Sends activation commands one at a time, showing all responses
//! 5. Waits 15s collecting ALL data from ALL characteristics
//! 6. Tries the second command characteristic (0x1104) too
//! 7. Probes IOBluetooth SDP records (macOS) for RFCOMM channels

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use btleplug::api::{
    Central, Characteristic, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, WriteType,
};
#[cfg(target_os = "macos")]
use btleplug::api::CentralState;
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use log::{info, warn};
use uuid::Uuid;

// ── Known UUIDs ───────────────────────────────────────────────────────────────

const CMD_1101: Uuid = Uuid::from_u128(0x00001101_d102_11e1_9b23_00025b00a5a5);
const STATUS_1102: Uuid = Uuid::from_u128(0x00001102_d102_11e1_9b23_00025b00a5a5);
const DATA_1103: Uuid = Uuid::from_u128(0x00001103_d102_11e1_9b23_00025b00a5a5);
const CMD_1104: Uuid = Uuid::from_u128(0x00001104_d102_11e1_9b23_00025b00a5a6);
const STATUS_1105: Uuid = Uuid::from_u128(0x00001105_d102_11e1_9b23_00025b00a5a6);
const DATA_1106: Uuid = Uuid::from_u128(0x00001106_d102_11e1_9b23_00025b00a5a6);

// Commands
const ENABLE_EEG: [u8; 5] = [0x09, 0x9A, 0x03, 0x60, 0x01];
const ENABLE_RAW: [u8; 5] = [0x09, 0x9A, 0x03, 0x41, 0x01];
const DISABLE_RAW: [u8; 5] = [0x09, 0x9A, 0x03, 0x41, 0x00];
const DISABLE_EEG: [u8; 5] = [0x09, 0x9A, 0x03, 0x60, 0x00];
const BATTERY: [u8; 5] = [0x09, 0x9A, 0x03, 0x14, 0xFF];

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

fn char_label(uuid: &Uuid) -> &'static str {
    if *uuid == CMD_1101 { "CMD_1101" }
    else if *uuid == STATUS_1102 { "STATUS_1102" }
    else if *uuid == DATA_1103 { "DATA_1103" }
    else if *uuid == CMD_1104 { "CMD_1104" }
    else if *uuid == STATUS_1105 { "STATUS_1105" }
    else if *uuid == DATA_1106 { "DATA_1106" }
    else { "UNKNOWN" }
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

/// Collect notifications for N seconds, printing details
async fn collect_for(
    notifs: &mut (impl StreamExt<Item = btleplug::api::ValueNotification> + Unpin),
    secs: u64,
    phase: &str,
) -> Vec<(Uuid, Vec<u8>)> {
    let mut collected = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();

    info!("  ⏱ Collecting for {secs}s ({phase}) …");

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() { break; }

        match tokio::time::timeout(remaining, notifs.next()).await {
            Ok(Some(n)) => {
                let lbl = char_label(&n.uuid);
                let entry = counts.entry(lbl.to_string()).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += n.value.len();

                // Show first 8 notifications per char in full, then just count
                if entry.0 <= 8 {
                    let h = hex(&n.value);
                    info!("    📨 {lbl:14} #{:<3} ({:>3} B): {h}", entry.0, n.value.len());

                    // Annotate known patterns
                    if n.value.first() == Some(&0xAA) && n.value.len() >= 4 {
                        info!("       → sync=0xAA event_id={} data_len={} counter={}",
                            n.value[1], n.value[2], n.value[3]);
                    }
                    if n.value.len() >= 5 && n.value[0] == 0x09 && n.value[1] == 0x9A {
                        let cmd = n.value[3];
                        let status = n.value[4];
                        let cmd_name = match cmd {
                            0x60 => "EEG",
                            0x41 => "RAW_MODE",
                            0x14 => "BATTERY",
                            0xE0 => "UNKNOWN_E0",
                            _ => "?",
                        };
                        info!("       → response: cmd=0x{cmd:02x}({cmd_name}) status=0x{status:02x}");
                    }
                } else if entry.0 == 9 {
                    info!("    📨 {lbl:14} (suppressing further detail, counting…)");
                }

                collected.push((n.uuid, n.value));
            }
            Ok(None) => { info!("  Stream ended"); break; }
            Err(_) => break, // timeout
        }
    }

    info!("  📊 Summary ({phase}):");
    if counts.is_empty() {
        info!("    (no notifications received)");
    }
    for (lbl, (count, bytes)) in counts.iter() {
        info!("    {lbl:14}: {count} notifications, {bytes} bytes");
    }
    collected
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("═══════════════════════════════════════════════════════════════");
    info!("MW75 BLE Probe");
    info!("═══════════════════════════════════════════════════════════════");

    let manager = Manager::new().await?;
    let adapter = manager.adapters().await?.into_iter().next()
        .ok_or_else(|| anyhow!("No Bluetooth adapter"))?;

    #[cfg(target_os = "macos")]
    {
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

    // ── Connect ───────────────────────────────────────────────────────────────
    info!("\n═══ Connecting …");
    peripheral.connect().await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    peripheral.discover_services().await?;
    info!("Connected.\n");

    let chars: BTreeSet<Characteristic> = peripheral.characteristics();

    // ── Characteristics ───────────────────────────────────────────────────────
    info!("═══ CHARACTERISTICS ({}) ═══", chars.len());
    for c in &chars {
        info!("  {:<14} uuid={}  svc={}  props={:?}",
            char_label(&c.uuid), c.uuid, c.service_uuid, c.properties);
    }

    // ── Read all readable ─────────────────────────────────────────────────────
    info!("\n═══ READING READABLE CHARACTERISTICS ═══");
    for c in &chars {
        if c.properties.contains(CharPropFlags::READ) {
            let lbl = char_label(&c.uuid);
            match peripheral.read(c).await {
                Ok(data) => {
                    info!("  {lbl:14} ({} B): {}", data.len(), hex(&data));
                    let ascii: String = data.iter()
                        .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' })
                        .collect();
                    info!("  {lbl:14} ASCII: {ascii}");
                }
                Err(e) => info!("  {lbl:14} error: {e}"),
            }
        }
    }

    // ── Subscribe ─────────────────────────────────────────────────────────────
    info!("\n═══ SUBSCRIBING TO NOTIFY/INDICATE ═══");
    for c in &chars {
        if c.properties.contains(CharPropFlags::NOTIFY) || c.properties.contains(CharPropFlags::INDICATE) {
            let lbl = char_label(&c.uuid);
            match peripheral.subscribe(c).await {
                Ok(()) => info!("  ✅ {lbl} ({})", c.uuid),
                Err(e) => warn!("  ❌ {lbl}: {e}"),
            }
        }
    }
    let mut notifications = peripheral.notifications().await?;

    // ── Phase 1: Baseline (3s) ────────────────────────────────────────────────
    info!("\n═══ PHASE 1: Baseline (no commands sent) ═══");
    collect_for(&mut notifications, 3, "baseline").await;

    // ── Phase 2: ENABLE_EEG → CMD_1101 ───────────────────────────────────────
    info!("\n═══ PHASE 2: ENABLE_EEG → CMD_1101 ═══");
    let c1101 = chars.iter().find(|c| c.uuid == CMD_1101);
    if let Some(c) = c1101 {
        peripheral.write(c, &ENABLE_EEG, WriteType::WithResponse).await?;
        info!("  ✅ Sent: {}", hex(&ENABLE_EEG));
    }
    collect_for(&mut notifications, 5, "after ENABLE_EEG").await;

    // ── Phase 3: ENABLE_RAW → CMD_1101 ───────────────────────────────────────
    info!("\n═══ PHASE 3: ENABLE_RAW → CMD_1101 ═══");
    if let Some(c) = c1101 {
        peripheral.write(c, &ENABLE_RAW, WriteType::WithResponse).await?;
        info!("  ✅ Sent: {}", hex(&ENABLE_RAW));
    }
    collect_for(&mut notifications, 5, "after ENABLE_RAW").await;

    // ── Phase 4: BATTERY → CMD_1101 ──────────────────────────────────────────
    info!("\n═══ PHASE 4: BATTERY → CMD_1101 ═══");
    if let Some(c) = c1101 {
        peripheral.write(c, &BATTERY, WriteType::WithResponse).await?;
        info!("  ✅ Sent: {}", hex(&BATTERY));
    }
    collect_for(&mut notifications, 3, "after BATTERY").await;

    // ── Phase 5: Same commands → CMD_1104 (second service) ───────────────────
    info!("\n═══ PHASE 5: Commands → CMD_1104 (second service) ═══");
    let c1104 = chars.iter().find(|c| c.uuid == CMD_1104);
    if let Some(c) = c1104 {
        for (name, cmd) in [("ENABLE_EEG", ENABLE_EEG), ("ENABLE_RAW", ENABLE_RAW), ("BATTERY", BATTERY)] {
            info!("  Sending {name} → CMD_1104 …");
            match peripheral.write(c, &cmd, WriteType::WithResponse).await {
                Ok(()) => info!("  ✅ Sent: {}", hex(&cmd)),
                Err(e) => warn!("  ❌ {e}"),
            }
            collect_for(&mut notifications, 3, &format!("CMD_1104 {name}")).await;
        }
    } else {
        info!("  CMD_1104 not found");
    }

    // ── Phase 6: Long listen (15s) ───────────────────────────────────────────
    info!("\n═══ PHASE 6: Long listen (15s) — all modes activated ═══");
    let long_data = collect_for(&mut notifications, 15, "long listen").await;

    // Analyze
    if !long_data.is_empty() {
        info!("\n  📊 Detailed analysis:");
        let mut by_uuid: HashMap<Uuid, Vec<Vec<u8>>> = HashMap::new();
        for (uuid, data) in &long_data {
            by_uuid.entry(*uuid).or_default().push(data.clone());
        }
        for (uuid, payloads) in &by_uuid {
            let lbl = char_label(uuid);
            let total: usize = payloads.iter().map(|p| p.len()).sum();
            let sizes: Vec<usize> = payloads.iter().map(|p| p.len()).collect();
            let min = sizes.iter().min().unwrap_or(&0);
            let max = sizes.iter().max().unwrap_or(&0);

            info!("    {lbl:14}: {} payloads, {total} bytes, sizes {min}–{max}", payloads.len());

            let sync_count = payloads.iter().filter(|p| p.first() == Some(&0xAA)).count();
            if sync_count > 0 {
                info!("      {sync_count} start with 0xAA sync byte");
            }

            // First-byte histogram
            let mut fb: HashMap<u8, usize> = HashMap::new();
            for p in payloads { if let Some(&b) = p.first() { *fb.entry(b).or_default() += 1; } }
            let mut sorted: Vec<_> = fb.iter().collect();
            sorted.sort_by_key(|(_, &c)| std::cmp::Reverse(c));
            let s: String = sorted.iter().take(5)
                .map(|(b, c)| format!("0x{b:02x}×{c}")).collect::<Vec<_>>().join(", ");
            info!("      first bytes: {s}");
        }
    }

    // ── Phase 7: Re-read readables (post-activation) ──────────────────────────
    info!("\n═══ PHASE 7: Re-read all readables (post-activation) ═══");
    for c in &chars {
        if c.properties.contains(CharPropFlags::READ) {
            let lbl = char_label(&c.uuid);
            match peripheral.read(c).await {
                Ok(data) => info!("  {lbl:14} ({} B): {}", data.len(), hex(&data)),
                Err(e) => info!("  {lbl:14} error: {e}"),
            }
        }
    }

    // ── Phase 8: Write to DATA chars (WRITE_WITHOUT_RESPONSE) ─────────────────
    info!("\n═══ PHASE 8: Write ENABLE_EEG via WRITE_WITHOUT_RESPONSE ═══");
    for uuid in [DATA_1103, DATA_1106] {
        let lbl = char_label(&uuid);
        if let Some(c) = chars.iter().find(|c| c.uuid == uuid) {
            if c.properties.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE) {
                info!("  Writing ENABLE_EEG → {lbl} …");
                match peripheral.write(c, &ENABLE_EEG, WriteType::WithoutResponse).await {
                    Ok(()) => { info!("  ✅ Sent"); }
                    Err(e) => { info!("  ❌ {e}"); }
                }
                collect_for(&mut notifications, 3, &format!("{lbl} write")).await;
            }
        }
    }

    // ── Cleanup ───────────────────────────────────────────────────────────────
    info!("\n═══ CLEANUP: Disable & disconnect ═══");
    if let Some(c) = c1101 {
        peripheral.write(c, &DISABLE_RAW, WriteType::WithResponse).await.ok();
        tokio::time::sleep(Duration::from_millis(200)).await;
        peripheral.write(c, &DISABLE_EEG, WriteType::WithResponse).await.ok();
    }
    peripheral.disconnect().await.ok();
    info!("Disconnected.");

    // ── macOS: SDP probe ──────────────────────────────────────────────────────
    #[cfg(target_os = "macos")]
    {
        info!("\n═══ IOBluetooth SDP records (macOS) ═══");
        tokio::task::spawn_blocking(probe_sdp_macos).await??;
    }

    info!("\n═══ DONE ═══");
    Ok(())
}

#[cfg(target_os = "macos")]
fn probe_sdp_macos() -> Result<()> {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, ClassType};
    use objc2_foundation::{NSArray, NSString};
    use objc2_io_bluetooth::IOBluetoothDevice;

    unsafe {
        let paired: Option<Retained<NSArray<IOBluetoothDevice>>> =
            msg_send![IOBluetoothDevice::class(), pairedDevices];
        if let Some(ref devices) = paired {
            let count: usize = msg_send![devices, count];
            for i in 0..count {
                let dev: Retained<IOBluetoothDevice> = msg_send![devices, objectAtIndex: i];
                let name_ptr: *const NSString = msg_send![&*dev, name];
                let name = if !name_ptr.is_null() { (*name_ptr).to_string() } else { "?".into() };
                if !name.to_uppercase().contains("MW75") { continue; }

                let addr_ptr: *const NSString = msg_send![&*dev, addressString];
                let addr = if !addr_ptr.is_null() { (*addr_ptr).to_string() } else { "?".into() };
                let connected: bool = msg_send![&*dev, isConnected];
                info!("  Device: {name} ({addr}) connected={connected}");

                let services: *const AnyObject = msg_send![&*dev, services];
                if services.is_null() {
                    info!("    No cached SDP records");
                    continue;
                }
                let svc_count: usize = msg_send![services, count];
                info!("    SDP records: {svc_count}");
                for j in 0..svc_count {
                    let rec: *const AnyObject = msg_send![services, objectAtIndex: j];
                    let svc_name_ptr: *const NSString = msg_send![rec, getServiceName];
                    let svc_name = if !svc_name_ptr.is_null() { (*svc_name_ptr).to_string() } else { "<unnamed>".into() };

                    let mut ch: u8 = 0;
                    let ch_r: i32 = msg_send![rec, getRFCOMMChannelID: &mut ch as *mut u8];
                    let mut psm: u16 = 0;
                    let psm_r: i32 = msg_send![rec, getL2CAPPSM: &mut psm as *mut u16];

                    let rfcomm = if ch_r == 0 { format!("RFCOMM={ch}") } else { String::new() };
                    let l2cap = if psm_r == 0 { format!("L2CAP={psm}") } else { String::new() };
                    info!("      [{j}] {svc_name:40} {rfcomm:12} {l2cap}");
                }
            }
        }
    }
    Ok(())
}
