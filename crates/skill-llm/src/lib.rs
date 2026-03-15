// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! `skill-llm` — LLM inference engine extracted from the NeuroSkill monolith.
//!
//! This crate contains the core LLM logic:
//!
//! - **config** — `LlmConfig` (LLM server config), re-exports `LlmToolConfig`
//! - **engine** — actor-based inference, HTTP router, tool orchestration
//! - **catalog** — model catalog management
//! - **chat_store** — SQLite chat history persistence
//! - **tools** — re-export of `skill_tools::parse` (tool-call extraction, parsing, validation)
//! - **event** — event emitter trait (abstracts tauri::AppHandle)

pub mod config;
pub mod event;
pub mod catalog;
pub mod chat_store;

/// Re-export the `skill_tools::parse` module as `tools` for backwards compatibility.
/// All tool-call parsing, extraction, validation, and injection lives in `skill-tools`.
pub use skill_tools::parse as tools;

#[cfg(feature = "llm")]
pub mod engine;

// Re-export the most-used types at crate root for convenience.
pub use config::{LlmConfig, LlmToolConfig, ToolExecutionMode};
pub use event::{LlmEventEmitter, NoopEmitter};

#[cfg(feature = "llm")]
pub use engine::{
    GenParams, InferRequest, InferToken, LlmLogBuffer, LlmLogEntry,
    LlmLogFile, LlmServerState, LlmStateCell, LlmStatus,
    BeforeToolCallFn, AfterToolCallFn, ToolEvent,
    cell_status, extract_images_from_messages, init, new_log_buffer,
    new_state_cell, push_log, router, shutdown_cell,
    run_chat_with_builtin_tools,
};
