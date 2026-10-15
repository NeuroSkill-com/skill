//! # openbci
//!
//! A pure-Rust driver for [OpenBCI](https://openbci.com) EEG/EMG boards.
//! No C/C++ runtime, no BrainFlow dependency — speaks directly to the hardware
//! over serial, Bluetooth LE, WiFi, or UDP.
//!
//! ## Supported boards
//!
//! | Board | Channels | Interface | Type |
//! |---|---|---|---|
//! | **Cyton** | 8 EEG | USB serial (FTDI dongle) | [`board::cyton::CytonBoard`] |
//! | **Cyton + Daisy** | 16 EEG | USB serial (FTDI dongle) | [`board::cyton_daisy::CytonDaisyBoard`] |
//! | **Cyton WiFi** | 8 EEG | WiFi Shield → TCP | [`board::cyton_wifi::CytonWifiBoard`] |
//! | **Cyton Daisy WiFi** | 16 EEG | WiFi Shield → TCP | [`board::cyton_daisy_wifi::CytonDaisyWifiBoard`] |
//! | **Ganglion** | 4 EEG | Bluetooth LE (`ble` feature) | [`board::ganglion::GanglionBoard`] |
//! | **Ganglion WiFi** | 4 EEG | WiFi Shield → TCP | [`board::ganglion_wifi::GanglionWifiBoard`] |
//! | **Galea** | 24 EEG+EMG | UDP | [`board::galea::GaleaBoard`] |
//!
//! ## Design overview
//!
//! Every board implements the [`Board`] trait, which follows a simple lifecycle:
//!
//! ```text
//! prepare()  →  [apply_channel_config()]  →  start_stream()  →  [samples]  →  release()
//! ```
//!
//! Cyton-family boards additionally implement [`ConfigurableBoard`], which lets
//! you configure the ADS1299 amplifier gain, input mux, bias network, and SRB
//! connections on a per-channel basis.
//!
//! Electrode placement is handled by [`ElectrodeLayout`], backed by MNE-Python's
//! standard 10-05/10-10/10-20 montage data (3-D head-surface positions for 334
//! named sites).
//!
//! ## Quick start
//!
//! ### Cyton — 8 channels over USB
//!
//! ```rust,no_run
//! use openbci::board::cyton::CytonBoard;
//! use openbci::board::{Board, ConfigurableBoard};
//! use openbci::channel_config::{ChannelConfig, Gain};
//! use openbci::electrode::{ElectrodeLayout, positions};
//!
//! // Map each of the 8 channels to a standard 10-20 electrode site.
//! let layout = ElectrodeLayout::from_labels(&[
//!     positions::FP1, positions::FP2,
//!     positions::C3,  positions::CZ,
//!     positions::C4,  positions::P3,
//!     positions::PZ,  positions::P4,
//! ]);
//!
//! let mut board = CytonBoard::new("/dev/ttyUSB0")   // Windows: "COM3"
//!     .with_electrode_layout(layout);
//!
//! // Open the port and wait for the board's "$$$" ready marker.
//! board.prepare().unwrap();
//!
//! // Optionally reconfigure amplifier gain on every channel.
//! board.apply_all_channel_configs(&vec![ChannelConfig::default(); 8]).unwrap();
//!
//! // Start acquisition — the returned StreamHandle owns a background reader thread.
//! let stream = board.start_stream().unwrap();
//! for sample in stream.into_iter().take(250) {   // ~1 second at 250 Hz
//!     println!(
//!         "t={:.3}s  {} = {:+.1} µV",
//!         sample.timestamp,
//!         board.electrode_layout().label(0),
//!         sample.eeg[0],
//!     );
//! }
//! // `stream` drops here → stop signal sent to background thread.
//! board.release().unwrap();
//! ```
//!
//! ### Cyton + Daisy — 16 channels
//!
//! ```rust,no_run
//! use openbci::board::cyton_daisy::CytonDaisyBoard;
//! use openbci::board::Board;
//! use openbci::electrode::ElectrodeLayout;
//!
//! let mut board = CytonDaisyBoard::new("/dev/ttyUSB0")
//!     .with_electrode_layout(ElectrodeLayout::from_labels(&[
//!         // Cyton  (channels 0–7)
//!         "Fp1","Fp2","F3","F4","C3","Cz","C4","Pz",
//!         // Daisy  (channels 8–15)
//!         "P3","P4","O1","O2","F7","F8","T7","T8",
//!     ]));
//! board.prepare().unwrap();
//!
//! let stream = board.start_stream().unwrap();
//! for sample in stream.into_iter().take(500) {
//!     // sample.eeg has 16 µV values — one per channel.
//!     let peak = sample.eeg.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
//!     println!("sample {} — peak {:+.1} µV", sample.sample_num, peak);
//! }
//! ```
//!
//! ### Ganglion — 4 channels over Bluetooth LE
//!
//! Requires the `ble` Cargo feature (enabled by default) and a Bluetooth
//! adapter.
//!
//! ```rust,no_run
//! #[cfg(feature = "ble")]
//! {
//!     use openbci::board::ganglion::{GanglionBoard, GanglionConfig};
//!     use openbci::board::Board;
//!     use openbci::electrode::ElectrodeLayout;
//!
//!     let mut board = GanglionBoard::new(GanglionConfig::default())
//!         .with_electrode_layout(ElectrodeLayout::from_labels(
//!             &["Fp1", "Fp2", "C3", "C4"]
//!         ));
//!
//!     // Scans BLE for up to 10 seconds (configurable via GanglionConfig).
//!     board.prepare().unwrap();
//!     let stream = board.start_stream().unwrap();
//!     for sample in stream.into_iter().take(400) {
//!         println!("{:?}", sample.eeg);
//!     }
//!     board.release().unwrap();
//! }
//! ```
//!
//! ## Channel configuration
//!
//! The Cyton's ADS1299 amplifier is configurable per-channel.  All
//! Cyton-family boards implement [`ConfigurableBoard`]:
//!
//! ```rust,no_run
//! use openbci::channel_config::{ChannelConfig, Gain, InputType};
//! use openbci::board::ConfigurableBoard;
//! # use openbci::board::cyton::CytonBoard;
//! # use openbci::board::Board;
//! # let mut board = CytonBoard::new("/dev/null");
//! # board.prepare().ok();
//!
//! // 24× gain, normal electrode input, included in bias, SRB2 reference.
//! let eeg = ChannelConfig::default()
//!     .gain(Gain::X24)
//!     .input_type(InputType::Normal)
//!     .bias(true)
//!     .srb2(true);
//!
//! // Shorted input — measures the amplifier's intrinsic noise floor.
//! let noise = ChannelConfig::default()
//!     .input_type(InputType::Shorted)
//!     .bias(false)
//!     .srb2(false);
//!
//! board.apply_channel_config(0, &eeg).unwrap();   // channel 0 only
//! board.apply_channel_config(1, &noise).unwrap();  // channel 1 only
//! board.reset_to_defaults().unwrap();              // "d" — restore factory defaults
//! ```
//!
//! ## Electrode placement and montage
//!
//! [`ElectrodeLayout`] maps channel indices to electrode metadata.
//! The [`electrode::positions`] module contains string constants for every
//! named site in the 10-05/10-10/10-20 systems, sourced from MNE-Python.
//!
//! ```rust
//! use openbci::electrode::{
//!     Electrode, ElectrodeLayout, ElectrodePosition, SignalType,
//!     positions, position, MONTAGE_1020, MONTAGE_1010, MONTAGE_1005,
//! };
//!
//! // Quick construction from label strings.
//! let layout = ElectrodeLayout::from_labels(&[
//!     positions::FP1, positions::FP2,
//!     positions::C3,  positions::CZ,
//!     positions::C4,  positions::P3,
//!     positions::PZ,  positions::P4,
//! ]);
//! assert_eq!(layout.label(2), "C3");
//! assert_eq!(layout.label(7), "P4");
//!
//! // Look up the 3-D head-surface position of an electrode (in metres).
//! let cz: ElectrodePosition = position("Cz").unwrap();
//! assert!(cz.z > 0.09);   // Cz sits near the top of the head
//!
//! // All 83 classic 10-20 sites, 176 10-10 sites, 334 10-05 sites.
//! println!("{} / {} / {} electrodes", MONTAGE_1020.len(), MONTAGE_1010.len(), MONTAGE_1005.len());
//! ```
//!
//! ## The `Sample` type
//!
//! [`Sample`] is returned by every board:
//!
//! | Field | Type | Meaning |
//! |---|---|---|
//! | `sample_num` | `u8` | Rolling counter from the board (0–255) |
//! | `eeg` | `Vec<f64>` | µV per channel |
//! | `accel` | `Option<[f64;3]>` | g (X, Y, Z) when available |
//! | `analog` | `Option<[f64;3]>` | Raw analog pin readings |
//! | `resistance` | `Option<Vec<f64>>` | Ω — Ganglion impedance mode |
//! | `timestamp` | `f64` | Seconds since UNIX epoch (host PC clock) |
//! | `end_byte` | `u8` | `0xC0` = accelerometer mode, `0xC1` = analog mode |
//!
//! ## Cargo features
//!
//! | Feature | Default | Description |
//! |---|---|---|
//! | `ble` | ✅ | Enables [`board::ganglion::GanglionBoard`] via `webbluetooth` + `tokio` |
//!
//! Disable `ble` to avoid the tokio/webbluetooth dependency tree:
//!
//! ```toml
//! [dependencies]
//! openbci = { version = "0.0.1", default-features = false }
//! ```

pub mod board;
pub mod channel_config;
pub mod electrode;
pub mod error;
pub mod packet;
pub mod sample;

#[cfg(test)]
mod tests;

// ── Top-level re-exports ──────────────────────────────────────────────────────

pub use board::{Board, ConfigurableBoard};
pub use channel_config::{ChannelConfig, Gain, GainTracker, InputType};
pub use electrode::{Electrode, ElectrodeLayout, SignalType};
pub use error::{OpenBciError, Result};
pub use sample::{Sample, StreamHandle};
