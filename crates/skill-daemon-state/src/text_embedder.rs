// SPDX-License-Identifier: GPL-3.0-only
//! Shared text embedder (fastembed ONNX models).
//!
//! A single `TextEmbedding` instance is created at daemon startup and shared
//! across labels, hooks, screenshot OCR, and screenshot search.  This avoids
//! loading the ~130 MB ONNX model multiple times.

use std::sync::{Arc, Mutex, Once};

/// Shared, cheaply-cloneable handle to the text embedder.
///
/// The ONNX model is loaded **lazily** on first use (not at daemon
/// startup) so the GPU isn't hammered during init.
#[derive(Clone)]
pub struct SharedTextEmbedder {
    inner: Arc<Mutex<Option<fastembed::TextEmbedding>>>,
    init: Arc<Once>,
    model_code: Arc<Mutex<String>>,
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
            model_code: Arc::new(Mutex::new("nomic-ai/nomic-embed-text-v1.5".into())),
        }
    }

    /// Create a handle that never loads the model (always returns `None`).
    /// Useful for unit tests that don't need real embeddings.
    pub fn new_noop() -> Self {
        let s = Self::new();
        s.init.call_once(|| {});
        s
    }

    /// Set the model code to use. Call [`reload`] after to apply.
    pub fn set_model_code(&self, code: &str) {
        if let Ok(mut guard) = self.model_code.lock() {
            *guard = code.to_string();
        }
    }

    /// Get the current model code.
    pub fn model_code(&self) -> String {
        self.model_code.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Reload the model (e.g. after changing model_code).
    /// Blocks while loading weights. Returns false for unknown model codes.
    pub fn reload(&self) -> bool {
        let code = self.model_code();
        let Some(fe_model) = model_code_to_fastembed(&code) else {
            eprintln!("[text-embedder] unknown model code: {code}");
            return false;
        };
        let cache_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".cache")
            .join("fastembed");
        let model = fastembed::TextEmbedding::try_new(
            fastembed::InitOptions::new(fe_model)
                .with_cache_dir(cache_dir)
                .with_show_download_progress(true),
        )
        .ok();
        let ok = model.is_some();
        if ok {
            eprintln!("[text-embedder] {code} loaded");
        } else {
            eprintln!("[text-embedder] failed to load {code}");
        }
        if let Ok(mut guard) = self.inner.lock() {
            *guard = model;
        }
        ok
    }

    /// Ensure the model is loaded (called at most once).
    fn ensure_loaded(&self) {
        let inner = self.inner.clone();
        let model_code = self.model_code.clone();
        self.init.call_once(move || {
            let code = model_code.lock().map(|g| g.clone()).unwrap_or_default();
            let fe_model = model_code_to_fastembed(&code).unwrap_or(fastembed::EmbeddingModel::NomicEmbedTextV15);
            let cache_dir = dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".cache")
                .join("fastembed");
            let model = fastembed::TextEmbedding::try_new(
                fastembed::InitOptions::new(fe_model)
                    .with_cache_dir(cache_dir)
                    .with_show_download_progress(false),
            )
            .ok();
            if model.is_some() {
                eprintln!("[text-embedder] {code} loaded");
            } else {
                eprintln!("[text-embedder] failed to load {code}");
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

/// Map a model code string to the fastembed enum variant.
/// Returns `None` for unrecognized model codes.
pub fn model_code_to_fastembed(code: &str) -> Option<fastembed::EmbeddingModel> {
    Some(match code {
        "nomic-ai/nomic-embed-text-v1" => fastembed::EmbeddingModel::NomicEmbedTextV1,
        "nomic-ai/nomic-embed-text-v1.5" => fastembed::EmbeddingModel::NomicEmbedTextV15,
        "nomic-ai/nomic-embed-text-v1.5-Q" => fastembed::EmbeddingModel::NomicEmbedTextV15Q,
        "BAAI/bge-small-en-v1.5" => fastembed::EmbeddingModel::BGESmallENV15,
        "BAAI/bge-small-en-v1.5-Q" => fastembed::EmbeddingModel::BGESmallENV15Q,
        "BAAI/bge-base-en-v1.5" => fastembed::EmbeddingModel::BGEBaseENV15,
        "BAAI/bge-base-en-v1.5-Q" => fastembed::EmbeddingModel::BGEBaseENV15Q,
        "BAAI/bge-large-en-v1.5" => fastembed::EmbeddingModel::BGELargeENV15,
        "BAAI/bge-large-en-v1.5-Q" => fastembed::EmbeddingModel::BGELargeENV15Q,
        "BAAI/bge-m3" => fastembed::EmbeddingModel::BGEM3,
        "sentence-transformers/all-MiniLM-L6-v2" => fastembed::EmbeddingModel::AllMiniLML6V2,
        "sentence-transformers/all-MiniLM-L12-v2" => fastembed::EmbeddingModel::AllMiniLML12V2,
        "sentence-transformers/all-mpnet-base-v2" => fastembed::EmbeddingModel::AllMpnetBaseV2,
        "sentence-transformers/paraphrase-MiniLM-L12-v2" => fastembed::EmbeddingModel::ParaphraseMLMiniLML12V2,
        "intfloat/multilingual-e5-small" => fastembed::EmbeddingModel::MultilingualE5Small,
        "intfloat/multilingual-e5-base" => fastembed::EmbeddingModel::MultilingualE5Base,
        "intfloat/multilingual-e5-large" => fastembed::EmbeddingModel::MultilingualE5Large,
        "mixedbread-ai/mxbai-embed-large-v1" => fastembed::EmbeddingModel::MxbaiEmbedLargeV1,
        "Alibaba-NLP/gte-base-en-v1.5" => fastembed::EmbeddingModel::GTEBaseENV15,
        // Also handle the Xenova/ prefix used in older settings
        "Xenova/bge-small-en-v1.5" => fastembed::EmbeddingModel::BGESmallENV15,
        _ => return None,
    })
}
