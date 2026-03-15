// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
#![allow(dead_code)]
//! Tool-call / function-calling helpers for OpenAI-compatible chat completions.
//!
//! These utilities are used by the proxy layer to normalise function-call
//! arguments before forwarding them to llama-server, and to extract tool
//! results from the response.
//!
//! The reference implementation is:
//! <https://github.com/eugenehp/llama-cpp-rs/tree/main/examples/server/src/tools.rs>

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

// ── Argument validation ───────────────────────────────────────────────────────

/// Validate tool-call arguments against the tool's JSON Schema `parameters`.
///
/// Returns the (potentially coerced) arguments value, or an `Err` with a
/// human-readable validation error message.
///
/// Modelled after pi-mono's `validateToolArguments` which uses AJV against
/// TypeBox schemas.  Here we use the `jsonschema` crate for Rust.
pub fn validate_tool_arguments(tool: &Tool, args: &Value) -> Result<Value, String> {
    let Some(ref schema) = tool.function.parameters else {
        // No schema defined — accept any arguments.
        return Ok(args.clone());
    };

    let compiled = jsonschema::validator_for(schema)
        .map_err(|e| format!("Invalid tool schema for \"{}\": {e}", tool.function.name))?;

    let errors: Vec<String> = compiled
        .iter_errors(args)
        .map(|err| {
            let path_str = err.instance_path.to_string();
            let path = if path_str.is_empty() {
                "root".to_string()
            } else {
                path_str
            };
            format!("  - {path}: {err}")
        })
        .collect();

    if !errors.is_empty() {
        return Err(format!(
            "Validation failed for tool \"{}\":\n{}\n\nReceived arguments:\n{}",
            tool.function.name,
            errors.join("\n"),
            serde_json::to_string_pretty(args).unwrap_or_default()
        ));
    }

    Ok(args.clone())
}

/// Built-in tool names used for dict-style multi-tool recognition:
///   { "date": {}, "location": {} }
/// These must stay in sync with `enabled_builtin_llm_tools` in mod.rs.
const KNOWN_TOOL_NAMES: &[&str] = &["date", "location", "web_search", "web_fetch", "bash", "read_file", "write_file", "edit_file", "search_output"];

/// Returns true if `v` is a dict-style multi-tool object whose keys are
/// (at least partially) known tool names and whose values are parameter objects.
///   { "date": {}, "location": {} }
fn is_dict_style_multi_tool(v: &Value) -> bool {
    let Some(obj) = v.as_object() else { return false; };
    if obj.is_empty() { return false; }
    let has_known_key = obj.keys().any(|k| KNOWN_TOOL_NAMES.contains(&k.as_str()));
    let all_obj_vals  = obj.values().all(|v| v.is_object() || v.is_null());
    has_known_key && all_obj_vals
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name:        String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters:  Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function:  ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name:      String,
    pub arguments: String,   // JSON-encoded arguments string (as per OpenAI spec)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id:       String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function:  ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ChatMessage {
    System    { content: MessageContent },
    User      { content: MessageContent },
    Assistant {
        #[serde(default)]
        content:    Option<MessageContent>,
        #[serde(default)]
        tool_calls: Vec<ToolCall>,
    },
    Tool {
        tool_call_id: String,
        content:      MessageContent,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

impl MessageContent {
    /// Flatten multipart content to a plain string (best-effort).
    pub fn as_text(&self) -> String {
        match self {
            Self::Text(s)   => s.clone(),
            Self::Parts(ps) => ps
                .iter()
                .filter_map(|p| if let ContentPart::Text { text } = p { Some(text.as_str()) } else { None })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text  { text:      String },
    Image { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
    #[serde(default)]
    pub detail: Option<String>,
}

// ── Tool injection / extraction ───────────────────────────────────────────────

/// Build a compact tool block for small context windows (≤ 4096 tokens).
/// Minimal instructions, no examples, terse parameter listing.
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
            names.push(format!("{name}"));
        } else {
            names.push(format!("{name}({})", params.join(",")));
        }
    }
    format!(
r#"Tools: {}
ALWAYS use tools when applicable. Do NOT show commands in code blocks — call them.
Call: [TOOL_CALL]{{"name":"<tool>","arguments":{{...}}}}[/TOOL_CALL]
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
Assistant: [TOOL_CALL]{{"name":"location","arguments":{{}}}}[/TOOL_CALL]"#
    )
}

/// Inject tool definitions and calling instructions into the system prompt.
///
/// llama.cpp local models do not have native function-calling support in all
/// builds; we inject a system prompt block that:
///   1. Lists available tools with their JSON Schema parameters.
///   2. Tells the model the exact format to emit a tool call.
///   3. Explains the tool-result flow so the model waits for results.
///
/// The extractor (`extract_tool_calls`) accepts several formats, but we teach
/// the model the `[TOOL_CALL]…[/TOOL_CALL]` format which is the most reliable
/// for local models (unambiguous delimiters, no fence/JSON confusion).
pub fn inject_tools_into_system_prompt(
    messages: &mut Vec<Value>,
    tools:    &[Tool],
    n_ctx:    usize,
) {
    if tools.is_empty() { return; }

    // Use a compact tool prompt for small context windows (≤ 4096 tokens)
    // to leave room for conversation history and the model's response.
    let compact = n_ctx > 0 && n_ctx <= 4096;

    let tool_block = if compact {
        build_compact_tool_block(tools)
    } else {
        build_full_tool_block(tools)
    };

    // Prepend to or create the first system message.
    let has_system = messages.first().and_then(|m| m.get("role")).and_then(|r| r.as_str()) == Some("system");

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

/// Extract tool calls from a raw assistant message body.
///
/// llama-server returns tool calls in `[TOOL_CALL]…[/TOOL_CALL]` blocks
/// or (in newer builds) as structured JSON under `tool_calls`.
pub fn extract_tool_calls(content: &str) -> Vec<ToolCall> {
    const START: &str = "[TOOL_CALL]";
    const END:   &str = "[/TOOL_CALL]";

    let mut calls = Vec::new();
    let mut dedup = HashSet::<(String, String)>::new();
    let mut remaining = content;

    while let Some(s) = remaining.find(START) {
        let after_start = &remaining[s + START.len()..];
        if let Some(e) = after_start.find(END) {
            let block = after_start[..e].trim();
            if let Ok(v) = serde_json::from_str::<Value>(block) {
                let name = v.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                let args = v.get("arguments")
                    .map(|a| if a.is_string() {
                        a.as_str().unwrap().to_string()
                    } else {
                        a.to_string()
                    })
                    .unwrap_or_else(|| "{}".to_string());

                push_tool_call(&mut calls, &mut dedup, name, args);
            }
            remaining = &after_start[e + END.len()..];
        } else {
            break;
        }
    }

    extract_tool_calls_from_json_text(content, &mut calls, &mut dedup);

    calls
}

fn push_tool_call(
    calls: &mut Vec<ToolCall>,
    dedup: &mut HashSet<(String, String)>,
    name: String,
    arguments: String,
) {
    let name = name.trim().to_string();
    if name.is_empty() {
        return;
    }
    let key = (name.clone(), arguments.clone());
    if !dedup.insert(key) {
        return;
    }

    calls.push(ToolCall {
        id: format!("call_{}", calls.len()),
        call_type: "function".into(),
        function: ToolCallFunction { name, arguments },
    });
}

fn args_to_json_string(v: Option<&Value>) -> String {
    match v {
        Some(a) if a.is_string() => a.as_str().unwrap_or("{}").to_string(),
        Some(a)                  => a.to_string(),
        None                     => "{}".to_string(),
    }
}

fn tool_name_from_value(v: &Value) -> String {
    v.get("name")
        .or_else(|| v.get("tool"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

fn extract_calls_from_value(v: &Value, calls: &mut Vec<ToolCall>, dedup: &mut HashSet<(String, String)>) {
    // Top-level array of tool-call objects (Qwen3.5 emits this format):
    //   [{"name":"date","parameters":{}},{"name":"location","parameters":{}}]
    if let Some(arr) = v.as_array() {
        for item in arr {
            extract_calls_from_value(item, calls, dedup);
        }
        return;
    }

    // OpenAI-style envelope: { "tool_calls": [ ... ] }
    if let Some(arr) = v.get("tool_calls").and_then(|x| x.as_array()) {
        for item in arr {
            let func = item.get("function").unwrap_or(item);
            let mut name = tool_name_from_value(func);
            if name.is_empty() {
                name = tool_name_from_value(item);
            }
            let args = args_to_json_string(func.get("arguments").or_else(|| func.get("parameters")));
            push_tool_call(calls, dedup, name, args);
        }
        return;
    }

    // Dict-style multi-tool call: { "date": {}, "location": {} }
    // Keys are tool names, values are parameter objects.
    if is_dict_style_multi_tool(v) {
        if let Some(obj) = v.as_object() {
            for (name, params) in obj {
                let args = if params.is_object() && !params.as_object().unwrap().is_empty() {
                    params.to_string()
                } else {
                    "{}".to_string()
                };
                push_tool_call(calls, dedup, name.clone(), args);
            }
        }
        return;
    }

    // Single call object forms:
    // {"name":"date","parameters":{}}
    // {"tool":"date","parameters":{}}
    // {"name":"date","arguments":"{}"}
    // {"function":{"name":"date","arguments":{}}}
    let single = if let Some(f) = v.get("function") { f } else { v };
    let name = tool_name_from_value(single);
    if !name.is_empty() {
        let args = args_to_json_string(single.get("arguments").or_else(|| single.get("parameters")));
        push_tool_call(calls, dedup, name, args);
    }
}

fn is_tool_call_value(v: &Value) -> bool {
    // Top-level array of tool-call objects
    if let Some(arr) = v.as_array() {
        return arr.iter().any(is_tool_call_value);
    }
    if v.get("tool_calls").and_then(|x| x.as_array()).is_some() {
        return true;
    }
    if is_dict_style_multi_tool(v) {
        return true;
    }
    let single = if let Some(f) = v.get("function") { f } else { v };
    !tool_name_from_value(single).is_empty()
}

fn extract_tool_calls_from_json_text(
    content: &str,
    calls: &mut Vec<ToolCall>,
    dedup: &mut HashSet<(String, String)>,
) {
    // 1) Code fences: JSON tool calls + bash/sh command fallback
    let mut cursor = 0usize;
    while let Some(rel) = content[cursor..].find("```") {
        let fence_start = cursor + rel;
        let after_open = fence_start + 3;
        let Some(nl_rel) = content[after_open..].find('\n') else {
            break;
        };
        let header_end = after_open + nl_rel;
        let header = content[after_open..header_end].trim().to_ascii_lowercase();
        let body_start = header_end + 1;
        let Some(close_rel) = content[body_start..].find("```") else {
            break;
        };
        let body_end = body_start + close_rel;
        let body = content[body_start..body_end].trim();

        if (header.is_empty() || header == "json") && !body.is_empty() {
            if let Ok(v) = serde_json::from_str::<Value>(body) {
                extract_calls_from_value(&v, calls, dedup);
            }
        }

        // Fallback: if the model emits a ```bash or ```sh code fence instead
        // of a [TOOL_CALL], treat the body as a bash tool call.  This catches
        // the common case where small models show the command in a code block
        // rather than using the proper tool-call format.
        if (header == "bash" || header == "sh" || header == "shell" || header == "zsh")
            && !body.is_empty()
            && calls.is_empty()
        {
            let cmd = body.to_string();
            push_tool_call(calls, dedup, "bash".into(), format!(r#"{{"command":{}}}"#, serde_json::to_string(&cmd).unwrap_or_else(|_| format!("\"{}\"", cmd))));
        }

        cursor = body_end + 3;
    }

    // 2) Bare JSON objects embedded in prose.
    //    We scan balanced {...} ranges and try to parse each range as JSON.
    for (start, end) in find_balanced_json_objects(content) {
        if let Ok(v) = serde_json::from_str::<Value>(&content[start..end]) {
            extract_calls_from_value(&v, calls, dedup);
        }
    }

    // 3) Bare JSON arrays embedded in prose (Qwen3.5 emits [{"name":"date",...}]).
    for (start, end) in find_balanced_json_arrays(content) {
        if let Ok(v) = serde_json::from_str::<Value>(&content[start..end]) {
            extract_calls_from_value(&v, calls, dedup);
        }
    }
}

fn find_balanced_json_objects(content: &str) -> Vec<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut out = Vec::new();

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut start = None::<usize>;

    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match b {
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match b {
            b'"' => in_string = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        out.push((s, i + 1));
                    }
                }
            }
            _ => {}
        }
    }

    out
}

/// Find balanced `[…]` JSON array ranges in text (for Qwen3.5 array-style tool calls).
fn find_balanced_json_arrays(content: &str) -> Vec<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut out = Vec::new();

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut start = None::<usize>;

    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match b {
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match b {
            b'"' => in_string = true,
            b'[' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b']' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        out.push((s, i + 1));
                    }
                }
            }
            _ => {}
        }
    }

    out
}

/// Remove `[TOOL_CALL]…[/TOOL_CALL]` markers from assistant message content.
pub fn strip_tool_call_blocks_preserve(content: &str) -> String {
    const START: &str = "[TOOL_CALL]";
    const END:   &str = "[/TOOL_CALL]";

    let mut out    = String::new();
    let mut cursor = 0;
    let bytes      = content.as_bytes();

    while cursor < bytes.len() {
        if let Some(s) = content[cursor..].find(START) {
            out.push_str(&content[cursor..cursor + s]);
            let after = cursor + s + START.len();
            if let Some(e) = content[after..].find(END) {
                cursor = after + e + END.len();
            } else {
                break;
            }
        } else {
            out.push_str(&content[cursor..]);
            break;
        }
    }

    strip_json_tool_call_payloads_preserve(&out)
}

fn strip_json_tool_call_payloads_preserve(content: &str) -> String {
    let mut ranges = Vec::<(usize, usize)>::new();

    // Strip fenced JSON blocks that are tool-call payloads.
    let mut cursor = 0usize;
    while let Some(rel) = content[cursor..].find("```") {
        let fence_start = cursor + rel;
        let after_open = fence_start + 3;
        let Some(nl_rel) = content[after_open..].find('\n') else {
            break;
        };
        let header_end = after_open + nl_rel;
        let header = content[after_open..header_end].trim().to_ascii_lowercase();
        let body_start = header_end + 1;
        let Some(close_rel) = content[body_start..].find("```") else {
            break;
        };
        let body_end = body_start + close_rel;
        let body = content[body_start..body_end].trim();

        if (header.is_empty() || header == "json") && !body.is_empty() {
            if let Ok(v) = serde_json::from_str::<Value>(body) {
                if is_tool_call_value(&v) {
                    ranges.push((fence_start, body_end + 3));
                }
            }
        }

        cursor = body_end + 3;
    }

    // Strip inline JSON objects that are tool-call payloads.
    for (start, end) in find_balanced_json_objects(content) {
        if let Ok(v) = serde_json::from_str::<Value>(&content[start..end]) {
            if is_tool_call_value(&v) {
                ranges.push((start, end));
            }
        }
    }

    // Strip inline JSON arrays that are tool-call payloads.
    for (start, end) in find_balanced_json_arrays(content) {
        if let Ok(v) = serde_json::from_str::<Value>(&content[start..end]) {
            if is_tool_call_value(&v) {
                ranges.push((start, end));
            }
        }
    }

    if let Some((start, end)) = find_incomplete_trailing_tool_call_range(content) {
        ranges.push((start, end));
    }

    if ranges.is_empty() {
        return content.to_string();
    }

    ranges.sort_by_key(|(s, _)| *s);
    let mut merged = Vec::<(usize, usize)>::new();
    for (s, e) in ranges {
        if let Some((_, last_e)) = merged.last_mut() {
            if s <= *last_e {
                if e > *last_e {
                    *last_e = e;
                }
                continue;
            }
        }
        merged.push((s, e));
    }

    let mut out = String::new();
    let mut keep_from = 0usize;
    for (s, e) in merged {
        if s > keep_from {
            out.push_str(&content[keep_from..s]);
        }
        keep_from = e;
    }
    if keep_from < content.len() {
        out.push_str(&content[keep_from..]);
    }

    out
}

fn find_incomplete_trailing_tool_call_range(content: &str) -> Option<(usize, usize)> {
    find_incomplete_trailing_fenced_tool_call_range(content)
        .or_else(|| find_incomplete_trailing_inline_tool_call_range(content))
}

fn find_incomplete_trailing_fenced_tool_call_range(content: &str) -> Option<(usize, usize)> {
    let fence_start = content.rfind("```")?;
    let after_open = fence_start + 3;
    if after_open >= content.len() {
        return None;
    }

    if content[after_open..].contains("```") {
        return None;
    }

    let nl_rel = content[after_open..].find('\n')?;
    let header_end = after_open + nl_rel;
    let header = content[after_open..header_end].trim().to_ascii_lowercase();
    if !header.is_empty() && header != "json" {
        return None;
    }

    let body = content[header_end + 1..].trim_start();
    if looks_like_tool_call_json_prefix(body) {
        let end = content[fence_start..]
            .find("<think>")
            .map(|idx| fence_start + idx)
            .unwrap_or(content.len());
        return Some((fence_start, end));
    }

    None
}

fn find_incomplete_trailing_inline_tool_call_range(content: &str) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut start = None::<usize>;

    for (i, b) in content.bytes().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match b {
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match b {
            b'"' => in_string = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        start = None;
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(start) = start {
        let tail = &content[start..];
        if looks_like_tool_call_json_prefix(tail) {
            let end = content[start..]
                .find("<think>")
                .map(|idx| start + idx)
                .unwrap_or(content.len());
            return Some((start, end));
        }
    }

    // Also check for unclosed `[` (array-style tool calls from Qwen3.5).
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut arr_start = None::<usize>;

    for (i, b) in content.bytes().enumerate() {
        if in_string {
            if escaped { escaped = false; continue; }
            match b {
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'[' => { if depth == 0 { arr_start = Some(i); } depth += 1; }
            b']' => { if depth > 0 { depth -= 1; if depth == 0 { arr_start = None; } } }
            _ => {}
        }
    }

    if let Some(start) = arr_start {
        let tail = &content[start..];
        if looks_like_tool_call_json_prefix(tail) {
            let end = content[start..]
                .find("<think>")
                .map(|idx| start + idx)
                .unwrap_or(content.len());
            return Some((start, end));
        }
    }

    None
}

fn looks_like_tool_call_json_prefix(s: &str) -> bool {
    let trimmed = s.trim_start();
    // Accept both `{` (object) and `[` (array) prefixes.
    // Qwen3.5 emits arrays like [{"name":"date","parameters":{}}].
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return false;
    }

    let probe: String = trimmed.chars().take(320).collect::<String>().to_ascii_lowercase();

    // Dict-style: any known tool name appears as a JSON key (e.g. "date":)
    let is_dict_style = KNOWN_TOOL_NAMES.iter().any(|n| {
        probe.contains(&format!("\"{}\":", n)) || probe.contains(&format!("\"{}\": ", n))
    });
    if is_dict_style {
        return true;
    }

    let mentions_tool_name = probe.contains("\"name\"")
        || probe.contains("\"tool\"")
        || probe.contains("\"tool_calls\"")
        || probe.contains("\"function\"");
    let mentions_args = probe.contains("\"parameter")
        || probe.contains("\"argument")
        || probe.contains("<think>");

    mentions_tool_name && mentions_args
}

pub fn strip_tool_call_blocks(content: &str) -> String {
    strip_tool_call_blocks_preserve(content).trim().to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_empty() {
        assert!(extract_tool_calls("Hello world").is_empty());
    }

    #[test]
    fn extract_single() {
        let msg = r#"Sure! [TOOL_CALL]{"name":"get_weather","arguments":{"city":"London"}}[/TOOL_CALL]"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "get_weather");
    }

        #[test]
        fn extract_openai_style_single_json_object() {
                let msg = r#"I'll use the date tool now.
json
{
    "name": "date",
    "parameters": {}
}"#;
                let calls = extract_tool_calls(msg);
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].function.name, "date");
                assert_eq!(calls[0].function.arguments, "{}");
        }

            #[test]
            fn extract_tool_key_alias_single_json_object() {
                let msg = r#"The user is asking about the current time.
        I'll fetch it now.
        json
        Copy
        {
          "tool": "date",
          "parameters": {}
        }
        I'll fetch that information for you right away."#;
                let calls = extract_tool_calls(msg);
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].function.name, "date");
                assert_eq!(calls[0].function.arguments, "{}");
            }

        #[test]
        fn extract_openai_tool_calls_envelope() {
                let msg = r#"```json
{
    "tool_calls": [
        {
            "type": "function",
            "function": {
                "name": "date",
                "arguments": "{}"
            }
        }
    ]
}
```"#;
                let calls = extract_tool_calls(msg);
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].function.name, "date");
                assert_eq!(calls[0].function.arguments, "{}");
        }

    #[test]
    fn strip_blocks() {
        let msg = r#"Here you go. [TOOL_CALL]{"name":"foo","arguments":{}}[/TOOL_CALL] Done."#;
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("[TOOL_CALL]"));
        assert!(stripped.contains("Done."));
    }

    #[test]
    fn strip_inline_json_tool_payload() {
        let msg = r#"I'll use a tool.
{"name":"date","parameters":{}}
Then answer naturally."#;
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("\"name\":\"date\""));
        assert!(stripped.contains("Then answer naturally."));
    }

    #[test]
    fn keep_non_tool_json_blocks() {
        let msg = r#"```json
{"status":"ok","count":3}
```"#;
        let stripped = strip_tool_call_blocks(msg);
        assert!(stripped.contains("\"status\":\"ok\""));
    }

    #[test]
    fn extract_dict_style_multi_tool() {
        let msg = "I'll get that information for you.\n```json\n{\n  \"date\": {},\n  \"location\": {}\n}\n```\nLet me fetch that for you.";
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 2, "expected 2 calls, got: {:?}", calls.iter().map(|c| &c.function.name).collect::<Vec<_>>());
        let names: Vec<&str> = calls.iter().map(|c| c.function.name.as_str()).collect();
        assert!(names.contains(&"date"),     "missing date");
        assert!(names.contains(&"location"), "missing location");
    }

    #[test]
    fn strip_dict_style_multi_tool_fence() {
        let msg = "I'll get that.\n```json\n{\n  \"date\": {},\n  \"location\": {}\n}\n```\nDone.";
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("\"date\""), "date key should be stripped");
        assert!(!stripped.contains("\"location\""), "location key should be stripped");
        assert!(stripped.contains("Done."), "prose should survive");
    }

    #[test]
    fn extract_array_style_tool_calls() {
        let msg = r#"I'll get that info.
```json
[
  {"name": "date", "parameters": {}},
  {"name": "location", "parameters": {}}
]
```"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 2, "expected 2, got {:?}", calls.iter().map(|c| &c.function.name).collect::<Vec<_>>());
        let names: Vec<&str> = calls.iter().map(|c| c.function.name.as_str()).collect();
        assert!(names.contains(&"date"));
        assert!(names.contains(&"location"));
    }

    #[test]
    fn strip_array_style_tool_call_fence() {
        let msg = r#"I'll get that info.
```json
[
  {"name": "date", "parameters": {}},
  {"name": "location", "parameters": {}}
]
```
Done."#;
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("\"name\""), "tool call JSON should be stripped");
        assert!(stripped.contains("Done."));
    }

    #[test]
    fn strip_incomplete_array_tool_call() {
        let msg = "I'll get that.\n[\n  {\n    \"name\": \"date\",\n    \"parameterarameter";
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("parameterarameter"), "incomplete array should be stripped: got '{}'", stripped);
    }

    #[test]
    fn validate_tool_args_valid() {
        let tool = Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "web_search".into(),
                description: Some("Search the web".into()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                })),
            },
        };
        let args = serde_json::json!({"query": "test"});
        let result = validate_tool_arguments(&tool, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_tool_args_missing_required() {
        let tool = Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "web_search".into(),
                description: Some("Search the web".into()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                })),
            },
        };
        let args = serde_json::json!({});
        let result = validate_tool_arguments(&tool, &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Validation failed"));
    }

    #[test]
    fn validate_tool_args_no_schema() {
        let tool = Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "date".into(),
                description: Some("Get date".into()),
                parameters: None,
            },
        };
        let args = serde_json::json!({"anything": true});
        let result = validate_tool_arguments(&tool, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_tool_args_wrong_type() {
        let tool = Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "web_search".into(),
                description: None,
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"]
                })),
            },
        };
        let args = serde_json::json!({"query": 123});
        let result = validate_tool_arguments(&tool, &args);
        assert!(result.is_err());
    }

    #[test]
    fn strip_incomplete_fenced_tool_payload_before_think() {
        let msg = "```json\n{\n  \"name\": \"date\",\n  \"parameter<think>thinking</think>\nFinal answer.";
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("```json"));
        assert!(!stripped.contains("\"name\": \"date\""));
        assert!(stripped.contains("<think>thinking</think>"));
        assert!(stripped.contains("Final answer."));
    }

    #[test]
    fn extract_bash_code_fence_fallback() {
        let msg = "To list files on your desktop:\n```bash\nls ~/Desktop/\n```";
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1, "should extract one bash tool call");
        assert_eq!(calls[0].function.name, "bash");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(args["command"].as_str().unwrap(), "ls ~/Desktop/");
    }

    #[test]
    fn extract_sh_code_fence_fallback() {
        let msg = "```sh\ndf -h\n```";
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "bash");
    }

    #[test]
    fn bash_fence_fallback_skipped_when_tool_call_present() {
        // If a proper [TOOL_CALL] is already present, bash fence should not add duplicates
        let msg = r#"[TOOL_CALL]{"name":"bash","arguments":{"command":"ls"}}[/TOOL_CALL]
Also:
```bash
echo hello
```"#;
        let calls = extract_tool_calls(msg);
        // Should only have the explicit tool call, not the code fence
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.arguments, r#"{"command":"ls"}"#);
    }
}
