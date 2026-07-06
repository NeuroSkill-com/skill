use serde::{Deserialize, Serialize};

/// Model size variants matching the Python EEG-DINO codebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelSize {
    Small,
    Medium,
    Large,
}

/// Full model configuration derived from model size.
///
/// All values match the Python EEG-DINO defaults exactly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub model_size: ModelSize,
    /// Embedding dimension (d_model): 200 / 512 / 1024
    pub feature_size: usize,
    /// Number of attention heads: 8 / 16 / 24
    pub num_heads: usize,
    /// Number of transformer encoder layers: 12 / 12 / 24
    pub num_layers: usize,
    /// Feed-forward hidden dimension: 512 / 1024 / 2048
    pub dim_feedforward: usize,
    /// Number of learnable global tokens (default: 1)
    pub num_global_tokens: usize,
    /// Layer index (1-based) at which global tokens are injected (default: 1)
    pub global_token_layer: usize,
    /// Number of EEG channels (default: 19)
    pub num_channels: usize,
    /// Samples per patch (default: 200)
    pub patch_size: usize,
    /// Conv channel widths for the 3 conv layers in proj_in: [c1, c2, c3]
    pub conv_channels: [usize; 3],
    /// GroupNorm group counts for the 3 norm layers in proj_in
    pub norm_groups: [usize; 3],
    /// LayerNorm epsilon
    pub layer_norm_eps: f64,
}

impl ModelConfig {
    pub fn from_size(size: ModelSize) -> Self {
        match size {
            ModelSize::Small => Self {
                model_size: size,
                feature_size: 200,
                num_heads: 8,
                num_layers: 12,
                dim_feedforward: 512,
                num_global_tokens: 1,
                global_token_layer: 1,
                num_channels: 19,
                patch_size: 200,
                conv_channels: [25, 25, 25],
                norm_groups: [5, 5, 5],
                layer_norm_eps: 1e-5,
            },
            ModelSize::Medium => Self {
                model_size: size,
                feature_size: 512,
                num_heads: 16,
                num_layers: 16,
                dim_feedforward: 1024,
                num_global_tokens: 1,
                global_token_layer: 1,
                num_channels: 19,
                patch_size: 200,
                conv_channels: [64, 128, 64],
                norm_groups: [8, 8, 8],
                layer_norm_eps: 1e-5,
            },
            ModelSize::Large => Self {
                model_size: size,
                feature_size: 1024,
                // NOTE: README claims 24 heads, but 1024/24 = 42.67 is non-integer.
                // Weights confirm all_head_dim=1024, so num_heads must divide 1024.
                // 16 heads (head_dim=64) matches the weight dimensions.
                num_heads: 16,
                num_layers: 24,
                dim_feedforward: 2048,
                num_global_tokens: 1,
                global_token_layer: 1,
                num_channels: 19,
                patch_size: 200,
                conv_channels: [128, 256, 128],
                norm_groups: [16, 16, 16],
                layer_norm_eps: 1e-5,
            },
        }
    }

    /// Load config from a JSON file.
    pub fn from_file(path: &std::path::Path) -> crate::error::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        serde_json::from_str(&data)
            .map_err(|e| crate::error::EegDinoError::WeightLoad(format!("config parse error: {e}")))
    }

    /// Head dimension (feature_size / num_heads).
    pub fn head_dim(&self) -> usize {
        self.feature_size / self.num_heads
    }

    /// Number of spectral bins from rfft of patch_size samples: patch_size/2 + 1.
    pub fn spectral_bins(&self) -> usize {
        self.patch_size / 2 + 1
    }

    /// Temporal output dimension of the conv stack: floor((patch_size - 49 + 2*24) / 25) + 1.
    pub fn temporal_conv_out(&self) -> usize {
        (self.patch_size - 49 + 2 * 24) / 25 + 1
    }
}
