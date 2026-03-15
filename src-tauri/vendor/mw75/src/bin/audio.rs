//! MW75 audio player — automatic Bluetooth connection and music playback.
//!
//! Usage:
//!   cargo run --bin mw75-audio --features audio -- song.mp3
//!   cargo run --bin mw75-audio --features audio -- --volume 0.5 album/*.flac
//!
//! What it does (fully automatic, no user interaction):
//!   1. Discovers MW75 headphones via BlueZ
//!   2. Pairs if not already paired
//!   3. Connects A2DP audio profile
//!   4. Sets MW75 as the default audio output (PipeWire/PulseAudio)
//!   5. Plays the specified audio file(s) through the headphones
//!   6. Restores previous audio output on exit
//!
//! Supported formats: MP3, WAV, FLAC, OGG/Vorbis

use anyhow::Result;
use log::info;

use mw75::audio::{AudioConfig, Mw75Audio};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().skip(1).collect();

    // Parse simple flags
    let mut volume: f32 = 0.8;
    let mut files: Vec<String> = Vec::new();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--volume" | "-v" => {
                i += 1;
                if i < args.len() {
                    volume = args[i]
                        .parse()
                        .unwrap_or_else(|_| {
                            eprintln!("Invalid volume '{}', using 0.8", args[i]);
                            0.8
                        });
                }
            }
            "--help" | "-h" => {
                println!("Usage: mw75-audio [OPTIONS] <FILE>...");
                println!();
                println!("Automatically connects MW75 headphones and plays audio files.");
                println!();
                println!("Options:");
                println!("  -v, --volume <0.0-1.0>  Playback volume (default: 0.8)");
                println!("  -h, --help              Show this help");
                println!();
                println!("Supported formats: MP3, WAV, FLAC, OGG/Vorbis");
                println!();
                println!("Examples:");
                println!("  mw75-audio song.mp3");
                println!("  mw75-audio --volume 0.5 track1.flac track2.flac");
                println!("  mw75-audio music/*.mp3");
                return Ok(());
            }
            _ => {
                files.push(args[i].clone());
            }
        }
        i += 1;
    }

    if files.is_empty() {
        eprintln!("Error: No audio files specified.");
        eprintln!("Usage: mw75-audio [--volume 0.8] <FILE>...");
        eprintln!("Run 'mw75-audio --help' for more information.");
        std::process::exit(1);
    }

    // Validate files exist before connecting
    for f in &files {
        if !std::path::Path::new(f).exists() {
            eprintln!("Error: File not found: {f}");
            std::process::exit(1);
        }
    }

    let config = AudioConfig {
        volume,
        ..AudioConfig::default()
    };

    let mut audio = Mw75Audio::new(config);

    // ── Connect ──────────────────────────────────────────────────────────────
    info!("Connecting to MW75 headphones…");
    let device = audio.connect().await?;
    info!("✅ Connected to {} [{}]", device.name, device.address);

    if let Some(ref sink) = device.sink_name {
        info!("🔊 Audio sink: {sink}");
    }

    // ── Play files ───────────────────────────────────────────────────────────
    for (i, file) in files.iter().enumerate() {
        info!("▶  Playing [{}/{}]: {file}", i + 1, files.len());
        if let Err(e) = audio.play_file(file).await {
            eprintln!("Error playing {file}: {e}");
        }
    }

    // ── Disconnect ───────────────────────────────────────────────────────────
    info!("Disconnecting…");
    audio.disconnect().await?;
    info!("👋 Done.");

    Ok(())
}
