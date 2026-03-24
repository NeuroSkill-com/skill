// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! LLM catalog data types — model entries, catalog structure, download state.
//!
//! ## On-disk format (normalized)
//!
//! The bundled `llm_catalog.json` and the persisted user catalog both use a
//! **normalized** representation where shared family metadata lives in a
//! `"families"` map and per-quant entries live in a slim `"models"` array.
//! This avoids duplicating `family_name`, `family_desc`, `repo`, `tags`,
//! `params_b`, and `max_context_length` across every quant.
//!
//! At load time the normalized form is **inflated** into the flat
//! `Vec<LlmModelEntry>` that the rest of the codebase (Rust + frontend)
//! expects.  At save time the flat entries are **deflated** back.
//!
//! ### Legacy format
//!
//! Old installs may still have a flat `"entries"` array in their persisted
//! `llm_catalog.json`.  The loader detects this automatically, converts to
//! the normalized representation in memory, and the next `save()` writes the
//! new format — seamless migration with zero user intervention.
//!
//! ### Future-proofing
//!
//! * **Adding a model at runtime** — push a new `LlmModelEntry` into
//!   `catalog.entries`; if the `family_id` doesn't exist yet, `deflate()`
//!   will synthesize a family from the entry's fields automatically.
//! * **Adding family-level fields** — add them to `LlmFamily` with
//!   `#[serde(default)]`; old catalogs that lack the field still parse.
//! * **Per-model overrides** — `LlmModelSlim` already supports `repo`
//!   override; add more `Option<T>` fields the same way.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// Re-export from skill-constants.
pub use skill_constants::LLM_CATALOG_FILE as CATALOG_FILE;

// ── Download state ───────────────────────────────────────────────────────────

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

// ── Normalized on-disk types ─────────────────────────────────────────────────

/// Family metadata — shared across all quants of the same model family.
///
/// Stored once in `"families": { "<id>": { … } }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmFamily {
    pub name: String,
    pub description: String,
    pub repo: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub is_mmproj: bool,
    #[serde(default)]
    pub params_b: f64,
    #[serde(default)]
    pub max_context_length: u32,
}

/// Slim per-quant model entry as stored on disk (normalized).
///
/// Fields that can be inherited from the parent [`LlmFamily`] are optional;
/// when absent the family value is used during inflation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmModelSlim {
    /// References a key in the `families` map.
    pub family: String,
    pub filename: String,
    pub quant: String,
    pub size_gb: f32,
    pub description: String,

    /// Override the family repo for this specific file (rare — e.g. mmproj
    /// hosted in a different repo than the main model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub recommended: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub advanced: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shard_files: Vec<String>,

    // ── Runtime (persisted in user catalog, absent in bundled) ────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "is_not_downloaded")]
    pub state: DownloadState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_msg: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub progress: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initiated_at_unix: Option<u64>,
}

fn is_false(v: &bool) -> bool {
    !v
}
fn is_not_downloaded(v: &DownloadState) -> bool {
    *v == DownloadState::NotDownloaded
}
fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

/// Normalized on-disk catalog (new format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCatalogNormalized {
    #[serde(default)]
    pub active_model: String,
    #[serde(default)]
    pub active_mmproj: String,
    pub families: HashMap<String, LlmFamily>,
    pub models: Vec<LlmModelSlim>,
}

/// Legacy flat on-disk catalog (old format, auto-migrated).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCatalogLegacy {
    pub entries: Vec<LlmModelEntry>,
    #[serde(default)]
    pub active_model: String,
    #[serde(default)]
    pub active_mmproj: String,
}

// ── Inflate / deflate ────────────────────────────────────────────────────────

impl LlmCatalogNormalized {
    /// Inflate normalized form into the flat `LlmCatalog`.
    ///
    /// Models whose `family` key doesn't match any family are silently
    /// dropped (should never happen with a well-formed catalog).
    pub fn inflate(self) -> LlmCatalog {
        let mut entries = Vec::with_capacity(self.models.len());
        for m in self.models {
            let Some(fam) = self.families.get(&m.family) else {
                log::warn!(
                    "catalog: model {} references unknown family '{}', skipping",
                    m.filename,
                    m.family,
                );
                continue;
            };
            entries.push(LlmModelEntry {
                repo: m.repo.unwrap_or_else(|| fam.repo.clone()),
                filename: m.filename,
                quant: m.quant,
                size_gb: m.size_gb,
                description: m.description,
                family_id: m.family,
                family_name: fam.name.clone(),
                family_desc: fam.description.clone(),
                tags: fam.tags.clone(),
                is_mmproj: fam.is_mmproj,
                recommended: m.recommended,
                advanced: m.advanced,
                params_b: fam.params_b,
                max_context_length: fam.max_context_length,
                shard_files: m.shard_files,
                local_path: m.local_path,
                state: m.state,
                status_msg: m.status_msg,
                progress: m.progress,
                initiated_at_unix: m.initiated_at_unix,
            });
        }
        LlmCatalog {
            entries,
            active_model: self.active_model,
            active_mmproj: self.active_mmproj,
        }
    }
}

impl LlmCatalog {
    /// Deflate the flat catalog into the normalized on-disk form.
    ///
    /// Families are reconstructed from entry fields.  If two entries share
    /// the same `family_id` but differ in family-level fields, the **first
    /// entry wins** (all entries within a family should agree).
    pub fn deflate(&self) -> LlmCatalogNormalized {
        let mut families: HashMap<String, LlmFamily> = HashMap::new();
        let mut models = Vec::with_capacity(self.entries.len());

        for e in &self.entries {
            // Build / update the family map.
            families.entry(e.family_id.clone()).or_insert_with(|| LlmFamily {
                name: e.family_name.clone(),
                description: e.family_desc.clone(),
                repo: e.repo.clone(),
                tags: e.tags.clone(),
                is_mmproj: e.is_mmproj,
                params_b: e.params_b,
                max_context_length: e.max_context_length,
            });

            let fam = &families[&e.family_id];
            let repo_override = if e.repo != fam.repo { Some(e.repo.clone()) } else { None };

            models.push(LlmModelSlim {
                family: e.family_id.clone(),
                filename: e.filename.clone(),
                quant: e.quant.clone(),
                size_gb: e.size_gb,
                description: e.description.clone(),
                repo: repo_override,
                recommended: e.recommended,
                advanced: e.advanced,
                shard_files: e.shard_files.clone(),
                local_path: e.local_path.clone(),
                state: e.state.clone(),
                status_msg: e.status_msg.clone(),
                progress: e.progress,
                initiated_at_unix: e.initiated_at_unix,
            });
        }

        LlmCatalogNormalized {
            active_model: self.active_model.clone(),
            active_mmproj: self.active_mmproj.clone(),
            families,
            models,
        }
    }
}

impl LlmCatalogLegacy {
    /// Convert a legacy flat catalog into the in-memory flat form.
    /// (Trivial — same shape, just rename `entries`.)
    pub fn into_catalog(self) -> LlmCatalog {
        LlmCatalog {
            entries: self.entries,
            active_model: self.active_model,
            active_mmproj: self.active_mmproj,
        }
    }
}

// ── In-memory flat types (public API — unchanged from before) ────────────────

/// One entry in the catalog — a single GGUF file (or a set of split shards).
///
/// This is the **in-memory** representation used by the rest of the codebase.
/// It is fully denormalized: every entry carries its own copy of the family
/// fields.  The on-disk format is normalized; see [`LlmCatalogNormalized`].
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

/// The full model catalog (in-memory, flat/denormalized).
///
/// Serialized to the frontend as-is.  Persisted to disk in the
/// [`LlmCatalogNormalized`] form via [`LlmCatalog::deflate()`].
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

// ── Parse helper ─────────────────────────────────────────────────────────────

/// Parse a JSON string that may be either the new normalized format or the
/// legacy flat format.  Returns the flat `LlmCatalog` either way.
///
/// The heuristic is simple: if the top-level object has a `"families"` key
/// it's the new format; if it has `"entries"` it's legacy.
pub fn parse_catalog_json(json: &str) -> Result<LlmCatalog, serde_json::Error> {
    // Try normalized first (cheaper check: the key is distinctive).
    if json.contains("\"families\"") {
        if let Ok(norm) = serde_json::from_str::<LlmCatalogNormalized>(json) {
            return Ok(norm.inflate());
        }
    }
    // Fall back to legacy.
    let legacy: LlmCatalogLegacy = serde_json::from_str(json)?;
    Ok(legacy.into_catalog())
}

// ── Tests ────────────────────────────────────────────────────────────────────

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

    fn mk_catalog() -> LlmCatalog {
        LlmCatalog {
            entries: vec![
                LlmModelEntry {
                    repo: "acme/Model-GGUF".into(),
                    filename: "Model-Q4_K_M.gguf".into(),
                    quant: "Q4_K_M".into(),
                    size_gb: 4.5,
                    description: "Recommended".into(),
                    family_id: "model-7b".into(),
                    family_name: "Model 7B".into(),
                    family_desc: "A great model.".into(),
                    tags: vec!["chat".into(), "reasoning".into()],
                    is_mmproj: false,
                    recommended: true,
                    advanced: false,
                    params_b: 7.0,
                    max_context_length: 32768,
                    shard_files: vec![],
                    local_path: None,
                    state: DownloadState::Downloaded,
                    status_msg: None,
                    progress: 0.0,
                    initiated_at_unix: None,
                },
                LlmModelEntry {
                    repo: "acme/Model-GGUF".into(),
                    filename: "Model-Q2_K.gguf".into(),
                    quant: "Q2_K".into(),
                    size_gb: 2.8,
                    description: "Smallest".into(),
                    family_id: "model-7b".into(),
                    family_name: "Model 7B".into(),
                    family_desc: "A great model.".into(),
                    tags: vec!["chat".into(), "reasoning".into()],
                    is_mmproj: false,
                    recommended: false,
                    advanced: true,
                    params_b: 7.0,
                    max_context_length: 32768,
                    shard_files: vec![],
                    local_path: None,
                    state: DownloadState::NotDownloaded,
                    status_msg: None,
                    progress: 0.0,
                    initiated_at_unix: None,
                },
                LlmModelEntry {
                    repo: "other/Vision-GGUF".into(),
                    filename: "Vision-mmproj-F16.gguf".into(),
                    quant: "F16".into(),
                    size_gb: 1.2,
                    description: "Vision projector".into(),
                    family_id: "vision-vl".into(),
                    family_name: "Vision VL".into(),
                    family_desc: "Vision model.".into(),
                    tags: vec!["vision".into()],
                    is_mmproj: true,
                    recommended: true,
                    advanced: false,
                    params_b: 0.6,
                    max_context_length: 8192,
                    shard_files: vec![],
                    local_path: None,
                    state: DownloadState::NotDownloaded,
                    status_msg: None,
                    progress: 0.0,
                    initiated_at_unix: None,
                },
            ],
            active_model: "Model-Q4_K_M.gguf".into(),
            active_mmproj: String::new(),
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

    // ── Deflate / inflate round-trip ─────────────────────────────────────

    #[test]
    fn deflate_creates_correct_families() {
        let cat = mk_catalog();
        let norm = cat.deflate();

        assert_eq!(norm.families.len(), 2);
        assert!(norm.families.contains_key("model-7b"));
        assert!(norm.families.contains_key("vision-vl"));

        let fam = &norm.families["model-7b"];
        assert_eq!(fam.name, "Model 7B");
        assert_eq!(fam.repo, "acme/Model-GGUF");
        assert_eq!(fam.params_b, 7.0);
        assert!(!fam.is_mmproj);

        let vis = &norm.families["vision-vl"];
        assert!(vis.is_mmproj);
    }

    #[test]
    fn deflate_models_omit_default_fields() {
        let cat = mk_catalog();
        let norm = cat.deflate();

        // The Q2_K entry should have advanced=true, no repo override.
        let q2 = norm.models.iter().find(|m| m.filename == "Model-Q2_K.gguf").unwrap();
        assert!(q2.advanced);
        assert!(!q2.recommended);
        assert!(q2.repo.is_none()); // same as family repo

        // Serialize and check that skipped fields are absent.
        let json = serde_json::to_string(q2).unwrap();
        assert!(!json.contains("\"repo\""));
        assert!(!json.contains("\"shard_files\""));
        assert!(!json.contains("\"local_path\""));
        assert!(!json.contains("\"state\""));
        assert!(!json.contains("\"progress\""));
    }

    #[test]
    fn deflate_inflate_roundtrip() {
        let original = mk_catalog();
        let norm = original.deflate();
        let restored = norm.inflate();

        assert_eq!(restored.active_model, original.active_model);
        assert_eq!(restored.active_mmproj, original.active_mmproj);
        assert_eq!(restored.entries.len(), original.entries.len());

        for (orig, rest) in original.entries.iter().zip(restored.entries.iter()) {
            assert_eq!(orig.filename, rest.filename);
            assert_eq!(orig.repo, rest.repo);
            assert_eq!(orig.family_id, rest.family_id);
            assert_eq!(orig.family_name, rest.family_name);
            assert_eq!(orig.family_desc, rest.family_desc);
            assert_eq!(orig.quant, rest.quant);
            assert_eq!(orig.size_gb, rest.size_gb);
            assert_eq!(orig.tags, rest.tags);
            assert_eq!(orig.is_mmproj, rest.is_mmproj);
            assert_eq!(orig.recommended, rest.recommended);
            assert_eq!(orig.advanced, rest.advanced);
            assert_eq!(orig.params_b, rest.params_b);
            assert_eq!(orig.max_context_length, rest.max_context_length);
            assert_eq!(orig.state, rest.state);
        }
    }

    #[test]
    fn deflate_preserves_repo_override() {
        let mut cat = mk_catalog();
        // Give one entry a different repo than its family.
        cat.entries[1].repo = "fork/Model-GGUF".into();
        let norm = cat.deflate();

        let q2 = norm.models.iter().find(|m| m.filename == "Model-Q2_K.gguf").unwrap();
        assert_eq!(q2.repo.as_deref(), Some("fork/Model-GGUF"));

        // Round-trip preserves it.
        let restored = norm.inflate();
        assert_eq!(restored.entries[1].repo, "fork/Model-GGUF");
    }

    #[test]
    fn deflate_preserves_runtime_state() {
        let mut cat = mk_catalog();
        cat.entries[0].local_path = Some(PathBuf::from("/tmp/model.gguf"));
        cat.entries[0].state = DownloadState::Downloaded;
        cat.entries[0].progress = 1.0;

        let norm = cat.deflate();
        let m = norm.models.iter().find(|m| m.filename == "Model-Q4_K_M.gguf").unwrap();
        assert_eq!(m.state, DownloadState::Downloaded);
        assert_eq!(m.progress, 1.0);
        assert!(m.local_path.is_some());

        let restored = norm.inflate();
        assert_eq!(restored.entries[0].state, DownloadState::Downloaded);
        assert_eq!(restored.entries[0].local_path, Some(PathBuf::from("/tmp/model.gguf")));
    }

    // ── parse_catalog_json ───────────────────────────────────────────────

    #[test]
    fn parse_normalized_json() {
        let cat = mk_catalog();
        let norm = cat.deflate();
        let json = serde_json::to_string_pretty(&norm).unwrap();

        let parsed = parse_catalog_json(&json).unwrap();
        assert_eq!(parsed.entries.len(), 3);
        assert_eq!(parsed.active_model, "Model-Q4_K_M.gguf");
    }

    #[test]
    fn parse_legacy_json() {
        let cat = mk_catalog();
        // Serialize as legacy format (flat entries).
        let legacy = LlmCatalogLegacy {
            entries: cat.entries.clone(),
            active_model: cat.active_model.clone(),
            active_mmproj: cat.active_mmproj.clone(),
        };
        let json = serde_json::to_string_pretty(&legacy).unwrap();
        assert!(json.contains("\"entries\""));
        assert!(!json.contains("\"families\""));

        let parsed = parse_catalog_json(&json).unwrap();
        assert_eq!(parsed.entries.len(), 3);
        assert_eq!(parsed.active_model, "Model-Q4_K_M.gguf");
        assert_eq!(parsed.entries[0].family_name, "Model 7B");
    }

    #[test]
    fn parse_legacy_then_deflate_roundtrips() {
        let cat = mk_catalog();
        let legacy_json = serde_json::to_string_pretty(&LlmCatalogLegacy {
            entries: cat.entries.clone(),
            active_model: cat.active_model.clone(),
            active_mmproj: cat.active_mmproj.clone(),
        })
        .unwrap();

        // Simulate: load legacy → deflate → save normalized → re-load.
        let loaded = parse_catalog_json(&legacy_json).unwrap();
        let norm_json = serde_json::to_string_pretty(&loaded.deflate()).unwrap();
        let reloaded = parse_catalog_json(&norm_json).unwrap();

        assert_eq!(reloaded.entries.len(), cat.entries.len());
        for (a, b) in reloaded.entries.iter().zip(cat.entries.iter()) {
            assert_eq!(a.filename, b.filename);
            assert_eq!(a.family_id, b.family_id);
            assert_eq!(a.family_name, b.family_name);
        }
    }
}
