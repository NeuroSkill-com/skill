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

pub mod error;
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

pub mod context;
pub mod defs;
pub mod exec;
pub mod parse;
pub(crate) mod search;
pub mod types;
pub mod web_cache;

// Re-export the most-used types at crate root for convenience.
pub use context::{estimate_messages_tokens, estimate_tokens, trim_messages_to_fit};
pub use defs::{
    builtin_llm_tools, enabled_builtin_llm_tools, filter_allowed_tool_defs, is_builtin_tool_enabled,
    rerank_tools_for_prompt, skill_api_tool,
};
pub use error::{ExecError, ParseError, ValidationError};
pub use exec::execute_builtin_tool_call;
pub use exec::{set_bash_edit_hook, BashEditHook};
pub use parse::{
    build_self_healing_message, coerce_tool_call_arguments, detect_garbled_tool_call, extract_tool_calls,
    inject_tools_into_system_prompt, inject_tools_into_system_prompt_with_options, strip_tool_call_blocks,
    strip_tool_call_blocks_preserve, validate_tool_arguments, ChatMessage, ContentPart, ImageUrl, MessageContent, Tool,
    ToolCall, ToolCallFunction, ToolFunction, ToolPromptOptions,
};
pub use types::{CompressionLevel, LlmToolConfig, ToolContextCompression, ToolExecutionMode, WebCacheConfig};
