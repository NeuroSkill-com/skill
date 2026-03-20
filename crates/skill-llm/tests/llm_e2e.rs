// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//!
//! End-to-end integration test: download a small model, start the LLM
//! server, send a chat request that triggers a built-in tool call, and verify
//! that the full pipeline completes correctly.
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

struct Step {
    name:     &'static str,
    duration: Duration,
    status:   StepStatus,
    detail:   String,
}

enum StepStatus { Ok, Warn(String), Fail(String) }

impl Step {
    fn icon(&self) -> &str {
        match &self.status {
            StepStatus::Ok      => "✅",
            StepStatus::Warn(_) => "⚠️ ",
            StepStatus::Fail(_) => "❌",
        }
    }
    fn text(&self) -> String {
        match &self.status {
            StepStatus::Ok       => "OK".into(),
            StepStatus::Warn(w)  => format!("WARN: {w}"),
            StepStatus::Fail(e)  => format!("FAIL: {e}"),
        }
    }
}

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
    tool_events:       Vec<ToolEvt>,
}

struct ToolEvt {
    kind:      String,
    tool_name: String,
    detail:    String,
    is_error:  bool,
}

struct Report {
    model_name:  String,
    model_size:  f32,
    model_quant: String,
    steps:       Vec<Step>,
    chats:       Vec<ChatRecord>,
}

impl Report {
    fn new() -> Self {
        Self {
            model_name: String::new(), model_size: 0.0, model_quant: String::new(),
            steps: vec![], chats: vec![],
        }
    }

    fn print_final(&self) {
        let total: Duration = self.steps.iter().map(|s| s.duration).sum();
        let w = 78usize;
        let bar = "═".repeat(w - 2);

        eprintln!();
        eprintln!("╔{bar}╗");
        eprintln!("║{:^width$}║", "E2E INTEGRATION TEST REPORT", width = w - 2);
        eprintln!("╠{bar}╣");
        self.print_padded(w, &format!(
            "Model: {} ({:.2} GB, {})", self.model_name, self.model_size, self.model_quant));
        self.print_padded(w, &format!("Total: {:.2}s", total.as_secs_f64()));
        eprintln!("╠{bar}╣");
        eprintln!("║{:^width$}║", "PIPELINE STEPS", width = w - 2);
        self.print_sep(w);

        for (i, step) in self.steps.iter().enumerate() {
            self.print_padded(w, &format!(
                "{} {}. {:<32} {:>8.2}s  {}",
                step.icon(), i + 1, step.name,
                step.duration.as_secs_f64(), step.text(),
            ));
            for line in step.detail.lines() {
                self.print_padded_indent(w, line);
            }
        }

        if !self.chats.is_empty() {
            eprintln!("╠{bar}╣");
            eprintln!("║{:^width$}║", "CHAT EXCHANGES", width = w - 2);
            for (i, chat) in self.chats.iter().enumerate() {
                self.print_sep(w);
                self.print_padded(w, &format!(
                    "Chat #{}: {} ({:.2}s, {:.1} tok/s)",
                    i + 1, chat.label, chat.duration.as_secs_f64(), chat.tok_per_sec,
                ));
                self.print_padded_indent(w, &format!(
                    "prompt={} completion={} n_ctx={} finish={}",
                    chat.prompt_tokens, chat.completion_tokens, chat.n_ctx, chat.finish_reason,
                ));
                for msg in &chat.messages_in {
                    let role = msg["role"].as_str().unwrap_or("?");
                    let content = msg["content"].as_str().unwrap_or("");
                    let abbr: String = content.chars().take(64).collect();
                    self.print_padded_indent(w, &format!(
                        "[{role}] {abbr}{}", if content.len() > 64 { "…" } else { "" }));
                }
                let resp: String = chat.response_text.replace('\n', " ⏎ ");
                let resp_abbr: String = resp.chars().take(w - 10).collect();
                self.print_padded(w, &format!("  → {resp_abbr}"));
                if !chat.tool_events.is_empty() {
                    self.print_padded_indent(w, "Tools:");
                    for te in &chat.tool_events {
                        let err = if te.is_error { " [ERR]" } else { "" };
                        self.print_padded_indent(w, &format!(
                            "  {} {}{}{}", te.kind, te.tool_name, err,
                            if te.detail.is_empty() { String::new() }
                            else { format!(": {}", &te.detail) },
                        ));
                    }
                }
            }
        }

        let all_ok = self.steps.iter().all(|s| matches!(s.status, StepStatus::Ok | StepStatus::Warn(_)));
        eprintln!("╠{bar}╣");
        let v = if all_ok { "ALL CHECKS PASSED ✅" } else { "SOME CHECKS FAILED ❌" };
        eprintln!("║{:^width$}║", v, width = w - 2);
        eprintln!("╚{bar}╝");
        eprintln!();
    }

    fn print_padded(&self, w: usize, text: &str) {
        let t: String = text.chars().take(w - 4).collect();
        eprintln!("║ {:<width$} ║", t, width = w - 4);
    }
    fn print_padded_indent(&self, w: usize, text: &str) {
        let t: String = text.chars().take(w - 6).collect();
        eprintln!("║   {:<width$} ║", t, width = w - 6);
    }
    fn print_sep(&self, w: usize) {
        eprintln!("║ {:<width$} ║", "─".repeat(w - 4), width = w - 4);
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Find the smallest model suitable for tool-calling tests.
///
/// Strategy: pick the smallest *recommended* model with params_b >= 1.5B.
/// These are large enough to follow tool-call instructions reliably.
/// Falls back to smallest recommended, then smallest overall.
fn best_test_model(catalog: &LlmCatalog) -> Option<&LlmModelEntry> {
    let non_mmproj: Vec<&LlmModelEntry> = catalog.entries.iter()
        .filter(|e| !e.is_mmproj)
        .collect();

    // Preferred: smallest recommended model with params >= 1.5B
    let mut capable: Vec<&&LlmModelEntry> = non_mmproj.iter()
        .filter(|e| e.recommended && e.params_b >= 1.5)
        .collect();
    capable.sort_by(|a, b| a.size_gb.total_cmp(&b.size_gb));
    if let Some(e) = capable.first() {
        return Some(e);
    }

    // Fallback: smallest recommended
    let mut recommended: Vec<&&LlmModelEntry> = non_mmproj.iter()
        .filter(|e| e.recommended)
        .collect();
    recommended.sort_by(|a, b| a.size_gb.total_cmp(&b.size_gb));
    if let Some(e) = recommended.first() {
        return Some(e);
    }

    // Last resort: smallest overall
    non_mmproj.iter()
        .min_by(|a, b| a.size_gb.total_cmp(&b.size_gb))
        .copied()
}

fn wait_ready(state: &skill_llm::LlmServerState, timeout: Duration) {
    let start = Instant::now();
    while !state.is_ready() {
        if start.elapsed() > timeout {
            panic!("LLM server did not become ready within {:.0}s", timeout.as_secs_f64());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

async fn collect_tokens(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<skill_llm::InferToken>,
) -> Result<(String, String, usize, usize, usize), String> {
    let mut text = String::new();
    let mut finish_reason = String::new();
    let mut pt = 0; let mut ct = 0; let mut nc = 0;
    while let Some(tok) = rx.recv().await {
        match tok {
            skill_llm::InferToken::Delta(t) => text.push_str(&t),
            skill_llm::InferToken::Done { finish_reason: fr, prompt_tokens, completion_tokens, n_ctx } => {
                finish_reason = fr; pt = prompt_tokens; ct = completion_tokens; nc = n_ctx;
                break;
            }
            skill_llm::InferToken::Error(e) => return Err(e),
        }
    }
    Ok((text, finish_reason, pt, ct, nc))
}

fn tok_per_sec(ct: usize, dur: Duration) -> f64 {
    let s = dur.as_secs_f64();
    if s > 0.0 { ct as f64 / s } else { 0.0 }
}

// ── Test ─────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn e2e_download_start_and_chat() {
    let test_start = Instant::now();
    let mut report = Report::new();

    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════════════════╗");
    eprintln!("║  LLM E2E Integration Test — live progress                                  ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════════════════╝");
    eprintln!();

    // ── 1. Create temp skill_dir ─────────────────────────────────────────────
    let t = Instant::now();
    let skill_dir = std::env::temp_dir().join(format!("skill-llm-e2e-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&skill_dir);
    eprintln!("[step 1] skill_dir = {}", skill_dir.display());
    report.steps.push(Step {
        name: "Create temp skill_dir", duration: t.elapsed(), status: StepStatus::Ok,
        detail: format!("path: {}", skill_dir.display()),
    });

    // ── 2. Load catalog and find test model ──────────────────────────────────
    let t = Instant::now();
    let mut catalog = LlmCatalog::load(&skill_dir);
    let entry = best_test_model(&catalog)
        .expect("catalog should contain at least one suitable model")
        .clone();
    report.model_name = entry.filename.clone();
    report.model_size = entry.size_gb;
    report.model_quant = entry.quant.clone();
    let detail = format!(
        "{} ({:.2} GB, quant={}, params={:.1}B, family={}, max_ctx={})",
        entry.filename, entry.size_gb, entry.quant, entry.params_b,
        entry.family_name, entry.max_context_length,
    );
    eprintln!("[step 2] selected: {detail}");
    report.steps.push(Step {
        name: "Load catalog + select model", duration: t.elapsed(), status: StepStatus::Ok,
        detail,
    });

    // ── 3. Download the model ────────────────────────────────────────────────
    let t = Instant::now();
    let progress = Arc::new(Mutex::new(DownloadProgress {
        filename: entry.filename.clone(), state: DownloadState::Downloading,
        status_msg: None, progress: 0.0, cancelled: false, pause_requested: false,
        current_shard: 0, total_shards: entry.shard_count() as u16,
    }));

    eprintln!("[step 3] downloading {} ({:.2} GB) …", entry.filename, entry.size_gb);
    let local_path = skill_llm::catalog::download_model(&entry, &progress)
        .expect("download should succeed");
    let dl_dur = t.elapsed();
    let dl_speed = if dl_dur.as_secs_f64() > 0.0 {
        (entry.size_gb as f64 * 1024.0) / dl_dur.as_secs_f64()
    } else { 0.0 };
    let detail = format!("{:.1}s ({:.1} MB/s) → {}", dl_dur.as_secs_f64(), dl_speed, local_path.display());
    eprintln!("[step 3] done: {detail}");
    report.steps.push(Step {
        name: "Download model", duration: dl_dur, status: StepStatus::Ok, detail,
    });

    if let Some(e) = catalog.entries.iter_mut().find(|e| e.filename == entry.filename) {
        e.state = DownloadState::Downloaded;
        e.local_path = Some(local_path.clone());
    }
    catalog.active_model = entry.filename.clone();

    // ── 4. Start LLM server ─────────────────────────────────────────────────
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
    let n_ctx = server.n_ctx.load(Ordering::Relaxed);
    let detail = format!("n_ctx={n_ctx}, load={:.2}s", load_dur.as_secs_f64());
    eprintln!("[step 4] ready: {detail}");
    report.steps.push(Step {
        name: "Start LLM server", duration: load_dur, status: StepStatus::Ok, detail,
    });

    // ── 5. Simple chat (no tools) ────────────────────────────────────────────
    let t = Instant::now();
    eprintln!("[step 5] simple chat: \"What is 2+2? Answer with just the number.\"");
    let msgs_simple = vec![
        json!({"role": "system", "content": "You are a helpful assistant. Answer concisely."}),
        json!({"role": "user",   "content": "What is 2+2? Answer with just the number."}),
    ];
    let params_simple = GenParams {
        max_tokens: 32,
        temperature: 0.0,
        thinking_budget: Some(0),
        ..GenParams::default()
    };
    let rx = server.chat(msgs_simple.clone(), vec![], params_simple).expect("accepted");
    let (text, fr, pt, ct, nc) = collect_tokens(rx).await.expect("generation ok");
    let dur = t.elapsed();
    let tps = tok_per_sec(ct, dur);
    eprintln!("[step 5] response ({:.2}s, {:.1} tok/s, finish={}): {:?}",
        dur.as_secs_f64(), tps, fr, text.trim());

    let simple_ok = !text.trim().is_empty();
    let simple_has_4 = text.contains('4');
    let status = if simple_ok && simple_has_4 {
        StepStatus::Ok
    } else if simple_ok {
        StepStatus::Warn(format!("response didn't contain '4': {:?}", text.trim()))
    } else {
        StepStatus::Fail("empty response".into())
    };
    report.steps.push(Step {
        name: "Simple chat (no tools)", duration: dur, status,
        detail: format!("{tps:.1} tok/s, prompt={pt}, completion={ct}, finish={fr}"),
    });
    report.chats.push(ChatRecord {
        label: "Simple chat", messages_in: msgs_simple,
        response_text: text.clone(), visible_text: String::new(),
        finish_reason: fr, prompt_tokens: pt, completion_tokens: ct,
        n_ctx: nc, duration: dur, tok_per_sec: tps, tool_events: vec![],
    });
    assert!(simple_ok, "simple chat response must not be empty");

    // ── 6. Tool chat — date tool ─────────────────────────────────────────────
    let t = Instant::now();
    eprintln!("[step 6] tool chat: asking for today's date (only date tool enabled)");
    {
        let mut tools = server.allowed_tools.lock().expect("lock");
        tools.enabled    = true;
        tools.date       = true;
        tools.location   = false;
        tools.web_search = false;
        tools.web_fetch  = false;
        tools.bash       = false;
        tools.read_file  = false;
        tools.write_file = false;
        tools.edit_file  = false;
        tools.skill_api  = false;
    }

    let msgs_tool = vec![
        json!({"role": "system", "content": "You are a helpful assistant with tool access. When asked about the current date or time, you MUST call the date tool. After receiving the tool result, state the date clearly. Be concise."}),
        json!({"role": "user",   "content": "What is today's date? Call the date tool to check."}),
    ];
    let params_tool = GenParams {
        max_tokens: 512,
        temperature: 0.0,
        thinking_budget: Some(0),
        ..GenParams::default()
    };

    let mut visible_text = String::new();
    let mut tool_evts: Vec<ToolEvt> = Vec::new();

    let result = run_chat_with_builtin_tools(
        &server,
        msgs_tool.clone(),
        params_tool,
        vec![],
        |delta| { visible_text.push_str(delta); },
        |event| match event {
            skill_llm::ToolEvent::ExecutionStart { tool_name, tool_call_id, args } => {
                let d = format!("id={tool_call_id} args={args}");
                eprintln!("[step 6]   ▶ {tool_name}: {d}");
                tool_evts.push(ToolEvt { kind: "start".into(), tool_name, detail: d, is_error: false });
            }
            skill_llm::ToolEvent::ExecutionEnd { tool_name, tool_call_id, result, is_error } => {
                let r: String = result.to_string().chars().take(120).collect();
                eprintln!("[step 6]   ■ {tool_name} (err={is_error}): {r}");
                tool_evts.push(ToolEvt {
                    kind: "end".into(), tool_name,
                    detail: format!("id={tool_call_id} result={r}"),
                    is_error,
                });
            }
            skill_llm::ToolEvent::Status { tool_name, status, detail } => {
                let d = detail.unwrap_or_default();
                eprintln!("[step 6]   ○ {tool_name}: {status} {d}");
                tool_evts.push(ToolEvt {
                    kind: "status".into(), tool_name, detail: format!("{status} {d}"), is_error: false,
                });
            }
        },
    )
    .await;

    let dur = t.elapsed();
    let (resp, fr, pt, ct, nc) = match result {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[step 6] ❌ {e}");
            report.steps.push(Step {
                name: "Tool chat (date)", duration: dur,
                status: StepStatus::Fail(e.clone()), detail: String::new(),
            });
            panic!("tool chat failed: {e}");
        }
    };
    let tps = tok_per_sec(ct, dur);
    eprintln!("[step 6] response ({:.2}s, {:.1} tok/s, finish={fr}):", dur.as_secs_f64(), tps);
    // Print full response for debugging
    for line in resp.lines() {
        eprintln!("[step 6]   | {line}");
    }

    // ── Validate tool execution ──────────────────────────────────────────────
    let date_started = tool_evts.iter().any(|e| e.kind == "start" && e.tool_name == "date");
    let date_ok = tool_evts.iter().any(|e| e.kind == "end" && e.tool_name == "date" && !e.is_error);
    let bogus_tools: Vec<String> = tool_evts.iter()
        .filter(|e| e.kind == "end" && e.is_error && e.tool_name != "date")
        .map(|e| e.tool_name.clone())
        .collect();

    let tool_status = if date_started && date_ok && bogus_tools.is_empty() {
        eprintln!("[step 6] ✅ date tool called and succeeded, no spurious tool calls");
        StepStatus::Ok
    } else if date_started && date_ok {
        let msg = format!("date tool OK but model also tried disabled tools: {:?}", bogus_tools);
        eprintln!("[step 6] ⚠️  {msg}");
        StepStatus::Warn(msg)
    } else if !date_started {
        let msg = "model did NOT call the date tool".into();
        eprintln!("[step 6] ❌ {msg}");
        StepStatus::Fail(msg)
    } else {
        let msg = "date tool was called but returned an error".into();
        eprintln!("[step 6] ❌ {msg}");
        StepStatus::Fail(msg)
    };

    let tools_called: Vec<String> = tool_evts.iter()
        .filter(|e| e.kind == "start")
        .map(|e| e.tool_name.clone())
        .collect();
    report.steps.push(Step {
        name: "Tool chat (date)", duration: dur, status: tool_status,
        detail: format!("{:.1} tok/s, prompt={pt}, completion={ct}, finish={fr}, tools={:?}", tps, tools_called),
    });
    report.chats.push(ChatRecord {
        label: "Tool chat (date)", messages_in: msgs_tool,
        response_text: resp.clone(), visible_text,
        finish_reason: fr, prompt_tokens: pt, completion_tokens: ct,
        n_ctx: nc, duration: dur, tok_per_sec: tps, tool_events: tool_evts,
    });

    // The response must not be empty.
    assert!(!resp.trim().is_empty(), "tool-call response must not be empty");
    // The date tool must have been called and succeeded.
    assert!(date_started, "model must call the date tool");
    assert!(date_ok, "date tool must complete without error");

    // ── 7. Shutdown ──────────────────────────────────────────────────────────
    let t = Instant::now();
    eprintln!("[step 7] shutting down …");
    let n_ctx_final = server.n_ctx.load(Ordering::Relaxed);
    match Arc::try_unwrap(server) {
        Ok(owned) => owned.shutdown(),
        Err(arc) => drop(arc),
    }
    let dur = t.elapsed();
    eprintln!("[step 7] done ({:.2}s)", dur.as_secs_f64());
    report.steps.push(Step {
        name: "Shutdown", duration: dur, status: StepStatus::Ok,
        detail: format!("n_ctx={n_ctx_final}"),
    });

    // ── 8. Cleanup ───────────────────────────────────────────────────────────
    let t = Instant::now();
    let _ = std::fs::remove_dir_all(&skill_dir);
    report.steps.push(Step {
        name: "Cleanup", duration: t.elapsed(), status: StepStatus::Ok, detail: String::new(),
    });

    // ── TOTAL ────────────────────────────────────────────────────────────────
    report.steps.push(Step {
        name: "TOTAL", duration: test_start.elapsed(), status: StepStatus::Ok, detail: String::new(),
    });

    report.print_final();

    // Final assertion: no FAIL steps
    let failures: Vec<&Step> = report.steps.iter()
        .filter(|s| matches!(s.status, StepStatus::Fail(_)))
        .collect();
    assert!(failures.is_empty(), "E2E test had {} failed step(s)", failures.len());
}
