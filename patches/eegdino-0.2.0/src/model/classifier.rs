use burn::nn::Linear;
/// Classification model: encoder + two-stage pooling + 3-layer MLP head.
///
/// Matches the Python `ClassificationModel` from `run_finetuning.py`.
///
/// Pipeline:
///   encoder(x) → strip global tokens → full_linear → GELU
///   → reshape → channel pool (mean over C) → channel_linear → GELU
///   → time pool (mean over P) → classifier MLP → logits
use burn::prelude::*;

use super::encoder::EEGEncoder;
use super::linear_zeros;
use crate::config::ModelConfig;

#[derive(Module, Debug)]
pub struct ClassificationModel<B: Backend> {
    pub encoder: EEGEncoder<B>,
    pub full_linear: Linear<B>,
    pub channel_linear: Linear<B>,
    /// 3-layer classifier: D → D/2 → D/4 → num_classes
    pub cls_fc1: Linear<B>,
    pub cls_fc2: Linear<B>,
    pub cls_fc3: Linear<B>,
    pub feature_size: usize,
    pub num_global_tokens: usize,
}

impl<B: Backend> ClassificationModel<B> {
    pub fn new(cfg: &ModelConfig, num_classes: usize, device: &B::Device) -> Self {
        let d = cfg.feature_size;
        Self {
            encoder: EEGEncoder::new(cfg, device),
            full_linear: linear_zeros::<B>(d, d, true, device),
            channel_linear: linear_zeros::<B>(d, d, true, device),
            cls_fc1: linear_zeros::<B>(d, d / 2, true, device),
            cls_fc2: linear_zeros::<B>(d / 2, d / 4, true, device),
            cls_fc3: linear_zeros::<B>(d / 4, num_classes, true, device),
            feature_size: d,
            num_global_tokens: cfg.num_global_tokens,
        }
    }

    /// Forward pass.
    ///
    /// x: [B, C, P, L] → [B, num_classes]
    pub fn forward(&self, x: Tensor<B, 4>) -> Tensor<B, 2> {
        let [bs, ch, seq_len, _feat] = x.dims();
        let d = self.feature_size;

        // Encoder: [B, C, P, L] → [B, num_global + C*P, D]
        let features = self.encoder.forward(x);

        // Strip global tokens: [B, C*P, D]
        let total_seq = features.dims()[1];
        let tokens = features.slice([0..bs, self.num_global_tokens..total_seq]);

        // full_linear + GELU on flattened tokens
        let flat = tokens.reshape([bs * ch * seq_len, d]);
        let processed = burn::tensor::activation::gelu(self.full_linear.forward(flat));

        // Reshape back: [B, C, P, D]
        let reshaped = processed.reshape([bs, ch, seq_len, d]);

        // Channel pool: mean over dim=1 (channels) → [B, P, D]
        let channel_pooled = reshaped.mean_dim(1);

        // channel_linear + GELU on flattened time steps
        let flat = channel_pooled.reshape([bs * seq_len, d]);
        let processed = burn::tensor::activation::gelu(self.channel_linear.forward(flat));
        let processed = processed.reshape([bs, seq_len, d]);

        // Time pool: mean over dim=1 (patches) → [B, D]
        let time_pooled = processed.mean_dim(1).reshape([bs, d]);

        // Classifier MLP: D → D/2 → D/4 → num_classes
        // (Dropout is omitted for inference)
        let h = burn::tensor::activation::gelu(self.cls_fc1.forward(time_pooled));
        let h = burn::tensor::activation::gelu(self.cls_fc2.forward(h));
        self.cls_fc3.forward(h)
    }
}
