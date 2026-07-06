//! REVE expressed as an RLX IR graph.
//!
//! The RLX graph runs the full model given already-prepared patch tokens and
//! 4D positions:
//! - `patches`: `[B, S, patch_size]` where `S = n_chans * n_patches`
//! - `pos_embed`: precomputed `[B, S, embed_dim]` positional embeddings
//! - `cls_q`:   `[B, 1, embed_dim]` (only when attention pooling is enabled)

use rlx::ir::GraphExt;
use rlx::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const KEY_ZEROS_EMBED: &str = "__reve.zeros_embed";
pub const KEY_ATTN_SCALE: &str = "__reve.attn_scale";
pub const KEY_ATTN_HEAD_SCALE: &str = "__reve.attn_head_scale";

/// Global LoRA rank for attention projections (q/k/v/o). When 0 (default),
/// graphs are built without LoRA paths. Set via `set_lora_rank` before
/// calling `build_reve_*_graph` from the trainer; the value is read by
/// `transformer_block`.
static LORA_RANK: AtomicUsize = AtomicUsize::new(0);

pub fn set_lora_rank(r: usize) { LORA_RANK.store(r, Ordering::SeqCst); }
pub fn get_lora_rank() -> usize { LORA_RANK.load(Ordering::SeqCst) }

#[derive(Clone, Copy, Debug)]
pub struct ReveSpec {
    pub b: usize,
    pub s: usize, // n_chans * n_patches
    pub patch_size: usize,
    pub embed_dim: usize,
    pub n_outputs: usize,
    pub depth: usize,
    pub heads: usize,
    pub head_dim: usize,
    pub mlp_dim: usize,
    pub use_geglu: bool,
    pub freqs: usize,
    pub attention_pooling: bool,
}

fn s1(d: usize) -> Shape {
    Shape::new(&[d], DType::F32)
}
fn s2_(a: usize, b: usize) -> Shape {
    Shape::new(&[a, b], DType::F32)
}
fn s3(a: usize, b: usize, c: usize) -> Shape {
    Shape::new(&[a, b, c], DType::F32)
}

fn attention_pool(
    g: &mut Graph,
    q: NodeId, // [B,1,D]
    x: NodeId, // [B,S,D] used as both K and V
    _b: usize,
    _s: usize,
    _d: usize,
) -> NodeId {
    // scores = q @ x^T  => [B,1,S]
    let x_t = g.transpose_(x, vec![0, 2, 1]); // [B,D,S]
    let scores = g.mm(q, x_t);
    let scale = g.param(KEY_ATTN_SCALE, s1(1)); // scalar
    let scores = g.mul(scores, scale);
    let w = g.sm(scores, 2); // softmax over S
    // out = w @ x  => [B,1,D]
    g.mm(w, x)
}

fn geglu(g: &mut Graph, x: NodeId, w_gate: NodeId, w_up: NodeId) -> NodeId {
    let gates = g.mm(x, w_gate);
    let up = g.mm(x, w_up);
    let g_act = g.gelu(gates);
    g.mul(g_act, up)
}

/// `Linear(x) + x·A·B` LoRA injection. `A` is `[in, rank]`, `B` is
/// `[rank, out]`. Param names are namespaced by layer + role so the
/// trainer can find them.
fn lora_proj(
    g: &mut Graph,
    x: NodeId,
    w: NodeId,
    in_dim: usize,
    out_dim: usize,
    layer_idx: usize,
    role: &str,      // "q" | "k" | "v" | "o"
    rank: usize,
) -> NodeId {
    let xw = g.mm(x, w);
    let a = g.param(
        format!("transformer.layers.{layer_idx}.0.lora_{role}_a"),
        s2_(in_dim, rank),
    );
    let b = g.param(
        format!("transformer.layers.{layer_idx}.0.lora_{role}_b"),
        s2_(rank, out_dim),
    );
    let xa = g.mm(x, a);
    let xab = g.mm(xa, b);
    g.add(xw, xab)
}

/// Same attention math as `self_attention` but takes pre-computed
/// `q/k/v` (so LoRA paths can mix into them externally) and runs the
/// inner attention without applying `wo` — the caller is responsible
/// for the output projection (also LoRA-able).
fn self_attention_with_qkv(
    g: &mut Graph,
    q: NodeId,
    k: NodeId,
    v: NodeId,
    attn_scale: NodeId,
    spec: &ReveSpec,
) -> NodeId {
    let b = spec.b;
    let s = spec.s;
    let nh = spec.heads;
    let dh = spec.head_dim;
    let inner = nh * dh;

    let q4 = g.reshape_(q, vec![b as i64, s as i64, nh as i64, dh as i64]);
    let k4 = g.reshape_(k, vec![b as i64, s as i64, nh as i64, dh as i64]);
    let v4 = g.reshape_(v, vec![b as i64, s as i64, nh as i64, dh as i64]);
    let q_bhsd = g.transpose_(q4, vec![0, 2, 1, 3]);
    let k_bhsd = g.transpose_(k4, vec![0, 2, 1, 3]);
    let v_bhsd = g.transpose_(v4, vec![0, 2, 1, 3]);
    let bh = (b * nh) as i64;
    let q3 = g.reshape_(q_bhsd, vec![bh, s as i64, dh as i64]);
    let k3 = g.reshape_(k_bhsd, vec![bh, s as i64, dh as i64]);
    let v3 = g.reshape_(v_bhsd, vec![bh, s as i64, dh as i64]);

    let k_t = g.transpose_(k3, vec![0, 2, 1]);
    let scores = g.mm(q3, k_t);
    let scores = g.mul(scores, attn_scale);
    let w = g.sm(scores, 2);
    let attn_out = g.mm(w, v3);

    let attn_bhsd = g.reshape_(attn_out, vec![b as i64, nh as i64, s as i64, dh as i64]);
    let attn_bshd = g.transpose_(attn_bhsd, vec![0, 2, 1, 3]);
    g.reshape_(attn_bshd, vec![b as i64, s as i64, inner as i64])
}

/// Manual multi-head SDPA via matmul + softmax + matmul.
///
/// Avoids fused `attention_kind` kernels whose GPU implementations can
/// diverge from the CPU reference on long sequences (REVE uses S ≈ 176).
fn self_attention(
    g: &mut Graph,
    x: NodeId,
    wq: NodeId,
    wk: NodeId,
    wv: NodeId,
    wo: NodeId,
    attn_scale: NodeId,
    spec: &ReveSpec,
) -> NodeId {
    let b = spec.b;
    let s = spec.s;
    let nh = spec.heads;
    let dh = spec.head_dim;
    let inner = nh * dh;

    let q = g.mm(x, wq);
    let k = g.mm(x, wk);
    let v = g.mm(x, wv);

    // [B, S, inner] → [B*H, S, D] for batched matmul attention.
    let q4 = g.reshape_(q, vec![b as i64, s as i64, nh as i64, dh as i64]);
    let k4 = g.reshape_(k, vec![b as i64, s as i64, nh as i64, dh as i64]);
    let v4 = g.reshape_(v, vec![b as i64, s as i64, nh as i64, dh as i64]);
    let q_bhsd = g.transpose_(q4, vec![0, 2, 1, 3]);
    let k_bhsd = g.transpose_(k4, vec![0, 2, 1, 3]);
    let v_bhsd = g.transpose_(v4, vec![0, 2, 1, 3]);
    let bh = (b * nh) as i64;
    let q3 = g.reshape_(q_bhsd, vec![bh, s as i64, dh as i64]);
    let k3 = g.reshape_(k_bhsd, vec![bh, s as i64, dh as i64]);
    let v3 = g.reshape_(v_bhsd, vec![bh, s as i64, dh as i64]);

    let k_t = g.transpose_(k3, vec![0, 2, 1]);
    let scores = g.mm(q3, k_t);
    let scores = g.mul(scores, attn_scale);
    let w = g.sm(scores, 2);
    let attn_out = g.mm(w, v3);

    let attn_bhsd = g.reshape_(attn_out, vec![b as i64, nh as i64, s as i64, dh as i64]);
    let attn_bshd = g.transpose_(attn_bhsd, vec![0, 2, 1, 3]);
    let out3 = g.reshape_(attn_bshd, vec![b as i64, s as i64, inner as i64]);
    g.mm(out3, wo)
}

fn ffn(
    g: &mut Graph,
    x: NodeId,
    w1: NodeId,
    w2: NodeId,
    w_gate: Option<NodeId>,
    w_up: Option<NodeId>,
    spec: &ReveSpec,
) -> NodeId {
    let h = if spec.use_geglu {
        let (wg, wu) = (w_gate.expect("geglu gate"), w_up.expect("geglu up"));
        geglu(g, x, wg, wu)
    } else {
        let h1 = g.mm(x, w1);
        g.gelu(h1)
    };
    g.mm(h, w2)
}

fn transformer_block(
    g: &mut Graph,
    x: NodeId,
    spec: &ReveSpec,
    layer_idx: usize,
    zeros: NodeId,
    attn_scale: NodeId,
) -> NodeId {
    let d = spec.embed_dim;
    let inner = spec.heads * spec.head_dim;
    let rank = get_lora_rank();

    let an_g = g.param(format!("transformer.layers.{layer_idx}.0.norm.weight"), s1(d));
    let xn = g.rms_norm(x, an_g, zeros, 1e-6);

    let wq = g.param(
        format!("transformer.layers.{layer_idx}.0.to_q.weight"),
        s2_(d, inner),
    );
    let wk = g.param(
        format!("transformer.layers.{layer_idx}.0.to_k.weight"),
        s2_(d, inner),
    );
    let wv = g.param(
        format!("transformer.layers.{layer_idx}.0.to_v.weight"),
        s2_(d, inner),
    );
    let wo = g.param(
        format!("transformer.layers.{layer_idx}.0.to_out.weight"),
        s2_(inner, d),
    );

    // LoRA on q/k/v/o: when rank > 0, add `x·A·B` next to each `x·W`.
    // A is small-Gaussian-initialised, B is zero-initialised, so the
    // step-0 output exactly equals the frozen-base forward.
    let attn = if rank == 0 {
        self_attention(g, xn, wq, wk, wv, wo, attn_scale, spec)
    } else {
        let q = lora_proj(g, xn, wq, d, inner, layer_idx, "q", rank);
        let k = lora_proj(g, xn, wk, d, inner, layer_idx, "k", rank);
        let v = lora_proj(g, xn, wv, d, inner, layer_idx, "v", rank);
        // Attention math the same; only the o-projection gets the input
        // shape `[B, S, inner]` so its LoRA factors are `[inner, rank]`
        // and `[rank, d]`.
        let attn_inner = self_attention_with_qkv(g, q, k, v, attn_scale, spec);
        lora_proj(g, attn_inner, wo, inner, d, layer_idx, "o", rank)
    };
    let x = g.add(x, attn);

    // FFN
    let fn_g = g.param(format!("transformer.layers.{layer_idx}.1.net.0.weight"), s1(d));
    let hn = g.rms_norm(x, fn_g, zeros, 1e-6);
    let w2 = g.param(
        format!("transformer.layers.{layer_idx}.1.net.3.weight"),
        s2_(spec.mlp_dim, d),
    );
    let out = if spec.use_geglu {
        let wg = g.param(
            format!("transformer.layers.{layer_idx}.1.net.1.w_gate.weight"),
            s2_(d, spec.mlp_dim),
        );
        let wu = g.param(
            format!("transformer.layers.{layer_idx}.1.net.1.w_up.weight"),
            s2_(d, spec.mlp_dim),
        );
        ffn(g, hn, wg, w2, Some(wg), Some(wu), spec)
    } else {
        let w1 = g.param(
            format!("transformer.layers.{layer_idx}.1.net.1.weight"),
            s2_(d, spec.mlp_dim),
        );
        ffn(g, hn, w1, w2, None, None, spec)
    };
    g.add(x, out)
}

fn build_head_output(g: &mut Graph, h: NodeId, spec: &ReveSpec) -> NodeId {
    let b = spec.b;
    let s = spec.s;
    let d = spec.embed_dim;

    if spec.n_outputs == 0 {
        if spec.attention_pooling {
            let cls_q = g.input("cls_q", s3(b, 1, d));
            let pooled = attention_pool(g, cls_q, h, b, s, d);
            g.reshape_(pooled, vec![b as i64, d as i64])
        } else {
            g.mean(h, vec![1], false)
        }
    } else if spec.attention_pooling {
        let cls_q = g.input("cls_q", s3(b, 1, d));
        let pooled = attention_pool(g, cls_q, h, b, s, d);
        let pooled = g.reshape_(pooled, vec![b as i64, d as i64]);
        let ln_g = g.param("final_layer.0.weight", s1(d));
        let ln_b = g.param("final_layer.0.bias", s1(d));
        let pooled = g.ln(pooled, ln_g, ln_b, 1e-5);
        let w = g.param("final_layer.1.weight", s2_(d, spec.n_outputs));
        let b0 = g.param("final_layer.1.bias", s1(spec.n_outputs));
        let y = g.mm(pooled, w);
        g.add(y, b0)
    } else {
        let final_dim = spec.s * d;
        let flat = g.reshape_(h, vec![b as i64, final_dim as i64]);
        let ln_g = g.param("final_layer.1.weight", s1(final_dim));
        let ln_b = g.param("final_layer.1.bias", s1(final_dim));
        let flat = g.ln(flat, ln_g, ln_b, 1e-5);
        let w = g.param("final_layer.2.weight", s2_(final_dim, spec.n_outputs));
        let b0 = g.param("final_layer.2.bias", s1(spec.n_outputs));
        let y = g.mm(flat, w);
        g.add(y, b0)
    }
}

/// Compile a contiguous layer range. `layer_start == 0` reads patches + pos;
/// otherwise `hidden` `[B,S,D]` is the input. When `with_head`, runs attention pool.
pub fn build_reve_graph_range(
    spec: &ReveSpec,
    layer_start: usize,
    layer_end: usize,
    with_head: bool,
) -> Graph {
    let mut g = Graph::new("reve");
    let b = spec.b;
    let s = spec.s;
    let d = spec.embed_dim;
    let layer_end = layer_end.min(spec.depth);

    let zeros = g.param(KEY_ZEROS_EMBED, s1(d));
    let attn_scale = g.param(KEY_ATTN_HEAD_SCALE, s1(1));

    let mut h = if layer_start == 0 {
        let patches = g.input("patches", s3(b, s, spec.patch_size));
        let pos = g.input("pos_embed", s3(b, s, d));
        let pe_w = g.param("to_patch_embedding.0.weight", s2_(spec.patch_size, d));
        let pe_b = g.param("to_patch_embedding.0.bias", s1(d));
        let x0 = g.mm(patches, pe_w);
        let patch_emb = g.add(x0, pe_b);
        g.add(patch_emb, pos)
    } else {
        g.input("hidden", s3(b, s, d))
    };

    for i in layer_start..layer_end {
        h = transformer_block(&mut g, h, spec, i, zeros, attn_scale);
    }

    let out = if with_head {
        build_head_output(&mut g, h, spec)
    } else {
        h
    };
    g.set_outputs(vec![out]);
    g
}

pub fn build_reve_graph(spec: &ReveSpec) -> Graph {
    build_reve_graph_range(spec, 0, spec.depth, true)
}

/// Build a training graph: REVE backbone (with optional LoRA, set via
/// [`set_lora_rank`]) + mean-pool over `S` + linear classification head
/// + softmax-CE loss. Returns a graph whose single output is the scalar
/// mean loss.
///
/// Inputs: `patches [B,S,patch_size]`, `pos_embed [B,S,D]`,
///   `labels [B, num_classes]` (one-hot or soft).
/// Params introduced beyond the base REVE set: `head.weight [D, C]`,
///   `head.bias [C]`, plus the per-layer `lora_*_{a,b}` when rank > 0.
pub fn build_reve_classification_training_graph(
    spec: &ReveSpec,
    num_classes: usize,
) -> Graph {
    let mut g = Graph::new("reve_train");
    let b = spec.b;
    let s = spec.s;
    let d = spec.embed_dim;

    let zeros = g.param(KEY_ZEROS_EMBED, s1(d));
    let attn_scale = g.param(KEY_ATTN_HEAD_SCALE, s1(1));

    let patches = g.input("patches", s3(b, s, spec.patch_size));
    let pos = g.input("pos_embed", s3(b, s, d));
    let labels = g.input("labels", s2_(b, num_classes));
    let pe_w = g.param("to_patch_embedding.0.weight", s2_(spec.patch_size, d));
    let pe_b = g.param("to_patch_embedding.0.bias", s1(d));
    let x0 = g.mm(patches, pe_w);
    let patch_emb = g.add(x0, pe_b);
    let mut h = g.add(patch_emb, pos);

    for i in 0..spec.depth {
        h = transformer_block(&mut g, h, spec, i, zeros, attn_scale);
    }

    // Mean-pool over the S=n_chans·n_patches token axis → [B, D]
    let pooled = g.mean(h, vec![1], false);

    let hw = g.param("head.weight", s2_(d, num_classes));
    let hb = g.param("head.bias", s1(num_classes));
    let yw = g.mm(pooled, hw);
    let logits = g.add(yw, hb);

    // Softmax-CE: log_softmax = logits − lse; loss = −mean_b(Σ_c y·log_p).
    let row_max = g.reduce(logits, rlx::ir::op::ReduceOp::Max, vec![1], true, s2_(b, 1));
    let shifted = g.sub(logits, row_max);
    let exped = g.activation(rlx::ops::Activation::Exp, shifted, s2_(b, num_classes));
    let sum_exp = g.reduce(exped, rlx::ir::op::ReduceOp::Sum, vec![1], true, s2_(b, 1));
    let log_sum = g.activation(rlx::ops::Activation::Log, sum_exp, s2_(b, 1));
    let log_probs = g.sub(shifted, log_sum);
    let mul = g.mul(labels, log_probs);
    let per_class_sum = g.reduce(mul, rlx::ir::op::ReduceOp::Sum, vec![1], false, s1(b));
    let neg = g.activation(rlx::ops::Activation::Neg, per_class_sum, s1(b));
    let loss = g.mean(neg, vec![0], false);

    g.set_outputs(vec![loss]);
    g
}

/// Build the same graph as the training-graph but with the loss
/// replaced by raw logits — used for eval / inference without
/// needing the labels input.
pub fn build_reve_classification_eval_graph(
    spec: &ReveSpec,
    num_classes: usize,
) -> Graph {
    let mut g = Graph::new("reve_eval");
    let b = spec.b;
    let s = spec.s;
    let d = spec.embed_dim;

    let zeros = g.param(KEY_ZEROS_EMBED, s1(d));
    let attn_scale = g.param(KEY_ATTN_HEAD_SCALE, s1(1));

    let patches = g.input("patches", s3(b, s, spec.patch_size));
    let pos = g.input("pos_embed", s3(b, s, d));
    let pe_w = g.param("to_patch_embedding.0.weight", s2_(spec.patch_size, d));
    let pe_b = g.param("to_patch_embedding.0.bias", s1(d));
    let x0 = g.mm(patches, pe_w);
    let patch_emb = g.add(x0, pe_b);
    let mut h = g.add(patch_emb, pos);

    for i in 0..spec.depth {
        h = transformer_block(&mut g, h, spec, i, zeros, attn_scale);
    }
    let pooled = g.mean(h, vec![1], false);
    let hw = g.param("head.weight", s2_(d, num_classes));
    let hb = g.param("head.bias", s1(num_classes));
    let yw = g.mm(pooled, hw);
    let logits = g.add(yw, hb);
    g.set_outputs(vec![logits]);
    g
}

