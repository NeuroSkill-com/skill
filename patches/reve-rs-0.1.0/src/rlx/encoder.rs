//! RLX-backed REVE encoder.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;

use crate::config::ModelConfig;
use super::graph::{build_reve_graph, ReveSpec};
use super::pos_embed;
use super::weights::{apply_params, build_params, load_safetensors, ParamMap};

/// Per-sample output from REVE.
#[derive(Clone, Debug)]
pub struct ReveOutput {
    /// Output values (row-major f32).
    pub output: Vec<f32>,
    /// Shape of the output (no batch dim).
    pub shape: Vec<usize>,
    pub n_channels: usize,
}

/// Collection of outputs.
#[derive(Clone, Debug)]
pub struct EncodingResult {
    pub outputs: Vec<ReveOutput>,
    pub ms_load: f64,
    pub ms_encode: f64,
}

pub struct ReveEncoder {
    pub model_cfg: ModelConfig,
    pub device: rlx::Device,

    params: ParamMap,
    cls_query_token: Option<Vec<f32>>,

    session: rlx::Session,
    cache: HashMap<usize, rlx::CompiledGraph>,
}

impl ReveEncoder {
    pub fn load(
        config_path: &Path,
        weights_path: &Path,
        device: rlx::Device,
    ) -> anyhow::Result<(Self, f64)> {
        let cfg_str = std::fs::read_to_string(config_path)
            .with_context(|| format!("config: {}", config_path.display()))?;
        let hf_val: serde_json::Value = serde_json::from_str(&cfg_str)?;
        let mut model_cfg: ModelConfig = serde_json::from_value(
            hf_val.get("model").cloned().unwrap_or(hf_val.clone()),
        )
        .context("parsing model config")?;

        let t = std::time::Instant::now();
        let mut raw = load_safetensors(
            weights_path.to_str().context("weights path not valid UTF-8")?,
        )?;

        if !model_cfg.attention_pooling && raw.contains_key("cls_query_token") {
            model_cfg.attention_pooling = true;
        }
        if model_cfg.n_outputs == 0 {
            let bias_key = if model_cfg.attention_pooling {
                "final_layer.1.bias"
            } else {
                "final_layer.2.bias"
            };
            if let Some(p) = raw.get(bias_key) {
                anyhow::ensure!(p.shape.len() == 1, "{bias_key} must be 1-D");
                model_cfg.n_outputs = p.shape[0];
            } else {
                model_cfg.n_outputs = 0;
            }
        }

        let mut params = build_params(&mut raw, &model_cfg)?;

        let cls_query_token = if model_cfg.attention_pooling {
            let p = params
                .remove("cls_query_token")
                .ok_or_else(|| anyhow::anyhow!("missing weight key: cls_query_token"))?;
            anyhow::ensure!(
                p.shape == vec![1, 1, model_cfg.embed_dim],
                "cls_query_token shape mismatch: {:?}",
                p.shape
            );
            Some(p.data)
        } else {
            None
        };

        super::prepare_device(device);
        let session = rlx::Session::new(device);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        Ok((
            Self {
                model_cfg,
                device,
                params,
                cls_query_token,
                session,
                cache: HashMap::new(),
            },
            ms,
        ))
    }

    pub fn describe(&self) -> String {
        let c = &self.model_cfg;
        format!(
            "REVE (RLX, dev={:?})  embed_dim={}  depth={}  heads={}  head_dim={}  patch={}  outputs={}",
            self.device, c.embed_dim, c.depth, c.heads, c.head_dim, c.patch_size, c.n_outputs,
        )
    }

    pub fn params(&self) -> &super::weights::ParamMap { &self.params }
    pub fn n_patches(&self) -> usize {
        let c = &self.model_cfg;
        let step = c.patch_size - c.patch_overlap;
        if c.n_times == 0 {
            0
        } else {
            (c.n_times - c.patch_size) / step + 1
        }
    }

    fn spec(&self, b: usize) -> ReveSpec {
        let c = &self.model_cfg;
        let n_patches = self.n_patches();
        ReveSpec {
            b,
            s: c.n_chans * n_patches,
            patch_size: c.patch_size,
            embed_dim: c.embed_dim,
            n_outputs: c.n_outputs,
            depth: c.depth,
            heads: c.heads,
            head_dim: c.head_dim,
            mlp_dim: c.mlp_dim(),
            use_geglu: c.use_geglu,
            freqs: c.freqs,
            attention_pooling: c.attention_pooling,
        }
    }

    fn compiled_for(&mut self, b: usize, s: usize) -> &mut rlx::CompiledGraph {
        let key = b * 0x10_0000 + s;
        if !self.cache.contains_key(&key) {
            let mut spec = self.spec(b);
            spec.s = s;
            let graph = build_reve_graph(&spec);
            let mut compiled = self.session.compile(graph);
            apply_params(&mut compiled, &self.params);
            self.cache.insert(key, compiled);
        }
        self.cache.get_mut(&key).expect("just inserted")
    }

    /// CPU-side z-score normalization per channel, clipped to ±15.
    fn normalize(signal: &mut [f32], n_channels: usize, n_times: usize) {
        for c in 0..n_channels {
            let row = &mut signal[c * n_times..(c + 1) * n_times];
            let mean = row.iter().copied().sum::<f32>() / (n_times as f32);
            let mut var = 0.0f32;
            for &v in row.iter() {
                let d = v - mean;
                var += d * d;
            }
            var /= n_times as f32;
            let std = (var + 1e-8).sqrt();
            let inv = 1.0 / std;
            for v in row.iter_mut() {
                let z = (*v - mean) * inv;
                *v = z.clamp(-15.0, 15.0);
            }
        }
    }

    /// CPU-side pre-processing: per-channel z-score on `signal`, then
    /// patch-extract `[n_channels, n_patches, patch_size]` (flat
    /// `[n_chans*n_patches, patch_size]`) and emit the 4-D position
    /// vectors `[n_chans*n_patches, 4]` (the 4th axis is patch index).
    /// Both outputs are inputs to the RLX graph.
    pub fn prep_inputs(
        &self,
        mut signal: Vec<f32>,
        positions_xyz: &[f32],
        n_channels: usize,
        n_times: usize,
    ) -> anyhow::Result<(Vec<f32>, Vec<f32>)> {
        let c = &self.model_cfg;
        if c.n_chans != 0 {
            anyhow::ensure!(
                n_channels == c.n_chans,
                "n_channels mismatch: got {n_channels}, cfg {}",
                c.n_chans
            );
        }
        if c.n_times != 0 {
            anyhow::ensure!(
                n_times == c.n_times,
                "n_times mismatch: got {n_times}, cfg {}",
                c.n_times
            );
        }
        anyhow::ensure!(positions_xyz.len() == n_channels * 3, "positions_xyz len mismatch");
        anyhow::ensure!(signal.len() == n_channels * n_times, "signal len mismatch");

        Self::normalize(&mut signal, n_channels, n_times);

        let step = c.patch_size - c.patch_overlap;
        anyhow::ensure!(
            n_times >= c.patch_size,
            "n_times ({n_times}) < patch_size ({})",
            c.patch_size
        );
        let n_patches = (n_times - c.patch_size) / step + 1;
        let s = n_channels * n_patches;

        let mut patches = vec![0f32; s * c.patch_size];
        let mut pos4 = vec![0f32; s * 4];

        for ch in 0..n_channels {
            let x = positions_xyz[ch * 3 + 0];
            let y = positions_xyz[ch * 3 + 1];
            let z = positions_xyz[ch * 3 + 2];
            let row = &signal[ch * n_times..(ch + 1) * n_times];
            for p in 0..n_patches {
                let start = p * step;
                let dst_tok = ch * n_patches + p;
                let dst_patch = dst_tok * c.patch_size;
                patches[dst_patch..dst_patch + c.patch_size]
                    .copy_from_slice(&row[start..start + c.patch_size]);

                let dst_pos = dst_tok * 4;
                pos4[dst_pos + 0] = x;
                pos4[dst_pos + 1] = y;
                pos4[dst_pos + 2] = z;
                pos4[dst_pos + 3] = p as f32;
            }
        }

        Ok((patches, pos4))
    }

    /// Run REVE up to (but not including) `layer_end` of the transformer,
    /// returning the raw hidden state `[s * embed_dim]` flattened
    /// (`s = n_channels * n_patches`). Skips the final classification /
    /// attention-pool head. Caller is responsible for any downstream
    /// pooling. `layer_end` is clamped to `depth`.
    ///
    /// Used for "intermediate-layer feature extraction" experiments where
    /// later layers may have specialised away from generic EEG structure.
    pub fn run_at_layer(
        &mut self,
        signal: Vec<f32>,
        positions_xyz: Vec<f32>,
        n_channels: usize,
        n_times: usize,
        layer_end: usize,
    ) -> anyhow::Result<ReveOutput> {
        let (patches, pos4) = self.prep_inputs(signal, &positions_xyz, n_channels, n_times)?;
        let s = pos4.len() / 4;
        let d = self.model_cfg.embed_dim;
        let pos_embed = pos_embed::precompute_pos_embed(&pos4, s, d, &self.params);
        let depth = self.model_cfg.depth;
        let layer_end = layer_end.min(depth);
        // Cache key: high bits encode layer_end, low bits encode `s`, batch=1.
        let key = 0x8000_0000usize.wrapping_add(layer_end * 0x10_0000 + s);
        if !self.cache.contains_key(&key) {
            let mut spec = self.spec(1);
            spec.s = s;
            let graph = super::graph::build_reve_graph_range(&spec, 0, layer_end, false);
            let mut compiled = self.session.compile(graph);
            super::weights::apply_params(&mut compiled, &self.params);
            self.cache.insert(key, compiled);
        }
        let compiled = self.cache.get_mut(&key).expect("just inserted");
        let outs = compiled.run(&[("patches", &patches), ("pos_embed", &pos_embed)]);
        let output = outs
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("reve graph produced no output"))?;
        Ok(ReveOutput {
            output,
            shape: vec![s, d],
            n_channels,
        })
    }

    pub fn run_one(
        &mut self,
        signal: Vec<f32>,
        positions_xyz: Vec<f32>,
        n_channels: usize,
        n_times: usize,
    ) -> anyhow::Result<ReveOutput> {
        let (patches, pos4) = self.prep_inputs(signal, &positions_xyz, n_channels, n_times)?;
        let s = pos4.len() / 4;
        let d = self.model_cfg.embed_dim;
        let pos_embed = pos_embed::precompute_pos_embed(&pos4, s, d, &self.params);
        let attention_pooling = self.model_cfg.attention_pooling;
        let cls_q = self.cls_query_token.clone();
        let compiled = self.compiled_for(1, s);

        let outs = if attention_pooling {
            let q = cls_q.as_ref().expect("cls token loaded");
            compiled.run(&[
                ("patches", &patches),
                ("pos_embed", &pos_embed),
                ("cls_q", q),
            ])
        } else {
            compiled.run(&[("patches", &patches), ("pos_embed", &pos_embed)])
        };

        let output = outs
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("reve graph produced no output"))?;

        Ok(ReveOutput {
            output,
            shape: if self.model_cfg.n_outputs == 0 {
                vec![self.model_cfg.embed_dim]
            } else {
                vec![self.model_cfg.n_outputs]
            },
            n_channels,
        })
    }
}
