#![allow(clippy::unwrap_used, clippy::panic)]
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//!
//! MiniCPM5-1B integration test via `rlx-minicpm5` through skill-llm.
//!
//! Run with:
//!   cargo test -p skill-llm --features llm-rlx-cpu --test minicpm5_e2e -- --nocapture
//!
//! Optional env:
//!   MINICPM5_WEIGHTS=/path/to/MiniCPM5-1B-Q4_K_M.gguf

#![cfg(feature = "llm-rlx")]

use std::path::{Path, PathBuf};
use std::sync::{atomic::Ordering, Arc};
use std::time::{Duration, Instant};

use serde_json::json;
use skill_llm::catalog::{DownloadState, LlmCatalog};
use skill_llm::config::LlmConfig;
use skill_llm::engine::protocol::GenParams;
use skill_llm::{init, new_log_buffer, LlmEventEmitter, NoopEmitter};

fn minicpm5_weights_path(entry: &skill_llm::catalog::LlmModelEntry) -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("MINICPM5_WEIGHTS") {
        let p = PathBuf::from(raw);
        if p.is_file() {
            return Some(p);
        }
    }

    if let Some(p) = entry.resolve_cached() {
        if p.is_file() {
            return Some(p);
        }
    }

    for candidate in [
        "/tmp/rlx-weights/MiniCPM5-1B-GGUF/MiniCPM5-1B-Q4_K_M.gguf",
        "/tmp/rlx-weights/MiniCPM5-1B-GGUF/MiniCPM5-1B-Q8_0.gguf",
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

async fn collect_tokens(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<skill_llm::InferToken>,
) -> Result<(String, String, usize, usize), String> {
    let mut text = String::new();
    let mut fr = String::new();
    let (mut pt, mut ct) = (0, 0);
    while let Some(tok) = rx.recv().await {
        match tok {
            skill_llm::InferToken::Delta(t) => text.push_str(&t),
            skill_llm::InferToken::Done {
                finish_reason,
                prompt_tokens,
                completion_tokens,
                ..
            } => {
                fr = finish_reason;
                pt = prompt_tokens;
                ct = completion_tokens;
                break;
            }
            skill_llm::InferToken::Error(e) => return Err(e),
        }
    }
    Ok((text, fr, pt, ct))
}

#[test]
fn bundled_catalog_includes_minicpm5() {
    let skill_dir = std::env::temp_dir().join(format!("skill-minicpm5-catalog-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&skill_dir);
    let catalog = LlmCatalog::load(&skill_dir);
    let _ = std::fs::remove_dir_all(&skill_dir);

    let family_entries: Vec<_> = catalog
        .entries
        .iter()
        .filter(|e| e.family_id == "minicpm5-1b")
        .collect();
    assert!(
        !family_entries.is_empty(),
        "bundled catalog should include minicpm5-1b entries"
    );
    assert!(
        family_entries.iter().any(|e| e.filename == "MiniCPM5-1B-Q4_K_M.gguf"),
        "expected Q4_K_M quant in catalog"
    );
    assert_eq!(family_entries[0].family_name, "MiniCPM5 1B".to_string());
    assert_eq!(family_entries[0].repo, "openbmb/MiniCPM5-1B-GGUF".to_string());
}

#[tokio::test(flavor = "multi_thread")]
async fn minicpm5_load_and_chat() {
    let skill_dir = std::env::temp_dir().join(format!("skill-minicpm5-e2e-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&skill_dir);

    let mut catalog = LlmCatalog::load(&skill_dir);
    let Some(entry) = catalog
        .entries
        .iter()
        .find(|e| e.family_id == "minicpm5-1b" && e.filename == "MiniCPM5-1B-Q4_K_M.gguf")
        .cloned()
    else {
        panic!("MiniCPM5-1B-Q4_K_M.gguf missing from bundled catalog");
    };

    let Some(weights) = minicpm5_weights_path(&entry) else {
        eprintln!("skip: MiniCPM5 weights not found — set MINICPM5_WEIGHTS or cache openbmb/MiniCPM5-1B-GGUF");
        let _ = std::fs::remove_dir_all(&skill_dir);
        return;
    };

    eprintln!("[minicpm5] weights: {}", weights.display());
    assert!(
        looks_like_minicpm5_path(&weights),
        "weight path should match MiniCPM5 heuristics"
    );

    if let Some(e) = catalog.entries.iter_mut().find(|e| e.filename == entry.filename) {
        e.state = DownloadState::Downloaded;
        e.local_path = Some(weights.clone());
    }
    catalog.active_model = entry.filename.clone();

    let config = LlmConfig {
        enabled: true,
        n_gpu_layers: u32::MAX,
        ctx_size: Some(2048),
        ..LlmConfig::default()
    };
    let emitter: Arc<dyn LlmEventEmitter> = Arc::new(NoopEmitter);
    let log_buf = new_log_buffer();

    let load_start = Instant::now();
    let server = init(&config, &catalog, emitter, log_buf, &skill_dir).expect("init MiniCPM5 server");
    wait_ready(&server, Duration::from_secs(180));
    eprintln!(
        "[minicpm5] server ready in {:.2}s (n_ctx={})",
        load_start.elapsed().as_secs_f64(),
        server.n_ctx.load(Ordering::Relaxed)
    );

    let msgs = vec![
        json!({"role": "system", "content": "You are a helpful assistant. Answer concisely."}),
        json!({"role": "user", "content": "What is 2+2? Reply with only the number."}),
    ];
    let params = GenParams {
        max_tokens: 8,
        temperature: 0.0,
        thinking_budget: Some(0),
        ..GenParams::default()
    };

    let gen_start = Instant::now();
    let rx = server.chat(msgs, vec![], params).expect("chat accepted");
    let (text, finish, pt, ct) = collect_tokens(rx).await.expect("generation ok");
    eprintln!(
        "[minicpm5] response ({:.2}s): finish={finish} prompt={pt} completion={ct} text={text:?}",
        gen_start.elapsed().as_secs_f64()
    );

    match Arc::try_unwrap(server) {
        Ok(owned) => owned.shutdown(),
        Err(arc) => drop(arc),
    };
    let _ = std::fs::remove_dir_all(&skill_dir);

    assert!(pt > 0, "expected prompt to tokenize (got prompt_tokens={pt})");
    assert!(ct > 0, "expected model to emit tokens (got completion_tokens={ct})");
    assert!(
        finish == "length" || finish == "stop",
        "unexpected finish_reason: {finish}"
    );
    // Note: coherent answers on GGUF+CPU require upstream rlx-minicpm5 parity work;
    // this test validates skill-llm wiring (catalog → init → chat → minicpm5 family).
}

fn looks_like_minicpm5_path(path: &Path) -> bool {
    let lossy = path.to_string_lossy().to_ascii_lowercase();
    lossy.contains("minicpm5") || lossy.contains("minicpm-5")
}
