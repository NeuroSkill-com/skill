// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
#![allow(dead_code)]
//! Tool-call / function-calling helpers for OpenAI-compatible chat completions.
//!
//! This module is split into focused sub-modules:
//! - [`types`]    — shared data types (`Tool`, `ToolCall`, `ChatMessage`, …)
//! - [`coerce`]   — schema-driven type coercion for LLM arguments
//! - [`validate`] — JSON Schema validation of tool-call arguments
//! - [`extract`]  — tool-call extraction from raw assistant output
//! - [`strip`]    — stripping tool-call blocks from message content
//! - [`inject`]   — injecting tool definitions into system prompts
//! - [`json_scan`]— balanced JSON range finders

pub mod types;
pub mod coerce;
pub mod validate;
pub mod extract;
pub mod strip;
pub mod inject;
pub(crate) mod json_scan;

// ── Re-exports (preserve backward-compatible public API) ─────────────────────

pub use types::{
    Tool, ToolFunction, ToolCall, ToolCallFunction,
    ChatMessage, MessageContent, ContentPart, ImageUrl,
};

pub use coerce::coerce_tool_call_arguments;
pub use validate::validate_tool_arguments;
pub use extract::{extract_tool_calls, detect_garbled_tool_call, build_self_healing_message};
pub use strip::{strip_tool_call_blocks, strip_tool_call_blocks_preserve};
pub use inject::inject_tools_into_system_prompt;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

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
    fn extract_openai_style_single_json_object() {
        let msg = r#"I'll use the date tool now.
json
{
    "name": "date",
    "parameters": {}
}"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "date");
        assert_eq!(calls[0].function.arguments, "{}");
    }

    #[test]
    fn extract_tool_key_alias_single_json_object() {
        let msg = r#"The user is asking about the current time.
I'll fetch it now.
json
Copy
{
  "tool": "date",
  "parameters": {}
}
I'll fetch that information for you right away."#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "date");
        assert_eq!(calls[0].function.arguments, "{}");
    }

    #[test]
    fn extract_openai_tool_calls_envelope() {
        let msg = r#"```json
{
    "tool_calls": [
        {
            "type": "function",
            "function": {
                "name": "date",
                "arguments": "{}"
            }
        }
    ]
}
```"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "date");
        assert_eq!(calls[0].function.arguments, "{}");
    }

    #[test]
    fn strip_blocks() {
        let msg = r#"Here you go. [TOOL_CALL]{"name":"foo","arguments":{}}[/TOOL_CALL] Done."#;
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("[TOOL_CALL]"));
        assert!(stripped.contains("Done."));
    }

    #[test]
    fn strip_inline_json_tool_payload() {
        let msg = r#"I'll use a tool.
{"name":"date","parameters":{}}
Then answer naturally."#;
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("\"name\":\"date\""));
        assert!(stripped.contains("Then answer naturally."));
    }

    #[test]
    fn keep_non_tool_json_blocks() {
        let msg = r#"```json
{"status":"ok","count":3}
```"#;
        let stripped = strip_tool_call_blocks(msg);
        assert!(stripped.contains("\"status\":\"ok\""));
    }

    #[test]
    fn extract_dict_style_multi_tool() {
        let msg = "I'll get that information for you.\n```json\n{\n  \"date\": {},\n  \"location\": {}\n}\n```\nLet me fetch that for you.";
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 2, "expected 2 calls, got: {:?}", calls.iter().map(|c| &c.function.name).collect::<Vec<_>>());
        let names: Vec<&str> = calls.iter().map(|c| c.function.name.as_str()).collect();
        assert!(names.contains(&"date"), "missing date");
        assert!(names.contains(&"location"), "missing location");
    }

    #[test]
    fn strip_dict_style_multi_tool_fence() {
        let msg = "I'll get that.\n```json\n{\n  \"date\": {},\n  \"location\": {}\n}\n```\nDone.";
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("\"date\""), "date key should be stripped");
        assert!(!stripped.contains("\"location\""), "location key should be stripped");
        assert!(stripped.contains("Done."), "prose should survive");
    }

    #[test]
    fn extract_array_style_tool_calls() {
        let msg = r#"I'll get that info.
```json
[
  {"name": "date", "parameters": {}},
  {"name": "location", "parameters": {}}
]
```"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 2, "expected 2, got {:?}", calls.iter().map(|c| &c.function.name).collect::<Vec<_>>());
        let names: Vec<&str> = calls.iter().map(|c| c.function.name.as_str()).collect();
        assert!(names.contains(&"date"));
        assert!(names.contains(&"location"));
    }

    #[test]
    fn strip_array_style_tool_call_fence() {
        let msg = r#"I'll get that info.
```json
[
  {"name": "date", "parameters": {}},
  {"name": "location", "parameters": {}}
]
```
Done."#;
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("\"name\""), "tool call JSON should be stripped");
        assert!(stripped.contains("Done."));
    }

    #[test]
    fn strip_incomplete_array_tool_call() {
        let msg = "I'll get that.\n[\n  {\n    \"name\": \"date\",\n    \"parameterarameter";
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("parameterarameter"), "incomplete array should be stripped: got '{}'", stripped);
    }

    #[test]
    fn validate_tool_args_valid() {
        let tool = Tool {
            tool_type: "function".into(),
            function: types::ToolFunction {
                name: "web_search".into(),
                description: Some("Search the web".into()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": { "query": { "type": "string" } },
                    "required": ["query"],
                    "additionalProperties": false
                })),
            },
        };
        let args = serde_json::json!({"query": "test"});
        let result = validate_tool_arguments(&tool, &args);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_tool_args_missing_required() {
        let tool = Tool {
            tool_type: "function".into(),
            function: types::ToolFunction {
                name: "web_search".into(),
                description: Some("Search the web".into()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": { "query": { "type": "string" } },
                    "required": ["query"],
                    "additionalProperties": false
                })),
            },
        };
        let args = serde_json::json!({});
        let result = validate_tool_arguments(&tool, &args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Validation failed"));
    }

    #[test]
    fn validate_tool_args_no_schema() {
        let tool = Tool {
            tool_type: "function".into(),
            function: types::ToolFunction {
                name: "date".into(),
                description: Some("Get date".into()),
                parameters: None,
            },
        };
        let args = serde_json::json!({"anything": true});
        assert!(validate_tool_arguments(&tool, &args).is_ok());
    }

    #[test]
    fn validate_tool_args_wrong_type_coerced() {
        let tool = make_web_search_tool();
        let args = serde_json::json!({"query": 123});
        let result = validate_tool_arguments(&tool, &args);
        assert!(result.is_ok(), "number should be coerced to string");
        assert_eq!(result.expect("valid")["query"], Value::String("123".into()));
    }

    #[test]
    fn validate_tool_args_truly_wrong_type() {
        let tool = Tool {
            tool_type: "function".into(),
            function: types::ToolFunction {
                name: "web_search".into(),
                description: None,
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": { "query": { "type": "string" } },
                    "required": ["query"],
                    "additionalProperties": false
                })),
            },
        };
        let args = serde_json::json!({"query": [1, 2, 3]});
        assert!(validate_tool_arguments(&tool, &args).is_err());
    }

    #[test]
    fn strip_incomplete_fenced_tool_payload_before_think() {
        let msg = "```json\n{\n  \"name\": \"date\",\n  \"parameter<think>thinking</think>\nFinal answer.";
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("```json"));
        assert!(!stripped.contains("\"name\": \"date\""));
        assert!(stripped.contains("<think>thinking</think>"));
        assert!(stripped.contains("Final answer."));
    }

    #[test]
    fn extract_bash_code_fence_fallback() {
        let msg = "To list files on your desktop:\n```bash\nls ~/Desktop/\n```";
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1, "should extract one bash tool call");
        assert_eq!(calls[0].function.name, "bash");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["command"].as_str().expect("has command"), "ls ~/Desktop/");
    }

    #[test]
    fn extract_sh_code_fence_fallback() {
        let msg = "```sh\ndf -h\n```";
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "bash");
    }

    #[test]
    fn bash_fence_fallback_skipped_when_tool_call_present() {
        let msg = r#"[TOOL_CALL]{"name":"bash","arguments":{"command":"ls"}}[/TOOL_CALL]
Also:
```bash
echo hello
```"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.arguments, r#"{"command":"ls"}"#);
    }

    #[test]
    fn bash_empty_args_filled_from_code_fence() {
        let msg = r#"I'll list your desktop files.
[TOOL_CALL]{"name":"bash","arguments":{}}[/TOOL_CALL]
```bash
ls ~/Desktop/
```"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "bash");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["command"].as_str().expect("has command"), "ls ~/Desktop/");
    }

    #[test]
    fn bash_empty_args_filled_from_code_fence_parameters_key() {
        let msg = r#"[TOOL_CALL]{"name":"bash","parameters":{}}[/TOOL_CALL]
```bash
df -h
```"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["command"].as_str().expect("has command"), "df -h");
    }

    #[test]
    fn tool_key_alias_in_tool_call_block() {
        let msg = r#"[TOOL_CALL]{"tool":"date","arguments":{}}[/TOOL_CALL]"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "date");
    }

    // ── Partial tag prefix stripping (streaming) ──────────────────────────

    #[test]
    fn strip_partial_tool_call_tag_prefix() {
        assert_eq!(strip_tool_call_blocks_preserve("["), "");
        assert_eq!(strip_tool_call_blocks_preserve("[T"), "");
        assert_eq!(strip_tool_call_blocks_preserve("[TO"), "");
        assert_eq!(strip_tool_call_blocks_preserve("[TOOL_CA"), "");
        assert_eq!(strip_tool_call_blocks_preserve("[TOOL_CALL"), "");
        assert_eq!(strip_tool_call_blocks_preserve("[TOOL_CALL]"), "");
        assert_eq!(strip_tool_call_blocks_preserve("Hello [TOOL_CA"), "Hello ");
        assert_eq!(strip_tool_call_blocks_preserve("Hello [TOOL_CALL"), "Hello ");
        assert_eq!(
            strip_tool_call_blocks_preserve(r#"[TOOL_CALL]{"name":"date","arguments":{}}[/TOOL_CALL]"#),
            ""
        );
        assert_eq!(
            strip_tool_call_blocks_preserve(r#"Hi [TOOL_CALL]{"name":"date","arguments":{}}[/TOOL_CALL] done"#),
            "Hi  done"
        );
    }

    #[test]
    fn strip_partial_close_tag_prefix() {
        assert_eq!(
            strip_tool_call_blocks_preserve(r#"[TOOL_CALL]{"name":"date","arguments":{}}[/TOOL_"#),
            ""
        );
    }

    #[test]
    fn legitimate_brackets_survive_streaming() {
        let full = "Here are [options] for you.";
        let mut raw = String::new();
        let mut emitted_len = 0usize;
        let mut all_visible = String::new();
        for ch in full.chars() {
            raw.push(ch);
            let visible = strip_tool_call_blocks_preserve(&raw);
            if visible.len() > emitted_len {
                all_visible.push_str(&visible[emitted_len..]);
                emitted_len = visible.len();
            }
        }
        assert_eq!(all_visible, full);
    }

    #[test]
    fn redirect_skill_subcmd_as_tool() {
        let msg = r#"[TOOL_CALL]{"name":"status","arguments":{}}[/TOOL_CALL]"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "skill");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["command"].as_str().expect("has command"), "status");
    }

    #[test]
    fn redirect_skill_subcmd_with_args() {
        let msg = r#"[TOOL_CALL]{"name":"say","arguments":{"text":"hello"}}[/TOOL_CALL]"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "skill");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["command"].as_str().expect("has command"), "say");
        assert_eq!(args["args"]["text"].as_str().expect("has text"), "hello");
    }

    #[test]
    fn no_redirect_for_real_tools() {
        let msg = r#"[TOOL_CALL]{"name":"date","arguments":{}}[/TOOL_CALL]"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "date");
    }

    #[test]
    fn redirect_neuroskill_alias() {
        let msg = r#"[TOOL_CALL]{"name":"neuroskill","arguments":{}}[/TOOL_CALL]"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "skill");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["command"].as_str().expect("has command"), "status");
    }

    #[test]
    fn redirect_neuroskill_hyphenated() {
        let msg = r#"[TOOL_CALL]{"name":"neuroskill-status","arguments":{}}[/TOOL_CALL]"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "skill");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["command"].as_str().expect("has command"), "status");
    }

    #[test]
    fn redirect_neuroskill_sessions() {
        let msg = r#"[TOOL_CALL]{"name":"neuroskill-sessions","arguments":{}}[/TOOL_CALL]"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "skill");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["command"].as_str().expect("has command"), "sessions");
    }

    #[test]
    fn redirect_neuroskill_hooks() {
        let msg = r#"[TOOL_CALL]{"name":"neuroskill-hooks","arguments":{}}[/TOOL_CALL]"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "skill");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["command"].as_str().expect("has command"), "hooks_status");
    }

    #[test]
    fn redirect_search_images_with_args() {
        let msg = r#"[TOOL_CALL]{"name":"search-images","arguments":{"query":"browser"}}[/TOOL_CALL]"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "skill");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["command"].as_str().expect("has command"), "search_screenshots");
        assert_eq!(args["args"]["query"].as_str().expect("has query"), "browser");
    }

    #[test]
    fn redirect_multiple_subcmds_in_one_turn() {
        let msg = r#"[TOOL_CALL]{"name":"status","arguments":{}}[/TOOL_CALL]
[TOOL_CALL]{"name":"sessions","arguments":{}}[/TOOL_CALL]"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.name, "skill");
        assert_eq!(calls[1].function.name, "skill");
        let a0: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        let a1: Value = serde_json::from_str(&calls[1].function.arguments).expect("valid json");
        assert_eq!(a0["command"].as_str().expect("has command"), "status");
        assert_eq!(a1["command"].as_str().expect("has command"), "sessions");
    }

    #[test]
    fn no_extract_from_tool_result_quoted_in_response() {
        let msg = r#"Based on your data:
{"ok":true,"tool":"skill","command":"status","device":{"connected":true,"battery":89}}
Your device is connected with 89% battery."#;
        let calls = extract_tool_calls(msg);
        assert!(calls.is_empty(), "tool result JSON should not be extracted as a tool call");
    }

    #[test]
    fn no_strip_tool_result_from_response() {
        let msg = r#"The result was {"ok":true,"tool":"skill","command":"status"}. Done."#;
        let stripped = strip_tool_call_blocks(msg);
        assert!(stripped.contains("ok"), "tool result should survive stripping: {}", stripped);
    }

    #[test]
    fn sanitizer_streaming_simulation() {
        let full = r#"[TOOL_CALL]{"name":"date","arguments":{}}[/TOOL_CALL]"#;
        let mut raw = String::new();
        let mut emitted_len = 0usize;
        let mut all_visible = String::new();
        for ch in full.chars() {
            raw.push(ch);
            let visible = strip_tool_call_blocks_preserve(&raw);
            if visible.len() > emitted_len {
                all_visible.push_str(&visible[emitted_len..]);
                emitted_len = visible.len();
            }
        }
        assert!(all_visible.trim().is_empty(), "tool call should produce no visible output");
    }

    // ── Llama XML format tests ────────────────────────────────────────────

    #[test]
    fn extract_llama_xml_single_tool() {
        let msg = r#"<function=date><parameter=dummy>ignored</parameter></function>"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "date");
    }

    #[test]
    fn extract_llama_xml_with_parameters() {
        let msg = r#"I'll search for that.
<function=web_search><parameter=query>rust programming</parameter><parameter=render>true</parameter></function>"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "web_search");
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["query"], Value::String("rust programming".into()));
        assert_eq!(args["render"], Value::Bool(true));
    }

    #[test]
    fn extract_llama_xml_json_body() {
        let msg = r#"<function=bash>{"command":"ls -la"}</function>"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["command"].as_str().expect("has command"), "ls -la");
    }

    #[test]
    fn extract_llama_xml_multiple_calls() {
        let msg = r#"<function=date></function>
<function=location></function>"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 2);
    }

    #[test]
    fn extract_llama_xml_no_params() {
        let msg = r#"<function=date></function>"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.arguments, "{}");
    }

    #[test]
    fn strip_llama_xml_blocks() {
        let msg = r#"Let me check. <function=date></function> Done."#;
        let stripped = strip_tool_call_blocks(msg);
        assert!(!stripped.contains("<function="));
        assert!(stripped.contains("Done."));
    }

    #[test]
    fn strip_llama_xml_incomplete_streaming() {
        let msg = r#"Checking... <function=date"#;
        let stripped = strip_tool_call_blocks_preserve(msg);
        assert!(!stripped.contains("<function="));
    }

    #[test]
    fn extract_llama_xml_numeric_param() {
        let msg = r#"<function=read_file><parameter=path>/etc/hosts</parameter><parameter=offset>10</parameter></function>"#;
        let calls = extract_tool_calls(msg);
        assert_eq!(calls.len(), 1);
        let args: Value = serde_json::from_str(&calls[0].function.arguments).expect("valid json");
        assert_eq!(args["path"], Value::String("/etc/hosts".into()));
        assert_eq!(args["offset"], serde_json::json!(10));
    }

    // ── Self-healing tests ────────────────────────────────────────────────

    #[test]
    fn detect_garbled_tool_call_tag() {
        let msg = r#"[TOOL_CALL]{"name":"date", "arguments": {[/TOOL_CALL]"#;
        assert!(detect_garbled_tool_call(msg).is_some());
    }

    #[test]
    fn detect_garbled_incomplete_json() {
        let msg = r#"I'll use a tool: {"name":"bash","arguments":{"command":"ls"#;
        assert!(detect_garbled_tool_call(msg).is_some());
    }

    #[test]
    fn detect_garbled_xml_format() {
        let msg = r#"<function=date><parameter=foo>bar"#;
        assert!(detect_garbled_tool_call(msg).is_some());
    }

    #[test]
    fn no_garble_on_clean_output() {
        assert!(detect_garbled_tool_call("The weather today is sunny.").is_none());
    }

    #[test]
    fn no_garble_when_parsed_ok() {
        let msg = r#"[TOOL_CALL]{"name":"date","arguments":{}}[/TOOL_CALL]"#;
        assert!(detect_garbled_tool_call(msg).is_none());
    }

    #[test]
    fn self_healing_message_format() {
        let msg = build_self_healing_message("[TOOL_CALL]{bad json");
        assert!(msg.contains("[TOOL_CALL]{bad json"));
        assert!(msg.contains("re-emit"));
    }

    // ── Argument coercion tests ───────────────────────────────────────────

    fn make_web_search_tool() -> Tool {
        Tool {
            tool_type: "function".into(),
            function: types::ToolFunction {
                name: "web_search".into(),
                description: Some("Search the web".into()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query":        { "type": "string" },
                        "render":       { "type": "boolean" },
                        "render_count": { "type": "number" }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                })),
            },
        }
    }

    fn make_read_file_tool() -> Tool {
        Tool {
            tool_type: "function".into(),
            function: types::ToolFunction {
                name: "read_file".into(),
                description: Some("Read a file".into()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path":   { "type": "string" },
                        "offset": { "type": "integer" },
                        "limit":  { "type": "integer" }
                    },
                    "required": ["path"],
                    "additionalProperties": false
                })),
            },
        }
    }

    #[test]
    fn coerce_string_true_to_bool() {
        let tool = make_web_search_tool();
        let args = serde_json::json!({"query": "weather", "render": "true"});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["render"], Value::Bool(true));
    }

    #[test]
    fn coerce_string_false_to_bool() {
        let tool = make_web_search_tool();
        let args = serde_json::json!({"query": "test", "render": "false"});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["render"], Value::Bool(false));
    }

    #[test]
    fn coerce_string_number_to_number() {
        let tool = make_web_search_tool();
        let args = serde_json::json!({"query": "test", "render_count": "3"});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["render_count"], serde_json::json!(3.0));
    }

    #[test]
    fn coerce_string_integer_to_integer() {
        let tool = make_read_file_tool();
        let args = serde_json::json!({"path": "foo.txt", "offset": "10", "limit": "50"});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["offset"], serde_json::json!(10));
        assert_eq!(result["limit"], serde_json::json!(50));
    }

    #[test]
    fn coerce_number_to_string() {
        let tool = make_read_file_tool();
        let args = serde_json::json!({"path": 42});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["path"], Value::String("42".into()));
    }

    #[test]
    fn coerce_bool_number_1_to_true() {
        let tool = make_web_search_tool();
        let args = serde_json::json!({"query": "test", "render": 1});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["render"], Value::Bool(true));
    }

    #[test]
    fn coerce_bool_number_0_to_false() {
        let tool = make_web_search_tool();
        let args = serde_json::json!({"query": "test", "render": 0});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["render"], Value::Bool(false));
    }

    #[test]
    fn coerce_string_yes_to_bool() {
        let tool = make_web_search_tool();
        let args = serde_json::json!({"query": "test", "render": "yes"});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["render"], Value::Bool(true));
    }

    #[test]
    fn coerce_no_schema_passthrough() {
        let tool = Tool {
            tool_type: "function".into(),
            function: types::ToolFunction {
                name: "date".into(),
                description: Some("Get date".into()),
                parameters: None,
            },
        };
        let args = serde_json::json!({"anything": "true"});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["anything"], Value::String("true".into()));
    }

    #[test]
    fn coerce_string_encoded_object() {
        let tool = Tool {
            tool_type: "function".into(),
            function: types::ToolFunction {
                name: "test".into(),
                description: None,
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "data": {
                            "type": "object",
                            "properties": { "key": { "type": "string" } }
                        }
                    }
                })),
            },
        };
        let args = serde_json::json!({"data": "{\"key\": \"value\"}"});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["data"]["key"], Value::String("value".into()));
    }

    #[test]
    fn coerce_multiple_fields_simultaneously() {
        let tool = make_web_search_tool();
        let args = serde_json::json!({"query": "weather", "render": "true", "render_count": "5"});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["render"], Value::Bool(true));
        assert_eq!(result["render_count"], serde_json::json!(5.0));
    }

    #[test]
    fn coerce_already_correct_types_unchanged() {
        let tool = make_web_search_tool();
        let args = serde_json::json!({"query": "test", "render": true, "render_count": 3});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["render"], Value::Bool(true));
    }

    fn make_skill_tool() -> Tool {
        crate::defs::skill_api_tool()
    }

    #[test]
    fn coerce_skill_flattened_args_into_args_object() {
        let tool = make_skill_tool();
        let args = serde_json::json!({"command": "search_screenshots", "query": "today"});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["command"], Value::String("search_screenshots".into()));
        let inner = result.get("args").expect("args must be present");
        assert_eq!(inner["query"], Value::String("today".into()));
    }

    #[test]
    fn coerce_skill_flattened_args_multiple_keys() {
        let tool = make_skill_tool();
        let args = serde_json::json!({"command": "search_screenshots", "query": "browser", "k": 10, "mode": "substring"});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        let inner = result.get("args").expect("has args");
        assert_eq!(inner["query"], Value::String("browser".into()));
        assert_eq!(inner["k"], serde_json::json!(10));
    }

    #[test]
    fn coerce_skill_already_nested_args_unchanged() {
        let tool = make_skill_tool();
        let args = serde_json::json!({"command": "search_screenshots", "args": {"query": "today", "k": 5}});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        let inner = result.get("args").expect("has args");
        assert_eq!(inner["query"], Value::String("today".into()));
    }

    #[test]
    fn coerce_skill_no_extra_args() {
        let tool = make_skill_tool();
        let args = serde_json::json!({"command": "status"});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        assert_eq!(result["command"], Value::String("status".into()));
    }

    #[test]
    fn coerce_skill_existing_args_take_precedence() {
        let tool = make_skill_tool();
        let args = serde_json::json!({"command": "search_labels", "query": "flat", "args": {"query": "nested"}});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        let inner = result.get("args").expect("has args");
        assert_eq!(inner["query"], Value::String("nested".into()));
    }

    #[test]
    fn coerce_skill_arguments_alias_for_args() {
        let tool = make_skill_tool();
        let args = serde_json::json!({"command": "search_screenshots", "arguments": {"query": "browser"}});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        let inner = result.get("args").expect("should have args");
        assert_eq!(inner["query"], Value::String("browser".into()));
        assert!(result.get("arguments").is_none());
    }

    #[test]
    fn coerce_skill_arguments_alias_with_flat_extras() {
        let tool = make_skill_tool();
        let args = serde_json::json!({"command": "search_screenshots", "arguments": {"query": "code"}, "k": 5});
        let result = validate_tool_arguments(&tool, &args).expect("valid");
        let inner = result.get("args").expect("has args");
        assert_eq!(inner["query"], Value::String("code".into()));
        assert_eq!(inner["k"], serde_json::json!(5));
    }

    #[test]
    fn coerce_tool_call_arguments_fn() {
        let tools = vec![make_web_search_tool()];
        let mut call = ToolCall {
            id: "call_0".into(),
            call_type: "function".into(),
            function: types::ToolCallFunction {
                name: "web_search".into(),
                arguments: r#"{"query":"test","render":"true","render_count":"3"}"#.into(),
            },
        };
        let coerced = coerce_tool_call_arguments(&mut call, &tools);
        assert_eq!(coerced["render"], Value::Bool(true));
        let re_parsed: Value = serde_json::from_str(&call.function.arguments).expect("valid json");
        assert_eq!(re_parsed["render"], Value::Bool(true));
    }
}
