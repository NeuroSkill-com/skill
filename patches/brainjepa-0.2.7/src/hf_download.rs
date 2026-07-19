/// HuggingFace Hub weight resolution and download.
///
/// Resolves model weights in priority order:
/// 1. Explicit paths (--weights + --gradient)
/// 2. hf-hub download/cache (requires feature `hf-download`)
/// 3. Scan ~/.cache/huggingface/hub/ for existing snapshots
///
/// # Feature gate
/// The `download()` function requires `--features hf-download`.
/// Without it, only `scan_cache()` is available.
use std::path::{Path, PathBuf};

/// Default HuggingFace repo for Brain-JEPA weights.
pub const DEFAULT_REPO: &str = "eugenehp/BrainJEPA";

/// Files expected in the HuggingFace repo.
const WEIGHTS_FILE: &str = "brainjepa.safetensors";
const GRADIENT_FILE: &str = "gradient_mapping_450.csv";

/// Resolved weight paths.
pub struct ResolvedWeights {
    pub weights_path: PathBuf,
    pub gradient_path: PathBuf,
}

/// Scan the HuggingFace cache directory for an existing snapshot.
///
/// Looks in `~/.cache/huggingface/hub/models--{org}--{name}/snapshots/*/`
/// for the expected files.
pub fn scan_cache(repo: &str, hf_cache: Option<&Path>) -> Option<ResolvedWeights> {
    let cache_root = hf_cache
        .map(|p| p.to_path_buf())
        .or_else(|| dirs_fallback().map(|home| home.join(".cache/huggingface/hub")))?;

    let repo_dir_name = format!("models--{}", repo.replace('/', "--"));
    let snapshots_dir = cache_root.join(&repo_dir_name).join("snapshots");

    if !snapshots_dir.exists() {
        return None;
    }

    // Find the most recent snapshot containing our files
    let mut entries: Vec<_> = std::fs::read_dir(&snapshots_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.metadata().ok().and_then(|m| m.modified().ok())));

    for entry in entries {
        let dir = entry.path();
        let weights = dir.join(WEIGHTS_FILE);
        let gradient = dir.join(GRADIENT_FILE);
        if weights.exists() && gradient.exists() {
            return Some(ResolvedWeights {
                weights_path: weights,
                gradient_path: gradient,
            });
        }
    }

    None
}

/// Download weights from HuggingFace Hub.
///
/// Requires feature `hf-download`. Downloads to the standard HF cache
/// directory and returns paths to the cached files.
#[cfg(feature = "hf-download")]
pub fn download(repo: &str, hf_cache: Option<&Path>) -> anyhow::Result<ResolvedWeights> {
    use hf_hub::api::sync::ApiBuilder;

    let mut builder = ApiBuilder::new();
    if let Some(cache) = hf_cache {
        builder = builder.with_cache_dir(cache.to_path_buf());
    }
    let api = builder.build()?;
    let repo = api.model(repo.to_string());

    println!("Downloading {WEIGHTS_FILE} from {repo:?} ...");
    let weights_path = repo.get(WEIGHTS_FILE)?;

    println!("Downloading {GRADIENT_FILE} from {repo:?} ...");
    let gradient_path = repo.get(GRADIENT_FILE)?;

    Ok(ResolvedWeights {
        weights_path,
        gradient_path,
    })
}

#[cfg(not(feature = "hf-download"))]
pub fn download(_repo: &str, _hf_cache: Option<&Path>) -> anyhow::Result<ResolvedWeights> {
    anyhow::bail!(
        "HuggingFace download requires --features hf-download.\n\
         Alternatively, download manually from https://huggingface.co/{DEFAULT_REPO}"
    )
}

/// Resolve weights: try explicit paths, then cache, then download.
pub fn resolve(
    repo: &str,
    weights: Option<&str>,
    gradient: Option<&str>,
    hf_cache: Option<&Path>,
) -> anyhow::Result<ResolvedWeights> {
    // 1. Explicit paths
    if let (Some(w), Some(g)) = (weights, gradient) {
        return Ok(ResolvedWeights {
            weights_path: PathBuf::from(w),
            gradient_path: PathBuf::from(g),
        });
    }

    // 2. Scan HF cache
    if let Some(resolved) = scan_cache(repo, hf_cache) {
        println!("Found cached weights: {}", resolved.weights_path.display());
        return Ok(resolved);
    }

    // 3. Download
    download(repo, hf_cache)
}

/// Fallback home directory detection (no external crate needed).
fn dirs_fallback() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
