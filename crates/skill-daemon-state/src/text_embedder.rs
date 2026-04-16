// SPDX-License-Identifier: GPL-3.0-only
//! Shared text embedder (nomic-embed-text-v1.5).
//!
//! A single `TextEmbedding` instance is created at daemon startup and shared
//! across labels, hooks, screenshot OCR, and screenshot search.  This avoids
//! loading the ~130 MB ONNX model multiple times.

use std::sync::{Arc, Mutex, Once};

/// Shared, cheaply-cloneable handle to the text embedder.
///
/// The ~130 MB ONNX model is loaded **lazily** on first use (not at daemon
/// startup) so the GPU isn't hammered during init.
#[derive(Clone)]
pub struct SharedTextEmbedder {
    inner: Arc<Mutex<Option<fastembed::TextEmbedding>>>,
    init: Arc<Once>,
}

impl Default for SharedTextEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedTextEmbedder {
    /// Create a new handle **without** loading the model yet.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            init: Arc::new(Once::new()),
        }
    }

    /// Ensure the model is loaded (called at most once).
    fn ensure_loaded(&self) {
        let inner = self.inner.clone();
        self.init.call_once(move || {
            let cache_dir = dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".cache")
                .join("fastembed");
            let model = fastembed::TextEmbedding::try_new(
                fastembed::InitOptions::new(fastembed::EmbeddingModel::NomicEmbedTextV15)
                    .with_cache_dir(cache_dir)
                    .with_show_download_progress(false),
            )
            .ok();
            if model.is_some() {
                eprintln!("[text-embedder] nomic-embed-text-v1.5 loaded");
            } else {
                eprintln!("[text-embedder] failed to load nomic-embed-text-v1.5");
            }
            if let Ok(mut guard) = inner.lock() {
                *guard = model;
            }
        });
    }

    /// Embed a single text string.  Returns `None` if the model is not loaded
    /// or embedding fails.
    pub fn embed(&self, text: &str) -> Option<Vec<f32>> {
        self.ensure_loaded();
        let mut guard = self.inner.lock().ok()?;
        let model = guard.as_mut()?;
        let mut vecs = model.embed(vec![text], None).ok()?;
        if vecs.is_empty() {
            None
        } else {
            Some(vecs.remove(0))
        }
    }

    /// Embed multiple texts in a single batch.
    pub fn embed_batch(&self, texts: Vec<&str>) -> Option<Vec<Vec<f32>>> {
        self.ensure_loaded();
        let mut guard = self.inner.lock().ok()?;
        let model = guard.as_mut()?;
        model.embed(texts, None).ok()
    }
}
