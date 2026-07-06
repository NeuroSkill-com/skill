/// Download REVE pretrained weights from HuggingFace Hub.
///
/// Requires the `hf-download` feature:
///   cargo run --release --features hf-download --bin download_weights -- --repo brain-bzh/reve-base

use clap::Parser;

#[derive(Parser, Debug)]
#[command(about = "Download REVE weights from HuggingFace Hub")]
struct Args {
    /// HuggingFace repo ID (e.g., "brain-bzh/reve-base").
    #[arg(long, default_value = "brain-bzh/reve-base")]
    repo: String,

    /// Output directory.
    #[arg(long, default_value = "data")]
    output_dir: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    #[cfg(feature = "hf-download")]
    {
        use hf_hub::api::sync::Api;
        let api = Api::new()?;
        let repo = api.model(args.repo.clone());

        println!("Downloading from {}...", args.repo);

        // Download model weights
        let weights_path = repo.get("model.safetensors")?;
        let config_path = repo.get("config.json")?;

        std::fs::create_dir_all(&args.output_dir)?;

        let dst_weights = format!("{}/model.safetensors", args.output_dir);
        let dst_config = format!("{}/config.json", args.output_dir);

        std::fs::copy(&weights_path, &dst_weights)?;
        std::fs::copy(&config_path, &dst_config)?;

        println!("Saved weights to: {}", dst_weights);
        println!("Saved config to:  {}", dst_config);
    }

    #[cfg(not(feature = "hf-download"))]
    {
        let _ = args;
        eprintln!("Error: built without `hf-download` feature.");
        eprintln!("Rebuild with: cargo run --features hf-download --bin download_weights");
        std::process::exit(1);
    }

    Ok(())
}
