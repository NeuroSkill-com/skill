// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Structured error types for skill-llm.

/// Errors from model download / catalog operations.
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    /// Download was paused by the user.
    #[error("download paused")]
    Paused,

    /// Download was cancelled by the user.
    #[error("download cancelled")]
    Cancelled,

    /// File not found in the HuggingFace manifest.
    #[error("{filename}: not listed in {repo_id} manifest")]
    NotInManifest { filename: String, repo_id: String },

    /// LFS sha256 hash missing from manifest.
    #[error("{filename}: LFS sha256 absent in manifest")]
    MissingHash { filename: String },

    /// HTTP error during download.
    #[error("HTTP {status} for {filename}: {body}")]
    Http {
        status: u16,
        filename: String,
        body: String,
    },

    /// SHA-256 verification failed after download.
    #[error("{filename}: sha256 mismatch (expected {expected}, got {actual})")]
    HashMismatch {
        filename: String,
        expected: String,
        actual: String,
    },

    /// Network or I/O error.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// Errors from the chat store.
#[derive(Debug, thiserror::Error)]
pub enum ChatStoreError {
    /// SQLite error.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Errors from the LLM engine / inference.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// Model is not loaded.
    #[error("no model loaded")]
    NoModel,

    /// Model loading failed.
    #[error("failed to load model: {0}")]
    LoadFailed(String),

    /// Inference error.
    #[error("inference error: {0}")]
    Inference(String),

    /// Generic wrapped error.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
