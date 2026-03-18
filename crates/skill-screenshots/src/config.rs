// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Screenshot configuration — re-exported from `skill-settings`.
//!
//! The canonical [`ScreenshotConfig`] struct lives in `skill-settings` so
//! that other crates can read/write it without pulling in `xcap`/`pipewire`.
//! This module re-exports it and adds the `fastembed`-specific helper that
//! requires the `fastembed` dependency.

pub use skill_settings::ScreenshotConfig;

/// Extension: resolve the fastembed model enum from the config string.
///
/// This lives here (not in `skill-settings`) because it depends on the
/// `fastembed` crate which is only available in `skill-screenshots`.
pub fn fastembed_model_enum(config: &ScreenshotConfig) -> Option<fastembed::ImageEmbeddingModel> {
    match config.fastembed_model.as_str() {
        "clip-vit-b-32"           => Some(fastembed::ImageEmbeddingModel::ClipVitB32),
        "nomic-embed-vision-v1.5" => Some(fastembed::ImageEmbeddingModel::NomicEmbedVisionV15),
        _                         => None,
    }
}
