//! # reve-rs — REVE EEG Foundation Model inference in Rust
//!
//! Pure-Rust inference for the REVE (Representation for EEG with Versatile
//! Embeddings) foundation model, built on [RLX](https://github.com/eugenehp/rlx)
//! (`rlx-cpu` by default; optional `rlx-metal`, `rlx-mlx`, …).
//!
//! REVE generalizes across diverse electrode configurations using 4D positional
//! encoding (x, y, z, t). It was pretrained on 60,000+ hours of EEG data from
//! 92 datasets spanning 25,000 subjects.
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use reve_rs::ReveEncoder;
//! use rlx::Device;
//! use std::path::Path;
//!
//! let (mut model, _ms) = ReveEncoder::load(
//!     Path::new("data/config.json"),
//!     Path::new("data/model.safetensors"),
//!     Device::Cpu,
//! )?;
//! let out = model.run_one(signal, positions_xyz, n_channels, n_times)?;
//! ```

#[cfg(not(feature = "rlx"))]
compile_error!("enable the `rlx` feature (enabled by default)");

pub mod config;

#[cfg(feature = "rlx")]
pub mod rlx;

pub mod position_bank;

pub use config::ModelConfig;
pub use position_bank::PositionBank;

#[cfg(feature = "rlx")]
pub use rlx::{EncodingResult, ReveEncoder, ReveOutput};
