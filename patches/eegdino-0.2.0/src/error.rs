//! Typed error type for the eegdino-rs public API.
//!
//! All public functions return [`Result<T, EegDinoError>`](Result) instead of
//! `anyhow::Result` so callers can match on specific failure modes.

/// Errors that can occur during model loading or inference.
#[derive(Debug, thiserror::Error)]
pub enum EegDinoError {
    /// A required weight key is missing from the safetensors file.
    #[error("missing weight key: {key}")]
    MissingWeight { key: String },

    /// A weight tensor has the wrong number of dimensions.
    #[error("shape mismatch for {key}: expected {expected}D, got {actual:?}")]
    ShapeMismatch {
        key: String,
        expected: usize,
        actual: Vec<usize>,
    },

    /// The input signal length does not match the expected dimensions.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Cannot determine the model size from the weight file.
    #[error("cannot detect model size: {0}")]
    UnknownModelSize(String),

    /// The weights file could not be read or parsed.
    #[error("failed to load weights: {0}")]
    WeightLoad(String),

    /// Builder is missing a required field.
    #[error("builder error: {0}")]
    Builder(String),

    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, EegDinoError>;
