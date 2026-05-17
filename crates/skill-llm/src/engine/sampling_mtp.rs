// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! MTP (Multi-Token Prediction) speculative-decoding loop.
//!
//! Mirrors the structure of the upstream `examples/mtp/src/main.rs` in
//! llama-cpp-rs v0.2.56, adapted to skill-llm's streaming token_tx + stop
//! string conventions. Per round the loop:
//!
//!   1. `session.draft(...)` → up to `n_draft_max` speculative tokens.
//!   2. Build verify batch `[last_token, drafts...]` with `logits = true`.
//!   3. Roll back the draft context's KV before the AR pre-advance.
//!   4. `target_ctx.decode(verify)` + `session.process(verify)`.
//!   5. Sample target at each output index; find longest matching prefix.
//!   6. Roll back rejected suffix on BOTH contexts.
//!   7. `session.accept(n_accepted)` and emit accepted drafts + new token.
//!
//! Limitations vs the standard `sampling::run_sampling_loop`:
//!   - No `<think>…</think>` budget injection (interaction with multi-token
//!     rounds is non-trivial). Generation honours `thinking_budget == Some(0)`
//!     via the prefill prefix only.
//!   - No partial stop-string holdback: stop matches are detected after each
//!     round on the accumulated text. Drafted tokens past a stop boundary
//!     are still emitted within the round — acceptable for the current
//!     stop strings (all short fixed sentinels like `<|im_end|>`).

use tokio::sync::mpsc::UnboundedSender;

use llama_cpp_4::{
    llama_batch::LlamaBatch, model::Special, mtp::MtpSession, sampling::LlamaSampler, token::LlamaToken,
};

use super::generation::GpuMemoryGuard;
use super::logging::{LlmLogBuffer, LlmLogFile};
use super::protocol::{GenParams, InferToken};
use crate::event::LlmEventEmitter;

/// Run the MTP speculative-decoding loop.
///
/// Preconditions:
/// * `target_ctx` already contains the fully-decoded prompt, and the prefill
///   batch has been processed by the MTP session so it can consume pre-norm
///   embeddings.
/// * `session.process(prefill_batch)` and `session.begin(0, &tokens)` have
///   already been called by the caller.
/// * `first_token` was sampled from the prefill's last logits.
#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(super) fn run_sampling_loop_mtp(
    model: &llama_cpp_4::model::LlamaModel,
    target_ctx: &mut llama_cpp_4::context::LlamaContext<'_>,
    draft_ctx: &mut llama_cpp_4::context::LlamaContext<'_>,
    session: &mut MtpSession,
    app: &dyn LlmEventEmitter,
    log_buf: &LlmLogBuffer,
    log_file: Option<&LlmLogFile>,
    params: &GenParams,
    token_tx: UnboundedSender<InferToken>,
    n_prompt: usize,
    first_token: LlamaToken,
    gpu_guard: GpuMemoryGuard,
) {
    let n_ctx = target_ctx.n_ctx() as usize;
    let n_draft_max = session.n_draft_max();

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::top_k(params.top_k),
        LlamaSampler::top_p(params.top_p, 1),
        LlamaSampler::temp(params.temperature),
        LlamaSampler::dist(params.seed),
    ]);
    sampler.accept(first_token);

    let mut stop_strings = params.stop.clone();
    for s in &[
        "<|im_end|>",
        "<|endoftext|>",
        "<|user|>",
        "<|eot_id|>",
        "<|EOT|>",
        "[/INST]",
    ] {
        if !stop_strings.iter().any(|x| x == s) {
            stop_strings.push(s.to_string());
        }
    }

    // Emit the first token (sampled from prefill before the loop starts).
    let mut accumulated = String::new();
    let emit = |s: &str, acc: &mut String, tx: &UnboundedSender<InferToken>| -> bool {
        if s.is_empty() {
            return true;
        }
        acc.push_str(s);
        tx.send(InferToken::Delta(s.to_string())).is_ok()
    };
    let first_piece = model.token_to_str(first_token, Special::Plaintext).unwrap_or_default();
    if !emit(&first_piece, &mut accumulated, &token_tx) {
        return;
    }

    let mut last_token = first_token;
    let mut n_past = n_prompt as i32;
    let mut n_generated: usize = 1;
    let max_new = params.max_tokens.min(n_ctx.saturating_sub(n_prompt));

    let mut n_rounds: u64 = 0;
    let mut n_drafts_total: u64 = 0;
    let mut n_accepted_total: u64 = 0;
    let mut finish_reason = "length".to_string();

    let verify_cap = (n_draft_max as usize + 1).max(target_ctx.n_batch() as usize);
    let mut verify = LlamaBatch::new(verify_cap, 1);

    'gen: loop {
        if n_generated >= max_new {
            break;
        }
        if model.is_eog_token(last_token) {
            finish_reason = "stop".to_string();
            break;
        }

        // Periodic GPU memory check (every 8 rounds — each round produces
        // up to n_draft_max+1 tokens, so ~64 tokens at n_draft_max=7).
        if n_rounds.is_multiple_of(8) && gpu_guard.gen_threshold > 0.0 {
            let (mem_ok, free_gb) = super::generation::gpu_memory_check(gpu_guard.gen_threshold);
            if !mem_ok {
                llm_warn!(
                    app,
                    log_buf,
                    log_file,
                    "stopping MTP generation — GPU memory critically low \
                     ({:.2} GB free < {:.2} GB threshold)",
                    free_gb.unwrap_or(0.0),
                    gpu_guard.gen_threshold
                );
                token_tx
                    .send(InferToken::Delta(format!(
                        "\n\n*[Generation stopped: GPU memory low ({:.2} GB free).]*",
                        free_gb.unwrap_or(0.0)
                    )))
                    .ok();
                finish_reason = "gpu_memory".to_string();
                break;
            }
        }

        // 1. Get drafts.
        let drafts = match session.draft(0, n_past, last_token) {
            Ok(d) => d,
            Err(e) => {
                llm_error!(app, log_buf, log_file, "MTP draft failed: {e}");
                token_tx.send(InferToken::Error(format!("MTP draft error: {e}"))).ok();
                return;
            }
        };
        n_rounds += 1;
        n_drafts_total += drafts.len() as u64;

        // 2. Build verify batch: [last_token, drafts...].
        verify.clear();
        if verify.add(last_token, n_past, &[0], true).is_err() {
            token_tx.send(InferToken::Error("verify batch overflow".into())).ok();
            return;
        }
        for (i, d) in drafts.iter().enumerate() {
            if verify.add(*d, n_past + 1 + i as i32, &[0], true).is_err() {
                token_tx.send(InferToken::Error("verify batch overflow".into())).ok();
                return;
            }
        }
        let n_verify = verify.n_tokens();

        // 3. Roll back draft KV before re-decoding via session.process(verify).
        if let Err(e) = draft_ctx.clear_kv_cache_seq(Some(0), Some(n_past as u32), None) {
            llm_error!(app, log_buf, log_file, "draft KV rollback failed: {e}");
            token_tx.send(InferToken::Error(format!("draft KV rollback: {e}"))).ok();
            return;
        }

        // 4. Verify decode + session.process(verify).
        if let Err(e) = target_ctx.decode(&mut verify) {
            llm_error!(app, log_buf, log_file, "MTP verify decode failed: {e}");
            token_tx.send(InferToken::Error(format!("verify decode: {e}"))).ok();
            return;
        }
        if let Err(e) = session.process(&verify) {
            llm_error!(app, log_buf, log_file, "MTP process(verify) failed: {e}");
            token_tx.send(InferToken::Error(format!("MTP process: {e}"))).ok();
            return;
        }

        // 5. Sample target at output position 0 (predicts draft[0]) and walk
        //    forward as long as drafts keep matching.
        let mut n_accepted: usize = 0;
        let mut next_token = sampler.sample(target_ctx, 0);
        sampler.accept(next_token);

        for (i, draft) in drafts.iter().enumerate() {
            if next_token == *draft {
                n_accepted = i + 1;
                if i + 1 < n_verify as usize {
                    next_token = sampler.sample(target_ctx, (i + 1) as i32);
                    sampler.accept(next_token);
                }
            } else {
                break;
            }
        }
        n_accepted_total += n_accepted as u64;

        let new_n_past = n_past + 1 + n_accepted as i32;

        // 6. Roll back the rejected suffix on BOTH contexts.
        if (n_accepted as i32) < drafts.len() as i32 {
            match target_ctx.clear_kv_cache_seq(Some(0), Some(new_n_past as u32), None) {
                Ok(true) => {}
                Ok(false) => {
                    llm_error!(
                        app,
                        log_buf,
                        log_file,
                        "target ctx refused partial seq_rm at pos {new_n_past} — \
                         with_n_rs_seq(>0) must be set on the target context"
                    );
                    token_tx
                        .send(InferToken::Error("target KV rollback rejected".into()))
                        .ok();
                    return;
                }
                Err(e) => {
                    llm_error!(app, log_buf, log_file, "target KV rollback errored: {e}");
                    token_tx
                        .send(InferToken::Error(format!("target KV rollback: {e}")))
                        .ok();
                    return;
                }
            }
            match draft_ctx.clear_kv_cache_seq(Some(0), Some(new_n_past as u32), None) {
                Ok(true) => {}
                Ok(false) => {
                    llm_error!(
                        app,
                        log_buf,
                        log_file,
                        "draft ctx refused partial seq_rm at pos {new_n_past}"
                    );
                    token_tx
                        .send(InferToken::Error("draft KV rollback rejected".into()))
                        .ok();
                    return;
                }
                Err(e) => {
                    llm_error!(app, log_buf, log_file, "draft KV rollback errored: {e}");
                    token_tx.send(InferToken::Error(format!("draft KV rollback: {e}"))).ok();
                    return;
                }
            }
        }

        // 7. Tell session how many drafts were accepted.
        if let Err(e) = session.accept(0, n_accepted as u16) {
            llm_error!(app, log_buf, log_file, "MTP accept failed: {e}");
            token_tx.send(InferToken::Error(format!("MTP accept: {e}"))).ok();
            return;
        }

        // Emit accepted drafts, then the newly sampled token. Check EOG and
        // stop strings after each emission so we never decode past a stop.
        for d in drafts.iter().take(n_accepted) {
            if model.is_eog_token(*d) {
                finish_reason = "stop".to_string();
                break 'gen;
            }
            let piece = model.token_to_str(*d, Special::Plaintext).unwrap_or_default();
            if !emit(&piece, &mut accumulated, &token_tx) {
                return;
            }
            n_generated += 1;
            for s in &stop_strings {
                if accumulated.ends_with(s.as_str()) {
                    finish_reason = "stop".to_string();
                    break 'gen;
                }
            }
        }

        if model.is_eog_token(next_token) {
            finish_reason = "stop".to_string();
            break;
        }
        let next_piece = model.token_to_str(next_token, Special::Plaintext).unwrap_or_default();
        if !emit(&next_piece, &mut accumulated, &token_tx) {
            return;
        }
        n_generated += 1;
        for s in &stop_strings {
            if accumulated.ends_with(s.as_str()) {
                finish_reason = "stop".to_string();
                break 'gen;
            }
        }

        last_token = next_token;
        n_past = new_n_past;
    }

    let acceptance = if n_drafts_total == 0 {
        0.0
    } else {
        n_accepted_total as f64 / n_drafts_total as f64
    };
    llm_info!(
        app,
        log_buf,
        log_file,
        "[mtp] generation done — prompt={n_prompt} completion={n_generated} ctx={n_ctx} \
         finish={finish_reason} rounds={n_rounds} drafts={n_drafts_total} \
         accepted={n_accepted_total} ({:.1}%)",
        100.0 * acceptance
    );
    token_tx
        .send(InferToken::Done {
            finish_reason,
            prompt_tokens: n_prompt,
            completion_tokens: n_generated,
            n_ctx,
        })
        .ok();
}
