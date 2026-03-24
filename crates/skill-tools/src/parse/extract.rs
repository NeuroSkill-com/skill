// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tool-call extraction from raw assistant message content.
//!
//! Supports multiple formats emitted by different LLM families:
//! - `[TOOL_CALL]…[/TOOL_CALL]` delimited blocks
//! - Llama-family XML: `<function=name><parameter=key>value</parameter></function>`
//! - Bare JSON objects / arrays with tool-call keys
//! - Bash/shell code fences as fallback

use serde_json::Value;
use std::collections::HashSet;

use skill_constants::{TOOL_CALL_START, TOOL_CALL_END};
use super::types::{ToolCall, ToolCallFunction};
use super::json_scan::{find_balanced_json_objects, find_balanced_json_arrays};

/// Built-in tool names used for dict-style multi-tool recognition.
pub(crate) const KNOWN_TOOL_NAMES: &[&str] = &[
    "date", "location", "web_search", "web_fetch", "bash",
    "read_file", "write_file", "edit_file", "search_output", "skill",
];

/// Extract tool calls from a raw assistant message body.
///
/// llama-server returns tool calls in `[TOOL_CALL]…[/TOOL_CALL]` blocks
/// or (in newer builds) as structured JSON under `tool_calls`.
pub fn extract_tool_calls(content: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    let mut dedup = HashSet::<(String, String)>::new();

    // Phase 1: [TOOL_CALL]…[/TOOL_CALL] blocks
    extract_delimited_blocks(content, &mut calls, &mut dedup);

    // Phase 2: Llama XML format
    extract_llama_xml_tool_calls(content, &mut calls, &mut dedup);

    // Phase 3: JSON in code fences and inline
    extract_tool_calls_from_json_text(content, &mut calls, &mut dedup);

    // Post-process: if any bash call has empty arguments, try to fill from
    // a ```bash/sh code fence in the content.
    if let Some(cmd) = extract_bash_fence_command(content) {
        for tc in &mut calls {
            if tc.function.name == "bash" {
                let args: Value = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(Value::Object(Default::default()));
                let command_empty = args.get("command")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .is_empty();
                if command_empty {
                    tc.function.arguments = format!(
                        r#"{{"command":{}}}"#,
                        serde_json::to_string(&cmd).unwrap_or_else(|_| format!("\"{cmd}\""))
                    );
                }
            }
        }
    }

    // If no tool calls were found at all but there's a bash fence, create one.
    if calls.is_empty() {
        if let Some(cmd) = extract_bash_fence_command(content) {
            push_tool_call(
                &mut calls, &mut dedup, "bash".into(),
                format!(
                    r#"{{"command":{}}}"#,
                    serde_json::to_string(&cmd).unwrap_or_else(|_| format!("\"{cmd}\""))
                ),
            );
        }
    }

    // Post-process: redirect Skill API sub-commands called as top-level tools.
    redirect_skill_aliases(&mut calls);

    calls
}

/// Detect whether the assistant output contains a garbled / malformed tool-call
/// attempt that `extract_tool_calls` could not parse.
///
/// Returns `Some(raw_fragment)` with the offending text when a failed attempt
/// is detected, or `None` when the output is clean.
pub fn detect_garbled_tool_call(content: &str) -> Option<String> {
    let parsed = extract_tool_calls(content);
    if !parsed.is_empty() {
        return None;
    }

    let has_start_tag = content.contains(TOOL_CALL_START);
    let has_function_xml = content.contains("<function=");
    let has_tool_call_json = {
        let lower = content.to_ascii_lowercase();
        (lower.contains("\"name\"") || lower.contains("\"tool\""))
            && (lower.contains("\"arguments\"") || lower.contains("\"parameters\""))
    };

    if has_start_tag {
        if let Some(s) = content.find(TOOL_CALL_START) {
            let fragment: String = content[s..].chars().take(500).collect();
            return Some(fragment);
        }
    }

    if has_function_xml {
        if let Some(s) = content.find("<function=") {
            let fragment: String = content[s..].chars().take(500).collect();
            return Some(fragment);
        }
    }

    if has_tool_call_json {
        for (start, end) in find_balanced_json_objects(content) {
            let blob = &content[start..end];
            let lower = blob.to_ascii_lowercase();
            if (lower.contains("\"name\"") || lower.contains("\"tool\""))
                && (lower.contains("\"arguments\"") || lower.contains("\"parameters\""))
            {
                let fragment: String = blob.chars().take(500).collect();
                return Some(fragment);
            }
        }
        if let Some(pos) = content.find('{') {
            let tail = &content[pos..];
            let lower = tail.to_ascii_lowercase();
            if (lower.contains("\"name\"") || lower.contains("\"tool\""))
                && (lower.contains("\"argument") || lower.contains("\"parameter"))
            {
                let fragment: String = tail.chars().take(500).collect();
                return Some(fragment);
            }
        }
    }

    None
}

/// Build a corrective user message for the self-healing loop.
pub fn build_self_healing_message(garbled_fragment: &str) -> String {
    format!(
        "Your previous tool call could not be parsed. Here is what you emitted:\n\n\
         ```\n{garbled_fragment}\n```\n\n\
         Please re-emit the tool call in the correct format:\n\
         [TOOL_CALL]{{\"name\":\"<tool_name>\",\"arguments\":{{...}}}}[/TOOL_CALL]\n\n\
         Make sure the JSON is valid and on a single line. Do NOT add any explanation — just emit the corrected tool call."
    )
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn extract_delimited_blocks(
    content: &str,
    calls: &mut Vec<ToolCall>,
    dedup: &mut HashSet<(String, String)>,
) {
    let mut remaining = content;
    while let Some(s) = remaining.find(TOOL_CALL_START) {
        let after_start = &remaining[s + TOOL_CALL_START.len()..];
        let Some(e) = after_start.find(TOOL_CALL_END) else { break };
        let block = after_start[..e].trim();
        if let Ok(v) = serde_json::from_str::<Value>(block) {
            let name = v.get("name")
                .or_else(|| v.get("tool"))
                .and_then(|n| n.as_str())
                .unwrap_or("").to_string();
            let args = args_to_json_string(v.get("arguments").or_else(|| v.get("parameters")));
            push_tool_call(calls, dedup, name, args);
        }
        remaining = &after_start[e + TOOL_CALL_END.len()..];
    }
}

/// Parse Llama-family XML tool-call format:
///
/// ```text
/// <function=tool_name><parameter=key>value</parameter></function>
/// ```
fn extract_llama_xml_tool_calls(
    content: &str,
    calls: &mut Vec<ToolCall>,
    dedup: &mut HashSet<(String, String)>,
) {
    let mut remaining = content;

    while let Some(fn_start) = remaining.find("<function=") {
        let after_eq = &remaining[fn_start + "<function=".len()..];
        let Some(gt_pos) = after_eq.find('>') else { break };
        let name = after_eq[..gt_pos].trim().to_string();
        let inner_start = &after_eq[gt_pos + 1..];
        let Some(fn_end) = inner_start.find("</function>") else { break };
        let body = &inner_start[..fn_end];

        let mut params = serde_json::Map::new();
        let mut param_remaining = body;
        let mut found_params = false;

        while let Some(p_start) = param_remaining.find("<parameter=") {
            found_params = true;
            let after_p_eq = &param_remaining[p_start + "<parameter=".len()..];
            let Some(p_gt) = after_p_eq.find('>') else { break };
            let param_name = after_p_eq[..p_gt].trim().to_string();
            let val_start = &after_p_eq[p_gt + 1..];
            let Some(p_end) = val_start.find("</parameter>") else { break };
            let raw_value = val_start[..p_end].trim();

            let value: Value = serde_json::from_str(raw_value)
                .unwrap_or_else(|_| Value::String(raw_value.to_string()));

            params.insert(param_name, value);
            param_remaining = &val_start[p_end + "</parameter>".len()..];
        }

        if !found_params {
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
                    let args = if v.is_object() { v.to_string() } else { "{}".to_string() };
                    push_tool_call(calls, dedup, name.clone(), args);
                    remaining = &inner_start[fn_end + "</function>".len()..];
                    continue;
                }
            }
        }

        let args = if params.is_empty() {
            "{}".to_string()
        } else {
            Value::Object(params).to_string()
        };
        push_tool_call(calls, dedup, name, args);
        remaining = &inner_start[fn_end + "</function>".len()..];
    }
}

fn extract_tool_calls_from_json_text(
    content: &str,
    calls: &mut Vec<ToolCall>,
    dedup: &mut HashSet<(String, String)>,
) {
    // 1) Code fences
    let mut cursor = 0usize;
    while let Some(rel) = content[cursor..].find("```") {
        let after_open = cursor + rel + 3;
        let Some(nl_rel) = content[after_open..].find('\n') else { break };
        let header_end = after_open + nl_rel;
        let header = content[after_open..header_end].trim().to_ascii_lowercase();
        let body_start = header_end + 1;
        let Some(close_rel) = content[body_start..].find("```") else { break };
        let body_end = body_start + close_rel;
        let body = content[body_start..body_end].trim();

        if (header.is_empty() || header == "json") && !body.is_empty() {
            if let Ok(v) = serde_json::from_str::<Value>(body) {
                extract_calls_from_value(&v, calls, dedup);
            }
        }
        cursor = body_end + 3;
    }

    // 2) Bare JSON objects
    for (start, end) in find_balanced_json_objects(content) {
        if let Ok(v) = serde_json::from_str::<Value>(&content[start..end]) {
            extract_calls_from_value(&v, calls, dedup);
        }
    }

    // 3) Bare JSON arrays
    for (start, end) in find_balanced_json_arrays(content) {
        if let Ok(v) = serde_json::from_str::<Value>(&content[start..end]) {
            extract_calls_from_value(&v, calls, dedup);
        }
    }
}

/// Extract the first bash/sh/shell/zsh code fence body from content.
pub(crate) fn extract_bash_fence_command(content: &str) -> Option<String> {
    let mut cursor = 0usize;
    while let Some(rel) = content[cursor..].find("```") {
        let after_open = cursor + rel + 3;
        let Some(nl_rel) = content[after_open..].find('\n') else { break };
        let header_end = after_open + nl_rel;
        let header = content[after_open..header_end].trim().to_ascii_lowercase();
        let body_start = header_end + 1;
        let Some(close_rel) = content[body_start..].find("```") else { break };
        let body_end = body_start + close_rel;
        let body = content[body_start..body_end].trim();

        if (header == "bash" || header == "sh" || header == "shell" || header == "zsh")
            && !body.is_empty()
        {
            return Some(body.to_string());
        }
        cursor = body_end + 3;
    }
    None
}

/// Redirect Skill API sub-commands and neuroskill aliases called as top-level tools.
fn redirect_skill_aliases(calls: &mut [ToolCall]) {
    for tc in calls.iter_mut() {
        if KNOWN_TOOL_NAMES.contains(&tc.function.name.as_str()) {
            continue;
        }
        if let Some(cmd) = crate::defs::resolve_skill_alias(&tc.function.name) {
            let orig_args: Value = serde_json::from_str(&tc.function.arguments)
                .unwrap_or_else(|_| serde_json::json!({}));
            let mut redirected = serde_json::json!({ "command": cmd });
            if let Some(obj) = orig_args.as_object() {
                if !obj.is_empty() {
                    redirected["args"] = orig_args;
                }
            }
            tc.function.name = "skill".to_string();
            tc.function.arguments = redirected.to_string();
        }
    }
}

/// Returns true if `v` is a dict-style multi-tool object whose keys are
/// (at least partially) known tool names and whose values are parameter objects.
pub(crate) fn is_dict_style_multi_tool(v: &Value) -> bool {
    let Some(obj) = v.as_object() else { return false };
    if obj.is_empty() { return false }
    let has_known_key = obj.keys().any(|k| KNOWN_TOOL_NAMES.contains(&k.as_str()));
    let all_obj_vals = obj.values().all(|v| v.is_object() || v.is_null());
    has_known_key && all_obj_vals
}

pub(crate) fn push_tool_call(
    calls: &mut Vec<ToolCall>,
    dedup: &mut HashSet<(String, String)>,
    name: String,
    arguments: String,
) {
    let name = name.trim().to_string();
    if name.is_empty() { return }
    let key = (name.clone(), arguments.clone());
    if !dedup.insert(key) { return }

    calls.push(ToolCall {
        id: format!("call_{}", calls.len()),
        call_type: "function".into(),
        function: ToolCallFunction { name, arguments },
    });
}

pub(crate) fn args_to_json_string(v: Option<&Value>) -> String {
    match v {
        Some(a) if a.is_string() => a.as_str().unwrap_or("{}").to_string(),
        Some(a) => a.to_string(),
        None => "{}".to_string(),
    }
}

fn tool_name_from_value(v: &Value) -> String {
    v.get("name")
        .or_else(|| v.get("tool"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

fn extract_calls_from_value(
    v: &Value,
    calls: &mut Vec<ToolCall>,
    dedup: &mut HashSet<(String, String)>,
) {
    if let Some(arr) = v.as_array() {
        for item in arr {
            extract_calls_from_value(item, calls, dedup);
        }
        return;
    }

    if let Some(arr) = v.get("tool_calls").and_then(|x| x.as_array()) {
        for item in arr {
            let func = item.get("function").unwrap_or(item);
            let mut name = tool_name_from_value(func);
            if name.is_empty() { name = tool_name_from_value(item); }
            let args = args_to_json_string(func.get("arguments").or_else(|| func.get("parameters")));
            push_tool_call(calls, dedup, name, args);
        }
        return;
    }

    if is_dict_style_multi_tool(v) {
        if let Some(obj) = v.as_object() {
            for (name, params) in obj {
                let args = if params.is_object() && params.as_object().is_some_and(|o| !o.is_empty()) {
                    params.to_string()
                } else {
                    "{}".to_string()
                };
                push_tool_call(calls, dedup, name.clone(), args);
            }
        }
        return;
    }

    // Skip tool *results* quoted in the response.
    if v.get("ok").is_some() || v.get("command").is_some() {
        return;
    }

    let single = if let Some(f) = v.get("function") { f } else { v };
    let name = tool_name_from_value(single);
    if !name.is_empty() {
        let args = args_to_json_string(single.get("arguments").or_else(|| single.get("parameters")));
        push_tool_call(calls, dedup, name, args);
    }
}

pub(crate) fn is_tool_call_value(v: &Value) -> bool {
    if v.get("ok").is_some() || v.get("command").is_some() {
        return false;
    }
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

/// Heuristic: does a JSON-ish prefix look like an incomplete tool-call?
pub(crate) fn looks_like_tool_call_json_prefix(s: &str) -> bool {
    let trimmed = s.trim_start();
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return false;
    }

    let probe: String = trimmed.chars().take(320).collect::<String>().to_ascii_lowercase();

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
