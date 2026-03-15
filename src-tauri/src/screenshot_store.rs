// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Persistent screenshot store — `~/.skill/screenshots.sqlite`.
//!
//! Each row records a captured window screenshot together with its vision
//! embedding (if available), the model that produced it, and active-window
//! context at capture time.

use rusqlite::{Connection, params};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;

use crate::MutexExt;

// ── DDL ───────────────────────────────────────────────────────────────────────

const DDL: &str = "
PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS screenshots (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Temporal keys
    timestamp       INTEGER NOT NULL,     -- YYYYMMDDHHmmss UTC
    unix_ts         INTEGER NOT NULL,     -- unix seconds

    -- File reference
    filename        TEXT    NOT NULL,     -- relative: \"20260315/20260315143025.webp\"
    width           INTEGER NOT NULL,
    height          INTEGER NOT NULL,
    file_size       INTEGER NOT NULL,     -- bytes on disk

    -- Embedding
    hnsw_id         INTEGER,              -- row in screenshots.hnsw (NULL if not embedded)
    embedding       BLOB,                 -- f32 LE × dim (NULL if model unavailable)
    embedding_dim   INTEGER NOT NULL DEFAULT 0,

    -- Model provenance
    model_backend   TEXT NOT NULL DEFAULT '',
    model_id        TEXT NOT NULL DEFAULT '',
    image_size      INTEGER NOT NULL DEFAULT 0,
    quality         INTEGER NOT NULL DEFAULT 0,

    -- Active-window context
    app_name        TEXT NOT NULL DEFAULT '',
    window_title    TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_ss_ts       ON screenshots (timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_ss_unix     ON screenshots (unix_ts DESC);
CREATE INDEX IF NOT EXISTS idx_ss_model    ON screenshots (model_backend, model_id);
";

// ── Public types ──────────────────────────────────────────────────────────────

/// Row data for inserting a new screenshot.
pub struct ScreenshotRow {
    pub timestamp:     i64,
    pub unix_ts:       u64,
    pub filename:      String,
    pub width:         u32,
    pub height:        u32,
    pub file_size:     u64,
    pub hnsw_id:       Option<u64>,
    pub embedding:     Option<Vec<f32>>,
    pub embedding_dim: usize,
    pub model_backend: String,
    pub model_id:      String,
    pub image_size:    u32,
    pub quality:       u8,
    pub app_name:      String,
    pub window_title:  String,
}

/// Lightweight result type for search queries.
#[derive(Clone, Debug, Serialize)]
pub struct ScreenshotResult {
    pub timestamp:    i64,
    pub unix_ts:      u64,
    pub filename:     String,
    pub app_name:     String,
    pub window_title: String,
    pub similarity:   f32,
}

/// Estimate for re-embedding work.
#[derive(Clone, Debug, Serialize)]
pub struct ReembedEstimate {
    pub total:        usize,
    pub stale:        usize,
    pub unembedded:   usize,
    pub per_image_ms: u64,
    pub eta_secs:     u64,
}

/// Result of a re-embedding run.
#[derive(Clone, Debug, Serialize)]
pub struct ReembedResult {
    pub embedded:     usize,
    pub skipped:      usize,
    pub elapsed_secs: f64,
}

/// Result when setting config and the model changed.
#[derive(Clone, Debug, Serialize)]
pub struct ConfigChangeResult {
    pub model_changed: bool,
    pub stale_count:   usize,
}

/// A row queried for re-embedding.
pub struct EmbeddableRow {
    pub id:       i64,
    pub filename: String,
}

// ── Store ─────────────────────────────────────────────────────────────────────

pub struct ScreenshotStore {
    conn: Mutex<Connection>,
}

impl ScreenshotStore {
    /// Open (or create) the screenshot database inside `skill_dir`.
    pub fn open(skill_dir: &Path) -> Option<Self> {
        let db_path = skill_dir.join(crate::constants::SCREENSHOTS_SQLITE);
        let conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[screenshot_store] open error: {e}");
                return None;
            }
        };
        if let Err(e) = conn.execute_batch(DDL) {
            eprintln!("[screenshot_store] DDL error: {e}");
            return None;
        }
        Some(Self { conn: Mutex::new(conn) })
    }

    /// Insert a new screenshot record.
    pub fn insert(&self, row: &ScreenshotRow) -> Option<i64> {
        let conn = self.conn.lock_or_recover();
        let emb_blob: Option<Vec<u8>> = row.embedding.as_ref().map(|v| {
            v.iter().flat_map(|f| f.to_le_bytes()).collect()
        });
        conn.execute(
            "INSERT INTO screenshots (
                timestamp, unix_ts, filename, width, height, file_size,
                hnsw_id, embedding, embedding_dim,
                model_backend, model_id, image_size, quality,
                app_name, window_title
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                row.timestamp,
                row.unix_ts as i64,
                row.filename,
                row.width,
                row.height,
                row.file_size as i64,
                row.hnsw_id.map(|v| v as i64),
                emb_blob,
                row.embedding_dim as i64,
                row.model_backend,
                row.model_id,
                row.image_size,
                row.quality as i64,
                row.app_name,
                row.window_title,
            ],
        ).ok()?;
        Some(conn.last_insert_rowid())
    }

    /// Count screenshots that have embeddings (any model).
    pub fn count_embedded(&self) -> usize {
        let conn = self.conn.lock_or_recover();
        conn.query_row(
            "SELECT COUNT(*) FROM screenshots WHERE embedding IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0),
        ).unwrap_or(0) as usize
    }

    /// Count screenshots that have no embedding.
    pub fn count_unembedded(&self) -> usize {
        let conn = self.conn.lock_or_recover();
        conn.query_row(
            "SELECT COUNT(*) FROM screenshots WHERE embedding IS NULL",
            [],
            |r| r.get::<_, i64>(0),
        ).unwrap_or(0) as usize
    }

    /// Count screenshots embedded with a model other than the specified one.
    pub fn count_stale(&self, backend: &str, model_id: &str) -> usize {
        let conn = self.conn.lock_or_recover();
        conn.query_row(
            "SELECT COUNT(*) FROM screenshots
             WHERE embedding IS NOT NULL
               AND (model_backend != ?1 OR model_id != ?2)",
            params![backend, model_id],
            |r| r.get::<_, i64>(0),
        ).unwrap_or(0) as usize
    }

    /// Get all rows that need (re-)embedding — either stale or unembedded.
    pub fn rows_needing_embed(&self, backend: &str, model_id: &str) -> Vec<EmbeddableRow> {
        let conn = self.conn.lock_or_recover();
        let mut stmt = conn.prepare(
            "SELECT id, filename FROM screenshots
             WHERE embedding IS NULL
                OR (model_backend != ?1 OR model_id != ?2)
             ORDER BY id"
        ).unwrap();
        stmt.query_map(params![backend, model_id], |r| {
            Ok(EmbeddableRow {
                id:       r.get(0)?,
                filename: r.get(1)?,
            })
        }).unwrap().filter_map(|r| r.ok()).collect()
    }

    /// Update embedding for a specific row.
    pub fn update_embedding(
        &self,
        id: i64,
        embedding: &[f32],
        hnsw_id: Option<u64>,
        backend: &str,
        model_id: &str,
        image_size: u32,
    ) {
        let conn = self.conn.lock_or_recover();
        let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        let _ = conn.execute(
            "UPDATE screenshots SET
                embedding = ?1, embedding_dim = ?2, hnsw_id = ?3,
                model_backend = ?4, model_id = ?5, image_size = ?6
             WHERE id = ?7",
            params![
                blob,
                embedding.len() as i64,
                hnsw_id.map(|v| v as i64),
                backend,
                model_id,
                image_size,
                id,
            ],
        );
    }

    /// Load all embeddings from the database (for HNSW rebuild).
    pub fn all_embeddings(&self) -> Vec<(i64, Vec<f32>)> {
        let conn = self.conn.lock_or_recover();
        let mut stmt = conn.prepare(
            "SELECT timestamp, embedding, embedding_dim FROM screenshots
             WHERE embedding IS NOT NULL
             ORDER BY id"
        ).unwrap();
        stmt.query_map([], |r| {
            let ts: i64 = r.get(0)?;
            let blob: Vec<u8> = r.get(1)?;
            let dim: i64 = r.get(2)?;
            let floats: Vec<f32> = blob.chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            debug_assert_eq!(floats.len(), dim as usize);
            Ok((ts, floats))
        }).unwrap().filter_map(|r| r.ok()).collect()
    }

    /// Find screenshots by timestamp range.
    pub fn around_timestamp(&self, ts: i64, window_secs: i32) -> Vec<ScreenshotResult> {
        let conn = self.conn.lock_or_recover();
        let lo = ts - window_secs as i64;
        let hi = ts + window_secs as i64;
        let mut stmt = conn.prepare(
            "SELECT timestamp, unix_ts, filename, app_name, window_title
             FROM screenshots
             WHERE unix_ts BETWEEN ?1 AND ?2
             ORDER BY unix_ts"
        ).unwrap();
        stmt.query_map(params![lo, hi], |r| {
            Ok(ScreenshotResult {
                timestamp:    r.get(0)?,
                unix_ts:      r.get::<_, i64>(1)? as u64,
                filename:     r.get(2)?,
                app_name:     r.get(3)?,
                window_title: r.get(4)?,
                similarity:   0.0,
            })
        }).unwrap().filter_map(|r| r.ok()).collect()
    }

    /// Get total screenshot count.
    #[allow(dead_code)]
    pub fn count_all(&self) -> usize {
        let conn = self.conn.lock_or_recover();
        conn.query_row("SELECT COUNT(*) FROM screenshots", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize
    }
}
