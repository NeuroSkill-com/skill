// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! OpenAI-compatible LLM inference server — native llama-cpp-4 backend.
//!
//! # Architecture
//!
//! A dedicated OS thread ("actor") owns the `LlamaBackend`, `LlamaModel`, and
//! `LlamaContext`.  Axum HTTP handlers communicate with the actor through a
//! pair of channels:
//!
//! ```text
//!  axum handler  ──InferRequest──▶  actor thread
//!  axum handler  ◀──InferToken ──  actor thread   (unbounded mpsc per request)
//! ```
//!
//! This design sidesteps all `LlamaContext<'model>` lifetime issues: the actor
//! owns both the model and the context in a single scope, so lifetimes are
//! trivially satisfied.
//!
//! # Sub-modules
//!
//! | Module               | Responsibility                                      |
//! |----------------------|-----------------------------------------------------|
//! | `logging`            | Log buffer, file sink, push helpers                  |
//! | `protocol`           | Wire types: InferRequest, InferToken, GenParams, … |
//! | `state`              | LlmServerState, LlmStateCell, status helpers        |
//! | `think_tracker`      | `<think>…</think>` budget enforcement                |
//! | `images`             | Base64 data-URL decoding for chat messages           |
//! | `tool_orchestration` | Multi-round tool-calling loop                        |
//! | `sampling`           | Token-by-token sampling with stop-string holdback    |
//! | `generation`         | Text-only and multimodal generation entry points     |
//! | `actor`              | The OS thread event loop                             |
//! | `init`               | Public `init()` — spawns actor, returns state        |

// ── Internal macros ───────────────────────────────────────────────────────────
// Defined before submodule declarations so they are in scope for all children.

macro_rules! llm_info  { ($app:expr, $buf:expr, $file:expr, $($t:tt)*) => { $crate::engine::logging::push_log_inner($app, $buf, $file, "info",  &format!($($t)*)) } }
macro_rules! llm_warn  { ($app:expr, $buf:expr, $file:expr, $($t:tt)*) => { $crate::engine::logging::push_log_inner($app, $buf, $file, "warn",  &format!($($t)*)) } }
macro_rules! llm_error { ($app:expr, $buf:expr, $file:expr, $($t:tt)*) => { $crate::engine::logging::push_log_inner($app, $buf, $file, "error", &format!($($t)*)) } }

// ── Sub-modules ───────────────────────────────────────────────────────────────

pub mod logging;
pub mod protocol;
pub mod state;

mod actor;
mod generation;
pub mod images;
mod init;
mod sampling;
mod think_tracker;
pub mod tool_orchestration;

// ── Re-exports ────────────────────────────────────────────────────────────────
// Preserve the flat `crate::engine::Foo` API so existing imports keep working.

pub use logging::{new_log_buffer, push_log, unix_ts_ms, LlmLogBuffer, LlmLogEntry, LlmLogFile};

pub use protocol::{ChatRequest, CompletionRequest, EmbeddingsRequest, GenParams, InferRequest, InferToken};

pub use state::{cell_status, new_state_cell, shutdown_cell, LlmServerState, LlmStateCell, LlmStatus};

pub use images::extract_images_from_messages;

pub use tool_orchestration::{run_chat_with_builtin_tools, AfterToolCallFn, BeforeToolCallFn, ToolEvent};

pub use init::init;
