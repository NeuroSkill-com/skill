//! # eegdino-rs
//!
//! Rust inference crate for the
//! [EEG-DINO](https://github.com/miraclefish/EEG-DINO) foundation model,
//! built on [RLX](https://github.com/eugenehp/rlx).
//!
//! EEG-DINO learns robust EEG representations via hierarchical self-distillation
//! on 9 000+ hours of EEG data.  This crate provides a faithful port of the
//! encoder architecture with verified numerical parity (NRMSE < 1e-6) against
//! the original PyTorch implementation on CPU, Metal, and MLX backends.
//!
//! ## Model sizes
//!
//! | Variant | Params | d_model | Heads | Layers | FFN dim |
//! |---------|--------|---------|-------|--------|---------|
//! | Small   | 4.6 M  | 200     | 8     | 12     | 512     |
//! | Medium  | 33 M   | 512     | 16    | 16     | 1 024   |
//! | Large   | 201 M  | 1 024   | 16    | 24     | 2 048   |
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use eegdino_rs::prelude::*;
//!
//! let device = parse_device("metal")?; // cpu | metal | mlx | gpu
//! let (mut encoder, load_ms) = EegDinoEncoder::load(
//!     "weights/eeg_dino_small.safetensors".as_ref(),
//!     None,
//!     device,
//! )?;
//!
//! let signal = vec![0.0f32; 19 * 2000];
//! let result = encoder.encode_raw(&signal, 1, 19, 2000)?;
//! // result.shape == [1, 191, 200]
//! ```
//!
//! ## Backends
//!
//! | Feature / device | Backend | Notes |
//! |------------------|---------|-------|
//! | `cpu`, `rlx-cpu` | RLX CPU | Rayon + SIMD; default |
//! | `metal` | Apple Metal / MPS | macOS |
//! | `mlx` | Apple MLX | macOS |
//! | `gpu`, `wgpu` | RLX wgpu | Metal/Vulkan/DX12 (parity vs CPU in progress) |
//!
//! Enable all with `--features all-backends`.

#[cfg(not(feature = "rlx"))]
compile_error!("the `rlx` feature is required (enabled by default)");

pub mod config;
pub mod error;
pub mod prelude;
pub mod rlx;

pub use config::{ModelConfig, ModelSize};
pub use error::{EegDinoError, Result};

pub use rlx::{
    detect_model_size, device_label, feature_for, is_device_available, parse_device,
    EegDinoEncoder, EegDinoEncoderBuilder, EncodingResult,
};

#[cfg(feature = "burn")]
pub mod model;

#[cfg(feature = "burn")]
pub(crate) mod weights;

#[cfg(feature = "burn")]
pub mod inference;

#[cfg(feature = "burn")]
pub use inference::{
    ClassificationResult, EegDinoClassifier, EegDinoEncoder as BurnEegDinoEncoder,
    EegDinoEncoderBuilder as BurnEegDinoEncoderBuilder, EncodingResult as BurnEncodingResult,
};

#[cfg(feature = "burn")]
pub use model::classifier::ClassificationModel;

#[cfg(feature = "burn")]
pub use model::encoder::EEGEncoder;

#[cfg(feature = "burn")]
pub use model::embedding::{EmbeddingCache, PatchEmbedding};

/// Configure the Rayon thread pool.  Call once before model use.
pub fn init_threads(n: Option<usize>) {
    let mut builder = rayon::ThreadPoolBuilder::new();
    if let Some(n) = n {
        builder = builder.num_threads(n);
    }
    builder.build_global().ok();
}
