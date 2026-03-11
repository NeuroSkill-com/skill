// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Persistent LLM chat history — `~/.skill/chat_history.sqlite`.
//!
//! Schema
//! ------
//! ```text
//! chat_sessions
//!   id         INTEGER PRIMARY KEY AUTOINCREMENT
//!   created_at INTEGER NOT NULL   -- unix milliseconds (UTC)
//!   model_name TEXT    NOT NULL DEFAULT ''
//!
//! chat_messages
//!   id         INTEGER PRIMARY KEY AUTOINCREMENT
//!   session_id INTEGER NOT NULL REFERENCES chat_sessions(id)
//!   role       TEXT    NOT NULL   -- 'user' | 'assistant'
//!   content    TEXT    NOT NULL
//!   thinking   TEXT              -- chain-of-thought (nullable)
//!   created_at INTEGER NOT NULL   -- unix milliseconds (UTC)
//! ```

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

const DDL: &str = "
    CREATE TABLE IF NOT EXISTS chat_sessions (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        created_at  INTEGER NOT NULL,
        model_name  TEXT    NOT NULL DEFAULT ''
    );
    CREATE TABLE IF NOT EXISTS chat_messages (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        session_id  INTEGER NOT NULL REFERENCES chat_sessions(id),
        role        TEXT    NOT NULL,
        content     TEXT    NOT NULL,
        thinking    TEXT,
        created_at  INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_chat_msg_session
        ON chat_messages (session_id);
";

/// A single persisted chat message returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id:         i64,
    pub session_id: i64,
    pub role:       String,
    pub content:    String,
    pub thinking:   Option<String>,
    pub created_at: i64,
}

/// Thin wrapper around a rusqlite [`Connection`] for chat history I/O.
pub struct ChatStore {
    conn: Connection,
}

impl ChatStore {
    /// Open (or create) the chat history database inside `skill_dir`.
    /// Returns `None` on any error so callers can degrade gracefully.
    pub fn open(skill_dir: &Path) -> Option<Self> {
        let db_path = skill_dir.join("chat_history.sqlite");
        let conn = match Connection::open(&db_path) {
            Ok(c)  => c,
            Err(e) => {
                eprintln!("[chat_store] failed to open {}: {e}", db_path.display());
                return None;
            }
        };
        if let Err(e) = conn.execute_batch(DDL) {
            eprintln!("[chat_store] DDL error: {e}");
            return None;
        }
        Some(ChatStore { conn })
    }

    /// Return the `id` of the most recent session, creating a fresh one if
    /// none exists yet.
    pub fn get_or_create_last_session(&mut self) -> i64 {
        let existing: Option<i64> = self.conn
            .query_row(
                "SELECT id FROM chat_sessions ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();
        existing.unwrap_or_else(|| self.new_session_inner(""))
    }

    /// Create a new session and return its `id`.
    pub fn new_session(&mut self) -> i64 {
        self.new_session_inner("")
    }

    fn new_session_inner(&mut self, model_name: &str) -> i64 {
        let now = unix_ms();
        self.conn
            .execute(
                "INSERT INTO chat_sessions (created_at, model_name) VALUES (?1, ?2)",
                params![now, model_name],
            )
            .ok();
        self.conn.last_insert_rowid()
    }

    /// Append a message to the given session.  Returns the new row id.
    pub fn save_message(
        &mut self,
        session_id: i64,
        role:       &str,
        content:    &str,
        thinking:   Option<&str>,
    ) -> i64 {
        let now = unix_ms();
        self.conn
            .execute(
                "INSERT INTO chat_messages \
                 (session_id, role, content, thinking, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![session_id, role, content, thinking, now],
            )
            .ok();
        self.conn.last_insert_rowid()
    }

    /// Load all messages for a session in insertion order.
    pub fn load_session(&mut self, session_id: i64) -> Vec<StoredMessage> {
        let mut stmt = match self.conn.prepare(
            "SELECT id, session_id, role, content, thinking, created_at \
             FROM chat_messages WHERE session_id = ?1 ORDER BY id ASC",
        ) {
            Ok(s)  => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map(params![session_id], |row| {
            Ok(StoredMessage {
                id:         row.get(0)?,
                session_id: row.get(1)?,
                role:       row.get(2)?,
                content:    row.get(3)?,
                thinking:   row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }
}

fn unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
