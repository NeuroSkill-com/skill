//! EEG-DINO encoder expressed as an RLX IR graph.
//!
//! RLX requires shapes to be known when a graph is built. We therefore build
//! one graph per `(batch, channels, patches)` shape and cache compiled graphs.

use rlx::ir::GraphExt;
use rlx::ops::Activation;
use rlx::prelude::*;

use crate::config::ModelConfig;

// Auxiliary per-graph constants (not present in safetensors).
pub const KEY_PAD_ZEROS: &str = "__eegdino.pad_zeros";
pub const KEY_INV_PATCH: &str = "__eegdino.inv_patch";
pub const KEY_DFT_COS_T: &str = "__eegdino.dft_cos_t";
pub const KEY_DFT_SIN_T: &str = "__eegdino.dft_sin_t";
/// Precomputed `channel_embedding(identity) + bias` as `[1, C, 1, D]` (no matmul at runtime).
pub const KEY_CHANNEL_EMB: &str = "__eegdino.channel_emb";

#[derive(Clone, Copy, Debug)]
pub struct EncoderSpec {
    pub b: usize,
    pub c: usize,
    pub p: usize,
}

/// Stable IR node ids for encoder stage profiling (no shape-based guessing).
#[derive(Clone, Copy, Debug)]
pub struct EncoderProfileTaps {
    pub conv3_gn_gelu: NodeId,
    pub patch_emb: NodeId,
    pub spectral_mag: NodeId,
    pub pre_transformer: NodeId,
    pub layer_0_attention: NodeId,
    pub layer_5_attention: NodeId,
    pub layer_11_attention: NodeId,
    pub concat_global: NodeId,
    pub qkv_l0: NodeId,
    pub mlp_fc1_l0: NodeId,
    pub full_encoder: NodeId,
}

impl EncoderProfileTaps {
    pub fn checkpoints(&self) -> [(&'static str, NodeId); 11] {
        [
            ("conv3_gn_gelu", self.conv3_gn_gelu),
            ("patch_emb", self.patch_emb),
            ("spectral_mag", self.spectral_mag),
            ("pre_transformer", self.pre_transformer),
            ("layer_0_attention", self.layer_0_attention),
            ("layer_5_attention", self.layer_5_attention),
            ("layer_11_attention", self.layer_11_attention),
            ("concat_global", self.concat_global),
            ("qkv_l0", self.qkv_l0),
            ("mlp_fc1_l0", self.mlp_fc1_l0),
            ("full_encoder", self.full_encoder),
        ]
    }

    /// Prefix stages through patch embedding + spectral (4 compiles).
    pub fn checkpoints_early(&self) -> [(&'static str, NodeId); 4] {
        [
            ("conv3_gn_gelu", self.conv3_gn_gelu),
            ("patch_emb", self.patch_emb),
            ("spectral_mag", self.spectral_mag),
            ("pre_transformer", self.pre_transformer),
        ]
    }
}

fn s1(d: usize) -> Shape {
    Shape::new(&[d], DType::F32)
}
fn s2(a: usize, b: usize) -> Shape {
    Shape::new(&[a, b], DType::F32)
}
fn s3(a: usize, b: usize, c: usize) -> Shape {
    Shape::new(&[a, b, c], DType::F32)
}
fn s4(a: usize, b: usize, c: usize, d: usize) -> Shape {
    Shape::new(&[a, b, c, d], DType::F32)
}

pub fn build_encoder_graph(cfg: &ModelConfig, spec: &EncoderSpec) -> Graph {
    build_encoder_graph_with_taps(cfg, spec).0
}

pub fn build_encoder_graph_with_taps(cfg: &ModelConfig, spec: &EncoderSpec) -> (Graph, EncoderProfileTaps) {
    let b = spec.b;
    let c = spec.c;
    let p = spec.p;

    let patch = cfg.patch_size;
    let d = cfg.feature_size;
    let k = cfg.spectral_bins();
    let h = cfg.num_heads;
    let dh = d / h;
    let hd = h * dh;
    let ff = cfg.dim_feedforward;

    let conv1_c = cfg.conv_channels[0];
    let conv2_c = cfg.conv_channels[1];
    let conv3_c = cfg.conv_channels[2];

    let gn1_g = cfg.norm_groups[0];
    let gn2_g = cfg.norm_groups[1];
    let gn3_g = cfg.norm_groups[2];

    let eps_ln = cfg.layer_norm_eps as f32;
    let eps_gn = 1e-5f32;

    let mut g = Graph::new("eegdino_encoder");

    // Input signal: [B, C, P, L]
    let x_in = g.input("x", s4(b, c, p, patch));

    // ── Patch embedding ──────────────────────────────────────────────────
    // Reshape to NCHW: [B, 1, C*P, L]
    let h_tokens = c * p;
    let x_conv = g.reshape_(x_in, vec![b as i64, 1, h_tokens as i64, patch as i64]);

    // Pad width by 24 zeros on both sides: concat([zeros, x, zeros], axis=3)
    let pad_w = 24usize;
    let zpad = g.param(KEY_PAD_ZEROS, s4(b, 1, h_tokens, pad_w));
    let x_pad = g.concat_(vec![zpad, x_conv, zpad], 3);

    // Conv1: weight [C1,1,1,49], stride (1,25), valid padding
    let w1 = g.param(
        "patch_embedding.proj_in.conv1.weight",
        s4(conv1_c, 1, 1, 49),
    );
    let b1 = g.param("patch_embedding.proj_in.conv1.bias", s1(conv1_c));
    let b1 = g.reshape_(b1, vec![1, conv1_c as i64, 1, 1]);
    let y1c = g.conv2d(x_pad, w1, [1, 49], [1, 25], [0, 0], [1, 1], 1);
    let y1 = g.add(y1c, b1);
    let gn1_w = g.param("patch_embedding.proj_in.norm1.weight", s1(conv1_c));
    let gn1_b = g.param("patch_embedding.proj_in.norm1.bias", s1(conv1_c));
    let y1 = g.group_norm(y1, gn1_w, gn1_b, gn1_g, eps_gn);
    let y1 = g.gelu(y1);

    // Conv2: weight [C2,C1,1,3], padding (0,1)
    let w2 = g.param(
        "patch_embedding.proj_in.conv2.weight",
        s4(conv2_c, conv1_c, 1, 3),
    );
    let b2 = g.param("patch_embedding.proj_in.conv2.bias", s1(conv2_c));
    let b2 = g.reshape_(b2, vec![1, conv2_c as i64, 1, 1]);
    let y2c = g.conv2d(y1, w2, [1, 3], [1, 1], [0, 1], [1, 1], 1);
    let y2 = g.add(y2c, b2);
    let gn2_w = g.param("patch_embedding.proj_in.norm2.weight", s1(conv2_c));
    let gn2_b = g.param("patch_embedding.proj_in.norm2.bias", s1(conv2_c));
    let y2 = g.group_norm(y2, gn2_w, gn2_b, gn2_g, eps_gn);
    let y2 = g.gelu(y2);

    // Conv3: weight [C3,C2,1,3], padding (0,1)
    let w3 = g.param(
        "patch_embedding.proj_in.conv3.weight",
        s4(conv3_c, conv2_c, 1, 3),
    );
    let b3 = g.param("patch_embedding.proj_in.conv3.bias", s1(conv3_c));
    let b3 = g.reshape_(b3, vec![1, conv3_c as i64, 1, 1]);
    let y3c = g.conv2d(y2, w3, [1, 3], [1, 1], [0, 1], [1, 1], 1);
    let y3 = g.add(y3c, b3);
    let gn3_w = g.param("patch_embedding.proj_in.norm3.weight", s1(conv3_c));
    let gn3_b = g.param("patch_embedding.proj_in.norm3.bias", s1(conv3_c));
    let y3 = g.group_norm(y3, gn3_w, gn3_b, gn3_g, eps_gn);
    let conv3_gn_gelu = g.gelu(y3);

    // [B, C3, H, 8] -> [B, H, C3, 8] -> [B, C, P, D]
    let y3 = g.transpose_(conv3_gn_gelu, vec![0, 2, 1, 3]);
    let patch_emb = g.reshape_(y3, vec![b as i64, c as i64, p as i64, d as i64]);

    // Spectral stream: flat = [B*C*P, L]
    let total = b * c * p;
    let flat = g.reshape_(x_in, vec![total as i64, patch as i64]);
    let cos_t = g.param(KEY_DFT_COS_T, s2(patch, k));
    let sin_t = g.param(KEY_DFT_SIN_T, s2(patch, k));
    let real = g.mm(flat, cos_t);
    let imag = g.mm(flat, sin_t);
    let real2 = g.mul(real, real);
    let imag2 = g.mul(imag, imag);
    let sum = g.add(real2, imag2);
    let mag = g.sqrt(sum);
    let inv = g.param(KEY_INV_PATCH, s1(1));
    let mag = g.mul(mag, inv);
    let spectral_mag = g.reshape_(mag, vec![b as i64, c as i64, p as i64, k as i64]);

    // spectral_proj: fused [*,K] @ [K,D] + bias
    let sp_w = g.param("patch_embedding.spectral_proj.weight", s2(k, d));
    let sp_b = g.param("patch_embedding.spectral_proj.bias", s1(d));
    let mag2 = g.reshape_(spectral_mag, vec![total as i64, k as i64]);
    let sp = g.linear_fused(mag2, sp_w, sp_b, None, s2(total, d));
    let sp = g.reshape_(sp, vec![b as i64, c as i64, p as i64, d as i64]);

    let mut emb = g.add(patch_emb, sp);

    // Channel stream: constant [1,C,1,D] (identity one-hot @ W + b, precomputed at load).
    let ch = g.param(KEY_CHANNEL_EMB, s4(1, c, 1, d));
    emb = g.add(emb, ch);

    // Time encoding depthwise conv: [B,C,P,D] -> NCHW [B,D,C,P]
    let te_in = g.transpose_(emb, vec![0, 3, 1, 2]);
    let te_w = g.param("patch_embedding.time_encoding.weight", s4(d, 1, 1, 5));
    let te_b = g.param("patch_embedding.time_encoding.bias", s1(d));
    let te_b = g.reshape_(te_b, vec![1, d as i64, 1, 1]);
    let tec = g.conv2d(te_in, te_w, [1, 5], [1, 1], [0, 2], [1, 1], d);
    let te = g.add(tec, te_b);
    let te = g.transpose_(te, vec![0, 2, 3, 1]);
    emb = g.add(emb, te);

    // ── Transformer ───────────────────────────────────────────────────────
    // Flatten to [B, S, D] where S=C*P initially
    let n = c * p;
    let mut s = n;
    let mut x = g.reshape_(emb, vec![b as i64, s as i64, d as i64]);
    let pre_transformer = x;

    // Global tokens already expanded per-graph: [B, G, D]
    let gtok = cfg.num_global_tokens;
    let global = g.param("global_tokens", s3(b, gtok, d));

    let mut layer_0_attention = pre_transformer;
    let mut layer_5_attention = pre_transformer;
    let mut layer_11_attention = pre_transformer;
    let mut concat_global = pre_transformer;
    let mut qkv_l0 = pre_transformer;
    let mut mlp_fc1_l0 = pre_transformer;

    for i in 0..cfg.num_layers {
        // norm1
        let n1_w = g.param(&format!("encoder_layers.{i}.norm1.weight"), s1(d));
        let n1_b = g.param(&format!("encoder_layers.{i}.norm1.bias"), s1(d));
        let x1 = g.ln(x, n1_w, n1_b, eps_ln);

        // qkv = fused x1 @ w + b  (bias pre-fused at load time)
        let qkv_w = g.param(
            &format!("encoder_layers.{i}.attn.qkv.weight"),
            s2(d, 3 * hd),
        );
        let qkv_b = g.param(&format!("encoder_layers.{i}.attn.qkv.bias"), s1(3 * hd));
        let qkv = g.linear_fused(x1, qkv_w, qkv_b, None, s3(b, s, 3 * hd));
        if i == 0 {
            qkv_l0 = qkv;
        }

        // [B,S,3HD] -> [B,S,3,H,Dh] -> Q/K/V as [B,S,H,Dh]
        let qkv4 = g.reshape_(qkv, vec![b as i64, s as i64, 3, h as i64, dh as i64]);
        let q0 = g.narrow_(qkv4, 2, 0, 1);
        let k0 = g.narrow_(qkv4, 2, 1, 1);
        let v0 = g.narrow_(qkv4, 2, 2, 1);
        let q = g.reshape_(q0, vec![b as i64, s as i64, h as i64, dh as i64]);
        let k_ = g.reshape_(k0, vec![b as i64, s as i64, h as i64, dh as i64]);
        let v = g.reshape_(v0, vec![b as i64, s as i64, h as i64, dh as i64]);

        let ctx = g.attention_kind(q, k_, v, h, dh, rlx::ops::MaskKind::None, s4(b, s, h, dh));
        let ctx = g.reshape_(ctx, vec![b as i64, s as i64, hd as i64]);
        match i {
            0 => layer_0_attention = ctx,
            5 => layer_5_attention = ctx,
            11 => layer_11_attention = ctx,
            _ => {}
        }

        // proj (fused matmul + bias)
        let p_w = g.param(&format!("encoder_layers.{i}.attn.proj.weight"), s2(hd, d));
        let p_b = g.param(&format!("encoder_layers.{i}.attn.proj.bias"), s1(d));
        let attn_out = g.linear_fused(ctx, p_w, p_b, None, s3(b, s, d));

        // residual
        x = g.add(x, attn_out);

        // norm2
        let n2_w = g.param(&format!("encoder_layers.{i}.norm2.weight"), s1(d));
        let n2_b = g.param(&format!("encoder_layers.{i}.norm2.bias"), s1(d));
        let x2 = g.ln(x, n2_w, n2_b, eps_ln);

        // mlp
        let fc1_w = g.param(&format!("encoder_layers.{i}.mlp.fc1.weight"), s2(d, ff));
        let fc1_b = g.param(&format!("encoder_layers.{i}.mlp.fc1.bias"), s1(ff));
        let fc2_w = g.param(&format!("encoder_layers.{i}.mlp.fc2.weight"), s2(ff, d));
        let fc2_b = g.param(&format!("encoder_layers.{i}.mlp.fc2.bias"), s1(d));

        let m = g.linear_fused(x2, fc1_w, fc1_b, Some(Activation::Gelu), s3(b, s, ff));
        if i == 0 {
            mlp_fc1_l0 = m;
        }
        let m = g.linear_fused(m, fc2_w, fc2_b, None, s3(b, s, d));

        x = g.add(x, m);

        // Inject global tokens after the full layer (matches Burn `EEGEncoder`).
        if i + 1 == cfg.global_token_layer {
            x = g.concat_(vec![global, x], 1);
            concat_global = x;
            s += gtok;
        }
    }

    let full_encoder = x;
    g.set_outputs(vec![full_encoder]);
    let taps = EncoderProfileTaps {
        conv3_gn_gelu,
        patch_emb,
        spectral_mag,
        pre_transformer,
        layer_0_attention,
        layer_5_attention,
        layer_11_attention,
        concat_global,
        qkv_l0,
        mlp_fc1_l0,
        full_encoder,
    };
    (g, taps)
}
