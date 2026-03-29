// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Catalog persistence — load / save / merge / cache refresh.
//!
//! ## Format migration
//!
//! The persisted `llm_catalog.json` in `skill_dir` may be either:
//!
//! * **Legacy** — flat `"entries"` array (pre-refactor installs).
//! * **Normalized** — `"families"` map + slim `"models"` array (current).
//!
//! [`parse_catalog_json`] handles both transparently.  [`LlmCatalog::save`]
//! always writes the normalized form, so legacy files are auto-migrated on
//! the next save.

use super::types::*;
use std::path::{Path, PathBuf};

/// The bundled default catalog, embedded at compile time.
const BUNDLED_CATALOG_JSON: &str = include_str!("../../../../src-tauri/llm_catalog.json");

/// Parse and return the bundled catalog.  Panics at startup (compile-time
/// guarantee) if `llm_catalog.json` contains invalid JSON.
fn bundled() -> LlmCatalog {
    #[allow(clippy::expect_used)]
    parse_catalog_json(BUNDLED_CATALOG_JSON)
        .expect("src-tauri/llm_catalog.json is not valid JSON — fix it and recompile")
}

impl Default for LlmCatalog {
    /// Returns the bundled catalog with all states set to `NotDownloaded`.
    fn default() -> Self {
        bundled()
    }
}

/// Extension trait for pipe syntax (avoids a temporary binding).
trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}
impl<T> Pipe for T {}

impl LlmCatalog {
    /// Load the catalog for `skill_dir`.
    ///
    /// 1. Parse the bundled JSON as the authoritative list of known entries.
    /// 2. Try to read `skill_dir/llm_catalog.json` (persisted user state).
    ///    Accepts **both** the legacy flat format and the new normalized
    ///    format — old installs are migrated transparently.
    /// 3. Forward-merge:
    ///    - Copy download state / local_path / progress from persisted -> bundled.
    ///    - Append persisted entries that have no match in the bundle (custom
    ///      models the user added manually or via the file picker).
    /// 4. Probe the HF Hub cache for any entries not already `Downloaded`.
    pub fn load(skill_dir: &Path) -> Self {
        let bundle = bundled();

        let persisted: Option<LlmCatalog> = skill_dir
            .join(CATALOG_FILE)
            .pipe(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| parse_catalog_json(&s).ok());

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
                            bundled_entry.state = saved.state;
                            bundled_entry.status_msg = saved.status_msg;
                            bundled_entry.progress = saved.progress;
                        }
                        bundled_entry
                    })
                    .collect();

                // Append any leftover persisted entries (custom / manually-added).
                merged.extend(pmap.into_values());

                LlmCatalog {
                    entries: merged,
                    active_model: p.active_model,
                    active_mmproj: p.active_mmproj,
                }
            }
        };

        cat.refresh_cache();
        cat
    }

    /// Save the catalog (runtime state) to `skill_dir/llm_catalog.json`.
    ///
    /// Always writes the **normalized** format.  Any legacy flat file is
    /// replaced, completing the migration.
    pub fn save(&self, skill_dir: &Path) {
        let path = skill_dir.join(CATALOG_FILE);
        let norm = self.deflate();
        if let Ok(json) = serde_json::to_string_pretty(&norm) {
            let _ = std::fs::write(path, json);
        }
    }

    /// Probe the HF Hub disk cache and update `local_path` / `state` for
    /// every entry that is not currently downloading.  Zero network I/O.
    pub fn refresh_cache(&mut self) {
        for entry in &mut self.entries {
            if entry.state == DownloadState::Downloading {
                continue;
            }
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
        if !self.active_model.is_empty() {
            return;
        }
        if let Some(e) = self
            .entries
            .iter()
            .find(|e| !e.is_mmproj() && e.recommended && e.state == DownloadState::Downloaded)
        {
            self.active_model = e.filename.clone();
        }
    }

    /// Active text model entry, regardless of download state.
    pub fn active_model_entry(&self) -> Option<&LlmModelEntry> {
        if self.active_model.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .find(|e| !e.is_mmproj() && e.filename == self.active_model)
    }

    /// Active mmproj entry, regardless of download state.
    pub fn active_mmproj_entry(&self) -> Option<&LlmModelEntry> {
        if self.active_mmproj.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .find(|e| e.is_mmproj() && e.filename == self.active_mmproj)
    }

    /// Local path of the active model if it is downloaded.
    pub fn active_model_path(&self) -> Option<PathBuf> {
        self.entries
            .iter()
            .find(|e| !e.is_mmproj() && e.filename == self.active_model && e.state == DownloadState::Downloaded)
            .and_then(|e| e.local_path.clone())
    }

    /// Local path of the active mmproj if it is downloaded.
    pub fn active_mmproj_path(&self) -> Option<PathBuf> {
        if self.active_mmproj.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .find(|e| e.is_mmproj() && e.filename == self.active_mmproj && e.state == DownloadState::Downloaded)
            .and_then(|e| e.local_path.clone())
    }

    /// Whether the explicit mmproj selection matches the active text model.
    pub fn active_mmproj_matches_active_model(&self) -> bool {
        match (self.active_model_entry(), self.active_mmproj_entry()) {
            (Some(model), Some(mmproj)) => model.repo == mmproj.repo,
            _ => true,
        }
    }

    /// Best downloaded text model for a specific repo.
    pub fn best_downloaded_model_for_repo(&self, repo: &str) -> Option<&LlmModelEntry> {
        fn quant_rank(quant: &str) -> usize {
            let order = [
                "Q4_K_M", "Q4_0", "Q4_K_S", "Q4_K_L", "Q4_1", "Q5_K_M", "Q5_K_S", "Q5_K_L", "Q6_K", "Q6_K_L", "Q8_0",
                "IQ4_XS", "IQ4_NL", "Q3_K_M", "Q3_K_L", "Q3_K_XL", "Q3_K_S", "IQ3_M", "IQ3_XS", "IQ3_XXS", "Q2_K",
                "Q2_K_L", "IQ2_M", "IQ2_S", "IQ2_XS", "IQ2_XXS", "BF16", "F16", "F32",
            ];
            order
                .iter()
                .position(|candidate| candidate.eq_ignore_ascii_case(quant))
                .unwrap_or(order.len())
        }

        self.entries
            .iter()
            .filter(|e| !e.is_mmproj() && e.repo == repo && e.state == DownloadState::Downloaded)
            .min_by(|a, b| {
                (!a.recommended)
                    .cmp(&!b.recommended)
                    .then_with(|| a.advanced.cmp(&b.advanced))
                    .then_with(|| quant_rank(&a.quant).cmp(&quant_rank(&b.quant)))
                    .then_with(|| a.size_gb.total_cmp(&b.size_gb))
                    .then_with(|| a.filename.cmp(&b.filename))
            })
    }

    /// Best downloaded text model that can pair with a specific mmproj file.
    pub fn best_model_for_mmproj(&self, mmproj_filename: &str) -> Option<&LlmModelEntry> {
        let repo = self
            .entries
            .iter()
            .find(|e| e.is_mmproj() && e.filename == mmproj_filename)?
            .repo
            .clone();
        self.best_downloaded_model_for_repo(&repo)
    }

    /// Find the best downloaded mmproj for the currently active model.
    pub fn best_mmproj_for_active_model(&self) -> Option<&LlmModelEntry> {
        let active_repo = self.active_model_entry()?.repo.as_str();

        fn quant_rank(quant: &str) -> u8 {
            match quant.to_uppercase().as_str() {
                "BF16" => 0,
                "F16" => 1,
                _ => 2,
            }
        }

        self.entries
            .iter()
            .filter(|e| e.is_mmproj() && e.repo == active_repo && e.state == DownloadState::Downloaded)
            .min_by_key(|e| (!e.recommended as u8, quant_rank(&e.quant)))
    }

    /// If `autoload_mmproj` is requested and no mmproj is currently selected,
    /// pick the best available one for the active model and return its path.
    pub fn resolve_mmproj_path(&self, autoload: bool) -> Option<PathBuf> {
        // Explicit selection always wins.
        if let path @ Some(_) = self.active_mmproj_path() {
            return path;
        }
        if autoload {
            self.best_mmproj_for_active_model().and_then(|e| e.local_path.clone())
        } else {
            None
        }
    }
}
