# skill-tools

LLM tool definitions, execution, and parsing for NeuroSkill.

## Overview

Implements the function-calling layer for the local LLM: declares the available tools (web search, file read/write, bash execution, etc.), parses tool-call blocks from model output, validates arguments against JSON schemas, and executes tool actions with safety checks.

## Modules

| Module | Description |
|---|---|
| `types` | `LlmToolConfig` — per-tool enable/disable flags and `ToolExecutionMode` (auto / confirm / deny) |
| `parse` | Tool-call extraction from raw LLM output, JSON schema validation, system-prompt injection, and stripping of tool-call blocks. Core types: `Tool`, `ToolCall`, `ToolFunction`, `ChatMessage`, `MessageContent`, `ContentPart`. |
| `defs` | `builtin_llm_tools()` — registry of all built-in tool definitions with JSON schemas; `enabled_builtin_llm_tools()` / `filter_allowed_tool_defs()` — config-aware filtering |
| `exec` | Tool execution: `resolve_tool_path`, `check_bash_safety`, `check_path_safety`, `truncate_text`; runs shell commands, reads/writes files |
| `search` | Web search backends: DuckDuckGo HTML, Brave API, SearXNG; headless URL fetch and rendering |
| `context` | Token estimation and context-window management: `estimate_tokens`, `estimate_messages_tokens`, `trim_messages_to_fit` |
| `log` | Standalone pluggable logger for tool-call tracing. Install a custom sink with `set_log_callback`; toggle at runtime with `set_log_enabled`. Falls back to `eprintln!` when no callback is registered. Use the `tool_log!` macro from call sites. |

## Key types

| Type | Description |
|---|---|
| `Tool` / `ToolFunction` | Tool definition with name, description, and JSON schema |
| `ToolCall` / `ToolCallFunction` | Parsed tool invocation from model output |
| `ChatMessage` | Unified message type (system / user / assistant / tool) |
| `LlmToolConfig` | Per-tool configuration: enabled, execution mode |
| `ToolExecutionMode` | `Auto` / `Confirm` / `Deny` |

## Key functions

| Function | Description |
|---|---|
| `extract_tool_calls(content)` | Parse `<tool_call>` blocks from LLM text |
| `inject_tools_into_system_prompt` | Append tool schemas to the system message |
| `validate_tool_arguments(tool, args)` | Validate call arguments against the tool's JSON schema |
| `strip_tool_call_blocks` | Remove tool-call markup from displayed text |
| `builtin_llm_tools()` | Full list of built-in tools |
| `check_bash_safety` / `check_path_safety` | Reject dangerous shell commands or paths |

## Dependencies

- `serde` / `serde_json` — serialization
- `chrono` — timestamps in tool context
- `jsonschema` — argument validation
- `regex` — tool-call block parsing
- `ureq` / `urlencoding` — web search execution
- `dirs` — home directory resolution
- `rfd` — native file dialogs
- `tokio` — async execution
