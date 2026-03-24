// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Shared types for tool-call parsing and function-calling.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A tool definition (OpenAI-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    /// The tool function's name.
    pub name:        String,
    /// Optional human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional JSON Schema for the function's parameters.
    #[serde(default)]
    pub parameters:  Option<Value>,
}

/// Wrapper around [`ToolFunction`] with a type discriminator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Always `"function"` for function-calling tools.
    #[serde(rename = "type")]
    pub tool_type: String,
    /// The tool's function definition.
    pub function:  ToolFunction,
}

/// The function half of a tool call (name + serialized arguments).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    /// Tool function name.
    pub name:      String,
    /// JSON-encoded arguments string (as per OpenAI spec).
    pub arguments: String,
}

/// A single tool call emitted by the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique call ID (e.g. `"call_0"`).
    pub id:       String,
    /// Always `"function"`.
    #[serde(rename = "type")]
    pub call_type: String,
    /// The function being called.
    pub function:  ToolCallFunction,
}

/// A chat message in the OpenAI-compatible format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ChatMessage {
    /// System prompt.
    System    { content: MessageContent },
    /// User message.
    User      { content: MessageContent },
    /// Assistant response (may include tool calls).
    Assistant {
        #[serde(default)]
        content:    Option<MessageContent>,
        #[serde(default)]
        tool_calls: Vec<ToolCall>,
    },
    /// Tool result.
    Tool {
        tool_call_id: String,
        content:      MessageContent,
    },
}

/// Message content — either plain text or multipart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Plain text content.
    Text(String),
    /// Multipart content (text + images).
    Parts(Vec<ContentPart>),
}

impl MessageContent {
    /// Flatten multipart content to a plain string (best-effort).
    pub fn as_text(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Parts(ps) => ps
                .iter()
                .filter_map(|p| if let ContentPart::Text { text } = p { Some(text.as_str()) } else { None })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

/// A single part in multipart message content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Text content part.
    Text  { text: String },
    /// Image content part.
    Image { image_url: ImageUrl },
}

/// An image URL reference in multipart content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    /// The image URL (may be a data: URI).
    pub url: String,
    /// Optional detail level (`"low"`, `"high"`, `"auto"`).
    #[serde(default)]
    pub detail: Option<String>,
}
