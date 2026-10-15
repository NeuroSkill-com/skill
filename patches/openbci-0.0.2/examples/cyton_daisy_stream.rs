//! Stream 5 seconds from a Cyton+Daisy board (16 channels).
//!
//! Run:
//! ```sh
//! cargo run --example cyton_daisy_stream -- /dev/ttyUSB0
//! ```

use std::env;
use openbci::board::{Board, ConfigurableBoard};
use openbci::board::cyton_daisy::CytonDaisyBoard;
use openbci::channel_config::{ChannelConfig, Gain};
use openbci::electrode::{ElectrodeLayout, positions};

fn main() -> openbci::Result<()> {
    env_logger::init();

    let port = env::args().nth(1).unwrap_or_else(|| "/dev/ttyUSB0".into());

    // ── 16-electrode standard montage ─────────────────────────────────────────
    // Channels 0-7:  Cyton board
    // Channels 8-15: Daisy board
    let layout = ElectrodeLayout::from_labels(&[
        // Cyton (channels 0-7)
        positions::FP1, positions::FP2,
        positions::F3,  positions::F4,
        positions::C3,  positions::CZ,
        positions::C4,  positions::PZ,
        // Daisy (channels 8-15)
        positions::P3,  positions::P4,
        positions::O1,  positions::O2,
        positions::F7,  positions::F8,
        positions::T7,  positions::T8,
    ]);

    let mut board = CytonDaisyBoard::new(&port).with_electrode_layout(layout);

    println!("Connecting to Cyton+Daisy on {}…", port);
    board.prepare()?;

    // Apply 24× gain to all 16 channels
    let cfg = ChannelConfig::default().gain(Gain::X24);
    board.apply_all_channel_configs(&vec![cfg; 16])?;

    println!("\n=== Electrode layout (16 channels) ===");
    for (i, e) in board.electrode_layout().iter() {
        let board_name = if i < 8 { "Cyton" } else { "Daisy" };
        println!("  Ch{:02} ({board_name}): {}", i + 1, e.label);
    }
    println!();

    // ── Stream at ~125 Hz effective rate for 5 seconds ────────────────────────
    let target = board.sampling_rate() as usize * 5;
    println!("Streaming {} combined samples…", target);

    let stream = board.start_stream()?;
    let mut count = 0usize;
    let mut min_uv = vec![f64::MAX; 16];
    let mut max_uv = vec![f64::MIN; 16];

    for sample in stream.into_iter().take(target) {
        count += 1;
        for (ch, &v) in sample.eeg.iter().enumerate() {
            if v < min_uv[ch] { min_uv[ch] = v; }
            if v > max_uv[ch] { max_uv[ch] = v; }
        }
    }

    println!("\n=== 5-second amplitude summary (µV) ===");
    println!("{:<6} {:<12} {:>10} {:>10} {:>12}",
             "Ch", "Label", "Min", "Max", "Peak-to-peak");
    for i in 0..16 {
        let label = board.electrode_layout().label(i);
        let pp = max_uv[i] - min_uv[i];
        println!("{:<6} {:<12} {:>10.2} {:>10.2} {:>12.2}",
                 i + 1, label, min_uv[i], max_uv[i], pp);
    }

    board.release()?;
    Ok(())
}
