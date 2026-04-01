// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! `skill-neurorvq` — NeuroRVQ biosignal tokenizer integration for NeuroSkill.
//!
//! Wraps [`neurorvq_rs`] to provide:
//!
//! - **HuggingFace weight resolution** — finds or downloads safetensors weights
//! - **Tokenizer construction** — loads the right model for a given modality
//! - **Batch helpers** — builds input batches from raw signal buffers
//! - **Token extraction** — encode → RVQ → discrete token indices
//!
//! # Backends
//!
//! | Feature   | Backend                          |
//! |-----------|----------------------------------|
//! | `ndarray` | CPU (NdArray + Rayon) — default  |
//! | `metal`   | GPU (wgpu / Metal on macOS)      |
//! | `vulkan`  | GPU (wgpu / Vulkan on Linux/Win) |
//!
//! # Example
//!
//! ```rust,ignore
//! use skill_neurorvq::{NeuroRVQ, Modality};
//!
//! let model = NeuroRVQ::from_hf("eugenehp/NeuroRVQ", Modality::EEG)?;
//! let tokens = model.tokenize(&signal, &channel_names)?;
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};

// Re-export core types from neurorvq-rs
pub use neurorvq_rs::{
    ConfigOverrides, FMEncoderResult, ForwardResult, InputBatch, Modality, NeuroRVQConfig, ReconstructionResult,
    TokenResult,
};

// ── Backend selection ─────────────────────────────────────────────────────────

#[cfg(feature = "ndarray")]
type B = burn_ndarray::NdArray;

#[cfg(feature = "ndarray")]
fn default_device() -> burn_ndarray::NdArrayDevice {
    burn_ndarray::NdArrayDevice::Cpu
}

#[cfg(all(any(feature = "metal", feature = "vulkan"), not(feature = "ndarray")))]
type B = burn_wgpu::Wgpu;

#[cfg(all(any(feature = "metal", feature = "vulkan"), not(feature = "ndarray")))]
fn default_device() -> burn_wgpu::WgpuDevice {
    burn_wgpu::WgpuDevice::DefaultDevice
}

// ── HuggingFace constants ─────────────────────────────────────────────────────

/// Default HuggingFace repo with pre-converted safetensors weights.
pub const HF_REPO: &str = "eugenehp/NeuroRVQ";

/// Weight filenames per modality/type.
pub fn tokenizer_weights_file(modality: Modality) -> &'static str {
    match modality {
        Modality::EEG => "NeuroRVQ_EEG_tokenizer_v1.safetensors",
        Modality::ECG => "NeuroRVQ_ECG_tokenizer_v1.safetensors",
        Modality::EMG => "NeuroRVQ_EMG_tokenizer_v1.safetensors",
    }
}

/// Foundation model weight filenames (EEG and EMG only).
pub fn fm_weights_file(modality: Modality) -> Option<&'static str> {
    match modality {
        Modality::EEG => Some("NeuroRVQ_EEG_foundation_model_v1.safetensors"),
        Modality::EMG => Some("NeuroRVQ_EMG_foundation_model_v1.safetensors"),
        Modality::ECG => None, // No FM released for ECG
    }
}

/// Config flag filenames.
pub fn config_file(modality: Modality) -> &'static str {
    match modality {
        Modality::EEG => "flags/NeuroRVQ_EEG_v1.yml",
        Modality::ECG => "flags/NeuroRVQ_ECG_v1.yml",
        Modality::EMG => "flags/NeuroRVQ_EMG_v1.yml",
    }
}

// ── Weight resolution ─────────────────────────────────────────────────────────

/// Resolve a file from the HuggingFace Hub cache, downloading if needed.
fn resolve_hf_file(repo: &str, filename: &str) -> Result<PathBuf> {
    use hf_hub::api::sync::Api;
    let api = Api::new().context("HuggingFace Hub API init failed")?;
    let repo_handle = api.model(repo.to_string());
    let path = repo_handle
        .get(filename)
        .with_context(|| format!("Failed to resolve {repo}/{filename}"))?;
    Ok(path)
}

// ── NeuroRVQ Tokenizer ────────────────────────────────────────────────────────

/// High-level NeuroRVQ tokenizer for NeuroSkill integration.
///
/// Wraps [`neurorvq_rs::NeuroRVQEncoder`] with HuggingFace weight resolution
/// and convenient batch construction from raw signal buffers.
pub struct NeuroRVQ {
    inner: neurorvq_rs::NeuroRVQEncoder<B>,
}

impl NeuroRVQ {
    /// Load a tokenizer from local config + weights files.
    pub fn from_files(config_path: &Path, weights_path: &Path, modality: Modality) -> Result<Self> {
        let dev = default_device();
        let (inner, ms) =
            neurorvq_rs::NeuroRVQEncoder::<B>::load_with_modality(config_path, weights_path, modality, dev)?;
        log::info!("NeuroRVQ-{modality} loaded in {ms:.0} ms");
        Ok(Self { inner })
    }

    /// Load a tokenizer from HuggingFace Hub (downloads if not cached).
    pub fn from_hf(repo: &str, modality: Modality) -> Result<Self> {
        let weights_file = tokenizer_weights_file(modality);
        let cfg_file = config_file(modality);

        log::info!("Resolving NeuroRVQ-{modality} from {repo}...");
        let t0 = Instant::now();

        let config_path = resolve_hf_file(repo, cfg_file)?;
        let weights_path = resolve_hf_file(repo, weights_file)?;

        log::info!(
            "Resolved in {:.0} ms: config={}, weights={}",
            t0.elapsed().as_secs_f64() * 1000.0,
            config_path.display(),
            weights_path.display(),
        );

        Self::from_files(&config_path, &weights_path, modality)
    }

    /// Load from the default HuggingFace repo ([`HF_REPO`]).
    pub fn from_default_hf(modality: Modality) -> Result<Self> {
        Self::from_hf(HF_REPO, modality)
    }

    /// Load with optional config overrides.
    pub fn from_hf_with_overrides(repo: &str, modality: Modality, overrides: &ConfigOverrides) -> Result<Self> {
        let weights_file = tokenizer_weights_file(modality);
        let cfg_file = config_file(modality);

        let config_path = resolve_hf_file(repo, cfg_file)?;
        let weights_path = resolve_hf_file(repo, weights_file)?;

        let dev = default_device();
        let (inner, ms) =
            neurorvq_rs::NeuroRVQEncoder::<B>::load_full(&config_path, &weights_path, modality, Some(overrides), dev)?;
        log::info!("NeuroRVQ-{modality} loaded in {ms:.0} ms (with overrides)");
        Ok(Self { inner })
    }

    /// Tokenize a raw signal buffer.
    ///
    /// `signal`: flat `f32` buffer of shape `[n_channels × n_samples]`
    /// `channel_names`: channel labels (e.g. `["fp1", "fp2", "c3", "c4"]`)
    ///
    /// Returns token indices for all 4 branches × 8 (or 16) RVQ levels.
    pub fn tokenize(&self, signal: &[f32], channel_names: &[&str]) -> Result<TokenResult> {
        let modality = self.inner.modality;
        let config = &self.inner.config;
        let n_channels = channel_names.len();
        let n_samples = signal.len() / n_channels;
        let n_time = neurorvq_rs::compute_n_time(config.n_patches, n_channels);

        anyhow::ensure!(
            n_samples == n_time * config.patch_size,
            "Signal length mismatch: got {} samples per channel, expected {} (n_time={} × patch_size={})",
            n_samples,
            n_time * config.patch_size,
            n_time,
            config.patch_size,
        );

        let dev = self.inner.device();
        let batch = neurorvq_rs::build_batch_with_modality(
            signal.to_vec(),
            channel_names,
            n_time,
            config.n_patches,
            n_channels,
            n_samples,
            modality,
            dev,
        );

        self.inner.tokenize(&batch)
    }

    /// Encode + quantize + decode → reconstructed FFT components.
    pub fn reconstruct(&self, signal: &[f32], channel_names: &[&str]) -> Result<ReconstructionResult> {
        let batch = self.build_batch(signal, channel_names)?;
        self.inner.reconstruct(&batch)
    }

    /// Full forward: encode → decode → iFFT → standardized signals.
    pub fn forward(&self, signal: &[f32], channel_names: &[&str]) -> Result<ForwardResult> {
        let batch = self.build_batch(signal, channel_names)?;
        self.inner.forward(&batch)
    }

    /// The loaded modality.
    pub fn modality(&self) -> Modality {
        self.inner.modality
    }

    /// The model configuration.
    pub fn config(&self) -> &NeuroRVQConfig {
        &self.inner.config
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    fn build_batch(&self, signal: &[f32], channel_names: &[&str]) -> Result<InputBatch<B>> {
        let config = &self.inner.config;
        let n_channels = channel_names.len();
        let n_samples = signal.len() / n_channels;
        let n_time = neurorvq_rs::compute_n_time(config.n_patches, n_channels);

        anyhow::ensure!(
            n_samples == n_time * config.patch_size,
            "Signal length mismatch: got {} samples/ch, expected {}",
            n_samples,
            n_time * config.patch_size,
        );

        let dev = self.inner.device();
        Ok(neurorvq_rs::build_batch_with_modality(
            signal.to_vec(),
            channel_names,
            n_time,
            config.n_patches,
            n_channels,
            n_samples,
            self.inner.modality,
            dev,
        ))
    }
}

// ── NeuroRVQ Foundation Model ─────────────────────────────────────────────────

/// High-level NeuroRVQ Foundation Model for NeuroSkill integration.
pub struct NeuroRVQFM {
    inner: neurorvq_rs::NeuroRVQFoundationModel<B>,
}

impl NeuroRVQFM {
    /// Load a foundation model from local files.
    pub fn from_files(config_path: &Path, weights_path: &Path, modality: Modality) -> Result<Self> {
        let dev = default_device();
        let (inner, ms) = neurorvq_rs::NeuroRVQFoundationModel::<B>::load(config_path, weights_path, modality, dev)?;
        log::info!("NeuroRVQ-FM-{modality} loaded in {ms:.0} ms");
        Ok(Self { inner })
    }

    /// Load from HuggingFace Hub.
    pub fn from_hf(repo: &str, modality: Modality) -> Result<Self> {
        let weights_file =
            fm_weights_file(modality).with_context(|| format!("No foundation model available for {modality}"))?;
        let cfg_file = config_file(modality);

        let config_path = resolve_hf_file(repo, cfg_file)?;
        let weights_path = resolve_hf_file(repo, weights_file)?;

        Self::from_files(&config_path, &weights_path, modality)
    }

    /// Load from the default HuggingFace repo.
    pub fn from_default_hf(modality: Modality) -> Result<Self> {
        Self::from_hf(HF_REPO, modality)
    }

    /// Encode → 4 branch feature vectors.
    pub fn encode(&self, signal: &[f32], channel_names: &[&str]) -> Result<FMEncoderResult> {
        let batch = self.build_batch(signal, channel_names)?;
        self.inner.encode(&batch)
    }

    /// Encode → concat → mean-pool → single representation vector.
    pub fn encode_pooled(&self, signal: &[f32], channel_names: &[&str]) -> Result<Vec<f32>> {
        let batch = self.build_batch(signal, channel_names)?;
        self.inner.encode_pooled(&batch)
    }

    fn build_batch(&self, signal: &[f32], channel_names: &[&str]) -> Result<InputBatch<B>> {
        let config = &self.inner.config;
        let n_channels = channel_names.len();
        let n_samples = signal.len() / n_channels;
        let n_time = neurorvq_rs::compute_n_time(config.n_patches, n_channels);

        anyhow::ensure!(
            n_samples == n_time * config.patch_size,
            "Signal length mismatch: got {} samples/ch, expected {}",
            n_samples,
            n_time * config.patch_size,
        );

        let dev = self.inner.device();
        Ok(neurorvq_rs::build_batch_with_modality(
            signal.to_vec(),
            channel_names,
            n_time,
            config.n_patches,
            n_channels,
            n_samples,
            self.inner.modality,
            dev,
        ))
    }
}

// ── Channel helpers re-exports ────────────────────────────────────────────────

pub use neurorvq_rs::{channel_indices, compute_n_time, global_channels, ECG_CHANNELS, EEG_CHANNELS, EMG_CHANNELS};
