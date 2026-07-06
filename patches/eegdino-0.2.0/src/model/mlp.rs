use burn::nn::Linear;
/// Feed-forward MLP with GELU activation.
///
/// Matches the Python `Mlp` class in `models/transformer.py`:
///   fc1(x) → GELU → fc2(x)
///
/// Dropout is omitted for inference (eval mode).
use burn::prelude::*;

use super::linear_zeros;

#[derive(Module, Debug)]
pub struct Mlp<B: Backend> {
    pub fc1: Linear<B>,
    pub fc2: Linear<B>,
}

impl<B: Backend> Mlp<B> {
    pub fn new(in_features: usize, hidden_features: usize, device: &B::Device) -> Self {
        Self {
            fc1: linear_zeros::<B>(in_features, hidden_features, true, device),
            fc2: linear_zeros::<B>(hidden_features, in_features, true, device),
        }
    }

    /// Forward pass.
    ///
    /// x: [B, N, in_features] → [B, N, in_features]
    pub fn forward(&self, x: Tensor<B, 3>) -> Tensor<B, 3> {
        let x = self.fc1.forward(x);
        let x = burn::tensor::activation::gelu(x);
        self.fc2.forward(x)
    }
}
