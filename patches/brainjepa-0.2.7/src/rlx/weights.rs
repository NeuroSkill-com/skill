//! Safetensors → flat parameter map loader for the RLX backend.
//!
//! Produces plain `Vec<f32>` buffers for `rlx::CompiledGraph::set_param`.

use std::collections::HashMap;

use half::bf16;
use safetensors::SafeTensors;

use crate::config::ModelConfig;

#[derive(Clone, Debug)]
pub struct ParamBuf {
    pub data: Vec<f32>,
    pub shape: Vec<usize>,
}

pub type ParamMap = HashMap<String, ParamBuf>;

pub fn load_safetensors(path: &str) -> anyhow::Result<HashMap<String, ParamBuf>> {
    let bytes = std::fs::read(path)?;
    let st = SafeTensors::deserialize(&bytes)?;
    let mut out = HashMap::with_capacity(st.len());
    for (raw_key, view) in st.tensors() {
        let key = raw_key
            .strip_prefix("module.")
            .unwrap_or(raw_key.as_str())
            .to_string();
        let shape: Vec<usize> = view.shape().to_vec();
        let data = match view.dtype() {
            safetensors::Dtype::BF16 => view
                .data()
                .chunks_exact(2)
                .map(|b| bf16::from_le_bytes([b[0], b[1]]).to_f32())
                .collect(),
            safetensors::Dtype::F16 => view
                .data()
                .chunks_exact(2)
                .map(|b| half::f16::from_le_bytes([b[0], b[1]]).to_f32())
                .collect(),
            safetensors::Dtype::F32 => view
                .data()
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect(),
            other => anyhow::bail!("unsupported safetensors dtype {:?} for key {}", other, key),
        };
        out.insert(key, ParamBuf { data, shape });
    }
    Ok(out)
}

fn transpose(data: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    let mut out = vec![0f32; data.len()];
    for r in 0..rows {
        for c in 0..cols {
            out[c * rows + r] = data[r * cols + c];
        }
    }
    out
}

fn take(raw: &mut HashMap<String, ParamBuf>, key: &str) -> anyhow::Result<ParamBuf> {
    raw.remove(key)
        .ok_or_else(|| anyhow::anyhow!("missing weight key: {key}"))
}

/// Checkpoint prefix for the online encoder (`encoder` or `target_encoder`).
pub fn encoder_prefix(raw: &HashMap<String, ParamBuf>) -> &'static str {
    if raw.contains_key("target_encoder.blocks.0.norm1.weight") {
        "target_encoder"
    } else {
        "encoder"
    }
}

/// Conv2d patch weight `[D, 1, 1, PS]` → matmul weight `[PS, D]` for `patches @ W`.
fn patch_embed_weight_for_matmul(w: ParamBuf) -> anyhow::Result<ParamBuf> {
    let &[d, one, one2, ps] = w.shape.as_slice() else {
        anyhow::bail!("patch_embed weight: expected rank-4, got {:?}", w.shape);
    };
    anyhow::ensure!(
        one == 1 && one2 == 1,
        "patch_embed weight: expected [D,1,1,PS]"
    );
    anyhow::ensure!(w.data.len() == d * ps, "patch_embed weight length mismatch");
    let mut out = vec![0f32; ps * d];
    for di in 0..d {
        for pi in 0..ps {
            out[pi * d + di] = w.data[di * ps + pi];
        }
    }
    Ok(ParamBuf {
        data: out,
        shape: vec![ps, d],
    })
}

pub fn take_linear_w(raw: &mut HashMap<String, ParamBuf>, key: &str) -> anyhow::Result<ParamBuf> {
    let p = take(raw, key)?;
    anyhow::ensure!(
        p.shape.len() == 2,
        "Linear weight {key} must be 2-D, got {:?}",
        p.shape
    );
    let (out_d, in_d) = (p.shape[0], p.shape[1]);
    let data = transpose(&p.data, out_d, in_d);
    Ok(ParamBuf {
        data,
        shape: vec![in_d, out_d],
    })
}

/// Build the encoder parameter map (graph-ready) from a raw safetensors map.
///
/// Also returns the gradient-projection weights (if present) transposed to
/// `[grad_dim, embed_dim/2]`, plus its bias `[embed_dim/2]`.
pub fn build_encoder_params(
    raw: &mut HashMap<String, ParamBuf>,
    cfg: &ModelConfig,
) -> anyhow::Result<(ParamMap, Option<(Vec<f32>, Vec<f32>, usize)>)> {
    let mut p = ParamMap::new();
    let d = cfg.embed_dim;
    let ckpt = encoder_prefix(raw);

    // Graph param names stay `encoder.*`; checkpoint keys use `ckpt` prefix.
    p.insert(
        "encoder.patch_embed.proj.weight".into(),
        patch_embed_weight_for_matmul(take(raw, &format!("{ckpt}.patch_embed.proj.weight"))?)?,
    );
    p.insert(
        "encoder.patch_embed.proj.bias".into(),
        take(raw, &format!("{ckpt}.patch_embed.proj.bias"))?,
    );

    for i in 0..cfg.depth {
        let ck = format!("{ckpt}.blocks.{i}");
        let gq = format!("encoder.blocks.{i}");
        for k in ["norm1.weight", "norm1.bias", "norm2.weight", "norm2.bias"] {
            p.insert(format!("{gq}.{k}"), take(raw, &format!("{ck}.{k}"))?);
        }
        p.insert(
            format!("{gq}.attn.qkv.weight"),
            take_linear_w(raw, &format!("{ck}.attn.qkv.weight"))?,
        );
        let qkv_bias = format!("{ck}.attn.qkv.bias");
        if raw.contains_key(&qkv_bias) {
            p.insert(format!("{gq}.attn.qkv.bias"), take(raw, &qkv_bias)?);
        } else {
            p.insert(
                format!("{gq}.attn.qkv.bias"),
                ParamBuf {
                    data: vec![0.0; 3 * d],
                    shape: vec![3 * d],
                },
            );
        }
        p.insert(
            format!("{gq}.attn.proj.weight"),
            take_linear_w(raw, &format!("{ck}.attn.proj.weight"))?,
        );
        p.insert(
            format!("{gq}.attn.proj.bias"),
            take(raw, &format!("{ck}.attn.proj.bias"))?,
        );
        p.insert(
            format!("{gq}.mlp.fc1.weight"),
            take_linear_w(raw, &format!("{ck}.mlp.fc1.weight"))?,
        );
        p.insert(
            format!("{gq}.mlp.fc1.bias"),
            take(raw, &format!("{ck}.mlp.fc1.bias"))?,
        );
        p.insert(
            format!("{gq}.mlp.fc2.weight"),
            take_linear_w(raw, &format!("{ck}.mlp.fc2.weight"))?,
        );
        p.insert(
            format!("{gq}.mlp.fc2.bias"),
            take(raw, &format!("{ck}.mlp.fc2.bias"))?,
        );
    }

    p.insert(
        "encoder.norm.weight".into(),
        take(raw, &format!("{ckpt}.norm.weight"))?,
    );
    p.insert(
        "encoder.norm.bias".into(),
        take(raw, &format!("{ckpt}.norm.bias"))?,
    );

    let grad_w_key = format!("{ckpt}.pos_embed_proj.predictor_pos_embed_proj.weight");
    let grad_b_key = format!("{ckpt}.pos_embed_proj.predictor_pos_embed_proj.bias");
    let grad_proj = if raw.contains_key(&grad_w_key) && raw.contains_key(&grad_b_key) {
        let w = take_linear_w(raw, &grad_w_key)?;
        let b = take(raw, &grad_b_key)?;
        let grad_dim = w.shape[0];
        Some((w.data, b.data, grad_dim))
    } else {
        None
    };

    Ok((p, grad_proj))
}

pub fn apply_params(compiled: &mut rlx::CompiledGraph, params: &ParamMap) {
    for (name, buf) in params {
        compiled.set_param(name, &buf.data);
    }
}

/// Predictor + shared encoder params for JEPA (`predictor.*` prefix in checkpoint).
pub fn build_predictor_params(
    raw: &mut HashMap<String, ParamBuf>,
    cfg: &ModelConfig,
) -> anyhow::Result<(ParamMap, Option<(Vec<f32>, Vec<f32>, usize)>)> {
    let mut p = ParamMap::new();
    let prefix = "predictor";
    let d = cfg.pred_emb_dim;
    let d_enc = cfg.embed_dim;

    p.insert(
        "predictor.predictor_embed.weight".into(),
        take_linear_w(raw, &format!("{prefix}.predictor_embed.weight"))?,
    );
    if raw.contains_key(&format!("{prefix}.predictor_embed.bias")) {
        p.insert(
            "predictor.predictor_embed.bias".into(),
            take(raw, &format!("{prefix}.predictor_embed.bias"))?,
        );
    } else {
        p.insert(
            "predictor.predictor_embed.bias".into(),
            ParamBuf {
                data: vec![0.0; d],
                shape: vec![d],
            },
        );
    }

    p.insert(
        "predictor.mask_token".into(),
        take(raw, &format!("{prefix}.mask_token"))?,
    );

    for i in 0..cfg.pred_depth {
        let ck = format!("{prefix}.predictor_blocks.{i}");
        let gq = format!("predictor.predictor_blocks.{i}");
        for k in ["norm1.weight", "norm1.bias", "norm2.weight", "norm2.bias"] {
            p.insert(format!("{gq}.{k}"), take(raw, &format!("{ck}.{k}"))?);
        }
        p.insert(
            format!("{gq}.attn.qkv.weight"),
            take_linear_w(raw, &format!("{ck}.attn.qkv.weight"))?,
        );
        let qkv_bias = format!("{ck}.attn.qkv.bias");
        if raw.contains_key(&qkv_bias) {
            p.insert(format!("{gq}.attn.qkv.bias"), take(raw, &qkv_bias)?);
        } else {
            p.insert(
                format!("{gq}.attn.qkv.bias"),
                ParamBuf {
                    data: vec![0.0; 3 * d],
                    shape: vec![3 * d],
                },
            );
        }
        p.insert(
            format!("{gq}.attn.proj.weight"),
            take_linear_w(raw, &format!("{ck}.attn.proj.weight"))?,
        );
        p.insert(
            format!("{gq}.attn.proj.bias"),
            take(raw, &format!("{ck}.attn.proj.bias"))?,
        );
        p.insert(
            format!("{gq}.mlp.fc1.weight"),
            take_linear_w(raw, &format!("{ck}.mlp.fc1.weight"))?,
        );
        p.insert(
            format!("{gq}.mlp.fc1.bias"),
            take(raw, &format!("{ck}.mlp.fc1.bias"))?,
        );
        p.insert(
            format!("{gq}.mlp.fc2.weight"),
            take_linear_w(raw, &format!("{ck}.mlp.fc2.weight"))?,
        );
        p.insert(
            format!("{gq}.mlp.fc2.bias"),
            take(raw, &format!("{ck}.mlp.fc2.bias"))?,
        );
    }

    p.insert(
        "predictor.predictor_norm.weight".into(),
        take(raw, &format!("{prefix}.predictor_norm.weight"))?,
    );
    p.insert(
        "predictor.predictor_norm.bias".into(),
        take(raw, &format!("{prefix}.predictor_norm.bias"))?,
    );
    p.insert(
        "predictor.predictor_proj.weight".into(),
        take_linear_w(raw, &format!("{prefix}.predictor_proj.weight"))?,
    );
    if raw.contains_key(&format!("{prefix}.predictor_proj.bias")) {
        p.insert(
            "predictor.predictor_proj.bias".into(),
            take(raw, &format!("{prefix}.predictor_proj.bias"))?,
        );
    } else {
        p.insert(
            "predictor.predictor_proj.bias".into(),
            ParamBuf {
                data: vec![0.0; d_enc],
                shape: vec![d_enc],
            },
        );
    }

    let grad_w_key = format!("{prefix}.predictor_2dpe_proj.predictor_pos_embed_proj.weight");
    let grad_b_key = format!("{prefix}.predictor_2dpe_proj.predictor_pos_embed_proj.bias");
    let pred_grad = if raw.contains_key(&grad_w_key) && raw.contains_key(&grad_b_key) {
        let w = take_linear_w(raw, &grad_w_key)?;
        let b = take(raw, &grad_b_key)?;
        let grad_dim = w.shape[0];
        Some((w.data, b.data, grad_dim))
    } else {
        None
    };

    Ok((p, pred_grad))
}

/// Build `[1, N, pred_dim]` positional table for the predictor (mapping mode).
pub fn build_predictor_pos_embed(
    mode: &str,
    n_rois: usize,
    n_time_patches: usize,
    pred_dim: usize,
    gradient_xyz: &[f32],
    grad_dim: usize,
    grad_proj: Option<(&[f32], &[f32])>,
) -> anyhow::Result<Vec<f32>> {
    let (w, b) = match grad_proj {
        Some((w, b)) => (Some(w), Some(b)),
        None => (None, None),
    };
    crate::rlx::pos_embed_cpu::build_pos_embed(
        mode,
        n_rois,
        n_time_patches,
        pred_dim,
        gradient_xyz,
        grad_dim,
        w,
        b,
    )
    .map_err(Into::into)
}
