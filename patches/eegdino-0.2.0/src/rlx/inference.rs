//! RLX inference utilities.

use std::path::Path;

use crate::config::ModelSize;
use crate::error::{EegDinoError, Result};

use super::weights::detect_model_size as detect_anyhow;

/// Detect model size from a safetensors file without loading all weights.
pub fn detect_model_size(weights_path: &Path) -> Result<ModelSize> {
    let path_str = weights_path
        .to_str()
        .ok_or_else(|| EegDinoError::Builder("weights path is not valid UTF-8".into()))?;
    detect_anyhow(path_str).map_err(|e| EegDinoError::WeightLoad(e.to_string()))
}
