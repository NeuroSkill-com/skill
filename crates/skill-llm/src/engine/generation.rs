// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Text-only and multimodal generation entry points.

use tokio::sync::mpsc::UnboundedSender;

use llama_cpp_4::{
    context::params::{LlamaContextParams, LlamaContextType},
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::AddBos,
    mtp::MtpSession,
    sampling::LlamaSampler,
};

use super::logging::{LlmLogBuffer, LlmLogFile};
use super::protocol::{GenParams, InferToken};
use super::sampling::run_sampling_loop;
use super::sampling_mtp::run_sampling_loop_mtp;
use crate::event::LlmEventEmitter;

/// GPU memory safety thresholds (configurable via LlmConfig).
#[derive(Clone, Copy, Debug)]
pub(super) struct GpuMemoryGuard {
    /// Minimum free GB before starting a decode pass.
    pub decode_threshold: f64,
    /// Minimum free GB during token-by-token generation.
    pub gen_threshold: f64,
}

/// Check whether the system has enough free GPU/unified memory to safely run
/// a Metal/CUDA decode pass.  Returns `(ok, free_gb)` — `ok` is `true` when
/// we either cannot determine memory (optimistic) or when at least
/// `min_free_gb` is available.
pub(super) fn gpu_memory_check(min_free_gb: f64) -> (bool, Option<f64>) {
    let Some(gpu) = skill_data::gpu_stats::read() else {
        return (true, None);
    };
    let free_gb = gpu.free_memory_bytes.map(|b| b as f64 / (1024.0 * 1024.0 * 1024.0));
    let ok = free_gb.is_none_or(|f| f >= min_free_gb);
    (ok, free_gb)
}

// ── Text-only generation ───────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub(super) fn run_generation(
    model: &llama_cpp_4::model::LlamaModel,
    ctx: &mut llama_cpp_4::context::LlamaContext<'_>,
    app: &dyn LlmEventEmitter,
    log_buf: &LlmLogBuffer,
    log_file: Option<&LlmLogFile>,
    prompt: String,
    params: GenParams,
    token_tx: UnboundedSender<InferToken>,
    gpu_guard: GpuMemoryGuard,
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
    let n_ctx = ctx.n_ctx() as usize;

    llm_info!(
        app,
        log_buf,
        log_file,
        "prompt: {n_prompt} tokens, thinking_budget={:?}",
        params.thinking_budget
    );
    if n_prompt >= n_ctx {
        let msg = format!("prompt too long ({n_prompt} ≥ n_ctx {n_ctx})");
        llm_warn!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    // Guard: verify enough GPU memory is available before starting decode.
    // Metal's ggml backend will call abort() on allocation failure, which
    // kills the entire process.  By checking early we can return a
    // recoverable error instead.
    let (mem_ok, free_gb) = gpu_memory_check(gpu_guard.decode_threshold);
    if !mem_ok {
        let msg = format!(
            "Insufficient GPU memory for decode ({:.2} GB free, {:.2} GB required). \
             Reduce context size, close other GPU apps, or adjust the GPU memory threshold in Settings → LLM.",
            free_gb.unwrap_or(0.0),
            gpu_guard.decode_threshold,
        );
        llm_error!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    let n_batch = ctx.n_batch() as usize;
    let mut i = 0;
    while i < n_prompt {
        let end = (i + n_batch).min(n_prompt);
        let mut batch = LlamaBatch::new(end - i, 1);
        for (j, &token) in tokens.iter().enumerate().take(end).skip(i) {
            let logits = j == n_prompt - 1;
            if batch.add(token, j as i32, &[0], logits).is_err() {
                break;
            }
        }
        if ctx.decode(&mut batch).is_err() {
            // Metal on macOS can transiently fail (GPU busy, command buffer
            // timeout).  Clear the KV cache and retry the entire prompt once
            // before giving up.
            llm_warn!(
                app,
                log_buf,
                log_file,
                "decode failed on prompt batch at token {i} — retrying after KV cache reset"
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
            ctx.clear_kv_cache();

            // Rebuild from token 0 so KV state is consistent.
            let mut retry_ok = true;
            let mut ri = 0;
            while ri < n_prompt {
                let rend = (ri + n_batch).min(n_prompt);
                let mut rb = LlamaBatch::new(rend - ri, 1);
                for (j, &token) in tokens.iter().enumerate().take(rend).skip(ri) {
                    let logits = j == n_prompt - 1;
                    if rb.add(token, j as i32, &[0], logits).is_err() {
                        break;
                    }
                }
                if ctx.decode(&mut rb).is_err() {
                    retry_ok = false;
                    break;
                }
                ri = rend;
            }
            if !retry_ok {
                llm_error!(
                    app,
                    log_buf,
                    log_file,
                    "decode error on prompt (batch at token {i}) — retry also failed"
                );
                token_tx
                    .send(InferToken::Error(
                        "Decode error — the GPU failed to process the prompt. \
                     Try sending the message again, or restart the model in Settings → LLM."
                            .into(),
                    ))
                    .ok();
                return;
            }
            // Retry succeeded — break out of the outer loop since we
            // already processed the entire prompt.
            llm_info!(app, log_buf, log_file, "prompt decode succeeded on retry");
            break;
        }
        i = end;
    }

    run_sampling_loop(
        model, ctx, app, log_buf, log_file, &params, token_tx, n_prompt, gpu_guard,
    );
}

// ── Multimodal generation (llm-mtmd feature) ──────────────────────────────────

#[cfg(feature = "llm-mtmd")]
#[allow(clippy::too_many_arguments)]
pub(super) fn run_generation_multimodal(
    model: &llama_cpp_4::model::LlamaModel,
    ctx: &mut llama_cpp_4::context::LlamaContext<'_>,
    mtmd_ctx: &llama_cpp_4::mtmd::MtmdContext,
    app: &dyn LlmEventEmitter,
    log_buf: &LlmLogBuffer,
    log_file: Option<&LlmLogFile>,
    prompt: String,
    images: Vec<Vec<u8>>,
    params: GenParams,
    token_tx: UnboundedSender<InferToken>,
    gpu_guard: GpuMemoryGuard,
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
    let bitmaps: Vec<MtmdBitmap> = images
        .iter()
        .enumerate()
        .filter_map(|(i, bytes)| match MtmdBitmap::from_buf(mtmd_ctx, bytes) {
            Ok(b) => Some(b),
            Err(e) => {
                llm_warn!(app, log_buf, log_file, "image {i} decode failed: {e}");
                None
            }
        })
        .collect();

    if bitmaps.is_empty() && !images.is_empty() {
        token_tx
            .send(InferToken::Error("all images failed to decode".into()))
            .ok();
        return;
    }

    llm_info!(
        app,
        log_buf,
        log_file,
        "multimodal prompt — {} image(s), thinking_budget={:?}",
        bitmaps.len(),
        params.thinking_budget
    );

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

    // Guard: verify enough GPU memory before multimodal decode.
    let (mem_ok, free_gb) = gpu_memory_check(gpu_guard.decode_threshold);
    if !mem_ok {
        let msg = format!(
            "Insufficient GPU memory for multimodal decode ({:.2} GB free, {:.2} GB required). \
             Reduce context size, close other GPU apps, or adjust the GPU memory threshold in Settings → LLM.",
            free_gb.unwrap_or(0.0),
            gpu_guard.decode_threshold,
        );
        llm_error!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    let n_batch = ctx.n_batch() as i32;
    let mut n_past = 0i32;
    if let Err(e) = mtmd_ctx.eval_chunks(ctx.as_ptr(), &chunks, 0, 0, n_batch, true, &mut n_past) {
        // Retry once after KV cache reset (transient Metal failures).
        llm_warn!(
            app,
            log_buf,
            log_file,
            "mtmd eval failed: {e} — retrying after KV cache reset"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
        ctx.clear_kv_cache();
        n_past = 0;
        if let Err(e2) = mtmd_ctx.eval_chunks(ctx.as_ptr(), &chunks, 0, 0, n_batch, true, &mut n_past) {
            let msg = format!("mtmd eval error: {e2} (retry also failed, original: {e})");
            llm_error!(app, log_buf, log_file, "{msg}");
            token_tx.send(InferToken::Error(msg)).ok();
            return;
        }
        llm_info!(app, log_buf, log_file, "multimodal eval succeeded on retry");
    }

    let n_prompt = n_past as usize;
    run_sampling_loop(
        model, ctx, app, log_buf, log_file, &params, token_tx, n_prompt, gpu_guard,
    );
}

// ── MTP (Multi-Token Prediction) generation ───────────────────────────────────

/// Text-only generation with MTP speculative decoding.
///
/// Differences vs `run_generation`:
/// * Prefill enables pre-norm embeddings for MTP while only keeping logits on
///   the final prompt position used for the first sampled token.
/// * Builds a per-request `LlamaContextType::Mtp` draft context and
///   `MtpSession` from `model + backend`. Both are dropped before this
///   function returns — the cost is one set of recurrent-state allocations
///   per request, which is acceptable for the win MTP offers (per the
///   v0.2.53 fork benchmark: +6.2% on Q4_K_M with `n_draft_max=1`).
/// * Hands off to `run_sampling_loop_mtp` for the draft/verify loop.
///
/// Vision (mtmd) requests do NOT route here — they stay on the
/// non-speculative `run_generation_multimodal` path. Combining MTP with
/// mtmd is not yet validated upstream.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_generation_mtp(
    model: &llama_cpp_4::model::LlamaModel,
    target_ctx: &mut llama_cpp_4::context::LlamaContext<'_>,
    backend: &LlamaBackend,
    app: &dyn LlmEventEmitter,
    log_buf: &LlmLogBuffer,
    log_file: Option<&LlmLogFile>,
    prompt: String,
    params: GenParams,
    token_tx: UnboundedSender<InferToken>,
    gpu_guard: GpuMemoryGuard,
    n_draft_max: i32,
    n_rs_seq: u32,
) {
    target_ctx.clear_kv_cache();

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
    let n_ctx = target_ctx.n_ctx() as usize;

    llm_info!(
        app,
        log_buf,
        log_file,
        "[mtp] prompt: {n_prompt} tokens, n_draft_max={n_draft_max}, thinking_budget={:?}",
        params.thinking_budget
    );
    if n_prompt >= n_ctx {
        let msg = format!("prompt too long ({n_prompt} ≥ n_ctx {n_ctx})");
        llm_warn!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    let (mem_ok, free_gb) = gpu_memory_check(gpu_guard.decode_threshold);
    if !mem_ok {
        let msg = format!(
            "Insufficient GPU memory for decode ({:.2} GB free, {:.2} GB required).",
            free_gb.unwrap_or(0.0),
            gpu_guard.decode_threshold,
        );
        llm_error!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    // Build per-request draft context. n_ctx matches target so KV positions
    // line up. n_batch / n_ubatch are intentionally left at the library
    // default for the draft side — the draft only ever processes
    // `1 + n_draft_max` tokens per round, far below typical batch sizes.
    let draft_n_ctx =
        std::num::NonZeroU32::new(target_ctx.n_ctx()).unwrap_or_else(|| std::num::NonZeroU32::new(2048).unwrap());
    let draft_params = LlamaContextParams::default()
        .with_n_ctx(Some(draft_n_ctx))
        .with_ctx_type(LlamaContextType::Mtp)
        .with_n_rs_seq(n_rs_seq);
    let mut draft_ctx = match model.new_context(backend, draft_params) {
        Ok(c) => c,
        Err(e) => {
            llm_error!(app, log_buf, log_file, "[mtp] draft context build failed: {e}");
            token_tx
                .send(InferToken::Error(format!("MTP draft context build failed: {e}")))
                .ok();
            return;
        }
    };

    let mut session = match MtpSession::new(target_ctx, &draft_ctx, 1, n_draft_max) {
        Ok(s) => s,
        Err(e) => {
            llm_error!(app, log_buf, log_file, "[mtp] session init failed: {e}");
            token_tx
                .send(InferToken::Error(format!("MTP session init failed: {e}")))
                .ok();
            return;
        }
    };

    // ── Prefill: decode the whole prompt in one batch for session.process.
    // MTP consumes pre-norm embeddings from the batch; logits are only needed
    // on the final prompt token for the first sampled token.
    let n_batch = target_ctx.n_batch() as usize;
    let prefill_cap = n_prompt.max(n_batch);
    let mut prefill = LlamaBatch::new(prefill_cap, 1);
    // Single-batch path keeps all prompt positions visible to
    // `session.process` in one call. If the prompt exceeds n_batch we grow the
    // batch capacity here (the underlying llama_batch_init allocates per
    // LlamaBatch::new).
    for (i, tok) in tokens.iter().copied().enumerate() {
        let logits = mtp_prefill_needs_logits(i, n_prompt);
        if prefill.add(tok, i as i32, &[0], logits).is_err() {
            token_tx.send(InferToken::Error("prefill batch overflow".into())).ok();
            return;
        }
    }
    if let Err(e) = target_ctx.decode(&mut prefill) {
        llm_error!(app, log_buf, log_file, "[mtp] prefill decode failed: {e}");
        token_tx.send(InferToken::Error(format!("prefill decode: {e}"))).ok();
        return;
    }
    if let Err(e) = session.process(&prefill) {
        llm_error!(app, log_buf, log_file, "[mtp] process(prefill) failed: {e}");
        token_tx
            .send(InferToken::Error(format!("MTP process(prefill): {e}")))
            .ok();
        return;
    }
    if let Err(e) = session.begin(0, &tokens) {
        llm_error!(app, log_buf, log_file, "[mtp] session.begin failed: {e}");
        token_tx.send(InferToken::Error(format!("MTP begin: {e}"))).ok();
        return;
    }

    // Sample the first token from the prefill's last logits.
    let first_sampler = LlamaSampler::chain_simple([
        LlamaSampler::top_k(params.top_k),
        LlamaSampler::top_p(params.top_p, 1),
        LlamaSampler::temp(params.temperature),
        LlamaSampler::dist(params.seed),
    ]);
    let first_token = first_sampler.sample(target_ctx, prefill.n_tokens() - 1);
    drop(first_sampler);

    run_sampling_loop_mtp(
        model,
        target_ctx,
        &mut draft_ctx,
        &mut session,
        app,
        log_buf,
        log_file,
        &params,
        token_tx,
        n_prompt,
        first_token,
        gpu_guard,
    );

    // Explicit drop order: session BEFORE draft_ctx, since session holds a
    // raw pointer into draft_ctx's underlying llama_context_p.
    drop(session);
    drop(draft_ctx);
}

fn mtp_prefill_needs_logits(position: usize, n_prompt: usize) -> bool {
    position + 1 == n_prompt
}

#[cfg(test)]
mod tests {
    use super::mtp_prefill_needs_logits;

    #[test]
    fn mtp_prefill_logits_only_on_final_prompt_token() {
        let n_prompt = 8;
        let mask: Vec<bool> = (0..n_prompt)
            .map(|position| mtp_prefill_needs_logits(position, n_prompt))
            .collect();

        assert_eq!(mask, vec![false, false, false, false, false, false, false, true]);
    }

    #[test]
    fn mtp_prefill_logits_handles_single_token_prompt() {
        assert!(mtp_prefill_needs_logits(0, 1));
    }
}
