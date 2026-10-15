//! Stream from a Galea board (24-channel EEG+EMG, UDP).
//!
//! Run:
//! ```sh
//! cargo run --example galea_stream -- 192.168.1.200
//! ```

use openbci::board::{Board, ConfigurableBoard};
use openbci::board::galea::GaleaBoard;
use openbci::channel_config::{ChannelConfig, Gain};
use openbci::electrode::{Electrode, ElectrodeLayout, SignalType, positions};

fn main() -> openbci::Result<()> {
    env_logger::init();

    let ip = std::env::args().nth(1).unwrap_or_default();

    // ── Electrode layout: Galea-specific montage ───────────────────────────────
    // Channels 0-7:   EMG electrodes  (muscle groups)
    // Channels 8-17:  EEG electrodes  (10-20 system)
    // Channels 18-21: Auxiliary EMG
    let mut layout = ElectrodeLayout::new(24);

    // EMG channels: upper-face muscles
    let emg_labels = ["Left Frontalis","Right Frontalis",
                      "Left Temporalis","Right Temporalis",
                      "Left Masseter","Right Masseter",
                      "Left Zygomaticus","Right Zygomaticus"];
    for (i, &label) in emg_labels.iter().enumerate() {
        layout.set_electrode(i, Electrode::emg(label));
    }

    // EEG channels (10-20)
    let eeg_labels = [
        positions::FP1, positions::FP2,
        positions::F3,  positions::F4,
        positions::C3,  positions::CZ,
        positions::C4,  positions::PZ,
        positions::O1,  positions::O2,
    ];
    for (i, &label) in eeg_labels.iter().enumerate() {
        layout.set_electrode(8 + i, Electrode::eeg(label));
    }

    // Auxiliary EMG
    for i in 0..4 {
        layout.set_electrode(18 + i, Electrode {
            label: format!("AUX_EMG{}", i + 1),
            signal_type: SignalType::Emg,
            note: Some("auxiliary".into()),
        });
    }

    let mut board = GaleaBoard::new(&ip).with_electrode_layout(layout);

    println!("Connecting to Galea{}…",
             if ip.is_empty() { " (auto-discover)".into() } else { format!(" at {}", ip) });
    board.prepare()?;

    // Optional: adjust gain for EEG channels
    let eeg_cfg = ChannelConfig::default().gain(Gain::X12);
    let emg_cfg = ChannelConfig::default().gain(Gain::X4);
    for ch in 0..8  { board.apply_channel_config(ch, &emg_cfg)?; }
    for ch in 8..18 { board.apply_channel_config(ch, &eeg_cfg)?; }

    println!("\n=== Galea channel map ===");
    for (i, e) in board.electrode_layout().iter() {
        println!("  Ch{:02}: {:25} ({:?})", i + 1, e.label, e.signal_type);
    }
    println!();

    // ── Stream 2 seconds at 250 Hz ────────────────────────────────────────────
    let target = board.sampling_rate() as usize * 2;
    let stream = board.start_stream()?;
    let mut count = 0usize;

    for sample in stream.into_iter().take(target) {
        count += 1;
        if count % 50 == 0 {
            // Print first EEG (ch 8) and first EMG (ch 0) as a quick check
            println!(
                "[{:3}] t={:.3}  EEG-Fp1={:+7.2} µV  EMG-LFront={:+7.2} µV",
                sample.sample_num,
                sample.timestamp,
                sample.eeg.get(8).copied().unwrap_or(0.0),
                sample.eeg.first().copied().unwrap_or(0.0),
            );
        }
    }

    println!("\nReceived {} samples.", count);
    board.release()?;
    Ok(())
}
