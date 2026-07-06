//! RLX-backed Brain-JEPA JEPA predictor (encoder context + mask tokens).

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use crate::config::{DataConfig, ModelConfig};
use crate::data::GradientData;
use crate::error::BrainJepaError;
use crate::masks;

use super::attn_layout::resolve_attn_layout;
use super::device::ensure_device;
use super::graph::{
    build_encoder_embed_graph, build_encoder_trunk_graph, build_predictor_embed_graph,
    build_predictor_graph, EncoderSpec, PredictorSpec,
};
use super::mask_ops::{apply_masks_f32, gather_positions_f32};
use super::pos_embed_cpu::build_pos_embed;
use super::weights::{
    apply_params, build_encoder_params, build_predictor_params, build_predictor_pos_embed,
    load_safetensors, ParamMap,
};

/// Encoder + predictor for masked JEPA evaluation (RLX).
pub struct BrainJepaPredictor {
    pub model_cfg: ModelConfig,
    pub data_cfg: DataConfig,
    pub device: rlx::Device,
    n_rois: usize,
    n_time_patches: usize,
    n_patches: usize,
    embed_dim: usize,
    n_ctx: usize,
    n_pred: usize,
    embed_compiled: rlx::CompiledGraph,
    trunk_cache: HashMap<usize, rlx::CompiledGraph>,
    predictor_compiled: rlx::CompiledGraph,
    predictor_embed_compiled: rlx::CompiledGraph,
    encoder_params: ParamMap,
    predictor_params: ParamMap,
    pred_pos_embed: Vec<f32>,
    /// Context / target patch indices used when compiling graphs (from [`MaskConfig::seed`]).
    pub enc_indices: Vec<i64>,
    pub pred_indices: Vec<i64>,
}

impl BrainJepaPredictor {
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
        if grad.n_rois != data_cfg.crop_size.0 {
            return Err(BrainJepaError::GradientRoiMismatch {
                expected: data_cfg.crop_size.0,
                got: grad.n_rois,
            }
            .into());
        }

        let patch = model_cfg.patch_size;
        let n_time = data_cfg.crop_size.1;
        let n_time_patches = n_time / patch;
        let n_rois = grad.n_rois;
        let n_patches = n_rois * n_time_patches;

        let mask_cfg = crate::masks::mask_config_for(n_rois, n_time_patches);
        let (enc_mask, pred_masks) = masks::jepa_masks(&mask_cfg);
        let n_ctx = enc_mask.len();
        let pred_indices = pred_masks
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("jepa_masks produced no predictor masks"))?;
        let n_pred = pred_indices.len();
        anyhow::ensure!(n_ctx > 0 && n_pred > 0, "invalid JEPA mask sizes");

        let spec = EncoderSpec {
            b: 1,
            h: n_rois,
            w: n_time,
            patch,
            w_p: n_time_patches,
            n: n_patches,
            dim: model_cfg.embed_dim,
            depth: model_cfg.depth,
            num_heads: model_cfg.num_heads,
            head_dim: model_cfg.embed_dim / model_cfg.num_heads,
            hidden_dim: (model_cfg.embed_dim as f64 * model_cfg.mlp_ratio) as usize,
            norm_eps: model_cfg.norm_eps as f32,
        };

        let attn = resolve_attn_layout(*device)?;
        let t = Instant::now();
        let mut raw = load_safetensors(weights_path)?;
        let (encoder_params, grad_proj) = build_encoder_params(&mut raw, model_cfg)?;
        let (predictor_params, pred_grad_proj) = build_predictor_params(&mut raw, model_cfg)?;

        let grad_w_store;
        let grad_b_store;
        let (grad_w, grad_b) = match &grad_proj {
            Some((w, b, _)) => {
                grad_w_store = w.clone();
                grad_b_store = b.clone();
                (Some(grad_w_store.as_slice()), Some(grad_b_store.as_slice()))
            }
            None => (None, None),
        };
        let pos_embed = build_pos_embed(
            &model_cfg.pos_mode,
            n_rois,
            n_time_patches,
            model_cfg.embed_dim,
            &grad.values,
            grad.grad_dim,
            grad_w,
            grad_b,
        )?;

        let pred_pos_embed = match pred_grad_proj {
            Some((w, b, _)) => build_predictor_pos_embed(
                &model_cfg.pos_mode,
                n_rois,
                n_time_patches,
                model_cfg.pred_emb_dim,
                &grad.values,
                grad.grad_dim,
                Some((&w, &b)),
            )?,
            None => build_predictor_pos_embed(
                &model_cfg.pos_mode,
                n_rois,
                n_time_patches,
                model_cfg.pred_emb_dim,
                &grad.values,
                grad.grad_dim,
                None,
            )?,
        };

        let session = rlx::Session::new(*device);
        let mut embed_compiled = session.compile(build_encoder_embed_graph(&spec));
        apply_params(&mut embed_compiled, &encoder_params);
        embed_compiled.set_param("pos_embed", &pos_embed);

        let pred_spec = PredictorSpec {
            b: 1,
            n_patches,
            n_ctx,
            n_pred,
            enc_dim: model_cfg.embed_dim,
            pred_dim: model_cfg.pred_emb_dim,
            depth: model_cfg.pred_depth,
            num_heads: model_cfg.num_heads,
            head_dim: model_cfg.pred_emb_dim / model_cfg.num_heads,
            hidden_dim: (model_cfg.pred_emb_dim as f64 * model_cfg.mlp_ratio) as usize,
            norm_eps: model_cfg.norm_eps as f32,
        };
        let mut predictor_embed_compiled = session.compile(build_predictor_embed_graph(
            1,
            n_ctx,
            model_cfg.embed_dim,
            model_cfg.pred_emb_dim,
        ));
        apply_params(&mut predictor_embed_compiled, &predictor_params);

        let mut predictor_compiled = session.compile(build_predictor_graph(&pred_spec, attn));
        apply_params(&mut predictor_compiled, &predictor_params);

        let ms = t.elapsed().as_secs_f64() * 1000.0;

        Ok((
            Self {
                model_cfg: model_cfg.clone(),
                data_cfg: data_cfg.clone(),
                device: *device,
                n_rois,
                n_time_patches,
                n_patches,
                embed_dim: model_cfg.embed_dim,
                n_ctx,
                n_pred,
                embed_compiled,
                trunk_cache: HashMap::new(),
                predictor_compiled,
                predictor_embed_compiled,
                encoder_params,
                predictor_params,
                pred_pos_embed,
                enc_indices: enc_mask,
                pred_indices,
            },
            ms,
        ))
    }

    /// Fixed mask indices from load (same seed as [`mask_config_for`]).
    pub fn mask_indices(&self) -> (&[i64], &[i64]) {
        (&self.enc_indices, &self.pred_indices)
    }

    fn trunk(
        &mut self,
        attn: super::attn_layout::AttnLayout,
    ) -> anyhow::Result<&mut rlx::CompiledGraph> {
        let n_ctx = self.n_ctx;
        if !self.trunk_cache.contains_key(&n_ctx) {
            let spec = EncoderSpec {
                b: 1,
                h: self.n_rois,
                w: self.data_cfg.crop_size.1,
                patch: self.model_cfg.patch_size,
                w_p: self.n_time_patches,
                n: self.n_patches,
                dim: self.embed_dim,
                depth: self.model_cfg.depth,
                num_heads: self.model_cfg.num_heads,
                head_dim: self.embed_dim / self.model_cfg.num_heads,
                hidden_dim: (self.embed_dim as f64 * self.model_cfg.mlp_ratio) as usize,
                norm_eps: self.model_cfg.norm_eps as f32,
            };
            let mut compiled = rlx::Session::new(self.device)
                .compile(build_encoder_trunk_graph(&spec, attn, n_ctx));
            apply_params(&mut compiled, &self.encoder_params);
            self.trunk_cache.insert(n_ctx, compiled);
        }
        Ok(self.trunk_cache.get_mut(&n_ctx).expect("trunk"))
    }

    /// Full encode (no masking) — delegates to the same path as [`super::BrainJepaEncoder`].
    pub fn encode_f32(
        &mut self,
        mut x: Vec<f32>,
        n_rois: usize,
        n_time: usize,
    ) -> anyhow::Result<Vec<f32>> {
        x = crate::data::preprocess_fmri_f32(
            x,
            n_rois,
            n_time,
            self.data_cfg.crop_size.1,
            self.data_cfg.downsample,
        )?;

        let attn = resolve_attn_layout(self.device)?;
        let h_full = self
            .embed_compiled
            .run(&[("x", &x)])
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("embed graph produced no output"))?;

        let all_idx: Vec<i64> = (0..self.n_patches as i64).collect();
        let h_ctx = apply_masks_f32(&h_full, 1, self.n_patches, self.embed_dim, &[all_idx]);
        let trunk = self.trunk(attn)?;
        trunk
            .run(&[("h0", &h_ctx)])
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("trunk graph produced no output"))
    }

    /// JEPA forward: masked encoder context + predictor targets.
    ///
    /// `enc_indices` / `pred_indices` must match the sizes used at load (from [`MaskConfig`]).
    pub fn predict_f32(
        &mut self,
        mut x: Vec<f32>,
        n_rois: usize,
        n_time: usize,
        enc_indices: &[i64],
        pred_indices: &[i64],
    ) -> anyhow::Result<(Vec<f32>, Vec<f32>)> {
        anyhow::ensure!(
            enc_indices.len() == self.n_ctx,
            "enc_indices len {} != compiled n_ctx {}",
            enc_indices.len(),
            self.n_ctx
        );
        anyhow::ensure!(
            pred_indices.len() == self.n_pred,
            "pred_indices len {} != compiled n_pred {}",
            pred_indices.len(),
            self.n_pred
        );

        x = crate::data::preprocess_fmri_f32(
            x,
            n_rois,
            n_time,
            self.data_cfg.crop_size.1,
            self.data_cfg.downsample,
        )?;

        let attn = resolve_attn_layout(self.device)?;
        let h_full = self
            .embed_compiled
            .run(&[("x", &x)])
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("embed graph produced no output"))?;

        let h_ctx = apply_masks_f32(
            &h_full,
            1,
            self.n_patches,
            self.embed_dim,
            &[enc_indices.to_vec()],
        );
        let enc_out = self
            .trunk(attn)?
            .run(&[("h0", &h_ctx)])
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("encoder trunk produced no output"))?;

        let tokens = self.assemble_predictor_tokens(&enc_out, enc_indices, pred_indices)?;

        let pred_out = self
            .predictor_compiled
            .run(&[("tokens", &tokens)])
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("predictor graph produced no output"))?;

        Ok((enc_out, pred_out))
    }

    fn assemble_predictor_tokens(
        &mut self,
        enc_out: &[f32],
        enc_indices: &[i64],
        pred_indices: &[i64],
    ) -> anyhow::Result<Vec<f32>> {
        let d = self.model_cfg.pred_emb_dim;
        let kc = self.n_ctx;
        let kp = self.n_pred;

        let mut ctx = self
            .predictor_embed_compiled
            .run(&[("ctx_enc", enc_out)])
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("predictor_embed produced no output"))?;

        let ctx_pos = gather_positions_f32(&self.pred_pos_embed, self.n_patches, d, enc_indices);
        for i in 0..kc * d {
            ctx[i] += ctx_pos[i];
        }

        let tgt_pos = gather_positions_f32(&self.pred_pos_embed, self.n_patches, d, pred_indices);
        let mask_t = self
            .predictor_params
            .get("predictor.mask_token")
            .ok_or_else(|| anyhow::anyhow!("missing mask_token"))?;
        anyhow::ensure!(mask_t.data.len() >= d, "mask_token too short");

        let mut pred = vec![0f32; kp * d];
        for i in 0..kp {
            for j in 0..d {
                pred[i * d + j] = mask_t.data[j] + tgt_pos[i * d + j];
            }
        }

        let mut tokens = Vec::with_capacity((kc + kp) * d);
        tokens.extend_from_slice(&ctx);
        tokens.extend_from_slice(&pred);
        Ok(tokens)
    }

    /// Regenerate JEPA masks with the same config as load (deterministic when seeded).
    pub fn default_jepa_masks(&self) -> (Vec<i64>, Vec<Vec<i64>>) {
        let cfg = crate::masks::mask_config_for(self.n_rois, self.n_time_patches);
        masks::jepa_masks(&cfg)
    }

    pub fn describe(&self) -> String {
        format!(
            "Brain-JEPA JEPA (RLX)  enc {}x{}  pred {}x{}  ctx={} pred={}",
            self.embed_dim,
            self.model_cfg.depth,
            self.model_cfg.pred_emb_dim,
            self.model_cfg.pred_depth,
            self.n_ctx,
            self.n_pred,
        )
    }
}
