// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Token-by-token sampling loop with stop-string hold-back buffer.

use tokio::sync::mpsc::UnboundedSender;

use llama_cpp_4::{
    llama_batch::LlamaBatch,
    model::{AddBos, Special},
    sampling::LlamaSampler,
};

use crate::event::LlmEventEmitter;
use super::generation::GpuMemoryGuard;
use super::logging::{LlmLogBuffer, LlmLogFile};
use super::protocol::{InferToken, GenParams};
use super::think_tracker::ThinkTracker;

/// Run the token-by-token generation loop starting at `n_prompt` KV positions.
///
/// Precondition: the KV cache already contains the fully-decoded prompt (text
/// or text+images) and the logits for the last prompt position are valid.
/// `sampler.sample(ctx, -1)` samples from those logits.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_sampling_loop(
    model:    &llama_cpp_4::model::LlamaModel,
    ctx:      &mut llama_cpp_4::context::LlamaContext<'_>,
    app: &dyn LlmEventEmitter,
    log_buf:  &LlmLogBuffer,
    log_file: Option<&LlmLogFile>,
    params:   &GenParams,
    token_tx: UnboundedSender<InferToken>,
    n_prompt: usize,
    gpu_guard: GpuMemoryGuard,
) {
    let n_ctx = ctx.n_ctx() as usize;
    let n_batch = ctx.n_batch() as usize; let _ = n_batch; // available for future use

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::top_k(params.top_k),
        LlamaSampler::top_p(params.top_p, 1),
        LlamaSampler::temp(params.temperature),
        LlamaSampler::dist(params.seed),
    ]);

    // Stop strings: user-supplied + model-family defaults.
    let mut stop_strings = params.stop.clone();
    for s in &["<|im_end|>", "<|endoftext|>", "<|user|>",
                "<|eot_id|>", "<|EOT|>", "[/INST]"] {
        if !stop_strings.iter().any(|x| x == s) {
            stop_strings.push(s.to_string());
        }
    }
    let max_stop_len = stop_strings.iter().map(|s| s.len()).max().unwrap_or(0);
    let hold_back    = max_stop_len.saturating_sub(1);

    // Think-budget tracker (budget=0 is handled before this call; None = unlimited)
    let tracker_budget = match params.thinking_budget {
        Some(0) | None => None,
        Some(n)        => Some(n),
    };
    let mut think_tracker = ThinkTracker::new(tracker_budget);

    let max_new = params.max_tokens.min(n_ctx.saturating_sub(n_prompt));
    let mut n_cur = n_prompt;
    let mut finish_reason = "length".to_string();
    let mut pending = String::new();
    // After a forced </think> injection, discard tokens (still decoded into KV
    // cache for coherence) until the model reaches a clean line break.
    let mut discard_until_nl = false;

    'gen: loop {
        if n_cur >= n_prompt + max_new { break; }

        // -1 = "last token that had logits computed"
        let token = sampler.sample(ctx, -1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            finish_reason = "stop".to_string();
            break;
        }

        let piece = model.token_to_str(token, Special::Plaintext).unwrap_or_default();

        // After forced </think> injection: decode token into KV cache for
        // coherence, but suppress it from the output stream.
        if discard_until_nl {
            if piece.contains('\n') {
                discard_until_nl = false;
            }
            let mut b = LlamaBatch::new(1, 1);
            b.add(token, n_cur as i32, &[0], true).ok();
            if ctx.decode(&mut b).is_err() {
                token_tx.send(InferToken::Error("decode error".into())).ok();
                break;
            }
            n_cur += 1;
            continue;
        }

        // Think-budget enforcement: inject </think> when budget exhausted.
        if let Some(inject) = think_tracker.feed(&piece) {
            token_tx.send(InferToken::Delta(inject.clone())).ok();

            if let Ok(inj_toks) = model.str_to_token(&inject, AddBos::Never) {
                if !inj_toks.is_empty() {
                    let mut inj_batch = LlamaBatch::new(inj_toks.len(), 1);
                    for (i, &t) in inj_toks.iter().enumerate() {
                        inj_batch.add(t, n_cur as i32 + i as i32, &[0],
                                      i == inj_toks.len() - 1).ok();
                    }
                    if ctx.decode(&mut inj_batch).is_err() {
                        llm_warn!(app, log_buf, log_file, "decode error injecting </think>");
                    }
                    n_cur += inj_toks.len();
                }
            }
            discard_until_nl = true;
            let mut b = LlamaBatch::new(1, 1);
            b.add(token, n_cur as i32, &[0], true).ok();
            if ctx.decode(&mut b).is_err() {
                token_tx.send(InferToken::Error("decode error".into())).ok();
                break;
            }
            n_cur += 1;
            continue;
        }

        pending.push_str(&piece);

        // Check for stop strings.
        for stop in &stop_strings {
            if pending.ends_with(stop.as_str()) {
                let safe_end = pending.len().saturating_sub(stop.len());
                if safe_end > 0 {
                    token_tx.send(InferToken::Delta(pending[..safe_end].to_string())).ok();
                }
                finish_reason = "stop".to_string();
                break 'gen;
            }
        }

        // Emit safe prefix (hold back potential partial stop string).
        if pending.len() > hold_back {
            let emit_end = pending.len() - hold_back;
            let emit_end = (0..=emit_end).rev()
                .find(|&i| pending.is_char_boundary(i))
                .unwrap_or(0);
            if emit_end > 0 {
                let chunk: String = pending.drain(..emit_end).collect();
                if token_tx.send(InferToken::Delta(chunk)).is_err() { break; }
            }
        }

        // Decode the new token so `sampler.sample(ctx, -1)` works next iteration.
        // Periodic GPU memory check (every 64 tokens) to avoid Metal abort().
        if n_cur % 64 == 0 && gpu_guard.gen_threshold > 0.0 {
            let (mem_ok, free_gb) = super::generation::gpu_memory_check(gpu_guard.gen_threshold);
            if !mem_ok {
                llm_warn!(app, log_buf, log_file,
                    "stopping generation — GPU memory critically low ({:.2} GB free < {:.2} GB threshold)",
                    free_gb.unwrap_or(0.0), gpu_guard.gen_threshold);
                token_tx.send(InferToken::Delta(
                    format!("\n\n*[Generation stopped: GPU memory low ({:.2} GB free). \
                             Adjust threshold in Settings → LLM.]*",
                        free_gb.unwrap_or(0.0))
                )).ok();
                finish_reason = "gpu_memory".to_string();
                break;
            }
        }
        let mut gen_batch = LlamaBatch::new(1, 1);
        if gen_batch.add(token, n_cur as i32, &[0], true).is_err() { break; }
        if ctx.decode(&mut gen_batch).is_err() {
            token_tx.send(InferToken::Error("decode error".into())).ok();
            break;
        }
        n_cur += 1;
    }

    // Flush hold-back buffer, trimming any trailing stop string.
    let flush_end = stop_strings.iter()
        .find_map(|s| pending.ends_with(s.as_str()).then_some(pending.len().saturating_sub(s.len())))
        .unwrap_or(pending.len());
    if flush_end > 0 {
        token_tx.send(InferToken::Delta(pending[..flush_end].to_string())).ok();
    }

    let n_gen = n_cur.saturating_sub(n_prompt);
    llm_info!(app, log_buf, log_file,
        "generation done — prompt={n_prompt} completion={n_gen} ctx={n_ctx} finish={finish_reason}");
    token_tx.send(InferToken::Done {
        finish_reason,
        prompt_tokens:     n_prompt,
        completion_tokens: n_gen,
        n_ctx,
    }).ok();
}
