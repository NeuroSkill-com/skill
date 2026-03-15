// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Context-aware history trimming for LLM tool conversations.

use serde_json::Value;

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
/// 3. First, truncate long "tool" role messages (tool results) to a summary.
/// 4. Then drop oldest non-system messages in pairs until the estimated
///    token count fits within 75% of `n_ctx` (leaving room for response).
pub fn trim_messages_to_fit(messages: &mut Vec<Value>, n_ctx: usize) {
    if n_ctx == 0 { return; }
    let budget = n_ctx * 3 / 4; // 75% of context for prompt

    // Phase 1: Truncate long tool results in history to save context space.
    const MAX_TOOL_RESULT_CHARS: usize = 2000;
    for msg in messages.iter_mut() {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if role == "tool" {
            if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                if content.len() > MAX_TOOL_RESULT_CHARS {
                    let truncated = format!(
                        "{}…\n[truncated {} chars]",
                        &content[..MAX_TOOL_RESULT_CHARS],
                        content.len() - MAX_TOOL_RESULT_CHARS
                    );
                    msg["content"] = Value::String(truncated);
                }
            }
        }
    }

    // Phase 2: Drop oldest non-system, non-last-user messages if still too long.
    while estimate_messages_tokens(messages) > budget && messages.len() > 2 {
        let start = if messages.first()
            .and_then(|m| m.get("role"))
            .and_then(|r| r.as_str()) == Some("system")
        { 1 } else { 0 };

        if start >= messages.len() - 1 { break; }

        messages.remove(start);
    }
}
