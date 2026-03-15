// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! LLM module — thin adapter layer over the `skill-llm` crate.
//!
//! The core inference engine, tool calling, catalog, and chat store live in the
//! `skill-llm` workspace crate.  This module re-exports their public API so
//! the rest of the main `skill` crate can keep using `crate::llm::…` paths
//! unchanged.
//!
//! `cmds.rs` (Tauri commands) stays here because it depends on `AppState`,
//! `tauri::State`, and `refresh_tray` — all main-crate concerns.

pub mod cmds;

// ── Re-exports from skill-llm ─────────────────────────────────────────────────
// These re-exports exist so the rest of the crate can keep using `crate::llm::…`
// paths unchanged after the extraction.

#[allow(unused_imports)] pub use skill_llm::tools;
#[allow(unused_imports)] pub use skill_llm::catalog;
#[allow(unused_imports)] pub use skill_llm::chat_store;

#[cfg(feature = "llm")]
#[allow(unused_imports)] pub use skill_llm::engine;

// Re-export commonly used types at this module level so `crate::llm::Foo` works.
#[allow(unused_imports)] pub use skill_llm::{LlmConfig, LlmToolConfig, ToolExecutionMode};
#[allow(unused_imports)] pub use skill_llm::{LlmEventEmitter, NoopEmitter};

#[cfg(feature = "llm")]
#[allow(unused_imports)]
pub use skill_llm::{
    GenParams, InferRequest, InferToken, LlmLogBuffer, LlmLogEntry,
    LlmLogFile, LlmServerState, LlmStateCell, LlmStatus,
    BeforeToolCallFn, AfterToolCallFn, ToolEvent,
    cell_status, extract_images_from_messages, init, new_log_buffer,
    new_state_cell, push_log, router, shutdown_cell,
    run_chat_with_builtin_tools,
};

// ── Tauri AppHandle adapter ───────────────────────────────────────────────────
//
// Implements `LlmEventEmitter` for `tauri::AppHandle` so the skill-llm crate
// can emit events to the Tauri frontend without depending on tauri itself.

use serde_json::Value;
use tauri::Emitter as _;

/// Wrapper that implements `LlmEventEmitter` for `tauri::AppHandle`.
#[derive(Clone)]
pub struct TauriEmitter(pub tauri::AppHandle);

impl skill_llm::LlmEventEmitter for TauriEmitter {
    fn emit_event(&self, event: &str, payload: Value) {
        // tauri::Emitter::emit requires the payload to be Serialize.
        // serde_json::Value implements Serialize, so this works directly.
        let _ = self.0.emit(event, payload);
    }
}
