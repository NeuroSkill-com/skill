/// Load pretrained EEG-DINO weights from a safetensors file.
///
/// The safetensors file is produced by `scripts/convert_weights.py` which
/// strips the `module.student.` prefix, transposes linear weights to
/// burn `[in, out]` layout, and saves everything as float32.
use std::collections::HashMap;

use burn::module::{Param, ParamId};
use burn::prelude::*;
use safetensors::SafeTensors;

use crate::config::ModelConfig;
use crate::error::{EegDinoError, Result};
use crate::model::classifier::ClassificationModel;
use crate::model::encoder::EEGEncoder;

// ── Weight map ──────────────────────────────────────────────────────────────

pub(crate) struct WeightMap {
    tensors: HashMap<String, (Vec<f32>, Vec<usize>)>,
}

impl WeightMap {
    /// Load all tensors from a safetensors file.
    pub fn from_file(path: &str) -> Result<Self> {
        let bytes =
            std::fs::read(path).map_err(|e| EegDinoError::WeightLoad(format!("{path}: {e}")))?;
        let st = SafeTensors::deserialize(&bytes)
            .map_err(|e| EegDinoError::WeightLoad(format!("{path}: {e}")))?;
        let mut tensors = HashMap::with_capacity(st.len());

        for (key, view) in st.tensors() {
            let shape: Vec<usize> = view.shape().to_vec();
            let raw_bytes = view.data();
            let data: Vec<f32> = raw_bytes
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            tensors.insert(key.to_string(), (data, shape));
        }

        Ok(Self { tensors })
    }

    fn take<B: Backend, const D: usize>(
        &mut self,
        key: &str,
        device: &B::Device,
    ) -> Result<Tensor<B, D>> {
        let (data, shape) =
            self.tensors
                .remove(key)
                .ok_or_else(|| EegDinoError::MissingWeight {
                    key: key.to_string(),
                })?;
        let t = Tensor::<B, 1>::from_floats(data.as_slice(), device);
        let shape_arr: [usize; D] =
            shape
                .clone()
                .try_into()
                .map_err(|_| EegDinoError::ShapeMismatch {
                    key: key.to_string(),
                    expected: D,
                    actual: shape,
                })?;
        Ok(t.reshape(shape_arr))
    }

    fn take_param<B: Backend, const D: usize>(
        &mut self,
        key: &str,
        device: &B::Device,
    ) -> Result<Param<Tensor<B, D>>> {
        let t = self.take::<B, D>(key, device)?;
        Ok(Param::initialized(ParamId::new(), t))
    }

    pub fn contains(&self, key: &str) -> bool {
        self.tensors.contains_key(key)
    }

    pub fn detect_model_size(&self) -> Result<crate::config::ModelSize> {
        let (_, shape) = self
            .tensors
            .get("global_tokens")
            .ok_or_else(|| EegDinoError::UnknownModelSize("missing global_tokens key".into()))?;
        match shape.last().copied() {
            Some(200) => Ok(crate::config::ModelSize::Small),
            Some(512) => Ok(crate::config::ModelSize::Medium),
            Some(1024) => Ok(crate::config::ModelSize::Large),
            other => Err(EegDinoError::UnknownModelSize(format!(
                "unexpected feature_size: {other:?}"
            ))),
        }
    }
}

// ── Load encoder ────────────────────────────────────────────────────────────

pub(crate) fn load_encoder<B: Backend>(
    cfg: &ModelConfig,
    weights_path: &str,
    device: &B::Device,
) -> Result<EEGEncoder<B>> {
    let mut w = WeightMap::from_file(weights_path)?;
    let mut enc = EEGEncoder::new(cfg, device);
    load_encoder_into::<B>(&mut w, &mut enc, cfg, device)?;
    Ok(enc)
}

pub(crate) fn load_classifier<B: Backend>(
    cfg: &ModelConfig,
    num_classes: usize,
    weights_path: &str,
    device: &B::Device,
) -> Result<ClassificationModel<B>> {
    let mut w = WeightMap::from_file(weights_path)?;
    let mut model = ClassificationModel::new(cfg, num_classes, device);
    load_encoder_into::<B>(&mut w, &mut model.encoder, cfg, device)?;

    if w.contains("full_linear.weight") {
        model.full_linear.weight = w.take_param("full_linear.weight", device)?;
        model.full_linear.bias = Some(w.take_param("full_linear.bias", device)?);
        model.channel_linear.weight = w.take_param("channel_linear.weight", device)?;
        model.channel_linear.bias = Some(w.take_param("channel_linear.bias", device)?);
        model.cls_fc1.weight = w.take_param("classifier.0.weight", device)?;
        model.cls_fc1.bias = Some(w.take_param("classifier.0.bias", device)?);
        model.cls_fc2.weight = w.take_param("classifier.3.weight", device)?;
        model.cls_fc2.bias = Some(w.take_param("classifier.3.bias", device)?);
        model.cls_fc3.weight = w.take_param("classifier.6.weight", device)?;
        model.cls_fc3.bias = Some(w.take_param("classifier.6.bias", device)?);
    }

    Ok(model)
}

// ── Internal ────────────────────────────────────────────────────────────────

fn load_encoder_into<B: Backend>(
    w: &mut WeightMap,
    enc: &mut EEGEncoder<B>,
    cfg: &ModelConfig,
    device: &B::Device,
) -> Result<()> {
    load_conv_block::<B>(
        w,
        &mut enc.patch_embedding.conv_block1,
        "patch_embedding.proj_in.conv1",
        "patch_embedding.proj_in.norm1",
        device,
    )?;
    load_conv_block::<B>(
        w,
        &mut enc.patch_embedding.conv_block2,
        "patch_embedding.proj_in.conv2",
        "patch_embedding.proj_in.norm2",
        device,
    )?;
    load_conv_block::<B>(
        w,
        &mut enc.patch_embedding.conv_block3,
        "patch_embedding.proj_in.conv3",
        "patch_embedding.proj_in.norm3",
        device,
    )?;

    enc.patch_embedding.spectral_proj.weight =
        w.take_param("patch_embedding.spectral_proj.weight", device)?;
    enc.patch_embedding.spectral_proj.bias =
        Some(w.take_param("patch_embedding.spectral_proj.bias", device)?);
    enc.patch_embedding.channel_embedding.weight =
        w.take_param("patch_embedding.channel_embedding.weight", device)?;
    enc.patch_embedding.channel_embedding.bias =
        Some(w.take_param("patch_embedding.channel_embedding.bias", device)?);
    enc.patch_embedding.time_encoding = load_conv2d::<B>(
        w,
        "patch_embedding.time_encoding",
        enc.patch_embedding.time_encoding.clone(),
        device,
    )?;

    enc.global_tokens = w.take_param("global_tokens", device)?;

    for i in 0..cfg.num_layers {
        let prefix = format!("encoder_layers.{i}");
        let layer = &mut enc.encoder_layers[i];

        layer.norm1 =
            load_layer_norm::<B>(w, &format!("{prefix}.norm1"), layer.norm1.clone(), device)?;
        layer.attn.qkv.weight = w.take_param(&format!("{prefix}.attn.qkv.weight"), device)?;
        layer.attn.q_bias = w.take_param(&format!("{prefix}.attn.q_bias"), device)?;
        layer.attn.v_bias = w.take_param(&format!("{prefix}.attn.v_bias"), device)?;
        layer.attn.proj.weight = w.take_param(&format!("{prefix}.attn.proj.weight"), device)?;
        layer.attn.proj.bias = Some(w.take_param(&format!("{prefix}.attn.proj.bias"), device)?);
        layer.norm2 =
            load_layer_norm::<B>(w, &format!("{prefix}.norm2"), layer.norm2.clone(), device)?;
        layer.mlp.fc1.weight = w.take_param(&format!("{prefix}.mlp.fc1.weight"), device)?;
        layer.mlp.fc1.bias = Some(w.take_param(&format!("{prefix}.mlp.fc1.bias"), device)?);
        layer.mlp.fc2.weight = w.take_param(&format!("{prefix}.mlp.fc2.weight"), device)?;
        layer.mlp.fc2.bias = Some(w.take_param(&format!("{prefix}.mlp.fc2.bias"), device)?);

        layer.attn.fuse_qkv_bias();
    }

    Ok(())
}

fn load_conv_block<B: Backend>(
    w: &mut WeightMap,
    block: &mut crate::model::embedding::ConvNormBlock<B>,
    conv_prefix: &str,
    norm_prefix: &str,
    device: &B::Device,
) -> Result<()> {
    block.conv = load_conv2d::<B>(w, conv_prefix, block.conv.clone(), device)?;
    block.norm = load_group_norm::<B>(w, norm_prefix, block.norm.clone(), device)?;
    Ok(())
}

fn load_conv2d<B: Backend>(
    w: &mut WeightMap,
    prefix: &str,
    mut conv: burn::nn::conv::Conv2d<B>,
    device: &B::Device,
) -> Result<burn::nn::conv::Conv2d<B>> {
    conv.weight = w.take_param(&format!("{prefix}.weight"), device)?;
    if w.contains(&format!("{prefix}.bias")) {
        conv.bias = Some(w.take_param(&format!("{prefix}.bias"), device)?);
    }
    Ok(conv)
}

fn load_group_norm<B: Backend>(
    w: &mut WeightMap,
    prefix: &str,
    mut norm: burn::nn::GroupNorm<B>,
    device: &B::Device,
) -> Result<burn::nn::GroupNorm<B>> {
    norm.gamma = Some(w.take_param(&format!("{prefix}.weight"), device)?);
    norm.beta = Some(w.take_param(&format!("{prefix}.bias"), device)?);
    Ok(norm)
}

fn load_layer_norm<B: Backend>(
    w: &mut WeightMap,
    prefix: &str,
    mut norm: burn::nn::LayerNorm<B>,
    device: &B::Device,
) -> Result<burn::nn::LayerNorm<B>> {
    norm.gamma = w.take_param(&format!("{prefix}.weight"), device)?;
    norm.beta = Some(w.take_param(&format!("{prefix}.bias"), device)?);
    Ok(norm)
}
