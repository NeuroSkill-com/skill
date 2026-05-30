// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Workspace wrapper around [`rlx_neutts`] — adds NPY I/O, reference-code cache,
//! HuggingFace download helpers, espeak phonemisation, and text preprocessing
//! that `rlx_neutts 0.2.0` does not yet ship.

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod download;

pub mod cache;
pub mod model;
pub mod npy;
pub mod phonemize;
pub mod preprocess;

/// Re-exports of codec types under the `neutts::codec` namespace that
/// `skill-tts` expects (mirrors the old `neutts` crate module layout).
pub mod codec {
    pub use rlx_neutts::{
        NeuCodecDecoder, NeuCodecEncoder, ENCODER_DEFAULT_INPUT_SAMPLES, ENCODER_SAMPLES_PER_TOKEN,
        ENCODER_SAMPLE_RATE, SAMPLES_PER_TOKEN, SAMPLE_RATE,
    };
}

pub use cache::{CacheOutcome, RefCodeCache};
pub use model::NeuTTS;

// Root-level re-exports for convenience.
pub use rlx_neutts::{GenerationConfig, NeuCodecDecoder, NeuCodecEncoder, SAMPLE_RATE};
