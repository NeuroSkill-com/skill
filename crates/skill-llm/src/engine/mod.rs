// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! OpenAI-compatible LLM inference server — RLX runtime backend.
//!
//! # Sub-modules
//!
//! | Module               | Responsibility                                      |
//! |----------------------|-----------------------------------------------------|
//! | `logging`            | Log buffer, file sink, push helpers                  |
//! | `protocol`           | Wire types: InferRequest, InferToken, GenParams, … |
//! | `state`              | LlmServerState, LlmStateCell, status helpers        |
//! | `images`             | Base64 data-URL decoding for chat messages           |
//! | `tool_orchestration` | Multi-round tool-calling loop                        |
//! | `rlx_actor`          | The OS thread event loop (RLX backend)               |
//! | `rlx_backend`        | RLX model loading and generation                     |
//! | `init`               | Public `init()` — spawns actor, returns state        |

// ── Internal macros ───────────────────────────────────────────────────────────
// Defined before submodule declarations so they are in scope for all children.

#[allow(unused_macros)]
macro_rules! llm_info  { ($app:expr, $buf:expr, $file:expr, $($t:tt)*) => { $crate::engine::logging::push_log_inner($app, $buf, $file, "info",  &format!($($t)*)) } }
#[allow(unused_macros)]
macro_rules! llm_warn  { ($app:expr, $buf:expr, $file:expr, $($t:tt)*) => { $crate::engine::logging::push_log_inner($app, $buf, $file, "warn",  &format!($($t)*)) } }
#[allow(unused_macros)]
macro_rules! llm_error { ($app:expr, $buf:expr, $file:expr, $($t:tt)*) => { $crate::engine::logging::push_log_inner($app, $buf, $file, "error", &format!($($t)*)) } }

// ── Sub-modules ───────────────────────────────────────────────────────────────

pub mod logging;
pub mod protocol;
pub mod state;

pub mod images;
mod init;
#[cfg(feature = "llm-rlx")]
mod rlx_actor;
#[cfg(feature = "llm-rlx")]
mod rlx_backend;
pub mod tool_orchestration;

// ── Re-exports ────────────────────────────────────────────────────────────────
// Preserve the flat `crate::engine::Foo` API so existing imports keep working.

pub use logging::{new_log_buffer, push_log, unix_ts_ms, LlmLogBuffer, LlmLogEntry, LlmLogFile};

pub use protocol::{ChatRequest, CompletionRequest, EmbeddingsRequest, GenParams, InferRequest, InferToken};

pub use state::{cell_status, new_state_cell, shutdown_cell, LlmServerState, LlmStateCell, LlmStatus};

pub use images::extract_images_from_messages;

pub use tool_orchestration::{run_chat_with_builtin_tools, AfterToolCallFn, BeforeToolCallFn, ToolEvent};

pub use init::init;
