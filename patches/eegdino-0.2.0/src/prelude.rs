//! Convenience re-exports for common usage patterns.
//!
//! ```rust,ignore
//! use eegdino_rs::prelude::*;
//!
//! let device = parse_device("cpu")?;
//! let (mut enc, _) = EegDinoEncoder::load(path, None, device)?;
//! ```

pub use crate::config::{ModelConfig, ModelSize};
pub use crate::error::{EegDinoError, Result};
pub use crate::init_threads;

pub use crate::{
    detect_model_size, device_label, feature_for, is_device_available, parse_device,
    EegDinoEncoder, EegDinoEncoderBuilder, EncodingResult,
};

#[cfg(feature = "burn")]
pub use crate::inference::{
    ClassificationResult, EegDinoClassifier, EegDinoEncoder as BurnEegDinoEncoder,
    EegDinoEncoderBuilder as BurnEegDinoEncoderBuilder, EncodingResult as BurnEncodingResult,
};

#[cfg(feature = "burn")]
pub use crate::model::classifier::ClassificationModel;

#[cfg(feature = "burn")]
pub use crate::model::embedding::{EmbeddingCache, PatchEmbedding};

#[cfg(feature = "burn")]
pub use crate::model::encoder::EEGEncoder;
