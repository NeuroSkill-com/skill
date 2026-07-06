//! Brain-JEPA fMRI classification — RLX encoder + RLX head.
//!
//! ```text
//! cargo build --release --bin classify
//! cargo run --release --bin classify -- \
//!   --weights data/brainjepa.safetensors \
//!   --gradient data/gradient_mapping_450.csv \
//!   --input data/test_fmri.safetensors \
//!   --head-weights path/to/downstream_head.safetensors
//! ```

use clap::Parser;

use brainjepa::rlx::{
    ensure_device, parse_device, predict_class, BrainJepaEncoder, RlxClassificationHead,
};
use brainjepa::{DataConfig, ModelConfig};

#[derive(Parser, Debug)]
#[command(about = "Brain-JEPA fMRI classification (RLX)")]
struct Args {
    #[arg(long, env = "BRAINJEPA_WEIGHTS")]
    weights: Option<String>,

    #[arg(long, env = "BRAINJEPA_GRADIENT")]
    gradient: Option<String>,

    #[arg(long)]
    input: String,

    /// Safetensors with `head.*` / `fc_norm.*` (optional — untrained head if omitted).
    #[arg(long)]
    head_weights: Option<String>,

    #[arg(long, default_value = "")]
    head_prefix: String,

    #[arg(long, default_value = "2")]
    num_classes: usize,

    #[arg(long, default_value = "cpu")]
    device: String,

    #[arg(long, default_value = "vit_base")]
    model: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    brainjepa::init_threads(None);

    let resolved = brainjepa::resolve_weights(
        brainjepa::DEFAULT_REPO,
        args.weights.as_deref(),
        args.gradient.as_deref(),
        None,
    )?;
    let weights = resolved.weights_path.display().to_string();
    let gradient = resolved.gradient_path.display().to_string();

    let dev = parse_device(&args.device)?;
    ensure_device(dev)?;

    let model_cfg = ModelConfig::from_variant(&args.model)?;
    let data_cfg = DataConfig::default();

    let (mut encoder, _) =
        BrainJepaEncoder::from_weights(&weights, &gradient, &model_cfg, &data_cfg, &dev)?;

    let enc = encoder.encode_safetensors(&args.input)?;
    let n_patches = enc.n_patches();

    let mut head = RlxClassificationHead::new(
        n_patches,
        model_cfg.embed_dim,
        args.num_classes,
        model_cfg.norm_eps as f32,
        &dev,
    )?;

    if let Some(ref hw) = args.head_weights {
        head.load_weights(hw, &args.head_prefix)?;
    }

    let logits = head.forward(&enc.embeddings)?;
    let pred = predict_class(&logits);

    println!("Logits ({}) : {logits:?}", logits.len());
    println!("Predicted class: {pred}");

    Ok(())
}
