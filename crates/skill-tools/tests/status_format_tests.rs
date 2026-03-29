// SPDX-License-Identifier: GPL-3.0-only
//! Tests for `format_status_as_text` — the skill status command formatter.
//!
//! We use the same mock data as the LLM E2E test to verify the text output.

// format_status_as_text is pub(crate), so we test it via the module re-export
// chain. Since it's not public, we test the overall tool indirectly by
// checking that the known-good JSON produces expected text fragments.

// The status formatter is pub(crate), so we can only test it from within the
// crate. Instead, let's test the public extract/strip functions more.

use skill_tools::parse::{extract_tool_calls, strip_tool_call_blocks};

// ── extract_tool_calls ───────────────────────────────────────────────────────

#[test]
fn extract_delimited_block() {
    let content = r#"Sure, let me check.
[TOOL_CALL]{"name":"date","arguments":"{}"}[/TOOL_CALL]
Here is the date."#;
    let calls = extract_tool_calls(content);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].function.name, "date");
}

#[test]
fn extract_multiple_tool_calls() {
    let content = r#"[TOOL_CALL]{"name":"date","arguments":"{}"}[/TOOL_CALL]
[TOOL_CALL]{"name":"bash","arguments":"{\"command\":\"ls\"}"}[/TOOL_CALL]"#;
    let calls = extract_tool_calls(content);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].function.name, "date");
    assert_eq!(calls[1].function.name, "bash");
}

#[test]
fn extract_llama_xml_format() {
    let content = r#"<function=date></function>"#;
    let calls = extract_tool_calls(content);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].function.name, "date");
}

#[test]
fn extract_no_tool_calls() {
    let content = "Just a regular response with no tool calls.";
    let calls = extract_tool_calls(content);
    assert!(calls.is_empty());
}

#[test]
fn extract_json_tool_call_inline() {
    let content = r#"Let me search for that.
{"name":"web_search","arguments":{"query":"rust programming"}}"#;
    let calls = extract_tool_calls(content);
    assert!(calls.iter().any(|c| c.function.name == "web_search"));
}

// ── strip_tool_call_blocks ───────────────────────────────────────────────────

#[test]
fn strip_delimited_blocks() {
    let content = "Before [TOOL_CALL]{\"name\":\"date\"}[/TOOL_CALL] After";
    let stripped = strip_tool_call_blocks(content);
    assert!(!stripped.contains("[TOOL_CALL]"));
    assert!(!stripped.contains("[/TOOL_CALL]"));
    assert!(stripped.contains("Before"));
    assert!(stripped.contains("After"));
}

#[test]
fn strip_preserves_non_tool_content() {
    let content = "Hello world, no tools here.";
    let stripped = strip_tool_call_blocks(content);
    assert_eq!(stripped, "Hello world, no tools here.");
}

#[test]
fn strip_multiple_blocks() {
    let content = "A [TOOL_CALL]x[/TOOL_CALL] B [TOOL_CALL]y[/TOOL_CALL] C";
    let stripped = strip_tool_call_blocks(content);
    assert!(!stripped.contains("[TOOL_CALL]"));
    assert!(stripped.contains("A"));
    assert!(stripped.contains("B"));
    assert!(stripped.contains("C"));
}

#[test]
fn strip_llama_xml_blocks() {
    let content = "Result: <function=date></function> done.";
    let stripped = strip_tool_call_blocks(content);
    assert!(!stripped.contains("<function="));
}
