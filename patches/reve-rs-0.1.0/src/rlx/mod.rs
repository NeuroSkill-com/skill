//! RLX-backed REVE inference (`rlx::Graph` + `rlx::Session`).

pub mod device;
pub mod encoder;
pub mod graph;
pub mod pos_embed;
pub mod weights;

pub use device::prepare_device;
pub use encoder::{EncodingResult, ReveEncoder, ReveOutput};
pub use graph::{
    build_reve_classification_eval_graph,
    build_reve_classification_training_graph,
    build_reve_graph_range, get_lora_rank, set_lora_rank, ReveSpec,
};
pub use pos_embed::precompute_pos_embed;
pub use weights::{apply_params, load_safetensors, ParamMap};

