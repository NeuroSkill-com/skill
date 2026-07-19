use burn::nn::{LayerNorm, LayerNormConfig};
/// Transformer encoder layer with pre-norm residual connections.
///
/// Matches the Python `TransformerEncoderLayer` in `models/transformer.py`.
///
/// Architecture (inference path, gamma=None):
/// ```text
/// x = x + Attn(LayerNorm(x))
/// x = x + MLP(LayerNorm(x))
/// ```
///
/// DropPath and gamma scaling are training-only and omitted here.
use burn::prelude::*;

use super::attention::Attention;
use super::mlp::Mlp;
use crate::config::ModelConfig;

#[derive(Module, Debug)]
pub struct TransformerEncoderLayer<B: Backend> {
    pub norm1: LayerNorm<B>,
    pub attn: Attention<B>,
    pub norm2: LayerNorm<B>,
    pub mlp: Mlp<B>,
}

impl<B: Backend> TransformerEncoderLayer<B> {
    pub fn new(cfg: &ModelConfig, device: &B::Device) -> Self {
        let d = cfg.feature_size;
        Self {
            norm1: LayerNormConfig::new(d)
                .with_epsilon(cfg.layer_norm_eps)
                .init(device),
            attn: Attention::new(d, cfg.num_heads, device),
            norm2: LayerNormConfig::new(d)
                .with_epsilon(cfg.layer_norm_eps)
                .init(device),
            mlp: Mlp::new(d, cfg.dim_feedforward, device),
        }
    }

    /// `x`: `[B, N, D]` -> `[B, N, D]`
    pub fn forward(&self, x: Tensor<B, 3>) -> Tensor<B, 3> {
        // Attention residual (clone is O(1) — Arc increment, not a data copy)
        let h = x.clone() + self.attn.forward(self.norm1.forward(x));
        // MLP residual
        h.clone() + self.mlp.forward(self.norm2.forward(h))
    }
}
