// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Shared server state, state cell, and status types.

use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::config::LlmToolConfig;
use super::protocol::{InferRequest, InferToken, GenParams};

// ── Shared state (held in axum Router via `.with_state()`) ────────────────────

pub struct LlmServerState {
    /// Channel to the inference actor.
    pub req_tx: mpsc::UnboundedSender<InferRequest>,
    /// Display name shown in `/v1/models`.
    pub model_name:       String,
    /// Optional Bearer token required on every request.
    pub api_key:      Option<String>,
    /// Built-in tools currently allowed for chat requests.
    #[cfg(feature = "llm")]
    pub allowed_tools: Arc<Mutex<LlmToolConfig>>,

    /// Set to `true` by the actor once the model + context are fully loaded.
    pub ready:        Arc<AtomicBool>,
    /// Context window size in tokens; set by the actor after context creation.
    pub n_ctx:        Arc<std::sync::atomic::AtomicUsize>,
    /// Whether a vision projector (mmproj) was loaded — enables image input.
    pub vision_ready: Arc<AtomicBool>,
    /// Set of tool_call_ids that the user has cancelled from the UI.
    /// Checked before each tool execution; cancelled calls return an error
    /// result instead of running.
    pub cancelled_tool_calls: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    /// Base directory for storing tool-generated script and output files.
    /// Located at `skill_dir/chats/scripts/`. Subdirectories are created
    /// lazily per tool invocation timestamp.
    pub scripts_dir: std::path::PathBuf,
    /// Discovered Agent Skills — injected into the system prompt so the LLM
    /// can load specialised instructions via `read_file` on demand.
    pub skills: Arc<Vec<skill_skills::Skill>>,

    /// Abort signal for IPC-streamed chat (`chat_completions_ipc`).
    ///
    /// Increment the value to cancel a running IPC generation:
    /// `abort_tx.send_modify(|v| *v = v.wrapping_add(1))`.
    /// The streaming command subscribes via `abort_tx.subscribe()` and
    /// breaks out of its token loop as soon as the value changes.
    pub abort_tx:     tokio::sync::watch::Sender<u64>,
    /// OS thread handle for the actor.  Taken (set to `None`) by `shutdown()`.
    pub(super) join_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl LlmServerState {
    /// Whether the actor has finished loading the model.
    pub fn is_ready(&self) -> bool { self.ready.load(Ordering::Relaxed) }

    pub fn set_allowed_tools(&self, tools: LlmToolConfig) {
        *self.allowed_tools.lock().unwrap() = tools;
    }

    /// Stop the actor and **block until the thread has fully exited**.
    ///
    /// Dropping `req_tx` closes the channel → the actor's `blocking_recv()`
    /// loop returns `None` → the actor drops `ctx`, `model`, and backend in
    /// the correct order → Metal/CUDA resources are released.
    ///
    /// Must be called while the caller holds the **only** remaining
    /// `Arc<LlmServerState>` (i.e. the cell has already been taken).
    pub fn shutdown(self) {
        // Taking the join handle *before* `req_tx` is dropped prevents a race
        // where the thread exits and the handle becomes invalid.
        let handle = self.join_handle.lock().unwrap().take();
        // Dropping `self` here also drops `req_tx`, closing the channel.
        drop(self);
        if let Some(h) = handle {
            let _ = h.join();
        }
    }

    /// Send a chat completion request and stream the generated tokens back via
    /// the returned `UnboundedReceiver`.
    ///
    /// Returns `Err` when the model is still loading or the actor has exited.
    /// Images should be raw JPEG/PNG bytes decoded from base64 data-URLs; pass
    /// an empty `Vec` for text-only prompts.
    pub fn chat(
        &self,
        messages: Vec<Value>,
        images:   Vec<Vec<u8>>,
        params:   GenParams,
    ) -> Result<mpsc::UnboundedReceiver<InferToken>, String> {
        if !self.is_ready() {
            return Err("LLM model still loading — retry in a few seconds".to_string());
        }
        let (tok_tx, tok_rx) = mpsc::unbounded_channel();
        self.req_tx
            .send(InferRequest::Generate { messages, images, params, token_tx: tok_tx })
            .map_err(|_| "LLM actor has exited".to_string())?;
        Ok(tok_rx)
    }
}

/// A dynamic cell that holds the (optional) running server state.
///
/// The axum router always has `/v1/*` routes registered; they check this cell
/// at request time and return 503 when `None` (server stopped/not started).
/// `start_llm_server` / `stop_llm_server` Tauri commands swap the contents.
pub type LlmStateCell = Arc<Mutex<Option<Arc<LlmServerState>>>>;

/// Create a new, empty server state cell.
pub fn new_state_cell() -> LlmStateCell {
    Arc::new(Mutex::new(None))
}

/// Gracefully stop the server referenced by `cell`, blocking until the actor
/// thread has fully exited.  Safe to call from any thread, including the Tauri
/// `RunEvent::Exit` handler.  No-op if the server is not running.
pub fn shutdown_cell(cell: &LlmStateCell) {
    if let Some(server_state) = cell.lock().unwrap().take() {
        match Arc::try_unwrap(server_state) {
            Ok(owned) => owned.shutdown(),
            Err(arc)  => drop(arc),   // in-flight axum handler; actor exits when arc drops
        }
    }
}

// ── Server status ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmStatus { Stopped, Loading, Running }

/// Query the current server status from the cell.
pub fn cell_status(cell: &LlmStateCell) -> (LlmStatus, String) {
    match &*cell.lock().unwrap() {
        None    => (LlmStatus::Stopped, String::new()),
        Some(s) => (
            if s.is_ready() { LlmStatus::Running } else { LlmStatus::Loading },
            s.model_name.clone(),
        ),
    }
}
