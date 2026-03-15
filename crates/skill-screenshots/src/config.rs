// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Screenshot configuration types.

use serde::{Deserialize, Serialize};

pub fn default_screenshot_interval()        -> u32    { 5 }
pub fn default_screenshot_image_size()      -> u32    { 768 }
pub fn default_screenshot_quality()         -> u8     { 60 }
pub fn default_screenshot_session_only()    -> bool   { true }
pub fn default_screenshot_embed_backend()   -> String { "fastembed".into() }
pub fn default_screenshot_fastembed_model() -> String { "clip-vit-b-32".into() }
pub fn default_screenshot_ocr_enabled()     -> bool   { true }
pub fn default_screenshot_ocr_engine()      -> String {
    #[cfg(target_os = "macos")]
    { "apple-vision".into() }
    #[cfg(not(target_os = "macos"))]
    { "ocrs".into() }
}
pub fn default_screenshot_use_gpu()         -> bool   { true }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ScreenshotConfig {
    pub enabled: bool,

    #[serde(default = "default_screenshot_interval")]
    pub interval_secs: u32,

    #[serde(default = "default_screenshot_image_size")]
    pub image_size: u32,

    #[serde(default = "default_screenshot_quality")]
    pub quality: u8,

    #[serde(default = "default_screenshot_session_only")]
    pub session_only: bool,

    #[serde(default = "default_screenshot_embed_backend")]
    pub embed_backend: String,

    #[serde(default = "default_screenshot_fastembed_model")]
    pub fastembed_model: String,

    #[serde(default = "default_screenshot_ocr_enabled")]
    pub ocr_enabled: bool,

    #[serde(default = "default_screenshot_ocr_engine")]
    pub ocr_engine: String,

    #[serde(default = "default_screenshot_use_gpu")]
    pub use_gpu: bool,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            enabled:         false,
            interval_secs:   default_screenshot_interval(),
            image_size:      default_screenshot_image_size(),
            quality:         default_screenshot_quality(),
            session_only:    default_screenshot_session_only(),
            embed_backend:   default_screenshot_embed_backend(),
            fastembed_model: default_screenshot_fastembed_model(),
            ocr_enabled:     default_screenshot_ocr_enabled(),
            ocr_engine:      default_screenshot_ocr_engine(),
            use_gpu:         default_screenshot_use_gpu(),
        }
    }
}

impl ScreenshotConfig {
    pub fn fastembed_model_enum(&self) -> Option<fastembed::ImageEmbeddingModel> {
        match self.fastembed_model.as_str() {
            "clip-vit-b-32"           => Some(fastembed::ImageEmbeddingModel::ClipVitB32),
            "nomic-embed-vision-v1.5" => Some(fastembed::ImageEmbeddingModel::NomicEmbedVisionV15),
            _                         => None,
        }
    }

    pub fn model_id(&self) -> String {
        match self.embed_backend.as_str() {
            "fastembed" => match self.fastembed_model.as_str() {
                "clip-vit-b-32"           => "Qdrant/clip-ViT-B-32-vision".into(),
                "nomic-embed-vision-v1.5" => "nomic-ai/nomic-embed-vision-v1.5".into(),
                other                     => other.into(),
            },
            "mmproj" => "mmproj".into(),
            other    => other.into(),
        }
    }

    pub fn recommended_image_size(&self) -> u32 {
        match self.embed_backend.as_str() {
            "fastembed" => match self.fastembed_model.as_str() {
                "nomic-embed-vision-v1.5" => 768,
                _                         => 768,
            },
            "mmproj" => 768,
            _        => 768,
        }
    }
}
