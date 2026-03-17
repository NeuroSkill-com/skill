// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Text-only and multimodal generation entry points.

use tokio::sync::mpsc::UnboundedSender;

use llama_cpp_4::{
    llama_batch::LlamaBatch,
    model::AddBos,
};

use crate::event::LlmEventEmitter;
use super::logging::{LlmLogBuffer, LlmLogFile};
use super::protocol::{InferToken, GenParams};
use super::sampling::run_sampling_loop;

// ── Text-only generation ───────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub(super) fn run_generation(
    model:    &llama_cpp_4::model::LlamaModel,
    ctx:      &mut llama_cpp_4::context::LlamaContext<'_>,
    app: &dyn LlmEventEmitter,
    log_buf:  &LlmLogBuffer,
    log_file: Option<&LlmLogFile>,
    prompt:   String,
    params:   GenParams,
    token_tx: UnboundedSender<InferToken>,
) {
    ctx.clear_kv_cache();

    // When thinking is disabled, pre-fill an empty <think>\n\n</think>\n block.
    let prompt = if params.thinking_budget == Some(0) {
        format!("{prompt}<think>\n\n</think>\n")
    } else {
        prompt
    };

    let Ok(tokens) = model.str_to_token(&prompt, AddBos::Always) else {
        token_tx.send(InferToken::Error("tokenization failed".into())).ok();
        return;
    };
    let n_prompt = tokens.len();
    let n_ctx    = ctx.n_ctx() as usize;

    llm_info!(app, log_buf, log_file, "prompt: {n_prompt} tokens, thinking_budget={:?}", params.thinking_budget);
    if n_prompt >= n_ctx {
        let msg = format!("prompt too long ({n_prompt} ≥ n_ctx {n_ctx})");
        llm_warn!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    let mut batch = LlamaBatch::new(n_ctx, 1);
    for (i, &tok) in tokens.iter().enumerate() {
        if batch.add(tok, i as i32, &[0], i == n_prompt - 1).is_err() { break; }
    }
    if ctx.decode(&mut batch).is_err() {
        llm_error!(app, log_buf, log_file, "decode error on prompt");
        token_tx.send(InferToken::Error("decode error on prompt".into())).ok();
        return;
    }

    run_sampling_loop(model, ctx, app, log_buf, log_file, &params, token_tx, n_prompt);
}

// ── Multimodal generation (llm-mtmd feature) ──────────────────────────────────

#[cfg(feature = "llm-mtmd")]
#[allow(clippy::too_many_arguments)]
pub(super) fn run_generation_multimodal(
    model:     &llama_cpp_4::model::LlamaModel,
    ctx:       &mut llama_cpp_4::context::LlamaContext<'_>,
    mtmd_ctx:  &llama_cpp_4::mtmd::MtmdContext,
    app: &dyn LlmEventEmitter,
    log_buf:   &LlmLogBuffer,
    log_file:  Option<&LlmLogFile>,
    prompt:    String,
    images:    Vec<Vec<u8>>,
    params:    GenParams,
    token_tx:  UnboundedSender<InferToken>,
) {
    use llama_cpp_4::mtmd::{MtmdBitmap, MtmdInputChunks, MtmdInputText};

    ctx.clear_kv_cache();

    let n_ctx = ctx.n_ctx() as usize;

    // When thinking is disabled, pre-fill an empty <think>\n\n</think>\n block.
    let prompt = if params.thinking_budget == Some(0) {
        format!("{prompt}<think>\n\n</think>\n")
    } else {
        prompt
    };

    // Decode raw bytes → MtmdBitmap (auto-detects JPEG/PNG/etc.)
    let bitmaps: Vec<MtmdBitmap> = images.iter()
        .enumerate()
        .filter_map(|(i, bytes)| {
            match MtmdBitmap::from_buf(mtmd_ctx, bytes) {
                Ok(b)  => Some(b),
                Err(e) => {
                    llm_warn!(app, log_buf, log_file, "image {i} decode failed: {e}");
                    None
                }
            }
        })
        .collect();

    if bitmaps.is_empty() && !images.is_empty() {
        token_tx.send(InferToken::Error("all images failed to decode".into())).ok();
        return;
    }

    llm_info!(app, log_buf, log_file,
        "multimodal prompt — {} image(s), thinking_budget={:?}",
        bitmaps.len(), params.thinking_budget);

    let bitmap_refs: Vec<&MtmdBitmap> = bitmaps.iter().collect();
    let text = MtmdInputText::new(&prompt, true, true);
    let mut chunks = MtmdInputChunks::new();

    if let Err(e) = mtmd_ctx.tokenize(&text, &bitmap_refs, &mut chunks) {
        let msg = format!("mtmd tokenize error: {e}");
        llm_error!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    let n_tokens = chunks.n_tokens();
    llm_info!(app, log_buf, log_file, "prompt+images: ~{n_tokens} tokens");
    if n_tokens >= n_ctx {
        let msg = format!("prompt+images too long ({n_tokens} ≥ n_ctx {n_ctx})");
        llm_warn!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    let n_batch = ctx.n_batch() as i32;
    let mut n_past = 0i32;
    if let Err(e) = mtmd_ctx.eval_chunks(ctx.as_ptr(), &chunks, 0, 0, n_batch, true, &mut n_past) {
        let msg = format!("mtmd eval error: {e}");
        llm_error!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    let n_prompt = n_past as usize;
    run_sampling_loop(model, ctx, app, log_buf, log_file, &params, token_tx, n_prompt);
}
