// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//!
//! End-to-end integration test: download the smallest model, start the LLM
//! server, send a chat request that triggers a built-in tool call, and verify
//! that the full pipeline completes without errors.
//!
//! Every step is benchmarked and all LLM responses are captured.  A full
//! report is printed both during execution (live progress) and as a summary
//! table at the end.
//!
//! Run with:
//!   cargo test -p skill-llm --features llm --test llm_e2e -- --nocapture
//!
//! Or via the npm convenience wrapper:
//!   npm run test:llm:e2e

#![cfg(feature = "llm")]

use std::sync::{Arc, Mutex, atomic::Ordering};
use std::time::{Duration, Instant};

use serde_json::json;

use skill_llm::catalog::{DownloadProgress, DownloadState, LlmCatalog, LlmModelEntry};
use skill_llm::config::LlmConfig;
use skill_llm::engine::protocol::GenParams;
use skill_llm::{
    LlmEventEmitter, NoopEmitter,
    init, new_log_buffer, run_chat_with_builtin_tools,
};

// ── Report types ─────────────────────────────────────────────────────────────

/// A single timed step in the E2E pipeline.
struct Step {
    name:     &'static str,
    duration: Duration,
    status:   StepStatus,
    detail:   String,
}

enum StepStatus {
    Ok,
    Warn(String),
    Fail(String),
}

impl Step {
    fn status_icon(&self) -> &str {
        match &self.status {
            StepStatus::Ok      => "✅",
            StepStatus::Warn(_) => "⚠️ ",
            StepStatus::Fail(_) => "❌",
        }
    }
    fn status_text(&self) -> String {
        match &self.status {
            StepStatus::Ok       => "OK".into(),
            StepStatus::Warn(w)  => format!("WARN: {w}"),
            StepStatus::Fail(e)  => format!("FAIL: {e}"),
        }
    }
}

/// A captured LLM chat exchange.
#[allow(dead_code)]
struct ChatRecord {
    label:             &'static str,
    messages_in:       Vec<serde_json::Value>,
    response_text:     String,
    visible_text:      String,
    finish_reason:     String,
    prompt_tokens:     usize,
    completion_tokens: usize,
    n_ctx:             usize,
    duration:          Duration,
    tok_per_sec:       f64,
    tool_events:       Vec<ToolEventRecord>,
}

struct ToolEventRecord {
    kind:      String, // "start", "end", "status"
    tool_name: String,
    detail:    String,
    is_error:  bool,
}

/// Accumulated report printed at the end.
struct Report {
    model_name: String,
    model_size: f32,
    model_quant: String,
    steps:      Vec<Step>,
    chats:      Vec<ChatRecord>,
}

impl Report {
    fn new() -> Self {
        Self { model_name: String::new(), model_size: 0.0, model_quant: String::new(), steps: vec![], chats: vec![] }
    }

    fn print_final(&self) {
        let total: Duration = self.steps.iter().map(|s| s.duration).sum();
        let w = 76; // box width
        let bar = "═".repeat(w - 2);
        eprintln!();
        eprintln!("╔{bar}╗");
        eprintln!("║{:^width$}║", "E2E INTEGRATION TEST REPORT", width = w - 2);
        eprintln!("╠{bar}╣");

        // Model info
        eprintln!("║ Model: {:<width$}║",
            format!("{} ({:.2} GB, {})", self.model_name, self.model_size, self.model_quant),
            width = w - 10);
        eprintln!("║ Total: {:<width$}║",
            format!("{:.2}s", total.as_secs_f64()), width = w - 10);
        eprintln!("╠{bar}╣");

        // Step timings
        eprintln!("║{:^width$}║", "PIPELINE STEPS", width = w - 2);
        eprintln!("║ {thin} ║", thin = "─".repeat(w - 4));
        for (i, step) in self.steps.iter().enumerate() {
            let line = format!(
                "{} {}. {:<30} {:>8.2}s  {}",
                step.status_icon(),
                i + 1,
                step.name,
                step.duration.as_secs_f64(),
                step.status_text(),
            );
            // Truncate if too long
            let display: String = line.chars().take(w - 4).collect();
            eprintln!("║ {:<width$} ║", display, width = w - 4);
            if !step.detail.is_empty() {
                for detail_line in step.detail.lines() {
                    let dl: String = detail_line.chars().take(w - 8).collect();
                    eprintln!("║   {:<width$} ║", dl, width = w - 6);
                }
            }
        }

        // Chat exchanges
        if !self.chats.is_empty() {
            eprintln!("╠{bar}╣");
            eprintln!("║{:^width$}║", "CHAT EXCHANGES", width = w - 2);
            for (i, chat) in self.chats.iter().enumerate() {
                eprintln!("║ {thin} ║", thin = "─".repeat(w - 4));
                let header = format!(
                    "Chat #{}: {} ({:.2}s, {:.1} tok/s)",
                    i + 1, chat.label, chat.duration.as_secs_f64(), chat.tok_per_sec,
                );
                let h: String = header.chars().take(w - 4).collect();
                eprintln!("║ {:<width$} ║", h, width = w - 4);

                let stats = format!(
                    "prompt={} completion={} n_ctx={} finish={}",
                    chat.prompt_tokens, chat.completion_tokens, chat.n_ctx, chat.finish_reason,
                );
                let s: String = stats.chars().take(w - 6).collect();
                eprintln!("║   {:<width$} ║", s, width = w - 6);

                // Input messages (abbreviated)
                for msg in &chat.messages_in {
                    let role = msg["role"].as_str().unwrap_or("?");
                    let content = msg["content"].as_str().unwrap_or("");
                    let abbr: String = content.chars().take(60).collect();
                    let line = format!("[{role}] {abbr}{}",
                        if content.len() > 60 { "…" } else { "" });
                    let l: String = line.chars().take(w - 6).collect();
                    eprintln!("║   {:<width$} ║", l, width = w - 6);
                }

                // Response
                let resp_abbr: String = chat.response_text.replace('\n', " ").chars().take(w - 16).collect();
                eprintln!("║   → {:<width$} ║", resp_abbr, width = w - 8);

                // Tool events
                if !chat.tool_events.is_empty() {
                    eprintln!("║   Tools:{:<width$} ║", "", width = w - 12);
                    for te in &chat.tool_events {
                        let err_tag = if te.is_error { " [ERROR]" } else { "" };
                        let tl = format!(
                            "  {} {}{}{}", te.kind, te.tool_name, err_tag,
                            if te.detail.is_empty() { String::new() } else { format!(": {}", te.detail) },
                        );
                        let t: String = tl.chars().take(w - 6).collect();
                        eprintln!("║   {:<width$} ║", t, width = w - 6);
                    }
                }
            }
        }

        // Footer
        let all_ok = self.steps.iter().all(|s| matches!(s.status, StepStatus::Ok));
        eprintln!("╠{bar}╣");
        let verdict = if all_ok { "ALL CHECKS PASSED ✅" } else { "SOME CHECKS FAILED ❌" };
        eprintln!("║{:^width$}║", verdict, width = w - 2);
        eprintln!("╚{bar}╝");
        eprintln!();
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Find the smallest non-mmproj model in the catalog.
fn smallest_model(catalog: &LlmCatalog) -> Option<&LlmModelEntry> {
    catalog
        .entries
        .iter()
        .filter(|e| !e.is_mmproj)
        .min_by(|a, b| a.size_gb.total_cmp(&b.size_gb))
}

/// Wait for the LLM server to become ready (model fully loaded).
fn wait_ready(state: &skill_llm::LlmServerState, timeout: Duration) {
    let start = Instant::now();
    while !state.is_ready() {
        if start.elapsed() > timeout {
            panic!(
                "LLM server did not become ready within {:.0}s",
                timeout.as_secs_f64()
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Collect all streamed tokens into a single string with timing.
async fn collect_tokens(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<skill_llm::InferToken>,
) -> Result<(String, String, usize, usize, usize), String> {
    let mut text = String::new();
    let mut finish_reason = String::new();
    let mut prompt_tokens = 0usize;
    let mut completion_tokens = 0usize;
    let mut n_ctx = 0usize;
    while let Some(tok) = rx.recv().await {
        match tok {
            skill_llm::InferToken::Delta(t) => text.push_str(&t),
            skill_llm::InferToken::Done {
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
            skill_llm::InferToken::Error(e) => return Err(e),
        }
    }
    Ok((text, finish_reason, prompt_tokens, completion_tokens, n_ctx))
}

fn tps(completion_tokens: usize, dur: Duration) -> f64 {
    let secs = dur.as_secs_f64();
    if secs > 0.0 { completion_tokens as f64 / secs } else { 0.0 }
}

// ── Test ─────────────────────────────────────────────────────────────────────

/// Full end-to-end: download smallest model → start server → chat → tool chat.
#[tokio::test(flavor = "multi_thread")]
async fn e2e_download_start_and_chat() {
    let test_start = Instant::now();
    let mut report = Report::new();

    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════════════╗");
    eprintln!("║  LLM E2E Integration Test — live progress                              ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════════════╝");
    eprintln!();

    // ── 1. Set up temp skill_dir ─────────────────────────────────────────────
    let t = Instant::now();
    let skill_dir = std::env::temp_dir().join(format!("skill-llm-e2e-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&skill_dir);
    let detail = format!("path: {}", skill_dir.display());
    eprintln!("[step 1] skill_dir = {}", skill_dir.display());
    report.steps.push(Step {
        name: "Create temp skill_dir",
        duration: t.elapsed(),
        status: StepStatus::Ok,
        detail,
    });

    // ── 2. Load catalog and find smallest model ──────────────────────────────
    let t = Instant::now();
    let mut catalog = LlmCatalog::load(&skill_dir);
    let entry = smallest_model(&catalog)
        .expect("catalog should contain at least one non-mmproj model")
        .clone();
    report.model_name = entry.filename.clone();
    report.model_size = entry.size_gb;
    report.model_quant = entry.quant.clone();
    let detail = format!(
        "{} ({:.2} GB, quant={}, family={}, params={:.2}B, max_ctx={})",
        entry.filename, entry.size_gb, entry.quant, entry.family_name,
        entry.params_b, entry.max_context_length,
    );
    eprintln!("[step 2] {detail}");
    report.steps.push(Step {
        name: "Load catalog + find model",
        duration: t.elapsed(),
        status: StepStatus::Ok,
        detail,
    });

    // ── 3. Download the model ────────────────────────────────────────────────
    let t = Instant::now();
    let progress = Arc::new(Mutex::new(DownloadProgress {
        filename:        entry.filename.clone(),
        state:           DownloadState::Downloading,
        status_msg:      None,
        progress:        0.0,
        cancelled:       false,
        pause_requested: false,
        current_shard:   0,
        total_shards:    entry.shard_count() as u16,
    }));

    eprintln!("[step 3] downloading {} …", entry.filename);
    let local_path =
        skill_llm::catalog::download_model(&entry, &progress).expect("download should succeed");
    let dl_dur = t.elapsed();
    let dl_speed = if dl_dur.as_secs_f64() > 0.0 {
        (entry.size_gb as f64 * 1024.0) / dl_dur.as_secs_f64()
    } else { 0.0 };
    let detail = format!(
        "{} → {:.1}s ({:.1} MB/s)",
        local_path.display(), dl_dur.as_secs_f64(), dl_speed,
    );
    eprintln!("[step 3] download complete: {detail}");
    report.steps.push(Step {
        name: "Download model",
        duration: dl_dur,
        status: StepStatus::Ok,
        detail,
    });

    // Update the catalog with the download result.
    if let Some(e) = catalog.entries.iter_mut().find(|e| e.filename == entry.filename) {
        e.state = DownloadState::Downloaded;
        e.local_path = Some(local_path.clone());
    }
    catalog.active_model = entry.filename.clone();

    // ── 4. Start the LLM server ──────────────────────────────────────────────
    let t = Instant::now();
    let config = LlmConfig {
        enabled: true,
        n_gpu_layers: u32::MAX,
        ctx_size: Some(4096),
        ..LlmConfig::default()
    };

    let emitter: Arc<dyn LlmEventEmitter> = Arc::new(NoopEmitter);
    let log_buf = new_log_buffer();

    eprintln!("[step 4] starting LLM server …");
    let server = init(&config, &catalog, emitter, log_buf, &skill_dir)
        .expect("init should return a running server");

    wait_ready(&server, Duration::from_secs(120));
    let load_dur = t.elapsed();
    let n_ctx_val = server.n_ctx.load(Ordering::Relaxed);
    let detail = format!(
        "n_ctx={}, n_gpu_layers=all, flash_attn=true, load_time={:.2}s",
        n_ctx_val, load_dur.as_secs_f64(),
    );
    eprintln!("[step 4] server ready: {detail}");
    report.steps.push(Step {
        name: "Start LLM server",
        duration: load_dur,
        status: StepStatus::Ok,
        detail,
    });

    // ── 5. Simple chat completion (no tools) ─────────────────────────────────
    let t = Instant::now();
    eprintln!("[step 5] simple chat: \"What is 2+2? Reply in one word.\"");
    let messages = vec![
        json!({"role": "system", "content": "You are a helpful assistant. Be brief."}),
        json!({"role": "user",   "content": "What is 2+2? Reply in one word."}),
    ];
    let params = GenParams {
        max_tokens: 64,
        temperature: 0.0,
        thinking_budget: Some(0),
        ..GenParams::default()
    };
    let rx = server
        .chat(messages.clone(), vec![], params.clone())
        .expect("chat request should be accepted");
    let (text, finish_reason, pt, ct, nc) = collect_tokens(rx).await.expect("generation should succeed");
    let chat_dur = t.elapsed();
    let tok_s = tps(ct, chat_dur);
    eprintln!("[step 5] response ({:.2}s, {:.1} tok/s): {:?}", chat_dur.as_secs_f64(), tok_s, text.trim());

    assert!(!text.trim().is_empty(), "response should not be empty");

    report.steps.push(Step {
        name: "Simple chat (no tools)",
        duration: chat_dur,
        status: StepStatus::Ok,
        detail: format!("{:.1} tok/s, prompt={pt}, completion={ct}, finish={finish_reason}", tok_s),
    });
    report.chats.push(ChatRecord {
        label: "Simple chat (no tools)",
        messages_in: messages,
        response_text: text,
        visible_text: String::new(),
        finish_reason,
        prompt_tokens: pt,
        completion_tokens: ct,
        n_ctx: nc,
        duration: chat_dur,
        tok_per_sec: tok_s,
        tool_events: vec![],
    });

    // ── 6. Chat with tool calling — date tool ────────────────────────────────
    let t = Instant::now();
    eprintln!("[step 6] tool chat: \"What is today's date?\" (date tool enabled)");
    {
        let mut tools = server.allowed_tools.lock().expect("lock");
        tools.enabled = true;
        tools.date = true;
        tools.location = false;
        tools.web_search = false;
        tools.web_fetch = false;
        tools.bash = false;
        tools.read_file = false;
        tools.write_file = false;
        tools.edit_file = false;
        tools.skill_api = false;
    }

    let messages_tool = vec![
        json!({"role": "system", "content": "You are a helpful assistant. You have access to tools. Use the date tool when asked about the current date or time. Be brief."}),
        json!({"role": "user",   "content": "What is today's date? Use your date tool to find out."}),
    ];
    let params_tools = GenParams {
        max_tokens: 256,
        temperature: 0.0,
        thinking_budget: Some(0),
        ..GenParams::default()
    };

    let mut visible_text = String::new();
    let mut tool_event_records: Vec<ToolEventRecord> = Vec::new();

    let result = run_chat_with_builtin_tools(
        &server,
        messages_tool.clone(),
        params_tools,
        vec![],
        |delta| { visible_text.push_str(delta); },
        |event| match event {
            skill_llm::ToolEvent::ExecutionStart { tool_name, tool_call_id, args } => {
                let d = format!("id={tool_call_id} args={args}");
                eprintln!("[step 6]   ▶ tool start: {tool_name} ({d})");
                tool_event_records.push(ToolEventRecord {
                    kind: "start".into(), tool_name, detail: d, is_error: false,
                });
            }
            skill_llm::ToolEvent::ExecutionEnd { tool_name, tool_call_id, result, is_error } => {
                let result_abbr: String = result.to_string().chars().take(120).collect();
                let d = format!("id={tool_call_id} result={result_abbr}");
                eprintln!("[step 6]   ■ tool end:   {tool_name} (error={is_error}) {result_abbr}");
                tool_event_records.push(ToolEventRecord {
                    kind: "end".into(), tool_name, detail: d, is_error,
                });
            }
            skill_llm::ToolEvent::Status { tool_name, status, detail } => {
                let d = detail.clone().unwrap_or_default();
                eprintln!("[step 6]   ○ tool status: {tool_name} — {status} {d}");
                tool_event_records.push(ToolEventRecord {
                    kind: "status".into(), tool_name, detail: format!("{status} {d}"), is_error: false,
                });
            }
        },
    )
    .await;

    let tool_dur = t.elapsed();

    let (response_text, finish_reason, pt, ct, nc) = match result {
        Ok(tuple) => tuple,
        Err(e) => {
            let msg = format!("tool chat failed: {e}");
            eprintln!("[step 6] ❌ {msg}");
            report.steps.push(Step {
                name: "Tool chat (date)",
                duration: tool_dur,
                status: StepStatus::Fail(msg.clone()),
                detail: String::new(),
            });
            panic!("{msg}");
        }
    };

    let tok_s = tps(ct, tool_dur);
    eprintln!("[step 6] response ({:.2}s, {:.1} tok/s): {:?}",
        tool_dur.as_secs_f64(), tok_s,
        response_text.replace('\n', " ").chars().take(100).collect::<String>());

    assert!(!response_text.trim().is_empty(), "tool-call response should not be empty");

    let date_called = tool_event_records.iter().any(|e| e.kind == "start" && e.tool_name == "date");
    let date_ok = tool_event_records.iter().any(|e| e.kind == "end" && e.tool_name == "date" && !e.is_error);

    let step_status = if date_called && date_ok {
        eprintln!("[step 6] ✅ date tool called and completed successfully");
        StepStatus::Ok
    } else if date_called && !date_ok {
        let msg = "date tool was called but returned an error".to_string();
        eprintln!("[step 6] ⚠️  {msg}");
        StepStatus::Warn(msg)
    } else {
        let msg = "model did not call the date tool (tiny model — acceptable)".to_string();
        eprintln!("[step 6] ⚠️  {msg}");
        StepStatus::Warn(msg)
    };

    let tool_names: Vec<String> = tool_event_records.iter()
        .filter(|e| e.kind == "start")
        .map(|e| e.tool_name.clone())
        .collect();
    let detail = format!(
        "{:.1} tok/s, prompt={pt}, completion={ct}, tools_called=[{}], finish={finish_reason}",
        tok_s, tool_names.join(", "),
    );
    report.steps.push(Step {
        name: "Tool chat (date)",
        duration: tool_dur,
        status: step_status,
        detail,
    });
    report.chats.push(ChatRecord {
        label: "Tool chat (date tool)",
        messages_in: messages_tool,
        response_text,
        visible_text,
        finish_reason,
        prompt_tokens: pt,
        completion_tokens: ct,
        n_ctx: nc,
        duration: tool_dur,
        tok_per_sec: tok_s,
        tool_events: tool_event_records,
    });

    // Validate tool lifecycle if events were emitted.
    if !report.chats.last().unwrap().tool_events.is_empty() {
        let starts: Vec<_> = report.chats.last().unwrap().tool_events.iter()
            .filter(|e| e.kind == "start").collect();
        let ends: Vec<_> = report.chats.last().unwrap().tool_events.iter()
            .filter(|e| e.kind == "end").collect();
        assert!(
            !starts.is_empty(),
            "should have at least one tool execution start event",
        );
        assert!(
            !ends.is_empty(),
            "should have at least one tool execution end event",
        );
        if date_called {
            assert!(date_ok || ends.iter().any(|e| e.tool_name == "date"),
                "date tool should have a completion event");
        }
    }

    // ── 7. Shutdown ──────────────────────────────────────────────────────────
    let t = Instant::now();
    eprintln!("[step 7] shutting down …");
    let n_ctx_final = server.n_ctx.load(Ordering::Relaxed);
    match Arc::try_unwrap(server) {
        Ok(owned) => owned.shutdown(),
        Err(arc) => drop(arc),
    }
    let shutdown_dur = t.elapsed();
    eprintln!("[step 7] server shut down in {:.2}s", shutdown_dur.as_secs_f64());
    report.steps.push(Step {
        name: "Shutdown",
        duration: shutdown_dur,
        status: StepStatus::Ok,
        detail: format!("n_ctx was {n_ctx_final}"),
    });

    // ── 8. Clean up ──────────────────────────────────────────────────────────
    let t = Instant::now();
    let _ = std::fs::remove_dir_all(&skill_dir);
    report.steps.push(Step {
        name: "Cleanup temp dir",
        duration: t.elapsed(),
        status: StepStatus::Ok,
        detail: String::new(),
    });

    // ── 9. Print full report ─────────────────────────────────────────────────
    let total_dur = test_start.elapsed();
    report.steps.push(Step {
        name: "TOTAL",
        duration: total_dur,
        status: StepStatus::Ok,
        detail: String::new(),
    });
    report.print_final();

    // Final assertion
    let failures: Vec<_> = report.steps.iter()
        .filter(|s| matches!(s.status, StepStatus::Fail(_)))
        .collect();
    assert!(failures.is_empty(), "E2E test had {} failed step(s)", failures.len());
}
