//! RFCOMM transport for MW75 Neuro EEG data streaming.
//!
//! After BLE activation, the MW75 headphones stream 63-byte EEG packets at
//! 500 Hz over Bluetooth Classic RFCOMM channel 25.  This module provides
//! the platform-specific RFCOMM socket connection and an async reader loop
//! that feeds data into the packet processor.
//!
//! # Platform support
//!
//! | Platform | Backend | Feature gate |
//! |----------|---------|--------------|
//! | Linux    | BlueZ `AF_BLUETOOTH` RFCOMM socket via [`bluer`] | `rfcomm` |
//! | macOS    | IOBluetooth framework via [`objc2-io-bluetooth`] | `rfcomm` |
//! | Windows  | `Windows.Devices.Bluetooth.Rfcomm` via [`windows`] crate | `rfcomm` |
//!
//! # Architecture
//!
//! ```text
//! BLE activation ──► start_rfcomm_stream(handle, address)
//!                                ↓
//!                    RFCOMM socket connect (channel 25)
//!                                ↓
//!                    async read loop ──► handle.feed_data()
//!                                ↓
//!                    PacketProcessor ──► Mw75Event::Eeg
//! ```
//!
//! # Example
//!
//! ```no_run
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! use mw75::mw75_client::{Mw75Client, Mw75ClientConfig};
//! use mw75::rfcomm::start_rfcomm_stream;
//! use mw75::types::Mw75Event;
//! use std::sync::Arc;
//!
//! let client = Mw75Client::new(Mw75ClientConfig::default());
//! let (mut rx, handle) = client.connect().await?;
//! handle.start().await?;
//!
//! // After BLE activation, start RFCOMM data stream
//! let handle = Arc::new(handle);
//! let rfcomm_task = start_rfcomm_stream(handle.clone(), "AA:BB:CC:DD:EE:FF").await?;
//!
//! while let Some(event) = rx.recv().await {
//!     match event {
//!         Mw75Event::Eeg(pkt) => println!("EEG: counter={}", pkt.counter),
//!         Mw75Event::Disconnected => break,
//!         _ => {}
//!     }
//! }
//!
//! rfcomm_task.abort();
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Context, Result};
use log::{debug, error, info};
use tokio::task::JoinHandle;

use crate::mw75_client::Mw75Handle;
use crate::protocol::RFCOMM_CHANNEL;

/// RFCOMM connection timeout in seconds.
#[cfg(target_os = "linux")]
const RFCOMM_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Read buffer size. MW75 packets are 63 bytes; RFCOMM may deliver
/// arbitrary-sized chunks (commonly 64, 128, or up to MTU).
#[cfg(any(target_os = "linux", target_os = "windows"))]
const READ_BUF_SIZE: usize = 1024;

/// Post-BLE-disconnect settle time in milliseconds.
/// Required on some platforms (especially macOS) for the Bluetooth stack
/// to release the BLE connection before RFCOMM can connect.
const BLE_SETTLE_MS: u64 = 1000;



// ── Public API ────────────────────────────────────────────────────────────────

/// Connect to the MW75 device over RFCOMM and spawn an async reader task
/// that feeds data into the given [`Mw75Handle`].
///
/// The `address` parameter is the Bluetooth MAC address of the MW75 device,
/// formatted as `"AA:BB:CC:DD:EE:FF"`.
///
/// # BLE disconnect requirement
///
/// On macOS (and recommended on Linux), the BLE connection should be
/// disconnected **before** calling this function. The MW75 uses the same
/// Bluetooth radio for BLE and RFCOMM, and keeping BLE open can block
/// RFCOMM delegate callbacks (especially on macOS 26+ "Taho").
///
/// This function includes a short settle delay before connecting.
///
/// # Returns
///
/// An [`RfcommHandle`] that can be used to cleanly shut down the RFCOMM
/// stream. The reader task will also terminate naturally if the RFCOMM
/// connection drops (device powered off, out of range, etc.), in which
/// case it sends
/// [`Mw75Event::Disconnected`](crate::types::Mw75Event::Disconnected).
pub async fn start_rfcomm_stream(
    handle: Arc<Mw75Handle>,
    address: &str,
) -> Result<RfcommHandle> {
    let address = address.to_string();
    let device_name = handle.device_name().to_string();
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    info!("Starting RFCOMM stream to {address} on channel {RFCOMM_CHANNEL}");

    // Brief settle time for BLE disconnect to complete
    tokio::time::sleep(std::time::Duration::from_millis(BLE_SETTLE_MS)).await;

    let task = tokio::spawn(async move {
        match rfcomm_reader_loop(&handle, &address, &device_name, &shutdown_clone).await {
            Ok(()) => {
                info!("RFCOMM stream ended normally");
            }
            Err(e) => {
                error!("RFCOMM stream error: {e}");
            }
        }
        // Only signal disconnection if this wasn't an intentional shutdown
        // (e.g. pause). If shutdown was requested, the caller manages the
        // lifecycle and doesn't want a spurious Disconnected event.
        if !shutdown_clone.load(Ordering::SeqCst) {
            handle.send_disconnected().await;
        } else {
            info!("RFCOMM: intentional shutdown — suppressing Disconnected event");
        }
    });

    Ok(RfcommHandle { task, shutdown })
}

/// Handle to a running RFCOMM stream.
///
/// Call [`shutdown`](RfcommHandle::shutdown) to cleanly close the
/// IOBluetooth RFCOMM channel (macOS) and abort the reader task.
pub struct RfcommHandle {
    task: JoinHandle<()>,
    shutdown: Arc<AtomicBool>,
}

impl RfcommHandle {
    /// Signal the RFCOMM thread to close the channel and abort the reader task.
    ///
    /// On macOS this sets a flag that the IOBluetooth run-loop thread checks,
    /// causing it to call `closeRFCOMMChannel` and exit. On other platforms
    /// it simply aborts the async reader task.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.task.abort();
    }
}

/// Parse a Bluetooth MAC address string into a 6-byte array.
///
/// Accepts `"AA:BB:CC:DD:EE:FF"` format.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn parse_mac(s: &str) -> Result<[u8; 6]> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return Err(anyhow!("Invalid MAC address format: {s} (expected AA:BB:CC:DD:EE:FF)"));
    }
    let mut bytes = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        bytes[i] = u8::from_str_radix(part, 16)
            .with_context(|| format!("Invalid hex byte '{part}' in MAC address"))?;
    }
    Ok(bytes)
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Linux implementation (BlueZ RFCOMM socket via bluer) ──────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "linux")]
async fn rfcomm_reader_loop(handle: &Mw75Handle, address: &str, _device_name: &str, _shutdown: &AtomicBool) -> Result<()> {
    use bluer::rfcomm::{SocketAddr, Stream};
    use bluer::Address;
    use tokio::io::AsyncReadExt;

    let addr: Address = address.parse()
        .with_context(|| format!("Invalid Bluetooth address: {address}"))?;

    let sa = SocketAddr::new(addr, RFCOMM_CHANNEL);

    info!("Linux RFCOMM: connecting to {sa}…");

    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(RFCOMM_CONNECT_TIMEOUT_SECS),
        Stream::connect(sa),
    )
    .await
    .map_err(|_| anyhow!("RFCOMM connect timed out after {RFCOMM_CONNECT_TIMEOUT_SECS} s"))?
    .context("RFCOMM connect failed")?;

    info!("Linux RFCOMM: connected to {address} on channel {RFCOMM_CHANNEL}");

    let mut buf = [0u8; READ_BUF_SIZE];
    let mut total_bytes: u64 = 0;

    loop {
        match stream.read(&mut buf).await {
            Ok(0) => {
                info!("RFCOMM: connection closed by remote (EOF)");
                break;
            }
            Ok(n) => {
                total_bytes += n as u64;
                debug!("RFCOMM: read {n} bytes (total: {total_bytes})");
                handle.feed_data(&buf[..n]).await;
            }
            Err(e) => {
                // Check for expected disconnection errors
                let kind = e.kind();
                if kind == std::io::ErrorKind::ConnectionReset
                    || kind == std::io::ErrorKind::BrokenPipe
                    || kind == std::io::ErrorKind::NotConnected
                {
                    info!("RFCOMM: connection lost ({kind})");
                } else {
                    error!("RFCOMM: read error: {e}");
                }
                break;
            }
        }
    }

    info!("RFCOMM reader loop ended (total bytes: {total_bytes})");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── macOS implementation (IOBluetooth RFCOMM via objc2) ────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
async fn rfcomm_reader_loop(handle: &Mw75Handle, address: &str, device_name: &str, shutdown: &AtomicBool) -> Result<()> {
    use std::sync::mpsc as std_mpsc;

    let name_owned = device_name.to_string();
    let (data_tx, data_rx) = std_mpsc::channel::<Vec<u8>>();
    let (status_tx, mut status_rx) = tokio::sync::mpsc::channel::<Result<()>>(1);
    let address_owned = address.to_string();

    // Share the shutdown flag with the OS thread
    let shutdown_ptr = shutdown as *const AtomicBool as usize;

    // IOBluetooth RFCOMM: the delegate callbacks must be delivered on the
    // thread that opened the channel. We spawn a dedicated thread with its
    // own run loop. The key difference from our previous attempts: we now
    // add an NSMachPort and use runMode:beforeDate: to properly block.
    std::thread::Builder::new()
        .name("rfcomm-runloop".to_string())
        .spawn(move || {
            // SAFETY: the AtomicBool lives in an Arc owned by RfcommHandle,
            // which outlives both the tokio task and this OS thread.
            let shutdown_ref = unsafe { &*(shutdown_ptr as *const AtomicBool) };
            macos_rfcomm_thread(&name_owned, data_tx.clone(), status_tx, address_owned, shutdown_ref);
        })
        .expect("failed to spawn RFCOMM thread");

    // Wait for connection status
    match status_rx.recv().await {
        Some(Ok(())) => {
            info!("macOS RFCOMM: connected, waiting for EEG data…");
        }
        Some(Err(e)) => {
            return Err(e);
        }
        None => {
            return Err(anyhow!("macOS RFCOMM: connection thread exited unexpectedly"));
        }
    }

    // Read data from the std channel and feed to handle
    let mut total_bytes: u64 = 0;
    loop {
        match data_rx.try_recv() {
            Ok(data) => {
                if data.is_empty() {
                    info!("macOS RFCOMM: connection closed signal");
                    break;
                }
                total_bytes += data.len() as u64;
                debug!("macOS RFCOMM: received {} bytes (total: {total_bytes})", data.len());
                handle.feed_data(&data).await;
            }
            Err(std_mpsc::TryRecvError::Empty) => {
                tokio::time::sleep(std::time::Duration::from_micros(100)).await;
            }
            Err(std_mpsc::TryRecvError::Disconnected) => {
                info!("macOS RFCOMM: data channel closed");
                break;
            }
        }
    }

    info!("macOS RFCOMM reader ended (total bytes: {total_bytes})");
    Ok(())
}

// ── macOS: RFCOMM channel delegate ───────────────────────────────────────────
//
// IOBluetooth delivers data via delegate callbacks on the RFCOMM channel.
// We define a custom Objective-C class that implements the
// IOBluetoothRFCOMMChannelDelegate informal protocol and forwards data
// to a std::sync::mpsc::Sender.

#[cfg(target_os = "macos")]
mod macos_delegate {
    use std::ffi::c_void;
    use std::sync::mpsc::Sender;

    use log::{debug, info};
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
    use objc2_foundation::NSObject;

    pub struct RfcommDelegateIvars {
        pub data_tx: Sender<Vec<u8>>,
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements.
        // RfcommDelegate does not implement Drop.
        #[unsafe(super(NSObject))]
        #[name = "Mw75RfcommDelegate"]
        #[ivars = RfcommDelegateIvars]
        pub struct RfcommDelegate;

        // IOBluetoothRFCOMMChannelDelegate informal protocol methods.
        // These are called by IOBluetooth on the run loop thread when
        // data arrives, the channel opens, or the channel closes.
        impl RfcommDelegate {
            /// Called when data is received on the RFCOMM channel.
            #[unsafe(method(rfcommChannelData:data:length:))]
            fn rfcomm_channel_data(
                &self,
                _channel: *mut AnyObject,
                data_ptr: *mut c_void,
                length: usize,
            ) {
                if !data_ptr.is_null() && length > 0 {
                    let slice = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, length) };
                    let data = slice.to_vec();
                    debug!("macOS delegate: received {} bytes", data.len());
                    let _ = self.ivars().data_tx.send(data);
                }
            }

            /// Called when the RFCOMM channel open completes.
            #[unsafe(method(rfcommChannelOpenComplete:status:))]
            fn rfcomm_channel_open_complete(
                &self,
                _channel: *mut AnyObject,
                status: i32,
            ) {
                if status == 0 {
                    info!("macOS delegate: RFCOMM channel open complete (success)");
                } else {
                    info!("macOS delegate: RFCOMM channel open failed (status=0x{status:08x})");
                }
            }

            /// Called when the RFCOMM channel is closed.
            #[unsafe(method(rfcommChannelClosed:))]
            fn rfcomm_channel_closed(&self, _channel: *mut AnyObject) {
                info!("macOS delegate: RFCOMM channel closed by remote");
                // Send empty vec as "closed" signal
                let _ = self.ivars().data_tx.send(Vec::new());
            }
        }
    );

    impl RfcommDelegate {
        pub fn new(data_tx: Sender<Vec<u8>>) -> Retained<Self> {
            let this = Self::alloc();
            let this = this.set_ivars(RfcommDelegateIvars { data_tx });
            unsafe { msg_send![super(this), init] }
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_rfcomm_thread(
    device_name: &str,
    data_tx: std::sync::mpsc::Sender<Vec<u8>>,
    status_tx: tokio::sync::mpsc::Sender<Result<()>>,
    address: String,
    shutdown: &AtomicBool,
) {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, ClassType};
    use objc2_foundation::{NSArray, NSDate, NSRunLoop, NSString};
    use objc2_io_bluetooth::IOBluetoothDevice;

    // ── Step 1: Find the IOBluetoothDevice by name ────────────────────────────
    //
    // CoreBluetooth (btleplug) gives us a UUID, not a MAC address.
    // IOBluetooth is a separate Classic-BT framework, so we search
    // pairedDevices / recentDevices by name to bridge the two worlds.

    info!("macOS RFCOMM: looking up IOBluetoothDevice by name '{device_name}' …");

    let device: Option<Retained<IOBluetoothDevice>> = unsafe {
        let mut found: Option<Retained<IOBluetoothDevice>> = None;

        // Search paired devices
        let paired: Option<Retained<NSArray<IOBluetoothDevice>>> =
            msg_send![IOBluetoothDevice::class(), pairedDevices];
        if let Some(ref devices) = paired {
            let count: usize = msg_send![devices, count];
            for i in 0..count {
                let dev: Retained<IOBluetoothDevice> = msg_send![devices, objectAtIndex: i];
                let name_ptr: *const NSString = msg_send![&*dev, name];
                if !name_ptr.is_null() {
                    let name_str = (*name_ptr).to_string();
                    debug!("macOS RFCOMM: paired device: {name_str}");
                    if name_str == device_name {
                        found = Some(dev);
                        break;
                    }
                }
            }
        }

        // Fallback: search recent devices
        if found.is_none() {
            let recent: Option<Retained<NSArray<IOBluetoothDevice>>> =
                msg_send![IOBluetoothDevice::class(), recentDevices: 10usize];
            if let Some(ref devices) = recent {
                let count: usize = msg_send![devices, count];
                for i in 0..count {
                    let dev: Retained<IOBluetoothDevice> = msg_send![devices, objectAtIndex: i];
                    let name_ptr: *const NSString = msg_send![&*dev, name];
                    if !name_ptr.is_null() {
                        let name_str = (*name_ptr).to_string();
                        debug!("macOS RFCOMM: recent device: {name_str}");
                        if name_str == device_name {
                            found = Some(dev);
                            break;
                        }
                    }
                }
            }
        }

        found
    };

    let device = match device {
        Some(d) => d,
        None => {
            // Last resort: try parsing as MAC address
            if let Ok(mac_bytes) = parse_mac(&address) {
                let addr_str = format!(
                    "{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}",
                    mac_bytes[0], mac_bytes[1], mac_bytes[2],
                    mac_bytes[3], mac_bytes[4], mac_bytes[5]
                );
                let ns_addr = NSString::from_str(&addr_str);
                let dev: Option<Retained<IOBluetoothDevice>> = unsafe {
                    msg_send![IOBluetoothDevice::class(), deviceWithAddressString: &*ns_addr]
                };
                match dev {
                    Some(d) => d,
                    None => {
                        let _ = status_tx.blocking_send(Err(anyhow!(
                            "macOS: IOBluetoothDevice not found by name '{device_name}' \
                             or address '{address}'"
                        )));
                        return;
                    }
                }
            } else {
                let _ = status_tx.blocking_send(Err(anyhow!(
                    "macOS: IOBluetoothDevice not found by name '{device_name}' \
                     (peripheral ID '{address}' is a CoreBluetooth UUID, not a MAC address)"
                )));
                return;
            }
        }
    };

    // Log device address for diagnostics
    unsafe {
        let addr_ptr: *const NSString = msg_send![&*device, addressString];
        if !addr_ptr.is_null() {
            let addr = (*addr_ptr).to_string();
            info!("macOS RFCOMM: found device '{device_name}' at address {addr}");
        } else {
            info!("macOS RFCOMM: found device '{device_name}' (address unavailable)");
        }
    }

    // ── Step 2: Ensure baseband connection and SDP ────────────────────────────

    let runloop = NSRunLoop::currentRunLoop();

    let is_connected: bool = unsafe { msg_send![&*device, isConnected] };
    info!("macOS: device isConnected = {is_connected}");

    if !is_connected {
        info!("macOS: opening baseband connection …");
        let r: i32 = unsafe { msg_send![&*device, openConnection] };
        info!("macOS: openConnection returned 0x{r:08x}");
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let c: bool = unsafe { msg_send![&*device, isConnected] };
            if c { info!("macOS: baseband connected"); break; }
        }
    }

    // SDP query (results are likely cached for paired devices)
    info!("macOS: performing SDP query …");
    let _: i32 = unsafe { msg_send![&*device, performSDPQuery: std::ptr::null::<AnyObject>()] };
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Enumerate SDP service records to find available channels
    let mut rfcomm_channels: Vec<u8> = Vec::new();
    let mut l2cap_psms: Vec<u16> = Vec::new();
    unsafe {
        let services: *const AnyObject = msg_send![&*device, services];
        if !services.is_null() {
            let count: usize = msg_send![services, count];
            info!("macOS: {count} SDP service record(s):");
            for i in 0..count {
                let rec: *const AnyObject = msg_send![services, objectAtIndex: i];
                let svc_name_ptr: *const NSString = msg_send![rec, getServiceName];
                let svc_name = if !svc_name_ptr.is_null() { (*svc_name_ptr).to_string() } else { "<unnamed>".into() };

                let mut ch: u8 = 0;
                let ch_r: i32 = msg_send![rec, getRFCOMMChannelID: &mut ch as *mut u8];
                let mut psm: u16 = 0;
                let psm_r: i32 = msg_send![rec, getL2CAPPSM: &mut psm as *mut u16];

                let rfcomm_s = if ch_r == 0 { rfcomm_channels.push(ch); format!("RFCOMM={ch}") } else { String::new() };
                let l2cap_s = if psm_r == 0 { l2cap_psms.push(psm); format!("L2CAP={psm}") } else { String::new() };
                info!("macOS:   [{i}] {svc_name:35} {rfcomm_s:12} {l2cap_s}");
            }
        }
    }

    // ── Step 3: Create delegate and try RFCOMM channels ──────────────────────
    //
    // The delegate receives data callbacks from IOBluetooth when EEG data
    // arrives on the RFCOMM channel. Without a delegate, no data is delivered.

    // ── Step 3: Open RFCOMM channel (async) and wait for delegate ──────────
    //
    // CRITICAL: Must use openRFCOMMChannelAsync (not Sync)!
    // The Python reference (mw75-streamer) uses the async version.
    // The sync version opens the channel but doesn't properly set up
    // the delegate data flow. The async version triggers
    // rfcommChannelOpenComplete:status: on the delegate, which
    // establishes the data path.

    let delegate = macos_delegate::RfcommDelegate::new(data_tx.clone());

    // We also need to disconnect BLE first (matching Python reference flow)
    // The Python code calls disconnect_after_activation() before RFCOMM

    let mut channel_ptr: *mut AnyObject = std::ptr::null_mut();
    let mut saw_not_permitted = false;

    info!("macOS: opening RFCOMM channel {RFCOMM_CHANNEL} (async) …");
    for attempt in 1..=5u32 {
        channel_ptr = std::ptr::null_mut();
        let r: i32 = unsafe {
            msg_send![
                &*device,
                openRFCOMMChannelAsync: &mut channel_ptr as *mut *mut AnyObject,
                withChannelID: RFCOMM_CHANNEL as u8,
                delegate: &*delegate
            ]
        };
        if r == 0 {
            info!("macOS: openRFCOMMChannelAsync returned success (attempt {attempt})");
            break;
        }
        if r as u32 == 0xe00002bc { saw_not_permitted = true; }
        let err_name = macos_ioreturn_name(r as u32);
        info!("macOS: RFCOMM ch {RFCOMM_CHANNEL} attempt {attempt}/5: 0x{r:08x} ({err_name})");
        std::thread::sleep(std::time::Duration::from_secs(1));

        if attempt == 5 {
            let signing_hint = if saw_not_permitted {
                "\n\nThis is likely a CODE SIGNING issue. Run: ./macos/sign-and-run.sh"
            } else {
                ""
            };
            let _ = status_tx.blocking_send(Err(anyhow!(
                "macOS: RFCOMM channel {RFCOMM_CHANNEL} open failed after {attempt} attempts.{signing_hint}"
            )));
            return;
        }
    }

    // ── Step 4: Add port to run loop, then pump for callbacks ───────────────
    //
    // NSRunLoop.runUntilDate: returns immediately if no input sources exist.
    // We add an NSMachPort to keep the run loop alive. Then:
    // Phase 1: Wait for rfcommChannelOpenComplete (10s timeout)
    // Phase 2: Pump continuously for data (matching Python's 0.001s interval)

    let _delegate_keepalive = &delegate;

    // Add a port so the run loop has an input source and doesn't exit immediately
    unsafe {
        let port: *mut AnyObject = msg_send![objc2::class!(NSMachPort), port];
        if !port.is_null() {
            let _: () = msg_send![&*runloop, addPort: port, forMode: objc2_foundation::NSDefaultRunLoopMode];
            info!("macOS: added NSMachPort to run loop");
        }
    }

    // Phase 1: Wait for connection (up to 10 seconds)
    info!("macOS: waiting for RFCOMM delegate callback (timeout 10s)…");
    let connect_deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut connected = false;

    while std::time::Instant::now() < connect_deadline {
        let date = NSDate::dateWithTimeIntervalSinceNow(0.1);
        runloop.runUntilDate(&date);

        if !channel_ptr.is_null() {
            let is_open: bool = unsafe { msg_send![channel_ptr, isOpen] };
            if is_open {
                connected = true;
                info!("macOS: ✅ RFCOMM channel is open!");
                break;
            }
        }
    }

    if !connected {
        let _ = status_tx.blocking_send(Err(anyhow!(
            "macOS: RFCOMM connection timed out (delegate never called back)"
        )));
        return;
    }

    // Log channel diagnostics
    unsafe {
        let mtu: u16 = msg_send![channel_ptr, getMTU];
        let ch_id: u8 = msg_send![channel_ptr, getChannelID];
        let is_incoming: bool = msg_send![channel_ptr, isIncoming];
        let delegate_obj: *const AnyObject = msg_send![channel_ptr, delegate];
        info!("macOS: channel diagnostics: id={ch_id} MTU={mtu} incoming={is_incoming} delegate={}", if delegate_obj.is_null() { "null" } else { "set" });
    }

    info!("macOS: RFCOMM connected — data streaming active!");
    let _ = status_tx.blocking_send(Ok(()));

    // Phase 2: Stream data — pump run loop with short intervals
    loop {
        // Check shutdown flag before blocking on the run loop
        if shutdown.load(Ordering::SeqCst) {
            info!("macOS: RFCOMM shutdown requested — closing channel");
            if !channel_ptr.is_null() {
                unsafe {
                    let r: i32 = msg_send![channel_ptr, closeChannel];
                    info!("macOS: closeChannel returned 0x{r:08x}");
                }
            }
            let _ = data_tx.send(Vec::new());
            break;
        }

        // Use runMode:beforeDate: which blocks properly for input sources
        // Short interval so we notice shutdown requests quickly
        let date = NSDate::dateWithTimeIntervalSinceNow(0.1);
        let _ran: bool = unsafe {
            msg_send![&*runloop, runMode: objc2_foundation::NSDefaultRunLoopMode, beforeDate: &*date]
        };

        if !channel_ptr.is_null() {
            let is_open: bool = unsafe { msg_send![channel_ptr, isOpen] };
            if !is_open {
                info!("macOS: RFCOMM channel closed");
                let _ = data_tx.send(Vec::new());
                break;
            }
        }
    }
}

/// Convert a macOS IOReturn code to a human-readable name.
#[cfg(target_os = "macos")]
fn macos_ioreturn_name(code: u32) -> &'static str {
    match code {
        0x00000000 => "kIOReturnSuccess",
        0xe00002bc => "kIOReturnNotPermitted",
        0xe00002be => "kIOReturnExclusiveAccess",
        0xe00002c0 => "kIOReturnNotAttached",
        0xe00002c7 => "kIOReturnAborted",
        0xe00002d8 => "kIOReturnNotOpen",
        0xe00002ed => "kIOReturnTimeout",
        _ => "unknown",
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Windows implementation (Windows.Devices.Bluetooth.Rfcomm) ─────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
async fn rfcomm_reader_loop(handle: &Mw75Handle, address: &str, _device_name: &str, _shutdown: &AtomicBool) -> Result<()> {
    use windows::Devices::Bluetooth::Rfcomm::RfcommDeviceService;
    use windows::Devices::Bluetooth::BluetoothDevice;
    use windows::Networking::Sockets::StreamSocket;
    use windows::Storage::Streams::{DataReader, InputStreamOptions};

    let mac_bytes = parse_mac(address)?;

    // Convert MAC to u64 for Windows API (big-endian 6 bytes in low 48 bits)
    let bt_addr: u64 = (mac_bytes[0] as u64) << 40
        | (mac_bytes[1] as u64) << 32
        | (mac_bytes[2] as u64) << 24
        | (mac_bytes[3] as u64) << 16
        | (mac_bytes[4] as u64) << 8
        | (mac_bytes[5] as u64);

    info!("Windows RFCOMM: connecting to {address} (0x{bt_addr:012x})…");

    // Get Bluetooth device
    let device = tokio::task::spawn_blocking(move || -> Result<BluetoothDevice> {
        let op = BluetoothDevice::FromBluetoothAddressAsync(bt_addr)?;
        let device = op.get()?;
        Ok(device)
    })
    .await
    .context("Bluetooth device lookup panicked")?
    .context("Failed to find Bluetooth device")?;

    info!("Windows: found Bluetooth device");

    // Get RFCOMM services
    let rfcomm_services = tokio::task::spawn_blocking(move || -> Result<_> {
        let op = device.GetRfcommServicesAsync()?;
        let result = op.get()?;
        Ok(result)
    })
    .await
    .context("RFCOMM service lookup panicked")?
    .context("Failed to get RFCOMM services")?;

    let services = rfcomm_services.Services()?;
    if services.Size()? == 0 {
        return Err(anyhow!("No RFCOMM services found on device {address}"));
    }

    // Find the Serial Port Profile service or use the first one
    let service = services.GetAt(0)?;
    let host = service.ConnectionHostName()?;
    let service_name = service.ConnectionServiceName()?;

    info!(
        "Windows RFCOMM: connecting to service '{}'",
        service_name.to_string()
    );

    // Connect StreamSocket
    let socket = StreamSocket::new()?;

    tokio::task::spawn_blocking(move || -> Result<()> {
        let op = socket.ConnectAsync(&host, &service_name)?;
        op.get()?;
        Ok(())
    })
    .await
    .context("RFCOMM socket connect panicked")?
    .context("RFCOMM socket connect failed")?;

    info!("Windows RFCOMM: connected to {address}");

    // Read loop
    let input_stream = socket.InputStream()?;
    let reader = DataReader::CreateDataReader(&input_stream)?;
    reader.SetInputStreamOptions(InputStreamOptions::Partial)?;

    let mut total_bytes: u64 = 0;

    loop {
        // Read data
        let result = tokio::task::spawn_blocking({
            let reader = reader.clone();
            move || -> Result<Vec<u8>> {
                let op = reader.LoadAsync(READ_BUF_SIZE as u32)?;
                let bytes_read = op.get()?;
                if bytes_read == 0 {
                    return Ok(Vec::new());
                }
                let mut buf = vec![0u8; bytes_read as usize];
                reader.ReadBytes(&mut buf)?;
                Ok(buf)
            }
        })
        .await;

        match result {
            Ok(Ok(data)) if data.is_empty() => {
                info!("Windows RFCOMM: connection closed (EOF)");
                break;
            }
            Ok(Ok(data)) => {
                total_bytes += data.len() as u64;
                debug!("Windows RFCOMM: read {} bytes (total: {total_bytes})", data.len());
                handle.feed_data(&data).await;
            }
            Ok(Err(e)) => {
                error!("Windows RFCOMM: read error: {e}");
                break;
            }
            Err(e) => {
                error!("Windows RFCOMM: read task panicked: {e}");
                break;
            }
        }
    }

    info!("Windows RFCOMM reader ended (total bytes: {total_bytes})");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Unsupported platforms ─────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
async fn rfcomm_reader_loop(_handle: &Mw75Handle, address: &str, _device_name: &str, _shutdown: &AtomicBool) -> Result<()> {
    Err(anyhow!(
        "RFCOMM is not supported on this platform. \
         Use Mw75Handle::feed_data() to push raw bytes from an external transport."
    ))
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Tests ─────────────────────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mac_valid() {
        let mac = parse_mac("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(mac, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn parse_mac_lowercase() {
        let mac = parse_mac("aa:bb:cc:dd:ee:ff").unwrap();
        assert_eq!(mac, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn parse_mac_mixed_case() {
        let mac = parse_mac("Aa:Bb:Cc:Dd:Ee:Ff").unwrap();
        assert_eq!(mac, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn parse_mac_zeros() {
        let mac = parse_mac("00:00:00:00:00:00").unwrap();
        assert_eq!(mac, [0; 6]);
    }

    #[test]
    fn parse_mac_invalid_format() {
        assert!(parse_mac("AA:BB:CC:DD:EE").is_err());
        assert!(parse_mac("AA-BB-CC-DD-EE-FF").is_err());
        assert!(parse_mac("AABBCCDDEEFF").is_err());
        assert!(parse_mac("").is_err());
    }

    #[test]
    fn parse_mac_invalid_hex() {
        assert!(parse_mac("GG:BB:CC:DD:EE:FF").is_err());
        assert!(parse_mac("AA:XX:CC:DD:EE:FF").is_err());
    }

    #[test]
    fn rfcomm_channel_is_25() {
        assert_eq!(RFCOMM_CHANNEL, 25);
    }

    #[test]
    fn read_buf_size_adequate() {
        // Must be larger than one MW75 packet (63 bytes)
        assert!(READ_BUF_SIZE >= 63);
        // Should be a reasonable power-of-2-ish size
        assert!(READ_BUF_SIZE >= 512);
    }
}
