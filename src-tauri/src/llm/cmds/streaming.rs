// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! IPC chat streaming, abort, and tool-call cancellation commands.

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

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
#[allow(dead_code)]
#[derive(serde::Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatChunk {
    Delta {
        content: String,
    },
    /// Real generation-phase marker for the progress UI ("vision" while the
    /// image is encoded, "prefill" during the LM prefill). No visible content.
    Status {
        phase: String,
    },
    /// Legacy event — still emitted for backwards compatibility.
    ToolUse {
        tool: String,
        status: String,
        detail: Option<String>,
    },
    /// Rich tool-execution lifecycle events (pi-mono style).
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    },
    /// A tool call was cancelled by the user.
    ToolCancelled {
        tool_call_id: String,
        tool_name: String,
    },
    Done {
        finish_reason: String,
        prompt_tokens: usize,
        completion_tokens: usize,
        n_ctx: usize,
    },
    Error {
        message: String,
    },
}

// ── Streaming command ─────────────────────────────────────────────────────────

/// Stream a chat completion directly through Tauri IPC, bypassing the HTTP
/// server entirely — no CORS, no port lookup, no WebSocket required.
///
/// Tokens arrive on `channel` as `ChatChunk` messages in order:
/// zero or more `Delta` (emitted **live** as each token is generated), then
/// exactly one `Done` **or** one `Error`. An `Error { message: "aborted" }` is
/// sent when `abort_llm_stream` is called.
///
/// Internally this consumes the daemon's SSE stream (`stream: true`) and
/// forwards each delta as it arrives — previously it buffered the whole reply
/// and emitted it as a single delta, which felt like a long freeze then a dump
/// (worst on slow image turns). The blocking SSE read runs on a blocking thread
/// so it never stalls the async runtime.
#[tauri::command]
pub async fn chat_completions_ipc(
    messages: Vec<serde_json::Value>,
    params: serde_json::Value,
    channel: tauri::ipc::Channel<ChatChunk>,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    let forward = channel.clone();
    let phase_forward = channel.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::daemon_cmds::llm_chat_completions_stream(
            messages,
            params,
            |piece| {
                // Forward each token piece; `send` fails once the UI closes the
                // channel — return false to stop generation early.
                forward
                    .send(ChatChunk::Delta {
                        content: piece.to_string(),
                    })
                    .is_ok()
            },
            |phase| {
                // Real phase marker during the pre-token lead-in.
                let _ = phase_forward.send(ChatChunk::Status {
                    phase: phase.to_string(),
                });
            },
        )
    })
    .await
    .map_err(|err| err.to_string())?;

    match result {
        Ok(done) => {
            let _ = channel.send(ChatChunk::Done {
                finish_reason: done.finish_reason,
                prompt_tokens: done.prompt_tokens,
                completion_tokens: done.completion_tokens,
                n_ctx: done.n_ctx,
            });
        }
        Err(message) => {
            let _ = channel.send(ChatChunk::Error { message });
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
pub fn abort_llm_stream(_state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let _ = crate::daemon_cmds::llm_abort_stream();
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
pub fn cancel_tool_call(tool_call_id: String, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    let _ = crate::daemon_cmds::llm_cancel_tool_call(tool_call_id);
    skill_headless::cancel_current_fetch();
}

// ── Chat window ───────────────────────────────────────────────────────────────

/// Open (or focus) the floating Chat window, optionally loading a specific session.
#[tauri::command]
pub async fn open_chat_window(app: AppHandle, session_id: Option<i64>) -> Result<(), String> {
    let spec = crate::window_cmds::WindowSpec {
        label: "chat",
        route: "chat",
        title: "NeuroSkill™ – Chat",
        inner_size: (760.0, 680.0),
        min_inner_size: Some((480.0, 400.0)),
        ..Default::default()
    };

    match session_id.filter(|id| *id > 0) {
        Some(id) => {
            let payload = format!("{{\"sessionId\":{id}}}");
            let is_new = app.get_webview_window("chat").is_none();
            crate::window_cmds::focus_or_create_with_emit(
                &app,
                spec,
                "chat:load-session",
                &payload,
            )?;
            // For newly created windows, re-emit after the webview mounts
            if is_new {
                let app2 = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    if let Some(win) = app2.get_webview_window("chat") {
                        let _ = win.emit("chat:load-session", payload);
                    }
                });
            }
            Ok(())
        }
        None => crate::window_cmds::focus_or_create(&app, spec),
    }
}
