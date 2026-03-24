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
//!
//! ## Sub-modules
//!
//! - **types** — `LlmModelEntry`, `LlmCatalog`, `DownloadState`, `DownloadProgress`
//! - **persistence** — load / save / merge / cache refresh / active-model queries
//! - **memory** — `estimate_memory_gb`, `recommend_ctx_size`
//! - **download** — resumable HuggingFace downloader with multi-shard support

pub mod download;
pub mod memory;
pub mod persistence;
pub mod types;

// Re-export the public API so existing `catalog::*` imports keep working.
pub use download::{download_file, download_model};
pub use memory::{estimate_memory_gb, recommend_ctx_size};
pub use types::{
    parse_catalog_json, DownloadProgress, DownloadState, LlmCatalog, LlmCatalogLegacy, LlmCatalogNormalized, LlmFamily,
    LlmModelEntry, LlmModelSlim, CATALOG_FILE,
};
