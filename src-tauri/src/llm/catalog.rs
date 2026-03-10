// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! LLM model catalog — loaded from the bundled `llm_catalog.json`.
//!
//! ## Source of truth
//!
//! `src-tauri/llm_catalog.json` is the **canonical** list of model families,
//! repos, quants, sizes and descriptions.  It is embedded at compile time via
//! `include_str!` and used in two ways:
//!
//! 1. **First run** – no `~/.skill/llm_catalog.json` exists yet.
//!    `LlmCatalog::load()` falls back to the bundled data directly.
//!
//! 2. **Subsequent runs** – persisted catalog exists (user may have models
//!    downloaded, custom `active_model`, etc.).  `load()` parses the persisted
//!    file and then **forward-merges** from the bundle:
//!    - New entries added to the bundle appear automatically.
//!    - Static metadata (description, tags, `recommended`, `advanced`) are
//!      refreshed from the bundle so edits propagate to existing users without
//!      losing their download state.
//!
//! To add a new model or change a description, **only edit `llm_catalog.json`**
//! — no Rust code changes are required.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

// ── Embedded catalog ──────────────────────────────────────────────────────────

/// The bundled default catalog, embedded at compile time.
const BUNDLED_CATALOG_JSON: &str = include_str!("../../llm_catalog.json");

// ── Constants ─────────────────────────────────────────────────────────────────

pub const CATALOG_FILE: &str = "llm_catalog.json";

// ── Per-file entry ────────────────────────────────────────────────────────────

/// Download / presence state for a single model file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    #[default]
    NotDownloaded,
    Downloading,
    Downloaded,
    Failed,
    Cancelled,
}

/// One entry in the catalog — a single GGUF file.
///
/// Fields in the first block come from `llm_catalog.json` (static knowledge).
/// Fields in the second block are runtime-only and never present in the
/// bundled JSON (they default to `None` / `NotDownloaded` / `0.0`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelEntry {
    // ── Static (from llm_catalog.json) ───────────────────────────────────────
    pub repo:        String,
    pub filename:    String,
    pub quant:       String,
    pub size_gb:     f32,
    pub description: String,
    pub family_id:   String,
    pub family_name: String,
    pub family_desc: String,
    /// e.g. `["chat","reasoning","small"]`
    pub tags:        Vec<String>,
    pub is_mmproj:   bool,
    pub recommended: bool,
    /// Hidden in simple view; shown under "Show all quants".
    pub advanced:    bool,

    // ── Runtime (persisted in skill_dir/llm_catalog.json) ────────────────────
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path:  Option<PathBuf>,
    #[serde(default)]
    pub state:       DownloadState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_msg:  Option<String>,
    #[serde(default)]
    pub progress:    f32,
}

impl LlmModelEntry {
    /// Resolve local path from the HF Hub cache — filesystem only, no network.
    pub fn resolve_cached(&self) -> Option<PathBuf> {
        use hf_hub::{Cache, Repo};
        let cache = Cache::from_env();
        let repo  = cache.repo(Repo::model(self.repo.clone()));
        repo.get(&self.filename)
    }
}

// ── Full catalog ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCatalog {
    pub entries:       Vec<LlmModelEntry>,
    #[serde(default)]
    pub active_model:  String,
    #[serde(default)]
    pub active_mmproj: String,
}

// ── Bundled-catalog helpers ───────────────────────────────────────────────────

/// Parse and return the bundled catalog.  Panics at startup (compile-time
/// guarantee) if `llm_catalog.json` contains invalid JSON.
fn bundled() -> LlmCatalog {
    serde_json::from_str(BUNDLED_CATALOG_JSON)
        .expect("src-tauri/llm_catalog.json is not valid JSON — fix it and recompile")
}


impl Default for LlmCatalog {
    /// Returns the bundled catalog with all states set to `NotDownloaded`.
    fn default() -> Self { bundled() }
}

// ── Persistence & merge ───────────────────────────────────────────────────────

impl LlmCatalog {
    /// Load the catalog for `skill_dir`.
    ///
    /// 1. Parse the bundled JSON as the authoritative list of known entries.
    /// 2. Try to read `skill_dir/llm_catalog.json` (persisted user state).
    /// 3. Forward-merge:
    ///    - Copy download state / local_path / progress from persisted → bundled.
    ///    - Append persisted entries that have no match in the bundle (custom
    ///      models the user added manually or via the file picker).
    /// 4. Probe the HF Hub cache for any entries not already `Downloaded`.
    pub fn load(skill_dir: &Path) -> Self {
        let bundle = bundled();

        let persisted: Option<LlmCatalog> = skill_dir
            .join(CATALOG_FILE)
            .pipe(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok());

        let mut cat = match persisted {
            None => bundle, // first run — use bundled directly
            Some(mut p) => {
                // Build a map from the persisted entries for fast lookup.
                let mut pmap: std::collections::HashMap<String, LlmModelEntry> =
                    p.entries.drain(..).map(|e| (e.filename.clone(), e)).collect();

                // Start from the bundle; apply persisted runtime state where available.
                let mut merged: Vec<LlmModelEntry> = bundle
                    .entries
                    .into_iter()
                    .map(|mut bundled_entry| {
                        if let Some(saved) = pmap.remove(&bundled_entry.filename) {
                            // Keep runtime fields from the persisted copy.
                            bundled_entry.local_path = saved.local_path;
                            bundled_entry.state      = saved.state;
                            bundled_entry.status_msg = saved.status_msg;
                            bundled_entry.progress   = saved.progress;
                        }
                        bundled_entry
                    })
                    .collect();

                // Append any leftover persisted entries (custom / manually-added).
                merged.extend(pmap.into_values());

                LlmCatalog {
                    entries:       merged,
                    active_model:  p.active_model,
                    active_mmproj: p.active_mmproj,
                }
            }
        };

        cat.refresh_cache();
        cat
    }

    /// Save the catalog (runtime state) to `skill_dir/llm_catalog.json`.
    pub fn save(&self, skill_dir: &Path) {
        let path = skill_dir.join(CATALOG_FILE);
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Probe the HF Hub disk cache and update `local_path` / `state` for
    /// every entry that is not currently downloading.  Zero network I/O.
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

    /// If `active_model` is empty, pick the first downloaded recommended model.
    pub fn auto_select(&mut self) {
        if !self.active_model.is_empty() { return; }
        if let Some(e) = self.entries.iter()
            .find(|e| !e.is_mmproj && e.recommended && e.state == DownloadState::Downloaded)
        {
            self.active_model = e.filename.clone();
        }
    }

    /// Local path of the active model if it is downloaded.
    pub fn active_model_path(&self) -> Option<PathBuf> {
        self.entries.iter()
            .find(|e| !e.is_mmproj && e.filename == self.active_model
                && e.state == DownloadState::Downloaded)
            .and_then(|e| e.local_path.clone())
    }

    /// Local path of the active mmproj if it is downloaded.
    pub fn active_mmproj_path(&self) -> Option<PathBuf> {
        if self.active_mmproj.is_empty() { return None; }
        self.entries.iter()
            .find(|e| e.is_mmproj && e.filename == self.active_mmproj
                && e.state == DownloadState::Downloaded)
            .and_then(|e| e.local_path.clone())
    }

    /// Find the best downloaded mmproj for the currently active model.
    ///
    /// Matches by repo (same HuggingFace repo as the active model entry).
    /// Preference order: recommended first, then by quant (BF16 > F16 > F32).
    /// Returns `None` if no compatible mmproj is downloaded.
    pub fn best_mmproj_for_active_model(&self) -> Option<&LlmModelEntry> {
        // Find the repo of the active model.
        let active_repo = self.entries.iter()
            .find(|e| !e.is_mmproj && e.filename == self.active_model)?
            .repo.as_str();

        fn quant_rank(quant: &str) -> u8 {
            match quant.to_uppercase().as_str() {
                "BF16" => 0,
                "F16"  => 1,
                _      => 2,  // F32 and others
            }
        }

        self.entries.iter()
            .filter(|e| e.is_mmproj
                && e.repo == active_repo
                && e.state == DownloadState::Downloaded)
            .min_by_key(|e| (!e.recommended as u8, quant_rank(&e.quant)))
    }

    /// If `autoload_mmproj` is requested and no mmproj is currently selected,
    /// pick the best available one for the active model and return its path.
    /// Does **not** mutate `active_mmproj` — the caller decides whether to
    /// persist the selection.
    pub fn resolve_mmproj_path(&self, autoload: bool) -> Option<PathBuf> {
        // Explicit selection always wins.
        if let path @ Some(_) = self.active_mmproj_path() {
            return path;
        }
        if autoload {
            self.best_mmproj_for_active_model()
                .and_then(|e| e.local_path.clone())
        } else {
            None
        }
    }
}

// ── Path extension helper (avoids a temporary binding) ───────────────────────

trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R where F: FnOnce(Self) -> R { f(self) }
}
impl<T> Pipe for T {}

// ── Shared download progress ──────────────────────────────────────────────────

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
/// in-place and checks `progress.cancelled` before I/O starts.
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

    let api  = Api::new().map_err(|e| format!("HF Hub init failed: {e}"))?;
    let repo = api.model(repo_id.to_string());

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
