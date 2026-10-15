//! Stream 5 seconds of data from a Cyton board (8 channels).
//!
//! Run:
//! ```sh
//! cargo run --example cyton_stream -- /dev/ttyUSB0
//! ```

use std::env;
use openbci::board::{Board, ConfigurableBoard};
use openbci::board::cyton::CytonBoard;
use openbci::channel_config::{ChannelConfig, Gain, InputType};
use openbci::electrode::{Electrode, ElectrodeLayout, positions};

fn main() -> openbci::Result<()> {
    env_logger::init();

    let port = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: cyton_stream <serial_port>");
        eprintln!("Example: cyton_stream /dev/ttyUSB0");
        std::process::exit(1);
    });

    // ── Electrode layout: standard 8-electrode EEG montage ────────────────────
    let layout = ElectrodeLayout::new(8)
        .with_electrode(0, Electrode::eeg(positions::FP1))
        .with_electrode(1, Electrode::eeg(positions::FP2))
        .with_electrode(2, Electrode::eeg(positions::C3))
        .with_electrode(3, Electrode::eeg(positions::CZ))
        .with_electrode(4, Electrode::eeg(positions::C4))
        .with_electrode(5, Electrode::eeg(positions::P3))
        .with_electrode(6, Electrode::eeg(positions::PZ))
        .with_electrode(7, Electrode::eeg(positions::P4));

    // ── Per-channel config ─────────────────────────────────────────────────────
    // Channels 0-5: standard EEG  (24× gain, normal input, bias + SRB2)
    let eeg_cfg = ChannelConfig::default()
        .gain(Gain::X24)
        .input_type(InputType::Normal)
        .bias(true)
        .srb2(true)
        .srb1(false);

    // Channel 6: reference shunted to check noise floor
    let noise_cfg = ChannelConfig::default()
        .gain(Gain::X24)
        .input_type(InputType::Shorted)
        .bias(false)
        .srb2(false);

    // Channel 7: powered off (not used in this montage)
    let off_cfg = ChannelConfig::default().power(false);

    let channel_configs = vec![
        eeg_cfg.clone(), // Fp1
        eeg_cfg.clone(), // Fp2
        eeg_cfg.clone(), // C3
        eeg_cfg.clone(), // Cz
        eeg_cfg.clone(), // C4
        eeg_cfg.clone(), // P3
        noise_cfg,       // Pz  → noise test
        off_cfg,         // P4  → off
    ];

    // ── Connect and configure ──────────────────────────────────────────────────
    let mut board = CytonBoard::new(&port).with_electrode_layout(layout);

    println!("Connecting to Cyton on {}…", port);
    board.prepare()?;
    println!("Connected!  Applying channel configs…");

    board.apply_all_channel_configs(&channel_configs)?;

    // Print the electrode map we'll be recording
    println!("\n=== Electrode layout ===");
    for (i, e) in board.electrode_layout().iter() {
        println!("  Ch{:02}: {} ({:?})", i + 1, e.label, e.signal_type);
    }
    println!();

    // ── Stream 5 seconds at 250 Hz = 1250 samples ────────────────────────────
    let target_samples = board.sampling_rate() as usize * 5;
    println!("Streaming {} samples ({} sec)…", target_samples, 5);
    println!("{:<8} {:<14} {}", "Sample#", "Time (s)", board.electrode_layout().labels().join("   "));

    let stream = board.start_stream()?;
    let mut count = 0usize;

    for sample in stream.into_iter().take(target_samples) {
        count += 1;

        // Print every 25th sample so the terminal stays readable
        if count % 25 == 0 {
            let labels: Vec<String> = sample.eeg
                .iter()
                .map(|v| format!("{:+8.2}", v))
                .collect();
            println!("{:<8} {:<14.3} {}", sample.sample_num, sample.timestamp, labels.join("  "));
        }

        // Print accelerometer if present
        if let Some([ax, ay, az]) = sample.accel {
            if count % 25 == 0 {
                println!("         accel: x={:.3}g  y={:.3}g  z={:.3}g", ax, ay, az);
            }
        }
    }

    println!("\nReceived {} samples.  Releasing board.", count);
    board.release()?;
    Ok(())
}
