// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Chat history persistence commands.

use std::sync::Mutex;

use crate::MutexExt;
use crate::AppState;

/// Payload returned by `get_last_chat_session`.
#[derive(serde::Serialize)]
pub struct ChatSessionResponse {
    pub session_id: i64,
    pub messages:   Vec<crate::llm::chat_store::StoredMessage>,
}

/// Return the most recent chat session and all its messages.
/// Creates a fresh empty session if none exists yet.
/// Returns an empty response if the chat store is unavailable.
#[tauri::command]
pub fn get_last_chat_session(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> ChatSessionResponse {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else {
        return ChatSessionResponse { session_id: 0, messages: vec![] };
    };
    let session_id = store.get_or_create_last_session();
    let messages   = store.load_session(session_id);
    ChatSessionResponse { session_id, messages }
}

/// Load a specific chat session by id.
#[tauri::command]
pub fn load_chat_session(
    id:    i64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> ChatSessionResponse {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else {
        return ChatSessionResponse { session_id: id, messages: vec![] };
    };
    let messages = store.load_session(id);
    ChatSessionResponse { session_id: id, messages }
}

/// Return all sessions (newest-first) for the sidebar.
#[tauri::command]
pub fn list_chat_sessions(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<crate::llm::chat_store::SessionSummary> {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return vec![]; };
    store.list_sessions()
}

/// Set a custom title for a session (called after auto-title or inline rename).
#[tauri::command]
pub fn rename_chat_session(
    id:    i64,
    title: String,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return; };
    store.rename_session(id, &title);
}

/// Delete a session and all its messages.
#[tauri::command]
pub fn delete_chat_session(
    id:    i64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return; };
    store.delete_session(id);
}

/// Archive a session (soft-delete — keeps data but hides from main list).
#[tauri::command]
pub fn archive_chat_session(
    id:    i64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return; };
    store.archive_session(id);
}

/// Restore an archived session back to the main list.
#[tauri::command]
pub fn unarchive_chat_session(
    id:    i64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return; };
    store.unarchive_session(id);
}

/// Return all archived sessions.
#[tauri::command]
pub fn list_archived_chat_sessions(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Vec<crate::llm::chat_store::SessionSummary> {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return vec![]; };
    store.list_archived_sessions()
}

/// Append a single message to a chat session.
/// Returns the new message row id, or 0 if the store is unavailable.
#[tauri::command]
pub fn save_chat_message(
    session_id: i64,
    role:       String,
    content:    String,
    thinking:   Option<String>,
    state:      tauri::State<'_, Mutex<Box<AppState>>>,
) -> i64 {
    eprintln!("[save_chat_message] called: session_id={session_id} role={role} content_len={}", content.len());
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else {
        eprintln!("[save_chat_message] chat_store is None!");
        return 0;
    };
    store.save_message(session_id, &role, &content, thinking.as_deref())
}

/// Get per-session generation params as a JSON string.
#[tauri::command]
pub fn get_session_params(
    id:    i64,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> String {
    let s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_ref() else { return String::new(); };
    store.get_session_params(id)
}

/// Save per-session generation params (JSON string).
#[tauri::command]
pub fn set_session_params(
    id:          i64,
    params_json: String,
    state:       tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return; };
    store.set_session_params(id, &params_json);
}

/// Create a fresh chat session and return its id.
/// Called when the user clicks "New Chat".
#[tauri::command]
pub fn new_chat_session(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> i64 {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return 0; };
    store.new_session()
}

/// Save tool calls associated with a chat message.
///
/// `message_id` must be the row id returned by `save_chat_message`.
/// `tool_calls` is a JSON array of objects matching `StoredToolCall` fields.
#[tauri::command]
pub fn save_chat_tool_calls(
    message_id: i64,
    tool_calls: Vec<crate::llm::chat_store::StoredToolCall>,
    state:      tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    let Some(store) = s.llm.chat_store.as_mut() else { return; };
    store.save_tool_calls(message_id, &tool_calls);
}
