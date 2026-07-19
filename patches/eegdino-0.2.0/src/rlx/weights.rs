//! RLX parameter loading for EEG-DINO.
//!
//! Loads safetensors produced by `scripts/convert_weights.py` and pushes all
//! parameters into an `rlx::CompiledGraph` by name.

use std::collections::HashMap;

use anyhow::Context;
use safetensors::SafeTensors;

use super::graph::{
    EncoderSpec, KEY_CHANNEL_EMB, KEY_DFT_COS_T, KEY_DFT_SIN_T, KEY_INV_PATCH, KEY_PAD_ZEROS,
};
use crate::config::{ModelConfig, ModelSize};

#[derive(Clone, Debug)]
pub struct TensorBlob {
    pub data: Vec<f32>,
    pub shape: Vec<usize>,
}

pub type ParamMap = HashMap<String, TensorBlob>;

pub fn load_safetensors(path: &str) -> anyhow::Result<ParamMap> {
    let bytes = std::fs::read(path).with_context(|| format!("reading weights: {path}"))?;
    let st = SafeTensors::deserialize(&bytes)
        .with_context(|| format!("deserializing safetensors: {path}"))?;

    let mut out: ParamMap = HashMap::with_capacity(st.len());
    for (key, view) in st.tensors() {
        let shape: Vec<usize> = view.shape().to_vec();
        let raw = view.data();
        let data: Vec<f32> = raw
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        out.insert(key.to_string(), TensorBlob { data, shape });
    }
    Ok(out)
}

/// One-time transforms after load: fuse QKV biases, precompute channel/DFT constants.
/// Pre-expanded `global_tokens` keyed as `__eegdino.global_tokens_b{N}`.
pub fn global_tokens_key(batch: usize) -> String {
    format!("__eegdino.global_tokens_b{batch}")
}

pub fn prepare_params(cfg: &ModelConfig, mut raw: ParamMap) -> anyhow::Result<ParamMap> {
    fuse_qkv_biases(cfg, &mut raw)?;
    precompute_channel_emb(cfg, &mut raw)?;
    precompute_dft_constants(cfg, &mut raw)?;
    preexpand_global_tokens(cfg, &mut raw)?;
    Ok(raw)
}

pub fn detect_model_size(path: &str) -> anyhow::Result<ModelSize> {
    let map = load_safetensors(path)?;
    let t = map
        .get("global_tokens")
        .context("missing global_tokens key")?;
    match t.shape.last().copied() {
        Some(200) => Ok(ModelSize::Small),
        Some(512) => Ok(ModelSize::Medium),
        Some(1024) => Ok(ModelSize::Large),
        other => anyhow::bail!("unexpected feature_size in global_tokens: {other:?}"),
    }
}

fn get<'a>(map: &'a ParamMap, key: &str) -> anyhow::Result<&'a TensorBlob> {
    map.get(key)
        .with_context(|| format!("missing weight: {key}"))
}

fn take(map: &mut ParamMap, key: &str) -> anyhow::Result<TensorBlob> {
    map.remove(key)
        .with_context(|| format!("missing weight: {key}"))
}

fn zeros(len: usize) -> Vec<f32> {
    vec![0.0f32; len]
}

fn build_dft_cos_sin_t(patch: usize) -> (Vec<f32>, Vec<f32>) {
    let n = patch;
    let k = n / 2 + 1;
    let two_pi_over_n = 2.0 * std::f64::consts::PI / n as f64;

    let mut cos_t = vec![0.0f32; n * k];
    let mut sin_t = vec![0.0f32; n * k];
    for ki in 0..k {
        for ni in 0..n {
            let angle = two_pi_over_n * (ki as f64) * (ni as f64);
            cos_t[ni * k + ki] = angle.cos() as f32;
            sin_t[ni * k + ki] = angle.sin() as f32;
        }
    }
    (cos_t, sin_t)
}

fn fuse_qkv_biases(cfg: &ModelConfig, raw: &mut ParamMap) -> anyhow::Result<()> {
    let hd = cfg.feature_size;
    for i in 0..cfg.num_layers {
        let p = format!("encoder_layers.{i}");
        let qb = take(raw, &format!("{p}.attn.q_bias"))?;
        let vb = take(raw, &format!("{p}.attn.v_bias"))?;
        if qb.shape != vec![hd] || vb.shape != vec![hd] {
            anyhow::bail!(
                "q_bias/v_bias shape mismatch at layer {i}: q={:?} v={:?} expected [{hd}]",
                qb.shape,
                vb.shape
            );
        }
        let mut fused = Vec::with_capacity(3 * hd);
        fused.extend_from_slice(&qb.data);
        fused.extend_from_slice(&zeros(hd));
        fused.extend_from_slice(&vb.data);
        raw.insert(
            format!("{p}.attn.qkv.bias"),
            TensorBlob {
                data: fused,
                shape: vec![3 * hd],
            },
        );
    }
    Ok(())
}

/// `channel_embedding(forward(one_hot))` with identity one-hot equals `W + broadcast(b)`.
fn precompute_channel_emb(cfg: &ModelConfig, raw: &mut ParamMap) -> anyhow::Result<()> {
    let w = get(raw, "patch_embedding.channel_embedding.weight")?;
    let b = get(raw, "patch_embedding.channel_embedding.bias")?;
    let c = cfg.num_channels;
    let d = cfg.feature_size;
    if w.shape != vec![c, d] || b.shape != vec![d] {
        anyhow::bail!(
            "channel_embedding shape mismatch: w={:?} b={:?} expected [{c},{d}] / [{d}]",
            w.shape,
            b.shape
        );
    }
    let mut emb = vec![0.0f32; c * d];
    for i in 0..c {
        for j in 0..d {
            emb[i * d + j] = w.data[i * d + j] + b.data[j];
        }
    }
    raw.remove("patch_embedding.channel_embedding.weight");
    raw.remove("patch_embedding.channel_embedding.bias");
    raw.insert(
        KEY_CHANNEL_EMB.to_string(),
        TensorBlob {
            data: emb,
            shape: vec![1, c, 1, d],
        },
    );
    Ok(())
}

fn preexpand_global_tokens(cfg: &ModelConfig, raw: &mut ParamMap) -> anyhow::Result<()> {
    let gt = get(raw, "global_tokens")?.clone();
    let gtok = cfg.num_global_tokens;
    let d = cfg.feature_size;
    if gt.shape != vec![1, gtok, d] {
        anyhow::bail!(
            "global_tokens shape mismatch: got {:?}, expected [1,{gtok},{d}]",
            gt.shape
        );
    }
    for b in [1usize, 2, 4, 8, 16, 32, 64] {
        let mut exp = Vec::with_capacity(b * gtok * d);
        for _ in 0..b {
            exp.extend_from_slice(&gt.data);
        }
        raw.insert(
            global_tokens_key(b),
            TensorBlob {
                data: exp,
                shape: vec![b, gtok, d],
            },
        );
    }
    Ok(())
}

fn precompute_dft_constants(cfg: &ModelConfig, raw: &mut ParamMap) -> anyhow::Result<()> {
    let patch = cfg.patch_size;
    let k = patch / 2 + 1;
    let (cos_t, sin_t) = build_dft_cos_sin_t(patch);
    raw.insert(
        KEY_DFT_COS_T.to_string(),
        TensorBlob {
            data: cos_t,
            shape: vec![patch, k],
        },
    );
    raw.insert(
        KEY_DFT_SIN_T.to_string(),
        TensorBlob {
            data: sin_t,
            shape: vec![patch, k],
        },
    );
    raw.insert(
        KEY_INV_PATCH.to_string(),
        TensorBlob {
            data: vec![1.0f32 / patch as f32],
            shape: vec![1],
        },
    );
    Ok(())
}

/// Push prepared weights into a compiled graph (cheap to call per batch shape).
pub fn apply_params(
    compiled: &mut rlx::CompiledGraph,
    cfg: &ModelConfig,
    spec: &EncoderSpec,
    raw: &ParamMap,
) -> anyhow::Result<()> {
    let h_tokens = spec.c * spec.p;
    let pad_w = 24usize;
    compiled.set_param(KEY_PAD_ZEROS, &zeros(spec.b * 1 * h_tokens * pad_w));

    compiled.set_param(KEY_INV_PATCH, &get(raw, KEY_INV_PATCH)?.data);
    compiled.set_param(KEY_DFT_COS_T, &get(raw, KEY_DFT_COS_T)?.data);
    compiled.set_param(KEY_DFT_SIN_T, &get(raw, KEY_DFT_SIN_T)?.data);
    compiled.set_param(KEY_CHANNEL_EMB, &get(raw, KEY_CHANNEL_EMB)?.data);

    let gt_key = global_tokens_key(spec.b);
    if let Ok(gt) = get(raw, &gt_key) {
        if gt.shape != vec![spec.b, cfg.num_global_tokens, cfg.feature_size] {
            anyhow::bail!("{} shape mismatch: {:?}", gt_key, gt.shape);
        }
        compiled.set_param("global_tokens", &gt.data);
    } else {
        let base = get(raw, "global_tokens")?;
        let gtok = cfg.num_global_tokens;
        let d = cfg.feature_size;
        if base.shape != vec![1, gtok, d] {
            anyhow::bail!(
                "global_tokens shape mismatch: got {:?}, expected [1,{gtok},{d}]",
                base.shape
            );
        }
        let mut gt_exp = Vec::with_capacity(spec.b * gtok * d);
        for _ in 0..spec.b {
            gt_exp.extend_from_slice(&base.data);
        }
        compiled.set_param("global_tokens", &gt_exp);
    }

    const PATCH_KEYS: &[&str] = &[
        "patch_embedding.proj_in.conv1.weight",
        "patch_embedding.proj_in.conv1.bias",
        "patch_embedding.proj_in.norm1.weight",
        "patch_embedding.proj_in.norm1.bias",
        "patch_embedding.proj_in.conv2.weight",
        "patch_embedding.proj_in.conv2.bias",
        "patch_embedding.proj_in.norm2.weight",
        "patch_embedding.proj_in.norm2.bias",
        "patch_embedding.proj_in.conv3.weight",
        "patch_embedding.proj_in.conv3.bias",
        "patch_embedding.proj_in.norm3.weight",
        "patch_embedding.proj_in.norm3.bias",
        "patch_embedding.spectral_proj.weight",
        "patch_embedding.spectral_proj.bias",
        "patch_embedding.time_encoding.weight",
        "patch_embedding.time_encoding.bias",
    ];
    for key in PATCH_KEYS {
        compiled.set_param(key, &get(raw, key)?.data);
    }

    let hd = cfg.feature_size;
    for i in 0..cfg.num_layers {
        let p = format!("encoder_layers.{i}");
        for key in [
            format!("{p}.norm1.weight"),
            format!("{p}.norm1.bias"),
            format!("{p}.attn.qkv.weight"),
            format!("{p}.attn.qkv.bias"),
            format!("{p}.attn.proj.weight"),
            format!("{p}.attn.proj.bias"),
            format!("{p}.norm2.weight"),
            format!("{p}.norm2.bias"),
            format!("{p}.mlp.fc1.weight"),
            format!("{p}.mlp.fc1.bias"),
            format!("{p}.mlp.fc2.weight"),
            format!("{p}.mlp.fc2.bias"),
        ] {
            compiled.set_param(&key, &get(raw, &key)?.data);
        }
        let _ = hd; // qkv.bias already fused to length 3*hd in prepare_params
    }

    Ok(())
}
