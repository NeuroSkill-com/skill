// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Persistent audit log for hook trigger events.
//!
//! ## Storage
//!
//! `~/.skill/hooks.sqlite` — a single flat SQLite database that records every
//! hook fire.  Each row stores full JSON snapshots of the rule and the trigger
//! context so the record remains meaningful even after the user changes or
//! deletes the hook configuration.
//!
//! ## Schema (`hook_events` table)
//!
//! | column            | type    | notes |
//! |-------------------|---------|-------|
//! | `id`              | INTEGER | PRIMARY KEY AUTOINCREMENT |
//! | `triggered_at_utc`| INTEGER | `YYYYMMDDHHmmss` UTC |
//! | `hook_json`       | TEXT    | Full copy of `HookRule` at trigger time |
//! | `trigger_json`    | TEXT    | `HookLastTrigger` + EEG distance details |
//! | `payload_json`    | TEXT    | What was dispatched (command / WS payload) |

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── Public types ──────────────────────────────────────────────────────────────

/// One row returned by [`HooksLog::query`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookLogRow {
    pub id:               i64,
    pub triggered_at_utc: i64,
    /// Full copy of the `HookRule` serialised as JSON.
    pub hook_json:        String,
    /// `HookLastTrigger` plus extra context (label_text, distance) as JSON.
    pub trigger_json:     String,
    /// Dispatched payload (command, text, ws context) as JSON.
    pub payload_json:     String,
}

/// A fire event to be appended by the caller.
pub struct HookFireEntry<'a> {
    pub triggered_at_utc: i64,
    pub hook_json:        &'a str,
    pub trigger_json:     &'a str,
    pub payload_json:     &'a str,
}

// ── Main store ────────────────────────────────────────────────────────────────

/// Wrapper around the `hooks.sqlite` audit-log database.
///
/// The connection uses WAL mode so concurrent readers (the UI) never block
/// the embed-worker thread that is writing.
pub struct HooksLog {
    conn: rusqlite::Connection,
    #[allow(dead_code)]
    path: PathBuf,
}

impl HooksLog {
    /// Open (or create) `hooks.sqlite` inside `skill_dir`.
    ///
    /// Returns `None` on failure so callers can continue without logging.
    pub fn open(skill_dir: &Path) -> Option<Self> {
        let path = skill_dir.join(skill_constants::HOOKS_LOG_FILE);
        let conn = rusqlite::Connection::open(&path)
            .map_err(|e| eprintln!("[hooks_log] open {}: {e}", path.display()))
            .ok()?;

        crate::util::init_wal_pragmas(&conn);

        let ddl = "
            CREATE TABLE IF NOT EXISTS hook_events (
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                triggered_at_utc  INTEGER NOT NULL,
                hook_json         TEXT    NOT NULL,
                trigger_json      TEXT    NOT NULL,
                payload_json      TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_hook_events_ts
                ON hook_events (triggered_at_utc DESC);
        ";
        conn.execute_batch(ddl)
            .map_err(|e| eprintln!("[hooks_log] DDL: {e}"))
            .ok()?;

        Some(Self { conn, path })
    }

    /// Append one fire event to the log.  Silently ignores write errors.
    pub fn record(&self, entry: HookFireEntry<'_>) {
        let r = self.conn.execute(
            "INSERT INTO hook_events
             (triggered_at_utc, hook_json, trigger_json, payload_json)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                entry.triggered_at_utc,
                entry.hook_json,
                entry.trigger_json,
                entry.payload_json,
            ],
        );
        if let Err(e) = r {
            eprintln!("[hooks_log] insert: {e}");
        }
    }

    /// Return the most-recent `limit` rows, skipping the first `offset`.
    pub fn query(&self, limit: i64, offset: i64) -> Vec<HookLogRow> {
        let mut stmt = match self.conn.prepare(
            "SELECT id, triggered_at_utc, hook_json, trigger_json, payload_json
             FROM hook_events
             ORDER BY triggered_at_utc DESC
             LIMIT ?1 OFFSET ?2",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[hooks_log] prepare query: {e}");
                return vec![];
            }
        };

        stmt.query_map(rusqlite::params![limit, offset], |row| {
            Ok(HookLogRow {
                id:               row.get(0)?,
                triggered_at_utc: row.get(1)?,
                hook_json:        row.get(2)?,
                trigger_json:     row.get(3)?,
                payload_json:     row.get(4)?,
            })
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    /// Return the total number of logged events.
    pub fn count(&self) -> i64 {
        self.conn
            .query_row("SELECT COUNT(*) FROM hook_events", [], |row| row.get(0))
            .unwrap_or(0)
    }
}
