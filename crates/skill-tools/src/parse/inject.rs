// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tool-definition injection into system prompts.
//!
//! llama.cpp local models do not always have native function-calling support;
//! we inject a system prompt block that lists available tools, their JSON
//! Schema parameters, and the exact format the model should use to call them.

use serde_json::Value;
use super::types::Tool;

/// Inject tool definitions and calling instructions into the system prompt.
pub fn inject_tools_into_system_prompt(
    messages: &mut Vec<Value>,
    tools:    &[Tool],
    n_ctx:    usize,
) {
    if tools.is_empty() { return; }

    let compact = n_ctx > 0 && n_ctx <= 2048;

    let mut tool_block = if compact {
        build_compact_tool_block(tools)
    } else {
        build_full_tool_block(tools)
    };

    tool_block.push_str(&build_os_context(tools));

    let has_system = messages.first()
        .and_then(|m| m.get("role"))
        .and_then(|r| r.as_str()) == Some("system");

    if has_system {
        if let Some(content) = messages[0].get_mut("content").and_then(|c| c.as_str()) {
            let merged = format!("{tool_block}\n\n{content}");
            messages[0]["content"] = Value::String(merged);
        }
    } else {
        messages.insert(0, serde_json::json!({
            "role":    "system",
            "content": tool_block,
        }));
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Build a compact tool block for very small context windows (<= 2048 tokens).
fn build_compact_tool_block(tools: &[Tool]) -> String {
    let mut names = Vec::new();
    for t in tools {
        let name = &t.function.name;
        let params: Vec<String> = t.function.parameters.as_ref()
            .and_then(|p| p.get("properties"))
            .and_then(|p| p.as_object())
            .map(|props| props.keys().cloned().collect())
            .unwrap_or_default();
        if params.is_empty() {
            names.push(name.to_string());
        } else {
            names.push(format!("{name}({})", params.join(",")));
        }
    }
    format!(
r#"Tools: {}
ALWAYS use tools when applicable. Do NOT show commands in code blocks — call them.
Format: [TOOL_CALL]{{"name":"<tool>","arguments":{{...}}}}[/TOOL_CALL]
Examples:
[TOOL_CALL]{{"name":"date","arguments":{{}}}}[/TOOL_CALL]
[TOOL_CALL]{{"name":"bash","arguments":{{"command":"ls ~/Desktop/"}}}}[/TOOL_CALL]
[TOOL_CALL]{{"name":"skill","arguments":{{"command":"status"}}}}[/TOOL_CALL]
For the "skill" tool, pass the command name inside arguments. Do NOT call command names like "status" directly — always use {{"name":"skill","arguments":{{"command":"..."}}}}.
Wait for results. Do NOT fabricate results."#,
        names.join(", ")
    )
}

/// Build the full tool block with descriptions, parameter docs, and examples.
fn build_full_tool_block(tools: &[Tool]) -> String {
    let mut tool_lines = String::new();
    for t in tools {
        let name = &t.function.name;
        let desc = t.function.description.as_deref().unwrap_or("");
        tool_lines.push_str(&format!("- **{name}**: {desc}\n"));

        if let Some(ref params) = t.function.parameters {
            if let Some(props) = params.get("properties").and_then(|p| p.as_object()) {
                let required: Vec<&str> = params.get("required")
                    .and_then(|r| r.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                for (pname, pval) in props {
                    let ptype = pval.get("type").and_then(|t| t.as_str()).unwrap_or("any");
                    let pdesc = pval.get("description").and_then(|d| d.as_str()).unwrap_or("");
                    let req_marker = if required.contains(&pname.as_str()) { " (required)" } else { " (optional)" };
                    tool_lines.push_str(&format!("  - `{pname}` ({ptype}{req_marker}): {pdesc}\n"));
                }
            }
        }
    }

    format!(
r#"# Tools

You have access to the following tools:

{tool_lines}
## IMPORTANT: You MUST use tools — do NOT just show commands

When the user asks you to do something that requires a tool (run a command, read a file, check the time, search the web, etc.), you MUST actually call the tool using the format below. NEVER just show the command or code in a code block — that does nothing. You must emit a [TOOL_CALL] block so the system executes it for you.

## How to call a tool

Output a tool-call block in exactly this format:

[TOOL_CALL]{{"name":"<tool_name>","arguments":{{"<param>":"<value>"}}}}[/TOOL_CALL]

Rules:
- The JSON inside [TOOL_CALL]…[/TOOL_CALL] MUST be valid JSON on a single line.
- You may call multiple tools by emitting multiple [TOOL_CALL]…[/TOOL_CALL] blocks.
- After emitting tool calls, STOP generating and wait. The system will execute the tool(s) and provide results in a follow-up message.
- Use the tool results to formulate your final answer to the user.
- Do NOT fabricate or guess tool results. Always call the tool and use the actual result.
- Do NOT describe what you would do — actually call the tool.
- Do NOT show commands in code blocks (```bash ...```) — use [TOOL_CALL] instead.
- If the user asks to list files, run a command, check something, etc. — ALWAYS use the appropriate tool.

## Examples

User: "What time is it?"
Assistant: [TOOL_CALL]{{"name":"date","arguments":{{}}}}[/TOOL_CALL]

User: "How much disk space is left?"
Assistant: [TOOL_CALL]{{"name":"bash","arguments":{{"command":"df -h"}}}}[/TOOL_CALL]

User: "What files are on my desktop?"
Assistant: [TOOL_CALL]{{"name":"bash","arguments":{{"command":"ls ~/Desktop/"}}}}[/TOOL_CALL]

User: "Read the file config.toml"
Assistant: [TOOL_CALL]{{"name":"read_file","arguments":{{"path":"config.toml"}}}}[/TOOL_CALL]

User: "Where am I located?"
Assistant: [TOOL_CALL]{{"name":"location","arguments":{{}}}}[/TOOL_CALL]

User: "What's the weather like?"
Assistant: [TOOL_CALL]{{"name":"web_search","arguments":{{"query":"weather <city>","render":true}}}}[/TOOL_CALL]
(Use render=true for factual queries like weather, prices, scores, or news so the actual page content is fetched and you can summarise it directly.)

User: "How do I feel?" / "What's my brain state?"
Assistant: [TOOL_CALL]{{"name":"skill","arguments":{{"command":"status"}}}}[/TOOL_CALL]
(Use the "skill" tool for ALL EEG/brain/device queries. Pass the command name inside "arguments", e.g. {{"command":"status"}}. Do NOT call "status" or any other command name directly as a tool — always wrap it with the "skill" tool.)"#
    )
}

/// Build a short OS/environment context line for the tool prompt.
fn build_os_context(tools: &[Tool]) -> String {
    let has_shell_or_fs = tools.iter().any(|t| {
        matches!(t.function.name.as_str(), "bash" | "read_file" | "write_file" | "edit_file" | "search_output")
    });
    if !has_shell_or_fs {
        return String::new();
    }

    let os = match std::env::consts::OS {
        "macos"   => "macOS",
        "linux"   => "Linux",
        "windows" => "Windows",
        other     => other,
    };
    let arch = std::env::consts::ARCH;
    let home = dirs::home_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~".into());
    let shell = if cfg!(target_os = "windows") { "PowerShell" } else { "bash" };

    format!("\n\nSystem: {os} ({arch}), shell: {shell}, home: {home}")
}
