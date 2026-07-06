//! RLX-backed EEG-DINO inference (`rlx::Graph` + `rlx::Session`).

pub mod device;
pub mod encoder;
pub mod graph;
pub mod inference;
pub mod weights;

pub use device::{device_label, feature_for, is_device_available, parse_device};
pub use encoder::{EegDinoEncoder, EegDinoEncoderBuilder, EncodingResult};
pub use inference::detect_model_size;
