//! Brain-JEPA JEPA evaluation — RLX encoder + predictor (masked).
//!
//! ```text
//! cargo build --release --bin predict
//! cargo run --release --bin predict -- \
//!   --weights data/brainjepa.safetensors \
//!   --gradient data/gradient_mapping_450.csv \
//!   --input data/test_fmri.safetensors
//! ```

use clap::Parser;

use brainjepa::{BrainJepaPredictor, DataConfig, ModelConfig};

#[derive(Parser, Debug)]
#[command(about = "Brain-JEPA JEPA predict (RLX)")]
struct Args {
    #[arg(long, env = "BRAINJEPA_WEIGHTS")]
    weights: Option<String>,

    #[arg(long, env = "BRAINJEPA_GRADIENT")]
    gradient: Option<String>,

    #[arg(long)]
    input: String,

    #[arg(long, default_value = "cpu")]
    device: String,

    #[arg(long, default_value = "vit_base")]
    model: String,

    #[arg(long, default_value = brainjepa::DEFAULT_REPO)]
    repo: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    brainjepa::init_threads(None);

    let resolved = brainjepa::resolve_weights(
        &args.repo,
        args.weights.as_deref(),
        args.gradient.as_deref(),
        None,
    )?;
    let weights = resolved.weights_path.display().to_string();
    let gradient = resolved.gradient_path.display().to_string();

    let dev = brainjepa::rlx::parse_device(&args.device)?;
    brainjepa::rlx::ensure_device(dev)?;

    let model_cfg = ModelConfig::from_variant(&args.model)?;
    let data_cfg = DataConfig::default();

    let (mut jepa, ms) =
        BrainJepaPredictor::from_weights(&weights, &gradient, &model_cfg, &data_cfg, &dev)?;
    println!("{}", jepa.describe());
    println!("Loaded in {ms:.0} ms");

    let fmri = brainjepa::data::load_fmri_safetensors_f32(&args.input)?;
    let (enc_idx, pred_masks) = jepa.default_jepa_masks();
    let pred_idx = &pred_masks[0];

    let (enc_out, pred_out) =
        jepa.predict_f32(fmri.data, fmri.n_rois, fmri.n_time, &enc_idx, pred_idx)?;

    println!(
        "Context  : {} patches × {} dims",
        enc_idx.len(),
        model_cfg.embed_dim
    );
    println!(
        "Predicted: {} targets × {} dims",
        pred_idx.len(),
        model_cfg.embed_dim
    );
    println!("enc_out len = {}", enc_out.len());
    println!("pred_out len = {}", pred_out.len());

    Ok(())
}
