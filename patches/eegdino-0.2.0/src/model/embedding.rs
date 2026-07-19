use burn::module::Ignored;
use burn::nn::{
    conv::{Conv2d, Conv2dConfig},
    GroupNorm, GroupNormConfig, Linear,
};
/// Patch embedding layer combining temporal, spectral, and channel embeddings.
///
/// Matches the Python `PatchEmbedding` class from `embedding_{small,medium,large}.py`.
///
/// Input:  `[B, C, P, L]`  (batch, channels, patches, patch_length=200)
/// Output: `[B, C, P, D]`  (batch, channels, patches, d_model)
///
/// Three embedding streams are summed:
/// 1. **Temporal** (`proj_in`): 3-layer Conv2d stack
/// 2. **Spectral**: rfft magnitude via on-device DFT matmul
/// 3. **Channel**: one-hot(channel_idx) → Linear(19, D)
///
/// A depthwise conv `time_encoding` is added on top.
use burn::prelude::*;
#[allow(unused_imports)]
use rayon::prelude::*;
#[allow(unused_imports)]
use rustfft::{num_complex::Complex64, FftPlanner};

use super::linear_zeros;
use crate::config::ModelConfig;

// ── Conv-Norm block ─────────────────────────────────────────────────────────

#[derive(Module, Debug)]
pub struct ConvNormBlock<B: Backend> {
    pub conv: Conv2d<B>,
    pub norm: GroupNorm<B>,
}

impl<B: Backend> ConvNormBlock<B> {
    fn forward(&self, x: Tensor<B, 4>) -> Tensor<B, 4> {
        burn::tensor::activation::gelu(self.norm.forward(self.conv.forward(x)))
    }
}

// ── Embedding cache (on-device, created once) ───────────────────────────────

/// Cached on-device tensors for the patch embedding.
///
/// Created once via [`EmbeddingCache::new`] and reused across forward calls,
/// avoiding repeated CPU→device transfers of constant data.
pub struct EmbeddingCache<B: Backend> {
    /// DFT cosine basis `[K, N]` where K = spectral_bins, N = patch_size.
    pub dft_cos: Tensor<B, 2>,
    /// DFT sine basis `[K, N]`.
    pub dft_sin: Tensor<B, 2>,
    /// Channel one-hot matrix `[C, C]`.
    pub channel_one_hot: Tensor<B, 2>,
    pub spectral_bins: usize,
    pub patch_size: usize,
}

impl<B: Backend> EmbeddingCache<B> {
    /// Build cached device tensors for a given config.
    pub fn new(cfg: &ModelConfig, device: &B::Device) -> Self {
        let n = cfg.patch_size;
        let k = cfg.spectral_bins();
        let c = cfg.num_channels;

        // DFT basis
        let two_pi_over_n = 2.0 * std::f64::consts::PI / n as f64;
        let mut cos_data = Vec::with_capacity(k * n);
        let mut sin_data = Vec::with_capacity(k * n);
        for ki in 0..k {
            for ni in 0..n {
                let angle = two_pi_over_n * (ki as f64) * (ni as f64);
                cos_data.push(angle.cos() as f32);
                sin_data.push(angle.sin() as f32);
            }
        }
        let dft_cos = Tensor::<B, 1>::from_floats(cos_data.as_slice(), device).reshape([k, n]);
        let dft_sin = Tensor::<B, 1>::from_floats(sin_data.as_slice(), device).reshape([k, n]);

        // Channel one-hot
        let mut oh = vec![0.0f32; c * c];
        for i in 0..c {
            oh[i * c + i] = 1.0;
        }
        let channel_one_hot = Tensor::<B, 1>::from_floats(oh.as_slice(), device).reshape([c, c]);

        Self {
            dft_cos,
            dft_sin,
            channel_one_hot,
            spectral_bins: k,
            patch_size: n,
        }
    }
}

// ── PatchEmbedding ──────────────────────────────────────────────────────────

#[derive(Module, Debug)]
pub struct PatchEmbedding<B: Backend> {
    /// Temporal conv stack (`proj_in`): 3 x (Conv2d + GroupNorm + GELU)
    pub conv_block1: ConvNormBlock<B>,
    pub conv_block2: ConvNormBlock<B>,
    pub conv_block3: ConvNormBlock<B>,
    /// Spectral projection: Linear(101, d_model)
    pub spectral_proj: Linear<B>,
    /// Channel position embedding: Linear(num_channels, d_model)
    pub channel_embedding: Linear<B>,
    /// Depthwise temporal encoding: Conv2d(d_model, d_model, (1,5), groups=d_model)
    pub time_encoding: Conv2d<B>,
    /// Fallback DFT basis (used only when no EmbeddingCache is provided).
    pub dft_basis: Ignored<DftBasis>,
    /// Fallback channel one-hot.
    pub channel_one_hot: Ignored<ChannelOneHot>,
    pub d_model: usize,
    pub num_channels: usize,
    pub patch_size: usize,
}

impl<B: Backend> PatchEmbedding<B> {
    pub fn new(cfg: &ModelConfig, device: &B::Device) -> Self {
        let [c1, c2, c3] = cfg.conv_channels;
        let [g1, g2, g3] = cfg.norm_groups;
        let d = cfg.feature_size;

        let conv1 = Conv2dConfig::new([1, c1], [1, 49])
            .with_stride([1, 25])
            .with_padding(burn::nn::PaddingConfig2d::Valid)
            .init(device);
        let norm1 = GroupNormConfig::new(g1, c1).init(device);
        let conv2 = Conv2dConfig::new([c1, c2], [1, 3])
            .with_padding(burn::nn::PaddingConfig2d::Explicit(0, 1))
            .init(device);
        let norm2 = GroupNormConfig::new(g2, c2).init(device);
        let conv3 = Conv2dConfig::new([c2, c3], [1, 3])
            .with_padding(burn::nn::PaddingConfig2d::Explicit(0, 1))
            .init(device);
        let norm3 = GroupNormConfig::new(g3, c3).init(device);

        Self {
            conv_block1: ConvNormBlock {
                conv: conv1,
                norm: norm1,
            },
            conv_block2: ConvNormBlock {
                conv: conv2,
                norm: norm2,
            },
            conv_block3: ConvNormBlock {
                conv: conv3,
                norm: norm3,
            },
            spectral_proj: linear_zeros::<B>(cfg.spectral_bins(), d, true, device),
            channel_embedding: linear_zeros::<B>(cfg.num_channels, d, true, device),
            time_encoding: Conv2dConfig::new([d, d], [1, 5])
                .with_padding(burn::nn::PaddingConfig2d::Explicit(0, 2))
                .with_groups(d)
                .init(device),
            dft_basis: Ignored(DftBasis::new(cfg.patch_size)),
            channel_one_hot: Ignored(ChannelOneHot::new(cfg.num_channels)),
            d_model: d,
            num_channels: cfg.num_channels,
            patch_size: cfg.patch_size,
        }
    }

    /// Forward pass using a pre-built on-device cache (fast path).
    pub fn forward_cached(&self, x: Tensor<B, 4>, cache: &EmbeddingCache<B>) -> Tensor<B, 4> {
        let [bz, ch_num, patch_num, patch_size] = x.dims();
        let device = x.device();

        // 1. Temporal conv stack
        let x_conv = x.clone().reshape([bz, 1, ch_num * patch_num, patch_size]);
        let pad_w = 24;
        let zeros = Tensor::<B, 4>::zeros([bz, 1, ch_num * patch_num, pad_w], &device);
        let x_padded = Tensor::cat(vec![zeros.clone(), x_conv, zeros], 3);
        let patch_emb = self.conv_block1.forward(x_padded);
        let patch_emb = self.conv_block2.forward(patch_emb);
        let patch_emb = self.conv_block3.forward(patch_emb);
        let patch_emb =
            patch_emb
                .permute([0, 2, 1, 3])
                .reshape([bz, ch_num, patch_num, self.d_model]);

        // 2. Spectral (cached DFT basis)
        let total = bz * ch_num * patch_num;
        let k = cache.spectral_bins;
        let inv_n = 1.0 / patch_size as f32;
        let flat = x.reshape([total, patch_size]);
        let real = flat.clone().matmul(cache.dft_cos.clone().transpose());
        let imag = flat.matmul(cache.dft_sin.clone().transpose());
        let spectral = (real.clone() * real + imag.clone() * imag).sqrt() * inv_n;
        let spectral_emb = self
            .spectral_proj
            .forward(spectral.reshape([bz, ch_num, patch_num, k]));

        let mut patch_emb = patch_emb + spectral_emb;

        // 3. Channel (cached one-hot)
        let chan_emb = self
            .channel_embedding
            .forward(cache.channel_one_hot.clone())
            .unsqueeze::<3>()
            .unsqueeze_dim::<4>(2)
            .expand([bz, ch_num, patch_num, self.d_model]);
        patch_emb = patch_emb + chan_emb;

        // 4. Time encoding
        let time_emb = self
            .time_encoding
            .forward(patch_emb.clone().permute([0, 3, 1, 2]))
            .permute([0, 2, 3, 1]);
        patch_emb + time_emb
    }

    /// Forward pass without cache (rebuilds DFT/one-hot from CPU each call).
    pub fn forward(&self, x: Tensor<B, 4>) -> Tensor<B, 4> {
        let [bz, ch_num, patch_num, patch_size] = x.dims();
        let device = x.device();

        let x_conv = x.clone().reshape([bz, 1, ch_num * patch_num, patch_size]);
        let pad_w = 24;
        let zeros = Tensor::<B, 4>::zeros([bz, 1, ch_num * patch_num, pad_w], &device);
        let x_padded = Tensor::cat(vec![zeros.clone(), x_conv, zeros], 3);
        let patch_emb = self.conv_block1.forward(x_padded);
        let patch_emb = self.conv_block2.forward(patch_emb);
        let patch_emb = self.conv_block3.forward(patch_emb);
        let patch_emb =
            patch_emb
                .permute([0, 2, 1, 3])
                .reshape([bz, ch_num, patch_num, self.d_model]);

        let spectral_emb = self
            .spectral_proj
            .forward(self.dft_basis.0.apply::<B>(&x, &device));
        let mut patch_emb = patch_emb + spectral_emb;

        let chan_emb = self
            .channel_embedding
            .forward(self.channel_one_hot.0.to_tensor::<B>(&device))
            .unsqueeze::<3>()
            .unsqueeze_dim::<4>(2)
            .expand([bz, ch_num, patch_num, self.d_model]);
        patch_emb = patch_emb + chan_emb;

        let time_emb = self
            .time_encoding
            .forward(patch_emb.clone().permute([0, 3, 1, 2]))
            .permute([0, 2, 3, 1]);
        patch_emb + time_emb
    }
}

// ── Fallback types (for uncached path) ──────────────────────────────────────

/// Pre-computed DFT basis stored as `Vec<f32>` (CPU-side fallback).
#[derive(Debug, Clone)]
pub struct DftBasis {
    cos_data: Vec<f32>,
    sin_data: Vec<f32>,
    spectral_bins: usize,
}

impl DftBasis {
    pub fn new(patch_size: usize) -> Self {
        let k = patch_size / 2 + 1;
        let two_pi_over_n = 2.0 * std::f64::consts::PI / patch_size as f64;
        let mut cos_data = Vec::with_capacity(k * patch_size);
        let mut sin_data = Vec::with_capacity(k * patch_size);
        for ki in 0..k {
            for ni in 0..patch_size {
                let angle = two_pi_over_n * (ki as f64) * (ni as f64);
                cos_data.push(angle.cos() as f32);
                sin_data.push(angle.sin() as f32);
            }
        }
        Self {
            cos_data,
            sin_data,
            spectral_bins: k,
        }
    }

    fn apply<B: Backend>(&self, x: &Tensor<B, 4>, device: &B::Device) -> Tensor<B, 4> {
        let [bz, ch, p, n] = x.dims();
        let total = bz * ch * p;
        let k = self.spectral_bins;
        let inv_n = 1.0 / n as f32;
        let cos_basis =
            Tensor::<B, 1>::from_floats(self.cos_data.as_slice(), device).reshape([k, n]);
        let sin_basis =
            Tensor::<B, 1>::from_floats(self.sin_data.as_slice(), device).reshape([k, n]);
        let flat = x.clone().reshape([total, n]);
        let real = flat.clone().matmul(cos_basis.transpose());
        let imag = flat.matmul(sin_basis.transpose());
        let mag = (real.clone() * real + imag.clone() * imag).sqrt() * inv_n;
        mag.reshape([bz, ch, p, k])
    }
}

/// Pre-computed one-hot matrix stored as `Vec<f32>` (CPU-side fallback).
#[derive(Debug, Clone)]
pub struct ChannelOneHot {
    data: Vec<f32>,
    num_channels: usize,
}

impl ChannelOneHot {
    pub fn new(num_channels: usize) -> Self {
        let mut data = vec![0.0f32; num_channels * num_channels];
        for i in 0..num_channels {
            data[i * num_channels + i] = 1.0;
        }
        Self { data, num_channels }
    }

    fn to_tensor<B: Backend>(&self, device: &B::Device) -> Tensor<B, 2> {
        let n = self.num_channels;
        Tensor::<B, 1>::from_floats(self.data.as_slice(), device).reshape([n, n])
    }
}
