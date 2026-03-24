// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Structured error types for skill-tools.

/// Errors from tool-call argument validation.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    /// A required argument is missing.
    #[error("missing required argument: {name}")]
    MissingArgument { name: String },

    /// An argument has the wrong type.
    #[error("argument {name}: expected {expected}, got {actual}")]
    TypeMismatch {
        name: String,
        expected: String,
        actual: String,
    },

    /// JSON Schema validation failed.
    #[error("schema validation failed: {message}")]
    SchemaViolation { message: String },

    /// Tool not found in the registry.
    #[error("unknown tool: {name}")]
    UnknownTool { name: String },
}

/// Errors from tool-call parsing / extraction.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// No tool calls found in the model output.
    #[error("no tool calls found in output")]
    NoToolCalls,

    /// Malformed tool-call JSON.
    #[error("malformed tool-call JSON: {message}")]
    MalformedJson { message: String },

    /// Malformed XML tool-call format.
    #[error("malformed XML tool-call: {message}")]
    MalformedXml { message: String },
}

/// Errors from tool execution.
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    /// Tool was blocked by safety checks.
    #[error("tool {name} blocked: {reason}")]
    Blocked { name: String, reason: String },

    /// Tool execution timed out.
    #[error("tool {name} timed out after {timeout_secs}s")]
    Timeout { name: String, timeout_secs: u64 },

    /// Tool execution failed.
    #[error("tool {name} failed: {message}")]
    Failed { name: String, message: String },

    /// Generic error.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
