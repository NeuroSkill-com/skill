//! # mw75
//!
//! Async Rust library for streaming EEG data from
//! [Master & Dynamic MW75 Neuro](https://www.masterdynamic.com/) headphones
//! over Bluetooth.
//!
//! ## Device overview
//!
//! The MW75 Neuro headphones contain a 12-channel EEG sensor array developed
//! by [Arctop](https://arctop.com).  The connection lifecycle has two phases:
//!
//! 1. **BLE activation** — discover the device via BLE, write activation
//!    commands (enable EEG, enable raw mode, query battery), and verify
//!    responses on the status characteristic.
//! 2. **RFCOMM data streaming** — after activation the device broadcasts
//!    63-byte EEG packets at 500 Hz over Bluetooth Classic RFCOMM channel 25.
//!
//! ## Quick start
//!
//! ```no_run
//! use mw75::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = Mw75Client::new(Mw75ClientConfig::default());
//!     let (mut rx, handle) = client.connect().await?;
//!     handle.start().await?;
//!
//!     while let Some(event) = rx.recv().await {
//!         match event {
//!             Mw75Event::Eeg(pkt) => {
//!                 println!("EEG counter={} channels={:?}", pkt.counter, &pkt.channels[..4]);
//!             }
//!             Mw75Event::Disconnected => break,
//!             _ => {}
//!         }
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## RFCOMM streaming (`rfcomm` feature)
//!
//! Enable the `rfcomm` feature to receive real EEG data from hardware:
//!
//! ```toml
//! [dependencies]
//! mw75 = { version = "0.1.0", features = ["rfcomm"] }
//! ```
//!
//! ```ignore
//! use mw75::prelude::*;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = Mw75Client::new(Mw75ClientConfig::default());
//!     let (mut rx, handle) = client.connect().await?;
//!     handle.start().await?;
//!
//!     // Disconnect BLE first (required on macOS, recommended on Linux)
//!     let bt_address = handle.peripheral_id();
//!     handle.disconnect_ble().await?;
//!
//!     // Start RFCOMM reader — data arrives as Mw75Event::Eeg on `rx`
//!     let handle = Arc::new(handle);
//!     let rfcomm = start_rfcomm_stream(handle.clone(), &bt_address).await?;
//!
//!     while let Some(event) = rx.recv().await {
//!         if let Mw75Event::Eeg(pkt) = event {
//!             println!("ch1={:.1} µV", pkt.channels[0]);
//!         }
//!     }
//!     rfcomm.abort();
//!     Ok(())
//! }
//! ```
//!
//! ## Simulation mode
//!
//! No hardware needed — use [`simulate::spawn_simulator`] to generate
//! realistic synthetic EEG packets at 500 Hz:
//!
//! ```no_run
//! use mw75::simulate::spawn_simulator;
//! use mw75::types::Mw75Event;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let (tx, mut rx) = tokio::sync::mpsc::channel(256);
//! let sim = spawn_simulator(tx, true);  // deterministic sinusoidal data
//!
//! while let Some(event) = rx.recv().await {
//!     if let Mw75Event::Eeg(pkt) = event {
//!         println!("ch1={:.1} µV", pkt.channels[0]);
//!     }
//! }
//! # }
//! ```
//!
//! ## Audio playback (Linux, `audio` feature)
//!
//! Automatically connect the MW75 as a Bluetooth audio device and play music:
//!
//! ```toml
//! [dependencies]
//! mw75 = { version = "0.1.0", features = ["audio"] }
//! ```
//!
//! ```ignore
//! use mw75::audio::{Mw75Audio, AudioConfig};
//!
//! let mut audio = Mw75Audio::new(AudioConfig::default());
//! let device = audio.connect().await?;      // pair + A2DP + set sink
//! audio.play_file("song.mp3").await?;        // play through headphones
//! audio.disconnect().await?;                 // restore previous sink
//! ```
//!
//! ## Cargo features
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `tui` | ✓ | Terminal UI binary (`mw75-tui`) with ratatui + crossterm |
//! | `rfcomm` | ✗ | RFCOMM data transport (Linux: BlueZ, macOS: IOBluetooth, Windows: WinRT) |
//! | `audio` | ✗ | Bluetooth A2DP audio + rodio playback (Linux only) |
//!
//! ## Module overview
//!
//! | Module | Purpose |
//! |---|---|
//! | [`prelude`] | One-line glob import of the most commonly needed types |
//! | [`mw75_client`] | BLE scanning, connecting, activation, and the [`mw75_client::Mw75Handle`] command API |
//! | [`types`] | All event and data types: [`EegPacket`](types::EegPacket), [`Mw75Event`](types::Mw75Event), etc. |
//! | [`protocol`] | GATT UUIDs, command bytes, and wire-format constants |
//! | [`parse`] | Packet parsing, checksum validation, and EEG sample decoding |
//! | [`simulate`] | Synthetic packet generation and simulator task for testing / TUI |
//! | [`rfcomm`] | RFCOMM transport — platform-specific BT Classic data streaming (`rfcomm` feature) |
//! | [`audio`] | Bluetooth A2DP audio connection and playback (`audio` feature, Linux only) |
//!
//! ## Platform support
//!
//! | Capability | Linux | macOS | Windows |
//! |-----------|-------|-------|---------|
//! | BLE activation | ✓ (btleplug/BlueZ) | ✓ (btleplug/CoreBluetooth) | ✓ (btleplug/WinRT) |
//! | RFCOMM streaming | ✓ (bluer) | ✓ (IOBluetooth) | ✓ (WinRT) |
//! | A2DP audio | ✓ (bluer + pactl) | ✗ | ✗ |
//! | Simulation | ✓ | ✓ | ✓ |
//! | TUI | ✓ | ✓ | ✓ |

pub mod mw75_client;
pub mod parse;
pub mod protocol;
pub mod simulate;
pub mod types;

/// RFCOMM transport — platform-specific Bluetooth Classic data streaming.
///
/// After BLE activation, the MW75 streams EEG packets over RFCOMM channel 25.
/// This module provides the async RFCOMM socket connection and reader loop.
///
/// **Requires the `rfcomm` Cargo feature.**
///
/// | Platform | Backend |
/// |----------|---------|
/// | Linux    | BlueZ (`bluer::rfcomm`) |
/// | macOS    | IOBluetooth (`objc2-io-bluetooth`) |
/// | Windows  | WinRT (`windows` crate) |
///
/// See [`rfcomm::start_rfcomm_stream`] for usage.
#[cfg(feature = "rfcomm")]
pub mod rfcomm;

/// Bluetooth A2DP audio management — automatic pairing, connection, sink
/// routing, and file playback.
///
/// **Linux only.** Requires the `audio` Cargo feature and a working
/// BlueZ + PipeWire/PulseAudio stack.
///
/// ```toml
/// [dependencies]
/// mw75 = { version = "0.1.0", features = ["audio"] }
/// ```
///
/// See [`audio::Mw75Audio`] for usage.
#[cfg(feature = "audio")]
pub mod audio;

// ── Prelude ───────────────────────────────────────────────────────────────────

/// Convenience re-exports for downstream crates.
///
/// ```no_run
/// use mw75::prelude::*;
/// ```
pub mod prelude {
    // ── Client ────────────────────────────────────────────────────────────────
    pub use crate::mw75_client::{Mw75Client, Mw75ClientConfig, Mw75Device, Mw75Handle};

    // ── Events and data types ─────────────────────────────────────────────────
    pub use crate::types::{
        ActivationStatus, BatteryInfo, ChecksumStats, EegPacket, Mw75Event,
    };

    // ── Protocol constants ────────────────────────────────────────────────────
    pub use crate::protocol::{
        EEG_CHANNEL_NAMES, EEG_SCALING_FACTOR, NUM_EEG_CHANNELS, PACKET_SIZE, SYNC_BYTE,
        SampleRate,
    };

    // ── GAIA protocol ─────────────────────────────────────────────────────────
    pub use crate::protocol::{
        build_gaia_command, parse_gaia_response,
        GaiaAncMode, GaiaAutoOff, GaiaCommand, GaiaEqPreset, GaiaResponse,
        GAIA_QUERY, GAIA_VENDOR_MD,
    };

    // ── Simulation ────────────────────────────────────────────────────────────
    pub use crate::simulate::{build_eeg_packet, build_sim_packet, spawn_simulator, spawn_simulator_with_rate};

    // ── RFCOMM transport ──────────────────────────────────────────────────────
    #[cfg(feature = "rfcomm")]
    pub use crate::rfcomm::{start_rfcomm_stream, RfcommHandle};
}
