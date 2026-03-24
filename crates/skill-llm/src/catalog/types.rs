// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! LLM catalog data types — model entries, catalog structure, download state.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Re-export from skill-constants.
pub use skill_constants::LLM_CATALOG_FILE as CATALOG_FILE;

/// Download / presence state for a single model file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    #[default]
    NotDownloaded,
    Downloading,
    Paused,
    Downloaded,
    Failed,
    Cancelled,
}

/// One entry in the catalog — a single GGUF file (or a set of split shards).
///
/// Fields in the first block come from `llm_catalog.json` (static knowledge).
/// Fields in the second block are runtime-only and never present in the
/// bundled JSON (they default to `None` / `NotDownloaded` / `0.0`).
///
/// ## Split / sharded GGUFs
///
/// When a model is too large for a single file, repos split it into numbered
/// shards (e.g. `Model-Q4_K_M-00001-of-00004.gguf`).  llama.cpp loads them
/// automatically when given the path to the **first** shard.
///
/// For split models, `filename` is the **first shard** (the one passed to
/// llama.cpp) and `shard_files` lists **all shards in order** (including the
/// first).  `size_gb` is the **total** across all shards.
///
/// Single-file models have `shard_files` empty (the default).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelEntry {
    // ── Static (from llm_catalog.json) ───────────────────────────────────────
    pub repo: String,
    /// Primary filename — for single-file models this is the only GGUF file.
    /// For split models this is the **first shard** (passed to llama.cpp).
    pub filename: String,
    pub quant: String,
    /// Total size across all shard files (GB).
    pub size_gb: f32,
    pub description: String,
    pub family_id: String,
    pub family_name: String,
    pub family_desc: String,
    /// e.g. `["chat","reasoning","small"]`
    pub tags: Vec<String>,
    pub is_mmproj: bool,
    pub recommended: bool,
    /// Hidden in simple view; shown under "Show all quants".
    pub advanced: bool,
    /// Model parameter count in billions (e.g. 7.0 for a 7B model).
    /// Used together with `max_context_length` to estimate memory needs and
    /// recommend a context size that fits the user's hardware.
    #[serde(default)]
    pub params_b: f64,
    /// Maximum context length the model was trained on (in tokens).
    /// The runtime context size is capped to this value.
    #[serde(default)]
    pub max_context_length: u32,
    /// Ordered list of **all** shard filenames for split GGUFs.
    /// Empty for single-file models.  When non-empty, `filename` must equal
    /// `shard_files[0]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shard_files: Vec<String>,

    // ── Runtime (persisted in skill_dir/llm_catalog.json) ────────────────────
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path: Option<PathBuf>,
    #[serde(default)]
    pub state: DownloadState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_msg: Option<String>,
    #[serde(default)]
    pub progress: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initiated_at_unix: Option<u64>,
}

impl LlmModelEntry {
    /// Whether this entry represents a split (sharded) GGUF model.
    pub fn is_split(&self) -> bool {
        self.shard_files.len() > 1
    }

    /// Total number of shards (1 for single-file models).
    pub fn shard_count(&self) -> usize {
        if self.shard_files.is_empty() {
            1
        } else {
            self.shard_files.len()
        }
    }

    /// Iterator over all filenames that need to be downloaded / present.
    /// For single-file models this yields just `filename`.
    pub fn all_filenames(&self) -> impl Iterator<Item = &str> {
        let single = std::iter::once(self.filename.as_str());
        let shards = self.shard_files.iter().map(String::as_str);
        // When shard_files is non-empty use it; otherwise fall back to filename.
        if self.shard_files.is_empty() {
            either::Either::Left(single)
        } else {
            either::Either::Right(shards)
        }
    }

    /// Resolve local path of the **first shard** from the HF Hub cache —
    /// filesystem only, no network.
    ///
    /// For split models, returns `Some` only when **all** shards are present.
    pub fn resolve_cached(&self) -> Option<PathBuf> {
        use hf_hub::{Cache, Repo};
        let cache = Cache::from_env();
        let repo = cache.repo(Repo::model(self.repo.clone()));

        let first = repo.get(&self.filename)?;

        // For split models, verify every shard is present.
        if self.is_split() {
            for name in self.shard_files.iter().skip(1) {
                repo.get(name)?;
            }
        }

        Some(first)
    }

    /// Resolve the local path of every shard that is already cached.
    /// Returns `(cached_paths, total_shards)`.
    pub fn resolve_cached_shards(&self) -> (Vec<PathBuf>, usize) {
        use hf_hub::{Cache, Repo};
        let cache = Cache::from_env();
        let repo = cache.repo(Repo::model(self.repo.clone()));
        let mut paths = Vec::new();
        let names: Vec<&str> = self.all_filenames().collect();
        for name in &names {
            if let Some(p) = repo.get(name) {
                paths.push(p);
            }
        }
        (paths, names.len())
    }
}

/// The full model catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCatalog {
    pub entries: Vec<LlmModelEntry>,
    #[serde(default)]
    pub active_model: String,
    #[serde(default)]
    pub active_mmproj: String,
}

/// Shared download progress state.
#[derive(Debug, Clone, Default)]
pub struct DownloadProgress {
    pub filename: String,
    pub state: DownloadState,
    pub status_msg: Option<String>,
    pub progress: f32,
    pub cancelled: bool,
    pub pause_requested: bool,
    /// 1-based index of the shard currently being downloaded (0 = single file).
    pub current_shard: u16,
    /// Total number of shards (0 or 1 = single file).
    pub total_shards: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_entry(filename: &str, shards: &[&str]) -> LlmModelEntry {
        LlmModelEntry {
            repo: "test/repo".into(),
            filename: filename.into(),
            quant: "Q4_K_M".into(),
            size_gb: 2.0,
            description: String::new(),
            family_id: "test".into(),
            family_name: "Test".into(),
            family_desc: String::new(),
            tags: vec![],
            params_b: 4.0,
            max_context_length: 4096,
            is_mmproj: false,
            recommended: false,
            advanced: false,
            shard_files: shards.iter().map(|s| s.to_string()).collect(),
            local_path: None,
            state: DownloadState::NotDownloaded,
            status_msg: None,
            progress: 0.0,
            initiated_at_unix: None,
        }
    }

    #[test]
    fn single_file_is_not_split() {
        let e = mk_entry("model.gguf", &[]);
        assert!(!e.is_split());
        assert_eq!(e.shard_count(), 1);
    }

    #[test]
    fn multi_shard_is_split() {
        let e = mk_entry("model-00001.gguf", &["model-00001.gguf", "model-00002.gguf"]);
        assert!(e.is_split());
        assert_eq!(e.shard_count(), 2);
    }

    #[test]
    fn all_filenames_single() {
        let e = mk_entry("model.gguf", &[]);
        let names: Vec<&str> = e.all_filenames().collect();
        assert_eq!(names, vec!["model.gguf"]);
    }

    #[test]
    fn all_filenames_sharded() {
        let e = mk_entry("a-00001.gguf", &["a-00001.gguf", "a-00002.gguf", "a-00003.gguf"]);
        let names: Vec<&str> = e.all_filenames().collect();
        assert_eq!(names, vec!["a-00001.gguf", "a-00002.gguf", "a-00003.gguf"]);
    }

    #[test]
    fn download_state_default_is_not_downloaded() {
        assert_eq!(DownloadState::default(), DownloadState::NotDownloaded);
    }

    #[test]
    fn download_state_serde_roundtrip() {
        let states = vec![
            DownloadState::NotDownloaded,
            DownloadState::Downloading,
            DownloadState::Paused,
            DownloadState::Downloaded,
            DownloadState::Failed,
            DownloadState::Cancelled,
        ];
        for s in states {
            let json = serde_json::to_string(&s).unwrap();
            let parsed: DownloadState = serde_json::from_str(&json).unwrap();
            assert_eq!(s, parsed);
        }
    }
}
