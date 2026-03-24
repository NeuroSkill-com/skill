// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//!
//! End-to-end integration test: download a small model, start the LLM
//! server, send chat requests that trigger built-in tool calls (date and
//! NeuroSkill status with mock EEG data), and verify the full pipeline.
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
        self.p(w, &format!(
            "Model: {} ({:.2} GB, {})", self.model_name, self.model_size, self.model_quant));
        self.p(w, &format!("Total: {:.2}s", total.as_secs_f64()));
        eprintln!("╠{bar}╣");
        eprintln!("║{:^width$}║", "PIPELINE STEPS", width = w - 2);
        self.sep(w);

        for (i, step) in self.steps.iter().enumerate() {
            self.p(w, &format!(
                "{} {}. {:<32} {:>8.2}s  {}",
                step.icon(), i + 1, step.name,
                step.duration.as_secs_f64(), step.text(),
            ));
            for line in step.detail.lines() {
                self.pi(w, line);
            }
        }

        if !self.chats.is_empty() {
            eprintln!("╠{bar}╣");
            eprintln!("║{:^width$}║", "CHAT EXCHANGES", width = w - 2);
            for (i, chat) in self.chats.iter().enumerate() {
                self.sep(w);
                self.p(w, &format!(
                    "Chat #{}: {} ({:.2}s, {:.1} tok/s)",
                    i + 1, chat.label, chat.duration.as_secs_f64(), chat.tok_per_sec,
                ));
                self.pi(w, &format!(
                    "prompt={} completion={} n_ctx={} finish={}",
                    chat.prompt_tokens, chat.completion_tokens, chat.n_ctx, chat.finish_reason,
                ));
                for msg in &chat.messages_in {
                    let role = msg["role"].as_str().unwrap_or("?");
                    let content = msg["content"].as_str().unwrap_or("");
                    let abbr: String = content.chars().take(64).collect();
                    self.pi(w, &format!(
                        "[{role}] {abbr}{}", if content.len() > 64 { "…" } else { "" }));
                }
                let resp: String = chat.response_text.replace('\n', " ⏎ ");
                let resp_abbr: String = resp.chars().take(w - 10).collect();
                self.p(w, &format!("  → {resp_abbr}"));
                if !chat.tool_events.is_empty() {
                    self.pi(w, "Tools:");
                    for te in &chat.tool_events {
                        let err = if te.is_error { " [ERR]" } else { "" };
                        self.pi(w, &format!(
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

    fn p(&self, w: usize, text: &str) {
        let t: String = text.chars().take(w - 4).collect();
        eprintln!("║ {:<width$} ║", t, width = w - 4);
    }
    fn pi(&self, w: usize, text: &str) {
        let t: String = text.chars().take(w - 6).collect();
        eprintln!("║   {:<width$} ║", t, width = w - 6);
    }
    fn sep(&self, w: usize) {
        eprintln!("║ {:<width$} ║", "─".repeat(w - 4), width = w - 4);
    }
}

// ── Mock NeuroSkill API server ───────────────────────────────────────────────
//
// Spins up a tiny HTTP server on a random port that responds to the same
// JSON-RPC-style protocol as the real NeuroSkill WebSocket API.  Returns
// realistic mock EEG session data so the `skill` tool can be tested end-to-end
// without a running Tauri app or real device.

fn mock_eeg_status() -> serde_json::Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    json!({
        "ok": true,
        "command": "status",
        "device": {
            "state":        "connected",
            "connected":    true,
            "streaming":    true,
            "name":         "Muse-2 (Mock)",
            "id":           "00:55:DA:B0:FF:EE",
            "serial_number": "MOCK-E2E-001",
            "mac_address":   "00:55:DA:B0:FF:EE",
            "firmware_version": "1.2.3",
            "hardware_version": "4.0",
            "battery":      78,
            "sample_count":  153600,
            "ppg_sample_count": 38400,
        },
        "session": {
            "start_utc":     now - 1800,
            "duration_secs": 1800,
        },
        "embeddings": {
            "today":          42,
            "total":          1337,
            "recording_days": 28,
            "encoder_loaded": true,
        },
        "labels": {
            "total": 156,
            "recent": [
                { "id": 1, "text": "focused deep work",   "created_at": now - 600 },
                { "id": 2, "text": "meditation session",  "created_at": now - 1200 },
                { "id": 3, "text": "reading paper on BCI", "created_at": now - 3600 },
            ],
        },
        "signal_quality": [
            { "channel": "TP9",  "quality": 0.92 },
            { "channel": "AF7",  "quality": 0.88 },
            { "channel": "AF8",  "quality": 0.95 },
            { "channel": "TP10", "quality": 0.90 },
        ],
        "scores": {
            "meditation":     0.72,
            "focus":          0.81,
            "cognitive_load": 0.45,
            "drowsiness":     0.12,
        },
        "sleep": {
            "window_hours": 48,
            "total_epochs":  0,
        },
        "hooks": {
            "total":   3,
            "enabled": 2,
        },
        "history": {
            "total_sessions":  42,
            "total_hours":     63.5,
            "streak_days":     7,
        },
    })
}

/// Start a mock NeuroSkill API server.  Returns `(port, shutdown_sender)`.
async fn start_mock_skill_api() -> (u16, tokio::sync::oneshot::Sender<()>) {
    use axum::{Router, Json, routing::post};
    use serde_json::Value;

    async fn handle(Json(body): Json<Value>) -> Json<Value> {
        let cmd = body["command"].as_str().unwrap_or("");
        eprintln!("[mock-api] ← command={cmd}");
        let resp = match cmd {
            "status" => mock_eeg_status(),
            "sessions" => json!({
                "ok": true, "command": "sessions",
                "sessions": [
                    {
                        "date": "2026-03-20",
                        "start_utc": 1774000000u64,
                        "end_utc":   1774003600u64,
                        "duration_secs": 3600,
                        "samples": 921600,
                        "embeddings": 42,
                    },
                ],
            }),
            other => json!({
                "ok": false,
                "error": format!("unknown command: {other}"),
            }),
        };
        eprintln!("[mock-api] → {}", resp.to_string().chars().take(120).collect::<String>());
        Json(resp)
    }

    let app = Router::new().route("/", post(handle));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async { let _ = rx.await; })
            .await
            .unwrap();
    });

    // Give the server a moment to accept connections.
    tokio::time::sleep(Duration::from_millis(50)).await;
    (port, tx)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn best_test_model(catalog: &LlmCatalog) -> Option<&LlmModelEntry> {
    let non_mmproj: Vec<&LlmModelEntry> = catalog.entries.iter()
        .filter(|e| !e.is_mmproj)
        .collect();
    // Smallest recommended model with params >= 1.5B — needs to be large
    // enough for reliable tool-call generation.  Models below 1.5B (e.g.
    // 1.2B) fail to produce tool calls consistently on CPU.
    let mut capable: Vec<&&LlmModelEntry> = non_mmproj.iter()
        .filter(|e| e.recommended && e.params_b >= 1.5)
        .collect();
    capable.sort_by(|a, b| a.size_gb.total_cmp(&b.size_gb));
    if let Some(e) = capable.first() { return Some(e); }
    // Fallback: smallest recommended
    let mut rec: Vec<&&LlmModelEntry> = non_mmproj.iter().filter(|e| e.recommended).collect();
    rec.sort_by(|a, b| a.size_gb.total_cmp(&b.size_gb));
    if let Some(e) = rec.first() { return Some(e); }
    non_mmproj.iter().min_by(|a, b| a.size_gb.total_cmp(&b.size_gb)).copied()
}

fn wait_ready(state: &skill_llm::LlmServerState, timeout: Duration) {
    let start = Instant::now();
    while !state.is_ready() {
        if start.elapsed() > timeout {
            panic!("LLM server not ready within {:.0}s", timeout.as_secs_f64());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

async fn collect_tokens(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<skill_llm::InferToken>,
) -> Result<(String, String, usize, usize, usize), String> {
    let mut text = String::new();
    let mut fr = String::new();
    let (mut pt, mut ct, mut nc) = (0, 0, 0);
    while let Some(tok) = rx.recv().await {
        match tok {
            skill_llm::InferToken::Delta(t) => text.push_str(&t),
            skill_llm::InferToken::Done { finish_reason, prompt_tokens, completion_tokens, n_ctx } => {
                fr = finish_reason; pt = prompt_tokens; ct = completion_tokens; nc = n_ctx;
                break;
            }
            skill_llm::InferToken::Error(e) => return Err(e),
        }
    }
    Ok((text, fr, pt, ct, nc))
}

/// Create a minimal 100x100 white PNG with some visual content for testing.
/// Uses a raw minimal PNG encoder — no image crate dependency needed.
fn create_test_png() -> Vec<u8> {
    // Generate a simple 2x2 white PNG (smallest valid PNG).
    // For the embedding test, even a tiny image works — the model will
    // process whatever it gets.
    let mut png = Vec::new();
    // PNG signature
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    // IHDR chunk: 2x2, 8-bit RGB
    let ihdr_data: [u8; 13] = [
        0, 0, 0, 2,  // width = 2
        0, 0, 0, 2,  // height = 2
        8,           // bit depth = 8
        2,           // color type = RGB
        0, 0, 0,     // compression, filter, interlace
    ];
    write_png_chunk(&mut png, b"IHDR", &ihdr_data);
    // IDAT chunk: 2 rows, each with filter byte (0) + 6 bytes RGB
    // Row 1: white white, Row 2: black white (simple pattern)
    let raw_rows: [u8; 14] = [
        0, 255, 255, 255, 255, 255, 255, // filter=0, white, white
        0, 0, 0, 0, 255, 255, 255,       // filter=0, black, white
    ];
    // Compress with deflate (use miniz_oxide or manual zlib)
    let compressed = zlib_compress(&raw_rows);
    write_png_chunk(&mut png, b"IDAT", &compressed);
    // IEND chunk
    write_png_chunk(&mut png, b"IEND", &[]);
    png
}

fn write_png_chunk(buf: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buf.extend_from_slice(chunk_type);
    buf.extend_from_slice(data);
    let mut crc_data = Vec::with_capacity(4 + data.len());
    crc_data.extend_from_slice(chunk_type);
    crc_data.extend_from_slice(data);
    buf.extend_from_slice(&crc32(&crc_data).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn zlib_compress(data: &[u8]) -> Vec<u8> {
    // Minimal zlib: header + stored block (no compression) + adler32
    let mut out = Vec::new();
    out.push(0x78); // CMF: deflate, window size 32K
    out.push(0x01); // FLG: no dict, check bits
    // Stored block: final=1, type=00 (stored)
    out.push(0x01);
    let len = data.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    // Adler-32 checksum
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    let adler = (b << 16) | a;
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

fn tps(ct: usize, dur: Duration) -> f64 {
    let s = dur.as_secs_f64();
    if s > 0.0 { ct as f64 / s } else { 0.0 }
}

/// Run a tool-calling chat step.  Returns the step + chat record.
async fn run_tool_chat(
    server: &skill_llm::LlmServerState,
    label: &'static str,
    step_name: &'static str,
    step_num: usize,
    messages: Vec<serde_json::Value>,
    params: GenParams,
    expect_tool: &str,
) -> (Step, ChatRecord, bool /* tool_ok */) {
    let t = Instant::now();
    let expect = expect_tool.to_string();

    let mut visible = String::new();
    let mut evts: Vec<ToolEvt> = Vec::new();

    let result = run_chat_with_builtin_tools(
        server,
        messages.clone(),
        params,
        vec![],
        |delta| { visible.push_str(delta); },
        |event| match event {
            skill_llm::ToolEvent::ExecutionStart { tool_name, tool_call_id, args } => {
                let d = format!("id={tool_call_id} args={args}");
                eprintln!("[step {step_num}]   ▶ {tool_name}: {d}");
                evts.push(ToolEvt { kind: "start".into(), tool_name, detail: d, is_error: false });
            }
            skill_llm::ToolEvent::ExecutionEnd { tool_name, tool_call_id, result, is_error } => {
                let r: String = result.to_string().chars().take(200).collect();
                eprintln!("[step {step_num}]   ■ {tool_name} (err={is_error}): {r}");
                evts.push(ToolEvt {
                    kind: "end".into(), tool_name,
                    detail: format!("id={tool_call_id} result={r}"), is_error,
                });
            }
            skill_llm::ToolEvent::Status { tool_name, status, detail } => {
                let d = detail.unwrap_or_default();
                eprintln!("[step {step_num}]   ○ {tool_name}: {status} {d}");
                evts.push(ToolEvt {
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
            eprintln!("[step {step_num}] ❌ {e}");
            let step = Step { name: step_name, duration: dur,
                status: StepStatus::Fail(e.to_string()), detail: String::new() };
            let chat = ChatRecord { label, messages_in: messages,
                response_text: String::new(), visible_text: visible,
                finish_reason: "error".into(), prompt_tokens: 0, completion_tokens: 0,
                n_ctx: 0, duration: dur, tok_per_sec: 0.0, tool_events: evts };
            return (step, chat, false);
        }
    };

    let t_s = tps(ct, dur);
    eprintln!("[step {step_num}] response ({:.2}s, {:.1} tok/s, finish={fr}):", dur.as_secs_f64(), t_s);
    for line in resp.lines() { eprintln!("[step {step_num}]   | {line}"); }

    let started = evts.iter().any(|e| e.kind == "start" && e.tool_name == expect);
    let ok = evts.iter().any(|e| e.kind == "end" && e.tool_name == expect && !e.is_error);
    let bogus: Vec<String> = evts.iter()
        .filter(|e| e.kind == "end" && e.is_error && e.tool_name != expect)
        .map(|e| e.tool_name.clone()).collect();

    let status = if started && ok && bogus.is_empty() {
        eprintln!("[step {step_num}] ✅ {expect} tool called and succeeded");
        StepStatus::Ok
    } else if started && ok {
        let msg = format!("{expect} OK, but also tried disabled tools: {bogus:?}");
        eprintln!("[step {step_num}] ⚠️  {msg}");
        StepStatus::Warn(msg)
    } else if !started {
        let msg = format!("model did NOT call {expect}");
        eprintln!("[step {step_num}] ❌ {msg}");
        StepStatus::Fail(msg)
    } else {
        let msg = format!("{expect} called but returned error");
        eprintln!("[step {step_num}] ❌ {msg}");
        StepStatus::Fail(msg)
    };

    let tools_called: Vec<String> = evts.iter()
        .filter(|e| e.kind == "start").map(|e| e.tool_name.clone()).collect();
    let step = Step {
        name: step_name, duration: dur, status,
        detail: format!("{t_s:.1} tok/s, prompt={pt}, completion={ct}, finish={fr}, tools={tools_called:?}"),
    };
    let chat = ChatRecord {
        label, messages_in: messages,
        response_text: resp, visible_text: visible,
        finish_reason: fr, prompt_tokens: pt, completion_tokens: ct,
        n_ctx: nc, duration: dur, tok_per_sec: t_s, tool_events: evts,
    };
    (step, chat, started && ok)
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

    // ── 2. Start mock NeuroSkill API ─────────────────────────────────────────
    let t = Instant::now();
    let (mock_port, mock_shutdown) = start_mock_skill_api().await;
    eprintln!("[step 2] mock NeuroSkill API on port {mock_port}");
    report.steps.push(Step {
        name: "Start mock Skill API", duration: t.elapsed(), status: StepStatus::Ok,
        detail: format!("port={mock_port}"),
    });

    // ── 3. Load catalog and find test model ──────────────────────────────────
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
    eprintln!("[step 3] selected: {detail}");
    report.steps.push(Step {
        name: "Load catalog + select model", duration: t.elapsed(), status: StepStatus::Ok, detail,
    });

    // ── 4. Download the model ────────────────────────────────────────────────
    let t = Instant::now();
    let progress = Arc::new(Mutex::new(DownloadProgress {
        filename: entry.filename.clone(), state: DownloadState::Downloading,
        status_msg: None, progress: 0.0, cancelled: false, pause_requested: false,
        current_shard: 0, total_shards: entry.shard_count() as u16,
    }));
    eprintln!("[step 4] downloading {} ({:.2} GB) …", entry.filename, entry.size_gb);
    let local_path = skill_llm::catalog::download_model(&entry, &progress)
        .expect("download should succeed");
    let dl_dur = t.elapsed();
    let dl_speed = if dl_dur.as_secs_f64() > 0.0 {
        (entry.size_gb as f64 * 1024.0) / dl_dur.as_secs_f64()
    } else { 0.0 };
    let detail = format!("{:.1}s ({:.1} MB/s) → {}", dl_dur.as_secs_f64(), dl_speed, local_path.display());
    eprintln!("[step 4] done: {detail}");
    report.steps.push(Step {
        name: "Download model", duration: dl_dur, status: StepStatus::Ok, detail,
    });

    if let Some(e) = catalog.entries.iter_mut().find(|e| e.filename == entry.filename) {
        e.state = DownloadState::Downloaded;
        e.local_path = Some(local_path.clone());
    }
    catalog.active_model = entry.filename.clone();

    // ── 5. Start LLM server ─────────────────────────────────────────────────
    let t = Instant::now();
    let config = LlmConfig {
        enabled: true,
        n_gpu_layers: u32::MAX,
        // 2048 is plenty for the short test prompts (< 200 tokens each)
        // and halves KV-cache allocation vs the previous 4096.
        ctx_size: Some(2048),
        ..LlmConfig::default()
    };
    let emitter: Arc<dyn LlmEventEmitter> = Arc::new(NoopEmitter);
    let log_buf = new_log_buffer();

    eprintln!("[step 5] starting LLM server …");
    let server = init(&config, &catalog, emitter, log_buf, &skill_dir)
        .expect("init should return a running server");
    wait_ready(&server, Duration::from_secs(120));
    let load_dur = t.elapsed();
    let n_ctx = server.n_ctx.load(Ordering::Relaxed);
    let detail = format!("n_ctx={n_ctx}, load={:.2}s", load_dur.as_secs_f64());
    eprintln!("[step 5] ready: {detail}");
    report.steps.push(Step {
        name: "Start LLM server", duration: load_dur, status: StepStatus::Ok, detail,
    });

    // ── 6. Simple chat (no tools) ────────────────────────────────────────────
    let t = Instant::now();
    eprintln!("[step 6] simple chat: \"What is 2+2? Answer with just the number.\"");
    let msgs = vec![
        json!({"role": "system", "content": "You are a helpful assistant. Answer concisely."}),
        json!({"role": "user",   "content": "What is 2+2? Answer with just the number."}),
    ];
    let params = GenParams {
        max_tokens: 32, temperature: 0.0, thinking_budget: Some(0), ..GenParams::default()
    };
    let rx = server.chat(msgs.clone(), vec![], params).expect("accepted");
    let (text, fr, pt, ct, nc) = collect_tokens(rx).await.expect("generation ok");
    let dur = t.elapsed();
    let t_s = tps(ct, dur);
    eprintln!("[step 6] response ({:.2}s, {:.1} tok/s, finish={}): {:?}", dur.as_secs_f64(), t_s, fr, text.trim());

    let ok = !text.trim().is_empty();
    let has_4 = text.contains('4');
    let status = if ok && has_4 { StepStatus::Ok }
        else if ok { StepStatus::Warn(format!("no '4' in response: {:?}", text.trim())) }
        else { StepStatus::Fail("empty response".into()) };
    report.steps.push(Step {
        name: "Simple chat (no tools)", duration: dur, status,
        detail: format!("{t_s:.1} tok/s, prompt={pt}, completion={ct}, finish={fr}"),
    });
    report.chats.push(ChatRecord {
        label: "Simple chat", messages_in: msgs,
        response_text: text, visible_text: String::new(),
        finish_reason: fr, prompt_tokens: pt, completion_tokens: ct,
        n_ctx: nc, duration: dur, tok_per_sec: t_s, tool_events: vec![],
    });
    assert!(ok, "simple chat response must not be empty");

    // ── 7. Tool chat — date tool ─────────────────────────────────────────────
    eprintln!("[step 7] tool chat: date tool");
    {
        let mut tools = server.allowed_tools.lock().expect("lock");
        tools.enabled = true;
        tools.date = true;
        tools.max_rounds = 1; // limit rounds so small models don't loop on CPU
        tools.location = false; tools.web_search = false; tools.web_fetch = false;
        tools.bash = false; tools.read_file = false; tools.write_file = false;
        tools.edit_file = false; tools.skill_api = false;
    }
    let msgs = vec![
        json!({"role": "system", "content": "You are a helpful assistant with tool access. When asked about the current date or time, you MUST call the date tool. After receiving the tool result, state the date clearly. Be concise. Do NOT call any other tool."}),
        json!({"role": "user",   "content": "What is today's date? Call the date tool to check."}),
    ];
    let params = GenParams {
        // 64 tokens is enough for the tool-call XML (~25 tok) and the
        // post-tool summary.  Prevents the model from echoing the entire
        // JSON result verbatim, saving ~60 tokens of wasted inference.
        max_tokens: 64, temperature: 0.0, thinking_budget: Some(0), ..GenParams::default()
    };
    let (step, chat, date_ok) = run_tool_chat(&server, "Tool chat (date)", "Tool chat (date)", 7, msgs, params, "date").await;
    report.steps.push(step);
    report.chats.push(chat);
    assert!(date_ok, "date tool must be called and succeed");

    // ── 8. Tool chat — NeuroSkill status (mock EEG data) ─────────────────────
    eprintln!("[step 8] tool chat: NeuroSkill status (mock EEG data via skill tool)");
    {
        let mut tools = server.allowed_tools.lock().expect("lock");
        tools.enabled = true;
        tools.date = false;
        tools.skill_api = true;
        tools.skill_api_port = mock_port;
        tools.max_rounds = 1;
        tools.bash = false; tools.read_file = false; tools.write_file = false;
        tools.edit_file = false; tools.location = false;
        tools.web_search = false; tools.web_fetch = false;
    }
    let msgs = vec![
        json!({"role": "system", "content": "You are a helpful assistant integrated with NeuroSkill, an EEG brain-computer interface app. You have access to the skill tool. When asked about the user's brain state or EEG status, call the skill tool with command \"status\". Summarize the result concisely. Do NOT call any other tool."}),
        json!({"role": "user",   "content": "What is my current EEG status? Use the skill tool with the status command to check."}),
    ];
    let params = GenParams {
        max_tokens: 64, temperature: 0.0, thinking_budget: Some(0), ..GenParams::default()
    };
    let (step, chat, skill_ok) = run_tool_chat(
        &server, "Tool chat (skill status)", "Tool chat (skill status)", 8, msgs, params, "skill",
    ).await;

    // Check the mock API was actually hit — look for the tool result containing mock data.
    let got_mock_data = chat.tool_events.iter().any(|e| {
        e.kind == "end" && e.tool_name == "skill" && !e.is_error
            && (e.detail.contains("Muse-2") || e.detail.contains("connected") || e.detail.contains("1337"))
    });
    if got_mock_data {
        eprintln!("[step 8] ✅ mock EEG data received in tool result");
    } else if skill_ok {
        eprintln!("[step 8] ⚠️  skill tool called but mock data not found in result");
    }

    report.steps.push(step);
    report.chats.push(chat);
    assert!(skill_ok, "skill tool must be called and succeed");
    assert!(got_mock_data, "mock EEG data must appear in skill tool result");

    // ── 9. VLM image embedding benchmark ────────────────────────────────────
    //
    // Test the EmbedImage path (mean-pooled vision token embedding).
    // This is the same path used by the "mmproj" and "llm-vlm" screenshot
    // backends.  On models without a vision projector this step is skipped.
    let t = Instant::now();
    eprintln!("[step 9] VLM image embedding benchmark");
    let has_vision = server.vision_ready.load(Ordering::Relaxed);
    if has_vision {
        // Create a simple test image: 100x100 white PNG with "Hello World" text
        let test_png = create_test_png();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let send_ok = server.req_tx.send(skill_llm::InferRequest::EmbedImage {
            bytes: test_png.clone(),
            result_tx: tx,
        }).is_ok();
        if send_ok {
            match rx.await {
                Ok(Some(emb)) => {
                    let dur = t.elapsed();
                    eprintln!("[step 9] ✅ image embedding: {} dims in {:.2}s", emb.len(), dur.as_secs_f64());
                    report.steps.push(Step {
                        name: "VLM image embed", duration: dur, status: StepStatus::Ok,
                        detail: format!("dims={}, time={:.2}s", emb.len(), dur.as_secs_f64()),
                    });
                }
                Ok(None) => {
                    let dur = t.elapsed();
                    eprintln!("[step 9] ⚠️  EmbedImage returned None");
                    report.steps.push(Step {
                        name: "VLM image embed", duration: dur,
                        status: StepStatus::Warn("EmbedImage returned None".into()),
                        detail: String::new(),
                    });
                }
                Err(e) => {
                    let dur = t.elapsed();
                    eprintln!("[step 9] ⚠️  EmbedImage channel error: {e}");
                    report.steps.push(Step {
                        name: "VLM image embed", duration: dur,
                        status: StepStatus::Warn(format!("channel error: {e}")),
                        detail: String::new(),
                    });
                }
            }
        } else {
            report.steps.push(Step {
                name: "VLM image embed", duration: t.elapsed(),
                status: StepStatus::Warn("failed to send EmbedImage request".into()),
                detail: String::new(),
            });
        }
    } else {
        eprintln!("[step 9] ⚠️  vision not available (no mmproj) — skipping image embed benchmark");
        report.steps.push(Step {
            name: "VLM image embed", duration: t.elapsed(),
            status: StepStatus::Warn("vision not available (no mmproj loaded)".into()),
            detail: String::new(),
        });
    }

    // ── 10. Shutdown ─────────────────────────────────────────────────────────
    let t = Instant::now();
    eprintln!("[step 10] shutting down …");
    let n_ctx_final = server.n_ctx.load(Ordering::Relaxed);
    match Arc::try_unwrap(server) {
        Ok(owned) => owned.shutdown(),
        Err(arc) => drop(arc),
    }
    let dur = t.elapsed();
    eprintln!("[step 10] LLM shutdown ({:.2}s)", dur.as_secs_f64());
    report.steps.push(Step {
        name: "Shutdown LLM", duration: dur, status: StepStatus::Ok,
        detail: format!("n_ctx={n_ctx_final}"),
    });

    // Stop mock API
    let _ = mock_shutdown.send(());
    report.steps.push(Step {
        name: "Stop mock Skill API", duration: Duration::ZERO, status: StepStatus::Ok,
        detail: String::new(),
    });

    // ── 10. Cleanup ──────────────────────────────────────────────────────────
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

    let failures: Vec<&Step> = report.steps.iter()
        .filter(|s| matches!(s.status, StepStatus::Fail(_))).collect();
    assert!(failures.is_empty(), "E2E test had {} failed step(s)", failures.len());
}
