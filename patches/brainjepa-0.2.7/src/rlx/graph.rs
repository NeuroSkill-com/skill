//! Brain-JEPA encoder expressed as an RLX IR graph.
//!
//! The graph is built for one fixed `(B=1, H, W)` shape derived from
//! `DataConfig` / `ModelConfig`. Positional embeddings are provided as
//! `pos_embed` is a graph parameter (uploaded once at load).

use rlx::ir::infer::GraphExt;
use rlx::ops::MaskKind;
use rlx::prelude::*;

pub use super::attn_layout::AttnLayout;

#[derive(Clone)]
pub struct EncoderSpec {
    pub b: usize,
    pub h: usize,     // n_rois
    pub w: usize,     // n_time
    pub patch: usize, // patch_size
    pub w_p: usize,   // w / patch
    pub n: usize,     // h * w_p
    pub dim: usize,   // embed_dim
    pub depth: usize,
    pub num_heads: usize,
    pub head_dim: usize,
    pub hidden_dim: usize,
    pub norm_eps: f32,
}

fn s1(d: usize) -> Shape {
    Shape::new(&[d], DType::F32)
}
fn s2(a: usize, b: usize) -> Shape {
    Shape::new(&[a, b], DType::F32)
}
fn s4(a: usize, b: usize, c: usize, d: usize) -> Shape {
    Shape::new(&[a, b, c, d], DType::F32)
}

fn attn_block(
    g: &mut Graph,
    x: NodeId,
    spec: &EncoderSpec,
    layout: AttnLayout,
    layer: usize,
) -> NodeId {
    let d = spec.dim;
    let n = spec.n;
    let b = spec.b;
    let nh = spec.num_heads;
    let dh = spec.head_dim;
    let p = format!("encoder.blocks.{layer}");

    // LayerNorm 1
    let ln1_w = g.param(format!("{p}.norm1.weight"), s1(d));
    let ln1_b = g.param(format!("{p}.norm1.bias"), s1(d));
    let xn = g.ln(x, ln1_w, ln1_b, spec.norm_eps);

    // QKV
    let qkv_w = g.param(format!("{p}.attn.qkv.weight"), s2(d, 3 * d));
    let qkv_b = g.param(format!("{p}.attn.qkv.bias"), s1(3 * d));
    let qkv_mm = g.mm(xn, qkv_w);
    let qkv = g.add(qkv_mm, qkv_b); // [B,N,3D]
    let qkv5 = g.reshape_(qkv, vec![b as i64, n as i64, 3, nh as i64, dh as i64]);

    let q5 = g.narrow_(qkv5, 2, 0, 1);
    let k5 = g.narrow_(qkv5, 2, 1, 1);
    let v5 = g.narrow_(qkv5, 2, 2, 1);
    let (q, k, v, attn_shape) = match layout {
        AttnLayout::Bsnh => {
            let q = g.reshape_(q5, vec![b as i64, n as i64, nh as i64, dh as i64]);
            let k = g.reshape_(k5, vec![b as i64, n as i64, nh as i64, dh as i64]);
            let v = g.reshape_(v5, vec![b as i64, n as i64, nh as i64, dh as i64]);
            (q, k, v, s4(b, n, nh, dh))
        }
        AttnLayout::Bhsd => {
            let q4 = g.reshape_(q5, vec![b as i64, n as i64, nh as i64, dh as i64]);
            let k4 = g.reshape_(k5, vec![b as i64, n as i64, nh as i64, dh as i64]);
            let v4 = g.reshape_(v5, vec![b as i64, n as i64, nh as i64, dh as i64]);
            (
                g.transpose_(q4, vec![0, 2, 1, 3]),
                g.transpose_(k4, vec![0, 2, 1, 3]),
                g.transpose_(v4, vec![0, 2, 1, 3]),
                s4(b, nh, n, dh),
            )
        }
    };

    let attn = g.attention_kind(q, k, v, nh, dh, MaskKind::None, attn_shape);
    let attn3 = match layout {
        AttnLayout::Bsnh => g.reshape_(attn, vec![b as i64, n as i64, d as i64]),
        AttnLayout::Bhsd => {
            let bsnh = g.transpose_(attn, vec![0, 2, 1, 3]);
            g.reshape_(bsnh, vec![b as i64, n as i64, d as i64])
        }
    };

    // Output projection
    let proj_w = g.param(format!("{p}.attn.proj.weight"), s2(d, d));
    let proj_b = g.param(format!("{p}.attn.proj.bias"), s1(d));
    let proj_mm = g.mm(attn3, proj_w);
    let attn_out = g.add(proj_mm, proj_b);
    let x = g.add(x, attn_out);

    // LayerNorm 2
    let ln2_w = g.param(format!("{p}.norm2.weight"), s1(d));
    let ln2_b = g.param(format!("{p}.norm2.bias"), s1(d));
    let hn = g.ln(x, ln2_w, ln2_b, spec.norm_eps);

    // MLP
    let fc1_w = g.param(format!("{p}.mlp.fc1.weight"), s2(d, spec.hidden_dim));
    let fc1_b = g.param(format!("{p}.mlp.fc1.bias"), s1(spec.hidden_dim));
    let fc2_w = g.param(format!("{p}.mlp.fc2.weight"), s2(spec.hidden_dim, d));
    let fc2_b = g.param(format!("{p}.mlp.fc2.bias"), s1(d));

    let fc1_mm = g.mm(hn, fc1_w);
    let m1 = g.add(fc1_mm, fc1_b);
    let act = g.gelu_approx(m1);
    let fc2_mm = g.mm(act, fc2_w);
    let m2 = g.add(fc2_mm, fc2_b);
    g.add(x, m2)
}

pub fn build_encoder_graph(spec: &EncoderSpec, attn_layout: AttnLayout) -> Graph {
    let mut g = Graph::new("brainjepa_encoder");

    let b = spec.b;
    let h = spec.h;
    let w = spec.w;
    let n = spec.n;
    let d = spec.dim;
    let ps = spec.patch;

    // Input: fMRI volume [B, 1, H, W]
    let x = g.input("x", Shape::new(&[b, 1, h, w], DType::F32));
    // Positional embedding [1, N, D] — set once via `set_param("pos_embed", …)`.
    let pos = g.param("pos_embed", Shape::new(&[1, n, d], DType::F32));

    // Patch embedding: reshape temporal windows + matmul (ViT-style patch embed).
    // Avoids Conv2d so Metal can lower the full graph via MPSGraph.
    let x_bhw = g.reshape_(x, vec![b as i64, h as i64, w as i64]);
    let x_win = g.reshape_(x_bhw, vec![b as i64, n as i64, ps as i64]);
    let pe_w = g.param("encoder.patch_embed.proj.weight", s2(ps, d));
    let pe_b = g.param("encoder.patch_embed.proj.bias", s1(d));
    let pe_mm = g.mm(x_win, pe_w);
    let mut h3 = g.add(pe_mm, pe_b);

    // Add positional embedding (broadcast along batch)
    h3 = g.add(h3, pos);

    // Transformer blocks
    for i in 0..spec.depth {
        h3 = attn_block(&mut g, h3, spec, attn_layout, i);
    }

    // Final norm
    let ln_w = g.param("encoder.norm.weight", s1(d));
    let ln_b = g.param("encoder.norm.bias", s1(d));
    let out = g.ln(h3, ln_w, ln_b, spec.norm_eps);

    g.set_outputs(vec![out]);
    g
}

/// Patch embed + positional add → `[B, N, D]` (before masking / transformer trunk).
pub fn build_encoder_embed_graph(spec: &EncoderSpec) -> Graph {
    let mut g = Graph::new("brainjepa_encoder_embed");
    let b = spec.b;
    let h = spec.h;
    let w = spec.w;
    let n = spec.n;
    let d = spec.dim;
    let ps = spec.patch;

    let x = g.input("x", Shape::new(&[b, 1, h, w], DType::F32));
    let pos = g.param("pos_embed", Shape::new(&[1, n, d], DType::F32));

    let x_bhw = g.reshape_(x, vec![b as i64, h as i64, w as i64]);
    let x_win = g.reshape_(x_bhw, vec![b as i64, n as i64, ps as i64]);
    let pe_w = g.param("encoder.patch_embed.proj.weight", s2(ps, d));
    let pe_b = g.param("encoder.patch_embed.proj.bias", s1(d));
    let pe_mm = g.mm(x_win, pe_w);
    let h3 = g.add(pe_mm, pe_b);
    let out = g.add(h3, pos);
    g.set_outputs(vec![out]);
    g
}

/// Transformer blocks + final norm on `[B, N, D]` patch tokens.
pub fn build_encoder_trunk_graph(
    spec: &EncoderSpec,
    attn_layout: AttnLayout,
    n_seq: usize,
) -> Graph {
    let mut g = Graph::new("brainjepa_encoder_trunk");
    let b = spec.b;
    let d = spec.dim;
    let mut trunk_spec = spec.clone();
    trunk_spec.n = n_seq;
    let mut h3 = g.input("h0", Shape::new(&[b, n_seq, d], DType::F32));
    for i in 0..spec.depth {
        h3 = attn_block(&mut g, h3, &trunk_spec, attn_layout, i);
    }
    let ln_w = g.param("encoder.norm.weight", s1(d));
    let ln_b = g.param("encoder.norm.bias", s1(d));
    let out = g.ln(h3, ln_w, ln_b, spec.norm_eps);
    g.set_outputs(vec![out]);
    g
}

#[derive(Clone)]
pub struct PredictorSpec {
    pub b: usize,
    pub n_patches: usize,
    pub n_ctx: usize,
    pub n_pred: usize,
    pub enc_dim: usize,
    pub pred_dim: usize,
    pub depth: usize,
    pub num_heads: usize,
    pub head_dim: usize,
    pub hidden_dim: usize,
    pub norm_eps: f32,
}

fn pred_attn_block(
    g: &mut Graph,
    x: NodeId,
    spec: &PredictorSpec,
    layout: AttnLayout,
    layer: usize,
) -> NodeId {
    let d = spec.pred_dim;
    let n = spec.n_ctx + spec.n_pred;
    let b = spec.b;
    let nh = spec.num_heads;
    let dh = spec.head_dim;
    let p = format!("predictor.predictor_blocks.{layer}");

    let ln1_w = g.param(format!("{p}.norm1.weight"), s1(d));
    let ln1_b = g.param(format!("{p}.norm1.bias"), s1(d));
    let xn = g.ln(x, ln1_w, ln1_b, spec.norm_eps);

    let qkv_w = g.param(format!("{p}.attn.qkv.weight"), s2(d, 3 * d));
    let qkv_b = g.param(format!("{p}.attn.qkv.bias"), s1(3 * d));
    let qkv_mm = g.mm(xn, qkv_w);
    let qkv = g.add(qkv_mm, qkv_b);
    let qkv5 = g.reshape_(qkv, vec![b as i64, n as i64, 3, nh as i64, dh as i64]);

    let q5 = g.narrow_(qkv5, 2, 0, 1);
    let k5 = g.narrow_(qkv5, 2, 1, 1);
    let v5 = g.narrow_(qkv5, 2, 2, 1);
    let (q, k, v, attn_shape) = match layout {
        AttnLayout::Bsnh => {
            let q = g.reshape_(q5, vec![b as i64, n as i64, nh as i64, dh as i64]);
            let k = g.reshape_(k5, vec![b as i64, n as i64, nh as i64, dh as i64]);
            let v = g.reshape_(v5, vec![b as i64, n as i64, nh as i64, dh as i64]);
            (q, k, v, s4(b, n, nh, dh))
        }
        AttnLayout::Bhsd => {
            let q4 = g.reshape_(q5, vec![b as i64, n as i64, nh as i64, dh as i64]);
            let k4 = g.reshape_(k5, vec![b as i64, n as i64, nh as i64, dh as i64]);
            let v4 = g.reshape_(v5, vec![b as i64, n as i64, nh as i64, dh as i64]);
            (
                g.transpose_(q4, vec![0, 2, 1, 3]),
                g.transpose_(k4, vec![0, 2, 1, 3]),
                g.transpose_(v4, vec![0, 2, 1, 3]),
                s4(b, nh, n, dh),
            )
        }
    };

    let attn = g.attention_kind(q, k, v, nh, dh, MaskKind::None, attn_shape);
    let attn3 = match layout {
        AttnLayout::Bsnh => g.reshape_(attn, vec![b as i64, n as i64, d as i64]),
        AttnLayout::Bhsd => {
            let bsnh = g.transpose_(attn, vec![0, 2, 1, 3]);
            g.reshape_(bsnh, vec![b as i64, n as i64, d as i64])
        }
    };

    let proj_w = g.param(format!("{p}.attn.proj.weight"), s2(d, d));
    let proj_b = g.param(format!("{p}.attn.proj.bias"), s1(d));
    let proj_mm = g.mm(attn3, proj_w);
    let attn_out = g.add(proj_mm, proj_b);
    let x = g.add(x, attn_out);

    let ln2_w = g.param(format!("{p}.norm2.weight"), s1(d));
    let ln2_b = g.param(format!("{p}.norm2.bias"), s1(d));
    let hn = g.ln(x, ln2_w, ln2_b, spec.norm_eps);

    let fc1_w = g.param(format!("{p}.mlp.fc1.weight"), s2(d, spec.hidden_dim));
    let fc1_b = g.param(format!("{p}.mlp.fc1.bias"), s1(spec.hidden_dim));
    let fc2_w = g.param(format!("{p}.mlp.fc2.weight"), s2(spec.hidden_dim, d));
    let fc2_b = g.param(format!("{p}.mlp.fc2.bias"), s1(d));

    let fc1_mm = g.mm(hn, fc1_w);
    let m1 = g.add(fc1_mm, fc1_b);
    let act = g.gelu_approx(m1);
    let fc2_mm = g.mm(act, fc2_w);
    let m2 = g.add(fc2_mm, fc2_b);
    g.add(x, m2)
}

/// Project encoder context to predictor dim: `[B, Kc, D_enc]` → `[B, Kc, D_pred]`.
pub fn build_predictor_embed_graph(b: usize, kc: usize, d_enc: usize, d_pred: usize) -> Graph {
    let mut g = Graph::new("brainjepa_predictor_embed");
    let ctx = g.input("ctx_enc", Shape::new(&[b, kc, d_enc], DType::F32));
    let w = g.param("predictor.predictor_embed.weight", s2(d_enc, d_pred));
    let bias = g.param("predictor.predictor_embed.bias", s1(d_pred));
    let mm = g.mm(ctx, w);
    let out = g.add(mm, bias);
    g.set_outputs(vec![out]);
    g
}

/// Predictor transformer on concatenated context + mask tokens `[B, N, D_pred]`.
///
/// Token assembly (embed, positional gather, mask token) is done on the CPU in
/// [`super::predictor::BrainJepaPredictor::predict_f32`].
pub fn build_predictor_graph(spec: &PredictorSpec, attn_layout: AttnLayout) -> Graph {
    let mut g = Graph::new("brainjepa_predictor");
    let b = spec.b;
    let d_enc = spec.enc_dim;
    let d = spec.pred_dim;
    let kc = spec.n_ctx;
    let kp = spec.n_pred;
    let n = kc + kp;

    let mut x = g.input("tokens", Shape::new(&[b, n, d], DType::F32));

    let mut blk_spec = spec.clone();
    blk_spec.n_ctx = kc;
    blk_spec.n_pred = kp;
    for i in 0..spec.depth {
        x = pred_attn_block(&mut g, x, &blk_spec, attn_layout, i);
    }

    let ln_w = g.param("predictor.predictor_norm.weight", s1(d));
    let ln_b = g.param("predictor.predictor_norm.bias", s1(d));
    x = g.ln(x, ln_w, ln_b, spec.norm_eps);

    x = g.narrow_(x, 1, kc, kp);

    let proj_w = g.param("predictor.predictor_proj.weight", s2(d, d_enc));
    let proj_b = g.param("predictor.predictor_proj.bias", s1(d_enc));
    let proj_mm = g.mm(x, proj_w);
    let out = g.add(proj_mm, proj_b);

    g.set_outputs(vec![out]);
    g
}
