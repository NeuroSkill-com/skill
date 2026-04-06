//! # brainmaster
//!
//! Rust library for **BrainMaster** EEG amplifiers via USB serial.
//!
//! Supports the **Atlantis** (2/4-channel), **Discovery** (24-channel),
//! and **Freedom** (24-channel wireless) neurofeedback amplifiers from
//! [BrainMaster Technologies](https://brainmaster.com/).
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use brainmaster::prelude::*;
//!
//! let ports = BrainMasterDevice::scan()?;
//! let mut device = BrainMasterDevice::open(&ports[0], DeviceModel::Atlantis4)?;
//! device.start_streaming()?;
//!
//! for _ in 0..1000 {
//!     let sample = device.read_sample()?;
//!     println!("EEG: {:?}", &sample.channels[..device.channel_count()]);
//! }
//!
//! device.stop_streaming()?;
//! ```
//!
//! ## Protocol
//!
//! BrainMaster amplifiers communicate over USB-serial (FTDI/CDC) at 57600 baud.
//! The binary packet protocol uses a sync byte header followed by channel data
//! as 16-bit signed integers (big-endian), with a checksum trailer.
//!
//! ## Module overview
//!
//! | Module | Purpose |
//! |---|---|
//! | [`device`] | High-level device API: scan, open, stream, read |
//! | [`protocol`] | Packet parsing, sync detection, checksum validation |
//! | [`error`] | Error types |
//! | [`prelude`] | Convenience re-exports |

pub mod device;
pub mod error;
pub mod protocol;

pub mod prelude {
    pub use crate::device::{
        BrainMasterDevice, DeviceModel, EegSample, ATLANTIS2_CHANNELS, ATLANTIS4_CHANNELS, DISCOVERY_CHANNELS,
        FREEDOM_CHANNELS,
    };
    pub use crate::error::BrainMasterError;
}
