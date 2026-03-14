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

/// Built-in tool names used for dict-style multi-tool recognition:
///   { "date": {}, "location": {} }
/// These must stay in sync with `enabled_builtin_llm_tools` in mod.rs.
const KNOWN_TOOL_NAMES: &[&str] = &["date", "location", "web_search", "web_fetch"];

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

/// Inject tool definitions as a system prompt prefix that llama.cpp can parse.
///
/// llama-server v0.0.x does not natively support function calling in all
/// builds; injecting a system prompt with JSON schema is a portable fallback.
pub fn inject_tools_into_system_prompt(
    messages: &mut Vec<Value>,
    tools:    &[Tool],
) {
    if tools.is_empty() { return; }

    let schema: Vec<Value> = tools.iter().map(|t| {
        serde_json::json!({
            "name":        t.function.name,
            "description": t.function.description,
            "parameters":  t.function.parameters,
        })
    }).collect();

    let tool_block = format!(
        "[TOOL_SCHEMA]\n{}\n[/TOOL_SCHEMA]",
        serde_json::to_string_pretty(&schema).unwrap_or_default()
    );

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
    // 1) JSON code fences (```json ... ``` and ``` ... ```)
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

        cursor = body_end + 3;
    }

    // 2) Bare JSON objects embedded in prose.
    //    We scan balanced {...} ranges and try to parse each range as JSON.
    for (start, end) in find_balanced_json_objects(content) {
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

    let start = start?;
    let tail = &content[start..];
    if looks_like_tool_call_json_prefix(tail) {
        let end = content[start..]
            .find("<think>")
            .map(|idx| start + idx)
            .unwrap_or(content.len());
        return Some((start, end));
    }
    None
}

fn looks_like_tool_call_json_prefix(s: &str) -> bool {
    let trimmed = s.trim_start();
    if !trimmed.starts_with('{') {
        return false;
    }

    let probe: String = trimmed.chars().take(240).collect::<String>().to_ascii_lowercase();

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
    let mentions_args = probe.contains("\"parameters")
        || probe.contains("\"arguments")
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
    fn strip_incomplete_fenced_tool_payload_before_think() {
        let msg = "```json\n{\n  \"name\": \"date\",\n  \"parameter<think>thinking</think>\nFinal answer.";
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("```json"));
        assert!(!stripped.contains("\"name\": \"date\""));
        assert!(stripped.contains("<think>thinking</think>"));
        assert!(stripped.contains("Final answer."));
    }
}
