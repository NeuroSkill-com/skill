// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! `skill-eeg` — EEG signal processing extracted from the NeuroSkill monolith.
//!
//! This crate contains:
//!
//! - **constants** — EEG hardware, filter, band, and model constants
//! - **eeg_bands** — band power analysis (FFT-based, GPU-accelerated)
//! - **eeg_filter** — overlap-save signal filter with GPU FFT
//! - **eeg_quality** — signal quality monitoring
//! - **eeg_model_config** — ZUNA model configuration persistence
//! - **artifact_detection** — EEG artifact detection (blink, jaw clench, etc.)
//! - **head_pose** — accelerometer-based head pose tracking

/// Re-export all constants from the canonical `skill-constants` crate so
/// `crate::constants::*` keeps working throughout `skill-eeg`.
pub mod constants {
    pub use skill_constants::*;
}

/// GPU-accelerated FFT via RLX (Metal / CUDA / wgpu / CPU).
#[cfg(feature = "rlx-fft")]
pub mod rlx_fft;

/// Apple Accelerate (vDSP) FFT backend.
#[cfg(target_os = "macos")]
pub mod accel_fft;
/// CPU FFT (rustfft) — the default FFT backend.
pub mod cpu_fft;
/// Selectable FFT backend used by the live DSP (filter, band analysis).
pub mod fft;
#[cfg(test)]
mod proptest_tests;

pub mod artifact_detection;
pub(crate) mod band_metrics;
pub mod eeg_bands;
pub mod eeg_filter;
pub mod eeg_model_config;
pub mod eeg_quality;
pub mod head_pose;
