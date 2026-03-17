// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! `skill-screenshots` — screenshot capture + vision embedding.
//!
//! - **config** — `ScreenshotConfig`
//! - **context** — `ScreenshotContext` trait (abstracts tauri/AppState)
//! - **capture** — capture worker, embed thread, HNSW search, OCR

pub mod config;
pub mod context;
pub(crate) mod platform;
pub mod capture;

pub use config::ScreenshotConfig;
pub use context::{ScreenshotContext, ActiveWindowInfo};
