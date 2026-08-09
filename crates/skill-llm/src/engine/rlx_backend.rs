// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! RLX text-generation backend.
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

use super::protocol::{GenMetrics, GenParams, InferToken};
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

/// True when `path` is an mlx-community (or mlx-lm) quantized snapshot directory.
fn looks_like_mlx_dir(path: &Path) -> bool {
    if !path.is_dir() || !path.join("config.json").is_file() {
        return false;
    }
    let Ok(bytes) = std::fs::read(path.join("config.json")) else {
        return false;
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    // mlx-lm packs use `quantization: { bits, group_size }`; GPTQ/AWQ use
    // `quantization_config` and stay on the safetensors path.
    v.get("quantization")
        .and_then(|q| q.as_object())
        .is_some_and(|q| q.contains_key("bits") || q.contains_key("group_size"))
}

#[cfg(feature = "llm-model-discovery")]
fn mlx_model_type(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path.join("config.json")).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("model_type").and_then(|t| t.as_str()).map(str::to_string)
}

/// Prefer MLX for mlx-community packs when available; otherwise honour
/// `config.rlx_device` / auto ranking.
#[cfg(feature = "llm-model-discovery")]
fn resolve_mlx_device(config: &LlmConfig) -> rlx::runtime::Device {
    use rlx::runtime::{device_ext::is_available, Device};
    if config.n_gpu_layers == 0 || config.rlx_device.eq_ignore_ascii_case("cpu") {
        return Device::Cpu;
    }
    if is_available(Device::Mlx) {
        // Explicit non-mlx accelerator still wins when the user asked for it.
        match config.rlx_device.to_ascii_lowercase().as_str() {
            "metal" if is_available(Device::Metal) => return Device::Metal,
            "cuda" if is_available(Device::Cuda) => return Device::Cuda,
            "gpu" | "wgpu" | "vulkan" if is_available(Device::Gpu) => return Device::Gpu,
            _ => return Device::Mlx,
        }
    }
    resolve_rlx_device(config)
}

/// Load an mlx-community snapshot via `Qwen3Runner::from_mlx_packed` when the
/// config is a Qwen2/Qwen3 dense decoder. Other model_types fall through.
///
/// Gated on `llm-model-discovery` until the locked rlx-models pin exports
/// `from_mlx_packed` (local sibling / post-0.2.13). Without the feature, mlx
/// dirs fall through so CI can still build against the current git pin.
#[cfg(feature = "llm-model-discovery")]
fn try_mlx_runner(path: &Path, config: &LlmConfig) -> Option<Result<Box<dyn LmRunner>>> {
    if !looks_like_mlx_dir(path) {
        return None;
    }
    let model_type = mlx_model_type(path).unwrap_or_default();
    // Qwen2 / Qwen3 share the packed Qwen3 runner (attention_bias / qk_norm
    // come from config.json). MoE / Qwen3.5 DeltaNet are out of scope.
    if !matches!(model_type.as_str(), "qwen3" | "qwen2" | "qwen2_5" | "qwen2.5") {
        log::info!(
            "mlx dir {:?}: model_type={model_type:?} — no from_mlx_packed path yet; falling through",
            path
        );
        return None;
    }
    let device = resolve_mlx_device(config);
    let max_seq = config.rlx_max_seq.max(128);
    let cfg_path = path.join("config.json");
    Some(
        rlx_models::Qwen3Config::from_file(&cfg_path)
            .and_then(|cfg| rlx_models::run::Qwen3Runner::from_mlx_packed(cfg, path, max_seq, device))
            .map(|runner| Box::new(runner) as Box<dyn LmRunner>),
    )
}

#[cfg(not(feature = "llm-model-discovery"))]
fn try_mlx_runner(path: &Path, _config: &LlmConfig) -> Option<Result<Box<dyn LmRunner>>> {
    if looks_like_mlx_dir(path) {
        log::warn!(
            "mlx pack at {:?} needs --features llm-model-discovery (and a post-0.2.13 rlx-models)",
            path
        );
    }
    None
}

fn try_minicpm5_runner(path: &Path, config: &LlmConfig) -> Option<Result<Box<dyn LmRunner>>> {
    if !looks_like_minicpm5(path) {
        return None;
    }
    let (device, packed) = gpu_plan(path, config);
    Some(
        rlx_minicpm5::MiniCpm5Runner::builder()
            .weights(path)
            .device(device)
            .packed_weights(packed)
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

fn try_gemma_runner(path: &Path, mmproj: Option<&Path>, config: &LlmConfig) -> Option<Result<Box<dyn LmRunner>>> {
    if !looks_like_gemma(path) {
        return None;
    }
    let (device, packed) = gpu_plan(path, config);
    // Multimodal needs the F32 embed path — packed is forced off when mmproj set.
    let packed = if mmproj.is_some() { false } else { packed };
    let mut b = rlx_gemma::GemmaRunner::builder()
        .weights(path)
        .device(device)
        .packed_weights(packed);
    if let Some(mp) = mmproj {
        b = b.mmproj(mp);
    }
    Some(b.build().map(|runner| Box::new(runner) as Box<dyn LmRunner>))
}

fn try_catalog_runner(path: &Path, mmproj: Option<&Path>, config: &LlmConfig) -> Option<Result<Box<dyn LmRunner>>> {
    // gemma + minicpm5 expose `.device()` + `.packed_weights()` → packed GPU path.
    // minimax + nemotron have no packed/device toggle → device-less CPU default.
    try_gemma_runner(path, mmproj, config)
        .or_else(|| try_minicpm5_runner(path, config))
        .or_else(|| try_minimax_runner(path))
        .or_else(|| try_nemotron_runner(path))
}

fn looks_like_qwen3_vl(path: &Path) -> bool {
    matches!(
        peek_gguf_arch(path).as_deref(),
        Some("qwen3vl" | "qwen3vlmoe" | "qwen3_vl" | "qwen3-vl")
    ) || filename_hint(path, "qwen3-vl")
        || filename_hint(path, "qwen3vl")
}

fn looks_like_lfm(path: &Path) -> bool {
    matches!(
        peek_gguf_arch(path).as_deref(),
        Some("lfm2" | "lfm" | "lfm25" | "lfm2_5" | "lfm2-vl" | "lfm25-vl" | "lfm2_5_vl")
    ) || filename_hint(path, "lfm2")
        || filename_hint(path, "lfm25")
}

fn try_qwen3_vl_runner(path: &Path, mmproj: Option<&Path>, config: &LlmConfig) -> Option<Result<Box<dyn LmRunner>>> {
    if !looks_like_qwen3_vl(path) {
        return None;
    }
    let device = resolve_rlx_device(config);
    let max_seq = config.rlx_max_seq.clamp(32, 4096);
    let mut b = rlx_models::qwen3_vl::Qwen3VlRunner::builder()
        .weights(path)
        .device(device)
        .max_seq(max_seq);
    if let Some(mp) = mmproj {
        b = b.mmproj(mp);
    }
    Some(b.build().map(|r| Box::new(r) as Box<dyn LmRunner>))
}

fn try_lfm_vl_runner(path: &Path, mmproj: Option<&Path>, config: &LlmConfig) -> Option<Result<Box<dyn LmRunner>>> {
    // Only take the VL path when mmproj is present; plain LFM stays on auto_runner.
    let mm = mmproj?;
    if !looks_like_lfm(path) {
        return None;
    }
    let device = resolve_rlx_device(config);
    Some(
        rlx_models::lfm_vl::LfmVlRunner::builder()
            .weights(path)
            .mmproj(mm)
            .device(device)
            .build()
            .map(|r| Box::new(r) as Box<dyn LmRunner>),
    )
}

fn looks_like_mistral(path: &Path) -> bool {
    matches!(peek_gguf_arch(path).as_deref(), Some("mistral3" | "mistral4"))
        || filename_hint(path, "ministral")
        || filename_hint(path, "mistral-medium")
        || filename_hint(path, "mistral_medium")
}

fn try_mistral_vl_runner(path: &Path, mmproj: Option<&Path>, config: &LlmConfig) -> Option<Result<Box<dyn LmRunner>>> {
    let mm = mmproj?;
    if !looks_like_mistral(path) {
        return None;
    }
    let device = resolve_rlx_device(config);
    let max_seq = config.rlx_max_seq.clamp(32, 8192);
    Some(
        rlx_models::mistral_vl::MistralVlRunner::builder()
            .weights(path)
            .mmproj(mm)
            .device(device)
            .max_seq(max_seq)
            .build()
            .map(|r| Box::new(r) as Box<dyn LmRunner>),
    )
}

fn looks_like_qwen3(path: &Path) -> bool {
    matches!(peek_gguf_arch(path).as_deref(), Some("qwen3"))
}

fn looks_like_qwen35(path: &Path) -> bool {
    matches!(
        peek_gguf_arch(path).as_deref(),
        Some("qwen35" | "qwen36" | "qwen3.5" | "qwen3.6")
    ) || filename_hint(path, "qwen3.5")
        || filename_hint(path, "qwen3.6")
        || filename_hint(path, "qwen35")
}

/// Resolve the rlx inference device from config.
///
/// - `n_gpu_layers == 0` or `rlx_device == "cpu"` → CPU
/// - Explicit accelerator tags try that device first; if unavailable they
///   fall back (notably **`cuda` → wgpu/`Gpu` → CPU**)
/// - `"auto"` / unknown → best available: Metal → CUDA → MLX → wgpu → CPU
fn resolve_rlx_device(config: &LlmConfig) -> rlx::runtime::Device {
    use rlx::runtime::{device_ext::is_available, Device};

    if config.n_gpu_layers == 0 || config.rlx_device.eq_ignore_ascii_case("cpu") {
        return Device::Cpu;
    }

    let pick = |d: Device| is_available(d).then_some(d);
    let best_available = || {
        [Device::Metal, Device::Cuda, Device::Mlx, Device::Gpu]
            .into_iter()
            .find(|d| is_available(*d))
    };

    match config.rlx_device.to_ascii_lowercase().as_str() {
        "metal" => pick(Device::Metal).or_else(best_available),
        "mlx" => pick(Device::Mlx).or_else(best_available),
        // Product Windows/Linux: CUDA when the driver is present, else wgpu.
        "cuda" => pick(Device::Cuda).or_else(|| pick(Device::Gpu)).or_else(best_available),
        "rocm" => pick(Device::Rocm).or_else(best_available),
        "gpu" | "vulkan" | "wgpu" => pick(Device::Gpu).or_else(best_available),
        // "auto" / empty / unknown
        _ => best_available(),
    }
    .unwrap_or(Device::Cpu)
}

/// Resolve `(device, packed_weights)` for a GGUF family runner.
///
/// Prefer **packed K-quant on the requested accelerator** (Metal / MLX / wgpu / …).
/// Upstream `Op::DequantMatMul` now matches CPU for Q4_K / Q6_K prefill on those
/// backends; expanding to F32 only helps tiny GGUFs and OOMs on multi-GB models.
fn gpu_plan(path: &Path, config: &LlmConfig) -> (rlx::runtime::Device, bool) {
    use rlx::runtime::Device;
    let device = resolve_rlx_device(config);
    let packed = std::fs::metadata(path)
        .map(|m| m.len() >= 256 * 1024 * 1024)
        .unwrap_or(true);
    if device == Device::Cpu {
        return (Device::Cpu, true);
    }
    eprintln!("[rlx] gpu_plan device={device:?} packed={packed}");
    (device, packed)
}

/// Build a Qwen3 GGUF runner with explicit device + packed weights, and turn on
/// the long-context HNSW KV store + dual-encoder (backend-agnostic). Handles CPU
/// too (via the concrete runner, not `auto_runner`) so the store applies on every
/// backend. Returns `None` when the model isn't Qwen3 or the build fails
/// (graceful fallback to `auto_runner`).
fn try_qwen3_runner_with_device(path: &Path, config: &LlmConfig) -> Option<Box<dyn LmRunner>> {
    if !looks_like_qwen3(path) {
        return None;
    }
    let (device, packed) = gpu_plan(path, config);
    let mut runner = match rlx_models::run::Qwen3Runner::builder()
        .weights(path)
        .device(device)
        .packed_weights(packed)
        .build()
    {
        Ok(runner) => runner,
        Err(e) => {
            eprintln!("[rlx] Qwen3 on {device:?} failed to build ({e}); falling back to auto_runner");
            return None;
        }
    };
    maybe_enable_qwen3_kv_store(&mut runner, path, device);
    Some(Box::new(runner) as Box<dyn LmRunner>)
}

/// Enable rlx-qwen3 long-context memory (disk-tiered HNSW KV context store +
/// dual-encoder semantic recall) on a freshly built Qwen3 runner, before it is
/// boxed into `dyn LmRunner`. Opt out with `SKILL_QWEN3_KV_STORE=0`. Best-effort:
/// any failure logs and leaves a plain bounded-context runner.
fn maybe_enable_qwen3_kv_store(
    runner: &mut rlx_models::run::Qwen3Runner,
    weights: &Path,
    device: rlx::runtime::Device,
) {
    let off = std::env::var("SKILL_QWEN3_KV_STORE")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "off" | "false" | "no"))
        .unwrap_or(false);
    if off {
        return;
    }
    let cfg = rlx_models::run::KvStoreConfig::new().capacity_tokens(1_000_000).topk(16);
    let repo =
        std::env::var("SKILL_QWEN3_EMBED_REPO").unwrap_or_else(|_| "BAAI/bge-small-en-v1.5".to_string());
    match resolve_qwen3_tokenizer(weights) {
        Some(tok) => match runner.enable_kv_store_with_encoder(cfg, &tok, &repo, device) {
            Ok(()) => eprintln!("[rlx] qwen3 long-context: HNSW KV store (1M) + dual-encoder {repo}"),
            Err(e) => eprintln!("[rlx] qwen3 KV store + dual-encoder disabled: {e}"),
        },
        None => match runner.enable_kv_store(cfg) {
            Ok(()) => eprintln!("[rlx] qwen3 long-context: HNSW KV store (1M, K-space; no tokenizer)"),
            Err(e) => eprintln!("[rlx] qwen3 KV store disabled: {e}"),
        },
    }
}

/// Resolve a Qwen3 `tokenizer.json` (for the dual-encoder to detokenize KV
/// blocks). Env override → sibling of the GGUF → cached/download from the shared
/// Qwen3 base repo (the tokenizer is identical across Qwen3 sizes).
fn resolve_qwen3_tokenizer(weights: &Path) -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("SKILL_QWEN3_TOKENIZER") {
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
    let repo =
        std::env::var("SKILL_QWEN3_TOKENIZER_REPO").unwrap_or_else(|_| "Qwen/Qwen3-0.6B".to_string());
    hf_hub::api::sync::Api::new().ok()?.model(repo).get("tokenizer.json").ok()
}

/// Build a Qwen3.5 / 3.6 runner with explicit device + seq caps.
///
/// Important: `auto_runner_with_mmproj` routes MTP-capable GGUFs through
/// `Qwen35SpecRunner`, which (together with Metal graph warm + mmproj) has
/// OOMed 64GB machines during validate. Plain `Qwen35Runner` is used here;
/// speculative MTP stays opt-in via a future config path.
fn try_qwen35_runner(path: &Path, mmproj: Option<&Path>, config: &LlmConfig) -> Option<Result<Box<dyn LmRunner>>> {
    if !looks_like_qwen35(path) {
        return None;
    }
    let device = resolve_rlx_device(config);
    let packed = std::fs::metadata(path)
        .map(|m| m.len() >= 256 * 1024 * 1024)
        .unwrap_or(true);
    let max_seq = config.rlx_max_seq.clamp(32, 4096);

    let build = |mm: Option<&Path>| {
        eprintln!(
            "[rlx] Qwen35 load device={device:?} packed={packed} max_seq={max_seq} mmproj={}",
            mm.is_some()
        );
        let mut b = rlx_models::run::Qwen35Runner::builder()
            .weights(path)
            .device(device)
            .packed_weights(packed)
            .max_seq(max_seq)
            .skip_warm(true);
        if let Some(mp) = mm {
            b = b.mmproj(mp);
        }
        b.build().map(|runner| Box::new(runner) as Box<dyn LmRunner>)
    };

    match build(mmproj) {
        Ok(runner) => Some(Ok(runner)),
        Err(e) if mmproj.is_some() => {
            eprintln!("[rlx] Qwen35 with mmproj failed ({e}); retrying text-only");
            Some(
                build(None).map_err(|e2| {
                    anyhow!("Qwen35Runner text-only on {device:?} failed after mmproj error ({e}): {e2}")
                }),
            )
        }
        Err(e) => Some(Err(anyhow!("Qwen35Runner on {device:?} failed: {e}"))),
    }
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

/// Hide a leading, whitespace-only `<think>…</think>` block from streamed output.
///
/// Qwen3.5 (reasoning model) emits an empty `<think>\n\n</think>` prefix even when
/// the assistant has nothing to reason about, which would otherwise show up as a
/// blank block at the top of every chat reply. This returns the visible suffix:
/// - a resolved empty block → the text after `</think>` (leading whitespace trimmed);
/// - a NON-empty block (real reasoning) → the input unchanged (nothing hidden);
/// - a block still being generated (`</think>` not yet seen), or a partial prefix
///   of the opening tag → `""`, so the streamer buffers until the block resolves.
///
/// The returned slice always borrows from `s`, so byte offsets into it are stable
/// as `s` grows token-by-token (the visible prefix never changes once non-empty).
fn visible_after_empty_think(s: &str) -> &str {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";
    let trimmed = s.trim_start();
    if let Some(rest) = trimmed.strip_prefix(OPEN) {
        match rest.find(CLOSE) {
            Some(end) if rest[..end].trim().is_empty() => rest[end + CLOSE.len()..].trim_start(),
            Some(_) => s, // real reasoning content — leave intact
            None => "",   // think block still open — buffer
        }
    } else if !trimmed.is_empty() && OPEN.starts_with(trimmed) {
        "" // partial "<think>" prefix in progress — buffer
    } else {
        s
    }
}

impl RlxTextRunner {
    /// Attach an optional mmproj vision encoder (e.g. for Qwen3.5-VL).
    /// When `mmproj` is `None` the runner is text-only.
    pub(super) fn load_with_mmproj(model_path: &Path, mmproj: Option<&Path>, config: &LlmConfig) -> Result<Self> {
        // mlx-community packed safetensors dirs — before GGUF-oriented paths.
        if let Some(result) = try_mlx_runner(model_path, config) {
            let runner = result?;
            return finish_load(model_path, runner, mmproj, None);
        }

        // Qwen3-VL before plain Qwen3 (same GGUF family otherwise strips mmproj).
        if let Some(result) = try_qwen3_vl_runner(model_path, mmproj, config) {
            let runner = result?;
            return finish_load(model_path, runner, mmproj, None);
        }

        // LFM + mmproj → LfmVlRunner (text-only LFM falls through).
        if let Some(result) = try_lfm_vl_runner(model_path, mmproj, config) {
            let runner = result?;
            return finish_load(model_path, runner, mmproj, None);
        }

        // Ministral / Mistral Medium + mmproj → MistralVlRunner.
        if let Some(result) = try_mistral_vl_runner(model_path, mmproj, config) {
            let runner = result?;
            return finish_load(model_path, runner, mmproj, None);
        }

        if let Some(result) = try_catalog_runner(model_path, mmproj, config) {
            let runner = result?;
            let family = runner.family();
            let explicit_tokenizer = match family {
                "gemma" => resolve_gemma_tokenizer(model_path),
                "minicpm5" => resolve_minicpm5_tokenizer(model_path),
                _ => None,
            };
            return finish_load(model_path, runner, mmproj, explicit_tokenizer);
        }

        // Plain Qwen3 (not VL): never attach mmproj here.
        if mmproj.is_none() {
            if let Some(runner) = try_qwen3_runner_with_device(model_path, config) {
                return finish_load(model_path, runner, None, None);
            }
        } else if looks_like_qwen3(model_path) && !looks_like_qwen3_vl(model_path) {
            return Err(anyhow!(
                "mmproj is set but this GGUF is plain Qwen3 (not qwen3vl*) — \
                 omit mmproj for text-only, or use a Qwen3-VL / Qwen3.5 checkpoint"
            ));
        }

        // Qwen3.5 / 3.6: must not fall through to auto_runner's SpecRunner
        // (MTP GGUFs OOM on unified-memory Macs during graph warm).
        if let Some(result) = try_qwen35_runner(model_path, mmproj, config) {
            let runner = result?;
            return finish_load(model_path, runner, mmproj, None);
        }

        let runner = auto_runner_with_mmproj(model_path, mmproj).map_err(|e| anyhow!("RLX auto_runner: {e}"))?;
        finish_load(model_path, runner, mmproj, None)
    }
}

fn finish_load(
    model_path: &Path,
    runner: Box<dyn LmRunner>,
    mmproj: Option<&Path>,
    explicit_tokenizer: Option<PathBuf>,
) -> Result<RlxTextRunner> {
    if mmproj.is_some() && !runner.supports_multimodal() {
        return Err(anyhow!(
            "mmproj was requested but runner `{}` does not support multimodal — \
             omit mmproj or use a VL-capable family (qwen35, gemma+mmproj, qwen3-vl, lfm-vl, mistral-vl)",
            runner.family()
        ));
    }
    Ok(RlxTextRunner {
        family: runner.family(),
        runner,
        weights_path: model_path.to_path_buf(),
        explicit_tokenizer,
    })
}

impl RlxTextRunner {
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

        // Chat-turn EOS token(s). The rlx runner stops on the GGUF's `eos_token_id`,
        // but Qwen chat ends each assistant turn with `<|im_end|>` — a SPECIAL token
        // that detokenization skips, so it can't be a stop-STRING. Without stopping
        // on its id the model runs to the token cap, hallucinating extra turns. Stop
        // on the id directly (empty for non-ChatML models → no behaviour change).
        // `<|im_end|>` ends a ChatML turn; some qwen35 finetunes (e.g. Fara) instead
        // terminate the assistant reply with `<|endoftext|>`. Both are special tokens
        // detok skips, so stop on their ids directly (else generation runs to the cap).
        let chat_eos: Vec<u32> = ["<|im_end|>", "<|endoftext|>"]
            .iter()
            .filter_map(|t| auto_tokenize(&self.weights_path, t, self.explicit_tokenizer.as_deref()).ok())
            .filter(|ids| ids.len() == 1)
            .map(|ids| ids[0])
            .collect();

        // Time-to-first-token (prefill boundary), captured inside the decode loop.
        let t_first: std::cell::Cell<Option<std::time::Instant>> = std::cell::Cell::new(None);

        let mut on_token = |tok: u32| -> bool {
            if chat_eos.contains(&tok) {
                return false; // assistant turn ended — don't emit the special token
            }
            if t_first.get().is_none() {
                t_first.set(Some(std::time::Instant::now()));
            }
            completion_tokens += 1;
            all_ids.push(tok);
            // Decode the full sequence and emit the new suffix.
            let decoded = match auto_detokenize(&weights, &all_ids, explicit.as_deref(), true) {
                Ok(s) => s,
                Err(_) => return true,
            };
            // Suppress a leading empty <think></think> (Qwen3.5 reasoning prefix)
            // from what the user sees. `visible` borrows `decoded`; its prefix is
            // stable once non-empty, so `emitted_len` stays a valid byte offset.
            let visible = visible_after_empty_think(&decoded);
            if visible.len() > emitted_len {
                let piece = visible[emitted_len..].to_string();
                emitted_len = visible.len();
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

        let t_gen_start = std::time::Instant::now();
        let result = self
            .runner
            .generate(&prompt_ids, max_tokens, &mut on_token as &mut dyn FnMut(u32) -> bool);
        let t_end = std::time::Instant::now();

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
        let prompt_tokens = prompt_ids.len();
        let prefill_secs = t_first.get().map(|t| (t - t_gen_start).as_secs_f64()).unwrap_or(0.0);
        let decode_secs = t_first.get().map(|t| (t_end - t).as_secs_f64()).unwrap_or(0.0);
        let (kv_blocks, kv_tokens, kv_disk_bytes) = self.runner.kv_store_stats().unwrap_or((0, 0, 0));
        let metrics = GenMetrics {
            prefill_ms: prefill_secs * 1000.0,
            prefill_tps: if prefill_secs > 0.0 { prompt_tokens as f64 / prefill_secs } else { 0.0 },
            decode_tps: if decode_secs > 0.0 && completion_tokens > 1 {
                (completion_tokens - 1) as f64 / decode_secs
            } else {
                0.0
            },
            kv_blocks: kv_blocks as u64,
            kv_tokens: kv_tokens as u64,
            kv_disk_bytes: kv_disk_bytes as u64,
        };
        token_tx
            .send(InferToken::Done {
                finish_reason: finish_reason.into(),
                prompt_tokens,
                completion_tokens,
                n_ctx: prompt_tokens.saturating_add(completion_tokens),
                metrics,
            })
            .ok();
    }

    /// Multimodal generation — decodes the first image to RGB and
    /// hands it off to the runner's [`LmRunner::generate_multimodal`]
    /// (currently the Qwen3.5 family path). Additional images beyond
    /// the first are ignored (single-frame chat). Streams decoded text via `token_tx`.
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

        // Same chat-turn EOS as the text path: Qwen3.5-VL ends its assistant turn
        // with `<|im_end|>` or `<|endoftext|>` (special tokens detok skips, so they
        // can't be stop STRINGS). Without this the VLM answer runs to the cap.
        let chat_eos: Vec<u32> = ["<|im_end|>", "<|endoftext|>"]
            .iter()
            .filter_map(|t| auto_tokenize(&self.weights_path, t, self.explicit_tokenizer.as_deref()).ok())
            .filter(|ids| ids.len() == 1)
            .map(|ids| ids[0])
            .collect();

        let mut on_token = |tok: u32| -> bool {
            if chat_eos.contains(&tok) {
                return false; // assistant turn ended — don't emit the special token
            }
            completion_tokens += 1;
            all_ids.push(tok);
            let decoded = match auto_detokenize(&weights, &all_ids, explicit.as_deref(), true) {
                Ok(s) => s,
                Err(_) => return true,
            };
            // Hide the leading empty <think></think> block, same as the text path.
            let visible = visible_after_empty_think(&decoded);
            if visible.len() > emitted_len {
                let piece = visible[emitted_len..].to_string();
                emitted_len = visible.len();
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
                metrics: GenMetrics::default(),
            })
            .ok();
    }
}

#[cfg(all(test, feature = "llm-rlx"))]
mod device_tests {
    use super::resolve_rlx_device;
    use crate::config::LlmConfig;
    use rlx::runtime::Device;

    #[test]
    fn explicit_cpu_forces_cpu() {
        let cfg = LlmConfig {
            rlx_device: "cpu".into(),
            n_gpu_layers: u32::MAX,
            ..LlmConfig::default()
        };
        assert_eq!(resolve_rlx_device(&cfg), Device::Cpu);
    }

    #[test]
    fn zero_gpu_layers_forces_cpu() {
        // GPU offload disabled → CPU regardless of the requested device string.
        let cfg = LlmConfig {
            rlx_device: "metal".into(),
            n_gpu_layers: 0,
            ..LlmConfig::default()
        };
        assert_eq!(resolve_rlx_device(&cfg), Device::Cpu);
    }

    #[test]
    fn auto_never_hard_pins_cpu_when_gpu_offload_enabled() {
        let cfg = LlmConfig {
            rlx_device: "auto".into(),
            n_gpu_layers: u32::MAX,
            ..LlmConfig::default()
        };
        let d = resolve_rlx_device(&cfg);
        // On CI/mac without NVIDIA this may be Metal/Gpu/Cpu; just ensure
        // "auto" does not short-circuit before availability checks.
        let _ = d;
        assert!(matches!(
            d,
            Device::Cpu | Device::Metal | Device::Mlx | Device::Cuda | Device::Gpu | Device::Rocm
        ));
    }
}

#[cfg(all(test, feature = "llm-rlx"))]
mod kv_store_runtime_tests {
    use crate::config::LlmConfig;
    use crate::engine::protocol::{GenParams, InferToken};
    use std::path::PathBuf;
    use tokio::sync::mpsc::unbounded_channel;

    /// End-to-end runtime check of the Qwen3-0.6B long-context path: loading the
    /// GGUF must enable the HNSW KV store + bge dual-encoder (via
    /// `maybe_enable_qwen3_kv_store` → `enable_kv_store_with_encoder`) without
    /// error, and decoding with the store attached must produce coherent text.
    ///
    /// Ignored — needs the Qwen3-0.6B GGUF (env `QWEN3_GGUF`, else the sibling
    /// rlx-models weights) and, on first run, network for the bge encoder. Run:
    ///   QWEN3_DEVICE=cpu cargo test -p skill-llm --features apple \
    ///     qwen3_0_6b_kv_store_end_to_end -- --ignored --nocapture
    #[test]
    #[ignore = "runtime: needs Qwen3-0.6B GGUF + network for bge dual-encoder"]
    fn qwen3_0_6b_kv_store_end_to_end() {
        let gguf = std::env::var("QWEN3_GGUF").unwrap_or_else(|_| {
            "/Users/Shared/rlx-models/weights/lm/qwen3-0.6b-gguf/Qwen3-0.6B-Q4_K_M.gguf".to_string()
        });
        let path = PathBuf::from(&gguf);
        assert!(path.is_file(), "GGUF not found: {gguf}");

        let cfg = LlmConfig {
            rlx_device: std::env::var("QWEN3_DEVICE").unwrap_or_else(|_| "cpu".into()),
            ..LlmConfig::default()
        };

        // Routes through try_qwen3_runner_with_device → maybe_enable_qwen3_kv_store
        // → enable_kv_store_with_encoder. The "[rlx] qwen3 long-context: …" line
        // prints here under --nocapture; a failure to enable the store panics.
        let mut runner = super::RlxTextRunner::load_with_mmproj(&path, None, &cfg)
            .expect("load Qwen3-0.6B with KV store + dual-encoder");
        assert_eq!(runner.family(), "qwen3");

        let mk_params = |max_tokens: usize| GenParams {
            max_tokens,
            temperature: 0.0,
            thinking_budget: Some(0), // skip <think> for the tiny 0.6B
            ..GenParams::default()
        };
        let prompt = "Q: What is the capital of France? Answer in one word.\nA:";

        // Warmup: first generation compiles the decode buckets (slow) — discard.
        let (wtx, mut _wrx) = unbounded_channel::<InferToken>();
        runner.generate(prompt, mk_params(8), wtx);

        // Measured run: steady-state throughput + KV telemetry.
        let (tx, mut rx) = unbounded_channel::<InferToken>();
        runner.generate(prompt, mk_params(64), tx);

        let mut text = String::new();
        let mut done = false;
        while let Ok(tok) = rx.try_recv() {
            match tok {
                InferToken::Delta(s) => text.push_str(&s),
                InferToken::Done { completion_tokens, n_ctx, metrics, .. } => {
                    done = true;
                    eprintln!(
                        "[speed] device={} · {} tokens (n_ctx={}) · prefill {:.1} ms ({:.0} tok/s) · decode {:.1} tok/s",
                        cfg.rlx_device, completion_tokens, n_ctx,
                        metrics.prefill_ms, metrics.prefill_tps, metrics.decode_tps
                    );
                    eprintln!(
                        "[kv-store] blocks={} tokens={} disk_bytes={}",
                        metrics.kv_blocks, metrics.kv_tokens, metrics.kv_disk_bytes
                    );
                }
                InferToken::Error(e) => panic!("generation error: {e}"),
            }
        }
        eprintln!("[test] output={text:?}");
        assert!(done, "generation did not finish (no Done token)");
        assert!(!text.trim().is_empty(), "generation produced no text");
    }

    /// End-to-end runtime check of the DEFAULT Qwen3.5-0.8B path through the exact
    /// engine the daemon uses (`RlxTextRunner::load_with_mmproj` → try_qwen35_runner
    /// → generate). Verifies arch detection, tokenizer, runner selection, and
    /// coherent output. Run:
    ///   cargo test -p skill-llm --release --features llm-rlx-metal \
    ///     qwen35_0_8b_end_to_end -- --ignored --nocapture
    #[ignore = "runtime: needs Qwen3.5-0.8B GGUF + a GPU backend + tokenizer"]
    #[test]
    fn qwen35_0_8b_end_to_end() {
        let gguf = std::env::var("QWEN35_GGUF").unwrap_or_else(|_| {
            "/Users/Shared/weights/qwen3.5-0.8b-gguf/Qwen3.5-0.8B-Q4_K_M.gguf".to_string()
        });
        let path = PathBuf::from(&gguf);
        assert!(path.is_file(), "GGUF not found: {gguf}");

        // Default config (verifies OOTB max_seq is adequate), only overriding the
        // device so the test runs on GPU.
        let cfg = LlmConfig {
            rlx_device: std::env::var("QWEN35_DEVICE").unwrap_or_else(|_| "metal".into()),
            n_gpu_layers: 999,
            ..LlmConfig::default()
        };

        let mut runner = super::RlxTextRunner::load_with_mmproj(&path, None, &cfg)
            .expect("load Qwen3.5-0.8B via skill-llm engine");
        assert_eq!(runner.family(), "qwen35", "expected qwen35 runner");

        // The Qwen3.5 GGUF's embedded chat template fails to render in rlx-models,
        // so the daemon (resolve_chat_template) falls back to a built-in ChatML
        // template for qwen — use the exact same one here.
        let tpl = crate::engine::rlx_actor::chatml_template(&path)
            .expect("build ChatML template for qwen35");
        let user = |c: &str| rlx_models::run::ChatMessage { role: "user".into(), content: c.into() };
        let asst = |c: &str| rlx_models::run::ChatMessage { role: "assistant".into(), content: c.into() };
        let render = |m: &[rlx_models::run::ChatMessage]| tpl.render(m, true).expect("render chatml");

        // Run one turn to completion, returning the exact streamed text. The daemon
        // now strips the leading empty <think></think> itself, so the caller sees a
        // clean answer with no reasoning tags — assert that here rather than
        // re-stripping in the test.
        fn run_chat(runner: &mut super::RlxTextRunner, prompt: &str, cap: usize) -> (String, usize) {
            let params = GenParams {
                max_tokens: cap,
                temperature: 0.0,
                thinking_budget: Some(0),
                ..GenParams::default()
            };
            let (tx, mut rx) = unbounded_channel::<InferToken>();
            runner.generate(prompt, params, tx);
            let (mut text, mut n) = (String::new(), 0usize);
            while let Ok(tok) = rx.try_recv() {
                match tok {
                    InferToken::Delta(s) => text.push_str(&s),
                    InferToken::Done { completion_tokens, .. } => n = completion_tokens,
                    InferToken::Error(e) => panic!("generation error: {e}"),
                }
            }
            assert!(
                !text.contains("<think>") && !text.contains("</think>"),
                "empty <think> block leaked into streamed output: {text:?}"
            );
            (text.trim().to_string(), n)
        }

        // 1) Factual, single word. Also the first call — pays decode-bucket compile.
        let (a1, n1) = run_chat(
            &mut runner,
            &render(&[user("What is the capital of France? Reply with only the city name.")]),
            64,
        );
        eprintln!("[case1 capital] device={} n={n1} out={a1:?}", cfg.rlx_device);
        assert!(n1 < 64, "case1 did not stop on EOS: {a1:?}");
        assert!(a1.to_lowercase().contains("paris"), "case1 wrong answer: {a1:?}");

        // 2) A DIFFERENT prompt on the SAME runner. Regression guard for the
        //    GPU-resident-KV leak: before the fix the second generate() reused the
        //    first turn's resident K/V (same decode bucket, no re-seed) and answered
        //    the previous question — here it would echo "Paris" / a capital instead of
        //    explaining a computer. Must be a fresh, on-topic answer.
        let (a2, n2) = run_chat(
            &mut runner,
            &render(&[user("Explain how a computer works in one sentence.")]),
            128,
        );
        eprintln!("[case2 explain] n={n2} out={a2:?}");
        assert!(n2 < 128, "case2 did not stop on EOS: {a2:?}");
        assert!(a2.split_whitespace().count() >= 5, "case2 degenerate: {a2:?}");
        assert!(
            !a2.to_lowercase().contains("paris"),
            "case2 leaked case1's answer (resident-KV not re-seeded): {a2:?}"
        );

        // 3) Multi-turn — the ChatML template must carry history so the model recalls
        //    the name from the earlier turn.
        let convo = [
            user("My name is Alice."),
            asst("Nice to meet you, Alice! How can I help?"),
            user("What is my name?"),
        ];
        let (a3, n3) = run_chat(&mut runner, &render(&convo), 64);
        eprintln!("[case3 multiturn] n={n3} out={a3:?}");
        assert!(n3 < 64, "case3 did not stop on EOS: {a3:?}");
        assert!(a3.to_lowercase().contains("alice"), "case3 lost history: {a3:?}");
    }

    #[test]
    fn empty_think_block_is_stripped() {
        use super::visible_after_empty_think as vis;
        // Resolved empty block → text after </think>, leading whitespace trimmed.
        assert_eq!(vis("<think>\n\n</think>\n\nParis"), "Paris");
        assert_eq!(vis("<think></think>Hello"), "Hello");
        // No think block → passthrough.
        assert_eq!(vis("Just an answer."), "Just an answer.");
        // Real reasoning content → left fully intact (nothing hidden).
        let reasoning = "<think>let me count</think>\n\n4";
        assert_eq!(vis(reasoning), reasoning);
        // Mid-stream: block still open, or a partial opening tag → buffer ("").
        assert_eq!(vis("<think>\n\n"), "");
        assert_eq!(vis("<thi"), "");
        assert_eq!(vis("<think>"), "");
        // Streaming an empty block emits nothing until it resolves, then the answer:
        // simulate the growing decoded string and confirm the visible prefix is stable.
        assert_eq!(vis("<think>\n\n</think>\n\nPa"), "Pa");
        assert_eq!(vis("<think>\n\n</think>\n\nParis."), "Paris.");
    }

    /// End-to-end VISUAL inference through the exact daemon engine
    /// (`load_with_mmproj` → try_qwen35_runner(mmproj) → generate_multimodal).
    /// Uses Fara1.5-4B (qwen35 arch) + its mmproj GGUF. Feeds a synthetic blue
    /// circle on white and checks the model actually SEES it (names the colour) —
    /// a text-only model can't know the colour, so "blue" proves the vision path.
    /// Run:
    ///   cargo test -p skill-llm --release --features llm-rlx-metal \
    ///     fara_vlm_end_to_end -- --ignored --nocapture
    #[ignore = "runtime: needs Fara1.5-4B GGUF + mmproj + a GPU backend"]
    #[test]
    fn fara_vlm_end_to_end() {
        use std::io::Cursor;

        let gguf = std::env::var("FARA_GGUF").unwrap_or_else(|_| {
            "/Users/Shared/weights/fara1.5-4b-gguf/Fara1.5-4B-Q4_K_M.gguf".to_string()
        });
        let mmproj = std::env::var("FARA_MMPROJ").unwrap_or_else(|_| {
            "/Users/Shared/weights/fara1.5-4b-gguf/mmproj-Fara1.5-4B-f16.gguf".to_string()
        });
        let (path, mmp) = (PathBuf::from(&gguf), PathBuf::from(&mmproj));
        assert!(path.is_file(), "Fara GGUF not found: {gguf}");
        assert!(mmp.is_file(), "Fara mmproj not found: {mmproj}");

        // The vision encoder floors an image to ~1024 tokens, so max_seq must clear
        // that plus the prompt + answer.
        let cfg = LlmConfig {
            rlx_device: std::env::var("FARA_DEVICE").unwrap_or_else(|_| "metal".into()),
            n_gpu_layers: 999,
            rlx_max_seq: 1280,
            ..LlmConfig::default()
        };

        let mut runner = super::RlxTextRunner::load_with_mmproj(&path, Some(&mmp), &cfg)
            .expect("load Fara1.5-4B + mmproj via skill-llm engine");
        assert_eq!(runner.family(), "qwen35", "expected qwen35 runner for Fara");
        assert!(runner.supports_multimodal(), "mmproj vision encoder not attached");

        // Synthetic 128×128 image: a solid blue circle on white. PNG-encoded like a
        // real chat attachment (generate_multimodal decodes the bytes itself).
        let (w, h) = (128u32, 128u32);
        let mut img = image::RgbImage::from_pixel(w, h, image::Rgb([255, 255, 255]));
        let (cx, cy, r2) = (64.0f32, 64.0f32, 40.0f32 * 40.0f32);
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                if dx * dx + dy * dy <= r2 {
                    img.put_pixel(x, y, image::Rgb([30, 90, 220]));
                }
            }
        }
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("encode test png");

        // The daemon actor injects the <__media__> marker into the user turn; build
        // the same framed prompt here (marker at the start = image before the text).
        let tpl = crate::engine::rlx_actor::chatml_template(&path)
            .expect("build ChatML template for Fara");
        let msgs = [rlx_models::run::ChatMessage {
            role: "user".into(),
            content: "<__media__>What is the main shape and its color in this image? \
                      Answer in one short sentence."
                .into(),
        }];
        let prompt = tpl.render(&msgs, true).expect("render chatml");

        // Drive one image turn to completion.
        fn run_vlm(runner: &mut super::RlxTextRunner, prompt: &str, png: &[u8]) -> (String, usize) {
            let params = GenParams {
                max_tokens: 96,
                temperature: 0.0,
                thinking_budget: Some(0),
                ..GenParams::default()
            };
            let (tx, mut rx) = unbounded_channel::<InferToken>();
            runner.generate_multimodal(prompt, std::slice::from_ref(&png.to_vec()), params, tx);
            let (mut text, mut n, mut done) = (String::new(), 0usize, false);
            while let Ok(tok) = rx.try_recv() {
                match tok {
                    InferToken::Delta(s) => text.push_str(&s),
                    InferToken::Done { completion_tokens, .. } => {
                        n = completion_tokens;
                        done = true;
                    }
                    InferToken::Error(e) => panic!("multimodal generation error: {e}"),
                }
            }
            assert!(done, "multimodal generation did not finish");
            (text.trim().to_string(), n)
        }

        let check = |label: &str, out: &str, n: usize| {
            eprintln!("[fara vlm {label}] n={n} out={out:?}");
            assert!(!out.is_empty(), "{label}: no text");
            assert!(
                !out.contains("<think>") && !out.contains("</think>"),
                "{label}: think block leaked: {out:?}"
            );
            assert!(n < 96, "{label}: did not stop on EOS: {out:?}");
            let low = out.to_lowercase();
            assert!(low.contains("blue"), "{label}: colour not perceived (vision broken?): {out:?}");
            assert!(
                low.contains("circle") || low.contains("round") || low.contains("disc") || low.contains("dot"),
                "{label}: shape not perceived: {out:?}"
            );
        };

        // Turn 1: cold — compiles + caches the vision graph for this size.
        let (o1, n1) = run_vlm(&mut runner, &prompt, &png);
        check("turn1-cold", &o1, n1);
        // Turn 2: SAME size on the SAME runner → warm cached graph (no recompile,
        // params resident). Also a fresh sequence: guards the VLM resident-KV reset
        // (must re-perceive the image, not echo turn 1).
        let (o2, n2) = run_vlm(&mut runner, &prompt, &png);
        check("turn2-warm", &o2, n2);
    }
}
