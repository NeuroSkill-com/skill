// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Persistent LLM chat history — `~/.skill/chats/chat_history.duckdb`.
//!
//! Schema
//! ------
//! ```text
//! chat_sessions
//!   id         BIGINT  PRIMARY KEY  -- chat_sessions_id_seq
//!   created_at BIGINT  NOT NULL     -- unix milliseconds (UTC)
//!   model_name VARCHAR NOT NULL DEFAULT ''
//!   title      VARCHAR NOT NULL DEFAULT ''
//!   archived   BIGINT  NOT NULL DEFAULT 0
//!   params     VARCHAR NOT NULL DEFAULT ''  -- JSON generation params
//!
//! chat_messages
//!   id         BIGINT  PRIMARY KEY  -- chat_messages_id_seq
//!   session_id BIGINT  NOT NULL     -- chat_sessions.id
//!   role       TEXT    NOT NULL   -- 'user' | 'assistant'
//!   content    TEXT    NOT NULL
//!   thinking   TEXT              -- chain-of-thought (nullable)
//!   created_at BIGINT  NOT NULL   -- unix milliseconds (UTC)
//!
//! chat_tool_calls
//!   id           BIGINT  PRIMARY KEY  -- chat_tool_calls_id_seq
//!   message_id   BIGINT  NOT NULL     -- chat_messages.id
//!   tool         TEXT    NOT NULL
//!   status       TEXT    NOT NULL
//!   detail       TEXT
//!   tool_call_id TEXT
//!   args         TEXT              -- JSON-encoded arguments
//!   result       TEXT              -- JSON-encoded result
//!   created_at   BIGINT  NOT NULL   -- unix milliseconds (UTC)
//! ```

use serde::{Deserialize, Serialize};
use skill_db::params;
use std::path::Path;

const SCHEMA: skill_db::StoreSchema<'static> = skill_db::StoreSchema {
    ddl: "
    CREATE TABLE IF NOT EXISTS chat_sessions (
        id          BIGINT  PRIMARY KEY DEFAULT nextval('chat_sessions_id_seq'),
        created_at  BIGINT  NOT NULL,
        model_name  VARCHAR NOT NULL DEFAULT '',
        title       VARCHAR NOT NULL DEFAULT '',
        archived    BIGINT  NOT NULL DEFAULT 0,
        params      VARCHAR NOT NULL DEFAULT ''
    );
    CREATE TABLE IF NOT EXISTS chat_messages (
        id          BIGINT  PRIMARY KEY DEFAULT nextval('chat_messages_id_seq'),
        session_id  BIGINT  NOT NULL,
        role        VARCHAR NOT NULL,
        content     VARCHAR NOT NULL,
        thinking    VARCHAR,
        created_at  BIGINT  NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_chat_msg_session
        ON chat_messages (session_id);
    CREATE TABLE IF NOT EXISTS chat_tool_calls (
        id           BIGINT  PRIMARY KEY DEFAULT nextval('chat_tool_calls_id_seq'),
        message_id   BIGINT  NOT NULL,
        tool         VARCHAR NOT NULL,
        status       VARCHAR NOT NULL,
        detail       VARCHAR,
        tool_call_id VARCHAR,
        args         VARCHAR,
        result       VARCHAR,
        created_at   BIGINT  NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_tool_calls_message
        ON chat_tool_calls (message_id);
",
    import_tables: &["chat_sessions", "chat_messages", "chat_tool_calls"],
    sequences: &[
        ("chat_sessions_id_seq", "chat_sessions", "id"),
        ("chat_messages_id_seq", "chat_messages", "id"),
        ("chat_tool_calls_id_seq", "chat_tool_calls", "id"),
    ],
};

/// A single persisted tool call returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToolCall {
    pub id: i64,
    pub message_id: i64,
    pub tool: String,
    pub status: String,
    pub detail: Option<String>,
    pub tool_call_id: Option<String>,
    pub args: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub created_at: i64,
}

/// Input struct for saving a new tool call (no auto-generated fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewToolCall {
    pub tool: String,
    pub status: String,
    pub detail: Option<String>,
    pub tool_call_id: Option<String>,
    pub args: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
}

/// A single persisted chat message returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: i64,
    pub session_id: i64,
    pub role: String,
    pub content: String,
    pub thinking: Option<String>,
    pub created_at: i64,
    /// Tool calls associated with this message (populated on load).
    #[serde(default)]
    pub tool_calls: Vec<StoredToolCall>,
}

/// Summary of one session — used by the sidebar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: i64,
    /// User-supplied title, or empty if auto-titled / untitled.
    pub title: String,
    /// First 80 chars of the first user message (for sidebar preview).
    pub preview: String,
    pub created_at: i64,
    pub message_count: i64,
}

/// Thin wrapper around a DuckDB [`Connection`] for chat history I/O.
pub struct ChatStore {
    conn: skill_db::Conn,
}

impl ChatStore {
    /// Open (or create) the chat history database inside `skill_dir/chats/`.
    /// Returns `None` on any error so callers can degrade gracefully.
    ///
    /// A pre-DuckDB `chats/chat_history.sqlite` (or the even older
    /// `skill_dir/chat_history.sqlite`) is imported once and kept as `.bak`.
    pub fn open(skill_dir: &Path) -> Option<Self> {
        let chats_dir = skill_dir.join("chats");
        if let Err(e) = std::fs::create_dir_all(&chats_dir) {
            llm_log!("chat_store", "failed to create {}: {e}", chats_dir.display());
            return None;
        }

        // Oldest layout: the SQLite file sat in skill_dir itself. Move it next
        // to the new database so the one-time DuckDB import picks it up.
        let db_path = chats_dir.join("chat_history.duckdb");
        let legacy_root = skill_dir.join("chat_history.sqlite");
        let legacy = skill_db::legacy_sqlite_path(&db_path);
        if legacy_root.exists() && !legacy.exists() && !db_path.exists() {
            if let Err(e) = std::fs::rename(&legacy_root, &legacy) {
                llm_log!(
                    "chat_store",
                    "migration rename {} -> {} failed: {e}",
                    legacy_root.display(),
                    legacy.display()
                );
            } else {
                for suffix in &["-wal", "-shm"] {
                    let src = skill_dir.join(format!("chat_history.sqlite{suffix}"));
                    let dst = chats_dir.join(format!("chat_history.sqlite{suffix}"));
                    let _ = std::fs::rename(&src, &dst);
                }
            }
        }

        match skill_db::open_store(&db_path, &SCHEMA) {
            Ok(conn) => Some(ChatStore { conn }),
            Err(e) => {
                llm_log!("chat_store", "failed to open {}: {e:#}", db_path.display());
                None
            }
        }
    }

    // ── Session list / rename / delete ────────────────────────────────────────

    /// Return all non-archived sessions newest-first, with preview text and message count.
    pub fn list_sessions(&mut self) -> Vec<SessionSummary> {
        let Ok(mut stmt) = self.conn.prepare(
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
        ) else {
            return Vec::new();
        };
        stmt.query_map([], |row| {
            Ok(SessionSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                preview: row.get(2)?,
                created_at: row.get(3)?,
                message_count: row.get(4)?,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Get per-session generation params as a JSON string (empty = use defaults).
    pub fn get_session_params(&self, id: i64) -> String {
        self.conn
            .query_row(
                "SELECT COALESCE(params, '') FROM chat_sessions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .unwrap_or_default()
    }

    /// Save per-session generation params (JSON string).
    pub fn set_session_params(&mut self, id: i64, params_json: &str) {
        let _ = self.conn.execute(
            "UPDATE chat_sessions SET params = ?1 WHERE id = ?2",
            params![params_json, id],
        );
    }

    /// Set a custom title for a session.
    pub fn rename_session(&mut self, id: i64, title: &str) {
        let _ = self
            .conn
            .execute("UPDATE chat_sessions SET title = ?1 WHERE id = ?2", params![title, id]);
    }

    /// Delete a session, its messages and their tool calls.
    pub fn delete_session(&mut self, id: i64) {
        let _ = self.conn.execute(
            "DELETE FROM chat_tool_calls WHERE message_id IN (SELECT id FROM chat_messages WHERE session_id = ?1)",
            params![id],
        );
        let _ = self
            .conn
            .execute("DELETE FROM chat_messages WHERE session_id = ?1", params![id]);
        let _ = self
            .conn
            .execute("DELETE FROM chat_sessions WHERE id = ?1", params![id]);
    }

    /// Archive a session (soft-delete).
    pub fn archive_session(&mut self, id: i64) {
        let _ = self
            .conn
            .execute("UPDATE chat_sessions SET archived = 1 WHERE id = ?1", params![id]);
    }

    /// Unarchive (restore) a session.
    pub fn unarchive_session(&mut self, id: i64) {
        let _ = self
            .conn
            .execute("UPDATE chat_sessions SET archived = 0 WHERE id = ?1", params![id]);
    }

    /// Return all archived sessions newest-first.
    pub fn list_archived_sessions(&mut self) -> Vec<SessionSummary> {
        let Ok(mut stmt) = self.conn.prepare(
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
        ) else {
            return Vec::new();
        };
        stmt.query_map([], |row| {
            Ok(SessionSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                preview: row.get(2)?,
                created_at: row.get(3)?,
                message_count: row.get(4)?,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
    }

    /// Return the `id` of the most recent session, creating a fresh one if
    /// none exists yet.
    pub fn get_or_create_last_session(&mut self) -> i64 {
        let existing: Option<i64> = self
            .conn
            .query_row("SELECT id FROM chat_sessions ORDER BY id DESC LIMIT 1", [], |row| {
                row.get(0)
            })
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
            .query_row(
                "INSERT INTO chat_sessions (created_at, model_name) VALUES (?1, ?2) RETURNING id",
                params![now, model_name],
                |row| row.get(0),
            )
            .unwrap_or(0)
    }

    /// Append a message to the given session.  Returns the new row id.
    pub fn save_message(&mut self, session_id: i64, role: &str, content: &str, thinking: Option<&str>) -> i64 {
        self.save_message_with_tools(session_id, role, content, thinking, &[])
    }

    /// Append a message with associated tool calls to the given session.
    /// Returns the new message row id.
    pub fn save_message_with_tools(
        &mut self,
        session_id: i64,
        role: &str,
        content: &str,
        thinking: Option<&str>,
        tool_calls: &[NewToolCall],
    ) -> i64 {
        let now = unix_ms();
        let msg_id = match self.conn.query_row(
            "INSERT INTO chat_messages \
             (session_id, role, content, thinking, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
            params![session_id, role, content, thinking, now],
            |row| row.get::<_, i64>(0),
        ) {
            Ok(id) => {
                llm_log!(
                    "chat_store",
                    "save_message OK: session={session_id} role={role} id={id} content_len={}",
                    content.len()
                );
                id
            }
            Err(e) => {
                llm_log!(
                    "chat_store",
                    "save_message FAILED: session={session_id} role={role} error={e}"
                );
                return 0;
            }
        };

        // Persist associated tool calls.
        for tc in tool_calls {
            let args_json = tc.args.as_ref().map(std::string::ToString::to_string);
            let result_json = tc.result.as_ref().map(std::string::ToString::to_string);
            if let Err(e) = self.conn.execute(
                "INSERT INTO chat_tool_calls \
                 (message_id, tool, status, detail, tool_call_id, args, result, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    msg_id,
                    tc.tool,
                    tc.status,
                    tc.detail,
                    tc.tool_call_id,
                    args_json,
                    result_json,
                    now
                ],
            ) {
                llm_log!(
                    "chat_store",
                    "save_tool_call FAILED: msg={msg_id} tool={} error={e}",
                    tc.tool
                );
            }
        }

        msg_id
    }

    /// Save tool calls associated with a message.  `message_id` must reference
    /// an existing row in `chat_messages`.
    pub fn save_tool_calls(&mut self, message_id: i64, tool_calls: &[StoredToolCall]) {
        let now = unix_ms();
        for tc in tool_calls {
            let args_json = tc.args.as_ref().map(std::string::ToString::to_string);
            let result_json = tc.result.as_ref().map(std::string::ToString::to_string);
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
                llm_log!(
                    "chat_store",
                    "save_tool_call FAILED: message_id={message_id} tool={} error={e}",
                    tc.tool
                );
            }
        }
    }

    /// Load all messages for a session in insertion order, including tool calls.
    pub fn load_session(&mut self, session_id: i64) -> Vec<StoredMessage> {
        let Ok(mut stmt) = self.conn.prepare(
            "SELECT id, session_id, role, content, thinking, created_at \
             FROM chat_messages WHERE session_id = ?1 ORDER BY id ASC",
        ) else {
            return Vec::new();
        };
        let mut messages: Vec<StoredMessage> = stmt
            .query_map(params![session_id], |row| {
                Ok(StoredMessage {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    thinking: row.get(4)?,
                    created_at: row.get(5)?,
                    tool_calls: Vec::new(),
                })
            })
            .map(|rows| rows.filter_map(std::result::Result::ok).collect())
            .unwrap_or_default();

        // Load tool calls for all messages in this session in one query.
        if !messages.is_empty() {
            let sql = "SELECT id, message_id, tool, status, detail, tool_call_id, args, result, created_at \
                       FROM chat_tool_calls \
                       WHERE message_id IN (SELECT id FROM chat_messages WHERE session_id = ?1) \
                       ORDER BY id ASC";
            if let Ok(mut tc_stmt) = self.conn.prepare(sql) {
                if let Ok(rows) = tc_stmt.query_map(params![session_id], |row| {
                    let args_str: Option<String> = row.get(6)?;
                    let result_str: Option<String> = row.get(7)?;
                    Ok(StoredToolCall {
                        id: row.get(0)?,
                        message_id: row.get(1)?,
                        tool: row.get(2)?,
                        status: row.get(3)?,
                        detail: row.get(4)?,
                        tool_call_id: row.get(5)?,
                        args: args_str.and_then(|s| serde_json::from_str(&s).ok()),
                        result: result_str.and_then(|s| serde_json::from_str(&s).ok()),
                        created_at: row.get(8)?,
                    })
                }) {
                    // Build a map from message_id → Vec<StoredToolCall>
                    let mut tc_map: std::collections::HashMap<i64, Vec<StoredToolCall>> =
                        std::collections::HashMap::new();
                    for tc in rows.filter_map(std::result::Result::ok) {
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

    fn temp_store() -> (PathBuf, ChatStore) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static CTR: AtomicU64 = AtomicU64::new(0);
        let id = CTR.fetch_add(1, Ordering::Relaxed);
        let tmp = std::env::temp_dir().join(format!("skill_chat_test_{}_{}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::create_dir_all(&tmp);
        let store = ChatStore::open(&tmp).expect("open store");
        (tmp, store)
    }

    #[test]
    fn list_sessions_empty() {
        let (tmp, mut store) = temp_store();
        let sessions = store.list_sessions();
        assert!(sessions.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn list_sessions_after_create() {
        let (tmp, mut store) = temp_store();
        let id1 = store.new_session();
        let id2 = store.new_session();
        let sessions = store.list_sessions();
        assert_eq!(sessions.len(), 2);
        // Newest first
        assert_eq!(sessions[0].id, id2);
        assert_eq!(sessions[1].id, id1);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn delete_session_removes_messages() {
        let (tmp, mut store) = temp_store();
        let sid = store.new_session();
        store.save_message(sid, "user", "test", None);
        store.save_message(sid, "assistant", "reply", None);
        assert_eq!(store.load_session(sid).len(), 2);

        store.delete_session(sid);
        assert_eq!(store.load_session(sid).len(), 0);
        assert!(!store.list_sessions().iter().any(|s| s.id == sid));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rename_session() {
        let (tmp, mut store) = temp_store();
        let sid = store.new_session();
        store.rename_session(sid, "My Chat");
        let sessions = store.list_sessions();
        assert_eq!(sessions[0].title, "My Chat");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn get_or_create_last_returns_existing() {
        let (tmp, mut store) = temp_store();
        let id1 = store.new_session();
        store.save_message(id1, "user", "first", None);
        let id2 = store.new_session();
        store.save_message(id2, "user", "second", None);
        let last = store.get_or_create_last_session();
        assert_eq!(last, id2);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_and_load_tool_calls() {
        let (tmp, mut store) = temp_store();
        let sid = store.new_session();
        let mid = store.save_message(sid, "assistant", "Let me search.", None);

        let tc = StoredToolCall {
            id: 0,
            message_id: mid,
            tool: "web_search".into(),
            status: "ok".into(),
            detail: Some("Found 5 results".into()),
            tool_call_id: Some("call_123".into()),
            args: Some(serde_json::json!({"query": "rust programming"})),
            result: Some(serde_json::json!({"count": 5})),
            created_at: 0,
        };
        store.save_tool_calls(mid, &[tc]);

        let msgs = store.load_session(sid);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].tool_calls.len(), 1);
        assert_eq!(msgs[0].tool_calls[0].tool, "web_search");
        assert_eq!(msgs[0].tool_calls[0].status, "ok");
        assert_eq!(msgs[0].tool_calls[0].tool_call_id.as_deref(), Some("call_123"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn archive_and_unarchive() {
        let (tmp, mut store) = temp_store();
        let sid = store.new_session();
        store.save_message(sid, "user", "test", None);

        // Initially not archived
        let active = store.list_sessions();
        assert_eq!(active.len(), 1);

        // Archive
        store.archive_session(sid);
        let active = store.list_sessions();
        assert!(active.is_empty());
        let archived = store.list_archived_sessions();
        assert_eq!(archived.len(), 1);

        // Unarchive
        store.unarchive_session(sid);
        let active = store.list_sessions();
        assert_eq!(active.len(), 1);
        let archived = store.list_archived_sessions();
        assert!(archived.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn session_params_roundtrip() {
        let (tmp, mut store) = temp_store();
        let sid = store.new_session();
        let params = r#"{"temperature":0.7,"top_p":0.9}"#;
        store.set_session_params(sid, params);
        let loaded = store.get_session_params(sid);
        assert_eq!(loaded, params);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
