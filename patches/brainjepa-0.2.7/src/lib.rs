//! # brainjepa-rs — Brain-JEPA fMRI Foundation Model inference in Rust
//!
//! Inference uses [RLX](https://docs.rs/rlx) (`rlx-engine`, default).
//!
//! | Binary | Purpose |
//! |--------|---------|
//! | `infer` | Encoder embeddings |
//! | `classify` | Downstream classification |
//! | `predict` | JEPA masked prediction |

#[cfg(not(feature = "rlx"))]
compile_error!("enable `rlx-engine` (default)");

/// Configure the global Rayon thread pool.
pub fn init_threads(n: Option<usize>) -> usize {
    let mut builder = rayon::ThreadPoolBuilder::new();
    if let Some(count) = n {
        if count > 0 {
            builder = builder.num_threads(count);
        }
    }
    let _ = builder.build_global();
    rayon::current_num_threads()
}

pub mod config;
pub mod csv_export;
pub mod data;
pub mod error;
pub mod hf_download;
pub mod masks;
pub mod prelude;

#[cfg(feature = "rlx")]
pub mod rlx;

pub use config::{DataConfig, ModelConfig, YamlConfig};
pub use csv_export::save_embeddings_csv;
pub use data::{FmriInputF32, GradientData};
pub use error::{BrainJepaError, Result};
pub use hf_download::{resolve as resolve_weights, ResolvedWeights, DEFAULT_REPO};
pub use masks::{full_context_mask, jepa_masks, mask_config_for, random_block_mask, MaskConfig};

#[cfg(feature = "rlx")]
pub use rlx::{
    predict_class, AttnLayout, BrainJepaEncoder, BrainJepaPredictor, EmbeddingResult,
    RlxClassificationHead as ClassificationHead,
};
