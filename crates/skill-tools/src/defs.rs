// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Built-in tool definitions — JSON Schema specs for each tool the LLM can invoke.

use serde_json::json;
use crate::parse::{Tool, ToolFunction};
use crate::types::LlmToolConfig;

/// Return the full set of built-in tool definitions.
pub fn builtin_llm_tools() -> Vec<Tool> {
    vec![
        Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "date".into(),
                description: Some("Get the current date/time metadata (Unix timestamps, timezone environment, and local/UTC placeholders).".into()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                })),
            },
        },
        Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "location".into(),
                description: Some("Get an approximate public-IP location snapshot (country/region/city/timezone).".into()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                })),
            },
        },
        Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "web_search".into(),
                description: Some("Search the web for a query and return concise results.".into()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                })),
            },
        },
        Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "web_fetch".into(),
                description: Some("Fetch the raw text body of a public HTTP(S) URL.".into()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string" }
                    },
                    "required": ["url"],
                    "additionalProperties": false
                })),
            },
        },
        Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "bash".into(),
                description: Some("Execute a bash command in the working directory. Returns stdout and stderr. Output is truncated to the last 2000 lines or 50 KB (whichever is hit first). Optionally provide a timeout in seconds (default: no timeout).".into()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "Bash command to execute"
                        },
                        "timeout": {
                            "type": "number",
                            "description": "Timeout in seconds (optional, no default timeout)"
                        }
                    },
                    "required": ["command"],
                    "additionalProperties": false
                })),
            },
        },
        Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "read_file".into(),
                description: Some("Read the contents of a text file. Output is truncated to 2000 lines or 50 KB (whichever is hit first). Use offset/limit for large files. When you need the full file, continue with offset until complete.".into()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to read (relative or absolute)"
                        },
                        "offset": {
                            "type": "number",
                            "description": "Line number to start reading from (1-indexed)"
                        },
                        "limit": {
                            "type": "number",
                            "description": "Maximum number of lines to read"
                        }
                    },
                    "required": ["path"],
                    "additionalProperties": false
                })),
            },
        },
        Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "write_file".into(),
                description: Some("Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories.".into()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to write (relative or absolute)"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write to the file"
                        }
                    },
                    "required": ["path", "content"],
                    "additionalProperties": false
                })),
            },
        },
        Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "edit_file".into(),
                description: Some("Edit a file by replacing exact text. The old_text must match exactly (including whitespace). Use this for precise, surgical edits.".into()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to edit (relative or absolute)"
                        },
                        "old_text": {
                            "type": "string",
                            "description": "Exact text to find and replace (must match exactly)"
                        },
                        "new_text": {
                            "type": "string",
                            "description": "New text to replace the old text with"
                        }
                    },
                    "required": ["path", "old_text", "new_text"],
                    "additionalProperties": false
                })),
            },
        },
        Tool {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "search_output".into(),
                description: Some("Search a bash output file using regex, or retrieve lines by range. Use this to explore large command outputs without loading them into context. The output_file path is returned by the bash tool.".into()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the output file (from bash tool's output_file field)"
                        },
                        "pattern": {
                            "type": "string",
                            "description": "Regex pattern to search for (case-insensitive). Omit to use head/tail mode."
                        },
                        "context_lines": {
                            "type": "number",
                            "description": "Number of context lines before and after each match (default: 2)"
                        },
                        "head": {
                            "type": "number",
                            "description": "Return the first N lines of the file"
                        },
                        "tail": {
                            "type": "number",
                            "description": "Return the last N lines of the file"
                        },
                        "line_start": {
                            "type": "number",
                            "description": "Return lines starting from this line number (1-indexed)"
                        },
                        "line_end": {
                            "type": "number",
                            "description": "Return lines up to this line number (inclusive)"
                        },
                        "max_matches": {
                            "type": "number",
                            "description": "Maximum number of matches to return (default: 50)"
                        }
                    },
                    "required": ["path"],
                    "additionalProperties": false
                })),
            },
        },
    ]
}

/// Check whether a builtin tool is enabled in the current config.
pub fn is_builtin_tool_enabled(config: &LlmToolConfig, name: &str) -> bool {
    match name {
        "date"          => config.date,
        "location"      => config.location,
        "web_search"    => config.web_search,
        "web_fetch"     => config.web_fetch,
        "bash"          => config.bash,
        "read_file"     => config.read_file,
        "write_file"    => config.write_file,
        "edit_file"     => config.edit_file,
        // search_output is automatically enabled when bash is enabled
        "search_output" => config.bash,
        _               => false,
    }
}

/// Return only the enabled tool definitions.
pub fn enabled_builtin_llm_tools(config: &LlmToolConfig) -> Vec<Tool> {
    builtin_llm_tools()
        .into_iter()
        .filter(|tool| is_builtin_tool_enabled(config, &tool.function.name))
        .collect()
}

/// Filter a provided set of tool definitions to only those enabled.
pub fn filter_allowed_tool_defs(tool_defs: Vec<Tool>, config: &LlmToolConfig) -> Vec<Tool> {
    tool_defs
        .into_iter()
        .filter(|tool| is_builtin_tool_enabled(config, &tool.function.name))
        .collect()
}
