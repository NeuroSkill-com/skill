// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! RLX text-generation backend — fully independent of llama-cpp.
//!
//! Routes through [`rlx_models::run::auto_runner`] which auto-detects
//! the model family from GGUF metadata / safetensors config and returns
//! a `Box<dyn LmRunner>`. Covers Qwen3 / Qwen3.5 / Qwen3.6 (incl. MTP
//! speculative-decode auto-dispatch), Gemma 1-4, Llama32-shaped
//! families (Phi/Mistral/Granite/Cohere/Bonsai/GPT-OSS/OmniCoder),
//! plus the in-tree SSM families (MiniMax / LFM2.5 / Nemotron-H
//! hybrid) via their per-family builders below.
//!
//! Uses [`rlx_models::run::auto_tokenize`] and [`auto_detokenize`]
//! for prompt encoding / streaming decode — no native C++ dependency.

use anyhow::{anyhow, Result};
use rlx_models::run::{auto_detokenize, auto_runner_with_mmproj, auto_tokenize};
use rlx_models::LmRunner;
use std::path::PathBuf;
use tokio::sync::mpsc::UnboundedSender;

use super::protocol::{GenParams, InferToken};
use crate::config::LlmConfig;

pub(super) struct RlxTextRunner {
    runner: Box<dyn LmRunner>,
    family: &'static str,
    weights_path: PathBuf,
    explicit_tokenizer: Option<PathBuf>,
}

impl RlxTextRunner {
    pub(super) fn load(model_path: &std::path::Path, _config: &LlmConfig) -> Result<Self> {
        Self::load_with_mmproj(model_path, None, _config)
    }

    /// Like [`load`] but attaches an mmproj vision encoder (e.g. for
    /// Qwen3.5-VL). When `mmproj` is `None` the runner is text-only.
    pub(super) fn load_with_mmproj(
        model_path: &std::path::Path,
        mmproj: Option<&std::path::Path>,
        _config: &LlmConfig,
    ) -> Result<Self> {
        let runner = auto_runner_with_mmproj(model_path, mmproj).map_err(|e| anyhow!("RLX auto_runner: {e}"))?;
        let family = runner.family();
        Ok(Self {
            runner,
            family,
            weights_path: model_path.to_path_buf(),
            explicit_tokenizer: None,
        })
    }

    pub(super) fn family(&self) -> &'static str {
        self.family
    }

    pub(super) fn supports_multimodal(&self) -> bool {
        self.runner.supports_multimodal()
    }

    pub(super) fn generate(&mut self, prompt: &str, params: GenParams, token_tx: UnboundedSender<InferToken>) {
        let prompt_ids = match auto_tokenize(&self.weights_path, prompt, self.explicit_tokenizer.as_deref()) {
            Ok(ids) => ids,
            Err(e) => {
                token_tx
                    .send(InferToken::Error(format!("RLX tokenization failed: {e}")))
                    .ok();
                return;
            }
        };
        if prompt_ids.is_empty() {
            token_tx
                .send(InferToken::Error(
                    "RLX prompt tokenization returned no usable tokens".into(),
                ))
                .ok();
            return;
        }

        // Streaming decode strategy: keep all generated ids, decode the
        // full vector each step, emit only the suffix that wasn't sent
        // before. This handles multi-byte UTF-8 codepoints split across
        // byte-level BPE tokens correctly (decoding ids individually
        // would emit broken codepoints). O(n²) in token count but
        // negligible for typical max_tokens (≤2048).
        let mut all_ids: Vec<u32> = Vec::with_capacity(params.max_tokens.min(4096));
        let mut emitted_len: usize = 0;
        let mut completion_tokens = 0usize;
        let max_tokens = params.max_tokens;
        let stop = params.stop.clone();
        let stop_for_cb = stop.clone();
        let weights = self.weights_path.clone();
        let explicit = self.explicit_tokenizer.clone();
        let token_tx_inner = token_tx.clone();
        // Track the full accumulated text for stop-string matching.
        let mut accumulated_text = String::new();

        let mut on_token = |tok: u32| -> bool {
            completion_tokens += 1;
            all_ids.push(tok);
            // Decode the full sequence and emit the new suffix.
            let decoded = match auto_detokenize(&weights, &all_ids, explicit.as_deref(), true) {
                Ok(s) => s,
                Err(_) => return true,
            };
            if decoded.len() > emitted_len {
                let piece = decoded[emitted_len..].to_string();
                emitted_len = decoded.len();
                if !piece.is_empty() {
                    accumulated_text.push_str(&piece);
                    token_tx_inner.send(InferToken::Delta(piece)).ok();
                }
            }
            for s in &stop_for_cb {
                if !s.is_empty() && accumulated_text.ends_with(s) {
                    return false;
                }
            }
            true
        };

        let result = self
            .runner
            .generate(&prompt_ids, max_tokens, &mut on_token as &mut dyn FnMut(u32) -> bool);

        if let Err(e) = result {
            token_tx
                .send(InferToken::Error(format!("RLX generation failed: {e}")))
                .ok();
            return;
        }

        let finish_reason = if stop.iter().any(|s| !s.is_empty() && accumulated_text.ends_with(s)) {
            "stop"
        } else {
            "length"
        };
        token_tx
            .send(InferToken::Done {
                finish_reason: finish_reason.into(),
                prompt_tokens: prompt_ids.len(),
                completion_tokens,
                n_ctx: prompt_ids.len().saturating_add(completion_tokens),
            })
            .ok();
    }

    /// Multimodal generation — decodes the first image to RGB and
    /// hands it off to the runner's [`LmRunner::generate_multimodal`]
    /// (currently the Qwen3.5 family path). Additional images beyond
    /// the first are ignored (matches llama-cpp's first-image behaviour
    /// for single-frame chat). Streams decoded text via `token_tx`.
    pub(super) fn generate_multimodal(
        &mut self,
        prompt: &str,
        images: &[Vec<u8>],
        params: GenParams,
        token_tx: UnboundedSender<InferToken>,
    ) {
        if !self.runner.supports_multimodal() {
            token_tx
                .send(InferToken::Error(
                    "this RLX model has no mmproj vision encoder attached".into(),
                ))
                .ok();
            return;
        }
        let Some(first) = images.first() else {
            // Empty image list — fall back to text-only path.
            return self.generate(prompt, params, token_tx);
        };
        let img = match image::load_from_memory(first) {
            Ok(i) => i.to_rgb8(),
            Err(e) => {
                token_tx
                    .send(InferToken::Error(format!("RLX image decode failed: {e}")))
                    .ok();
                return;
            }
        };
        let (img_w, img_h) = (img.width() as usize, img.height() as usize);
        let rgb = img.into_raw();

        let max_tokens = params.max_tokens;
        let stop = params.stop.clone();
        let stop_for_cb = stop.clone();
        let mut completion_tokens = 0usize;
        let mut all_ids: Vec<u32> = Vec::with_capacity(max_tokens.min(4096));
        let mut emitted_len: usize = 0;
        let mut accumulated_text = String::new();
        let weights = self.weights_path.clone();
        let explicit = self.explicit_tokenizer.clone();
        let token_tx_inner = token_tx.clone();

        let mut on_token = |tok: u32| -> bool {
            completion_tokens += 1;
            all_ids.push(tok);
            let decoded = match auto_detokenize(&weights, &all_ids, explicit.as_deref(), true) {
                Ok(s) => s,
                Err(_) => return true,
            };
            if decoded.len() > emitted_len {
                let piece = decoded[emitted_len..].to_string();
                emitted_len = decoded.len();
                if !piece.is_empty() {
                    accumulated_text.push_str(&piece);
                    token_tx_inner.send(InferToken::Delta(piece)).ok();
                }
            }
            for s in &stop_for_cb {
                if !s.is_empty() && accumulated_text.ends_with(s) {
                    return false;
                }
            }
            true
        };

        let result = self.runner.generate_multimodal(
            prompt,
            &rgb,
            img_w,
            img_h,
            self.explicit_tokenizer.as_deref(),
            max_tokens,
            &mut on_token as &mut dyn FnMut(u32) -> bool,
        );
        if let Err(e) = result {
            token_tx
                .send(InferToken::Error(format!("RLX multimodal generation failed: {e}")))
                .ok();
            return;
        }
        let finish_reason = if stop.iter().any(|s| !s.is_empty() && accumulated_text.ends_with(s)) {
            "stop"
        } else {
            "length"
        };
        token_tx
            .send(InferToken::Done {
                finish_reason: finish_reason.into(),
                prompt_tokens: 0,
                completion_tokens,
                n_ctx: completion_tokens,
            })
            .ok();
    }
}
