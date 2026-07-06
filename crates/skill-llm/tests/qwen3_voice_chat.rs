#![allow(clippy::unwrap_used, clippy::panic)]
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//!
//! Qwen3-0.6B integration test for the ASR voice-loop's LLM step.
//!
//! This is the LLM half of the daemon's `routes::asr::voice_loop_turn`: it feeds
//! a *simulated ASR transcript* through the exact `run_chat_with_builtin_tools`
//! call the voice loop uses and asserts a usable spoken reply.
//!
//! Quant: Q4_K_M — the efficient/snappy sweet spot for a 0.6B model while staying
//! precise enough to answer simple prompts. (Q8_0 is the higher-precision option.)
//!
//! Run with (use `--release`; debug-build rlx gemm is far too slow for inference):
//!   QWEN3_06B_GGUF=/path/Qwen3-0.6B-Q4_K_M.gguf \
//!     cargo test -p skill-llm --release --features llm-rlx-metal \
//!       --test qwen3_voice_chat -- --ignored --nocapture
#![cfg(feature = "llm-rlx")]

use std::path::PathBuf;
use std::sync::{atomic::Ordering, Arc};
use std::time::{Duration, Instant};

use serde_json::json;
use skill_llm::catalog::LlmCatalog;
use skill_llm::config::LlmConfig;
use skill_llm::engine::protocol::GenParams;
use skill_llm::{init, new_log_buffer, InferToken, LlmEventEmitter, NoopEmitter};

/// Drain the streamed reply: `(text, finish_reason, prompt_tokens, completion_tokens)`.
async fn collect_tokens(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<InferToken>,
) -> Result<(String, String, usize, usize), String> {
    let mut text = String::new();
    let (mut finish, mut pt, mut ct) = (String::new(), 0, 0);
    while let Some(tok) = rx.recv().await {
        match tok {
            InferToken::Delta(t) => text.push_str(&t),
            InferToken::Done {
                finish_reason,
                prompt_tokens,
                completion_tokens,
                ..
            } => {
                finish = finish_reason;
                pt = prompt_tokens;
                ct = completion_tokens;
                break;
            }
            InferToken::Error(e) => return Err(e),
        }
    }
    Ok((text, finish, pt, ct))
}

/// Resolve a Qwen3-0.6B GGUF from `QWEN3_06B_GGUF` or a couple of cache spots.
fn qwen3_weights() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("QWEN3_06B_GGUF") {
        let p = PathBuf::from(raw);
        if p.is_file() {
            return Some(p);
        }
    }
    for candidate in [
        "/tmp/rlx-weights/Qwen3-0.6B-GGUF/Qwen3-0.6B-Q4_K_M.gguf",
        "/tmp/rlx-weights/Qwen3-0.6B-GGUF/Qwen3-0.6B-Q8_0.gguf",
    ] {
        let p = PathBuf::from(candidate);
        if p.is_file() {
            return Some(p);
        }
    }
    None
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

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a Qwen3-0.6B GGUF — set QWEN3_06B_GGUF (Q4_K_M recommended)"]
async fn qwen3_voice_loop_chat_step() {
    let Some(weights) = qwen3_weights() else {
        eprintln!("skip: set QWEN3_06B_GGUF to a Qwen3-0.6B GGUF (e.g. Qwen3-0.6B-Q4_K_M.gguf)");
        return;
    };
    eprintln!("[qwen3] weights: {}", weights.display());

    let skill_dir = std::env::temp_dir().join(format!("skill-qwen3-voice-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&skill_dir);

    // Empty active model → init falls back to config.model_path (our GGUF).
    let mut catalog = LlmCatalog::load(&skill_dir);
    catalog.active_model = String::new();

    let config = LlmConfig {
        enabled: true,
        model_path: Some(weights.clone()),
        n_gpu_layers: u32::MAX,
        ctx_size: Some(2048),
        ..LlmConfig::default()
    };
    let emitter: Arc<dyn LlmEventEmitter> = Arc::new(NoopEmitter);
    let log_buf = new_log_buffer();

    let load_start = Instant::now();
    let server = init(&config, &catalog, emitter, log_buf, &skill_dir).expect("init Qwen3 server");
    wait_ready(&server, Duration::from_secs(180));
    eprintln!(
        "[qwen3] server ready in {:.2}s (n_ctx={})",
        load_start.elapsed().as_secs_f64(),
        server.n_ctx.load(Ordering::Relaxed)
    );

    // What the ASR voice loop sends: the user's transcribed utterance. We drive
    // the generation path directly via `server.chat` (the daemon's voice loop
    // wraps this same call in `run_chat_with_builtin_tools` for tool support;
    // tools are skipped here to keep the prompt small and the test fast).
    let transcript = "In one short sentence, what is the capital of France?";
    let messages = vec![
        json!({ "role": "system", "content": "You are a concise voice assistant. Reply in one short sentence." }),
        json!({ "role": "user", "content": transcript }),
    ];
    // NOTE: rlx 0.2.9 builds every LLM family runner device-less (→ Device::Cpu),
    // and the Qwen3 GGUF path runs `Op::DequantMatMul` single-threaded on CPU, so
    // generation is slow even with `--features llm-rlx-metal`. Keep max_tokens small
    // so this on-demand test stays tractable until the runner is built on Metal.
    let params = GenParams {
        max_tokens: 24,
        temperature: 0.0,
        thinking_budget: Some(0), // Qwen3 thinking off → direct spoken answer
        ..GenParams::default()
    };

    let gen_start = Instant::now();
    let rx = server.chat(messages, vec![], params).expect("chat accepted");
    let (reply, _finish, prompt_tokens, completion_tokens) = collect_tokens(rx).await.expect("generation failed");
    eprintln!(
        "[qwen3] reply ({:.2}s) prompt={prompt_tokens} completion={completion_tokens}: {reply:?}",
        gen_start.elapsed().as_secs_f64()
    );

    match Arc::try_unwrap(server) {
        Ok(owned) => owned.shutdown(),
        Err(arc) => drop(arc),
    };
    let _ = std::fs::remove_dir_all(&skill_dir);

    assert!(prompt_tokens > 0, "prompt should tokenize");
    assert!(completion_tokens > 0, "model should emit tokens");
    assert!(!reply.trim().is_empty(), "voice-loop reply should be non-empty");
    assert!(
        reply.to_lowercase().contains("paris"),
        "Qwen3-0.6B should answer 'Paris' — got {reply:?}"
    );
}
