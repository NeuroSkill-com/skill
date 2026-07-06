// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! LuMamba EEG foundation-model integration (RLX backend via [`lumamba`]).
//!
//! LuMamba reuses LUNA's topology-invariant front-end (cross-attention channel
//! unification) and replaces the rotary Transformer encoder with a stack of
//! bidirectional Mamba (FEMBA) blocks.  This wrapper resolves HuggingFace
//! weights, maps arbitrary headset channels onto LuMamba's electrode-position +
//! vocabulary inputs, and mean-pools the encoder latent into a single
//! embedding vector.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use lumamba::LuMambaEncoder;

/// Default HuggingFace repo with the released LuMamba checkpoints.
pub const HF_REPO: &str = "PulpBio/LuMamba";

/// Config filename shipped alongside the weights in [`HF_REPO`].
pub const CONFIG_FILE: &str = "config.json";

/// Resolve the safetensors filename for a variant name, falling back to the
/// reconstruction-only checkpoint for unknown variants.
pub fn weights_file(variant: &str) -> &'static str {
    skill_constants::LUMAMBA_VARIANTS
        .iter()
        .find(|(v, _)| *v == variant)
        .map(|(_, f)| *f)
        .unwrap_or(skill_constants::LUMAMBA_VARIANTS[0].1)
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

/// RLX-backed LuMamba encoder producing a fixed-dimension EEG embedding.
pub struct LuMamba {
    inner: LuMambaEncoder,
    embed_dim: usize,
}

impl LuMamba {
    /// Load from a local config JSON + safetensors checkpoint.
    pub fn from_files(config_path: &Path, weights_path: &Path, device: rlx::Device) -> Result<Self> {
        let (inner, ms) = LuMambaEncoder::load(config_path, weights_path, device)?;
        let embed_dim = inner.model_cfg.hidden_dim();
        log::info!("LuMamba loaded in {ms:.0} ms ({})", inner.describe());
        Ok(Self { inner, embed_dim })
    }

    /// Resolve `variant` from `repo` on the HuggingFace Hub (downloading on a
    /// cache miss) and load it.
    pub fn from_hf(repo: &str, variant: &str, device: rlx::Device) -> Result<Self> {
        let wf = weights_file(variant);
        log::info!("Resolving LuMamba-{variant} from {repo}...");
        let t0 = Instant::now();
        let config_path = resolve_hf_file(repo, CONFIG_FILE)?;
        let weights_path = resolve_hf_file(repo, wf)?;
        log::info!(
            "Resolved in {:.0} ms: config={}, weights={}",
            t0.elapsed().as_secs_f64() * 1000.0,
            config_path.display(),
            weights_path.display(),
        );
        Self::from_files(&config_path, &weights_path, device)
    }

    /// Resolve the default LuMamba variant from [`HF_REPO`].
    pub fn from_default_hf(variant: &str, device: rlx::Device) -> Result<Self> {
        Self::from_hf(HF_REPO, variant, device)
    }

    /// Load from a config repo + variant: `local:/path/to/weights.safetensors`,
    /// HF cache hit, or Hub download on miss (same flow as EEG-DINO).
    pub fn load(repo: &str, variant: &str, device: rlx::Device) -> Result<Self> {
        if let Some(path) = repo.strip_prefix("local:") {
            let weights_path = PathBuf::from(path);
            let config_path = weights_path
                .parent()
                .map(|p| p.join(CONFIG_FILE))
                .filter(|p| p.exists())
                .unwrap_or_else(|| PathBuf::from(CONFIG_FILE));
            return Self::from_files(&config_path, &weights_path, device);
        }

        let wf = weights_file(variant);
        if let Some((w, c)) = crate::resolve_lumamba_weights(repo, wf) {
            return Self::from_files(&c, &w, device);
        }

        Self::from_hf(repo, variant, device)
    }

    /// Encode one epoch and mean-pool the encoder latent → `[embed_dim]`.
    ///
    /// Each headset channel is mapped onto LuMamba's electrode position (NeRF
    /// positional encoding) and channel-vocabulary index; unknown channels are
    /// zero-positioned with vocab index 0 (the topology-agnostic front-end
    /// relies primarily on the 3-D position).  Samples are truncated to the
    /// largest multiple of `patch_size` (40) that fits.
    pub fn encode_pooled(&mut self, samples: &[Vec<f32>], channel_names: &[&str]) -> Result<Vec<f32>> {
        let patch = self.inner.model_cfg.patch_size;
        let n_ch = channel_names.len().min(samples.len());
        anyhow::ensure!(n_ch > 0, "no channels in epoch");

        let n_samples = samples.iter().take(n_ch).map(Vec::len).min().unwrap_or(0);
        anyhow::ensure!(
            n_samples >= patch,
            "epoch shorter than one LuMamba patch ({patch} samples)"
        );
        let aligned = n_samples - (n_samples % patch);

        let mut chan_pos: Vec<f32> = Vec::with_capacity(n_ch * 3);
        let mut vocab: Vec<i32> = Vec::with_capacity(n_ch);
        let mut signal: Vec<f32> = Vec::with_capacity(n_ch * aligned);
        for (idx, name) in channel_names.iter().take(n_ch).enumerate() {
            let xyz = lumamba::channel_positions::bipolar_channel_xyz(name).unwrap_or([0.0, 0.0, 0.0]);
            chan_pos.extend_from_slice(&xyz);
            vocab.push(lumamba::channel_index(&name.to_uppercase()).unwrap_or(0) as i32);
            // Row-major [C, T]: each channel's aligned samples contiguously.
            signal.extend_from_slice(&samples[idx][..aligned]);
        }

        let (latent, shape) = self.inner.encode(&signal, &chan_pos, Some(&vocab), n_ch, aligned)?;
        let seq_len = shape.first().copied().unwrap_or(1).max(1);
        let dim = shape.get(1).copied().unwrap_or(self.embed_dim);

        let mut pooled = vec![0.0f32; dim];
        for s in 0..seq_len {
            for (d, p) in pooled.iter_mut().enumerate() {
                *p += latent[s * dim + d];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_file_maps_variants() {
        assert_eq!(weights_file("recon"), "LuMamba_ReconstructionOnly.safetensors");
        assert_eq!(weights_file("lejepa-128"), "LuMamba_LeJEPAOnly_128slices.safetensors");
        assert_eq!(weights_file("unknown"), "LuMamba_ReconstructionOnly.safetensors");
    }
}
