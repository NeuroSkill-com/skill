//! RLX-backed Brain-JEPA inference (`rlx::Graph` + `rlx::Session`).
//!
//! RLX-backed Brain-JEPA inference (`rlx-engine`, default).

pub mod attn_layout;
pub mod classification;
pub mod device;
pub mod graph;
pub mod inference;
pub mod mask_ops;
pub mod pos_embed_cpu;
pub mod predictor;
pub mod weights;

pub use attn_layout::{resolve_attn_layout, AttnLayout};
pub use classification::{predict_class, ClassificationHead as RlxClassificationHead};
pub use device::{
    available_devices, ensure_device, parse_device, prepare_device, recommended_features,
};
pub use inference::{BrainJepaEncoder, EmbeddingResult};
pub use predictor::BrainJepaPredictor;
