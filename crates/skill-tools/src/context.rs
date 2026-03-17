// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Context-aware history trimming for LLM tool conversations.

use serde_json::Value;
use crate::types::ToolContextCompression;

/// Rough estimate of token count for a string (~4 chars per token).
pub fn estimate_tokens(s: &str) -> usize {
    s.len() / 4 + 1
}

/// Estimate total token count across all messages.
pub fn estimate_messages_tokens(messages: &[Value]) -> usize {
    messages.iter().map(|m| {
        let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
        // Add overhead for role tags, separators (~10 tokens per message)
        estimate_tokens(content) + 10
    }).sum()
}

/// Trim conversation history to fit within the context window.
///
/// Strategy:
/// 1. Never remove the system message (index 0 if role == "system").
/// 2. Never remove the last user message (the current query).
/// 3. First, aggressively compress old tool results (especially web_search).
/// 4. Then truncate remaining long tool results to a hard cap.
/// 5. Then drop oldest non-system messages until the estimated
///    token count fits within 75% of `n_ctx` (leaving room for response).
pub fn trim_messages_to_fit(messages: &mut Vec<Value>, n_ctx: usize, compression: &ToolContextCompression) {
    if n_ctx == 0 { return; }
    let budget = n_ctx * 3 / 4; // 75% of context for prompt

    if compression.should_compress_old_results() {
        // Phase 1: Compress web_search / location tool results to a compact
        // summary.  These tools produce verbose JSON that the LLM only needs
        // briefly to decide the next action.
        let max_web_search_chars = compression.effective_max_search_result_chars();
        let msg_count = messages.len();
        for (i, msg) in messages.iter_mut().enumerate() {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            if role != "tool" { continue; }

            if let Some(content) = msg.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()) {
                let is_web_search = content.contains("\"tool\":\"web_search\"")
                    || content.contains("\"tool\": \"web_search\"");
                let is_location = content.contains("\"tool\":\"location\"")
                    || content.contains("\"tool\": \"location\"");

                // For older tool results (not the most recent pair), compress harder.
                let is_recent = i + 4 >= msg_count;

                if is_web_search {
                    let limit = if is_recent { max_web_search_chars } else { max_web_search_chars / 2 };
                    if content.len() > limit {
                        let summary = compact_web_search_result(&content, limit);
                        msg["content"] = Value::String(summary);
                    }
                } else if is_location && !is_recent && content.len() > 300 {
                    let summary = format!("{}…\n[location result — already consumed]",
                        &content[..content.len().min(200)]);
                    msg["content"] = Value::String(summary);
                }
            }
        }
    }

    // Phase 2: Truncate remaining long tool results.
    let max_result_chars = compression.effective_max_result_chars();
    for msg in messages.iter_mut() {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if role == "tool" {
            if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                if content.len() > max_result_chars {
                    let truncated = format!(
                        "{}…\n[truncated {} chars]",
                        &content[..max_result_chars],
                        content.len() - max_result_chars
                    );
                    msg["content"] = Value::String(truncated);
                }
            }
        }
    }

    // Phase 3: Drop oldest non-system, non-last-user messages if still too long.
    while estimate_messages_tokens(messages) > budget && messages.len() > 2 {
        let start = if messages.first()
            .and_then(|m| m.get("role"))
            .and_then(|r| r.as_str()) == Some("system")
        { 1 } else { 0 };

        if start >= messages.len() - 1 { break; }

        messages.remove(start);
    }
}

/// Compact a web_search JSON result to fit within `max_chars`.
///
/// Extracts just the query and a condensed list of titles + URLs,
/// dropping snippets and other metadata to save context.
fn compact_web_search_result(content: &str, max_chars: usize) -> String {
    // If the result is already in compact text format, just truncate.
    if let Ok(v) = serde_json::from_str::<Value>(content) {
        if let Some(compact_text) = v.get("compact").and_then(|c| c.as_str()) {
            if compact_text.len() <= max_chars {
                return compact_text.to_string();
            }
            return format!("{}…", &compact_text[..max_chars]);
        }

        // Legacy JSON format — convert to compact text.
        let query = v.get("query").and_then(|q| q.as_str()).unwrap_or("?");
        let mut compact = format!("web_search \"{}\":\n", query);
        if let Some(results) = v.get("results").and_then(|r| r.as_array()) {
            for (i, r) in results.iter().enumerate() {
                let title = r.get("title").and_then(|t| t.as_str()).unwrap_or("?");
                let url = r.get("url").and_then(|u| u.as_str()).unwrap_or("");
                let line = format!("{}. {} - {}\n", i + 1, title, url);
                if compact.len() + line.len() > max_chars - 40 {
                    compact.push_str("…[truncated]\n");
                    break;
                }
                compact.push_str(&line);
            }
        }
        compact
    } else {
        // Fallback: raw truncation.
        format!("{}…", &content[..content.len().min(max_chars)])
    }
}
