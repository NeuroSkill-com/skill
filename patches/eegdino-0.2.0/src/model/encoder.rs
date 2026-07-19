use burn::module::{Param, ParamId};
/// EEG-DINO Encoder: patch embedding → transformer layers with global tokens.
///
/// Matches the Python `EEGEncoder` class from `models/eeg_encoder.py`.
///
/// Input:  `[B, C, P, L]`  (batch, channels, patches, patch_length)
/// Output: `[B, num_global + C*P, D]`
///
/// Global tokens are injected after layer `global_token_layer` (1-indexed).
use burn::prelude::*;

use super::embedding::{EmbeddingCache, PatchEmbedding};
use super::transformer::TransformerEncoderLayer;
use crate::config::ModelConfig;

#[derive(Module, Debug)]
pub struct EEGEncoder<B: Backend> {
    pub patch_embedding: PatchEmbedding<B>,
    pub encoder_layers: Vec<TransformerEncoderLayer<B>>,
    /// Learnable global tokens: `[1, num_global_tokens, feature_size]`
    pub global_tokens: Param<Tensor<B, 3>>,
    pub global_token_layer: usize,
    pub num_global_tokens: usize,
}

impl<B: Backend> EEGEncoder<B> {
    pub fn new(cfg: &ModelConfig, device: &B::Device) -> Self {
        let layers: Vec<_> = (0..cfg.num_layers)
            .map(|_| TransformerEncoderLayer::new(cfg, device))
            .collect();

        let global_tokens = Param::initialized(
            ParamId::new(),
            Tensor::zeros([1, cfg.num_global_tokens, cfg.feature_size], device),
        );

        Self {
            patch_embedding: PatchEmbedding::new(cfg, device),
            encoder_layers: layers,
            global_tokens,
            global_token_layer: cfg.global_token_layer,
            num_global_tokens: cfg.num_global_tokens,
        }
    }

    /// Forward pass using a pre-built embedding cache (fast path).
    ///
    /// `x_in`: `[B, C, P, L]` → `[B, num_global + C*P, D]`
    pub fn forward_cached(&self, x_in: Tensor<B, 4>, cache: &EmbeddingCache<B>) -> Tensor<B, 3> {
        let [b, _c, _p, _l] = x_in.dims();
        let x = self.patch_embedding.forward_cached(x_in, cache);
        self.run_transformer(x, b)
    }

    /// Forward pass without cache (rebuilds constants each call).
    ///
    /// `x_in`: `[B, C, P, L]` → `[B, num_global + C*P, D]`
    pub fn forward(&self, x_in: Tensor<B, 4>) -> Tensor<B, 3> {
        let [b, _c, _p, _l] = x_in.dims();
        let x = self.patch_embedding.forward(x_in);
        self.run_transformer(x, b)
    }

    fn run_transformer(&self, emb: Tensor<B, 4>, b: usize) -> Tensor<B, 3> {
        let d = emb.dims()[3];
        let seq_len = emb.dims()[1] * emb.dims()[2];
        let mut x = emb.reshape([b, seq_len, d]);

        let global = self
            .global_tokens
            .val()
            .expand([b, self.num_global_tokens, d]);

        for (i, layer) in self.encoder_layers.iter().enumerate() {
            x = layer.forward(x);
            if i + 1 == self.global_token_layer {
                x = Tensor::cat(vec![global.clone(), x], 1);
            }
        }
        x
    }
}
