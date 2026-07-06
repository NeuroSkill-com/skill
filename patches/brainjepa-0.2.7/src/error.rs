/// Typed errors for brainjepa.
///
/// All public APIs return `Result<T, BrainJepaError>` (or `anyhow::Result`
/// for the CLI).  Match on the variant to handle specific failure modes.
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum BrainJepaError {
    // ── Config ───────────────────────────────────────────────────────────────
    #[error("unknown model variant \"{name}\" (expected vit_small, vit_base, or vit_large)")]
    UnknownVariant { name: String },

    #[error("invalid positional embedding mode \"{mode}\" (expected \"mapping\" or \"origin\")")]
    InvalidPosMode { mode: String },

    // ── Files ────────────────────────────────────────────────────────────────
    #[error("{kind} file not found: {path}")]
    FileNotFound { kind: &'static str, path: PathBuf },

    // ── Data ─────────────────────────────────────────────────────────────────
    #[error("CSV is empty or contains no valid rows: {path}")]
    EmptyCsv { path: PathBuf },

    #[error("CSV row {row} has {got} columns, expected {expected} (file: {path})")]
    InconsistentCsvRow {
        path: PathBuf,
        row: usize,
        expected: usize,
        got: usize,
    },

    #[error("gradient CSV has {got} ROIs, expected {expected}")]
    GradientRoiMismatch { expected: usize, got: usize },

    #[error("cannot downsample {src} frames to {dst} (target must be <= source)")]
    DownsampleUpscale { src: usize, dst: usize },

    // ── Weights ──────────────────────────────────────────────────────────────
    #[error("weight key not found: {key}")]
    WeightKeyMissing { key: String },

    // ── Tensor ───────────────────────────────────────────────────────────────
    #[error("tensor conversion failed: {reason}")]
    TensorConversion { reason: String },

    // ── Pass-through ─────────────────────────────────────────────────────────
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, BrainJepaError>;
