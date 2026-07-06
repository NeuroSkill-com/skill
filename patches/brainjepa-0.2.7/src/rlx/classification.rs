//! Downstream classification head in RLX (mean-pool + LayerNorm + linear).
//!
//! Checkpoint keys:
//!   `{prefix}.head.weight`, `{prefix}.head.bias`,
//!   `{prefix}.fc_norm.weight`, `{prefix}.fc_norm.bias`

use rlx::ir::infer::GraphExt;
use rlx::prelude::*;

use crate::config::ModelConfig;

use super::device::{ensure_device, prepare_device};
use super::weights::{apply_params, load_safetensors, take_linear_w, ParamBuf, ParamMap};

fn s1(d: usize) -> Shape {
    Shape::new(&[d], DType::F32)
}
fn s2(a: usize, b: usize) -> Shape {
    Shape::new(&[a, b], DType::F32)
}

/// Build classification graph: `[1, N, D]` → `[1, num_classes]`.
pub fn build_classification_graph(
    n_patches: usize,
    embed_dim: usize,
    num_classes: usize,
    norm_eps: f32,
) -> Graph {
    let mut g = Graph::new("brainjepa_cls");
    let emb = g.input(
        "embeddings",
        Shape::new(&[1, n_patches, embed_dim], DType::F32),
    );
    let pooled = g.mean(emb, vec![1], false);
    let ln_w = g.param("fc_norm.weight", s1(embed_dim));
    let ln_b = g.param("fc_norm.bias", s1(embed_dim));
    let xn = g.ln(pooled, ln_w, ln_b, norm_eps);
    let head_w = g.param("head.weight", s2(embed_dim, num_classes));
    let head_b = g.param("head.bias", s1(num_classes));
    let mm = g.mm(xn, head_w);
    let logits = g.add(mm, head_b);
    g.set_outputs(vec![logits]);
    g
}

fn load_head_params(
    raw: &mut std::collections::HashMap<String, ParamBuf>,
    prefix: &str,
    embed_dim: usize,
    num_classes: usize,
) -> anyhow::Result<ParamMap> {
    let pfx = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix}.")
    };
    let mut p = ParamMap::new();
    p.insert(
        "fc_norm.weight".into(),
        raw.remove(&format!("{pfx}fc_norm.weight"))
            .ok_or_else(|| anyhow::anyhow!("missing {pfx}fc_norm.weight"))?,
    );
    p.insert(
        "fc_norm.bias".into(),
        raw.remove(&format!("{pfx}fc_norm.bias"))
            .ok_or_else(|| anyhow::anyhow!("missing {pfx}fc_norm.bias"))?,
    );
    p.insert(
        "head.weight".into(),
        take_linear_w(raw, &format!("{pfx}head.weight"))?,
    );
    p.insert(
        "head.bias".into(),
        raw.remove(&format!("{pfx}head.bias"))
            .ok_or_else(|| anyhow::anyhow!("missing {pfx}head.bias"))?,
    );
    let _ = (embed_dim, num_classes);
    Ok(p)
}

/// Linear classification head on top of encoder embeddings.
pub struct ClassificationHead {
    pub num_classes: usize,
    pub embed_dim: usize,
    n_patches: usize,
    compiled: rlx::CompiledGraph,
}

impl ClassificationHead {
    pub fn new(
        n_patches: usize,
        embed_dim: usize,
        num_classes: usize,
        norm_eps: f32,
        device: &rlx::Device,
    ) -> anyhow::Result<Self> {
        ensure_device(*device)?;
        prepare_device(*device);
        let graph = build_classification_graph(n_patches, embed_dim, num_classes, norm_eps);
        let compiled = rlx::Session::new(*device).compile(graph);
        Ok(Self {
            num_classes,
            embed_dim,
            n_patches,
            compiled,
        })
    }

    /// Load head weights from a safetensors file (downstream checkpoint).
    pub fn load_weights(&mut self, weights_path: &str, prefix: &str) -> anyhow::Result<()> {
        let mut raw = load_safetensors(weights_path)?;
        let params = load_head_params(&mut raw, prefix, self.embed_dim, self.num_classes)?;
        apply_params(&mut self.compiled, &params);
        Ok(())
    }

    /// Run head on encoder output `[n_patches, embed_dim]` row-major.
    pub fn forward(&mut self, embeddings: &[f32]) -> anyhow::Result<Vec<f32>> {
        anyhow::ensure!(
            embeddings.len() == self.n_patches * self.embed_dim,
            "embeddings length {} != {}*{}",
            embeddings.len(),
            self.n_patches,
            self.embed_dim
        );
        let out = self
            .compiled
            .run(&[("embeddings", embeddings)])
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("classification graph produced no output"))?;
        Ok(out)
    }
}

/// Argmax over logits.
pub fn predict_class(logits: &[f32]) -> usize {
    logits
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Build a classification head using defaults from `ModelConfig` (untrained head params are zeros).
pub fn untrained_head(
    n_patches: usize,
    num_classes: usize,
    model_cfg: &ModelConfig,
    device: &rlx::Device,
) -> anyhow::Result<ClassificationHead> {
    ClassificationHead::new(
        n_patches,
        model_cfg.embed_dim,
        num_classes,
        model_cfg.norm_eps as f32,
        device,
    )
}
