//! Brain-JEPA encoder inference — RLX engine only.
//!
//! ```text
//! cargo build --release                              # rlx-engine (default)
//! cargo build --release --no-default-features --features rlx-engine,rlx-metal
//! cargo run --release --bin infer -- --device metal --input ...
//! ```
use std::time::Instant;

use clap::Parser;

use brainjepa::rlx::{ensure_device, parse_device, BrainJepaEncoder};
use brainjepa::{DataConfig, ModelConfig};

#[derive(Parser, Debug)]
#[command(about = "Brain-JEPA fMRI encoder inference (RLX engine)")]
struct Args {
    #[arg(long, env = "BRAINJEPA_WEIGHTS")]
    weights: Option<String>,

    #[arg(long, env = "BRAINJEPA_GRADIENT")]
    gradient: Option<String>,

    #[arg(long)]
    input: String,

    #[arg(long, default_value = "embeddings.safetensors")]
    output: String,

    #[arg(long, default_value = "vit_base")]
    model: String,

    #[arg(long)]
    config: Option<String>,

    /// RLX device: cpu, metal, mlx, gpu, cuda, rocm, tpu (aliases: wgpu, mtl).
    #[arg(long, default_value = "cpu")]
    device: String,

    #[arg(long, default_value = brainjepa::DEFAULT_REPO)]
    repo: String,

    #[arg(long, env = "RAYON_NUM_THREADS")]
    threads: Option<usize>,

    #[arg(long, short = 'v')]
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let n_threads = brainjepa::init_threads(args.threads);
    let t0 = Instant::now();

    let resolved = brainjepa::resolve_weights(
        &args.repo,
        args.weights.as_deref(),
        args.gradient.as_deref(),
        None,
    )?;
    let weights = resolved.weights_path.display().to_string();
    let gradient = resolved.gradient_path.display().to_string();

    let rlx_dev = parse_device(&args.device)?;
    ensure_device(rlx_dev)?;
    println!(
        "Backend  : {}  ({n_threads} threads)",
        brainjepa::rlx::device::display_name(rlx_dev)
    );

    let (model_cfg, data_cfg) = if let Some(ref cfg_path) = args.config {
        let yaml = brainjepa::YamlConfig::from_file(cfg_path)?;
        (yaml.to_model_config()?, yaml.to_data_config())
    } else {
        (
            ModelConfig::from_variant(&args.model)?,
            DataConfig::default(),
        )
    };

    println!("Loading  : {weights}");
    let (mut encoder, ms_weights) =
        BrainJepaEncoder::from_weights(&weights, &gradient, &model_cfg, &data_cfg, &rlx_dev)?;
    println!("Model    : {}  ({ms_weights:.0} ms)", encoder.describe());

    println!("Input    : {}", args.input);
    let result = if args.input.ends_with(".csv") {
        encoder.encode_csv(&args.input)?
    } else {
        encoder.encode_safetensors(&args.input)?
    };

    println!(
        "Encoding : {} patches × {} dims  ({:.1} ms)",
        result.n_patches(),
        result.embed_dim(),
        result.ms_encode
    );

    if args.verbose {
        let mean: f64 = result.embeddings.iter().map(|&v| v as f64).sum::<f64>()
            / result.embeddings.len() as f64;
        let std: f64 = (result
            .embeddings
            .iter()
            .map(|&v| {
                let d = v as f64 - mean;
                d * d
            })
            .sum::<f64>()
            / result.embeddings.len() as f64)
            .sqrt();
        println!("  mean={mean:.4}  std={std:.4}");
    }

    result.save_safetensors(&args.output)?;
    println!("Output   : {}", args.output);

    let ms_total = t0.elapsed().as_secs_f64() * 1000.0;
    println!("Total    : {ms_total:.0} ms");
    eprintln!(
        "TIMING weights={ms_weights:.1}ms encode={:.1}ms total={ms_total:.1}ms",
        result.ms_encode
    );

    Ok(())
}
