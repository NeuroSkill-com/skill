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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::types::CompressionLevel;

    fn msg(role: &str, content: &str) -> Value {
        json!({"role": role, "content": content})
    }

    fn tool_msg(content: &str) -> Value {
        json!({"role": "tool", "content": content})
    }

    #[test]
    fn estimate_tokens_is_nonzero_for_empty() {
        assert!(estimate_tokens("") >= 1);
    }

    #[test]
    fn estimate_tokens_roughly_4_chars_per_token() {
        // 400 chars → ~100 tokens (+ 1)
        let s = "a".repeat(400);
        let t = estimate_tokens(&s);
        assert!(t >= 90 && t <= 110, "expected ~101, got {t}");
    }

    #[test]
    fn estimate_messages_tokens_counts_overhead() {
        let messages = vec![msg("user", "hello")];
        let t = estimate_messages_tokens(&messages);
        // "hello" = 5 chars → 2 tokens + 10 overhead = 12
        assert!(t >= 10, "expected at least 10, got {t}");
    }

    #[test]
    fn trim_is_noop_for_zero_ctx() {
        let mut messages = vec![msg("system", "sys"), msg("user", "hi")];
        trim_messages_to_fit(&mut messages, 0, &ToolContextCompression::default());
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn trim_preserves_system_and_last_user() {
        let mut messages = vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "old question"),
            msg("assistant", "old answer"),
            msg("user", "current question"),
        ];
        // Very small context → must drop middle messages but keep system + last
        trim_messages_to_fit(&mut messages, 50, &ToolContextCompression::default());
        assert!(messages.len() >= 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages.last().unwrap()["role"], "user");
    }

    #[test]
    fn trim_truncates_long_tool_results() {
        let long_content = "x".repeat(5000);
        let mut messages = vec![
            msg("system", "sys"),
            tool_msg(&long_content),
            msg("user", "hi"),
        ];
        let compression = ToolContextCompression {
            level: CompressionLevel::Normal,
            max_search_results: 0,
            max_result_chars: 500,
        };
        trim_messages_to_fit(&mut messages, 100_000, &compression);
        let tool_content = messages[1]["content"].as_str().unwrap();
        assert!(tool_content.len() < 5000, "expected truncation");
        assert!(tool_content.contains("[truncated"));
    }

    #[test]
    fn trim_compression_off_does_not_truncate() {
        let long_content = "x".repeat(3000);
        let mut messages = vec![
            msg("system", "sys"),
            tool_msg(&long_content),
            msg("user", "hi"),
        ];
        let compression = ToolContextCompression {
            level: CompressionLevel::Off,
            max_search_results: 0,
            max_result_chars: 0, // Off level defaults to 16000
        };
        trim_messages_to_fit(&mut messages, 100_000, &compression);
        let tool_content = messages[1]["content"].as_str().unwrap();
        assert_eq!(tool_content.len(), 3000, "Off should not truncate");
    }

    #[test]
    fn effective_defaults_match_levels() {
        let off = ToolContextCompression { level: CompressionLevel::Off, ..Default::default() };
        let norm = ToolContextCompression::default(); // Normal
        let agg = ToolContextCompression { level: CompressionLevel::Aggressive, ..Default::default() };

        assert!(off.effective_max_search_results() > norm.effective_max_search_results());
        assert!(norm.effective_max_search_results() > agg.effective_max_search_results());

        assert!(off.effective_max_result_chars() > norm.effective_max_result_chars());
        assert!(norm.effective_max_result_chars() > agg.effective_max_result_chars());
    }

    #[test]
    fn custom_overrides_take_precedence() {
        let c = ToolContextCompression {
            level: CompressionLevel::Normal,
            max_search_results: 42,
            max_result_chars: 999,
        };
        assert_eq!(c.effective_max_search_results(), 42);
        assert_eq!(c.effective_max_result_chars(), 999);
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
