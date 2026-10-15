//! Stream from a Cyton board via the OpenBCI WiFi Shield.
//!
//! Run (auto-discovery):
//! ```sh
//! cargo run --example wifi_stream
//! ```
//!
//! Run (explicit IP):
//! ```sh
//! cargo run --example wifi_stream -- 192.168.1.105 3000
//! ```

use openbci::board::{Board, ConfigurableBoard};
use openbci::board::cyton_wifi::{CytonWifiBoard, CytonWifiConfig};
use openbci::channel_config::{ChannelConfig, Gain};
use openbci::electrode::{Electrode, ElectrodeLayout, positions};

fn main() -> openbci::Result<()> {
    env_logger::init();

    let mut args = std::env::args().skip(1);
    let shield_ip  = args.next().unwrap_or_default();
    let local_port = args.next()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(3000);

    // ── Electrode layout ───────────────────────────────────────────────────────
    let layout = ElectrodeLayout::new(8)
        .with_electrode(0, Electrode::eeg(positions::FP1))
        .with_electrode(1, Electrode::eeg(positions::FP2))
        .with_electrode(2, Electrode::eeg(positions::F3))
        .with_electrode(3, Electrode::eeg(positions::F4))
        .with_electrode(4, Electrode::eeg(positions::C3))
        .with_electrode(5, Electrode::eeg(positions::CZ))
        .with_electrode(6, Electrode::eeg(positions::C4))
        .with_electrode(7, Electrode::eeg(positions::PZ));

    let cfg = CytonWifiConfig {
        shield_ip,
        local_port,
        http_timeout: 10,
    };

    println!("Connecting to Cyton via WiFi Shield{}…",
             if cfg.shield_ip.is_empty() { " (auto-discover)".into() }
             else { format!(" at {}", cfg.shield_ip) });

    let mut board = CytonWifiBoard::new(cfg).with_electrode_layout(layout);
    board.prepare()?;

    // All channels at 24× gain
    board.apply_all_channel_configs(&vec![ChannelConfig::default().gain(Gain::X24); 8])?;

    println!("Connected! Shield IP: {}", board.electrode_layout().label(0)); // just a test print

    println!("\n=== Electrode layout ===");
    for (i, e) in board.electrode_layout().iter() {
        println!("  Ch{:02}: {}", i + 1, e.label);
    }
    println!("\nSampling rate: {} Hz\n", board.sampling_rate());

    // ── Stream 3 seconds ──────────────────────────────────────────────────────
    let target = board.sampling_rate() as usize * 3;
    println!("Streaming {} samples…", target);

    let stream = board.start_stream()?;
    let mut count = 0usize;

    for sample in stream.into_iter().take(target) {
        count += 1;
        if count % 100 == 0 {
            let vals: Vec<String> = sample.eeg.iter().map(|v| format!("{:+7.1}", v)).collect();
            println!("[{:4}] t={:.4}  {}",
                     sample.sample_num, sample.timestamp, vals.join("  "));
        }
    }

    println!("\nTotal samples received: {}", count);
    board.release()?;
    Ok(())
}
