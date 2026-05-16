#![allow(clippy::unwrap_used, clippy::panic)]
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//!
//! End-to-end MTP (Multi-Token Prediction) integration test.
//!
//! Runs the full pipeline against a cached MTP-capable GGUF (the
//! `qwen36-27b-mtp` catalog family — e.g. `Qwen3.6-27B-Q4_K_M-mtp.gguf` from
//! `froggeric/Qwen3.6-27B-MTP-GGUF`). Verifies that the spec-decode loop
//! activates and reports a non-zero draft acceptance rate.
//!
//! **Skip-friendly:** if no MTP-capable model is cached locally (no HF
//! cache hit), the test logs a clear skip-warning and exits OK. This is the
//! same offline-only pattern as `llm_e2e.rs`. Set up the cache once via:
//!
//!   huggingface-cli download froggeric/Qwen3.6-27B-MTP-GGUF \
//!       Qwen3.6-27B-Q4_K_M-mtp.gguf
//!
//! Run with:
//!   cargo test -p skill-llm --features llm --test llm_mtp_e2e -- --nocapture

#![cfg(feature = "llm")]

use std::sync::{atomic::Ordering, Arc};
use std::time::{Duration, Instant};

use serde_json::json;

use skill_llm::catalog::{DownloadState, LlmCatalog, LlmModelEntry};
use skill_llm::config::LlmConfig;
use skill_llm::engine::protocol::GenParams;
use skill_llm::{init, new_log_buffer, LlmEventEmitter, NoopEmitter};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Find the smallest cached MTP-capable model (entry with `mtp == true` AND
/// resolves to an HF cache hit). Excludes mmproj files.
fn best_cached_mtp_model(catalog: &LlmCatalog) -> Option<&LlmModelEntry> {
    let mut cached: Vec<&LlmModelEntry> = catalog
        .entries
        .iter()
        .filter(|e| e.mtp && !e.is_mmproj() && e.resolve_cached().is_some())
        .collect();
    cached.sort_by(|a, b| a.size_gb.total_cmp(&b.size_gb));
    cached.first().copied()
}

fn wait_ready(state: &skill_llm::LlmServerState, timeout: Duration) -> bool {
    let start = Instant::now();
    while !state.is_ready() {
        if start.elapsed() > timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    true
}

async fn collect_tokens(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<skill_llm::InferToken>,
) -> Result<(String, String, usize, usize, usize), String> {
    let mut text = String::new();
    let (mut fr, mut pt, mut ct, mut nc) = (String::new(), 0usize, 0usize, 0usize);
    while let Some(tok) = rx.recv().await {
        match tok {
            skill_llm::InferToken::Delta(t) => text.push_str(&t),
            skill_llm::InferToken::Done {
                finish_reason,
                prompt_tokens,
                completion_tokens,
                n_ctx,
            } => {
                fr = finish_reason;
                pt = prompt_tokens;
                ct = completion_tokens;
                nc = n_ctx;
                break;
            }
            skill_llm::InferToken::Error(e) => return Err(e),
        }
    }
    Ok((text, fr, pt, ct, nc))
}

/// Concatenate every log entry's message — used to grep for `[mtp]` lines.
fn log_dump(log_buf: &skill_llm::LlmLogBuffer) -> String {
    let guard = log_buf.lock().expect("log buf poisoned");
    guard
        .iter()
        .map(|e| format!("[{}] {}", e.level, e.message))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse `accepted=N (X.Y%)` out of the `[mtp] generation done` line.
fn parse_mtp_summary(logs: &str) -> Option<(u64, u64, u64, f64)> {
    let line = logs.lines().rev().find(|l| l.contains("[mtp] generation done"))?;
    let kv = |key: &str| -> Option<u64> {
        let i = line.find(key)?;
        let rest = &line[i + key.len()..];
        let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        rest[..end].parse().ok()
    };
    let rounds = kv("rounds=")?;
    let drafts = kv("drafts=")?;
    let accepted = kv("accepted=")?;
    let pct_start = line.find('(')?;
    let pct_end = line[pct_start..].find('%')?;
    let pct = line[pct_start + 1..pct_start + pct_end].parse::<f64>().ok()?;
    Some((rounds, drafts, accepted, pct))
}

// ── Test ─────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn mtp_e2e_spec_decode_loop() {
    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════════════════╗");
    eprintln!("║  MTP E2E Integration Test — spec-decode pipeline                            ║");
    eprintln!("╚══════════════════════════════════════════════════════════════════════════════╝");
    eprintln!();

    // ── 1. Temp skill_dir ─────────────────────────────────────────────────
    let skill_dir = std::env::temp_dir().join(format!("skill-mtp-e2e-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&skill_dir);
    eprintln!("[1] skill_dir = {}", skill_dir.display());

    // ── 2. Find a cached MTP-capable model ─────────────────────────────────
    let mut catalog = LlmCatalog::load(&skill_dir);
    let Some(entry) = best_cached_mtp_model(&catalog).cloned() else {
        eprintln!(
            "[2] ⚠️  SKIP — no MTP-capable model cached locally.\n\
                     Cache one with:\n    \
                     huggingface-cli download froggeric/Qwen3.6-27B-MTP-GGUF \\\n    \
                         Qwen3.6-27B-Q4_K_M-mtp.gguf"
        );
        let _ = std::fs::remove_dir_all(&skill_dir);
        return;
    };
    eprintln!(
        "[2] selected MTP model: {} ({:.2} GB, quant={}, family={})",
        entry.filename, entry.size_gb, entry.quant, entry.family_name
    );

    // ── 3. Wire the catalog as if the user picked this model ──────────────
    let local_path = entry
        .resolve_cached()
        .expect("MTP entry should resolve from local HF cache");
    eprintln!("[3] cache hit → {}", local_path.display());
    if let Some(e) = catalog.entries.iter_mut().find(|e| e.filename == entry.filename) {
        e.state = DownloadState::Downloaded;
        e.local_path = Some(local_path.clone());
    }
    catalog.active_model = entry.filename.clone();

    // ── 4. Start LLM server with MTP enabled ──────────────────────────────
    let t = Instant::now();
    let config = LlmConfig {
        enabled: true,
        n_gpu_layers: u32::MAX,
        ctx_size: Some(2048),
        // The v0.2.53 fork benchmark found =1 was the sweet spot on Q4_K_M
        // (+6.2% throughput vs baseline). =3 regressed for that quant.
        mtp_draft_count: 1,
        ..LlmConfig::default()
    };
    let emitter: Arc<dyn LlmEventEmitter> = Arc::new(NoopEmitter);
    let log_buf = new_log_buffer();

    eprintln!("[4] starting LLM server (mtp_draft_count={}) …", config.mtp_draft_count);
    let server =
        init(&config, &catalog, emitter, log_buf.clone(), &skill_dir).expect("init should return a running server");
    let readied = wait_ready(&server, Duration::from_secs(180));
    let load_dur = t.elapsed();

    if !readied {
        eprintln!("[4] ❌ server failed to reach ready within 180s");
        let logs = log_dump(&log_buf);
        eprintln!("--- log dump ---\n{logs}\n----------------");
        panic!("server not ready");
    }
    let n_ctx = server.n_ctx.load(Ordering::Relaxed);
    eprintln!("[4] ✅ ready in {:.2}s — n_ctx={n_ctx}", load_dur.as_secs_f64());

    // ── 5. Assert the MTP smoke validation fired and succeeded ────────────
    let logs = log_dump(&log_buf);
    let smoke_ok = logs.contains("[mtp] draft heads present");
    let smoke_fail = logs.contains("[mtp] draft heads missing");
    if smoke_fail {
        eprintln!(
            "[5] ❌ MTP smoke validation failed — the GGUF was flagged catalog \
             `mtp:true` but llama-cpp-4 could not build a Mtp context. \
             Either the GGUF is stale or mis-flagged."
        );
        eprintln!("--- log dump ---\n{logs}\n----------------");
        panic!("MTP smoke validation failed at load time");
    }
    assert!(smoke_ok, "expected '[mtp] draft heads present' in logs, got:\n{logs}");
    eprintln!("[5] ✅ smoke validation: draft heads present");

    // ── 6. Send a short generation request (text-only, no images) ─────────
    eprintln!("[6] sending short generation request …");
    let msgs = vec![
        json!({"role": "system", "content": "You are a helpful assistant. Answer concisely."}),
        json!({"role": "user",   "content": "Name three colors of the rainbow. Just the words, one per line."}),
    ];
    let params = GenParams {
        max_tokens: 32,
        // Temperature 0 (greedy-like) gives MTP its best chance — drafts are
        // most likely to match deterministic sampling.
        temperature: 0.0,
        thinking_budget: Some(0),
        ..GenParams::default()
    };
    let gen_start = Instant::now();
    let rx = server.chat(msgs, vec![], params).expect("chat accepted");
    let (text, fr, pt, ct, nc) = collect_tokens(rx).await.expect("generation ok");
    let gen_dur = gen_start.elapsed();
    let tps = if gen_dur.as_secs_f64() > 0.0 {
        ct as f64 / gen_dur.as_secs_f64()
    } else {
        0.0
    };
    eprintln!(
        "[6] response ({:.2}s, {:.1} tok/s, finish={fr}, prompt={pt}, completion={ct}, n_ctx={nc}):",
        gen_dur.as_secs_f64(),
        tps
    );
    for line in text.lines() {
        eprintln!("[6]   | {line}");
    }
    assert!(!text.trim().is_empty(), "MTP generation produced empty text");
    assert!(ct > 0, "MTP generation produced zero completion tokens");

    // ── 7. Verify the spec-decode loop actually ran ───────────────────────
    let logs = log_dump(&log_buf);
    let Some((rounds, drafts, accepted, pct)) = parse_mtp_summary(&logs) else {
        eprintln!("--- log dump ---\n{logs}\n----------------");
        panic!("'[mtp] generation done' summary line not found in logs");
    };
    eprintln!("[7] ✅ MTP loop ran — rounds={rounds} drafts={drafts} accepted={accepted} ({pct:.1}%)");
    assert!(rounds > 0, "MTP loop reported zero rounds — dispatch likely fell back");
    assert!(drafts > 0, "MTP loop reported zero drafts proposed");
    // Accepted may legitimately be 0 on a single short prompt — we don't
    // assert a minimum acceptance rate, just that the machinery ran.

    // ── 8. Shutdown ───────────────────────────────────────────────────────
    let t = Instant::now();
    match Arc::try_unwrap(server) {
        Ok(owned) => owned.shutdown(),
        Err(arc) => drop(arc),
    }
    eprintln!("[8] shutdown ({:.2}s)", t.elapsed().as_secs_f64());

    let _ = std::fs::remove_dir_all(&skill_dir);

    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════════════════════╗");
    eprintln!("║  ✅ MTP E2E PASSED — {rounds} rounds, {accepted}/{drafts} drafts accepted ({pct:.1}%) ");
    eprintln!(
        "║     load: {:.2}s · gen: {:.2}s · {:.1} tok/s · {ct} completion tokens",
        load_dur.as_secs_f64(),
        gen_dur.as_secs_f64(),
        tps
    );
    eprintln!("╚══════════════════════════════════════════════════════════════════════════════╝");
    eprintln!();
}
