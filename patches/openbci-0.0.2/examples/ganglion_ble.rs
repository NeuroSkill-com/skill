//! Stream from a Ganglion board via native Bluetooth LE.
//!
//! Requires the `ble` feature (enabled by default).
//!
//! Run:
//! ```sh
//! cargo run --example ganglion_ble
//! # With a specific device identifier:
//! cargo run --example ganglion_ble -- D4:CA:6E:1A:2B:3C
//! ```
//!
//! The optional argument is a device identifier rather than strictly a MAC
//! address: on Linux and Windows it is the address, on macOS a per-host UUID,
//! because CoreBluetooth never discloses a peripheral's address.  Omit it and
//! the first peripheral named like a Ganglion is used.  Run with
//! `RUST_LOG=info` to see the identifier of whatever it finds.

#[cfg(feature = "ble")]
fn main() -> openbci::Result<()> {
    use std::time::Duration;
    use openbci::board::{Board};
    use openbci::board::ganglion::{GanglionBoard, GanglionConfig, GanglionFilter, GanglionFirmware};
    use openbci::electrode::{Electrode, ElectrodeLayout, positions};

    env_logger::init();

    let mac = std::env::args().nth(1);

    // ── Electrode layout ───────────────────────────────────────────────────────
    // Ganglion has 4 EEG channels
    let layout = ElectrodeLayout::new(4)
        .with_electrode(0, Electrode::eeg(positions::FP1))
        .with_electrode(1, Electrode::eeg(positions::FP2))
        .with_electrode(2, Electrode::eeg(positions::C3))
        .with_electrode(3, Electrode::eeg(positions::C4));

    let filter = GanglionFilter {
        mac_address: mac.clone(),
        device_name: None,
    };

    let config = GanglionConfig {
        scan_timeout: Duration::from_secs(15),
        firmware:     GanglionFirmware::Auto,
        filter,
        // The default service list — the Simblee service the data
        // characteristics live under, plus Device Information for the firmware
        // probe.  Override it here if discovery reports no characteristics.
        ..Default::default()
    };

    let mut board = GanglionBoard::new(config).with_electrode_layout(layout);

    println!("Scanning for Ganglion{}…",
             mac.as_deref().map(|m| format!(" (MAC: {})", m)).unwrap_or_default());
    board.prepare()?;
    println!("Connected!");

    // ── Stream 5 seconds at ~200 Hz = 1000 samples ───────────────────────────
    // Note: each BLE packet decodes into 2 samples, so real rate depends on firmware
    let target = 1000usize;
    println!("Streaming {} samples…\n", target);

    let labels = board.electrode_layout().labels();
    println!("{:<10} {:<14}  {}", "Sample#", "Time (s)", labels.join("        "));

    let stream = board.start_stream()?;
    let mut count = 0usize;

    for sample in stream.into_iter().take(target) {
        count += 1;

        if count % 20 == 0 {
            let vals: Vec<String> = sample.eeg.iter().map(|v| format!("{:+8.2}", v)).collect();
            println!("{:<10} {:<14.3}  {}", sample.sample_num, sample.timestamp, vals.join("  "));

            if let Some([ax, ay, az]) = sample.accel {
                println!("           accel: {:.3}g  {:.3}g  {:.3}g", ax, ay, az);
            }

            if let Some(ref resist) = sample.resistance {
                println!("           impedance: {:?} Ω", resist);
            }
        }
    }

    println!("\nReceived {} samples.", count);
    board.release()?;
    Ok(())
}

#[cfg(not(feature = "ble"))]
fn main() {
    eprintln!("This example requires the `ble` feature.");
    eprintln!("Run with: cargo run --example ganglion_ble --features ble");
}
