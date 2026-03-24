// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Built-in tool execution — the runtime implementation of each tool.
//!
//! This module is split into focused sub-modules:
//!
//! - **tools_system** — `date`, `location`, `bash`, `skill`
//! - **tools_web** — `web_search`, `web_fetch`
//! - **tools_fs** — `read_file`, `write_file`, `edit_file`, `search_output`
//! - **status** — human-readable text formatter for `skill status`
//! - **truncate** — text/output truncation helpers
//! - **safety** — dangerous-operation detection and user-approval dialogs
//! - **helpers** — path resolution, UTC offset formatting

mod tools_system;
mod tools_web;
mod tools_fs;
mod status;
pub(crate) mod truncate;
pub(crate) mod safety;
pub(crate) mod helpers;

#[cfg(test)]
mod tests;

use serde_json::{Value, json};

use crate::parse::ToolCall;
use crate::types::LlmToolConfig;
use crate::defs::is_builtin_tool_enabled;

// Re-export public API items that were previously accessible from `exec`.
pub use truncate::truncate_text;
pub use helpers::resolve_tool_path;
pub use safety::{check_bash_safety, check_path_safety, request_tool_approval};

// ── Public execution entry point ──────────────────────────────────────────────

/// Execute a single built-in tool call and return the JSON result.
pub async fn execute_builtin_tool_call(call: &ToolCall, allowed_tools: &LlmToolConfig, scripts_dir: &std::path::Path) -> Value {
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or_else(|_| json!({}));
    let tool_name = &call.function.name;

    // Distinguish "unknown tool" from "known but disabled".
    // When the name matches a Skill API sub-command or neuroskill alias,
    // give a precise hint so the model self-corrects in one step.
    if !crate::defs::is_known_builtin_tool(tool_name) {
        if let Some(cmd) = crate::defs::resolve_skill_alias(tool_name) {
            tool_log!("tool", "[blocked] tool={} reason=skill alias, should be skill({})", tool_name, cmd);
            return json!({
                "ok": false,
                "tool": call.function.name,
                "error": format!(
                    "\"{}\" is not a top-level tool — it maps to the \"skill\" tool. \
                     Call the \"skill\" tool with {{\"command\": \"{}\"}} instead.",
                    tool_name, cmd
                )
            });
        }
        tool_log!("tool", "[blocked] tool={} reason=unsupported tool", tool_name);
        return json!({ "ok": false, "tool": call.function.name, "error": format!("unsupported tool \"{}\". Use one of the available tools listed in the system prompt.", tool_name) });
    }
    if !is_builtin_tool_enabled(allowed_tools, tool_name) {
        tool_log!("tool", "[blocked] tool={} reason=disabled in settings", tool_name);
        return json!({ "ok": false, "tool": call.function.name, "error": "tool disabled in settings" });
    }

    tool_log!("tool", "[invoke] tool={} args={}", tool_name, args);
    let start = std::time::Instant::now();

    let result = match call.function.name.as_str() {
        "date"          => tools_system::exec_date(),
        "location"      => tools_system::exec_location().await,
        "bash"          => tools_system::exec_bash(&args, scripts_dir).await,
        "skill"         => tools_system::exec_skill(&args, allowed_tools).await,
        "web_search"    => tools_web::exec_web_search(&args, allowed_tools).await,
        "web_fetch"     => tools_web::exec_web_fetch(&args, allowed_tools).await,
        "read_file"     => tools_fs::exec_read_file(&args).await,
        "write_file"    => tools_fs::exec_write_file(&args).await,
        "edit_file"     => tools_fs::exec_edit_file(&args).await,
        "search_output" => tools_fs::exec_search_output(&args).await,
        other => {
            tool_log!("tool", "[error] tool={} unsupported", other);
            json!({ "ok": false, "tool": other, "error": "unsupported tool" })
        }
    };

    let elapsed = start.elapsed();
    let ok = result.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false);
    if ok {
        tool_log!("tool", "[done] tool={} elapsed={:.1?}", tool_name, elapsed);
    } else {
        let err = result.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
        tool_log!("tool", "[fail] tool={} elapsed={:.1?} error={}", tool_name, elapsed, err);
    }
    result
}
