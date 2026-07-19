//! REVE EEG inference — thin CLI over `reve_rs::rlx::ReveEncoder`.
//!
//! Build:
//!   cargo build --release                       # CPU (default, RLX)
//!   cargo build --release --features rlx-metal  # Apple Metal
//!   cargo build --release --features rlx-mlx    # Apple MLX
//!
//! Usage:
//!   infer --weights <st> --config <json> [--device cpu|metal|mlx|gpu|cuda|rocm|tpu]

use std::{path::Path, time::Instant};
use clap::{Parser, ValueEnum};

use reve_rs::rlx::ReveEncoder;

// ── CLI ───────────────────────────────────────────────────────────────────────
#[derive(Parser, Debug)]
#[command(about = "REVE EEG model inference (RLX runtime)")]
struct Args {
    /// Compute device.
    #[arg(long, default_value = "cpu")]
    device: DeviceArg,

    /// Safetensors weights file.
    #[arg(long)]
    weights: String,

    /// config.json.
    #[arg(long)]
    config: String,

    /// Print details.
    #[arg(long, short = 'v')]
    verbose: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DeviceArg {
    Cpu,
    Metal,
    Mlx,
    Gpu,
    Cuda,
    Rocm,
    Tpu,
}

impl DeviceArg {
    fn into_rlx(self) -> rlx::Device {
        match self {
            Self::Cpu => rlx::Device::Cpu,
            Self::Metal => rlx::Device::Metal,
            Self::Mlx => rlx::Device::Mlx,
            Self::Gpu => rlx::Device::Gpu,
            Self::Cuda => rlx::Device::Cuda,
            Self::Rocm => rlx::Device::Rocm,
            Self::Tpu => rlx::Device::Tpu,
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let t0 = Instant::now();
    let dev = args.device.into_rlx();
    eprintln!("Device   : {:?}", dev);

    // Load model
    let (mut model, ms_weights) = ReveEncoder::load(
        Path::new(&args.config),
        Path::new(&args.weights),
        dev,
    )?;

    eprintln!("Model    : {}  ({ms_weights:.0} ms)", model.describe());

    // Example: create a dummy input for testing
    let n_channels = 22;
    let n_samples = 1000; // 5s @ 200 Hz

    // Dummy positions (normally from position bank)
    let positions = vec![0.0f32; n_channels * 3];
    let signal = vec![0.0f32; n_channels * n_samples];

    let t_inf = Instant::now();
    let result = model.run_one(signal, positions, n_channels, n_samples)?;
    let ms_infer = t_inf.elapsed().as_secs_f64() * 1000.0;

    eprintln!("Output   : shape={:?}  ({ms_infer:.1} ms)", result.shape);

    if args.verbose {
        let mean: f64 = result.output.iter().map(|&v| v as f64).sum::<f64>()
            / result.output.len() as f64;
        let std: f64 = (result.output
            .iter()
            .map(|&v| {
                let d = v as f64 - mean;
                d * d
            })
            .sum::<f64>()
            / result.output.len() as f64)
            .sqrt();
        println!("  mean={mean:+.4}  std={std:.4}");
    }

    let ms_total = t0.elapsed().as_secs_f64() * 1000.0;
    eprintln!("── Timing ───────────────────────────────────────────────────────");
    eprintln!("  Weights  : {ms_weights:.0} ms");
    eprintln!("  Infer    : {ms_infer:.0} ms");
    eprintln!("  Total    : {ms_total:.0} ms");

    Ok(())
}
