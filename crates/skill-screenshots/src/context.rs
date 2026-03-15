// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Abstraction traits that decouple screenshot logic from tauri/AppState.

use crate::config::ScreenshotConfig;
use serde_json::Value;

/// Active-window metadata snapshot.
#[derive(Clone, Default)]
pub struct ActiveWindowInfo {
    pub app_name: String,
    pub window_title: String,
}

/// Trait providing the runtime context the screenshot worker needs.
///
/// In production this is implemented over `tauri::AppHandle` + `AppState`.
/// In tests or standalone usage it can be a mock.
pub trait ScreenshotContext: Send + Sync + 'static {
    /// Read the current screenshot config.
    fn config(&self) -> ScreenshotConfig;

    /// Whether an EEG session is currently active.
    fn is_session_active(&self) -> bool;

    /// Read current active window info.
    fn active_window(&self) -> ActiveWindowInfo;

    /// Emit a named event with a JSON payload (to the UI).
    fn emit_event(&self, event: &str, payload: Value);

    /// Try to embed an image via the LLM vision projector (mmproj).
    /// Returns `None` if no LLM/mmproj is loaded or vision is not ready.
    fn embed_image_via_llm(&self, png_bytes: &[u8]) -> Option<Vec<f32>>;

    /// Embed a short text string using the app-wide shared text embedder
    /// (typically `bge-small-en-v1.5`).  Used for OCR text embeddings so the
    /// screenshot crate can reuse the same model instance as labels / hooks
    /// instead of loading a separate ~130 MB copy.
    ///
    /// Returns `None` if the embedder is not yet initialised or embedding
    /// fails.  The default implementation always returns `None` (standalone /
    /// test contexts that don't have a text embedder).
    fn embed_text(&self, _text: &str) -> Option<Vec<f32>> { None }
}
