/// Download Brain-JEPA weights from HuggingFace.
///
/// ```text
/// cargo run --release --bin download_weights --features hf-download
/// ```
use anyhow::Result;
use brainjepa::hf_download::{download, DEFAULT_REPO};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(about = "Download Brain-JEPA weights from HuggingFace")]
struct Args {
    /// HuggingFace repo ID.
    #[arg(long, default_value = DEFAULT_REPO)]
    repo: String,

    /// Override the HuggingFace cache directory.
    #[arg(long)]
    cache_dir: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let resolved = download(&args.repo, args.cache_dir.as_deref())?;
    println!("{}", resolved.weights_path.display());
    println!("{}", resolved.gradient_path.display());
    Ok(())
}
