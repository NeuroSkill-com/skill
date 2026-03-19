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
                description: Some("Search the web for a query. Without render=true this returns ONLY links and snippets (no page content). For factual/current-data queries (weather, prices, scores, news) you SHOULD set render=true so the top pages are fetched and their text is included — otherwise you will only get URLs and must follow up with web_fetch. Do NOT retry if results already contain page content — summarize what you have.".into()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "render": {
                            "type": "boolean",
                            "description": "If true, visit top result URLs in a headless browser and return their rendered text content (slower but handles JS-rendered pages). Default: false."
                        },
                        "render_count": {
                            "type": "number",
                            "description": "Number of top results to render when render=true (default: 3, max: 5)."
                        }
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
                description: Some("Fetch content from a public HTTP(S) URL. By default returns the raw text body. When render=true, uses a headless browser to render the page (executes JavaScript) and returns the rendered text content.".into()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string" },
                        "render": {
                            "type": "boolean",
                            "description": "If true, render the page in a headless browser (handles JS-rendered SPAs, dynamic content). Default: false."
                        },
                        "wait_ms": {
                            "type": "number",
                            "description": "Milliseconds to wait after page load before capturing content (only when render=true). Default: 2000."
                        },
                        "selector": {
                            "type": "string",
                            "description": "CSS selector to wait for before capturing content (only when render=true). Overrides wait_ms."
                        },
                        "eval_js": {
                            "type": "string",
                            "description": "JavaScript expression to evaluate after page load and return its result (only when render=true)."
                        }
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

/// Return the Skill API tool definition.
///
/// This is a single tool that gives the LLM access to the full Skill
/// WebSocket API — device status, EEG sessions, labels, search, hooks,
/// DND, calibrations, TTS, and more.
pub fn skill_api_tool() -> Tool {
    Tool {
        tool_type: "function".into(),
        function: ToolFunction {
            name: "skill".into(),
            description: Some(
                "Query the NeuroSkill EEG/BCI application via its API. \
                 Send a JSON command and receive the full response.\n\n\
                 Available commands:\n\
                 - status: Full device/session/embeddings/scores snapshot. No args.\n\
                 - sessions: List all recording sessions. No args.\n\
                 - session_metrics: Metrics for a session. Args: start_utc (number), end_utc (number).\n\
                 - say: Speak text via TTS. Args: text (string, required), voice (string, optional).\n\
                 - notify: Show OS notification. Args: title (string, required), body (string, optional).\n\
                 - label: Create timestamped annotation. Args: text (string, required), context (string, optional), label_start_utc (number, optional).\n\
                 - search_labels: Semantic label search. Args: query (string, required), k (number, default 10), mode (\"text\"|\"context\"|\"both\", default \"text\"), ef (number, optional).\n\
                 - interactive_search: Cross-modal graph search. Args: query (string, required), k_text (number, default 5), k_eeg (number, default 5), k_labels (number, default 3), reach_minutes (number, default 10).\n\
                 - search: ANN EEG-similarity search. Args: start_utc (number), end_utc (number), k (number, default 5).\n\
                 - compare: A/B session comparison. Args: a_start_utc, a_end_utc, b_start_utc, b_end_utc (numbers).\n\
                 - sleep: Sleep staging. Args: start_utc (number), end_utc (number).\n\
                 - calibrate: Open calibration window. No args (or id for specific profile).\n\
                 - timer: Open focus-timer. No args.\n\
                 - run_calibration: Start calibration. Args: id (string, optional profile UUID).\n\
                 - list_calibrations: List calibration profiles. No args.\n\
                 - get_calibration: Get one profile. Args: id (number).\n\
                 - create_calibration: Create profile. Args: name (string), actions (array of {label, duration_secs}), loop_count (number), break_duration_secs (number), auto_start (bool).\n\
                 - update_calibration: Update profile. Args: id (string), plus optional name/actions/loop_count/break_duration_secs/auto_start.\n\
                 - delete_calibration: Delete profile. Args: id (string).\n\
                 - dnd: DND automation status. No args.\n\
                 - dnd_set: Force DND on/off. Args: enabled (bool).\n\
                 - hooks_status: List hooks with last-trigger metadata. No args.\n\
                 - hooks_get: List raw hook rules. No args.\n\
                 - hooks_set: Replace all hooks. Args: hooks (array of hook rule objects).\n\
                 - hooks_suggest: Suggest threshold. Args: keywords (array of strings).\n\
                 - hooks_log: Hook trigger history. Args: limit (number, default 20), offset (number, default 0).\n\
                 - umap: Enqueue 3D UMAP projection. Args: a_start_utc, a_end_utc, b_start_utc, b_end_utc (numbers).\n\
                 - umap_poll: Poll UMAP job. Args: job_id (number).\n\
                 - llm_status: LLM server status. No args.\n\
                 - llm_catalog: Model catalog. No args.\n\
                 - llm_downloads: List downloads. No args.\n\
                 - llm_hardware_fit: Check model fit. No args.".into()
            ),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The API command name (e.g. \"status\", \"sessions\", \"label\", \"search\", etc.)"
                    },
                    "args": {
                        "type": "object",
                        "description": "Command-specific arguments as key-value pairs (omit or {} for commands with no args)"
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            })),
        },
    }
}

/// Check whether a tool name is a known built-in tool (regardless of enabled state).
pub fn is_known_builtin_tool(name: &str) -> bool {
    matches!(name,
        "date" | "location" | "web_search" | "web_fetch" |
        "bash" | "read_file" | "write_file" | "edit_file" |
        "search_output" | "skill"
    )
}

/// Check whether a builtin tool is enabled in the current config.
/// Returns `false` for every tool when the master `enabled` flag is off.
pub fn is_builtin_tool_enabled(config: &LlmToolConfig, name: &str) -> bool {
    if !config.enabled {
        return false;
    }
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
        // skill API tool — enabled when toggle is on AND port is known
        "skill"         => config.skill_api && config.skill_api_port > 0,
        _               => false,
    }
}

/// Return only the enabled tool definitions.
pub fn enabled_builtin_llm_tools(config: &LlmToolConfig) -> Vec<Tool> {
    let mut tools: Vec<Tool> = builtin_llm_tools()
        .into_iter()
        .filter(|tool| is_builtin_tool_enabled(config, &tool.function.name))
        .collect();
    // Append the Skill API tool if enabled and port is known.
    if is_builtin_tool_enabled(config, "skill") {
        tools.push(skill_api_tool());
    }
    tools
}

/// Filter a provided set of tool definitions to only those enabled.
pub fn filter_allowed_tool_defs(tool_defs: Vec<Tool>, config: &LlmToolConfig) -> Vec<Tool> {
    tool_defs
        .into_iter()
        .filter(|tool| is_builtin_tool_enabled(config, &tool.function.name))
        .collect()
}
