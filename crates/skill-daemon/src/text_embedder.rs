// SPDX-License-Identifier: GPL-3.0-only
//! Shared text embedder (nomic-embed-text-v1.5).
//!
//! A single `TextEmbedding` instance is created at daemon startup and shared
//! across labels, hooks, screenshot OCR, and screenshot search.  This avoids
//! loading the ~130 MB ONNX model multiple times.

use std::sync::{Arc, Mutex};

/// Shared, cheaply-cloneable handle to the text embedder.
#[derive(Clone)]
pub(crate) struct SharedTextEmbedder {
    inner: Arc<Mutex<Option<fastembed::TextEmbedding>>>,
}

impl SharedTextEmbedder {
    /// Create and load the nomic-embed-text-v1.5 model.
    ///
    /// If model loading fails the embedder starts empty — `embed()` will
    /// return `None` until a model is available.
    pub(crate) fn new() -> Self {
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
        Self {
            inner: Arc::new(Mutex::new(model)),
        }
    }

    /// Embed a single text string.  Returns `None` if the model is not loaded
    /// or embedding fails.
    pub(crate) fn embed(&self, text: &str) -> Option<Vec<f32>> {
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
    pub(crate) fn embed_batch(&self, texts: Vec<&str>) -> Option<Vec<Vec<f32>>> {
        let mut guard = self.inner.lock().ok()?;
        let model = guard.as_mut()?;
        model.embed(texts, None).ok()
    }
}
