// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Persistent label storage — `~/.skill/labels.sqlite`.
//!
//! Each row records the text the user typed together with two identical time
//! windows (EEG and label-dialog open time are the same, because EEG is always
//! recording while the user types):
//!
//! * `eeg_start / eeg_end`     — unix-second range of EEG data that overlaps
//!   with the labelling session.  Query the daily
//!   `eeg.sqlite` embeddings table with
//!   `WHERE timestamp BETWEEN … AND …` to retrieve
//!   all embeddings recorded during this label.
//! * `label_start / label_end` — when the label window was opened and submitted
//!   (identical to the EEG range above).
//! * `created_at`              — insertion timestamp (unix seconds).

use rusqlite::{Connection, params};
use std::path::Path;

use skill_constants::LABELS_FILE;

const DDL: &str = "
    CREATE TABLE IF NOT EXISTS labels (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        eeg_start   INTEGER NOT NULL,
        eeg_end     INTEGER NOT NULL,
        label_start INTEGER NOT NULL,
        label_end   INTEGER NOT NULL,
        text        TEXT    NOT NULL,
        context     TEXT    NOT NULL DEFAULT '',
        created_at  INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_labels_eeg_start ON labels (eeg_start);
";

/// Migration: add the `context` column to databases created before this field existed.
/// SQLite returns an error if the column already exists — we silently ignore it.
const MIGRATE_CONTEXT: &str = "ALTER TABLE labels ADD COLUMN context TEXT NOT NULL DEFAULT ''";

/// Migration: add embedding BLOBs (nullable; populated asynchronously after insert).
const MIGRATE_TEXT_EMBEDDING: &str    = "ALTER TABLE labels ADD COLUMN text_embedding BLOB";
const MIGRATE_CONTEXT_EMBEDDING: &str = "ALTER TABLE labels ADD COLUMN context_embedding BLOB";
/// Migration: track which fastembed model produced the stored vectors.
/// NULL means the row has not been embedded yet (or was embedded before this
/// column was added and needs re-embedding).
const MIGRATE_EMBEDDING_MODEL: &str   = "ALTER TABLE labels ADD COLUMN embedding_model TEXT";

pub struct LabelStore {
    conn: Connection,
}

impl LabelStore {
    /// Open (or create) the label database inside `skill_dir`.
    pub fn open(skill_dir: &Path) -> Option<Self> {
        let db_path = skill_dir.join(LABELS_FILE);
        let conn = match Connection::open(&db_path) {
            Ok(c)  => c,
            Err(e) => { eprintln!("[labels] open {}: {e}", db_path.display()); return None; }
        };
        if let Err(e) = conn.execute_batch(DDL) {
            eprintln!("[labels] DDL: {e}");
            return None;
        }
        // Best-effort migrations — each fails silently if the column already exists.
        let _ = conn.execute(MIGRATE_CONTEXT, []);
        let _ = conn.execute(MIGRATE_TEXT_EMBEDDING, []);
        let _ = conn.execute(MIGRATE_CONTEXT_EMBEDDING, []);
        let _ = conn.execute(MIGRATE_EMBEDDING_MODEL, []);
        Some(Self { conn })
    }

    /// Return the total number of labels in the database.
    pub fn count(&self) -> u64 {
        self.conn
            .query_row("SELECT COUNT(*) FROM labels", [], |row| row.get::<_, i64>(0))
            .unwrap_or(0) as u64
    }

    /// Return the `n` most recently created labels, newest first.
    pub fn recent(&self, n: usize) -> Vec<LabelRow> {
        let mut stmt = match self.conn.prepare(
            "SELECT id, eeg_start, eeg_end, text, context, created_at
             FROM labels ORDER BY created_at DESC LIMIT ?1"
        ) {
            Ok(s)  => s,
            Err(e) => { eprintln!("[labels] recent: {e}"); return vec![]; }
        };
        let rows = match stmt.query_map([n as i64], |row| {
            Ok(LabelRow {
                id:         row.get(0)?,
                eeg_start:  row.get::<_, i64>(1)? as u64,
                eeg_end:    row.get::<_, i64>(2)? as u64,
                text:       row.get(3)?,
                context:    row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                created_at: row.get::<_, i64>(5)? as u64,
            })
        }) {
            Ok(r)  => r,
            Err(e) => { eprintln!("[labels] recent map: {e}"); return vec![]; }
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Insert a label row.  Returns the new `rowid` on success.
    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        &self,
        eeg_start:   u64,
        eeg_end:     u64,
        label_start: u64,
        label_end:   u64,
        text:        &str,
        context:     &str,
        created_at:  u64,
    ) -> Option<i64> {
        let r = self.conn.execute(
            "INSERT INTO labels
             (eeg_start, eeg_end, label_start, label_end, text, context, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                eeg_start as i64, eeg_end as i64,
                label_start as i64, label_end as i64,
                text, context, created_at as i64,
            ],
        );
        match r {
            Ok(_)  => Some(self.conn.last_insert_rowid()),
            Err(e) => { eprintln!("[labels] insert: {e}"); None }
        }
    }

    /// Return every label in the database, newest first.
    pub fn list_all(&self) -> Vec<LabelRow> {
        let mut stmt = match self.conn.prepare(
            "SELECT id, eeg_start, eeg_end, text, context, created_at
             FROM labels ORDER BY created_at DESC"
        ) {
            Ok(s)  => s,
            Err(e) => { eprintln!("[labels] list_all: {e}"); return vec![]; }
        };
        let rows = match stmt.query_map([], |row| {
            Ok(LabelRow {
                id:         row.get(0)?,
                eeg_start:  row.get::<_, i64>(1)? as u64,
                eeg_end:    row.get::<_, i64>(2)? as u64,
                text:       row.get(3)?,
                context:    row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                created_at: row.get::<_, i64>(5)? as u64,
            })
        }) {
            Ok(r)  => r,
            Err(e) => { eprintln!("[labels] list_all map: {e}"); return vec![]; }
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Update the text and context of an existing label by id.
    pub fn update_text(&self, id: i64, new_text: &str, new_context: &str) -> bool {
        match self.conn.execute(
            "UPDATE labels SET text = ?1, context = ?2 WHERE id = ?3",
            params![new_text, new_context, id],
        ) {
            Ok(n) => n > 0,
            Err(e) => { eprintln!("[labels] update_text: {e}"); false }
        }
    }

    /// Persist pre-computed embeddings for a label row, recording which model
    /// produced them.  Embeddings are stored as little-endian f32 byte arrays.
    pub fn update_embeddings(
        &self,
        id:              i64,
        text_emb:        &[f32],
        context_emb:     &[f32],
        model_code:      &str,
    ) -> bool {
        let text_blob:    Vec<u8> = crate::util::f32_to_blob(text_emb);
        let context_blob: Vec<u8> = crate::util::f32_to_blob(context_emb);
        match self.conn.execute(
            "UPDATE labels \
             SET text_embedding = ?1, context_embedding = ?2, embedding_model = ?3 \
             WHERE id = ?4",
            params![text_blob, context_blob, model_code, id],
        ) {
            Ok(n) => n > 0,
            Err(e) => { eprintln!("[labels] update_embeddings: {e}"); false }
        }
    }

    /// Return ids + text + context for rows that have never been embedded
    /// OR were embedded with a different model than `current_model`.
    pub fn rows_needing_embed(&self, current_model: &str) -> Vec<(i64, String, String)> {
        let mut stmt = match self.conn.prepare(
            "SELECT id, text, context FROM labels \
             WHERE text_embedding IS NULL OR embedding_model IS NULL OR embedding_model != ?1"
        ) {
            Ok(s)  => s,
            Err(e) => { eprintln!("[labels] rows_needing_embed: {e}"); return vec![]; }
        };
        stmt.query_map(params![current_model], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Return ALL rows — used by the explicit "re-embed everything" command.
    pub fn all_rows_for_embed(&self) -> Vec<(i64, String, String)> {
        let mut stmt = match self.conn.prepare(
            "SELECT id, text, context FROM labels"
        ) {
            Ok(s)  => s,
            Err(e) => { eprintln!("[labels] all_rows_for_embed: {e}"); return vec![]; }
        };
        stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Delete a label by id.
    pub fn delete(&self, id: i64) -> bool {
        match self.conn.execute("DELETE FROM labels WHERE id = ?1", params![id]) {
            Ok(n) => n > 0,
            Err(e) => { eprintln!("[labels] delete: {e}"); false }
        }
    }

    /// Return all labels whose EEG window overlaps [`from`, `to`] (unix seconds).
    pub fn query_range(&self, from: u64, to: u64) -> Vec<LabelRow> {
        // prepare_cached reuses the compiled statement across calls — avoids
        // re-parsing the SQL once per session when hydrating a full history load.
        let mut stmt = match self.conn.prepare_cached(
            "SELECT id, eeg_start, eeg_end, text, context, created_at
             FROM labels
             WHERE eeg_end >= ?1 AND eeg_start <= ?2
             ORDER BY eeg_start"
        ) {
            Ok(s)  => s,
            Err(e) => { eprintln!("[labels] query_range: {e}"); return vec![]; }
        };
        let rows = match stmt.query_map(params![from as i64, to as i64], |row| {
            Ok(LabelRow {
                id:         row.get(0)?,
                eeg_start:  row.get::<_, i64>(1)? as u64,
                eeg_end:    row.get::<_, i64>(2)? as u64,
                text:       row.get(3)?,
                context:    row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                created_at: row.get::<_, i64>(5)? as u64,
            })
        }) {
            Ok(r)  => r,
            Err(e) => { eprintln!("[labels] query_range map: {e}"); return vec![]; }
        };
        rows.filter_map(|r| r.ok()).collect()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LabelRow {
    pub id:         i64,
    pub eeg_start:  u64,
    pub eeg_end:    u64,
    pub text:       String,
    pub context:    String,
    pub created_at: u64,
}

// ── Tests ──────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{TempDir, tempdir};

    fn open_temp() -> (LabelStore, TempDir) {
        let dir = tempdir().expect("tempdir");
        let store = LabelStore::open(dir.path()).expect("LabelStore::open");
        (store, dir)
    }

    #[test]
    fn insert_and_count() {
        let (store, _dir) = open_temp();
        assert_eq!(store.count(), 0);
        let id = store.insert(100, 200, 100, 200, "alpha", "", 1_700_000_000).unwrap();
        assert!(id > 0);
        assert_eq!(store.count(), 1);
    }

    #[test]
    fn list_all_order() {
        let (store, _dir) = open_temp();
        store.insert(100, 200, 100, 200, "first",  "", 1_000).unwrap();
        store.insert(200, 300, 200, 300, "second", "", 2_000).unwrap();
        let rows = store.list_all();
        // newest first
        assert_eq!(rows[0].text, "second");
        assert_eq!(rows[1].text, "first");
    }

    #[test]
    fn update_text_roundtrip() {
        let (store, _dir) = open_temp();
        let id = store.insert(10, 20, 10, 20, "original", "", 999).unwrap();
        assert!(store.update_text(id, "updated", "some context"));
        let rows = store.list_all();
        assert_eq!(rows[0].text, "updated");
        assert_eq!(rows[0].context, "some context");
    }

    #[test]
    fn update_nonexistent_returns_false() {
        let (store, _dir) = open_temp();
        assert!(!store.update_text(9999, "x", ""));
    }

    #[test]
    fn delete_removes_row() {
        let (store, _dir) = open_temp();
        let id = store.insert(10, 20, 10, 20, "to_delete", "", 1).unwrap();
        assert_eq!(store.count(), 1);
        assert!(store.delete(id));
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let (store, _dir) = open_temp();
        assert!(!store.delete(9999));
    }

    #[test]
    fn query_range_filters_correctly() {
        let (store, _dir) = open_temp();
        store.insert(100, 200, 100, 200, "in_range",     "", 1).unwrap();
        store.insert(500, 600, 500, 600, "out_of_range", "", 2).unwrap();
        let rows = store.query_range(50, 250);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "in_range");
    }
}
