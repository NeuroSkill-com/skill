// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Stripping tool-call blocks from assistant message content.
//!
//! Removes `[TOOL_CALL]…[/TOOL_CALL]`, Llama XML `<function=…>…</function>`,
//! inline JSON tool payloads, and trailing partial tag prefixes (for streaming).

use serde_json::Value;
use skill_constants::{TOOL_CALL_START, TOOL_CALL_END};

use super::extract::{is_tool_call_value, looks_like_tool_call_json_prefix};
use super::json_scan::{find_balanced_json_objects, find_balanced_json_arrays};

/// Remove `[TOOL_CALL]…[/TOOL_CALL]` markers from assistant message content.
///
/// Also strips trailing partial prefixes of `[TOOL_CALL]` (e.g. `[TOOL_CA`)
/// so that the streaming sanitizer holds them back instead of emitting them
/// as visible text to clients.
pub fn strip_tool_call_blocks_preserve(content: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0;

    while cursor < content.len() {
        if let Some(s) = content[cursor..].find(TOOL_CALL_START) {
            out.push_str(&content[cursor..cursor + s]);
            let after = cursor + s + TOOL_CALL_START.len();
            if let Some(e) = content[after..].find(TOOL_CALL_END) {
                cursor = after + e + TOOL_CALL_END.len();
            } else {
                break;
            }
        } else {
            out.push_str(&content[cursor..]);
            break;
        }
    }

    strip_trailing_tag_prefix(&mut out, TOOL_CALL_START);
    strip_trailing_tag_prefix(&mut out, TOOL_CALL_END);

    let out = strip_llama_xml_tool_call_blocks(&out);
    strip_json_tool_call_payloads_preserve(&out)
}

/// Strip tool-call blocks and trim whitespace.
pub fn strip_tool_call_blocks(content: &str) -> String {
    strip_tool_call_blocks_preserve(content).trim().to_string()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// If `out` ends with a string that is a proper prefix of `tag`, remove it.
fn strip_trailing_tag_prefix(out: &mut String, tag: &str) {
    for prefix_len in (1..tag.len()).rev() {
        let prefix = &tag[..prefix_len];
        if out.ends_with(prefix) {
            out.truncate(out.len() - prefix_len);
            return;
        }
    }
}

/// Strip Llama-family XML tool-call blocks from content.
fn strip_llama_xml_tool_call_blocks(content: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0;

    while let Some(fn_start) = content[cursor..].find("<function=") {
        out.push_str(&content[cursor..cursor + fn_start]);
        let after_eq = cursor + fn_start + "<function=".len();
        if let Some(fn_close) = content[after_eq..].find("</function>") {
            cursor = after_eq + fn_close + "</function>".len();
        } else {
            cursor = content.len();
            break;
        }
    }

    if cursor < content.len() {
        out.push_str(&content[cursor..]);
    }

    strip_trailing_tag_prefix(&mut out, "<function=");
    out
}

fn strip_json_tool_call_payloads_preserve(content: &str) -> String {
    let mut ranges = Vec::<(usize, usize)>::new();

    // Strip fenced JSON blocks that are tool-call payloads.
    let mut cursor = 0usize;
    while let Some(rel) = content[cursor..].find("```") {
        let fence_start = cursor + rel;
        let after_open = fence_start + 3;
        let Some(nl_rel) = content[after_open..].find('\n') else { break };
        let header_end = after_open + nl_rel;
        let header = content[after_open..header_end].trim().to_ascii_lowercase();
        let body_start = header_end + 1;
        let Some(close_rel) = content[body_start..].find("```") else { break };
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

    // Strip inline JSON arrays.
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

    // Merge overlapping ranges and excise.
    ranges.sort_by_key(|(s, _)| *s);
    let mut merged = Vec::<(usize, usize)>::new();
    for (s, e) in ranges {
        if let Some((_, last_e)) = merged.last_mut() {
            if s <= *last_e {
                if e > *last_e { *last_e = e; }
                continue;
            }
        }
        merged.push((s, e));
    }

    let mut out = String::new();
    let mut keep_from = 0usize;
    for (s, e) in merged {
        if s > keep_from { out.push_str(&content[keep_from..s]); }
        keep_from = e;
    }
    if keep_from < content.len() {
        out.push_str(&content[keep_from..]);
    }
    out
}

fn find_incomplete_trailing_tool_call_range(content: &str) -> Option<(usize, usize)> {
    find_incomplete_trailing_fenced(content)
        .or_else(|| find_incomplete_trailing_inline(content))
}

fn find_incomplete_trailing_fenced(content: &str) -> Option<(usize, usize)> {
    let fence_start = content.rfind("```")?;
    let after_open = fence_start + 3;
    if after_open >= content.len() { return None }
    if content[after_open..].contains("```") { return None }

    let nl_rel = content[after_open..].find('\n')?;
    let header_end = after_open + nl_rel;
    let header = content[after_open..header_end].trim().to_ascii_lowercase();
    if !header.is_empty() && header != "json" { return None }

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

fn find_incomplete_trailing_inline(content: &str) -> Option<(usize, usize)> {
    // Check for unclosed `{` (object-style tool calls).
    if let Some(range) = find_unclosed_brace(content, b'{', b'}') {
        return Some(range);
    }
    // Also check for unclosed `[` (array-style tool calls from Qwen3.5).
    find_unclosed_brace(content, b'[', b']')
}

fn find_unclosed_brace(content: &str, open: u8, close: u8) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut start = None::<usize>;

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
            b if b == open => { if depth == 0 { start = Some(i); } depth += 1; }
            b if b == close => { if depth > 0 { depth -= 1; if depth == 0 { start = None; } } }
            _ => {}
        }
    }

    if let Some(s) = start {
        let tail = &content[s..];
        if looks_like_tool_call_json_prefix(tail) {
            let end = content[s..]
                .find("<think>")
                .map(|idx| s + idx)
                .unwrap_or(content.len());
            return Some((s, end));
        }
    }
    None
}
