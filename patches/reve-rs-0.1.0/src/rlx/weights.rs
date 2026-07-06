//! Safetensors → flat parameter loader for the RLX backend.
//!
//! Mirrors `crate::weights` but produces plain `Vec<f32>` buffers so they can
//! be pushed into an `rlx::CompiledGraph` with `set_param(name, &[f32])`.

use std::collections::HashMap;

use half::bf16;
use safetensors::SafeTensors;

use crate::config::ModelConfig;
use super::graph::{KEY_ATTN_HEAD_SCALE, KEY_ATTN_SCALE, KEY_ZEROS_EMBED};

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
            .strip_prefix("model.")
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
    raw.remove(key).ok_or_else(|| anyhow::anyhow!("missing weight key: {key}"))
}

fn take_linear_w(raw: &mut HashMap<String, ParamBuf>, key: &str) -> anyhow::Result<ParamBuf> {
    let p = take(raw, key)?;
    anyhow::ensure!(p.shape.len() == 2, "Linear weight {key} must be 2-D, got {:?}", p.shape);
    let (out_d, in_d) = (p.shape[0], p.shape[1]);
    let data = transpose(&p.data, out_d, in_d);
    Ok(ParamBuf { data, shape: vec![in_d, out_d] })
}

/// Split a fused QKV linear `[in, 3*inner]` into three `[in, inner]` mats.
fn split_qkv(w: ParamBuf, inner: usize) -> anyhow::Result<(ParamBuf, ParamBuf, ParamBuf)> {
    anyhow::ensure!(w.shape.len() == 2, "QKV weight must be 2-D, got {:?}", w.shape);
    let (d, cols) = (w.shape[0], w.shape[1]);
    anyhow::ensure!(cols == 3 * inner, "QKV cols mismatch: got {cols}, expected {}", 3 * inner);
    let mut wq = vec![0f32; d * inner];
    let mut wk = vec![0f32; d * inner];
    let mut wv = vec![0f32; d * inner];
    for r in 0..d {
        let row = r * cols;
        wq[r * inner..(r + 1) * inner].copy_from_slice(&w.data[row..row + inner]);
        wk[r * inner..(r + 1) * inner].copy_from_slice(&w.data[row + inner..row + 2 * inner]);
        wv[r * inner..(r + 1) * inner].copy_from_slice(&w.data[row + 2 * inner..row + 3 * inner]);
    }
    let shape = vec![d, inner];
    Ok((
        ParamBuf { data: wq, shape: shape.clone() },
        ParamBuf { data: wk, shape: shape.clone() },
        ParamBuf { data: wv, shape },
    ))
}

/// Split a GEGLU linear `[in, 2*hidden]` into value/gate `[in, hidden]` mats.
/// PyTorch `GEGLU` uses the first half as value and the second as gates.
fn split_geglu(w: ParamBuf, hidden: usize) -> anyhow::Result<(ParamBuf, ParamBuf)> {
    anyhow::ensure!(w.shape.len() == 2, "GEGLU weight must be 2-D, got {:?}", w.shape);
    let (d, cols) = (w.shape[0], w.shape[1]);
    anyhow::ensure!(cols == 2 * hidden, "GEGLU cols mismatch: got {cols}, expected {}", 2 * hidden);
    let mut w_up = vec![0f32; d * hidden];
    let mut w_gate = vec![0f32; d * hidden];
    for r in 0..d {
        let row = r * cols;
        w_up[r * hidden..(r + 1) * hidden].copy_from_slice(&w.data[row..row + hidden]);
        w_gate[r * hidden..(r + 1) * hidden].copy_from_slice(&w.data[row + hidden..row + 2 * hidden]);
    }
    let shape = vec![d, hidden];
    Ok((
        ParamBuf { data: w_up, shape: shape.clone() },
        ParamBuf { data: w_gate, shape },
    ))
}

fn take_vec(raw: &mut HashMap<String, ParamBuf>, key: &str, len: usize) -> anyhow::Result<ParamBuf> {
    let p = take(raw, key)?;
    anyhow::ensure!(p.shape == vec![len], "param {key} shape mismatch: {:?}", p.shape);
    Ok(p)
}

/// Precompute the Fourier frequency matrix `freq_t` used by the RLX graph.
///
/// Burn builds a 4D grid of `(fx,fy,fz,fw)` in `[0,freqs)` and projects
/// positions with `2π*freq/width`, then truncates to `half_dim = embed_dim/2`.
/// We store the transposed matrix as `[4, half_dim]` for `pos@[4,half]`.
pub fn build_freq_t(cfg: &ModelConfig) -> anyhow::Result<ParamBuf> {
    use std::f32::consts::PI;
    let freqs = cfg.freqs;
    let d = cfg.embed_dim;
    anyhow::ensure!(d % 2 == 0, "embed_dim must be even, got {}", d);
    let half_dim = d / 2;
    let margin = 0.4f32;
    let width = 1.0 + 2.0 * margin;
    let n_freq4 = freqs.pow(4);
    anyhow::ensure!(
        n_freq4 >= half_dim,
        "freqs^4 = {n_freq4} < embed_dim/2 = {half_dim}; increase freqs"
    );

    // Build first `half_dim` combos in the same nested-loop order as the Burn impl.
    let mut cols: Vec<[f32; 4]> = Vec::with_capacity(half_dim);
    'outer: for fx in 0..freqs {
        for fy in 0..freqs {
            for fz in 0..freqs {
                for fw in 0..freqs {
                    cols.push([
                        2.0 * PI * fx as f32 / width,
                        2.0 * PI * fy as f32 / width,
                        2.0 * PI * fz as f32 / width,
                        2.0 * PI * fw as f32 / width,
                    ]);
                    if cols.len() == half_dim {
                        break 'outer;
                    }
                }
            }
        }
    }

    // Transpose to [4, half_dim] row-major.
    let mut data = vec![0f32; 4 * half_dim];
    for (c, v) in cols.iter().enumerate() {
        data[0 * half_dim + c] = v[0];
        data[1 * half_dim + c] = v[1];
        data[2 * half_dim + c] = v[2];
        data[3 * half_dim + c] = v[3];
    }
    Ok(ParamBuf { data, shape: vec![4, half_dim] })
}

pub fn build_params(
    raw: &mut HashMap<String, ParamBuf>,
    cfg: &ModelConfig,
) -> anyhow::Result<ParamMap> {
    let mut p = ParamMap::new();
    let d = cfg.embed_dim;
    let half_dim = d / 2;

    // Patch embedding
    p.insert("to_patch_embedding.0.weight".into(), take_linear_w(raw, "to_patch_embedding.0.weight")?);
    p.insert("to_patch_embedding.0.bias".into(), take_vec(raw, "to_patch_embedding.0.bias", d)?);

    // MLP4D + LNs
    p.insert("mlp4d.0.weight".into(), take_linear_w(raw, "mlp4d.0.weight")?);
    p.insert("mlp4d.2.weight".into(), take_vec(raw, "mlp4d.2.weight", d)?);
    p.insert("mlp4d.2.bias".into(), take_vec(raw, "mlp4d.2.bias", d)?);

    p.insert("ln.weight".into(), take_vec(raw, "ln.weight", d)?);
    p.insert("ln.bias".into(), take_vec(raw, "ln.bias", d)?);

    // Transformer
    for i in 0..cfg.depth {
        p.insert(
            format!("transformer.layers.{i}.0.norm.weight"),
            take_vec(raw, &format!("transformer.layers.{i}.0.norm.weight"), d)?,
        );
        let qkv = take_linear_w(raw, &format!("transformer.layers.{i}.0.to_qkv.weight"))?;
        let inner = cfg.head_dim * cfg.heads;
        let (wq, wk, wv) = split_qkv(qkv, inner)?;
        p.insert(format!("transformer.layers.{i}.0.to_q.weight"), wq);
        p.insert(format!("transformer.layers.{i}.0.to_k.weight"), wk);
        p.insert(format!("transformer.layers.{i}.0.to_v.weight"), wv);
        p.insert(
            format!("transformer.layers.{i}.0.to_out.weight"),
            take_linear_w(raw, &format!("transformer.layers.{i}.0.to_out.weight"))?,
        );
        p.insert(
            format!("transformer.layers.{i}.1.net.0.weight"),
            take_vec(raw, &format!("transformer.layers.{i}.1.net.0.weight"), d)?,
        );
        let ff1 = take_linear_w(raw, &format!("transformer.layers.{i}.1.net.1.weight"))?;
        if cfg.use_geglu {
            let (wu, wg) = split_geglu(ff1, cfg.mlp_dim())?;
            p.insert(format!("transformer.layers.{i}.1.net.1.w_up.weight"), wu);
            p.insert(format!("transformer.layers.{i}.1.net.1.w_gate.weight"), wg);
        } else {
            p.insert(format!("transformer.layers.{i}.1.net.1.weight"), ff1);
        }
        p.insert(
            format!("transformer.layers.{i}.1.net.3.weight"),
            take_linear_w(raw, &format!("transformer.layers.{i}.1.net.3.weight"))?,
        );
    }

    // Head
    if cfg.attention_pooling {
        // Always load the query token when present (encoder-only checkpoints still have it).
        if raw.contains_key("cls_query_token") {
            p.insert("cls_query_token".into(), take(raw, "cls_query_token")?);
        }
        // Optional classifier head: LayerNorm + Linear.
        if raw.contains_key("final_layer.1.weight") {
            p.insert("final_layer.0.weight".into(), take_vec(raw, "final_layer.0.weight", d)?);
            p.insert("final_layer.0.bias".into(), take_vec(raw, "final_layer.0.bias", d)?);
            p.insert("final_layer.1.weight".into(), take_linear_w(raw, "final_layer.1.weight")?);
            p.insert("final_layer.1.bias".into(), take_vec(raw, "final_layer.1.bias", cfg.n_outputs)?);
        }
    } else {
        // If `n_times`/`n_chans` aren't provided (HF config), infer `final_dim`
        // from the LayerNorm gamma vector length.
        let final_dim = if cfg.n_times == 0 || cfg.n_chans == 0 {
            let p = raw
                .get("final_layer.1.weight")
                .ok_or_else(|| anyhow::anyhow!("missing weight key: final_layer.1.weight"))?;
            anyhow::ensure!(p.shape.len() == 1, "final_layer.1.weight must be 1-D");
            p.shape[0]
        } else {
            let n_patches =
                (cfg.n_times - cfg.patch_size) / (cfg.patch_size - cfg.patch_overlap) + 1;
            cfg.n_chans * n_patches * d
        };
        // Optional head for non-attention pooling checkpoints.
        if raw.contains_key("final_layer.2.weight") {
            p.insert("final_layer.1.weight".into(), take_vec(raw, "final_layer.1.weight", final_dim)?);
            p.insert("final_layer.1.bias".into(), take_vec(raw, "final_layer.1.bias", final_dim)?);
            p.insert("final_layer.2.weight".into(), take_linear_w(raw, "final_layer.2.weight")?);
            p.insert("final_layer.2.bias".into(), take_vec(raw, "final_layer.2.bias", cfg.n_outputs)?);
        }
    }

    // Aux params expected by the graph.
    p.insert(KEY_ZEROS_EMBED.into(), ParamBuf { data: vec![0.0; d], shape: vec![d] });
    p.insert(KEY_ATTN_SCALE.into(), ParamBuf { data: vec![(d as f32).powf(-0.5)], shape: vec![1] });
    p.insert(
        KEY_ATTN_HEAD_SCALE.into(),
        ParamBuf {
            data: vec![(cfg.head_dim as f32).powf(-0.5)],
            shape: vec![1],
        },
    );
    p.insert("__reve.freq_t".into(), build_freq_t(cfg)?);
    p.insert("__reve.margin".into(), ParamBuf { data: vec![0.4], shape: vec![1] });
    p.insert("__reve.increment_time".into(), ParamBuf { data: vec![0.1], shape: vec![1] });

    // Sanity: embed_dim must be even because the graph concatenates cos+sin.
    anyhow::ensure!(half_dim * 2 == d, "embed_dim must be even, got {}", d);
    Ok(p)
}

pub fn apply_params(compiled: &mut rlx::CompiledGraph, params: &ParamMap) {
    for (name, buf) in params {
        compiled.set_param(name, &buf.data);
    }
}

