// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! `skill-tools` — LLM tool definitions, parsing, execution, and context management.
//!
//! This crate contains all tool-related logic extracted from the NeuroSkill monolith:
//!
//! - **types** — `LlmToolConfig`, `ToolExecutionMode`
//! - **parse** — tool-call extraction, parsing, validation, injection, stripping
//! - **defs** — built-in tool definitions (date, location, web_search, bash, etc.)
//! - **exec** — tool execution (each tool's runtime implementation)
//! - **context** — context-aware history trimming for tool conversations
//! - **log** — standalone pluggable logger for tool-call tracing

pub mod log;

/// Log a message from the tool-call subsystem.
///
/// ```ignore
/// tool_log!("tool", "[info] executing tool: {name}");
/// tool_log!("tool:bash", "command={cmd}");
/// ```
///
/// Short-circuits (no `format!` allocation) when logging is disabled.
#[macro_export]
macro_rules! tool_log {
    ($tag:expr, $($arg:tt)*) => {
        if $crate::log::log_enabled() {
            $crate::log::write_log($tag, &format!($($arg)*));
        }
    };
}

pub mod types;
pub mod parse;
pub mod defs;
pub mod exec;
pub(crate) mod search;
pub mod context;

// Re-export the most-used types at crate root for convenience.
pub use types::{LlmToolConfig, ToolExecutionMode, ToolContextCompression, CompressionLevel};
pub use parse::{
    Tool, ToolFunction, ToolCall, ToolCallFunction,
    ChatMessage, MessageContent, ContentPart, ImageUrl,
    validate_tool_arguments, coerce_tool_call_arguments,
    inject_tools_into_system_prompt,
    extract_tool_calls,
    strip_tool_call_blocks, strip_tool_call_blocks_preserve,
};
pub use defs::{
    builtin_llm_tools, skill_api_tool, is_builtin_tool_enabled,
    enabled_builtin_llm_tools, filter_allowed_tool_defs,
};
pub use exec::execute_builtin_tool_call;
pub use context::{estimate_tokens, estimate_messages_tokens, trim_messages_to_fit};
