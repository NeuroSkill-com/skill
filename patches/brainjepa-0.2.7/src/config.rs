/// Model and runtime configuration for Brain-JEPA inference.
///
/// Configuration can be loaded from YAML files matching the Brain-JEPA Python format,
/// or constructed programmatically.
use serde::Deserialize;

// ── ModelConfig ──────────────────────────────────────────────────────────────

/// Architecture hyperparameters for the Vision Transformer.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    /// Model variant name: "vit_small", "vit_base", or "vit_large"
    #[serde(default = "default_model_name")]
    pub model_name: String,

    /// Embedding dimension (384 / 768 / 1024)
    #[serde(default = "default_embed_dim")]
    pub embed_dim: usize,

    /// Number of transformer layers in the encoder
    #[serde(default = "default_depth")]
    pub depth: usize,

    /// Number of attention heads
    #[serde(default = "default_num_heads")]
    pub num_heads: usize,

    /// MLP hidden dim = embed_dim * mlp_ratio
    #[serde(default = "default_mlp_ratio")]
    pub mlp_ratio: f64,

    /// Predictor depth (transformer layers)
    #[serde(default = "default_pred_depth")]
    pub pred_depth: usize,

    /// Predictor embedding dimension
    #[serde(default = "default_pred_emb_dim")]
    pub pred_emb_dim: usize,

    /// Temporal patch size
    #[serde(default = "default_patch_size")]
    pub patch_size: usize,

    /// LayerNorm epsilon
    #[serde(default = "default_norm_eps")]
    pub norm_eps: f64,

    /// Positional embedding mode: "mapping" or "origin"
    #[serde(default = "default_pos_mode")]
    pub pos_mode: String,
}

fn default_model_name() -> String {
    "vit_base".into()
}
fn default_embed_dim() -> usize {
    768
}
fn default_depth() -> usize {
    12
}
fn default_num_heads() -> usize {
    12
}
fn default_mlp_ratio() -> f64 {
    4.0
}
fn default_pred_depth() -> usize {
    6
}
fn default_pred_emb_dim() -> usize {
    384
}
fn default_patch_size() -> usize {
    16
}
fn default_norm_eps() -> f64 {
    1e-6
}
fn default_pos_mode() -> String {
    "mapping".into()
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model_name: default_model_name(),
            embed_dim: default_embed_dim(),
            depth: default_depth(),
            num_heads: default_num_heads(),
            mlp_ratio: default_mlp_ratio(),
            pred_depth: default_pred_depth(),
            pred_emb_dim: default_pred_emb_dim(),
            patch_size: default_patch_size(),
            norm_eps: default_norm_eps(),
            pos_mode: default_pos_mode(),
        }
    }
}

impl ModelConfig {
    /// Construct a config for one of the standard ViT variants.
    pub fn from_variant(name: &str) -> crate::error::Result<Self> {
        match name {
            "vit_small" => Ok(Self {
                model_name: "vit_small".into(),
                embed_dim: 384,
                depth: 12,
                num_heads: 6,
                ..Default::default()
            }),
            "vit_base" => Ok(Self::default()),
            "vit_large" => Ok(Self {
                model_name: "vit_large".into(),
                embed_dim: 1024,
                depth: 24,
                num_heads: 16,
                ..Default::default()
            }),
            _ => Err(crate::error::BrainJepaError::UnknownVariant {
                name: name.to_string(),
            }),
        }
    }

    /// Head dimension (embed_dim / num_heads).
    pub fn head_dim(&self) -> usize {
        self.embed_dim / self.num_heads
    }

    /// MLP hidden dimension.
    pub fn mlp_hidden_dim(&self) -> usize {
        (self.embed_dim as f64 * self.mlp_ratio) as usize
    }
}

// ── DataConfig ───────────────────────────────────────────────────────────────

/// fMRI data parameters.
#[derive(Debug, Clone, Deserialize)]
pub struct DataConfig {
    /// Number of ROIs (spatial dimension)
    #[serde(default = "default_n_rois")]
    pub n_rois: usize,

    /// Number of time points (before downsampling)
    #[serde(default = "default_seq_length")]
    pub seq_length: usize,

    /// Number of cortical ROIs
    #[serde(default = "default_n_cortical")]
    pub n_cortical_rois: usize,

    /// Number of subcortical ROIs
    #[serde(default = "default_n_subcortical")]
    pub n_subcortical_rois: usize,

    /// Whether to downsample temporally
    #[serde(default)]
    pub downsample: bool,

    /// Temporal sampling rate (for downsampling)
    #[serde(default = "default_sampling_rate")]
    pub sampling_rate: usize,

    /// Number of frames after downsampling
    #[serde(default = "default_num_frames")]
    pub num_frames: usize,

    /// Input image size (n_rois, n_time)
    #[serde(default = "default_crop_size")]
    pub crop_size: (usize, usize),

    /// Gradient dimension from CSV (3 axes by default)
    #[serde(default = "default_gradient_dim")]
    pub gradient_dim: usize,
}

fn default_n_rois() -> usize {
    450
}
fn default_seq_length() -> usize {
    490
}
fn default_n_cortical() -> usize {
    400
}
fn default_n_subcortical() -> usize {
    50
}
fn default_sampling_rate() -> usize {
    3
}
fn default_num_frames() -> usize {
    160
}
fn default_crop_size() -> (usize, usize) {
    (450, 160)
}
fn default_gradient_dim() -> usize {
    30
}

impl Default for DataConfig {
    fn default() -> Self {
        Self {
            n_rois: default_n_rois(),
            seq_length: default_seq_length(),
            n_cortical_rois: default_n_cortical(),
            n_subcortical_rois: default_n_subcortical(),
            downsample: true,
            sampling_rate: default_sampling_rate(),
            num_frames: default_num_frames(),
            crop_size: default_crop_size(),
            gradient_dim: default_gradient_dim(),
        }
    }
}

// ── Full YAML config ─────────────────────────────────────────────────────────

/// Top-level YAML config matching Brain-JEPA's format.
#[derive(Debug, Clone, Deserialize)]
pub struct YamlConfig {
    pub data: Option<YamlDataSection>,
    pub mask: Option<YamlMaskSection>,
    pub meta: Option<YamlMetaSection>,
    pub optimization: Option<YamlOptSection>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct YamlDataSection {
    pub batch_size: Option<usize>,
    pub crop_size: Option<Vec<usize>>,
    pub num_workers: Option<usize>,
    pub pin_mem: Option<bool>,
    pub gradient_csv_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(non_snake_case)]
pub struct YamlMaskSection {
    pub patch_size: Option<usize>,
    pub min_keep: Option<usize>,
    pub enc_mask_scale: Option<Vec<f64>>,
    pub pred_mask_R_scale: Option<Vec<f64>>,
    pub pred_mask_T_scale: Option<Vec<f64>>,
    pub pred_mask_T_roi_scale: Option<Vec<f64>>,
    pub pred_mask_R_roi_scale: Option<Vec<f64>>,
    pub allow_overlap: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct YamlMetaSection {
    pub model_name: Option<String>,
    pub pred_depth: Option<usize>,
    pub pred_emb_dim: Option<usize>,
    pub use_bfloat16: Option<bool>,
    pub accumulation_steps: Option<usize>,
    pub attn_mode: Option<String>,
    pub add_w: Option<String>,
    pub downsample: Option<bool>,
    pub mask_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct YamlOptSection {
    pub lr: Option<f64>,
    pub start_lr: Option<f64>,
    pub final_lr: Option<f64>,
    pub warmup: Option<usize>,
    pub weight_decay: Option<f64>,
    pub final_weight_decay: Option<f64>,
    pub epochs: Option<usize>,
    pub ema: Option<Vec<f64>>,
    pub ipe_scale: Option<f64>,
}

impl YamlConfig {
    /// Parse from a YAML file.
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let s = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&s)?)
    }

    /// Extract ModelConfig from YAML sections.
    pub fn to_model_config(&self) -> crate::error::Result<ModelConfig> {
        let mut cfg = if let Some(meta) = &self.meta {
            let name = meta.model_name.as_deref().unwrap_or("vit_base");
            let mut c = ModelConfig::from_variant(name)?;
            if let Some(d) = meta.pred_depth {
                c.pred_depth = d;
            }
            if let Some(d) = meta.pred_emb_dim {
                c.pred_emb_dim = d;
            }
            if let Some(ref m) = meta.add_w {
                c.pos_mode = m.clone();
            }
            c
        } else {
            ModelConfig::default()
        };
        if let Some(mask) = &self.mask {
            if let Some(ps) = mask.patch_size {
                cfg.patch_size = ps;
            }
        }
        Ok(cfg)
    }

    /// Extract DataConfig from YAML sections.
    pub fn to_data_config(&self) -> DataConfig {
        let mut cfg = DataConfig::default();
        if let Some(data) = &self.data {
            if let Some(ref cs) = data.crop_size {
                if cs.len() == 2 {
                    cfg.crop_size = (cs[0], cs[1]);
                }
            }
        }
        if let Some(meta) = &self.meta {
            if let Some(ds) = meta.downsample {
                cfg.downsample = ds;
            }
        }
        cfg
    }
}
