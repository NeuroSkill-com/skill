// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! LLM module — thin adapter layer over the `skill-llm` crate.
//!
//! The core inference engine, tool calling, catalog, and chat store live in the
//! `skill-llm` workspace crate.  This module re-exports their public API so
//! the rest of the main `skill` crate can keep using `crate::llm::…` paths
//! unchanged.
//!
//! Tauri commands live in sub-modules under `cmds/` because they depend on
//! `AppState`, `tauri::State`, and `refresh_tray` — all main-crate concerns.

// ── Re-exports from skill-llm ─────────────────────────────────────────────────
// These re-exports exist so the rest of the crate can keep using `crate::llm::…`
// paths unchanged after the extraction.

#[allow(unused_imports)]
pub use skill_llm::catalog;
#[allow(unused_imports)]
pub use skill_llm::chat_store;
#[allow(unused_imports)]
pub use skill_llm::tools;

// Re-export commonly used types at this module level so `crate::llm::Foo` works.
#[allow(unused_imports)]
pub use skill_llm::{LlmConfig, LlmToolConfig, ToolExecutionMode};
#[allow(unused_imports)]
pub use skill_llm::{LlmEventEmitter, NoopEmitter};

// ── Lightweight types for daemon-proxy commands (no engine dependency) ───────
//
// LLM inference runs exclusively in skill-daemon.  The Tauri binary is a thin
// UI client and never links llama-cpp-4.  These types exist so the proxy
// commands compile without the engine crate.

/// LLM server status — mirrors the daemon's status strings.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmStatus {
    Running,
    Loading,
    Stopped,
}

/// Log buffer type for LLM log entries.
pub type LlmLogBuffer =
    std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<serde_json::Value>>>;

/// Create an empty log buffer.
pub fn new_log_buffer() -> LlmLogBuffer {
    std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()))
}

// ── Tauri commands ────────────────────────────────────────────────────────────
pub mod cmds;

// ── Logger integration ────────────────────────────────────────────────────────

/// Wire LLM log output through the app's [`SkillLogger`](crate::skill_log::SkillLogger).
///
/// Call once during setup, after the logger is registered as managed state.
pub fn init_llm_logger(app: &tauri::AppHandle) {
    use tauri::Manager;
    let logger = app
        .state::<std::sync::Arc<crate::skill_log::SkillLogger>>()
        .inner()
        .clone();
    skill_llm::log::set_log_callback(move |tag, msg| {
        if logger.enabled(tag) {
            logger.write(tag, msg);
        }
    });
}

/// Enable or disable LLM log output (backwards-compatible wrapper).
pub fn set_llm_logging(enabled: bool) {
    skill_llm::log::set_log_enabled(enabled);
}

/// Wire `skill_tools::log` into the central [`SkillLogger`].
///
/// Call once during setup, after the logger is registered as managed state.
pub fn init_tool_logger(app: &tauri::AppHandle) {
    use tauri::Manager;
    let logger = app
        .state::<std::sync::Arc<crate::skill_log::SkillLogger>>()
        .inner()
        .clone();
    skill_tools::log::set_log_callback(move |tag, msg| {
        if logger.enabled(tag) {
            logger.write(tag, msg);
        }
    });

    // Register the bash-edit hook — shows the command in a dialog and lets
    // the user approve or cancel before execution.
    skill_tools::set_bash_edit_hook(std::sync::Arc::new(|command: &str| {
        // Truncate very long commands for the dialog display.
        // Use char boundary to avoid panic on multi-byte UTF-8.
        let display = if command.chars().count() > 2000 {
            let truncated: String = command.chars().take(2000).collect();
            format!(
                "{}...\n\n({} chars total)",
                truncated,
                command.chars().count()
            )
        } else {
            command.to_string()
        };
        let message = format!(
            "The LLM wants to run this bash command:\n\n{}\n\nAllow execution?",
            display
        );
        let approved = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Info)
            .set_title("NeuroSkill \u{2014} Review Bash Command")
            .set_description(&message)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            == rfd::MessageDialogResult::Yes;

        if approved {
            Some(command.to_string())
        } else {
            None
        }
    }));
}

/// Enable or disable tool-call log output.
pub fn set_tool_logging(enabled: bool) {
    skill_tools::log::set_log_enabled(enabled);
}

// ── Tauri AppHandle adapter ───────────────────────────────────────────────────
//
// Implements `LlmEventEmitter` for `tauri::AppHandle` so the skill-llm crate
// can emit events to the Tauri frontend without depending on tauri itself.

use serde_json::Value;
use tauri::Emitter as _;

/// Wrapper that implements `LlmEventEmitter` for `tauri::AppHandle`.
#[derive(Clone)]
#[allow(dead_code)]
pub struct TauriEmitter(pub tauri::AppHandle);

impl skill_llm::LlmEventEmitter for TauriEmitter {
    fn emit_event(&self, event: &str, payload: Value) {
        // tauri::Emitter::emit requires the payload to be Serialize.
        // serde_json::Value implements Serialize, so this works directly.
        let _ = self.0.emit(event, payload);
    }
}
