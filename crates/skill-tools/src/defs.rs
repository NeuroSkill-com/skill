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
                "Query the NeuroSkill EEG/BCI application. \
                 IMPORTANT: always call THIS tool (\"skill\") and pass a command name via the \"command\" argument, \
                 with command-specific parameters inside \"args\". \
                 Examples: {\"command\":\"status\"}, {\"command\":\"search_screenshots\",\"args\":{\"query\":\"browser\"}}, \
                 {\"command\":\"search_labels\",\"args\":{\"query\":\"focus\",\"k\":5}}.\n\n\
                 Commands (pass as \"command\" value, parameters go in \"args\"):\n\
                 STATUS: status | sessions | session_metrics(start_utc,end_utc) | sleep(start_utc,end_utc)\n\
                 ACTIONS: say(text) | notify(title,body?) | label(text,context?) | calibrate | timer\n\
                 SEARCH: search_labels(query,k?,mode?) | interactive_search(query) | search(start_utc,end_utc,k?) | compare(a_start_utc,a_end_utc,b_start_utc,b_end_utc)\n\
                 SCREENSHOTS: search_screenshots(query,k?,mode?) | screenshots_around(timestamp,window_secs?) | screenshots_for_eeg(start_utc?,end_utc?,window_secs?,limit?) | eeg_for_screenshots(query,k?,window_secs?,mode?)\n\
                 CALIBRATION: list_calibrations | get_calibration(id) | create_calibration(name,actions,loop_count) | update_calibration(id,...) | delete_calibration(id) | run_calibration(id?)\n\
                 HOOKS: hooks_status | hooks_get | hooks_set(hooks) | hooks_suggest(keywords) | hooks_log(limit?,offset?)\n\
                 DND: dnd | dnd_set(enabled)\n\
                 ADVANCED: umap(a_start_utc,a_end_utc,b_start_utc,b_end_utc) | umap_poll(job_id) | llm_status | llm_catalog".into()
            ),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command name to execute (e.g. \"status\", \"sessions\", \"say\"). This is NOT a separate tool — pass it here.",
                        "enum": [
                            "status", "sessions", "session_metrics", "say", "notify",
                            "label", "search_labels", "interactive_search", "search",
                            "compare", "sleep", "calibrate", "timer",
                            "run_calibration", "list_calibrations", "get_calibration",
                            "create_calibration", "update_calibration", "delete_calibration",
                            "dnd", "dnd_set",
                            "hooks_status", "hooks_get", "hooks_set", "hooks_suggest", "hooks_log",
                            "umap", "umap_poll",
                            "llm_status", "llm_catalog", "llm_downloads", "llm_hardware_fit",
                            "search_screenshots", "screenshots_around",
                            "screenshots_for_eeg", "eeg_for_screenshots",
                            "search_screenshots_vision", "search_screenshots_by_image_b64"
                        ]
                    },
                    "args": {
                        "type": "object",
                        "description": "Command-specific arguments as key-value pairs. Examples: {\"query\":\"focus\"} for search_labels, {\"query\":\"browser\",\"k\":10} for search_screenshots, {\"text\":\"meditation start\"} for label. Omit for commands with no args (e.g. status, sessions)."
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

/// Known Skill API sub-command names.
///
/// When the LLM mistakenly calls one of these as a top-level tool we can
/// return a targeted hint instead of the generic "unsupported tool" error,
/// dramatically reducing wasted round-trips.
pub fn is_skill_api_command(name: &str) -> bool {
    matches!(name,
        "status" | "sessions" | "session_metrics" | "say" | "notify" |
        "label" | "search_labels" | "interactive_search" | "search" |
        "compare" | "sleep" | "calibrate" | "timer" |
        "run_calibration" | "list_calibrations" | "get_calibration" |
        "create_calibration" | "update_calibration" | "delete_calibration" |
        "dnd" | "dnd_set" |
        "hooks_status" | "hooks_get" | "hooks_set" | "hooks_suggest" | "hooks_log" |
        "umap" | "umap_poll" |
        "llm_status" | "llm_catalog" | "llm_downloads" | "llm_hardware_fit" |
        "search_screenshots" | "screenshots_around" |
        "screenshots_for_eeg" | "eeg_for_screenshots"
    )
}

/// Try to resolve a tool name to a `("skill", "<command>")` redirect.
///
/// Handles:
/// - Bare sub-commands: `"status"` → `Some("status")`
/// - `neuroskill` alias: `"neuroskill"` → `Some("status")` (default command)
/// - Hyphenated form: `"neuroskill-status"` → `Some("status")`
/// - Underscore form:  `"neuroskill_sessions"` → `Some("sessions")`
///
/// Returns `None` if the name is not a skill-related alias.
pub fn resolve_skill_alias(name: &str) -> Option<String> {
    // CLI command name aliases (hyphenated CLI names → underscore WS names).
    match name {
        "search-images"        => return Some("search_screenshots".to_string()),
        "screenshots-around"   => return Some("screenshots_around".to_string()),
        "screenshots-for-eeg"  => return Some("screenshots_for_eeg".to_string()),
        "eeg-for-screenshots"  => return Some("eeg_for_screenshots".to_string()),
        _ => {}
    }

    // Exact sub-command match.
    if is_skill_api_command(name) {
        return Some(name.to_string());
    }

    // "neuroskill" alone → default to "status".
    if name == "neuroskill" {
        return Some("status".to_string());
    }

    // "neuroskill-<cmd>" or "neuroskill_<cmd>" patterns.
    let suffix = name
        .strip_prefix("neuroskill-")
        .or_else(|| name.strip_prefix("neuroskill_"));

    if let Some(cmd) = suffix {
        // Map known skill names to their API command.
        // Some skill folder names differ from command names (e.g.
        // "neuroskill-hooks" → "hooks_status" as a default, but we
        // use the base name if it's a valid command, otherwise try
        // common suffixes).
        let normalised = cmd.replace('-', "_");
        if is_skill_api_command(&normalised) {
            return Some(normalised);
        }
        // Skill folder names map to a primary command:
        match cmd {
            "hooks"        => return Some("hooks_status".to_string()),
            "labels"       => return Some("search_labels".to_string()),
            "search"       => return Some("interactive_search".to_string()),
            "dnd"          => return Some("dnd".to_string()),
            "llm"          => return Some("llm_status".to_string()),
            "protocols"    => return Some("list_calibrations".to_string()),
            "screenshots"  => return Some("search_screenshots".to_string()),
            "streaming"    => return Some("status".to_string()),
            "transport"    => return Some("status".to_string()),
            "recipes"      => return Some("status".to_string()),
            "data-reference" | "data_reference" => return Some("status".to_string()),
            _ => {}
        }
    }

    None
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── builtin_llm_tools ─────────────────────────────────────────────────

    #[test]
    fn builtin_tools_are_nonempty() {
        assert!(!builtin_llm_tools().is_empty());
    }

    #[test]
    fn all_builtin_tools_have_function_type() {
        for tool in builtin_llm_tools() {
            assert_eq!(tool.tool_type, "function", "tool {} has wrong type", tool.function.name);
        }
    }

    #[test]
    fn all_builtin_tools_have_nonempty_name() {
        for tool in builtin_llm_tools() {
            assert!(!tool.function.name.is_empty());
        }
    }

    #[test]
    fn all_builtin_tools_have_description() {
        for tool in builtin_llm_tools() {
            assert!(
                tool.function.description.is_some(),
                "tool {} missing description", tool.function.name
            );
        }
    }

    #[test]
    fn builtin_tools_contain_expected_names() {
        let names: Vec<String> = builtin_llm_tools().iter()
            .map(|t| t.function.name.clone())
            .collect();
        assert!(names.contains(&"date".into()));
        assert!(names.contains(&"web_search".into()));
        assert!(names.contains(&"web_fetch".into()));
        assert!(names.contains(&"location".into()));
    }

    #[test]
    fn builtin_tools_have_no_duplicate_names() {
        let tools = builtin_llm_tools();
        let mut names: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate tool names found");
    }

    // ── is_builtin_tool_enabled ───────────────────────────────────────────

    #[test]
    fn master_switch_disables_all_tools() {
        let cfg = LlmToolConfig { enabled: false, ..Default::default() };
        assert!(!is_builtin_tool_enabled(&cfg, "date"));
        assert!(!is_builtin_tool_enabled(&cfg, "web_search"));
        assert!(!is_builtin_tool_enabled(&cfg, "bash"));
    }

    #[test]
    fn individual_toggles_work() {
        let mut cfg = LlmToolConfig::default();
        assert!(is_builtin_tool_enabled(&cfg, "date"));

        cfg.date = false;
        assert!(!is_builtin_tool_enabled(&cfg, "date"));
    }

    #[test]
    fn skill_api_needs_port() {
        let mut cfg = LlmToolConfig::default();
        cfg.skill_api = true;
        cfg.skill_api_port = 0;
        assert!(!is_builtin_tool_enabled(&cfg, "skill"), "skill needs port > 0");

        cfg.skill_api_port = 8080;
        assert!(is_builtin_tool_enabled(&cfg, "skill"));
    }

    #[test]
    fn unknown_tool_is_disabled() {
        let cfg = LlmToolConfig::default();
        assert!(!is_builtin_tool_enabled(&cfg, "nonexistent_tool"));
    }

    #[test]
    fn search_output_follows_bash() {
        let mut cfg = LlmToolConfig::default();
        assert!(!cfg.bash);
        assert!(!is_builtin_tool_enabled(&cfg, "search_output"));

        cfg.bash = true;
        assert!(is_builtin_tool_enabled(&cfg, "search_output"));
    }

    // ── enabled_builtin_llm_tools ─────────────────────────────────────────

    #[test]
    fn enabled_tools_respects_config() {
        let mut cfg = LlmToolConfig::default();
        cfg.date = false;
        cfg.location = false;
        let tools = enabled_builtin_llm_tools(&cfg);
        let names: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
        assert!(!names.contains(&"date"));
        assert!(!names.contains(&"location"));
        assert!(names.contains(&"web_search"));
    }

    #[test]
    fn enabled_tools_includes_skill_when_port_set() {
        let mut cfg = LlmToolConfig::default();
        cfg.skill_api = true;
        cfg.skill_api_port = 9000;
        let tools = enabled_builtin_llm_tools(&cfg);
        let names: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
        assert!(names.contains(&"skill"));
    }

    // ── is_skill_api_command ──────────────────────────────────────────────

    #[test]
    fn known_skill_commands_are_recognised() {
        assert!(is_skill_api_command("status"));
        assert!(is_skill_api_command("sessions"));
        assert!(is_skill_api_command("label"));
        assert!(is_skill_api_command("search"));
        assert!(is_skill_api_command("dnd"));
        assert!(is_skill_api_command("llm_status"));
    }

    #[test]
    fn unknown_commands_are_not_skill_api() {
        assert!(!is_skill_api_command("random_thing"));
        assert!(!is_skill_api_command(""));
    }

    // ── resolve_skill_alias ───────────────────────────────────────────────

    #[test]
    fn bare_subcommand_resolves() {
        assert_eq!(resolve_skill_alias("status"), Some("status".into()));
    }

    #[test]
    fn neuroskill_alone_resolves_to_status() {
        assert_eq!(resolve_skill_alias("neuroskill"), Some("status".into()));
    }

    #[test]
    fn hyphenated_prefix_resolves() {
        assert_eq!(resolve_skill_alias("neuroskill-sessions"), Some("sessions".into()));
    }

    #[test]
    fn underscore_prefix_resolves() {
        assert_eq!(resolve_skill_alias("neuroskill_sessions"), Some("sessions".into()));
    }

    #[test]
    fn hooks_folder_maps_to_hooks_status() {
        assert_eq!(resolve_skill_alias("neuroskill-hooks"), Some("hooks_status".into()));
    }

    #[test]
    fn screenshot_commands_are_recognised() {
        assert!(is_skill_api_command("search_screenshots"));
        assert!(is_skill_api_command("screenshots_around"));
        assert!(is_skill_api_command("screenshots_for_eeg"));
        assert!(is_skill_api_command("eeg_for_screenshots"));
    }

    #[test]
    fn search_images_alias_resolves() {
        assert_eq!(resolve_skill_alias("search-images"), Some("search_screenshots".into()));
    }

    #[test]
    fn screenshot_cli_aliases_resolve() {
        assert_eq!(resolve_skill_alias("screenshots-around"), Some("screenshots_around".into()));
        assert_eq!(resolve_skill_alias("screenshots-for-eeg"), Some("screenshots_for_eeg".into()));
        assert_eq!(resolve_skill_alias("eeg-for-screenshots"), Some("eeg_for_screenshots".into()));
    }

    #[test]
    fn neuroskill_screenshots_resolves_to_search() {
        assert_eq!(resolve_skill_alias("neuroskill-screenshots"), Some("search_screenshots".into()));
    }

    #[test]
    fn unknown_alias_returns_none() {
        assert_eq!(resolve_skill_alias("totally_unrelated"), None);
    }
}
