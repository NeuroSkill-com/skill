// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Structured error types for skill-data.

use std::path::PathBuf;

/// Errors that can occur when opening or writing session files.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// Failed to create or open a CSV file.
    #[error("failed to open CSV file {path}: {source}")]
    CsvOpen { path: PathBuf, source: std::io::Error },

    /// Failed to create or open a Parquet file.
    #[cfg(feature = "parquet")]
    #[error("failed to open Parquet file {path}: {source}")]
    ParquetOpen {
        path: PathBuf,
        #[source]
        source: parquet::errors::ParquetError,
    },

    /// I/O error during session write.
    #[error("session I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors from the screenshot store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite error.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// Serialization / deserialization error.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
