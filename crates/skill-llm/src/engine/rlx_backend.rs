// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! RLX text-generation backend — fully independent of llama-cpp.
//!
//! Routes through [`rlx_models::run::auto_runner_with_mmproj`] for the
//! families it knows (Qwen3 / Qwen3.5 / Qwen3.6 incl. MTP, Llama32-shaped
//! stacks, LFM2.5). Catalog families that need an explicit builder before
//! the generic auto path — Gemma 3/4, MiniCPM5, MiniMax M2.x, Nemotron-H
//! — are wired below via per-family runners.
//!
//! Uses [`rlx_models::run::auto_tokenize`] and [`auto_detokenize`]
//! for prompt encoding / streaming decode — no native C++ dependency.

use anyhow::{anyhow, Result};
use rlx::gguf::{GgufFile, MetaValue};
use rlx_models::run::{auto_detokenize, auto_runner_with_mmproj, auto_tokenize};
use rlx_models::LmRunner;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc::UnboundedSender;

use super::protocol::{GenParams, InferToken};
use crate::config::LlmConfig;

fn peek_gguf_arch(path: &Path) -> Option<String> {
    let raw = GgufFile::from_path(path).ok()?;
    raw.metadata
        .get("general.architecture")
        .and_then(MetaValue::as_str)
        .map(str::to_string)
}

fn filename_hint(path: &Path, needle: &str) -> bool {
    path.to_string_lossy().to_ascii_lowercase().contains(needle)
}

fn argmax_u32(logits: &[f32]) -> u32 {
    let mut best = 0usize;
    let mut best_v = f32::NEG_INFINITY;
    for (i, &v) in logits.iter().enumerate() {
        if v > best_v {
            best_v = v;
            best = i;
        }
    }
    best as u32
}

/// Thin [`LmRunner`] adapter over [`rlx_minicpm5::MiniCpm5Runner`].
struct MiniCpm5LmRunner(rlx_minicpm5::MiniCpm5Runner);

impl LmRunner for MiniCpm5LmRunner {
    fn family(&self) -> &'static str {
        "minicpm5"
    }

    fn vocab_size(&self) -> usize {
        self.0.llama_config().vocab_size
    }

    fn predict_logits(&mut self, prompt_ids: &[u32]) -> Result<Vec<f32>> {
        self.0.predict_logits(prompt_ids)
    }

    fn generate(
        &mut self,
        prompt_ids: &[u32],
        n_new: usize,
        on_token: &mut dyn FnMut(u32) -> bool,
    ) -> Result<Vec<u32>> {
        self.0.generate(prompt_ids, n_new, |tok| {
            let _ = on_token(tok);
        })
    }
}

/// Thin [`LmRunner`] adapter over [`rlx_minimax::MiniMaxRunner`].
struct MiniMaxLmRunner(rlx_minimax::MiniMaxRunner);

impl LmRunner for MiniMaxLmRunner {
    fn family(&self) -> &'static str {
        "minimax"
    }

    fn vocab_size(&self) -> usize {
        self.0.config().vocab_size
    }

    fn predict_logits(&mut self, prompt_ids: &[u32]) -> Result<Vec<f32>> {
        if prompt_ids.is_empty() {
            return Err(anyhow!("MiniMaxLmRunner::predict_logits: empty prompt"));
        }
        self.0.reset_state();
        let mut last = Vec::new();
        for &t in prompt_ids {
            last = self.0.step(t);
        }
        Ok(last)
    }

    fn generate(
        &mut self,
        prompt_ids: &[u32],
        n_new: usize,
        on_token: &mut dyn FnMut(u32) -> bool,
    ) -> Result<Vec<u32>> {
        self.0.reset_state();
        let mut last = Vec::new();
        for &t in prompt_ids {
            last = self.0.step(t);
        }
        let mut out = Vec::with_capacity(n_new);
        for _ in 0..n_new {
            let next = argmax_u32(&last);
            out.push(next);
            if !on_token(next) {
                break;
            }
            last = self.0.step(next);
        }
        Ok(out)
    }
}

/// Thin [`LmRunner`] adapter over [`rlx_nemotron::NemotronHybridRunner`].
struct NemotronHybridLmRunner(rlx_nemotron::NemotronHybridRunner);

impl LmRunner for NemotronHybridLmRunner {
    fn family(&self) -> &'static str {
        "nemotron_h"
    }

    fn vocab_size(&self) -> usize {
        self.0.config().vocab_size
    }

    fn predict_logits(&mut self, prompt_ids: &[u32]) -> Result<Vec<f32>> {
        if prompt_ids.is_empty() {
            return Err(anyhow!("NemotronHybridLmRunner::predict_logits: empty prompt"));
        }
        self.0.reset_state();
        let mut last = Vec::new();
        for &t in prompt_ids {
            last = self.0.step(t);
        }
        Ok(last)
    }

    fn generate(
        &mut self,
        prompt_ids: &[u32],
        n_new: usize,
        on_token: &mut dyn FnMut(u32) -> bool,
    ) -> Result<Vec<u32>> {
        self.0.reset_state();
        let mut last = Vec::new();
        for &t in prompt_ids {
            last = self.0.step(t);
        }
        let mut out = Vec::with_capacity(n_new);
        for _ in 0..n_new {
            let next = argmax_u32(&last);
            out.push(next);
            if !on_token(next) {
                break;
            }
            last = self.0.step(next);
        }
        Ok(out)
    }
}

/// Dense `nemotron` arch — delegates to the inner [`Llama32Runner`].
struct NemotronLmRunner(rlx_nemotron::NemotronRunner);

impl LmRunner for NemotronLmRunner {
    fn family(&self) -> &'static str {
        "nemotron"
    }

    fn vocab_size(&self) -> usize {
        self.0.config().vocab_size
    }

    fn predict_logits(&mut self, prompt_ids: &[u32]) -> Result<Vec<f32>> {
        LmRunner::predict_logits(self.0.inner_mut(), prompt_ids)
    }

    fn generate(
        &mut self,
        prompt_ids: &[u32],
        n_new: usize,
        on_token: &mut dyn FnMut(u32) -> bool,
    ) -> Result<Vec<u32>> {
        LmRunner::generate(self.0.inner_mut(), prompt_ids, n_new, on_token)
    }
}

fn looks_like_minicpm5(path: &Path) -> bool {
    if filename_hint(path, "minicpm5") || filename_hint(path, "minicpm-5") {
        return true;
    }
    let Ok(cfg) = rlx_minicpm5::config::llama_config_from_hf(path) else {
        return false;
    };
    let preset = rlx_minicpm5::config::minicpm5_1b_preset();
    cfg.hidden_size == preset.hidden_size
        && cfg.num_hidden_layers == preset.num_hidden_layers
        && cfg.vocab_size == preset.vocab_size
}

fn looks_like_minimax(path: &Path) -> bool {
    if filename_hint(path, "minimax") {
        return true;
    }
    matches!(
        peek_gguf_arch(path).as_deref(),
        Some("minimax" | "minimax-m2" | "minimax_m2")
    )
}

fn looks_like_nemotron(path: &Path) -> bool {
    if filename_hint(path, "nemotron") {
        return true;
    }
    matches!(
        peek_gguf_arch(path).as_deref(),
        Some("nemotron" | "nemotron_h" | "nemotron_h_moe" | "nemotron-h")
    )
}

fn looks_like_gemma(path: &Path) -> bool {
    if filename_hint(path, "gemma") {
        return true;
    }
    matches!(
        peek_gguf_arch(path).as_deref(),
        Some(
            "gemma"
                | "gemma2"
                | "gemma3"
                | "gemma3n"
                | "gemma4"
                | "gemma4moe"
                | "gemma4_unified"
                | "gemma4_unified_text"
        )
    )
}

fn try_minicpm5_runner(path: &Path) -> Option<Result<Box<dyn LmRunner>>> {
    if !looks_like_minicpm5(path) {
        return None;
    }
    Some(
        rlx_minicpm5::MiniCpm5Runner::builder()
            .weights(path)
            .build()
            .map(|runner| Box::new(MiniCpm5LmRunner(runner)) as Box<dyn LmRunner>),
    )
}

fn try_minimax_runner(path: &Path) -> Option<Result<Box<dyn LmRunner>>> {
    if !looks_like_minimax(path) {
        return None;
    }
    Some(
        rlx_minimax::MiniMaxRunner::builder()
            .weights(path)
            .build()
            .map(|runner| Box::new(MiniMaxLmRunner(runner)) as Box<dyn LmRunner>),
    )
}

fn try_nemotron_runner(path: &Path) -> Option<Result<Box<dyn LmRunner>>> {
    if !looks_like_nemotron(path) {
        return None;
    }
    let arch = peek_gguf_arch(path);
    if matches!(arch.as_deref(), Some("nemotron_h" | "nemotron_h_moe" | "nemotron-h")) {
        return Some(
            rlx_nemotron::NemotronHybridRunner::builder()
                .weights(path)
                .build()
                .map(|runner| Box::new(NemotronHybridLmRunner(runner)) as Box<dyn LmRunner>),
        );
    }
    if arch.as_deref() == Some("nemotron") {
        return Some(
            rlx_nemotron::NemotronRunner::builder()
                .weights(path)
                .build()
                .map(|runner| Box::new(NemotronLmRunner(runner)) as Box<dyn LmRunner>),
        );
    }
    // Filename hinted nemotron but arch unknown — try hybrid first, then dense.
    Some(
        rlx_nemotron::NemotronHybridRunner::builder()
            .weights(path)
            .build()
            .map(|runner| Box::new(NemotronHybridLmRunner(runner)) as Box<dyn LmRunner>)
            .or_else(|_| {
                rlx_nemotron::NemotronRunner::builder()
                    .weights(path)
                    .build()
                    .map(|runner| Box::new(NemotronLmRunner(runner)) as Box<dyn LmRunner>)
            }),
    )
}

fn try_gemma_runner(path: &Path) -> Option<Result<Box<dyn LmRunner>>> {
    if !looks_like_gemma(path) {
        return None;
    }
    Some(
        rlx_gemma::GemmaRunner::builder()
            .weights(path)
            .build()
            .map(|runner| Box::new(runner) as Box<dyn LmRunner>),
    )
}

fn try_catalog_runner(path: &Path) -> Option<Result<Box<dyn LmRunner>>> {
    try_gemma_runner(path)
        .or_else(|| try_minicpm5_runner(path))
        .or_else(|| try_minimax_runner(path))
        .or_else(|| try_nemotron_runner(path))
}

fn resolve_gemma_tokenizer(weights: &Path) -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("GEMMA_TOKENIZER") {
        let p = PathBuf::from(raw);
        if p.is_file() {
            return Some(p);
        }
    }
    rlx_gemma::resolve_tokenizer_path(weights, None)
}

fn resolve_minicpm5_tokenizer(weights: &Path) -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("MINICPM5_TOKENIZER") {
        let p = PathBuf::from(raw);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(parent) = weights.parent() {
        let sibling = parent.join("tokenizer.json");
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    use hf_hub::{Cache, Repo};
    let cache = Cache::from_env();
    cache
        .repo(Repo::model("openbmb/MiniCPM5-1B".to_string()))
        .get("tokenizer.json")
}

pub(super) struct RlxTextRunner {
    runner: Box<dyn LmRunner>,
    family: &'static str,
    weights_path: PathBuf,
    explicit_tokenizer: Option<PathBuf>,
}

impl RlxTextRunner {
    pub(super) fn load(model_path: &Path, _config: &LlmConfig) -> Result<Self> {
        Self::load_with_mmproj(model_path, None, _config)
    }

    /// Like [`load`] but attaches an mmproj vision encoder (e.g. for
    /// Qwen3.5-VL). When `mmproj` is `None` the runner is text-only.
    pub(super) fn load_with_mmproj(model_path: &Path, mmproj: Option<&Path>, _config: &LlmConfig) -> Result<Self> {
        if let Some(result) = try_catalog_runner(model_path) {
            let runner = result?;
            let family = runner.family();
            let explicit_tokenizer = match family {
                "gemma" => resolve_gemma_tokenizer(model_path),
                "minicpm5" => resolve_minicpm5_tokenizer(model_path),
                _ => None,
            };
            return Ok(Self {
                runner,
                family,
                weights_path: model_path.to_path_buf(),
                explicit_tokenizer,
            });
        }

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
