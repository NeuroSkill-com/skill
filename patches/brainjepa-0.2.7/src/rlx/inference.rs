//! RLX-backed Brain-JEPA encoder inference.

use std::path::Path;
use std::time::Instant;

use anyhow::Context;

use crate::config::{DataConfig, ModelConfig};
use crate::data::{self, GradientData};
use crate::error::BrainJepaError;

use super::attn_layout::resolve_attn_layout;
use super::device::ensure_device;
use super::graph::{build_encoder_graph, EncoderSpec};
use super::pos_embed_cpu::build_pos_embed;
use super::weights::{apply_params, build_encoder_params, load_safetensors, ParamMap};

/// Encoder embedding output.
pub struct EmbeddingResult {
    /// Latent embeddings: row-major f32, shape [n_patches, embed_dim]
    pub embeddings: Vec<f32>,
    /// Shape: [n_patches, embed_dim]
    pub shape: Vec<usize>,
    /// Number of ROI patches
    pub n_rois: usize,
    /// Number of temporal patches
    pub n_time_patches: usize,
    /// Encoding time in milliseconds
    pub ms_encode: f64,
}

impl EmbeddingResult {
    pub fn n_patches(&self) -> usize {
        self.n_rois * self.n_time_patches
    }
    pub fn embed_dim(&self) -> usize {
        self.shape.get(1).copied().unwrap_or(0)
    }

    pub fn save_safetensors(&self, path: &str) -> anyhow::Result<()> {
        use safetensors::{Dtype, View};
        use std::borrow::Cow;

        struct RawTensor {
            data: Vec<u8>,
            shape: Vec<usize>,
        }
        impl View for RawTensor {
            fn dtype(&self) -> Dtype {
                Dtype::F32
            }
            fn shape(&self) -> &[usize] {
                &self.shape
            }
            fn data(&self) -> Cow<'_, [u8]> {
                Cow::Borrowed(&self.data)
            }
            fn data_len(&self) -> usize {
                self.data.len()
            }
        }

        let bytes: Vec<u8> = self
            .embeddings
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let tensor = RawTensor {
            data: bytes,
            shape: self.shape.clone(),
        };
        let pairs: Vec<(&str, RawTensor)> = vec![("embeddings", tensor)];
        let out = safetensors::serialize(pairs, None)?;
        std::fs::write(path, out)?;
        Ok(())
    }
}

/// One forward pass for warmup (wgpu has no `run_slots` yet).
fn warmup_run(compiled: &mut rlx::CompiledGraph, x: &[f32]) {
    if compiled.run_slots(&[x]).is_empty() {
        let _ = compiled.run(&[("x", x)]);
    }
}

/// Copy the sole graph output from the arena after `run_slots`.
fn read_output_f32(
    compiled: &rlx::CompiledGraph,
    off: usize,
    len: usize,
) -> anyhow::Result<Vec<f32>> {
    let base = compiled.arena_ptr();
    anyhow::ensure!(len > 0, "encoder output is empty");
    let out = unsafe { std::slice::from_raw_parts(base.add(off) as *const f32, len) };
    Ok(out.to_vec())
}

pub struct BrainJepaEncoder {
    pub model_cfg: ModelConfig,
    pub data_cfg: DataConfig,
    pub device: rlx::Device,

    #[allow(dead_code)]
    params: ParamMap,
    compiled: rlx::CompiledGraph,

    n_rois: usize,
    #[allow(dead_code)]
    n_time: usize,
    n_time_patches: usize,
}

impl BrainJepaEncoder {
    pub fn from_weights(
        weights_path: &str,
        gradient_csv_path: &str,
        model_cfg: &ModelConfig,
        data_cfg: &DataConfig,
        device: &rlx::Device,
    ) -> anyhow::Result<(Self, f64)> {
        ensure_device(*device)?;

        if !Path::new(weights_path).exists() {
            return Err(BrainJepaError::FileNotFound {
                kind: "weights",
                path: weights_path.into(),
            }
            .into());
        }

        let grad = GradientData::from_csv(gradient_csv_path)?;
        let expected_rois = data_cfg.crop_size.0;
        if grad.n_rois != expected_rois {
            return Err(BrainJepaError::GradientRoiMismatch {
                expected: expected_rois,
                got: grad.n_rois,
            }
            .into());
        }

        let t = Instant::now();
        let mut raw = load_safetensors(weights_path)?;
        let (params, grad_proj) = build_encoder_params(&mut raw, model_cfg)?;
        let ms_weights = t.elapsed().as_secs_f64() * 1000.0;

        let n_rois = data_cfg.crop_size.0;
        let n_time = data_cfg.crop_size.1;
        let patch = model_cfg.patch_size;
        let n_time_patches = n_time / patch;
        let n = n_rois * n_time_patches;

        // CPU build positional embedding once.
        let (grad_w, grad_b, grad_dim) = grad_proj
            .map(|(w, b, gd)| (Some(w), Some(b), gd))
            .unwrap_or((None, None, grad.grad_dim));

        let pos = build_pos_embed(
            &model_cfg.pos_mode,
            n_rois,
            n_time_patches,
            model_cfg.embed_dim,
            &grad.values,
            grad_dim,
            grad_w.as_deref(),
            grad_b.as_deref(),
        )?;

        let spec = EncoderSpec {
            b: 1,
            h: n_rois,
            w: n_time,
            patch,
            w_p: n_time_patches,
            n,
            dim: model_cfg.embed_dim,
            depth: model_cfg.depth,
            num_heads: model_cfg.num_heads,
            head_dim: model_cfg.embed_dim / model_cfg.num_heads,
            hidden_dim: (model_cfg.embed_dim as f64 * model_cfg.mlp_ratio) as usize,
            norm_eps: model_cfg.norm_eps as f32,
        };

        let attn_layout = resolve_attn_layout(*device)?;
        let graph = build_encoder_graph(&spec, attn_layout);
        let session = rlx::Session::new(*device);
        let mut compiled = session.compile(graph);
        apply_params(&mut compiled, &params);
        compiled.set_param("pos_embed", &pos);

        // Warm up GPU backends (MPSGraph first-run specialization, kernel cache).
        if !matches!(*device, rlx::Device::Cpu) {
            let x_warm = vec![0.0f32; 1 * 1 * n_rois * n_time];
            warmup_run(&mut compiled, &x_warm);
        }

        Ok((
            Self {
                model_cfg: model_cfg.clone(),
                data_cfg: data_cfg.clone(),
                device: *device,
                params,
                compiled,
                n_rois,
                n_time,
                n_time_patches,
            },
            ms_weights,
        ))
    }

    pub fn describe(&self) -> String {
        format!(
            "Brain-JEPA encoder (RLX, {})  embed_dim={}  depth={}  heads={}  patch={}",
            super::device::display_name(self.device),
            self.model_cfg.embed_dim,
            self.model_cfg.depth,
            self.model_cfg.num_heads,
            self.model_cfg.patch_size
        )
    }

    pub fn encode_safetensors(&mut self, fmri_path: &str) -> anyhow::Result<EmbeddingResult> {
        let input = data::load_fmri_safetensors_f32(fmri_path)
            .with_context(|| format!("loading fmri safetensors: {fmri_path}"))?;
        self.encode_f32(input.data, input.n_rois, input.n_time)
    }

    pub fn encode_csv(&mut self, csv_path: &str) -> anyhow::Result<EmbeddingResult> {
        let input = data::load_fmri_csv_f32(csv_path)
            .with_context(|| format!("loading fmri csv: {csv_path}"))?;
        self.encode_f32(input.data, input.n_rois, input.n_time)
    }

    fn encode_f32(
        &mut self,
        mut x: Vec<f32>, // [1, 1, H, W] row-major
        n_rois: usize,
        n_time: usize,
    ) -> anyhow::Result<EmbeddingResult> {
        // Optional temporal downsampling (CPU)
        x = data::preprocess_fmri_f32(
            x,
            n_rois,
            n_time,
            self.data_cfg.crop_size.1,
            self.data_cfg.downsample,
        )?;

        let t = Instant::now();
        let slots = self.compiled.run_slots(&[&x]);
        let embeddings = if let Some(&(out_off, out_len)) = slots.first() {
            read_output_f32(&self.compiled, out_off, out_len)?
        } else {
            // rlx-wgpu (and some other backends) do not implement run_slots yet.
            self.compiled
                .run(&[("x", &x)])
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("encoder graph produced no output"))?
        };
        let ms_encode = t.elapsed().as_secs_f64() * 1000.0;

        Ok(EmbeddingResult {
            embeddings,
            shape: vec![self.n_rois * self.n_time_patches, self.model_cfg.embed_dim],
            n_rois: self.n_rois,
            n_time_patches: self.n_time_patches,
            ms_encode,
        })
    }
}
