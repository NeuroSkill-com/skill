//! Convenience re-exports for common usage.

pub use crate::config::{DataConfig, ModelConfig, YamlConfig};
pub use crate::csv_export::save_embeddings_csv;
pub use crate::data::{FmriInputF32, GradientData};
pub use crate::error::{BrainJepaError, Result};
pub use crate::hf_download::{resolve as resolve_weights, ResolvedWeights, DEFAULT_REPO};
pub use crate::masks::{full_context_mask, jepa_masks, random_block_mask, MaskConfig};

pub use crate::{
    predict_class, BrainJepaEncoder, BrainJepaPredictor, ClassificationHead, EmbeddingResult,
};
