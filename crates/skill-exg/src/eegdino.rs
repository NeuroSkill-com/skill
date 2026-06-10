// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! EEG-DINO foundation model integration (RLX backend via [`eegdino`]).
//!
//! Wraps [`eegdino_rs::EegDinoEncoder`] with HuggingFace weight resolution and
//! channel mapping onto the model's fixed 19-channel montage.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use eegdino_rs::{EegDinoEncoder, EncodingResult, ModelConfig, ModelSize};

/// Default HuggingFace repo with pre-converted safetensors weights.
pub const HF_REPO: &str = "eugenehp/eegdino";

/// International 10–20 montage order expected by EEG-DINO (19 channels).
pub const EEG_DINO_CHANNELS: [&str; 19] = [
    "fp1", "fp2", "f7", "f3", "fz", "f4", "f8", "t7", "c3", "cz", "c4", "t8", "p7", "p3", "pz", "p4", "p8", "o1", "o2",
];

pub fn weights_file(variant: &str) -> &'static str {
    match variant {
        "medium" => "eeg_dino_medium.safetensors",
        "large" => "eeg_dino_large.safetensors",
        _ => "eeg_dino_small.safetensors",
    }
}

pub fn model_size(variant: &str) -> ModelSize {
    match variant {
        "medium" => ModelSize::Medium,
        "large" => ModelSize::Large,
        _ => ModelSize::Small,
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

fn normalize_channel(name: &str) -> String {
    name.to_lowercase().replace(['-', ' '], "")
}

fn channel_index(name: &str) -> Option<usize> {
    let n = normalize_channel(name);
    let aliases: [(&str, &str); 8] = [
        ("t3", "t7"),
        ("t4", "t8"),
        ("t5", "p7"),
        ("t6", "p8"),
        ("t7", "t7"),
        ("t8", "t8"),
        ("p7", "p7"),
        ("p8", "p8"),
    ];
    let key = aliases
        .iter()
        .find(|(a, _)| *a == n.as_str())
        .map(|(_, canon)| *canon)
        .unwrap_or(n.as_str());
    EEG_DINO_CHANNELS.iter().position(|&c| c == key)
}

/// Map arbitrary headset channels onto the fixed 19-channel EEG-DINO layout.
///
/// Unmapped montage slots are zero-filled. Samples are truncated to the largest
/// multiple of `patch_size` (200) that fits in the input.
pub fn prepare_signal(samples: &[Vec<f32>], channel_names: &[&str], patch_size: usize) -> Result<(Vec<f32>, usize)> {
    let n_in = channel_names.len().min(samples.len());
    anyhow::ensure!(n_in > 0, "no channels in epoch");

    let n_samples = samples.iter().take(n_in).map(|s| s.len()).min().unwrap_or(0);
    anyhow::ensure!(
        n_samples >= patch_size,
        "epoch shorter than one EEG-DINO patch ({patch_size} samples)"
    );

    let aligned = n_samples - (n_samples % patch_size);
    let mut out = vec![0.0f32; EEG_DINO_CHANNELS.len() * aligned];

    for (ch_idx, name) in channel_names.iter().take(n_in).enumerate() {
        let Some(slot) = channel_index(name) else {
            continue;
        };
        let src = &samples[ch_idx];
        for t in 0..aligned {
            out[slot * aligned + t] = src.get(t).copied().unwrap_or(0.0);
        }
    }

    Ok((out, aligned))
}

pub struct EegDino {
    inner: EegDinoEncoder,
    embed_dim: usize,
}

impl EegDino {
    pub fn from_files(weights_path: &Path, size: ModelSize, device: rlx::Device) -> Result<Self> {
        let cfg = ModelConfig::from_size(size);
        let embed_dim = cfg.feature_size;
        let (inner, ms) = EegDinoEncoder::load(weights_path, Some(cfg), device)?;
        log::info!("EEG-DINO-{size:?} loaded in {ms:.0} ms");
        Ok(Self { inner, embed_dim })
    }

    pub fn from_hf(repo: &str, variant: &str, device: rlx::Device) -> Result<Self> {
        let wf = weights_file(variant);
        log::info!("Resolving EEG-DINO-{variant} from {repo}...");
        let t0 = Instant::now();
        let weights_path = resolve_hf_file(repo, wf)?;
        log::info!(
            "Resolved in {:.0} ms: weights={}",
            t0.elapsed().as_secs_f64() * 1000.0,
            weights_path.display(),
        );
        Self::from_files(&weights_path, model_size(variant), device)
    }

    pub fn from_default_hf(variant: &str, device: rlx::Device) -> Result<Self> {
        Self::from_hf(HF_REPO, variant, device)
    }

    pub fn encode_raw(&mut self, signal: &[f32], num_channels: usize, num_samples: usize) -> Result<EncodingResult> {
        self.inner
            .encode_raw(signal, 1, num_channels, num_samples)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Mean-pool token embeddings → `[embed_dim]`.
    pub fn encode_pooled(&mut self, samples: &[Vec<f32>], channel_names: &[&str]) -> Result<Vec<f32>> {
        let patch_size = self.inner.cfg.patch_size;
        let (signal, num_samples) = prepare_signal(samples, channel_names, patch_size)?;
        let result = self.encode_raw(&signal, EEG_DINO_CHANNELS.len(), num_samples)?;
        let seq_len = result.shape.get(1).copied().unwrap_or(1);
        let dim = result.shape.get(2).copied().unwrap_or(self.embed_dim);
        let mut pooled = vec![0.0f32; dim];
        for t in 0..seq_len {
            for (d, p) in pooled.iter_mut().enumerate() {
                *p += result.embeddings[t * dim + d];
            }
        }
        let inv = 1.0 / seq_len as f32;
        for p in &mut pooled {
            *p *= inv;
        }
        Ok(pooled)
    }

    pub fn embed_dim(&self) -> usize {
        self.embed_dim
    }
}
