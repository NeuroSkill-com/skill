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

                calls.push(ToolCall {
                    id:        format!("call_{}", calls.len()),
                    call_type: "function".into(),
                    function:  ToolCallFunction { name, arguments: args },
                });
            }
            remaining = &after_start[e + END.len()..];
        } else {
            break;
        }
    }

    calls
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

    out
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
    fn strip_blocks() {
        let msg = r#"Here you go. [TOOL_CALL]{"name":"foo","arguments":{}}[/TOOL_CALL] Done."#;
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("[TOOL_CALL]"));
        assert!(stripped.contains("Done."));
    }
}
