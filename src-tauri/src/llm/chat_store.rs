// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Persistent LLM chat history — `~/.skill/chats/chat_history.sqlite`.
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
//!
//! chat_tool_calls
//!   id           INTEGER PRIMARY KEY AUTOINCREMENT
//!   message_id   INTEGER NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE
//!   tool         TEXT    NOT NULL
//!   status       TEXT    NOT NULL
//!   detail       TEXT
//!   tool_call_id TEXT
//!   args         TEXT              -- JSON-encoded arguments
//!   result       TEXT              -- JSON-encoded result
//!   created_at   INTEGER NOT NULL   -- unix milliseconds (UTC)
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
    CREATE TABLE IF NOT EXISTS chat_tool_calls (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        message_id   INTEGER NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
        tool         TEXT    NOT NULL,
        status       TEXT    NOT NULL,
        detail       TEXT,
        tool_call_id TEXT,
        args         TEXT,
        result       TEXT,
        created_at   INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_tool_calls_message
        ON chat_tool_calls (message_id);
";

/// A single persisted tool call returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToolCall {
    pub id:           i64,
    pub message_id:   i64,
    pub tool:         String,
    pub status:       String,
    pub detail:       Option<String>,
    pub tool_call_id: Option<String>,
    pub args:         Option<serde_json::Value>,
    pub result:       Option<serde_json::Value>,
    pub created_at:   i64,
}

/// Input struct for saving a new tool call (no auto-generated fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewToolCall {
    pub tool:         String,
    pub status:       String,
    pub detail:       Option<String>,
    pub tool_call_id: Option<String>,
    pub args:         Option<serde_json::Value>,
    pub result:       Option<serde_json::Value>,
}

/// A single persisted chat message returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id:         i64,
    pub session_id: i64,
    pub role:       String,
    pub content:    String,
    pub thinking:   Option<String>,
    pub created_at: i64,
    /// Tool calls associated with this message (populated on load).
    #[serde(default)]
    pub tool_calls: Vec<StoredToolCall>,
}

/// Summary of one session — used by the sidebar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id:            i64,
    /// User-supplied title, or empty if auto-titled / untitled.
    pub title:         String,
    /// First 80 chars of the first user message (for sidebar preview).
    pub preview:       String,
    pub created_at:    i64,
    pub message_count: i64,
}

/// Thin wrapper around a rusqlite [`Connection`] for chat history I/O.
pub struct ChatStore {
    conn: Connection,
}

impl ChatStore {
    /// Open (or create) the chat history database inside `skill_dir/chats/`.
    /// Returns `None` on any error so callers can degrade gracefully.
    ///
    /// If a legacy `chat_history.sqlite` exists directly in `skill_dir`
    /// (pre-migration location) it is moved into the new `chats/` subdirectory
    /// automatically.
    pub fn open(skill_dir: &Path) -> Option<Self> {
        let chats_dir = skill_dir.join("chats");
        if let Err(e) = std::fs::create_dir_all(&chats_dir) {
            eprintln!("[chat_store] failed to create {}: {e}", chats_dir.display());
            return None;
        }

        // Migrate legacy DB from skill_dir root into chats/ subdirectory.
        let legacy_path = skill_dir.join("chat_history.sqlite");
        let db_path     = chats_dir.join("chat_history.sqlite");
        if legacy_path.exists() && !db_path.exists() {
            if let Err(e) = std::fs::rename(&legacy_path, &db_path) {
                eprintln!(
                    "[chat_store] migration rename {} -> {} failed: {e}",
                    legacy_path.display(),
                    db_path.display()
                );
                // Fall through — we'll create a fresh DB at the new path.
            } else {
                // Also move WAL/SHM sidecar files if they exist.
                for suffix in &["-wal", "-shm"] {
                    let src = skill_dir.join(format!("chat_history.sqlite{suffix}"));
                    let dst = chats_dir.join(format!("chat_history.sqlite{suffix}"));
                    let _ = std::fs::rename(&src, &dst);
                }
                eprintln!("[chat_store] migrated legacy DB to {}", db_path.display());
            }
        }

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
        // Migration: add title column if it doesn't exist yet (existing databases).
        // Silently ignored if the column is already present.
        let _ = conn.execute_batch(
            "ALTER TABLE chat_sessions ADD COLUMN title TEXT NOT NULL DEFAULT '';",
        );
        // Migration: add archived column (0 = active, 1 = archived).
        let _ = conn.execute_batch(
            "ALTER TABLE chat_sessions ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;",
        );
        Some(ChatStore { conn })
    }

    // ── Session list / rename / delete ────────────────────────────────────────

    /// Return all non-archived sessions newest-first, with preview text and message count.
    pub fn list_sessions(&mut self) -> Vec<SessionSummary> {
        let mut stmt = match self.conn.prepare(
            "SELECT
                 s.id,
                 COALESCE(s.title, '') AS title,
                 COALESCE(
                     SUBSTR(
                         (SELECT content FROM chat_messages
                          WHERE session_id = s.id AND role = 'user'
                          ORDER BY id ASC LIMIT 1),
                         1, 80
                     ), ''
                 ) AS preview,
                 s.created_at,
                 (SELECT COUNT(*) FROM chat_messages WHERE session_id = s.id)
                     AS message_count
             FROM chat_sessions s
             WHERE COALESCE(s.archived, 0) = 0
             ORDER BY s.id DESC
             LIMIT 300",
        ) {
            Ok(s)  => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |row| {
            Ok(SessionSummary {
                id:            row.get(0)?,
                title:         row.get(1)?,
                preview:       row.get(2)?,
                created_at:    row.get(3)?,
                message_count: row.get(4)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Set a custom title for a session.
    pub fn rename_session(&mut self, id: i64, title: &str) {
        let _ = self.conn.execute(
            "UPDATE chat_sessions SET title = ?1 WHERE id = ?2",
            params![title, id],
        );
    }

    /// Delete a session and all its messages.
    pub fn delete_session(&mut self, id: i64) {
        let _ = self.conn.execute(
            "DELETE FROM chat_messages WHERE session_id = ?1", params![id],
        );
        let _ = self.conn.execute(
            "DELETE FROM chat_sessions WHERE id = ?1", params![id],
        );
    }

    /// Archive a session (soft-delete).
    pub fn archive_session(&mut self, id: i64) {
        let _ = self.conn.execute(
            "UPDATE chat_sessions SET archived = 1 WHERE id = ?1",
            params![id],
        );
    }

    /// Unarchive (restore) a session.
    pub fn unarchive_session(&mut self, id: i64) {
        let _ = self.conn.execute(
            "UPDATE chat_sessions SET archived = 0 WHERE id = ?1",
            params![id],
        );
    }

    /// Return all archived sessions newest-first.
    pub fn list_archived_sessions(&mut self) -> Vec<SessionSummary> {
        let mut stmt = match self.conn.prepare(
            "SELECT
                 s.id,
                 COALESCE(s.title, '') AS title,
                 COALESCE(
                     SUBSTR(
                         (SELECT content FROM chat_messages
                          WHERE session_id = s.id AND role = 'user'
                          ORDER BY id ASC LIMIT 1),
                         1, 80
                     ), ''
                 ) AS preview,
                 s.created_at,
                 (SELECT COUNT(*) FROM chat_messages WHERE session_id = s.id)
                     AS message_count
             FROM chat_sessions s
             WHERE s.archived = 1
             ORDER BY s.id DESC
             LIMIT 300",
        ) {
            Ok(s)  => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |row| {
            Ok(SessionSummary {
                id:            row.get(0)?,
                title:         row.get(1)?,
                preview:       row.get(2)?,
                created_at:    row.get(3)?,
                message_count: row.get(4)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
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
        self.save_message_with_tools(session_id, role, content, thinking, &[])
    }

    /// Append a message with associated tool calls to the given session.
    /// Returns the new message row id.
    pub fn save_message_with_tools(
        &mut self,
        session_id:  i64,
        role:        &str,
        content:     &str,
        thinking:    Option<&str>,
        tool_calls:  &[NewToolCall],
    ) -> i64 {
        let now = unix_ms();
        let msg_id = match self.conn.execute(
            "INSERT INTO chat_messages \
             (session_id, role, content, thinking, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![session_id, role, content, thinking, now],
        ) {
            Ok(rows) => {
                let id = self.conn.last_insert_rowid();
                eprintln!("[chat_store] save_message OK: session={session_id} role={role} rows={rows} id={id} content_len={}", content.len());
                id
            }
            Err(e) => {
                eprintln!("[chat_store] save_message FAILED: session={session_id} role={role} error={e}");
                return 0;
            }
        };

        // Persist associated tool calls.
        for tc in tool_calls {
            let args_json   = tc.args.as_ref().map(|v| v.to_string());
            let result_json = tc.result.as_ref().map(|v| v.to_string());
            if let Err(e) = self.conn.execute(
                "INSERT INTO chat_tool_calls \
                 (message_id, tool, status, detail, tool_call_id, args, result, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![msg_id, tc.tool, tc.status, tc.detail, tc.tool_call_id,
                        args_json, result_json, now],
            ) {
                eprintln!("[chat_store] save_tool_call FAILED: msg={msg_id} tool={} error={e}", tc.tool);
            }
        }

        msg_id
    }

    /// Save tool calls associated with a message.  `message_id` must reference
    /// an existing row in `chat_messages`.
    pub fn save_tool_calls(&mut self, message_id: i64, tool_calls: &[StoredToolCall]) {
        let now = unix_ms();
        for tc in tool_calls {
            let args_json   = tc.args.as_ref().map(|v| v.to_string());
            let result_json = tc.result.as_ref().map(|v| v.to_string());
            if let Err(e) = self.conn.execute(
                "INSERT INTO chat_tool_calls \
                 (message_id, tool, status, detail, tool_call_id, args, result, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    message_id,
                    tc.tool,
                    tc.status,
                    tc.detail,
                    tc.tool_call_id,
                    args_json,
                    result_json,
                    now,
                ],
            ) {
                eprintln!("[chat_store] save_tool_call FAILED: message_id={message_id} tool={} error={e}", tc.tool);
            }
        }
    }

    /// Load all messages for a session in insertion order, including tool calls.
    pub fn load_session(&mut self, session_id: i64) -> Vec<StoredMessage> {
        let mut stmt = match self.conn.prepare(
            "SELECT id, session_id, role, content, thinking, created_at \
             FROM chat_messages WHERE session_id = ?1 ORDER BY id ASC",
        ) {
            Ok(s)  => s,
            Err(_) => return Vec::new(),
        };
        let mut messages: Vec<StoredMessage> = stmt.query_map(params![session_id], |row| {
            Ok(StoredMessage {
                id:         row.get(0)?,
                session_id: row.get(1)?,
                role:       row.get(2)?,
                content:    row.get(3)?,
                thinking:   row.get(4)?,
                created_at: row.get(5)?,
                tool_calls: Vec::new(),
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

        // Load tool calls for all messages in this session in one query.
        let msg_ids: Vec<i64> = messages.iter().map(|m| m.id).collect();
        if !msg_ids.is_empty() {
            // Build a parameterised IN clause.
            let placeholders: Vec<String> = (1..=msg_ids.len()).map(|i| format!("?{i}")).collect();
            let sql = format!(
                "SELECT id, message_id, tool, status, detail, tool_call_id, args, result, created_at \
                 FROM chat_tool_calls WHERE message_id IN ({}) ORDER BY id ASC",
                placeholders.join(", ")
            );
            if let Ok(mut tc_stmt) = self.conn.prepare(&sql) {
                let params_vec: Vec<&dyn rusqlite::types::ToSql> =
                    msg_ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
                if let Ok(rows) = tc_stmt.query_map(params_vec.as_slice(), |row| {
                    let args_str: Option<String>   = row.get(6)?;
                    let result_str: Option<String>  = row.get(7)?;
                    Ok(StoredToolCall {
                        id:           row.get(0)?,
                        message_id:   row.get(1)?,
                        tool:         row.get(2)?,
                        status:       row.get(3)?,
                        detail:       row.get(4)?,
                        tool_call_id: row.get(5)?,
                        args:         args_str.and_then(|s| serde_json::from_str(&s).ok()),
                        result:       result_str.and_then(|s| serde_json::from_str(&s).ok()),
                        created_at:   row.get(8)?,
                    })
                }) {
                    // Build a map from message_id → Vec<StoredToolCall>
                    let mut tc_map: std::collections::HashMap<i64, Vec<StoredToolCall>> =
                        std::collections::HashMap::new();
                    for tc in rows.filter_map(|r| r.ok()) {
                        tc_map.entry(tc.message_id).or_default().push(tc);
                    }
                    for msg in &mut messages {
                        if let Some(tcs) = tc_map.remove(&msg.id) {
                            msg.tool_calls = tcs;
                        }
                    }
                }
            }
        }

        messages
    }
}

fn unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_save_and_load_messages() {
        let tmp = std::env::temp_dir().join("skill_chat_store_test");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::create_dir_all(&tmp);

        let mut store = ChatStore::open(&tmp).expect("failed to open store");

        // Create a session
        let session_id = store.new_session();
        assert!(session_id > 0, "session id should be positive, got {}", session_id);

        // Save a user message
        let msg_id = store.save_message(session_id, "user", "Hello world", None);
        assert!(msg_id > 0, "user msg id should be positive, got {}", msg_id);

        // Save an assistant message with thinking
        let msg_id2 = store.save_message(session_id, "assistant", "Hi there!", Some("thinking..."));
        assert!(msg_id2 > msg_id, "assistant msg id should be greater");

        // Load and verify
        let msgs = store.load_session(session_id);
        assert_eq!(msgs.len(), 2, "expected 2 messages, got {}", msgs.len());
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "Hello world");
        assert!(msgs[0].thinking.is_none());
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].content, "Hi there!");
        assert_eq!(msgs[1].thinking.as_deref(), Some("thinking..."));

        // Save with thinking = None (like the frontend does)
        let msg_id3 = store.save_message(session_id, "user", "test message", None);
        assert!(msg_id3 > 0);
        let msgs = store.load_session(session_id);
        assert_eq!(msgs.len(), 3);

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
