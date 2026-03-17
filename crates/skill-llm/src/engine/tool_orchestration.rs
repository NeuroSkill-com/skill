// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tool-call orchestration: extraction, validation, sequential/parallel
//! execution, and the multi-round chat loop.

use std::sync::{Arc, atomic::Ordering};

use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::tools;
use crate::config;
use super::protocol::{InferRequest, InferToken, GenParams};
use super::state::LlmServerState;
use super::images::extract_images_from_messages;

use skill_tools::defs::{enabled_builtin_llm_tools, filter_allowed_tool_defs, is_builtin_tool_enabled};
use skill_tools::exec::execute_builtin_tool_call;
use skill_tools::context::trim_messages_to_fit;

// ── Stream sanitizer ──────────────────────────────────────────────────────────

struct ToolCallStreamSanitizer {
    raw:                 String,
    emitted_visible_len: usize,
}

impl ToolCallStreamSanitizer {
    fn new() -> Self {
        Self { raw: String::new(), emitted_visible_len: 0 }
    }

    fn push(&mut self, piece: &str) -> String {
        self.raw.push_str(piece);
        let visible = tools::strip_tool_call_blocks_preserve(&self.raw);
        if visible.len() <= self.emitted_visible_len {
            return String::new();
        }
        if !visible.is_char_boundary(self.emitted_visible_len) {
            return String::new();
        }

        let delta = visible[self.emitted_visible_len..].to_string();
        self.emitted_visible_len = visible.len();
        delta
    }
}

// ── Collect infer output ──────────────────────────────────────────────────────

async fn collect_infer_output<F>(
    mut tok_rx: mpsc::UnboundedReceiver<InferToken>,
    mut on_visible_delta: F,
) -> Result<(String, String, usize, usize, usize), String>
where
    F: FnMut(&str),
{
    let mut text              = String::new();
    let mut finish_reason     = "stop".to_string();
    let mut prompt_tokens     = 0usize;
    let mut completion_tokens = 0usize;
    let mut n_ctx             = 0usize;
    let mut sanitizer         = ToolCallStreamSanitizer::new();

    while let Some(tok) = tok_rx.recv().await {
        match tok {
            InferToken::Delta(t) => {
                text.push_str(&t);
                let visible = sanitizer.push(&t);
                if !visible.is_empty() {
                    on_visible_delta(&visible);
                }
            }
            InferToken::Done { finish_reason: fr, prompt_tokens: pt, completion_tokens: ct, n_ctx: nc } => {
                finish_reason = fr;
                prompt_tokens = pt;
                completion_tokens = ct;
                n_ctx = nc;
                break;
            }
            InferToken::Error(e) => return Err(e),
        }
    }

    Ok((text, finish_reason, prompt_tokens, completion_tokens, n_ctx))
}

// ── Callback types ────────────────────────────────────────────────────────────

/// Callback signatures for tool-call lifecycle hooks (pi-mono style).
///
/// `BeforeToolCallFn`: called after argument validation but before execution.
/// Return `Some(reason)` to block execution (the reason becomes the error
/// message in the tool result).  Return `None` to allow execution.
///
/// `AfterToolCallFn`: called after execution with the raw result.
/// Return `Some(replacement)` to override the result; `None` to keep it.
#[allow(dead_code)]
pub type BeforeToolCallFn = Box<dyn Fn(&tools::ToolCall, &Value) -> Option<String> + Send + Sync>;
#[allow(dead_code)]
pub type AfterToolCallFn  = Box<dyn Fn(&tools::ToolCall, &Value, bool) -> Option<(Value, bool)> + Send + Sync>;

/// Extended tool-call event sink (pi-mono style lifecycle events).
pub enum ToolEvent {
    /// Legacy: simple status string (kept for backwards compat).
    Status { tool_name: String, status: String, detail: Option<String> },
    /// Tool execution is about to begin (after validation).
    ExecutionStart { tool_call_id: String, tool_name: String, args: Value },
    /// Tool execution finished.
    ExecutionEnd { tool_call_id: String, tool_name: String, result: Value, is_error: bool },
}

// ── Main orchestration loop ───────────────────────────────────────────────────

pub async fn run_chat_with_builtin_tools<F, G>(
    state: &LlmServerState,
    base_messages: Vec<Value>,
    params: GenParams,
    mut tools_from_req: Vec<tools::Tool>,
    mut on_visible_delta: F,
    mut on_tool_event: G,
) -> Result<(String, String, usize, usize, usize), String>
where
    F: FnMut(&str),
    G: FnMut(ToolEvent),
{
    let cancelled_set = state.cancelled_tool_calls.clone();
    // Clear cancelled set at the start of a new chat request.
    { cancelled_set.lock().unwrap().clear(); }
    let allowed_tools = state.allowed_tools.lock().unwrap().clone();

    let max_rounds         = allowed_tools.max_rounds;
    let max_calls_per_round = allowed_tools.max_calls_per_round;
    let execution_mode     = allowed_tools.execution_mode.clone();

    let mut messages = base_messages;
    if tools_from_req.is_empty() {
        tools_from_req = enabled_builtin_llm_tools(&allowed_tools);
    } else {
        tools_from_req = filter_allowed_tool_defs(tools_from_req, &allowed_tools);
    }
    let n_ctx = state.n_ctx.load(Ordering::Relaxed);
    tools::inject_tools_into_system_prompt(&mut messages, &tools_from_req, n_ctx);

    // Inject discovered Agent Skills into the system prompt so the LLM knows
    // which specialised instruction files it can load via read_file.
    let skills_block = skill_skills::format_skills_for_prompt(&state.skills);
    if !skills_block.is_empty() {
        let has_system = messages.first()
            .and_then(|m| m.get("role"))
            .and_then(|r| r.as_str()) == Some("system");
        if has_system {
            if let Some(content) = messages[0].get("content").and_then(|c| c.as_str()).map(|s| s.to_string()) {
                messages[0]["content"] = serde_json::Value::String(format!("{content}{skills_block}"));
            }
        } else {
            messages.insert(0, json!({ "role": "system", "content": skills_block }));
        }
    }

    // Build a lookup map for argument validation.
    let tool_defs: std::collections::HashMap<String, tools::Tool> = tools_from_req
        .iter()
        .map(|t| (t.function.name.clone(), t.clone()))
        .collect();

    // Cross-round dedup: track (tool_name, arguments) pairs already executed.
    let mut executed_calls = std::collections::HashSet::<(String, String)>::new();

    for _ in 0..=max_rounds {
        // ── Context-aware history trimming ──────────────────────────────
        trim_messages_to_fit(&mut messages, n_ctx);

        let images = extract_images_from_messages(&messages);
        let (tok_tx, tok_rx) = mpsc::unbounded_channel();
        state.req_tx
            .send(InferRequest::Generate {
                messages: messages.clone(),
                images,
                params: params.clone(),
                token_tx: tok_tx,
            })
            .map_err(|_| "LLM actor has exited".to_string())?;

        let (assistant_text, finish_reason, prompt_tokens, completion_tokens, n_ctx) = collect_infer_output(tok_rx, |delta| {
            on_visible_delta(delta);
        }).await?;
        let tool_calls = tools::extract_tool_calls(&assistant_text);
        if tool_calls.is_empty() {
            let cleaned = tools::strip_tool_call_blocks(&assistant_text);
            return Ok((cleaned, finish_reason, prompt_tokens, completion_tokens, n_ctx));
        }

        let cleaned = tools::strip_tool_call_blocks(&assistant_text);

        // Filter out empty bash calls and cross-round duplicates.
        let selected_calls: Vec<tools::ToolCall> = tool_calls
            .into_iter()
            .filter(|tc| {
                if tc.function.name == "bash" {
                    let args: Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(Value::Object(Default::default()));
                    if args.get("command").and_then(|c| c.as_str()).unwrap_or("").is_empty() {
                        return false;
                    }
                }
                let key = (tc.function.name.clone(), tc.function.arguments.clone());
                if executed_calls.contains(&key) {
                    return false;
                }
                true
            })
            .take(max_calls_per_round)
            .collect();

        if selected_calls.is_empty() {
            return Ok((cleaned, finish_reason, prompt_tokens, completion_tokens, n_ctx));
        }

        // Always push an assistant message to maintain alternation.
        let assistant_content = if cleaned.trim().is_empty() {
            "[Calling tools…]".to_string()
        } else {
            cleaned
        };
        messages.push(json!({
            "role": "assistant",
            "content": assistant_content,
        }));

        // Record for cross-round dedup.
        for tc in &selected_calls {
            executed_calls.insert((tc.function.name.clone(), tc.function.arguments.clone()));
        }

        match execution_mode {
            config::ToolExecutionMode::Sequential => {
                execute_tool_calls_sequential(
                    &selected_calls, &tool_defs, &allowed_tools,
                    &mut messages, &mut on_tool_event,
                    &cancelled_set, &state.scripts_dir,
                ).await;
            }
            config::ToolExecutionMode::Parallel => {
                execute_tool_calls_parallel(
                    &selected_calls, &tool_defs, &allowed_tools,
                    &mut messages, &mut on_tool_event,
                    &cancelled_set, &state.scripts_dir,
                ).await;
            }
        }
    }

    Err(format!("tool-calling round limit reached ({max_rounds} rounds). You can increase this in Settings → LLM → Tools → Max rounds."))
}

// ── Validation ────────────────────────────────────────────────────────────────

/// Validate arguments for a tool call.  Returns the parsed args `Value` or an
/// error result to inject directly.
fn validate_and_prepare(
    tc: &tools::ToolCall,
    tool_defs: &std::collections::HashMap<String, tools::Tool>,
    allowed_tools: &config::LlmToolConfig,
) -> Result<Value, Value> {
    if !is_builtin_tool_enabled(allowed_tools, &tc.function.name) {
        return Err(json!({ "ok": false, "tool": tc.function.name, "error": "tool disabled in settings" }));
    }

    let args: Value = serde_json::from_str(&tc.function.arguments)
        .unwrap_or_else(|_| json!({}));

    if let Some(tool_def) = tool_defs.get(&tc.function.name) {
        match tools::validate_tool_arguments(tool_def, &args) {
            Ok(validated) => Ok(validated),
            Err(err_msg) => Err(json!({ "ok": false, "tool": tc.function.name, "error": err_msg })),
        }
    } else {
        Ok(args)
    }
}

// ── Sequential execution ──────────────────────────────────────────────────────

async fn execute_tool_calls_sequential<G>(
    calls: &[tools::ToolCall],
    tool_defs: &std::collections::HashMap<String, tools::Tool>,
    allowed_tools: &config::LlmToolConfig,
    messages: &mut Vec<Value>,
    on_tool_event: &mut G,
    cancelled_set: &Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    scripts_dir: &std::path::Path,
)
where
    G: FnMut(ToolEvent),
{
    for tc in calls {
        // Check if cancelled before execution.
        if cancelled_set.lock().unwrap().contains(&tc.id) {
            let cancel_result = json!({ "ok": false, "tool": tc.function.name, "error": "cancelled by user" });
            on_tool_event(ToolEvent::Status {
                tool_name: tc.function.name.clone(),
                status: "cancelled".into(),
                detail: Some("cancelled by user".into()),
            });
            on_tool_event(ToolEvent::ExecutionEnd {
                tool_call_id: tc.id.clone(),
                tool_name: tc.function.name.clone(),
                result: cancel_result.clone(),
                is_error: true,
            });
            messages.push(json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": cancel_result.to_string(),
            }));
            continue;
        }

        let args_result = validate_and_prepare(tc, tool_defs, allowed_tools);

        let args_for_event = match &args_result {
            Ok(v) => v.clone(),
            Err(_) => serde_json::from_str(&tc.function.arguments).unwrap_or(json!({})),
        };

        // Emit start events.
        let detail_str = if tc.function.arguments.len() > 2 {
            Some(tc.function.arguments.clone())
        } else { None };
        on_tool_event(ToolEvent::Status {
            tool_name: tc.function.name.clone(),
            status: "calling".into(),
            detail: detail_str,
        });
        on_tool_event(ToolEvent::ExecutionStart {
            tool_call_id: tc.id.clone(),
            tool_name: tc.function.name.clone(),
            args: args_for_event,
        });

        // Re-check cancellation after emitting start.
        let (tool_result, is_error) = if cancelled_set.lock().unwrap().contains(&tc.id) {
            (json!({ "ok": false, "tool": tc.function.name, "error": "cancelled by user" }), true)
        } else {
            match args_result {
                Err(err_val) => (err_val, true),
                Ok(_) => {
                    let result = execute_builtin_tool_call(tc, allowed_tools, scripts_dir).await;
                    let ok = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                    (result, !ok)
                }
            }
        };

        // Emit end events.
        on_tool_event(ToolEvent::Status {
            tool_name: tc.function.name.clone(),
            status: if is_error { "error" } else { "done" }.into(),
            detail: if is_error { tool_result.get("error").and_then(|v| v.as_str()).map(|s| s.to_string()) } else { None },
        });
        on_tool_event(ToolEvent::ExecutionEnd {
            tool_call_id: tc.id.clone(),
            tool_name: tc.function.name.clone(),
            result: tool_result.clone(),
            is_error,
        });

        messages.push(json!({
            "role": "tool",
            "tool_call_id": tc.id,
            "content": tool_result.to_string(),
        }));
    }
}

// ── Parallel execution ────────────────────────────────────────────────────────

async fn execute_tool_calls_parallel<G>(
    calls: &[tools::ToolCall],
    tool_defs: &std::collections::HashMap<String, tools::Tool>,
    allowed_tools: &config::LlmToolConfig,
    messages: &mut Vec<Value>,
    on_tool_event: &mut G,
    cancelled_set: &Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    scripts_dir: &std::path::Path,
)
where
    G: FnMut(ToolEvent),
{
    // Phase 1: Prepare all calls.
    struct PreparedCall {
        tc: tools::ToolCall,
        validation: Result<Value, Value>,
    }

    let mut prepared = Vec::with_capacity(calls.len());
    for tc in calls {
        if cancelled_set.lock().unwrap().contains(&tc.id) {
            let cancel_result = json!({ "ok": false, "tool": tc.function.name, "error": "cancelled by user" });
            on_tool_event(ToolEvent::Status {
                tool_name: tc.function.name.clone(),
                status: "cancelled".into(),
                detail: Some("cancelled by user".into()),
            });
            on_tool_event(ToolEvent::ExecutionEnd {
                tool_call_id: tc.id.clone(),
                tool_name: tc.function.name.clone(),
                result: cancel_result.clone(),
                is_error: true,
            });
            messages.push(json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": cancel_result.to_string(),
            }));
            continue;
        }

        let args_result = validate_and_prepare(tc, tool_defs, allowed_tools);

        let args_for_event = match &args_result {
            Ok(v) => v.clone(),
            Err(_) => serde_json::from_str(&tc.function.arguments).unwrap_or(json!({})),
        };

        let detail_str = if tc.function.arguments.len() > 2 {
            Some(tc.function.arguments.clone())
        } else { None };
        on_tool_event(ToolEvent::Status {
            tool_name: tc.function.name.clone(),
            status: "calling".into(),
            detail: detail_str,
        });
        on_tool_event(ToolEvent::ExecutionStart {
            tool_call_id: tc.id.clone(),
            tool_name: tc.function.name.clone(),
            args: args_for_event,
        });

        prepared.push(PreparedCall { tc: tc.clone(), validation: args_result });
    }

    // Phase 2: Execute concurrently.
    let mut futures = Vec::with_capacity(prepared.len());
    for p in &prepared {
        let tc = p.tc.clone();
        let allowed = allowed_tools.clone();
        let is_valid = p.validation.is_ok();
        let cancel_check = cancelled_set.clone();
        let sdir = scripts_dir.to_path_buf();

        if is_valid {
            futures.push(tokio::spawn(async move {
                if cancel_check.lock().unwrap().contains(&tc.id) {
                    return (tc.clone(), json!({ "ok": false, "tool": tc.function.name, "error": "cancelled by user" }), true);
                }
                let result = execute_builtin_tool_call(&tc, &allowed, &sdir).await;
                let ok = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                (tc, result, !ok)
            }));
        } else {
            let err_val = p.validation.as_ref().err().unwrap().clone();
            futures.push(tokio::spawn(async move {
                (tc, err_val, true)
            }));
        }
    }

    // Phase 3: Collect results in source order.
    for future in futures {
        let (tc, tool_result, is_error) = future.await.unwrap_or_else(|e| {
            let tc = calls[0].clone(); // fallback
            (tc, json!({"ok": false, "error": e.to_string()}), true)
        });

        on_tool_event(ToolEvent::Status {
            tool_name: tc.function.name.clone(),
            status: if is_error { "error" } else { "done" }.into(),
            detail: if is_error { tool_result.get("error").and_then(|v| v.as_str()).map(|s| s.to_string()) } else { None },
        });
        on_tool_event(ToolEvent::ExecutionEnd {
            tool_call_id: tc.id.clone(),
            tool_name: tc.function.name.clone(),
            result: tool_result.clone(),
            is_error,
        });

        messages.push(json!({
            "role": "tool",
            "tool_call_id": tc.id,
            "content": tool_result.to_string(),
        }));
    }
}
