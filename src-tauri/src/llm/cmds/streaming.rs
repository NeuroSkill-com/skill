// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! IPC chat streaming, abort, and tool-call cancellation commands.

use std::sync::Mutex;
use tauri::AppHandle;

use crate::MutexExt;
use crate::AppState;

// ── Chat chunk types ──────────────────────────────────────────────────────────

/// One message delivered through the Tauri IPC `Channel` for `chat_completions_ipc`.
///
/// Serialised as a tagged-union JSON object, e.g.:
/// ```json
/// {"type":"delta","content":"Hello"}
/// {"type":"done","finish_reason":"stop","prompt_tokens":42,"completion_tokens":18,"n_ctx":4096}
/// {"type":"error","message":"decode error"}
/// ```
/// An `"error"` with `message == "aborted"` means the caller invoked
/// `abort_llm_stream` — the frontend should treat partial content as the
/// final answer rather than showing an error.
#[derive(serde::Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatChunk {
    Delta    { content: String },
    /// Legacy event — still emitted for backwards compatibility.
    ToolUse  { tool: String, status: String, detail: Option<String> },
    /// Rich tool-execution lifecycle events (pi-mono style).
    ToolExecutionStart  { tool_call_id: String, tool_name: String, args: serde_json::Value },
    ToolExecutionEnd    { tool_call_id: String, tool_name: String, result: serde_json::Value, is_error: bool },
    /// A tool call was cancelled by the user.
    ToolCancelled       { tool_call_id: String, tool_name: String },
    Done     { finish_reason: String, prompt_tokens: usize, completion_tokens: usize, n_ctx: usize },
    Error    { message: String },
}

// ── Streaming command ─────────────────────────────────────────────────────────

/// Stream a chat completion directly through Tauri IPC, bypassing the HTTP
/// server entirely — no CORS, no port lookup, no WebSocket required.
///
/// Tokens arrive on `channel` as `ChatChunk` messages in order:
/// zero or more `Delta`, then exactly one `Done` **or** one `Error`.
/// An `Error { message: "aborted" }` is sent when `abort_llm_stream` is called.
///
/// The command blocks (async-awaits) until generation finishes, is aborted,
/// or the channel is closed by the JS side.
#[tauri::command]
pub async fn chat_completions_ipc(
    messages: Vec<serde_json::Value>,
    params:   crate::llm::GenParams,
    channel:  tauri::ipc::Channel<ChatChunk>,
    state:    tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    let cell = state.lock_or_recover().llm.state_cell.clone();
    let srv  = cell.lock().expect("lock poisoned").clone()
        .ok_or_else(|| "LLM server not running — start it in Settings → LLM".to_string())?;

    // Subscribe to the abort watch and mark the current value as "seen" so
    // that only a *new* increment (from `abort_llm_stream`) wakes us up.
    let mut abort_rx = srv.abort_tx.subscribe();
    abort_rx.borrow_and_update();

    let tool_channel = channel.clone();
    let gen_fut = crate::llm::run_chat_with_builtin_tools(&srv, messages, params, Vec::new(), |delta| {
        let _ = channel.send(ChatChunk::Delta { content: delta.to_string() });
    }, move |event: crate::llm::ToolEvent| {
        match event {
            crate::llm::ToolEvent::Status { tool_name, status, detail } => {
                if status.as_str() == "cancelled" {
                    let _ = tool_channel.send(ChatChunk::ToolCancelled {
                        tool_call_id: String::new(),
                        tool_name: tool_name.clone(),
                    });
                }
                let _ = tool_channel.send(ChatChunk::ToolUse {
                    tool:   tool_name,
                    status,
                    detail,
                });
            }
            crate::llm::ToolEvent::ExecutionStart { tool_call_id, tool_name, args } => {
                let _ = tool_channel.send(ChatChunk::ToolExecutionStart {
                    tool_call_id,
                    tool_name,
                    args,
                });
            }
            crate::llm::ToolEvent::ExecutionEnd { tool_call_id, tool_name, result, is_error } => {
                let _ = tool_channel.send(ChatChunk::ToolExecutionEnd {
                    tool_call_id,
                    tool_name,
                    result,
                    is_error,
                });
            }
        }
    });
    tokio::pin!(gen_fut);

    tokio::select! {
        biased;

        // Abort signal — higher priority than completion so we stop fast.
        Ok(()) = abort_rx.changed() => {
            let _ = channel.send(ChatChunk::Error { message: "aborted".into() });
        }

        result = &mut gen_fut => {
            match result {
                Ok((_text, finish_reason, prompt_tokens, completion_tokens, n_ctx)) => {
                    let _ = channel.send(ChatChunk::Done {
                        finish_reason,
                        prompt_tokens,
                        completion_tokens,
                        n_ctx,
                    });
                }
                Err(msg) => {
                    let _ = channel.send(ChatChunk::Error { message: msg });
                }
            }
        }
    }

    Ok(())
}

// ── Abort ─────────────────────────────────────────────────────────────────────

/// Cancel a running `chat_completions_ipc` stream.
///
/// Increments the abort watch in `LlmServerState`; the streaming command
/// detects the change via `watch::Receiver::changed()` and returns early,
/// sending `ChatChunk::Error { message: "aborted" }` to the frontend first.
///
/// Safe to call even when no generation is in progress — it is a no-op if
/// the server is stopped or idle.
#[tauri::command]
pub fn abort_llm_stream(state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let cell = { let g = state.lock_or_recover(); g.llm.state_cell.clone() };
    let guard = cell.lock().expect("lock poisoned");
    if let Some(srv) = guard.as_ref() {
        srv.abort_tx.send_modify(|v| *v = v.wrapping_add(1));
    }
}

// ── Tool-call cancellation ────────────────────────────────────────────────────

/// Cancel a specific tool call by its `tool_call_id`.
///
/// Adds the ID to the server's cancelled-tool-call set. The tool execution
/// functions check this set before and during execution. If the tool is
/// already running (e.g. a long bash command), the cancellation takes effect
/// the next time the runner checks; for tools that haven't started yet,
/// execution is skipped entirely.
///
/// Safe to call even when no generation is in progress — it is a no-op if
/// the server is stopped or the ID doesn't match any pending call.
#[tauri::command]
pub fn cancel_tool_call(
    tool_call_id: String,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let cell = { let g = state.lock_or_recover(); g.llm.state_cell.clone() };
    let guard = cell.lock().expect("lock poisoned");
    if let Some(srv) = guard.as_ref() {
        srv.cancelled_tool_calls.lock().expect("lock poisoned").insert(tool_call_id);
    }
    // Also cancel any in-progress external page fetch (headless webview).
    skill_headless::cancel_current_fetch();
}

// ── Chat window ───────────────────────────────────────────────────────────────

/// Open (or focus) the floating Chat window.
#[tauri::command]
pub async fn open_chat_window(app: AppHandle) -> Result<(), String> {
    crate::window_cmds::focus_or_create(&app, crate::window_cmds::WindowSpec {
        label: "chat", route: "chat", title: "NeuroSkill™ – Chat",
        inner_size: (760.0, 680.0), min_inner_size: Some((480.0, 400.0)),
        ..Default::default()
    })
}
