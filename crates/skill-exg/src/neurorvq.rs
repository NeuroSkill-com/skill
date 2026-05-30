// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! NeuroRVQ biosignal tokenizer/foundation-model integration.
//!
//! Wraps [`neurorvq_rs`] to provide:
//! - HuggingFace weight resolution
//! - tokenization helpers
//! - foundation model encoding helpers

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};

pub use neurorvq_rs::{
    ConfigOverrides, FMEncoderResult, ForwardResult, Modality, NeuroRVQConfig, ReconstructionResult, RlxInputBatch,
    TokenResult,
};

/// Default HuggingFace repo with pre-converted safetensors weights.
pub const HF_REPO: &str = "eugenehp/NeuroRVQ";

pub fn tokenizer_weights_file(modality: Modality) -> &'static str {
    match modality {
        Modality::EEG => "NeuroRVQ_EEG_tokenizer_v1.safetensors",
        Modality::ECG => "NeuroRVQ_ECG_tokenizer_v1.safetensors",
        Modality::EMG => "NeuroRVQ_EMG_tokenizer_v1.safetensors",
    }
}

pub fn fm_weights_file(modality: Modality) -> Option<&'static str> {
    match modality {
        Modality::EEG => Some("NeuroRVQ_EEG_foundation_model_v1.safetensors"),
        Modality::EMG => Some("NeuroRVQ_EMG_foundation_model_v1.safetensors"),
        Modality::ECG => None,
    }
}

pub fn config_file(modality: Modality) -> &'static str {
    match modality {
        Modality::EEG => "flags/NeuroRVQ_EEG_v1.yml",
        Modality::ECG => "flags/NeuroRVQ_ECG_v1.yml",
        Modality::EMG => "flags/NeuroRVQ_EMG_v1.yml",
    }
}

fn resolve_hf_file(repo: &str, filename: &str) -> Result<PathBuf> {
    use hf_hub::api::sync::Api;
    let api = Api::new().context("HuggingFace Hub API init failed")?;
    let repo_handle = api.model(repo.to_string());
    let path = repo_handle
        .get(filename)
        .with_context(|| format!("Failed to resolve {repo}/{filename}"))?;
    Ok(path)
}

pub struct NeuroRVQ {
    inner: neurorvq_rs::NeuroRVQEncoder,
}

impl NeuroRVQ {
    pub fn from_files(
        config_path: &Path,
        weights_path: &Path,
        modality: Modality,
        device: rlx::Device,
    ) -> Result<Self> {
        let (inner, ms) =
            neurorvq_rs::NeuroRVQEncoder::load_with_modality(config_path, weights_path, modality, device)?;
        log::info!("NeuroRVQ-{modality} loaded in {ms:.0} ms");
        Ok(Self { inner })
    }

    pub fn from_hf(repo: &str, modality: Modality, device: rlx::Device) -> Result<Self> {
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

        Self::from_files(&config_path, &weights_path, modality, device)
    }

    pub fn from_default_hf(modality: Modality, device: rlx::Device) -> Result<Self> {
        Self::from_hf(HF_REPO, modality, device)
    }

    pub fn from_hf_with_overrides(
        repo: &str,
        modality: Modality,
        overrides: &ConfigOverrides,
        device: rlx::Device,
    ) -> Result<Self> {
        let weights_file = tokenizer_weights_file(modality);
        let cfg_file = config_file(modality);

        let config_path = resolve_hf_file(repo, cfg_file)?;
        let weights_path = resolve_hf_file(repo, weights_file)?;

        let (inner, ms) =
            neurorvq_rs::NeuroRVQEncoder::load_full(&config_path, &weights_path, modality, Some(overrides), device)?;
        log::info!("NeuroRVQ-{modality} loaded in {ms:.0} ms (with overrides)");
        Ok(Self { inner })
    }

    pub fn tokenize(&mut self, signal: &[f32], channel_names: &[&str]) -> Result<TokenResult> {
        let batch = self.build_batch(signal, channel_names)?;
        self.inner.tokenize(&batch)
    }

    pub fn reconstruct(&mut self, signal: &[f32], channel_names: &[&str]) -> Result<ReconstructionResult> {
        let batch = self.build_batch(signal, channel_names)?;
        self.inner.reconstruct(&batch)
    }

    pub fn forward(&mut self, signal: &[f32], channel_names: &[&str]) -> Result<ForwardResult> {
        let batch = self.build_batch(signal, channel_names)?;
        self.inner.forward(&batch)
    }

    pub fn modality(&self) -> Modality {
        self.inner.modality
    }

    pub fn config(&self) -> &NeuroRVQConfig {
        &self.inner.config
    }

    fn build_batch(&self, signal: &[f32], channel_names: &[&str]) -> Result<RlxInputBatch> {
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

        Ok(neurorvq_rs::build_batch(
            signal.to_vec(),
            channel_names,
            n_time,
            config.n_patches,
            n_channels,
            n_samples,
            self.inner.modality,
        ))
    }
}

pub struct NeuroRVQFM {
    inner: neurorvq_rs::NeuroRVQFoundationModel,
}

impl NeuroRVQFM {
    pub fn from_files(
        config_path: &Path,
        weights_path: &Path,
        modality: Modality,
        device: rlx::Device,
    ) -> Result<Self> {
        let (inner, ms) = neurorvq_rs::NeuroRVQFoundationModel::load(config_path, weights_path, modality, device)?;
        log::info!("NeuroRVQ-FM-{modality} loaded in {ms:.0} ms");
        Ok(Self { inner })
    }

    pub fn from_hf(repo: &str, modality: Modality, device: rlx::Device) -> Result<Self> {
        let weights_file =
            fm_weights_file(modality).with_context(|| format!("No foundation model available for {modality}"))?;
        let cfg_file = config_file(modality);

        let config_path = resolve_hf_file(repo, cfg_file)?;
        let weights_path = resolve_hf_file(repo, weights_file)?;

        Self::from_files(&config_path, &weights_path, modality, device)
    }

    pub fn from_default_hf(modality: Modality, device: rlx::Device) -> Result<Self> {
        Self::from_hf(HF_REPO, modality, device)
    }

    pub fn encode(&mut self, signal: &[f32], channel_names: &[&str]) -> Result<FMEncoderResult> {
        let batch = self.build_batch(signal, channel_names)?;
        self.inner.encode(&batch)
    }

    /// Mean-pool branch features across the sequence dimension and concatenate.
    /// Output shape: `[4 * embed_dim]` — mirrors the old Burn `encode_pooled`.
    pub fn encode_pooled(&mut self, signal: &[f32], channel_names: &[&str]) -> Result<Vec<f32>> {
        let result = self.encode(signal, channel_names)?;
        let seq_len = result.shape.get(1).copied().unwrap_or(1);
        let embed_dim = result.shape.get(2).copied().unwrap_or(1);
        let mut pooled = Vec::with_capacity(4 * embed_dim);
        for branch in &result.branch_features {
            let mut mean = vec![0f32; embed_dim];
            for s in 0..seq_len {
                for e in 0..embed_dim {
                    mean[e] += branch[s * embed_dim + e];
                }
            }
            let inv = 1.0 / seq_len as f32;
            for v in &mut mean {
                *v *= inv;
            }
            pooled.extend_from_slice(&mean);
        }
        Ok(pooled)
    }

    fn build_batch(&self, signal: &[f32], channel_names: &[&str]) -> Result<RlxInputBatch> {
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

        Ok(neurorvq_rs::build_batch(
            signal.to_vec(),
            channel_names,
            n_time,
            config.n_patches,
            n_channels,
            n_samples,
            self.inner.modality,
        ))
    }
}

pub use neurorvq_rs::{channel_indices, compute_n_time, global_channels, ECG_CHANNELS, EEG_CHANNELS, EMG_CHANNELS};
