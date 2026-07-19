// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! `skill-screenshots` — screenshot capture + vision embedding.
//!
//! - **config** — re-exports `ScreenshotConfig` from `skill-settings`
//! - **context** — `ScreenshotContext` trait (abstracts tauri/AppState)
//! - **capture** — capture worker, embed thread, HNSW search, OCR
//! - **rlx_image** — RLX (`rlx-embed`) image embedder for query-by-image

pub mod capture;
pub mod chat_image;
pub mod config;
pub mod context;
#[allow(dead_code)]
pub(crate) mod gif_encode;
pub(crate) mod platform;
#[cfg(feature = "text-embeddings-rlx")]
pub mod rlx_image;
pub mod user_screenshot;

// Re-export so existing `skill_screenshots::ScreenshotConfig` paths keep working.
pub use context::{ActiveWindowInfo, ScreenshotContext};
pub use skill_settings::ScreenshotConfig;
