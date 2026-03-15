use std::io::{self, Read as _};
use std::sync::Arc;

use anyhow::Result;
use log::info;
use tokio::sync::mpsc;

use mw75::mw75_client::{Mw75Client, Mw75ClientConfig};
use mw75::protocol::{EEG_CHANNEL_NAMES, SampleRate};
use mw75::types::Mw75Event;

/// Delay before attempting to reconnect after a disconnect.
const RECONNECT_DELAY_SECS: u64 = 3;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // On macOS, CoreBluetooth (btleplug) and IOBluetooth (RFCOMM) both need
    // the main thread's NSRunLoop to be pumped. We run the tokio async work
    // on a background thread and keep the main thread pumping NSRunLoop.
    let (result_tx, result_rx) = std::sync::mpsc::channel::<Result<()>>();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        let result = rt.block_on(async_main());
        let _ = result_tx.send(result);
    });

    // Main thread: pump NSRunLoop for CoreBluetooth + IOBluetooth callbacks
    #[cfg(target_os = "macos")]
    {
        loop {
            // Use CFRunLoop to pump the main run loop
            unsafe {
                extern "C" {
                    fn CFRunLoopRunInMode(
                        mode: *const std::ffi::c_void,
                        seconds: f64,
                        return_after_source_handled: bool,
                    ) -> i32;
                }
                extern "C" {
                    static kCFRunLoopDefaultMode: *const std::ffi::c_void;
                }
                CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, false);
            }

            match result_rx.try_recv() {
                Ok(result) => return result,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err(anyhow::anyhow!("Async runtime exited unexpectedly"));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        result_rx
            .recv()
            .map_err(|_| anyhow::anyhow!("Async runtime exited"))?
    }
}

/// Set terminal to raw mode (no line buffering, no echo).
/// Returns the original termios settings for restoration.
#[cfg(unix)]
fn set_raw_mode() -> Option<libc::termios> {
    use std::mem::MaybeUninit;
    unsafe {
        let mut orig = MaybeUninit::<libc::termios>::uninit();
        if libc::tcgetattr(libc::STDIN_FILENO, orig.as_mut_ptr()) != 0 {
            return None;
        }
        let orig = orig.assume_init();
        let mut raw = orig;
        // Disable canonical mode (line buffering) and echo
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        // Read returns after 1 byte
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw);
        Some(orig)
    }
}

/// Restore terminal settings.
#[cfg(unix)]
fn restore_term(orig: &libc::termios) {
    unsafe {
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, orig);
    }
}

async fn async_main() -> Result<()> {
    // ── Configuration ─────────────────────────────────────────────────────────
    let sample_rate = match std::env::args().any(|a| a == "--256hz" || a == "--256") {
        true => SampleRate::Hz256,
        false => SampleRate::Hz500,
    };
    info!("Sample rate: {sample_rate}");

    let config = Mw75ClientConfig {
        scan_timeout_secs: 10,
        name_pattern: "MW75".into(),
        sample_rate,
    };
    let client = Mw75Client::new(config);

    // ── Stdin command channel — single keypress, no Enter needed ──────────────
    let (key_tx, mut key_rx) = mpsc::unbounded_channel::<char>();

    // Set raw mode so we get individual keypresses
    #[cfg(unix)]
    let orig_term = set_raw_mode();

    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut buf = [0u8; 1];
        loop {
            match stdin.lock().read(&mut buf) {
                Ok(1) => {
                    let ch = buf[0] as char;
                    // Handle quit immediately in the stdin thread —
                    // this works even if the event loop is blocked
                    // awaiting a BLE operation.
                    if ch == 'q' || ch == 'Q' {
                        eprintln!("\nQuit requested — exiting.");
                        // Restore terminal before exit
                        #[cfg(unix)]
                        unsafe {
                            let mut orig = std::mem::MaybeUninit::<libc::termios>::uninit();
                            if libc::tcgetattr(libc::STDIN_FILENO, orig.as_mut_ptr()) == 0 {
                                let mut t = orig.assume_init();
                                t.c_lflag |= libc::ICANON | libc::ECHO;
                                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &t);
                            }
                        }
                        std::process::exit(0);
                    }
                    if key_tx.send(ch).is_err() {
                        break;
                    }
                }
                Ok(_) | Err(_) => break,
            }
        }
    });

    // ── Connect / reconnect loop ──────────────────────────────────────────────
    let result = loop {
        match connect_and_run(&client, &mut key_rx).await {
            Ok(quit) if quit => {
                info!("Quit requested — exiting.");
                break Ok(());
            }
            Ok(_) => {
                // Disconnected — try to reconnect after a delay
                info!(
                    "Will attempt to reconnect in {RECONNECT_DELAY_SECS} s … \
                     (press 'q' to quit)"
                );
                tokio::time::sleep(std::time::Duration::from_secs(RECONNECT_DELAY_SECS)).await;
            }
            Err(e) => {
                info!(
                    "Connection failed: {e:#} — retrying in {RECONNECT_DELAY_SECS} s …"
                );
                tokio::time::sleep(std::time::Duration::from_secs(RECONNECT_DELAY_SECS)).await;
            }
        }
    };

    // Restore terminal on clean exit
    #[cfg(unix)]
    if let Some(ref orig) = orig_term {
        restore_term(orig);
    }

    result
}

/// Start (or restart) the RFCOMM reader task.
#[cfg(feature = "rfcomm")]
async fn start_rfcomm(
    handle: &Arc<mw75::mw75_client::Mw75Handle>,
    bt_address: &str,
) -> Option<mw75::rfcomm::RfcommHandle> {
    let rfcomm_handle = handle.clone();
    match mw75::rfcomm::start_rfcomm_stream(rfcomm_handle, bt_address).await {
        Ok(rfcomm) => {
            info!("RFCOMM reader task started");
            Some(rfcomm)
        }
        Err(e) => {
            info!("RFCOMM failed: {e}");
            None
        }
    }
}

/// Run a single connect → activate → stream → disconnect cycle.
///
/// Returns `Ok(true)` if the user typed 'q' (quit), `Ok(false)` on
/// device disconnect (caller should reconnect), or `Err` on failure.
async fn connect_and_run(
    client: &Mw75Client,
    key_rx: &mut mpsc::UnboundedReceiver<char>,
) -> Result<bool> {
    info!("Connecting to MW75 headphones …");
    let (mut rx, handle) = client.connect().await?;
    let handle = Arc::new(handle);

    // ── Activation ────────────────────────────────────────────────────────────
    handle.start().await?;
    info!("Activation complete.");

    // ── Data transport ──────────────────────────────────────────────────────
    //
    // EEG data streams over RFCOMM channel 25 (Bluetooth Classic) after
    // the BLE activation handshake. BLE must be disconnected first —
    // on macOS, CoreBluetooth and IOBluetooth share the radio.
    //
    // IMPORTANT: On macOS, BLE and RFCOMM cannot be active simultaneously.
    // To pause/resume EEG, we must: abort RFCOMM → reconnect BLE → send
    // command → disconnect BLE → restart RFCOMM.
    #[cfg(feature = "rfcomm")]
    let bt_address = handle.peripheral_id();

    #[cfg(feature = "rfcomm")]
    let mut rfcomm_task: Option<mw75::rfcomm::RfcommHandle> = {
        info!("Disconnecting BLE before RFCOMM …");
        handle.disconnect_ble().await.ok();
        start_rfcomm(&handle, &bt_address).await
    };

    #[cfg(not(feature = "rfcomm"))]
    info!("RFCOMM feature not enabled — EEG data requires RFCOMM.\n\
           Run with: cargo run --bin mw75 --features rfcomm");

    info!("Commands: q = quit, s = stats, p = pause/resume EEG  (no Enter needed)\n");

    let mut eeg_paused = false;

    // ── Event loop ────────────────────────────────────────────────────────────
    let quit = false;

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(Mw75Event::Connected(name)) => {
                        info!("✅  Connected to: {name}");
                    }
                    Some(Mw75Event::Disconnected) => {
                        info!("❌  Disconnected from device.");
                        break;
                    }
                    Some(Mw75Event::Activated(status)) => {
                        info!(
                            "🔋  Activated: EEG={}, Raw={}",
                            status.eeg_enabled, status.raw_mode_enabled
                        );
                    }
                    Some(Mw75Event::Battery(bat)) => {
                        info!("🔋  Battery: {}%", bat.level);
                    }
                    Some(Mw75Event::Eeg(pkt)) => {
                        let ch_summary: String = pkt
                            .channels
                            .iter()
                            .enumerate()
                            .map(|(i, &v)| format!("{}={:+.1}", EEG_CHANNEL_NAMES[i], v))
                            .collect::<Vec<_>>()
                            .join(" ");
                        println!(
                            "[EEG] cnt={:3}  {ch_summary}  µV",
                            pkt.counter
                        );
                    }
                    Some(Mw75Event::RawData(data)) => {
                        println!("[RAW] {} bytes", data.len());
                    }
                    Some(Mw75Event::OtherEvent { event_id, counter, raw }) => {
                        println!(
                            "[OTHER] event_id={event_id} counter={counter} len={}",
                            raw.len()
                        );
                    }
                    None => {
                        // Channel closed — all senders dropped
                        info!("Event channel closed.");
                        break;
                    }
                }
            }
            key = key_rx.recv() => {
                match key {
                    // 'q' is handled directly in the stdin thread
                    // (bypasses the event loop so it works even during
                    // a blocking BLE operation)
                    Some('s') | Some('S') => {
                        let stats = handle.get_stats();
                        info!(
                            "Stats: {} total, {} valid, {} invalid ({:.1}% error rate)",
                            stats.total_packets,
                            stats.valid_packets,
                            stats.invalid_packets,
                            stats.error_rate()
                        );
                    }
                    #[cfg(feature = "rfcomm")]
                    Some('p') | Some('P') => {
                        if eeg_paused {
                            // ── Resume: restart RFCOMM ───────────────────
                            // The device is still in EEG-enabled state from
                            // initial activation. Just reconnect RFCOMM.
                            info!("Resuming EEG streaming …");
                            rfcomm_task = start_rfcomm(&handle, &bt_address).await;
                            eeg_paused = false;
                            info!("✅ EEG streaming resumed");
                        } else {
                            // ── Pause: shut down RFCOMM ──────────────────
                            // Closing the RFCOMM channel stops data flow.
                            // BLE disable is skipped — on macOS, BLE reconnect
                            // hangs after an RFCOMM session. The device stays
                            // in EEG-enabled state so resume can just restart
                            // RFCOMM without re-activation.
                            info!("Pausing EEG streaming …");
                            if let Some(rfcomm) = rfcomm_task.take() {
                                rfcomm.shutdown();
                                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            }
                            eeg_paused = true;
                            info!("⏸  EEG streaming paused (press 'p' to resume)");
                        }
                    }
                    #[cfg(not(feature = "rfcomm"))]
                    Some('p') | Some('P') => {
                        if eeg_paused {
                            info!("Resuming EEG streaming …");
                            match handle.start().await {
                                Ok(()) => { eeg_paused = false; info!("✅ EEG streaming resumed"); }
                                Err(e) => { info!("❌ Failed to resume EEG: {e}"); }
                            }
                        } else {
                            info!("Pausing EEG streaming …");
                            match handle.stop().await {
                                Ok(()) => { eeg_paused = true; info!("⏸  EEG streaming paused"); }
                                Err(e) => { info!("❌ Failed to pause EEG: {e}"); }
                            }
                        }
                    }
                    // Ignore other keys (Enter, arrow keys, etc.)
                    _ => {}
                }
            }
        }
    }

    // Shut down RFCOMM so the process can exit cleanly
    #[cfg(feature = "rfcomm")]
    if let Some(rfcomm) = rfcomm_task {
        rfcomm.shutdown();
    }

    // Print final stats for this session
    let stats = handle.get_stats();
    if stats.total_packets > 0 {
        info!(
            "Session stats: {} packets, {} valid ({:.1}%), {} invalid ({:.1}%)",
            stats.total_packets,
            stats.valid_packets,
            100.0 - stats.error_rate(),
            stats.invalid_packets,
            stats.error_rate()
        );
    }

    Ok(quit)
}
