// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tool-call / function-calling helpers for OpenAI-compatible chat completions.
//!
//! This module is split into focused sub-modules:
//! - [`types`]    — shared data types (`Tool`, `ToolCall`, `ChatMessage`, …)
//! - [`coerce`]   — schema-driven type coercion for LLM arguments
//! - [`validate`] — JSON Schema validation of tool-call arguments
//! - [`extract`]  — tool-call extraction from raw assistant output
//! - [`strip`]    — stripping tool-call blocks from message content
//! - [`inject`]   — injecting tool definitions into system prompts
//! - `json_scan` — balanced JSON range finders

pub mod coerce;
pub mod extract;
pub mod inject;
pub(crate) mod json_scan;
#[cfg(test)]
mod proptest_tests;
pub mod strip;
pub mod types;
pub mod validate;

// ── Re-exports (preserve backward-compatible public API) ─────────────────────

pub use types::{ChatMessage, ContentPart, ImageUrl, MessageContent, Tool, ToolCall, ToolCallFunction, ToolFunction};

pub use coerce::{coerce_tool_call_arguments, coerce_value};
pub use extract::{build_self_healing_message, detect_garbled_tool_call, extract_tool_calls};
pub use inject::inject_tools_into_system_prompt;
pub use strip::{strip_tool_call_blocks, strip_tool_call_blocks_preserve};
pub use validate::validate_tool_arguments;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
