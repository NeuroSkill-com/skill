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

pub mod constants;
pub mod eeg_bands;
pub mod eeg_filter;
pub mod eeg_quality;
pub mod eeg_model_config;
pub mod artifact_detection;
pub mod head_pose;
