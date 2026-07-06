//! Minimal RLX embedding example.
//!
//! ```sh
//! cargo run --example embed_rlx --release -- \
//!   --features rlx-engine \
//!   data/brainjepa.safetensors \
//!   data/gradient_mapping_450.csv \
//!   data/test_fmri.safetensors
//! ```
use brainjepa::rlx::{ensure_device, BrainJepaEncoder};
use brainjepa::{DataConfig, ModelConfig};
use rlx::Device;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "usage: embed_rlx <weights> <gradient.csv> <input.safetensors> [output.safetensors]"
        );
        std::process::exit(1);
    }

    brainjepa::init_threads(None);
    let dev = Device::Cpu;
    ensure_device(dev)?;

    let (mut encoder, ms) = BrainJepaEncoder::from_weights(
        &args[1],
        &args[2],
        &ModelConfig::default(),
        &DataConfig::default(),
        &dev,
    )?;
    println!("Loaded in {ms:.0} ms: {}", encoder.describe());

    let result = encoder.encode_safetensors(&args[3])?;
    println!(
        "Encoded: {} patches x {} dims in {:.1} ms",
        result.n_patches(),
        result.embed_dim(),
        result.ms_encode
    );

    let out = args
        .get(4)
        .map(|s| s.as_str())
        .unwrap_or("embeddings.safetensors");
    result.save_safetensors(out)?;
    println!("Saved: {out}");
    Ok(())
}
