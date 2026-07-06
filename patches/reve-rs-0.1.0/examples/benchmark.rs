//! REVE RLX inference benchmark.
//!
//! Usage:
//!   benchmark [--device cpu|metal|mlx] [--config PATH] [--weights PATH] \
//!       <n_chans> <n_times> <warmup> <repeats>
//!
//! Prints JSON to stdout:
//!   {"times_ms": [..], "backend": "rlx-cpu", "ok": true, "output_dim": 512}

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use reve_rs::rlx::ReveEncoder;

#[derive(Parser, Debug)]
#[command(about = "REVE RLX inference benchmark")]
struct Args {
    #[arg(long, default_value = "cpu")]
    device: String,

    #[arg(long)]
    config: Option<PathBuf>,

    #[arg(long)]
    weights: Option<PathBuf>,

    n_chans: usize,
    n_times: usize,
    warmup: usize,
    repeats: usize,
}

fn locate_paths(config: Option<PathBuf>, weights: Option<PathBuf>) -> anyhow::Result<(PathBuf, PathBuf)> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config = config.unwrap_or_else(|| manifest.join("data/config.json"));
    let weights = weights.unwrap_or_else(|| manifest.join("data/model.safetensors"));
    anyhow::ensure!(config.exists(), "config not found: {}", config.display());
    anyhow::ensure!(weights.exists(), "weights not found: {}", weights.display());
    Ok((config, weights))
}

fn parse_device(s: &str) -> anyhow::Result<rlx::Device> {
    Ok(match s.to_lowercase().as_str() {
        "cpu" => rlx::Device::Cpu,
        "metal" => rlx::Device::Metal,
        "mlx" => rlx::Device::Mlx,
        other => anyhow::bail!("unsupported device '{other}' (use cpu, metal, or mlx)"),
    })
}

fn backend_name(dev: rlx::Device) -> &'static str {
    match dev {
        rlx::Device::Cpu => "rlx-cpu",
        rlx::Device::Metal => "rlx-metal",
        rlx::Device::Mlx => "rlx-mlx",
        _ => "rlx-other",
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let dev = parse_device(&args.device)?;
    let (config, weights) = locate_paths(args.config, args.weights)?;

    let (mut enc, _) = ReveEncoder::load(&config, &weights, dev)?;

    let n_chans = args.n_chans;
    let n_times = args.n_times;
    let signal = vec![0.0f32; n_chans * n_times];
    let positions = vec![0.0f32; n_chans * 3];

    for _ in 0..args.warmup {
        let out = enc.run_one(signal.clone(), positions.clone(), n_chans, n_times)?;
        anyhow::ensure!(
            out.output.iter().all(|v| v.is_finite()),
            "warmup produced non-finite values"
        );
    }

    let mut times = Vec::with_capacity(args.repeats);
    let mut last_dim = 0usize;
    for _ in 0..args.repeats {
        let t0 = Instant::now();
        let out = enc.run_one(signal.clone(), positions.clone(), n_chans, n_times)?;
        anyhow::ensure!(
            out.output.iter().all(|v| v.is_finite()),
            "run produced non-finite values"
        );
        last_dim = out.output.len();
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }

    let times_str: Vec<String> = times.iter().map(|t| format!("{t:.4}")).collect();
    println!(
        "{{\"times_ms\": [{}], \"backend\": \"{}\", \"ok\": true, \"output_dim\": {}}}",
        times_str.join(", "),
        backend_name(dev),
        last_dim,
    );
    Ok(())
}
