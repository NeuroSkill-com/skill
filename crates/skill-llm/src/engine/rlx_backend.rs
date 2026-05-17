// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Experimental RLX text-generation adapter.

use anyhow::{anyhow, Result};
use llama_cpp_4::{
    model::{AddBos, LlamaModel, Special},
    token::LlamaToken,
};
use tokio::sync::mpsc::UnboundedSender;

use super::protocol::{GenParams, InferToken};
use crate::config::LlmConfig;

pub(super) struct RlxTextRunner {
    runner: rlx::run::Qwen3Runner,
}

impl RlxTextRunner {
    pub(super) fn load(model_path: &std::path::Path, config: &LlmConfig) -> Result<Self> {
        let device = parse_device(&config.rlx_device)?;
        let mut builder = rlx::run::Qwen3Runner::builder()
            .weights(model_path)
            .device(device)
            .max_seq(config.rlx_max_seq)
            .precision(rlx::run::Qwen3Precision::F32)
            .stream(true)
            .sample(sample_opts(&GenParams::default()));

        if let Some(gb) = config.rlx_max_memory_gb {
            builder = builder.max_memory_gb(gb);
        }

        Ok(Self {
            runner: builder.build()?,
        })
    }

    pub(super) fn generate(
        &mut self,
        tokenizer: &LlamaModel,
        prompt: &str,
        params: GenParams,
        token_tx: UnboundedSender<InferToken>,
    ) {
        let prompt_tokens = match tokenizer.str_to_token(prompt, AddBos::Always) {
            Ok(tokens) => tokens,
            Err(e) => {
                token_tx
                    .send(InferToken::Error(format!("RLX tokenization failed: {e}")))
                    .ok();
                return;
            }
        };

        let prompt_ids: Vec<u32> = prompt_tokens
            .iter()
            .filter_map(|tok| u32::try_from(tok.0).ok())
            .collect();
        if prompt_ids.is_empty() {
            token_tx
                .send(InferToken::Error(
                    "RLX prompt tokenization returned no usable tokens".into(),
                ))
                .ok();
            return;
        }

        let mut text = String::new();
        let mut completion_tokens = 0usize;
        let max_tokens = params.max_tokens;
        let stop = params.stop.clone();

        let result = self.runner.generate(&prompt_ids, max_tokens, |tok| {
            completion_tokens += 1;
            let piece = tokenizer
                .token_to_str(LlamaToken(tok as i32), Special::Plaintext)
                .unwrap_or_default();
            if piece.is_empty() {
                return;
            }
            text.push_str(&piece);
            token_tx.send(InferToken::Delta(piece)).ok();
        });

        if let Err(e) = result {
            token_tx
                .send(InferToken::Error(format!("RLX generation failed: {e}")))
                .ok();
            return;
        }

        let finish_reason = if stop.iter().any(|s| !s.is_empty() && text.ends_with(s)) {
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
}

fn sample_opts(params: &GenParams) -> rlx::models::qwen3::SampleOpts {
    let opts = if params.temperature <= 0.0 {
        rlx::models::qwen3::SampleOpts::greedy()
    } else {
        rlx::models::qwen3::SampleOpts::temperature(params.temperature, params.seed as u64)
    };
    opts.with_top_k(params.top_k.max(0) as usize)
        .with_top_p(params.top_p.clamp(0.0, 1.0))
}

fn parse_device(tag: &str) -> Result<rlx::Device> {
    match tag.to_ascii_lowercase().as_str() {
        "cpu" => Ok(rlx::Device::Cpu),
        "metal" => Ok(rlx::Device::Metal),
        "mlx" => Ok(rlx::Device::Mlx),
        "gpu" | "wgpu" => Ok(rlx::Device::Gpu),
        "cuda" => Ok(rlx::Device::Cuda),
        "rocm" => Ok(rlx::Device::Rocm),
        "tpu" => Ok(rlx::Device::Tpu),
        other => Err(anyhow!("unsupported RLX device '{other}'")),
    }
}
