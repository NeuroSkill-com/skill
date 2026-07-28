// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Discover GGUF **and** mlx-community safetensors models that **other local
//! apps** already downloaded and turn them into catalog entries the engine can
//! load directly.
//!
//! `rlx-models` ships a filesystem cache scanner
//! (`rlx_models::weights_discover`) that walks the download directories of
//! LM Studio, Ollama, Lemonade, the HuggingFace hub, MLX and vLLM — **no
//! network I/O**. This module maps each discovered weight into an
//! [`LlmModelEntry`] with `local_path` set and `state = Downloaded`.
//!
//! ## Scope
//!
//! - [`DiscoveredFormat::Gguf`] — RLX GGUF runners (`RlxTextRunner`).
//! - [`DiscoveredFormat::Safetensors`] — HF snapshot **directories** (path is
//!   the snapshot root). mlx-community quantized packs (`config.json`
//!   `quantization` block) load via `Qwen3Runner::from_mlx_packed`; plain
//!   safetensors dirs fall through to `auto_runner`.
//!
//! Ollama blobs are extension-less but the scanner tags them `Gguf` via
//! magic-byte sniffing, so they are included.
//!
//! ## Overlay semantics
//!
//! Discovered entries are a **live overlay**, recomputed on every catalog load
//! / refresh and never persisted to `llm_catalog.json` (see
//! [`super::LlmCatalog::save`]). They are tagged with [`DISCOVERED_TAG`] so
//! [`LlmModelEntry::is_discovered`] can identify them and
//! [`super::LlmCatalog::refresh_cache`] can skip the HF-cache path clobber.

use super::types::LlmModelEntry;
use crate::config::ModelDiscoveryConfig;

/// Tag applied to every discovered (external-app) catalog entry.
pub const DISCOVERED_TAG: &str = "discovered";

/// Discover local models downloaded by other apps (LM Studio, Ollama,
/// Lemonade, HF / mlx-community cache, …). Returns freshly-built
/// [`LlmModelEntry`] values with `local_path` set and `state = Downloaded`.
///
/// Honors [`ModelDiscoveryConfig`]: returns empty when `!cfg.enabled`, filters
/// to `cfg.sources` when non-empty, and also scans `cfg.extra_dirs`. The scan
/// is fully local (no network I/O).
///
/// Returns an empty list unless the `llm-model-discovery` feature is enabled.
#[cfg(feature = "llm-model-discovery")]
pub fn discover_local_models(cfg: &ModelDiscoveryConfig) -> Vec<LlmModelEntry> {
    use rlx_models::{DiscoverOpts, DiscoveredFormat, WeightSourceKind};

    if !cfg.enabled {
        return Vec::new();
    }

    let mut opts = DiscoverOpts::default();

    if !cfg.sources.is_empty() {
        let kinds: Vec<WeightSourceKind> = cfg
            .sources
            .iter()
            .filter_map(|s| WeightSourceKind::parse(s).ok())
            .collect();
        if !kinds.is_empty() {
            opts = opts.with_sources(kinds);
        }
    }
    if !cfg.extra_dirs.is_empty() {
        opts = opts.with_extra_roots(cfg.extra_dirs.clone());
    }

    let hits = match rlx_models::scan_weights(&opts) {
        Ok(h) => h,
        Err(e) => {
            log::warn!("discover_local_models: scan failed: {e}");
            return Vec::new();
        }
    };

    hits.into_iter()
        .filter(|w| matches!(w.format, DiscoveredFormat::Gguf | DiscoveredFormat::Safetensors))
        .map(discovered_to_entry)
        .collect()
}

#[cfg(not(feature = "llm-model-discovery"))]
pub fn discover_local_models(_cfg: &ModelDiscoveryConfig) -> Vec<LlmModelEntry> {
    Vec::new()
}

/// Human-facing label for a raw source token (`lmstudio` → `LM Studio`).
pub fn source_label(source: &str) -> &'static str {
    match source {
        "lmstudio" => "LM Studio",
        "ollama" => "Ollama",
        "lemonade" => "Lemonade",
        "hf" => "HuggingFace",
        "mlx" => "MLX",
        "vllm" => "vLLM",
        "rlx" => "RLX",
        _ => "Local",
    }
}

/// Map one scanned weight into a catalog entry. Feature-gated because the
/// `DiscoveredWeight` type comes from `rlx-models`' `weights_discover` module.
#[cfg(feature = "llm-model-discovery")]
fn discovered_to_entry(w: rlx_models::DiscoveredWeight) -> LlmModelEntry {
    use super::types::DownloadState;
    use rlx_models::DiscoveredFormat;

    let source = w
        .sources
        .iter()
        .copied()
        .min_by_key(|s| s.resolve_priority())
        .map(|s| s.as_str())
        .unwrap_or("local")
        .to_string();

    let is_dir = w.path.is_dir();
    let file_name = if is_dir {
        // Snapshot dirs (mlx-community / safetensors): use config.json as the
        // catalog key so resolve_cached / UI have a stable primary filename.
        "config.json".to_string()
    } else {
        w.path
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| w.display_name.clone())
    };

    let quant = w.quant_hint.clone().unwrap_or_else(|| {
        if matches!(w.format, DiscoveredFormat::Safetensors) {
            infer_mlx_quant(&w.path).unwrap_or_else(|| "safetensors".to_string())
        } else {
            infer_quant(&file_name)
        }
    });

    let size_gb = w.size_bytes.map(|b| b as f32 / 1e9).unwrap_or(0.0);

    let is_mmproj = file_name.to_ascii_lowercase().contains("mmproj");

    let label = source_label(&source);
    let family_slug = slug(&w.display_name);

    let mut tags = vec![DISCOVERED_TAG.to_string(), source.clone()];
    if (source == "mlx" || (matches!(w.format, DiscoveredFormat::Safetensors) && is_dir))
        && !tags.iter().any(|t| t == "mlx")
    {
        tags.push("mlx".to_string());
    }

    LlmModelEntry {
        repo: format!("local/{source}"),
        filename: file_name,
        remote_filename: None,
        quant,
        size_gb,
        description: format!("Found in {label}"),
        family_id: format!("local-{source}-{family_slug}"),
        family_name: w.display_name,
        family_desc: format!("Discovered on this machine ({label})."),
        tags,
        is_mmproj,
        mtp: false,
        recommended: false,
        advanced: false,
        params_b: 0.0,
        max_context_length: 0,
        shard_files: Vec::new(),
        local_path: Some(w.path),
        state: DownloadState::Downloaded,
        status_msg: None,
        progress: 0.0,
        initiated_at_unix: None,
    }
}

/// Read `config.json` `quantization.bits` when present (mlx-community packs).
#[cfg(feature = "llm-model-discovery")]
fn infer_mlx_quant(path: &std::path::Path) -> Option<String> {
    let cfg = if path.is_dir() {
        path.join("config.json")
    } else {
        path.parent()?.join("config.json")
    };
    let bytes = std::fs::read(cfg).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let bits = v.get("quantization")?.get("bits")?.as_u64()?;
    Some(format!("{bits}bit"))
}

/// Best-effort quant guess from a filename when the scanner has no hint.
#[cfg(any(feature = "llm-model-discovery", test))]
fn infer_quant(filename: &str) -> String {
    let upper = filename.to_ascii_uppercase();
    const KNOWN: &[&str] = &[
        "Q4_K_M", "Q4_K_S", "Q4_K_L", "Q4_0", "Q4_1", "Q5_K_M", "Q5_K_S", "Q5_K_L", "Q6_K", "Q8_0", "Q3_K_M", "Q3_K_L",
        "Q3_K_S", "Q2_K", "IQ4_XS", "IQ4_NL", "IQ3_M", "IQ3_XS", "IQ2_M", "BF16", "F16", "F32",
    ];
    for q in KNOWN {
        if upper.contains(q) {
            return (*q).to_string();
        }
    }
    "GGUF".to_string()
}

/// Slugify a display name for use in a `family_id` (lowercase, `[a-z0-9-]`).
#[cfg(any(feature = "llm-model-discovery", test))]
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("model");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_normalizes() {
        assert_eq!(slug("Qwen3 0.6B (Instruct)"), "qwen3-0-6b-instruct");
        assert_eq!(slug("  "), "model");
        assert_eq!(slug("Model__Name"), "model-name");
    }

    #[test]
    fn infer_quant_from_name() {
        assert_eq!(infer_quant("qwen3-0.6b-Q4_K_M.gguf"), "Q4_K_M");
        assert_eq!(infer_quant("model-f16.gguf"), "F16");
        assert_eq!(infer_quant("mystery.gguf"), "GGUF");
    }

    #[test]
    fn no_backend_returns_empty() {
        let _ = discover_local_models(&ModelDiscoveryConfig::default());
    }

    #[test]
    fn disabled_config_yields_nothing() {
        let cfg = ModelDiscoveryConfig {
            enabled: false,
            sources: vec![],
            extra_dirs: vec![],
        };
        assert!(discover_local_models(&cfg).is_empty());
    }
}
