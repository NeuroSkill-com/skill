// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tool-call orchestration: extraction, validation, sequential/parallel
//! execution, and the multi-round chat loop.

use skill_constants::MutexExt;
use std::sync::{atomic::Ordering, Arc};

use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::images::extract_images_from_messages;
use super::protocol::{GenParams, InferRequest, InferToken};
use super::state::LlmServerState;
use crate::config;
use crate::tools;

use anyhow::Context;
use skill_tools::context::trim_messages_to_fit;
use skill_tools::defs::{
    enabled_builtin_llm_tools, filter_allowed_tool_defs, is_builtin_tool_enabled, rerank_tools_for_prompt,
};
use skill_tools::exec::execute_builtin_tool_call;
use skill_tools::types::ChatMode;

// ── Stream sanitizer ──────────────────────────────────────────────────────────

struct ToolCallStreamSanitizer {
    raw: String,
    emitted_visible_len: usize,
}

impl ToolCallStreamSanitizer {
    fn new() -> Self {
        Self {
            raw: String::new(),
            emitted_visible_len: 0,
        }
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
) -> anyhow::Result<(String, String, usize, usize, usize)>
where
    F: FnMut(&str),
{
    let mut text = String::new();
    let mut finish_reason = "stop".to_string();
    let mut prompt_tokens = 0usize;
    let mut completion_tokens = 0usize;
    let mut n_ctx = 0usize;
    let mut sanitizer = ToolCallStreamSanitizer::new();

    while let Some(tok) = tok_rx.recv().await {
        match tok {
            InferToken::Delta(t) => {
                text.push_str(&t);
                let visible = sanitizer.push(&t);
                if !visible.is_empty() {
                    on_visible_delta(&visible);
                }
            }
            InferToken::Done {
                finish_reason: fr,
                prompt_tokens: pt,
                completion_tokens: ct,
                n_ctx: nc,
            } => {
                finish_reason = fr;
                prompt_tokens = pt;
                completion_tokens = ct;
                n_ctx = nc;
                break;
            }
            InferToken::Error(e) => return Err(anyhow::anyhow!("{e}")),
        }
    }

    Ok((text, finish_reason, prompt_tokens, completion_tokens, n_ctx))
}

// ── Prompt helpers ────────────────────────────────────────────────────────────

fn message_text_content(msg: &Value) -> String {
    if let Some(s) = msg.get("content").and_then(|c| c.as_str()) {
        return s.to_string();
    }
    let mut out = String::new();
    if let Some(parts) = msg.get("content").and_then(|c| c.as_array()) {
        for p in parts {
            if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(t);
            }
        }
    }
    out
}

fn latest_user_prompt(messages: &[Value]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .map(message_text_content)
        .filter(|s| !s.trim().is_empty())
}

fn apply_runtime_prompt_variables(
    messages: &mut [Value],
    state: &LlmServerState,
    allowed_tools: &config::LlmToolConfig,
) {
    use std::collections::HashMap;

    fn expand(s: &str, vars: &HashMap<String, String>) -> String {
        let mut out = String::with_capacity(s.len());
        let bytes = s.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'{' {
                if let Some(close_rel) = s[i + 1..].find('}') {
                    let end = i + 1 + close_rel;
                    let key = &s[i + 1..end];
                    if let Some(v) = vars.get(key) {
                        out.push_str(v);
                        i = end + 1;
                        continue;
                    }
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut vars = HashMap::<String, String>::new();
    vars.insert("time".into(), now.to_string());
    vars.insert("date".into(), now.to_string());
    vars.insert("datetime".into(), now.to_string());
    vars.insert("os".into(), std::env::consts::OS.to_string());
    vars.insert("arch".into(), std::env::consts::ARCH.to_string());
    vars.insert(
        "home".into(),
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| "~".into()),
    );
    vars.insert(
        "cwd".into(),
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    );
    vars.insert("llm.model".into(), state.model_name.clone());
    vars.insert("llm.n_ctx".into(), state.n_ctx.load(Ordering::Relaxed).to_string());

    for (k, v) in &allowed_tools.prompt_variables {
        vars.insert(k.clone(), v.clone());
    }

    for msg in messages.iter_mut() {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if !matches!(role, "system" | "user") {
            continue;
        }

        if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
            msg["content"] = Value::String(expand(content, &vars));
            continue;
        }

        if let Some(parts) = msg.get_mut("content").and_then(|c| c.as_array_mut()) {
            for p in parts {
                if let Some(text) = p.get("text").and_then(|t| t.as_str()) {
                    p["text"] = Value::String(expand(text, &vars));
                }
            }
        }
    }
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
pub type AfterToolCallFn = Box<dyn Fn(&tools::ToolCall, &Value, bool) -> Option<(Value, bool)> + Send + Sync>;

/// Extended tool-call event sink (pi-mono style lifecycle events).
pub enum ToolEvent {
    /// Legacy: simple status string (kept for backwards compat).
    Status {
        tool_name: String,
        status: String,
        detail: Option<String>,
    },
    /// Tool execution is about to begin (after validation).
    ExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: Value,
    },
    /// Tool execution finished.
    ExecutionEnd {
        tool_call_id: String,
        tool_name: String,
        result: Value,
        is_error: bool,
    },
    /// Emitted after each inference + tool-execution round completes.
    /// Allows consumers to track cumulative token usage across the
    /// multi-round tool-calling loop (inspired by agentic CLI patterns).
    RoundComplete {
        round: usize,
        prompt_tokens: usize,
        completion_tokens: usize,
        tool_calls_count: usize,
    },
}

// ── Main orchestration loop ───────────────────────────────────────────────────

pub async fn run_chat_with_builtin_tools<F, G>(
    state: &LlmServerState,
    base_messages: Vec<Value>,
    params: GenParams,
    mut tools_from_req: Vec<tools::Tool>,
    mut on_visible_delta: F,
    mut on_tool_event: G,
) -> anyhow::Result<(String, String, usize, usize, usize)>
where
    F: FnMut(&str),
    G: FnMut(ToolEvent),
{
    let cancelled_set = state.cancelled_tool_calls.clone();
    // Clear cancelled set at the start of a new chat request.
    {
        cancelled_set.lock_or_recover().clear();
    }
    let allowed_tools = state.allowed_tools.lock_or_recover().clone();

    let max_rounds = allowed_tools.max_rounds;
    let max_calls_per_round = allowed_tools.max_calls_per_round;
    let execution_mode = allowed_tools.execution_mode.clone();
    let compression = allowed_tools.context_compression.clone();
    let tool_thinking_budget = allowed_tools.thinking_budget;

    let mut messages = base_messages;
    apply_runtime_prompt_variables(&mut messages, state, &allowed_tools);

    if tools_from_req.is_empty() {
        tools_from_req = enabled_builtin_llm_tools(&allowed_tools);
    } else {
        tools_from_req = filter_allowed_tool_defs(tools_from_req, &allowed_tools);
    }

    // Intelligent skill selection: rerank and keep only top-N most relevant tools.
    if allowed_tools.intelligent_selection_enabled {
        if let Some(prompt) = latest_user_prompt(&messages) {
            tools_from_req =
                rerank_tools_for_prompt(&prompt, tools_from_req, allowed_tools.intelligent_selection_top_n);
        }
    }

    let n_ctx = state.n_ctx.load(Ordering::Relaxed);
    let prompt_opts = skill_tools::parse::ToolPromptOptions {
        prefer_native_tool_calling: allowed_tools.prefer_native_tool_calling,
        chat_mode: Some(
            match allowed_tools.chat_mode {
                ChatMode::Automatic => "automatic",
                ChatMode::Chat => "chat",
                ChatMode::Query => "query",
            }
            .to_string(),
        ),
        query_refusal_response: Some(allowed_tools.query_refusal_response.clone()),
    };
    tools::inject_tools_into_system_prompt_with_options(&mut messages, &tools_from_req, n_ctx, &prompt_opts);

    // Inject discovered Agent Skills into the system prompt so the LLM knows
    // which specialised instruction files it can load via read_file.
    // Filter out skills the user has explicitly disabled.
    let disabled = &allowed_tools.disabled_skills;
    let filtered_skills: Vec<&skill_skills::Skill> = state
        .skills
        .iter()
        .filter(|s| !disabled.iter().any(|d| d == &s.name))
        .collect();
    let filtered_refs: Vec<skill_skills::Skill> = filtered_skills.into_iter().cloned().collect();
    let skills_block = skill_skills::format_skills_for_prompt(&filtered_refs);
    if !skills_block.is_empty() {
        let has_system = messages.first().and_then(|m| m.get("role")).and_then(|r| r.as_str()) == Some("system");
        if has_system {
            if let Some(content) = messages[0]
                .get("content")
                .and_then(|c| c.as_str())
                .map(std::string::ToString::to_string)
            {
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
    // Track last successful tool result so we can surface it if the model
    // never produces a text response.
    let mut last_tool_result: Option<String> = None;
    let mut had_successful_tool_result = false;
    let mut dedup_nudge_count = 0u32;
    let mut self_heal_count = 0u32;
    let mut cumulative_prompt_tokens = 0usize;
    let mut cumulative_completion_tokens = 0usize;
    const MAX_SELF_HEAL_ATTEMPTS: u32 = 2;

    for round_idx in 0..=max_rounds {
        // ── Context-aware history trimming ──────────────────────────────
        trim_messages_to_fit(&mut messages, n_ctx, &compression);

        let images = extract_images_from_messages(&messages);
        // Apply tool-specific thinking budget override if configured.
        let mut round_params = params.clone();
        if let Some(budget) = tool_thinking_budget {
            round_params.thinking_budget = Some(budget);
        }
        let (tok_tx, tok_rx) = mpsc::unbounded_channel();
        state
            .req_tx
            .send(InferRequest::Generate {
                messages: messages.clone(),
                images,
                params: round_params,
                token_tx: tok_tx,
            })
            .context("LLM actor has exited")?;

        let (assistant_text, finish_reason, prompt_tokens, completion_tokens, n_ctx) =
            collect_infer_output(tok_rx, |delta| {
                on_visible_delta(delta);
            })
            .await?;
        cumulative_prompt_tokens += prompt_tokens;
        cumulative_completion_tokens += completion_tokens;
        let tool_calls = tools::extract_tool_calls(&assistant_text);
        if tool_calls.is_empty() {
            // ── Self-healing: detect garbled tool-call attempts ──────────
            // If the model tried to emit a tool call but it was malformed,
            // inject a corrective message and let the loop retry.
            if self_heal_count < MAX_SELF_HEAL_ATTEMPTS {
                if let Some(garbled) = tools::detect_garbled_tool_call(&assistant_text) {
                    self_heal_count += 1;
                    log::info!(
                        "[tool-orchestration] garbled tool call detected (attempt {}/{}), requesting re-emit",
                        self_heal_count,
                        MAX_SELF_HEAL_ATTEMPTS
                    );

                    // Push the failed assistant message so the model sees what it emitted.
                    messages.push(json!({
                        "role": "assistant",
                        "content": assistant_text,
                    }));
                    // Inject a corrective user message.
                    messages.push(json!({
                        "role": "user",
                        "content": tools::build_self_healing_message(&garbled),
                    }));
                    continue;
                }
            }

            let cleaned = tools::strip_tool_call_blocks(&assistant_text);
            // If the model returned empty text after we already have tool
            // results, surface the raw result as a fallback.
            if cleaned.trim().is_empty() {
                if let Some(ref result) = last_tool_result {
                    log::warn!("[tool-orchestration] model returned empty after tool call — returning raw result");
                    let fallback = format!("*(The model could not summarize the tool output. Here is the raw result:)*\n\n```json\n{}\n```", result);
                    return Ok((fallback, finish_reason, prompt_tokens, completion_tokens, n_ctx));
                }
            }

            // Strict query mode: refuse if we have no tool-backed evidence yet.
            if matches!(allowed_tools.chat_mode, ChatMode::Query) && !had_successful_tool_result {
                return Ok((
                    allowed_tools.query_refusal_response.clone(),
                    "no_evidence".into(),
                    prompt_tokens,
                    completion_tokens,
                    n_ctx,
                ));
            }

            return Ok((cleaned, finish_reason, prompt_tokens, completion_tokens, n_ctx));
        }

        let cleaned = tools::strip_tool_call_blocks(&assistant_text);

        // Filter out empty bash calls and cross-round duplicates.
        let n_raw_calls = tool_calls.len();
        let selected_calls: Vec<tools::ToolCall> = tool_calls
            .into_iter()
            .filter(|tc| {
                if tc.function.name == "bash" {
                    let args: Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Object(Default::default()));
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
            // If there's meaningful text alongside the (deduped) tool calls,
            // return it — the model wrote something useful.
            if !cleaned.trim().is_empty() {
                return Ok((cleaned, finish_reason, prompt_tokens, completion_tokens, n_ctx));
            }
            // All tool calls were duplicates and no visible text was produced.
            // The model is stuck re-emitting the same call.
            dedup_nudge_count += 1;
            if dedup_nudge_count > 2 {
                // Model is hopelessly stuck — return the raw tool result.
                log::warn!(
                    "[tool-orchestration] model stuck after {} dedup nudges — returning raw tool result",
                    dedup_nudge_count
                );
                if let Some(ref result) = last_tool_result {
                    let fallback = format!("*(The model could not summarize the tool output. Here is the raw result:)*\n\n```json\n{}\n```", result);
                    return Ok((fallback, finish_reason, prompt_tokens, completion_tokens, n_ctx));
                }
            }
            // Inject a nudge telling it the results are already available,
            // then let the loop run one more inference round.
            log::info!(
                "[tool-orchestration] all {} tool calls deduped (nudge #{}), injecting nudge",
                n_raw_calls,
                dedup_nudge_count
            );
            messages.push(json!({
                "role": "user",
                "content": "The tool has already been called and the results are above. Do NOT call any tools. Answer my original question using the tool results you already have."
            }));
            continue;
        }

        // Always push an assistant message to maintain alternation.
        // Include the tool call details so the model knows what it already
        // called and can interpret the following tool-result "user" message
        // as a result (not a new question).
        let tool_call_summary: String = selected_calls
            .iter()
            .map(|tc| {
                let args_preview: String = tc.function.arguments.chars().take(120).collect();
                format!("{}({})", tc.function.name, args_preview)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let assistant_content = if cleaned.trim().is_empty() {
            format!("[Calling tool: {}]", tool_call_summary)
        } else {
            format!("{}\n[Calling tool: {}]", cleaned, tool_call_summary)
        };
        messages.push(json!({
            "role": "assistant",
            "content": assistant_content,
        }));

        // Auto-redirect: if the LLM called a Skill API sub-command as a
        // top-level tool (e.g. tool="status" instead of tool="skill" +
        // command="status"), silently rewrite the call so it goes through
        // the skill tool with the correct payload.
        let mut selected_calls: Vec<tools::ToolCall> = selected_calls
            .into_iter()
            .map(|mut tc| {
                if !skill_tools::defs::is_known_builtin_tool(&tc.function.name)
                    && skill_tools::defs::is_skill_api_command(&tc.function.name)
                {
                    // Parse whatever args the LLM sent (may be empty).
                    let orig_args: Value = serde_json::from_str(&tc.function.arguments).unwrap_or_else(|_| json!({}));
                    // Build the redirected payload: { "command": "<name>", "args": { ...orig } }
                    let mut redirected = json!({ "command": tc.function.name });
                    if let Some(obj) = orig_args.as_object() {
                        if !obj.is_empty() {
                            redirected["args"] = orig_args;
                        }
                    }
                    log::info!("[tool-redirect] {} → skill({})", tc.function.name, tc.function.name);
                    tc.function.name = "skill".to_string();
                    tc.function.arguments = redirected.to_string();
                }
                tc
            })
            .collect();

        // Record for cross-round dedup.
        for tc in &selected_calls {
            executed_calls.insert((tc.function.name.clone(), tc.function.arguments.clone()));
        }

        // Track where new tool results start (for condensation).
        let tool_results_start = messages.len();

        match execution_mode {
            config::ToolExecutionMode::Sequential => {
                execute_tool_calls_sequential(
                    &mut selected_calls,
                    &tool_defs,
                    &allowed_tools,
                    &mut messages,
                    &mut on_tool_event,
                    &cancelled_set,
                    &state.scripts_dir,
                )
                .await;
            }
            config::ToolExecutionMode::Parallel => {
                execute_tool_calls_parallel(
                    &mut selected_calls,
                    &tool_defs,
                    &allowed_tools,
                    &mut messages,
                    &mut on_tool_event,
                    &cancelled_set,
                    &state.scripts_dir,
                )
                .await;
            }
        }

        // Capture the last successful tool result for the fallback.
        for msg in messages[tool_results_start..].iter().rev() {
            if msg.get("role").and_then(|r| r.as_str()) == Some("tool") {
                if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                    if let Ok(v) = serde_json::from_str::<Value>(content) {
                        if v.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
                            had_successful_tool_result = true;
                            last_tool_result = Some(content.to_string());
                            break;
                        }
                    }
                    // Keep last output even if not parseable/successful for fallback/debug.
                    if last_tool_result.is_none() {
                        last_tool_result = Some(content.to_string());
                    }
                }
            }
        }

        // ── Condense prior tool results ─────────────────────────────
        // The model already read old tool results and made its decision.
        // Replace them with one-line summaries to free context for the
        // next round.  Only the results from THIS round (tool_results_start..)
        // are kept full.
        if compression.should_compress_old_results() {
            condense_prior_tool_results(&mut messages, tool_results_start);
        }

        // Emit per-round usage so consumers can track cumulative costs.
        on_tool_event(ToolEvent::RoundComplete {
            round: round_idx + 1,
            prompt_tokens: cumulative_prompt_tokens,
            completion_tokens: cumulative_completion_tokens,
            tool_calls_count: selected_calls.len(),
        });
    }

    // If we exhausted all rounds but have a tool result, return it as a
    // fallback so the user at least sees the raw data.
    if let Some(result) = last_tool_result {
        log::warn!("[tool-orchestration] round limit reached but have tool result — returning raw");
        let fallback = format!(
            "*(The model could not summarize the tool output. Here is the raw result:)*\n\n```json\n{}\n```",
            result
        );
        return Ok((fallback, "stop".into(), 0, 0, n_ctx));
    }

    Err(anyhow::anyhow!("tool-calling round limit reached ({max_rounds} rounds). You can increase this in Settings → LLM → Tools → Max rounds."))
}

// ── Prior-round condensation ──────────────────────────────────────────────────

/// Replace tool-result messages from previous rounds with one-line summaries.
///
/// `current_round_start` is the index where the current round's tool results
/// begin — everything before that is a prior round and gets condensed.
///
/// This is the key to keeping multi-step tool chains working on small context
/// windows: the model already consumed the old results and chose its next
/// action, so we only need a brief reminder of what happened.
fn condense_prior_tool_results(messages: &mut [Value], current_round_start: usize) {
    for (i, msg) in messages.iter_mut().enumerate() {
        if i >= current_round_start {
            break;
        }

        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if role != "tool" {
            continue;
        }

        let content = match msg.get("content").and_then(|c| c.as_str()) {
            Some(c) => c.to_string(),
            None => continue,
        };

        // Already condensed (< 200 chars) — skip.
        if content.len() < 200 {
            continue;
        }

        let summary = summarize_tool_result(&content);
        msg["content"] = Value::String(summary);
    }
}

/// Extract a one-line summary from a tool result JSON string.
fn summarize_tool_result(content: &str) -> String {
    // Try to parse as JSON for structured extraction.
    let v: Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => {
            // Already a plain-text compact result — truncate hard.
            let first_line = content.lines().next().unwrap_or(content);
            return if first_line.len() > 120 {
                format!("{}…", &first_line[..120])
            } else {
                first_line.to_string()
            };
        }
    };

    let tool = v.get("tool").and_then(|t| t.as_str()).unwrap_or("unknown");
    let ok = v.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false);

    if !ok {
        let err = v.get("error").and_then(|e| e.as_str()).unwrap_or("failed");
        return format!("[{tool}: error — {err}]");
    }

    match tool {
        "location" => {
            let city = v.get("city").and_then(|c| c.as_str()).unwrap_or("?");
            let region = v.get("region").and_then(|c| c.as_str()).unwrap_or("");
            let country = v.get("country").and_then(|c| c.as_str()).unwrap_or("");
            let tz = v.get("timezone").and_then(|c| c.as_str()).unwrap_or("");
            format!("[location: {city}, {region}, {country} ({tz})]")
        }
        "date" => {
            let iso = v.get("iso_local").and_then(|c| c.as_str()).unwrap_or("?");
            format!("[date: {iso}]")
        }
        "web_search" => {
            let query = v.get("query").and_then(|q| q.as_str()).unwrap_or("?");
            // Handle compact text format.
            if let Some(compact) = v.get("compact").and_then(|c| c.as_str()) {
                let n = compact
                    .lines()
                    .filter(|l| l.starts_with(|c: char| c.is_ascii_digit()))
                    .count();
                return format!("[web_search: {n} results for \"{query}\"]");
            }
            let n = v
                .get("results")
                .and_then(|r| r.as_array())
                .map(std::vec::Vec::len)
                .unwrap_or(0);
            format!("[web_search: {n} results for \"{query}\"]")
        }
        "web_fetch" => {
            let url = v.get("url").and_then(|u| u.as_str()).unwrap_or("?");
            let chars = v.get("content").and_then(|c| c.as_str()).map(str::len).unwrap_or(0);
            let short_url = if url.len() > 60 { &url[..60] } else { url };
            format!("[web_fetch: {short_url}… ({chars} chars)]")
        }
        "bash" => {
            let cmd = v.get("command").and_then(|c| c.as_str()).unwrap_or("?");
            let exit = v.get("exit_code").and_then(serde_json::Value::as_i64).unwrap_or(-1);
            let short_cmd = if cmd.len() > 60 { &cmd[..60] } else { cmd };
            format!("[bash: `{short_cmd}` exit={exit}]")
        }
        "read_file" => {
            let lines = v.get("total_lines").and_then(serde_json::Value::as_u64).unwrap_or(0);
            format!("[read_file: {lines} lines]")
        }
        "skill" => {
            let cmd = v.get("command").and_then(|c| c.as_str()).unwrap_or("?");
            format!("[skill: {cmd} — ok]")
        }
        _ => {
            format!("[{tool}: ok]")
        }
    }
}

// ── Validation ────────────────────────────────────────────────────────────────

/// Validate arguments for a tool call.  Returns the parsed args `Value` or an
/// error result to inject directly.
///
/// If the LLM called a Skill API sub-command (e.g. `status`) as a top-level
/// tool, the call is silently rewritten to `skill` with `{"command":"status"}`
/// so it goes through the normal execution path.
fn validate_and_prepare(
    tc: &mut tools::ToolCall,
    tool_defs: &std::collections::HashMap<String, tools::Tool>,
    allowed_tools: &config::LlmToolConfig,
) -> Result<Value, Value> {
    // Auto-redirect: Skill API sub-command or neuroskill alias used as tool.
    if !skill_tools::defs::is_known_builtin_tool(&tc.function.name) {
        if let Some(cmd) = skill_tools::defs::resolve_skill_alias(&tc.function.name) {
            let orig_args: Value = serde_json::from_str(&tc.function.arguments).unwrap_or_else(|_| json!({}));
            let mut redirected = json!({ "command": cmd });
            if let Some(obj) = orig_args.as_object() {
                if !obj.is_empty() {
                    redirected["args"] = orig_args;
                }
            }
            log::info!("[tool-redirect] {} → skill({})", tc.function.name, cmd);
            tc.function.name = "skill".to_string();
            tc.function.arguments = redirected.to_string();
        }
    }

    if !skill_tools::defs::is_known_builtin_tool(&tc.function.name) {
        return Err(
            json!({ "ok": false, "tool": tc.function.name, "error": format!("unsupported tool \"{}\". Use one of the available tools listed in the system prompt.", tc.function.name) }),
        );
    }
    if !is_builtin_tool_enabled(allowed_tools, &tc.function.name) {
        return Err(json!({ "ok": false, "tool": tc.function.name, "error": "tool disabled in settings" }));
    }

    let args: Value = serde_json::from_str(&tc.function.arguments).unwrap_or_else(|_| json!({}));

    if let Some(tool_def) = tool_defs.get(&tc.function.name) {
        match tools::validate_tool_arguments(tool_def, &args) {
            Ok(validated) => Ok(validated),
            Err(err_msg) => Err(json!({ "ok": false, "tool": tc.function.name, "error": err_msg.to_string() })),
        }
    } else {
        Ok(args)
    }
}

// ── Sequential execution ──────────────────────────────────────────────────────

async fn execute_tool_calls_sequential<G>(
    calls: &mut [tools::ToolCall],
    tool_defs: &std::collections::HashMap<String, tools::Tool>,
    allowed_tools: &config::LlmToolConfig,
    messages: &mut Vec<Value>,
    on_tool_event: &mut G,
    cancelled_set: &Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    scripts_dir: &std::path::Path,
) where
    G: FnMut(ToolEvent),
{
    for tc in calls.iter_mut() {
        // Check if cancelled before execution.
        if cancelled_set.lock_or_recover().contains(&tc.id) {
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

        // Write coerced/validated arguments back so exec sees them.
        if let Ok(ref validated) = args_result {
            if let Ok(s) = serde_json::to_string(validated) {
                tc.function.arguments = s;
            }
        }

        let args_for_event = match &args_result {
            Ok(v) => v.clone(),
            Err(_) => serde_json::from_str(&tc.function.arguments).unwrap_or(json!({})),
        };

        // Emit start events.
        let detail_str = if tc.function.arguments.len() > 2 {
            Some(tc.function.arguments.clone())
        } else {
            None
        };
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
        let (tool_result, is_error) = if cancelled_set.lock_or_recover().contains(&tc.id) {
            (
                json!({ "ok": false, "tool": tc.function.name, "error": "cancelled by user" }),
                true,
            )
        } else {
            match args_result {
                Err(err_val) => (err_val, true),
                Ok(_) => {
                    let result = execute_builtin_tool_call(tc, allowed_tools, scripts_dir).await;
                    let ok = result.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false);
                    (result, !ok)
                }
            }
        };

        // Emit end events.
        on_tool_event(ToolEvent::Status {
            tool_name: tc.function.name.clone(),
            status: if is_error { "error" } else { "done" }.into(),
            detail: if is_error {
                tool_result
                    .get("error")
                    .and_then(|v| v.as_str())
                    .map(std::string::ToString::to_string)
            } else {
                None
            },
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
    calls: &mut [tools::ToolCall],
    tool_defs: &std::collections::HashMap<String, tools::Tool>,
    allowed_tools: &config::LlmToolConfig,
    messages: &mut Vec<Value>,
    on_tool_event: &mut G,
    cancelled_set: &Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    scripts_dir: &std::path::Path,
) where
    G: FnMut(ToolEvent),
{
    // Phase 1: Prepare all calls.
    struct PreparedCall {
        tc: tools::ToolCall,
        validation: Result<Value, Value>,
    }

    let mut prepared = Vec::with_capacity(calls.len());
    for tc in calls.iter_mut() {
        if cancelled_set.lock_or_recover().contains(&tc.id) {
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

        // Write coerced/validated arguments back so exec sees them.
        if let Ok(ref validated) = args_result {
            if let Ok(s) = serde_json::to_string(validated) {
                tc.function.arguments = s;
            }
        }

        let args_for_event = match &args_result {
            Ok(v) => v.clone(),
            Err(_) => serde_json::from_str(&tc.function.arguments).unwrap_or(json!({})),
        };

        let detail_str = if tc.function.arguments.len() > 2 {
            Some(tc.function.arguments.clone())
        } else {
            None
        };
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

        prepared.push(PreparedCall {
            tc: tc.clone(),
            validation: args_result,
        });
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
                if cancel_check.lock_or_recover().contains(&tc.id) {
                    return (
                        tc.clone(),
                        json!({ "ok": false, "tool": tc.function.name, "error": "cancelled by user" }),
                        true,
                    );
                }
                let result = execute_builtin_tool_call(&tc, &allowed, &sdir).await;
                let ok = result.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false);
                (tc, result, !ok)
            }));
        } else {
            let err_val = p.validation.as_ref().expect_err("guarded by else branch").clone();
            futures.push(tokio::spawn(async move { (tc, err_val, true) }));
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
            detail: if is_error {
                tool_result
                    .get("error")
                    .and_then(|v| v.as_str())
                    .map(std::string::ToString::to_string)
            } else {
                None
            },
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sanitizer_passes_plain_text() {
        let mut s = ToolCallStreamSanitizer::new();
        assert_eq!(s.push("Hello"), "Hello");
        assert_eq!(s.push(" world"), " world");
    }

    #[test]
    fn sanitizer_strips_tool_call_blocks() {
        let mut s = ToolCallStreamSanitizer::new();
        assert_eq!(s.push("Before "), "Before ");
        let _ = s.push("[TOOL_CALL]");
        let _ = s.push(r#"{"name":"date"}"#);
        let _ = s.push("[/TOOL_CALL]");
        let after = s.push(" After");
        assert!(after.contains("After"));
    }

    #[test]
    fn sanitizer_incremental() {
        let mut s = ToolCallStreamSanitizer::new();
        let mut full = String::new();
        for piece in &["He", "llo", " wo", "rld"] {
            full.push_str(&s.push(piece));
        }
        assert_eq!(full, "Hello world");
    }

    #[test]
    fn sanitizer_empty() {
        let mut s = ToolCallStreamSanitizer::new();
        assert_eq!(s.push(""), "");
    }

    #[test]
    fn summarize_date() {
        let r = json!({"ok":true,"tool":"date","iso_local":"2026-03-28T12:00:00-07:00"});
        let s = summarize_tool_result(&r.to_string());
        assert!(s.contains("date") && s.contains("2026-03-28"));
    }

    #[test]
    fn summarize_bash() {
        let r = json!({"ok":true,"tool":"bash","command":"ls -la","exit_code":0});
        let s = summarize_tool_result(&r.to_string());
        assert!(s.contains("bash") && s.contains("ls -la") && s.contains("exit=0"));
    }

    #[test]
    fn summarize_web_search() {
        let r = json!({"ok":true,"tool":"web_search","query":"rust","results":[{"t":"a"},{"t":"b"}]});
        assert!(summarize_tool_result(&r.to_string()).contains("2 results"));
    }

    #[test]
    fn summarize_error() {
        let r = json!({"ok":false,"tool":"bash","error":"not found"});
        assert!(summarize_tool_result(&r.to_string()).contains("not found"));
    }

    #[test]
    fn summarize_skill() {
        let r = json!({"ok":true,"tool":"skill","command":"status"});
        assert!(summarize_tool_result(&r.to_string()).contains("status"));
    }

    #[test]
    fn summarize_unknown() {
        let r = json!({"ok":true,"tool":"custom"});
        assert!(summarize_tool_result(&r.to_string()).contains("custom"));
    }

    #[test]
    fn summarize_long_text_truncates() {
        assert!(summarize_tool_result(&"x".repeat(500)).len() < 200);
    }

    #[test]
    fn summarize_read_file() {
        let r = json!({"ok":true,"tool":"read_file","total_lines":42});
        assert!(summarize_tool_result(&r.to_string()).contains("42"));
    }

    #[test]
    fn summarize_web_fetch() {
        let r = json!({"ok":true,"tool":"web_fetch","url":"https://example.com","content":"hi"});
        assert!(summarize_tool_result(&r.to_string()).contains("example.com"));
    }

    #[test]
    fn summarize_location() {
        let r = json!({"ok":true,"tool":"location","city":"SF","region":"CA","country":"US","timezone":"PST"});
        assert!(summarize_tool_result(&r.to_string()).contains("SF"));
    }

    #[test]
    fn condense_skips_current_round() {
        let long = "x".repeat(500);
        let mut msgs = vec![
            json!({"role":"user","content":"hi"}),
            json!({"role":"tool","tool_call_id":"old","content":long}),
            json!({"role":"tool","tool_call_id":"new","content":long}),
        ];
        condense_prior_tool_results(&mut msgs, 2);
        assert!(msgs[1]["content"].as_str().unwrap().len() < 200);
        assert_eq!(msgs[2]["content"].as_str().unwrap().len(), 500);
    }

    #[test]
    fn condense_skips_short() {
        let mut msgs = vec![json!({"role":"tool","tool_call_id":"t","content":"ok"})];
        condense_prior_tool_results(&mut msgs, 1);
        assert_eq!(msgs[0]["content"].as_str().unwrap(), "ok");
    }

    #[test]
    fn condense_skips_non_tool() {
        let long = "x".repeat(500);
        let mut msgs = vec![
            json!({"role":"user","content":long}),
            json!({"role":"assistant","content":long}),
        ];
        condense_prior_tool_results(&mut msgs, 2);
        assert_eq!(msgs[0]["content"].as_str().unwrap().len(), 500);
    }

    // ── message_text_content ─────────────────────────────────────────────

    #[test]
    fn message_text_content_plain_string() {
        let msg = json!({"role": "user", "content": "hello"});
        assert_eq!(message_text_content(&msg), "hello");
    }

    #[test]
    fn message_text_content_multipart() {
        let msg = json!({"role": "user", "content": [
            {"type": "text", "text": "first"},
            {"type": "image_url", "image_url": {"url": "data:..."}},
            {"type": "text", "text": "second"},
        ]});
        assert_eq!(message_text_content(&msg), "first\nsecond");
    }

    #[test]
    fn message_text_content_no_content() {
        let msg = json!({"role": "assistant"});
        assert_eq!(message_text_content(&msg), "");
    }

    #[test]
    fn message_text_content_null_content() {
        let msg = json!({"role": "assistant", "content": null});
        assert_eq!(message_text_content(&msg), "");
    }

    // ── latest_user_prompt ───────────────────────────────────────────────

    #[test]
    fn latest_user_prompt_finds_last_user() {
        let msgs = vec![
            json!({"role": "user", "content": "first"}),
            json!({"role": "assistant", "content": "reply"}),
            json!({"role": "user", "content": "second"}),
        ];
        assert_eq!(latest_user_prompt(&msgs), Some("second".into()));
    }

    #[test]
    fn latest_user_prompt_none_when_no_users() {
        let msgs = vec![
            json!({"role": "system", "content": "setup"}),
            json!({"role": "assistant", "content": "hi"}),
        ];
        assert_eq!(latest_user_prompt(&msgs), None);
    }

    #[test]
    fn latest_user_prompt_skips_whitespace_only() {
        let msgs = vec![
            json!({"role": "user", "content": "real question"}),
            json!({"role": "user", "content": "   "}),
        ];
        assert_eq!(latest_user_prompt(&msgs), Some("real question".into()));
    }

    #[test]
    fn latest_user_prompt_empty_messages() {
        let msgs: Vec<Value> = vec![];
        assert_eq!(latest_user_prompt(&msgs), None);
    }
}
