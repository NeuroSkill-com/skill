// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! LLM model catalog — Qwen3.5-27B GGUF variants + mmproj projectors.
//!
//! The catalog is persisted as JSON at `~/.skill/llm_catalog.json`.  It tracks
//! which model files are available on HuggingFace, which are downloaded
//! locally, and the current download status for each.
//!
//! Static knowledge of the available files lives in [`KNOWN_MODELS`].
//! At runtime we merge this list with the user's persisted cache so that
//! previously-downloaded paths survive app restarts.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

// ── Constants ─────────────────────────────────────────────────────────────────

pub const CATALOG_FILE:  &str = "llm_catalog.json";
pub const QWEN_HF_REPO:  &str = "unsloth/Qwen3.5-27B-GGUF";

// ── Per-file entry ────────────────────────────────────────────────────────────

/// Download / presence state for a single model file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    /// File not yet downloaded; nothing in progress.
    NotDownloaded,
    /// Download is actively running.
    Downloading,
    /// File is present on disk.
    Downloaded,
    /// Last download attempt failed.
    Failed,
    /// Download was cancelled by the user.
    Cancelled,
}

impl Default for DownloadState { fn default() -> Self { Self::NotDownloaded } }

/// One entry in the model catalog — represents a single GGUF file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelEntry {
    /// HuggingFace repo id (e.g. `"unsloth/Qwen3.5-27B-GGUF"`).
    pub repo: String,

    /// Filename inside the repo (e.g. `"Qwen3.5-27B-UD-Q4_K_XL.gguf"`).
    pub filename: String,

    /// Human-readable quantisation label (e.g. `"Q4_K_XL"`).
    pub quant: String,

    /// Approximate size in GiB for display.
    pub size_gb: f32,

    /// Short description shown in the UI.
    pub description: String,

    /// Whether this is a multimodal projector (mmproj) file.
    pub is_mmproj: bool,

    /// Whether this is the recommended default model.
    pub recommended: bool,

    /// Absolute local path if the file is downloaded, `None` otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path: Option<PathBuf>,

    /// Current download / presence state.
    #[serde(default)]
    pub state: DownloadState,

    /// Human-readable status message (download progress, error detail, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_msg: Option<String>,

    /// Download progress 0.0 – 1.0 (only valid while `state == Downloading`).
    #[serde(default)]
    pub progress: f32,
}

impl LlmModelEntry {
    /// Return the full resolved local path from the HuggingFace Hub cache
    /// without downloading.  Uses the same cache layout as `hf-hub`.
    pub fn resolve_cached(&self) -> Option<PathBuf> {
        // Use the offline `hf_hub::Cache` API — pure filesystem, zero network.
        // `CacheRepo::get()` walks the local HF hub snapshot directories and
        // returns the symlink/blob path if the file is already downloaded.
        use hf_hub::{Cache, Repo};
        let cache = Cache::from_env();
        let repo  = cache.repo(Repo::model(self.repo.clone()));
        repo.get(&self.filename)
    }
}

// ── Full catalog ──────────────────────────────────────────────────────────────

/// The complete model catalog, persisted to `~/.skill/llm_catalog.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCatalog {
    /// All known model entries (main models + mmproj projectors).
    pub entries: Vec<LlmModelEntry>,

    /// Filename of the currently active LLM model (empty = none selected).
    #[serde(default)]
    pub active_model: String,

    /// Filename of the currently active mmproj file (empty = none selected).
    #[serde(default)]
    pub active_mmproj: String,
}

// ── Static knowledge: Qwen3.5-27B-GGUF ───────────────────────────────────────

/// All known GGUF files in `unsloth/Qwen3.5-27B-GGUF`.
///
/// Sizes are approximate (rounded to 0.1 GiB).
/// Updated whenever Unsloth publishes new quants.
fn known_models() -> Vec<LlmModelEntry> {
    use DownloadState::NotDownloaded as N;
    let r = QWEN_HF_REPO.to_string();

    vec![
        // ── Main model quantisations ──────────────────────────────────────────
        LlmModelEntry {
            repo: r.clone(), is_mmproj: false, state: N,
            filename:    "Qwen3.5-27B-Q2_K.gguf".into(),
            quant:       "Q2_K".into(),
            size_gb:     9.4,
            description: "Smallest — fits 12 GB VRAM; lowest quality".into(),
            recommended: false,
            local_path: None, status_msg: None, progress: 0.0,
        },
        LlmModelEntry {
            repo: r.clone(), is_mmproj: false, state: N,
            filename:    "Qwen3.5-27B-Q3_K_M.gguf".into(),
            quant:       "Q3_K_M".into(),
            size_gb:     11.6,
            description: "Good trade-off for 16 GB VRAM / 32 GB RAM".into(),
            recommended: false,
            local_path: None, status_msg: None, progress: 0.0,
        },
        LlmModelEntry {
            repo: r.clone(), is_mmproj: false, state: N,
            filename:    "Qwen3.5-27B-Q4_0.gguf".into(),
            quant:       "Q4_0".into(),
            size_gb:     14.5,
            description: "Default — best for GPU; fast, good quality".into(),
            recommended: true,
            local_path: None, status_msg: None, progress: 0.0,
        },
        LlmModelEntry {
            repo: r.clone(), is_mmproj: false, state: N,
            filename:    "Qwen3.5-27B-Q4_K_M.gguf".into(),
            quant:       "Q4_K_M".into(),
            size_gb:     15.4,
            description: "Slightly higher quality than Q4_0; same VRAM".into(),
            recommended: false,
            local_path: None, status_msg: None, progress: 0.0,
        },
        LlmModelEntry {
            repo: r.clone(), is_mmproj: false, state: N,
            filename:    "Qwen3.5-27B-Q6_K.gguf".into(),
            quant:       "Q6_K".into(),
            size_gb:     20.3,
            description: "Near-lossless; needs ≥ 24 GB VRAM / 48 GB RAM".into(),
            recommended: false,
            local_path: None, status_msg: None, progress: 0.0,
        },
        LlmModelEntry {
            repo: r.clone(), is_mmproj: false, state: N,
            filename:    "Qwen3.5-27B-Q8_0.gguf".into(),
            quant:       "Q8_0".into(),
            size_gb:     27.0,
            description: "Effectively lossless 8-bit; very large".into(),
            recommended: false,
            local_path: None, status_msg: None, progress: 0.0,
        },
        LlmModelEntry {
            repo: r.clone(), is_mmproj: false, state: N,
            filename:    "Qwen3.5-27B-F16.gguf".into(),
            quant:       "F16".into(),
            size_gb:     54.0,
            description: "Full FP16 — reference quality; needs ≥ 64 GB RAM".into(),
            recommended: false,
            local_path: None, status_msg: None, progress: 0.0,
        },
        // ── Multimodal projectors (mmproj) ────────────────────────────────────
        LlmModelEntry {
            repo: r.clone(), is_mmproj: true, state: N,
            filename:    "mmproj-BF16.gguf".into(),
            quant:       "BF16".into(),
            size_gb:     0.5,
            description: "Multimodal projector — BF16 (best quality, GPU recommended)".into(),
            recommended: true,
            local_path: None, status_msg: None, progress: 0.0,
        },
        LlmModelEntry {
            repo: r.clone(), is_mmproj: true, state: N,
            filename:    "mmproj-F16.gguf".into(),
            quant:       "F16".into(),
            size_gb:     0.5,
            description: "Multimodal projector — FP16".into(),
            recommended: false,
            local_path: None, status_msg: None, progress: 0.0,
        },
        LlmModelEntry {
            repo: r, is_mmproj: true, state: N,
            filename:    "mmproj-F32.gguf".into(),
            quant:       "F32".into(),
            size_gb:     0.9,
            description: "Multimodal projector — FP32 (largest; highest precision)".into(),
            recommended: false,
            local_path: None, status_msg: None, progress: 0.0,
        },
    ]
}

/// Filename of the model selected by default (Q4_0 — GPU-friendly).
pub const DEFAULT_MODEL: &str = "Qwen3.5-27B-Q4_0.gguf";

impl Default for LlmCatalog {
    fn default() -> Self {
        Self {
            entries:       known_models(),
            // Pre-select Q4_0 so the "Start" button is enabled as soon as
            // the model is downloaded — no manual selection required.
            active_model:  DEFAULT_MODEL.to_string(),
            active_mmproj: String::new(),
        }
    }
}

// ── Persistence ───────────────────────────────────────────────────────────────

impl LlmCatalog {
    /// Load the catalog from disk (or return the default if not present).
    ///
    /// After loading we merge with the static [`known_models`] list so newly
    /// added quants appear automatically on upgrade.
    pub fn load(skill_dir: &Path) -> Self {
        let path = skill_dir.join(CATALOG_FILE);
        let mut cat: Self = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        // Merge: add any known models not already in the persisted list.
        for known in known_models() {
            if !cat.entries.iter().any(|e| e.filename == known.filename) {
                cat.entries.push(known);
            }
        }

        // Refresh local_path / state by probing the HF cache.
        cat.refresh_cache();

        cat
    }

    /// Save the catalog to disk.
    pub fn save(&self, skill_dir: &Path) {
        let path = skill_dir.join(CATALOG_FILE);
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Update `local_path` and `state` for every entry by probing the HF
    /// Hub disk cache.  Does **not** trigger any network requests.
    pub fn refresh_cache(&mut self) {
        for entry in &mut self.entries {
            if entry.state == DownloadState::Downloading { continue; }
            entry.local_path = entry.resolve_cached();
            entry.state = if entry.local_path.is_some() {
                DownloadState::Downloaded
            } else {
                DownloadState::NotDownloaded
            };
        }
    }

    /// Set `active_model` to the recommended entry if it is downloaded and
    /// no explicit selection exists.
    pub fn auto_select(&mut self) {
        if !self.active_model.is_empty() { return; }
        if let Some(e) = self.entries.iter()
            .find(|e| !e.is_mmproj && e.recommended && e.state == DownloadState::Downloaded)
        {
            self.active_model = e.filename.clone();
        }
    }

    /// Return the `local_path` for the currently active model, if available.
    pub fn active_model_path(&self) -> Option<PathBuf> {
        self.entries.iter()
            .find(|e| !e.is_mmproj && e.filename == self.active_model
                && e.state == DownloadState::Downloaded)
            .and_then(|e| e.local_path.clone())
    }

    /// Return the `local_path` for the currently active mmproj, if available.
    pub fn active_mmproj_path(&self) -> Option<PathBuf> {
        if self.active_mmproj.is_empty() { return None; }
        self.entries.iter()
            .find(|e| e.is_mmproj && e.filename == self.active_mmproj
                && e.state == DownloadState::Downloaded)
            .and_then(|e| e.local_path.clone())
    }
}

// ── Shared download status ────────────────────────────────────────────────────

/// Per-file download status held in a shared `Arc<Mutex<>>` between the
/// blocking download thread and the Tauri command poll.
#[derive(Debug, Clone, Default)]
pub struct DownloadProgress {
    pub filename:   String,
    pub state:      DownloadState,
    pub status_msg: Option<String>,
    pub progress:   f32,
    pub cancelled:  bool,
}

/// Download a single GGUF file from HuggingFace Hub.
///
/// Runs synchronously — call from `spawn_blocking`.  Updates `progress`
/// in-place and checks `progress.cancelled` between steps.
pub fn download_file(
    repo_id:  &str,
    filename: &str,
    progress: &Arc<Mutex<DownloadProgress>>,
) -> Result<PathBuf, String> {
    use hf_hub::api::sync::Api;

    {
        let mut p = progress.lock().unwrap();
        p.state      = DownloadState::Downloading;
        p.status_msg = Some(format!("Connecting to HuggingFace ({repo_id})…"));
        p.progress   = 0.0;
        p.cancelled  = false;
    }

    let api = Api::new().map_err(|e| format!("HF Hub init failed: {e}"))?;
    let repo = api.model(repo_id.to_string());

    // Check cancellation before network I/O.
    if progress.lock().unwrap().cancelled {
        let mut p = progress.lock().unwrap();
        p.state      = DownloadState::Cancelled;
        p.status_msg = Some("Cancelled.".into());
        return Err("cancelled".into());
    }

    {
        let mut p = progress.lock().unwrap();
        p.status_msg = Some(format!("Downloading {filename}…"));
        p.progress   = 0.05;
    }

    let local_path = repo.get(filename)
        .map_err(|e| format!("Download failed: {e}"))?;

    {
        let mut p = progress.lock().unwrap();
        p.state      = DownloadState::Downloaded;
        p.status_msg = Some(format!("Downloaded → {}", local_path.display()));
        p.progress   = 1.0;
    }

    Ok(local_path)
}
