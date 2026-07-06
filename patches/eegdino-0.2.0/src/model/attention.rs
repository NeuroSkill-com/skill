use burn::module::Param;
use burn::nn::Linear;
/// Multi-head self-attention with fused QKV bias.
///
/// Matches the Python `Attention` class in `models/transformer.py`.
///
/// The QKV projection is a single `Linear(dim, 3 * all_head_dim)`.
/// After weight loading, call [`Attention::fuse_qkv_bias`] to bake the
/// separate Q/V biases into the Linear's bias field, so forward needs no
/// per-call allocation.
use burn::prelude::*;

use super::linear_zeros;

#[derive(Module, Debug)]
pub struct Attention<B: Backend> {
    /// Combined QKV projection.  Starts with `bias=false`; after
    /// [`fuse_qkv_bias`] the bias holds `[q_bias, 0, v_bias]`.
    pub qkv: Linear<B>,
    /// Learned Q bias: `[all_head_dim]` — consumed by [`fuse_qkv_bias`].
    pub q_bias: Param<Tensor<B, 1>>,
    /// Learned V bias: `[all_head_dim]` — consumed by [`fuse_qkv_bias`].
    pub v_bias: Param<Tensor<B, 1>>,
    /// Output projection.
    pub proj: Linear<B>,
    pub num_heads: usize,
    pub head_dim: usize,
    pub scale: f32,
}

impl<B: Backend> Attention<B> {
    pub fn new(dim: usize, num_heads: usize, device: &B::Device) -> Self {
        let head_dim = dim / num_heads;
        let all_head_dim = head_dim * num_heads;

        Self {
            qkv: linear_zeros::<B>(dim, all_head_dim * 3, false, device),
            q_bias: Param::initialized(
                burn::module::ParamId::new(),
                Tensor::zeros([all_head_dim], device),
            ),
            v_bias: Param::initialized(
                burn::module::ParamId::new(),
                Tensor::zeros([all_head_dim], device),
            ),
            proj: linear_zeros::<B>(all_head_dim, dim, true, device),
            num_heads,
            head_dim,
            scale: (head_dim as f32).powf(-0.5),
        }
    }

    /// Bake `[q_bias, zeros, v_bias]` into `self.qkv.bias` so that
    /// `Linear::forward` applies it automatically.  Call once after weight loading.
    pub fn fuse_qkv_bias(&mut self) {
        let dim = self.num_heads * self.head_dim;
        let device = self.q_bias.val().device();
        let k_bias = Tensor::<B, 1>::zeros([dim], &device);
        let fused = Tensor::cat(vec![self.q_bias.val(), k_bias, self.v_bias.val()], 0);
        self.qkv.bias = Some(Param::initialized(burn::module::ParamId::new(), fused));
    }

    /// `x`: `[B, N, dim]` → `[B, N, dim]`
    pub fn forward(&self, x: Tensor<B, 3>) -> Tensor<B, 3> {
        let [b, n, _] = x.dims();
        let h = self.num_heads;
        let d = self.head_dim;

        // QKV projection (bias fused into the Linear if fuse_qkv_bias was called)
        let qkv = if self.qkv.bias.is_some() {
            self.qkv.forward(x)
        } else {
            // Fallback: build bias on the fly
            let device = x.device();
            let k_bias = Tensor::<B, 1>::zeros([h * d], &device);
            let bias = Tensor::cat(vec![self.q_bias.val(), k_bias, self.v_bias.val()], 0);
            self.qkv.forward(x) + bias.unsqueeze::<2>().unsqueeze::<3>()
        };

        // [B, N, 3HD] → [3, B, H, N, D]
        let qkv = qkv.reshape([b, n, 3, h, d]).permute([2, 0, 3, 1, 4]);

        let q = qkv.clone().narrow(0, 0, 1).reshape([b, h, n, d]);
        let k = qkv.clone().narrow(0, 1, 1).reshape([b, h, n, d]);
        let v = qkv.narrow(0, 2, 1).reshape([b, h, n, d]);

        // Scaled dot-product attention
        let attn = burn::tensor::activation::softmax((q * self.scale).matmul(k.transpose()), 3);

        // [B, H, N, D] → [B, N, HD]
        self.proj
            .forward(attn.matmul(v).permute([0, 2, 1, 3]).reshape([b, n, h * d]))
    }
}
