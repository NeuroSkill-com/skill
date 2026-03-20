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

use crate::util::MutexExt;

// ── DDL ───────────────────────────────────────────────────────────────────────

const DDL: &str = "
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
    window_title    TEXT NOT NULL DEFAULT '',

    -- OCR
    ocr_text        TEXT NOT NULL DEFAULT '',       -- extracted text (full)
    ocr_embedding   BLOB,                           -- text embedding (f32 LE × dim)
    ocr_embedding_dim INTEGER NOT NULL DEFAULT 0,
    ocr_hnsw_id     INTEGER                         -- row in screenshots_ocr.hnsw
);

CREATE INDEX IF NOT EXISTS idx_ss_ts       ON screenshots (timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_ss_unix     ON screenshots (unix_ts DESC);
CREATE INDEX IF NOT EXISTS idx_ss_model    ON screenshots (model_backend, model_id);
";

// ── Migrations ────────────────────────────────────────────────────────────────

const MIGRATE_OCR_TEXT: &str       = "ALTER TABLE screenshots ADD COLUMN ocr_text TEXT NOT NULL DEFAULT ''";
const MIGRATE_OCR_EMBEDDING: &str  = "ALTER TABLE screenshots ADD COLUMN ocr_embedding BLOB";
const MIGRATE_OCR_DIM: &str        = "ALTER TABLE screenshots ADD COLUMN ocr_embedding_dim INTEGER NOT NULL DEFAULT 0";
const MIGRATE_OCR_HNSW: &str       = "ALTER TABLE screenshots ADD COLUMN ocr_hnsw_id INTEGER";
const MIGRATE_GIF_FILENAME: &str   = "ALTER TABLE screenshots ADD COLUMN gif_filename TEXT NOT NULL DEFAULT ''";

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
    pub ocr_text:      String,
    pub ocr_embedding: Option<Vec<f32>>,
    pub ocr_embedding_dim: usize,
    pub ocr_hnsw_id:   Option<u64>,
}

/// Lightweight result type for search queries.
#[derive(Clone, Debug, Serialize)]
pub struct ScreenshotResult {
    pub timestamp:    i64,
    pub unix_ts:      u64,
    pub filename:     String,
    pub app_name:     String,
    pub window_title: String,
    pub ocr_text:     String,
    pub similarity:   f32,
    /// Relative path to the animated GIF (empty if no motion was detected).
    #[serde(default)]
    pub gif_filename: String,
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

/// Snapshot of embedding + OCR data for a single row (used when copying
/// results from a duplicate screenshot).
pub struct EmbeddingAndOcr {
    pub embedding:     Option<Vec<f32>>,
    pub model_backend: String,
    pub model_id:      String,
    pub image_size:    u32,
    pub ocr_text:      String,
    pub ocr_embedding: Option<Vec<f32>>,
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
        let db_path = skill_dir.join(skill_constants::SCREENSHOTS_SQLITE);
        let conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[screenshot_store] open error: {e}");
                return None;
            }
        };
        crate::util::init_wal_pragmas(&conn);
        if let Err(e) = conn.execute_batch(DDL) {
            eprintln!("[screenshot_store] DDL error: {e}");
            return None;
        }
        // Run OCR migrations (silently ignore "duplicate column" errors
        // on databases that already have these columns).
        for sql in [MIGRATE_OCR_TEXT, MIGRATE_OCR_EMBEDDING, MIGRATE_OCR_DIM, MIGRATE_OCR_HNSW, MIGRATE_GIF_FILENAME] {
            let _ = conn.execute(sql, []);
        }
        Some(Self { conn: Mutex::new(conn) })
    }

    /// Insert a new screenshot record.
    pub fn insert(&self, row: &ScreenshotRow) -> Option<i64> {
        let conn = self.conn.lock_or_recover();
        let emb_blob: Option<Vec<u8>> = row.embedding.as_ref().map(|v| crate::util::f32_to_blob(v));
        let ocr_blob: Option<Vec<u8>> = row.ocr_embedding.as_ref().map(|v| crate::util::f32_to_blob(v));
        conn.execute(
            "INSERT INTO screenshots (
                timestamp, unix_ts, filename, width, height, file_size,
                hnsw_id, embedding, embedding_dim,
                model_backend, model_id, image_size, quality,
                app_name, window_title,
                ocr_text, ocr_embedding, ocr_embedding_dim, ocr_hnsw_id
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
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
                row.ocr_text,
                ocr_blob,
                row.ocr_embedding_dim as i64,
                row.ocr_hnsw_id.map(|v| v as i64),
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
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, filename FROM screenshots
             WHERE embedding IS NULL
                OR (model_backend != ?1 OR model_id != ?2)
             ORDER BY id"
        ) else { return vec![] };
        let Ok(rows) = stmt.query_map(params![backend, model_id], |r| {
            Ok(EmbeddableRow {
                id:       r.get(0)?,
                filename: r.get(1)?,
            })
        }) else { return vec![] };
        rows.filter_map(|r| r.ok()).collect()
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
        let blob: Vec<u8> = crate::util::f32_to_blob(embedding);
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
        let Ok(mut stmt) = conn.prepare(
            "SELECT timestamp, embedding, embedding_dim FROM screenshots
             WHERE embedding IS NOT NULL
             ORDER BY id"
        ) else { return vec![] };
        let Ok(rows) = stmt.query_map([], |r| {
            let ts: i64 = r.get(0)?;
            let blob: Vec<u8> = r.get(1)?;
            let dim: i64 = r.get(2)?;
            let floats: Vec<f32> = crate::util::blob_to_f32(&blob);
            debug_assert_eq!(floats.len(), dim as usize);
            Ok((ts, floats))
        }) else { return vec![] };
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Find a screenshot by its exact YYYYMMDDHHmmss timestamp (HNSW payload).
    pub fn find_by_timestamp(&self, ts: i64) -> Option<ScreenshotResult> {
        let conn = self.conn.lock_or_recover();
        conn.query_row(
            "SELECT timestamp, unix_ts, filename, app_name, window_title, ocr_text, gif_filename
             FROM screenshots WHERE timestamp = ?1",
            params![ts],
            |r| Ok(ScreenshotResult {
                timestamp:    r.get(0)?,
                unix_ts:      r.get::<_, i64>(1)? as u64,
                filename:     r.get(2)?,
                app_name:     r.get(3)?,
                window_title: r.get(4)?,
                ocr_text:     r.get::<_, String>(5).unwrap_or_default(),
                similarity:   0.0,
                gif_filename: r.get::<_, String>(6).unwrap_or_default(),
            }),
        ).ok()
    }

    /// Find screenshots by unix timestamp range.
    pub fn around_timestamp(&self, ts: i64, window_secs: i32) -> Vec<ScreenshotResult> {
        let conn = self.conn.lock_or_recover();
        let lo = ts - window_secs as i64;
        let hi = ts + window_secs as i64;
        let Ok(mut stmt) = conn.prepare(
            "SELECT timestamp, unix_ts, filename, app_name, window_title, ocr_text, gif_filename
             FROM screenshots
             WHERE unix_ts BETWEEN ?1 AND ?2
             ORDER BY unix_ts"
        ) else { return vec![] };
        let Ok(rows) = stmt.query_map(params![lo, hi], |r| {
            Ok(ScreenshotResult {
                timestamp:    r.get(0)?,
                unix_ts:      r.get::<_, i64>(1)? as u64,
                filename:     r.get(2)?,
                app_name:     r.get(3)?,
                window_title: r.get(4)?,
                ocr_text:     r.get::<_, String>(5).unwrap_or_default(),
                similarity:   0.0,
                gif_filename: r.get::<_, String>(6).unwrap_or_default(),
            })
        }) else { return vec![] };
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Load all OCR text embeddings from the database (for HNSW rebuild).
    pub fn all_ocr_embeddings(&self) -> Vec<(i64, Vec<f32>)> {
        let conn = self.conn.lock_or_recover();
        let Ok(mut stmt) = conn.prepare(
            "SELECT timestamp, ocr_embedding, ocr_embedding_dim FROM screenshots
             WHERE ocr_embedding IS NOT NULL
             ORDER BY id"
        ) else { return vec![] };
        let Ok(rows) = stmt.query_map([], |r| {
            let ts: i64 = r.get(0)?;
            let blob: Vec<u8> = r.get(1)?;
            let dim: i64 = r.get(2)?;
            let floats: Vec<f32> = crate::util::blob_to_f32(&blob);
            debug_assert_eq!(floats.len(), dim as usize);
            Ok((ts, floats))
        }) else { return vec![] };
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Update OCR text and embedding for a specific row.
    pub fn update_ocr(
        &self,
        id: i64,
        ocr_text: &str,
        ocr_embedding: Option<&[f32]>,
        ocr_hnsw_id: Option<u64>,
    ) {
        let conn = self.conn.lock_or_recover();
        let blob: Option<Vec<u8>> = ocr_embedding.map(|emb| crate::util::f32_to_blob(emb));
        let dim = ocr_embedding.map_or(0i64, |e| e.len() as i64);
        let _ = conn.execute(
            "UPDATE screenshots SET
                ocr_text = ?1, ocr_embedding = ?2, ocr_embedding_dim = ?3, ocr_hnsw_id = ?4
             WHERE id = ?5",
            params![
                ocr_text,
                blob,
                dim,
                ocr_hnsw_id.map(|v| v as i64),
                id,
            ],
        );
    }

    /// Search screenshots by OCR text (LIKE query).
    pub fn search_by_ocr_text(&self, query: &str, limit: usize) -> Vec<ScreenshotResult> {
        let conn = self.conn.lock_or_recover();
        let pattern = format!("%{query}%");
        let Ok(mut stmt) = conn.prepare(
            "SELECT timestamp, unix_ts, filename, app_name, window_title, ocr_text, gif_filename
             FROM screenshots
             WHERE ocr_text LIKE ?1
             ORDER BY unix_ts DESC
             LIMIT ?2"
        ) else { return vec![] };
        let Ok(rows) = stmt.query_map(params![pattern, limit as i64], |r| {
            Ok(ScreenshotResult {
                timestamp:    r.get(0)?,
                unix_ts:      r.get::<_, i64>(1)? as u64,
                filename:     r.get(2)?,
                app_name:     r.get(3)?,
                window_title: r.get(4)?,
                ocr_text:     r.get::<_, String>(5).unwrap_or_default(),
                similarity:   0.0,
                gif_filename: r.get::<_, String>(6).unwrap_or_default(),
            })
        }) else { return vec![] };
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Get rows that have no vision embedding yet (captured but not embedded).
    pub fn rows_without_embedding(&self) -> Vec<EmbeddableRow> {
        let conn = self.conn.lock_or_recover();
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, filename FROM screenshots
             WHERE embedding IS NULL
             ORDER BY id"
        ) else { return vec![] };
        let Ok(rows) = stmt.query_map([], |r| {
            Ok(EmbeddableRow {
                id:       r.get(0)?,
                filename: r.get(1)?,
            })
        }) else { return vec![] };
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Get rows that have no OCR text yet (ocr_text is empty).
    pub fn rows_without_ocr(&self) -> Vec<EmbeddableRow> {
        let conn = self.conn.lock_or_recover();
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, filename FROM screenshots
             WHERE ocr_text = '' OR ocr_text IS NULL
             ORDER BY id"
        ) else { return vec![] };
        let Ok(rows) = stmt.query_map([], |r| {
            Ok(EmbeddableRow {
                id:       r.get(0)?,
                filename: r.get(1)?,
            })
        }) else { return vec![] };
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Fetch the vision embedding, model provenance, OCR text, and OCR
    /// embedding for a given row — used to copy results when consecutive
    /// screenshots are identical.
    pub fn get_embedding_and_ocr(&self, id: i64) -> Option<EmbeddingAndOcr> {
        let conn = self.conn.lock_or_recover();
        conn.query_row(
            "SELECT embedding, embedding_dim, model_backend, model_id, image_size,
                    ocr_text, ocr_embedding, ocr_embedding_dim
             FROM screenshots WHERE id = ?1",
            params![id],
            |r| {
                let emb_blob: Option<Vec<u8>> = r.get(0)?;
                let emb_dim: i64 = r.get(1)?;
                let embedding = emb_blob.map(|b| {
                    let v = crate::util::blob_to_f32(&b);
                    debug_assert_eq!(v.len(), emb_dim as usize);
                    v
                });
                let ocr_emb_blob: Option<Vec<u8>> = r.get(6)?;
                let ocr_emb_dim: i64 = r.get(7)?;
                let ocr_embedding = ocr_emb_blob.map(|b| {
                    let v = crate::util::blob_to_f32(&b);
                    debug_assert_eq!(v.len(), ocr_emb_dim as usize);
                    v
                });
                Ok(EmbeddingAndOcr {
                    embedding,
                    model_backend: r.get(2)?,
                    model_id:      r.get(3)?,
                    image_size:    r.get::<_, i64>(4)? as u32,
                    ocr_text:      r.get::<_, String>(5).unwrap_or_default(),
                    ocr_embedding,
                })
            },
        ).ok()
    }

    /// Get the timestamp for a row by id.
    pub fn get_timestamp(&self, id: i64) -> Option<i64> {
        let conn = self.conn.lock_or_recover();
        conn.query_row(
            "SELECT timestamp FROM screenshots WHERE id = ?1",
            params![id],
            |r| r.get(0),
        ).ok()
    }

    /// Get total screenshot count.
    #[allow(dead_code)]
    pub fn count_all(&self) -> usize {
        let conn = self.conn.lock_or_recover();
        conn.query_row("SELECT COUNT(*) FROM screenshots", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize
    }

    /// Set the animated GIF filename for a screenshot row.
    pub fn update_gif_filename(&self, id: i64, gif_filename: &str) {
        let conn = self.conn.lock_or_recover();
        let _ = conn.execute(
            "UPDATE screenshots SET gif_filename = ?1 WHERE id = ?2",
            params![gif_filename, id],
        );
    }
}
