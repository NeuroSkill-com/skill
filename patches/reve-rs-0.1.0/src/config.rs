/// Model and runtime configuration for REVE inference.
///
/// Field names match the REVE hyperparameters from the Python implementation.

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelConfig {
    /// Embedding dimension (512 for REVE-Base, 1250 for REVE-Large).
    #[serde(default = "default_embed_dim")]
    pub embed_dim: usize,

    /// Number of Transformer layers.
    #[serde(default = "default_depth")]
    pub depth: usize,

    /// Number of attention heads.
    #[serde(default = "default_heads")]
    pub heads: usize,

    /// Dimension per attention head.
    #[serde(default = "default_head_dim")]
    pub head_dim: usize,

    /// FFN hidden dimension ratio: mlp_dim = embed_dim * mlp_dim_ratio.
    #[serde(default = "default_mlp_dim_ratio")]
    pub mlp_dim_ratio: f64,

    /// Use GEGLU activation.
    #[serde(default = "default_use_geglu")]
    pub use_geglu: bool,

    /// Number of frequencies for Fourier positional embedding.
    #[serde(default = "default_freqs")]
    pub freqs: usize,

    /// Temporal patch size in samples.
    #[serde(default = "default_patch_size")]
    pub patch_size: usize,

    /// Overlap between patches in samples.
    #[serde(default = "default_patch_overlap")]
    pub patch_overlap: usize,

    /// Use attention pooling for classification.
    #[serde(default)]
    pub attention_pooling: bool,

    /// Number of output classes.
    #[serde(default)]
    pub n_outputs: usize,

    /// Number of EEG channels.
    #[serde(default)]
    pub n_chans: usize,

    /// Number of time samples per input.
    #[serde(default)]
    pub n_times: usize,
}

fn default_embed_dim()     -> usize { 512 }
fn default_depth()         -> usize { 22 }
fn default_heads()         -> usize { 8 }
fn default_head_dim()      -> usize { 64 }
fn default_mlp_dim_ratio() -> f64   { 2.66 }
fn default_use_geglu()     -> bool  { true }
fn default_freqs()         -> usize { 4 }
fn default_patch_size()    -> usize { 200 }
fn default_patch_overlap() -> usize { 20 }

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            embed_dim:         default_embed_dim(),
            depth:             default_depth(),
            heads:             default_heads(),
            head_dim:          default_head_dim(),
            mlp_dim_ratio:     default_mlp_dim_ratio(),
            use_geglu:         default_use_geglu(),
            freqs:             default_freqs(),
            patch_size:        default_patch_size(),
            patch_overlap:     default_patch_overlap(),
            attention_pooling: false,
            n_outputs:         4,
            n_chans:           22,
            n_times:           1000,
        }
    }
}

impl ModelConfig {
    /// Inner attention dimension: head_dim * heads.
    pub fn inner_dim(&self) -> usize {
        self.head_dim * self.heads
    }

    /// FFN hidden dimension.
    pub fn mlp_dim(&self) -> usize {
        (self.embed_dim as f64 * self.mlp_dim_ratio) as usize
    }

    /// GEGLU doubles the FFN input features.
    pub fn ffn_in_features(&self) -> usize {
        let mlp = self.mlp_dim();
        if self.use_geglu { mlp * 2 } else { mlp }
    }
}
