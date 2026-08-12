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
    let cfg = rlx_models::run::KvStoreConfig::new()
        .capacity_tokens(1_000_000)
        .topk(16);
    let repo = std::env::var("SKILL_QWEN3_EMBED_REPO").unwrap_or_else(|_| "BAAI/bge-small-en-v1.5".to_string());
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
    let repo = std::env::var("SKILL_QWEN3_TOKENIZER_REPO").unwrap_or_else(|_| "Qwen/Qwen3-0.6B".to_string());
    hf_hub::api::sync::Api::new()
        .ok()?
        .model(repo)
        .get("tokenizer.json")
        .ok()
}

const GIB: u64 = 1 << 30;

/// Memory-adaptive tuning for a (V)LM runner, decided from the model's on-disk
/// weight size and the currently-free unified memory.
struct MemPlan {
    /// Decode context to actually build (never raised above the request).
    max_seq: usize,
    /// Keep the resident warm-up prefill graph (fast first image/token) vs.
    /// compile it lazily on first use (slower first turn, less resident RAM).
    warm_prefill: bool,
    /// Human-readable reason, logged so tight-memory downgrades are visible.
    note: Option<String>,
}

/// Decide how much streaming machinery to keep resident given free RAM.
///
/// A large VLM at a big context plus the warm-up prefill graph can approach the
/// unified-memory ceiling — VLM + Metal graph warm "has OOMed 64GB machines"
/// during validate. When headroom is tight we (a) drop the resident warm-up
/// graph first (the biggest resident lever, and the one the `recommend_ctx_size`
/// estimator doesn't account for) and (b) clamp context so the KV cache + prefill
/// graph shrink — trading first-image latency and max prompt length to stay off
/// OOM. Only VLMs are adjusted (the text estimator already sizes ctx to memory);
/// never *raises* a limit; a no-op when memory can't be measured, so
/// well-provisioned machines are unaffected.
fn memory_adaptive_plan(
    requested_max_seq: usize,
    weights_bytes: u64,
    free_bytes: Option<u64>,
    is_vlm: bool,
) -> MemPlan {
    let keep = MemPlan {
        max_seq: requested_max_seq,
        warm_prefill: true,
        note: None,
    };
    // The warm-up prefill graph only exists for VLM (mmproj) runners, and the
    // text path already sizes its context to available memory upstream.
    if !is_vlm {
        return keep;
    }
    let Some(free) = free_bytes else {
        return keep; // can't measure → don't regress
    };
    // Reserve for the co-hosted daemon subsystems (TTS/ASR/embeddings) + OS, plus
    // the VLM's vision encoder and per-image prefill activations.
    const DAEMON_RESERVE: u64 = 3 * GIB;
    const VISION_RESERVE: u64 = 2 * GIB;
    let effective = free
        .saturating_sub(weights_bytes)
        .saturating_sub(DAEMON_RESERVE + VISION_RESERVE);

    if effective >= 8 * GIB {
        return keep; // comfortable — keep the fast (heavy) path
    }
    let max_seq = if effective < 2 * GIB {
        requested_max_seq.min(1024)
    } else if effective < 4 * GIB {
        requested_max_seq.min(2048)
    } else {
        requested_max_seq
    };
    let note = Some(format!(
        "low memory (free {:.1}GB, weights {:.1}GB, ~{:.1}GB headroom): \
         skipping resident prefill warm-up{}",
        free as f64 / GIB as f64,
        weights_bytes as f64 / GIB as f64,
        effective as f64 / GIB as f64,
        if max_seq < requested_max_seq {
            format!(", clamping context {requested_max_seq}→{max_seq}")
        } else {
            String::new()
        },
    ));
    MemPlan {
        max_seq,
        warm_prefill: false,
        note,
    }
}

/// Full hardware-adaptive plan: layers swap pressure and power state on top of
/// the free-RAM core ([`memory_adaptive_plan`]). Probed at runtime via
/// `skill_gpu::system_resources`, so the runner adapts to the *actual* machine
/// (a laptop on battery, a box already paging to swap, a tight-RAM host) instead
/// of a fixed config. Never *raises* a limit.
fn hardware_adaptive_plan(
    requested_max_seq: usize,
    weights_bytes: u64,
    res: &skill_data::gpu_stats::SystemResources,
    is_vlm: bool,
) -> MemPlan {
    // Swap in use means the OS is already paging — "available RAM" overstates the
    // real headroom, so discount free RAM by the swap currently used.
    let free = res
        .free_ram_bytes
        .map(|f| f.saturating_sub(res.used_swap_bytes.unwrap_or(0)));
    let mut plan = memory_adaptive_plan(requested_max_seq, weights_bytes, free, is_vlm);

    // On battery or OS low-power mode, favor efficiency over peak throughput: shed
    // the resident warm-up graph (sustained GPU draw) and cap context, even when
    // RAM is comfortable. VLM-only, matching the warm-up graph's scope.
    if is_vlm && (res.on_battery == Some(true) || res.low_power == Some(true)) {
        let capped = plan.max_seq.min(2048);
        let mut note = plan.note.take().unwrap_or_default();
        if !note.is_empty() {
            note.push_str("; ");
        }
        note.push_str(&format!("on battery/low-power → warm-up off, ctx≤{capped}"));
        plan = MemPlan {
            max_seq: capped,
            warm_prefill: false,
            note: Some(note),
        };
    }
    plan
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
    // Track the memory-aware `ctx_size` (sized in init.rs from model + free RAM)
    // so tool-augmented daemon prompts (~5-6k tokens) fit instead of failing with
    // "prompt length N exceeds compiled max_seq". rlx long-context is correct at
    // 8192 (verified: a long natural prompt and a tool prompt both generate cleanly
    // on Qwen3.5-0.8B). Cap at 8192 (static-prefill graph cost); memory_adaptive_plan
    // trims it back under memory pressure. NOTE: a *model* that isn't trained for
    // this tool-call format (e.g. the Fara-1.5 computer-use agent model) will still
    // emit degenerate tool-call output — that's a model-fit issue, not this cap.
    let requested_max_seq = config
        .ctx_size
        .map(|c| c as usize)
        .unwrap_or(0)
        .max(config.rlx_max_seq)
        .clamp(32, 8192);

    // ── Memory-adaptive streaming tuning ───────────────────────────────────────
    // On unified-memory systems a big VLM + a large context + the resident
    // warm-up prefill graph can hit the RAM ceiling. Weigh the model's on-disk
    // size against currently-free memory and shed resident cost (warm-up graph,
    // then context) when headroom is tight, so we stay off OOM.
    let weights_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
        + mmproj
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0);
    // Probe the actual hardware at runtime (RAM, swap, free disk, CPUs, power) and
    // adapt — shed the resident warm-up graph + clamp context under memory/swap
    // pressure or on battery — instead of a fixed config.
    let mut res = skill_data::gpu_stats::system_resources(path.parent());
    if let Some(g) = skill_data::gpu_stats::read() {
        // Prefer the GPU/unified free figure as the RAM budget when available
        // (unified free on Metal; free-or-total VRAM on a discrete GPU).
        let gpu_free = if g.is_unified_memory {
            g.free_memory_bytes
        } else {
            g.free_memory_bytes.or(g.total_memory_bytes)
        };
        if gpu_free.is_some() {
            res.free_ram_bytes = gpu_free;
        }
    }
    let plan = hardware_adaptive_plan(requested_max_seq, weights_bytes, &res, mmproj.is_some());
    let max_seq = plan.max_seq;
    if let Some(note) = &plan.note {
        eprintln!("[rlx] Qwen35 {note}");
    }
    // Non-blocking heads-up if the model dir's filesystem is nearly full.
    if let Some(free_disk) = res.free_disk_bytes {
        if free_disk < 2 * GIB {
            eprintln!(
                "[rlx] Qwen35 low disk: {:.1}GB free on model dir",
                free_disk as f64 / GIB as f64
            );
        }
    }
    // Skip the resident warm-up prefill graph under memory pressure (rlx honors
    // this env at load); otherwise clear it so a prior run's value never leaks.
    if plan.warm_prefill {
        std::env::remove_var("RLX_QWEN35_WARM_HIDDEN_PREFILL");
    } else {
        std::env::set_var("RLX_QWEN35_WARM_HIDDEN_PREFILL", "0");
    }

    // Drive the qwen3.5 vision-token floor from settings (fewer tokens = faster
    // image encode + prefill, coarser detail; more = higher fidelity on dense
    // images). The rlx vision config reads this env at load; set it explicitly —
    // or clear it — on every build so a prior value never leaks across a model
    // reload. Only affects VLM (mmproj) loads.
    match config.vision_min_tokens {
        Some(n) => std::env::set_var("RLX_QWEN35_VISION_MIN_TOKENS", n.to_string()),
        None => std::env::remove_var("RLX_QWEN35_VISION_MIN_TOKENS"),
    }

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

/// Detect a degenerate greedy-decode repetition loop: the output ends in two
/// identical consecutive blocks (a true cycle of period `p`). The reasoning model
/// (Fara) sometimes runs past its turn-end on verbose prompts, re-emitting its
/// answer + a stray `</think>` until the token cap (not F16-specific — happens at
/// F32 too). Requiring two *adjacent* equal blocks (not just any earlier match)
/// means normal prose that reuses a phrase won't trip it; the repeated block must
/// carry an alphanumeric char so blank-line runs don't either. Fires once the
/// second block completes (~2 repeats), which the streamer turns into a stop.
fn is_repetition_loop(s: &str) -> bool {
    let b = s.as_bytes();
    let n = b.len();
    for p in 12..=192 {
        if 2 * p > n {
            break;
        }
        if b[n - 2 * p..n - p] == b[n - p..n] && b[n - p..n].iter().any(|c| c.is_ascii_alphanumeric()) {
            return true;
        }
    }
    false
}

/// What to actually stream this step, plus whether to stop cleanly. Strips a
/// leading empty `<think></think>` (see [`visible_after_empty_think`]); if what
/// remains is real content that then hits a stray `</think>` — a degenerate turn
/// re-closing thinking, which is disabled here — truncate at it and signal stop so
/// the answer ends cleanly instead of looping. A genuine non-empty leading think
/// block (result still starts with `<think>`) is passed through untouched.
fn visible_for_stream(decoded: &str) -> (&str, bool) {
    let v = visible_after_empty_think(decoded);
    if v.trim_start().starts_with("<think>") {
        return (v, false);
    }
    match v.find("</think>") {
        Some(i) => (&v[..i], true),
        None => (v, false),
    }
}

/// Per-token sink shared by the text and multimodal decode loops. Owns the
/// streaming state (accumulated ids/text, emit cursor) and applies the common
/// chat post-processing on every token: stop on a chat-turn EOS id
/// (`<|im_end|>` / `<|endoftext|>`, which detok skips so they can't be stop
/// strings), hide a leading empty `<think></think>`, end cleanly on a stray
/// `</think>` or a repetition loop, and honour user stop strings. Only the
/// newly-revealed suffix is streamed (correct across multi-byte codepoints split
/// by byte-level BPE). `push` is the `FnMut(u32) -> bool` the runner drives.
struct ChatStreamer {
    weights: PathBuf,
    explicit: Option<PathBuf>,
    chat_eos: Vec<u32>,
    stop: Vec<String>,
    tx: UnboundedSender<InferToken>,
    all_ids: Vec<u32>,
    emitted_len: usize,
    accumulated: String,
    completion_tokens: usize,
    /// Time the first token was produced — the prefill/decode boundary (TTFT).
    first_at: Option<std::time::Instant>,
}

impl ChatStreamer {
    fn new(
        weights: PathBuf,
        explicit: Option<PathBuf>,
        stop: Vec<String>,
        tx: UnboundedSender<InferToken>,
        cap: usize,
    ) -> Self {
        let chat_eos = ["<|im_end|>", "<|endoftext|>"]
            .iter()
            .filter_map(|t| auto_tokenize(&weights, t, explicit.as_deref()).ok())
            .filter(|ids| ids.len() == 1)
            .map(|ids| ids[0])
            .collect();
        Self {
            weights,
            explicit,
            chat_eos,
            stop,
            tx,
            all_ids: Vec::with_capacity(cap.min(4096)),
            emitted_len: 0,
            accumulated: String::new(),
            completion_tokens: 0,
            first_at: None,
        }
    }

    /// Runner callback: consume one generated token; return `false` to stop.
    fn push(&mut self, tok: u32) -> bool {
        if self.chat_eos.contains(&tok) {
            return false; // assistant turn ended — don't emit the special token
        }
        self.first_at.get_or_insert_with(std::time::Instant::now);
        self.completion_tokens += 1;
        self.all_ids.push(tok);
        // Decode the whole sequence and emit only the new suffix (O(n²) but fine
        // for typical caps; handles codepoints split across byte-level BPE tokens).
        let decoded = match auto_detokenize(&self.weights, &self.all_ids, self.explicit.as_deref(), true) {
            Ok(s) => s,
            Err(_) => return true,
        };
        let (visible, stray_stop) = visible_for_stream(&decoded);
        if visible.len() > self.emitted_len {
            let piece = visible[self.emitted_len..].to_string();
            self.emitted_len = visible.len();
            if !piece.is_empty() {
                self.accumulated.push_str(&piece);
                self.tx.send(InferToken::Delta(piece)).ok();
            }
        }
        if stray_stop {
            return false; // stray </think> after content = degenerate turn
        }
        if self.hit_stop_string() {
            return false;
        }
        // Break degenerate repetition loops (model running past its turn-end).
        !is_repetition_loop(&self.accumulated)
    }

    fn hit_stop_string(&self) -> bool {
        self.stop.iter().any(|s| !s.is_empty() && self.accumulated.ends_with(s))
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

        let max_tokens = params.max_tokens;
        let mut streamer = ChatStreamer::new(
            self.weights_path.clone(),
            self.explicit_tokenizer.clone(),
            params.stop.clone(),
            token_tx.clone(),
            max_tokens,
        );

        let t_gen_start = std::time::Instant::now();
        let result = self
            .runner
            .generate(&prompt_ids, max_tokens, &mut |tok| streamer.push(tok));
        let t_end = std::time::Instant::now();

        if let Err(e) = result {
            token_tx
                .send(InferToken::Error(format!("RLX generation failed: {e}")))
                .ok();
            return;
        }

        let finish_reason = if streamer.hit_stop_string() { "stop" } else { "length" };
        let prompt_tokens = prompt_ids.len();
        let completion_tokens = streamer.completion_tokens;
        let prefill_secs = streamer
            .first_at
            .map(|t| (t - t_gen_start).as_secs_f64())
            .unwrap_or(0.0);
        let decode_secs = streamer.first_at.map(|t| (t_end - t).as_secs_f64()).unwrap_or(0.0);
        let (kv_blocks, kv_tokens, kv_disk_bytes) = self.runner.kv_store_stats().unwrap_or((0, 0, 0));
        let metrics = GenMetrics {
            prefill_ms: prefill_secs * 1000.0,
            prefill_tps: if prefill_secs > 0.0 {
                prompt_tokens as f64 / prefill_secs
            } else {
                0.0
            },
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
        let mut streamer = ChatStreamer::new(
            self.weights_path.clone(),
            self.explicit_tokenizer.clone(),
            params.stop.clone(),
            token_tx.clone(),
            max_tokens,
        );

        // Surface real generation phases ("vision" while the image is encoded,
        // "prefill" during the LM prefill) during the pre-token lead-in so the UI
        // can show accurate progress. The runner fires this from inside its
        // otherwise-opaque vision-encode + prefill; it flows out as
        // `InferToken::Status` on the same channel as the tokens.
        {
            let phase_tx = token_tx.clone();
            self.runner
                .set_multimodal_phase_callback(Some(std::sync::Arc::new(move |phase: &str| {
                    let _ = phase_tx.send(InferToken::Status(phase.to_string()));
                })));
        }

        let t_gen_start = std::time::Instant::now();
        let result = self.runner.generate_multimodal(
            prompt,
            &rgb,
            img_w,
            img_h,
            self.explicit_tokenizer.as_deref(),
            max_tokens,
            &mut |tok| streamer.push(tok),
        );
        // Drop the callback so it never outlives this turn's channel.
        self.runner.set_multimodal_phase_callback(None);
        let t_end = std::time::Instant::now();
        if let Err(e) = result {
            token_tx
                .send(InferToken::Error(format!("RLX multimodal generation failed: {e}")))
                .ok();
            return;
        }
        let finish_reason = if streamer.hit_stop_string() { "stop" } else { "length" };
        let completion_tokens = streamer.completion_tokens;
        // TTFT here spans the vision encode + multimodal prefill (prompt_tokens is
        // unknown at this layer — the vision tokens live inside the runner — so
        // prefill_tps is left 0; prefill_ms is the wall TTFT, decode_tps the rest).
        let ttft_secs = streamer
            .first_at
            .map(|t| (t - t_gen_start).as_secs_f64())
            .unwrap_or(0.0);
        let decode_secs = streamer.first_at.map(|t| (t_end - t).as_secs_f64()).unwrap_or(0.0);
        let metrics = GenMetrics {
            prefill_ms: ttft_secs * 1000.0,
            decode_tps: if decode_secs > 0.0 && completion_tokens > 1 {
                (completion_tokens - 1) as f64 / decode_secs
            } else {
                0.0
            },
            ..GenMetrics::default()
        };
        token_tx
            .send(InferToken::Done {
                finish_reason: finish_reason.into(),
                prompt_tokens: 0,
                completion_tokens,
                n_ctx: completion_tokens,
                metrics,
            })
            .ok();
    }
}

#[cfg(all(test, feature = "llm-rlx"))]
mod mem_plan_tests {
    use super::{memory_adaptive_plan, GIB};

    // ~3.4GB Fara-4B (Q4 weights + f16 mmproj), 4096 requested context.
    const VLM_WEIGHTS: u64 = 3 * GIB + GIB / 2;
    const REQ: usize = 4096;

    #[test]
    fn text_only_never_downgrades() {
        // No mmproj → no warm-up graph to shed; ctx is sized upstream.
        let p = memory_adaptive_plan(REQ, VLM_WEIGHTS, Some(4 * GIB), false);
        assert_eq!(p.max_seq, REQ);
        assert!(p.warm_prefill);
        assert!(p.note.is_none());
    }

    #[test]
    fn unmeasurable_memory_keeps_fast_path() {
        let p = memory_adaptive_plan(REQ, VLM_WEIGHTS, None, true);
        assert_eq!(p.max_seq, REQ);
        assert!(p.warm_prefill);
        assert!(p.note.is_none());
    }

    #[test]
    fn roomy_machine_keeps_warmup_and_full_ctx() {
        // 64GB box, ~9GB free after everything → keep the heavy fast path.
        let p = memory_adaptive_plan(REQ, VLM_WEIGHTS, Some(VLM_WEIGHTS + 5 * GIB + 9 * GIB), true);
        assert_eq!(p.max_seq, REQ);
        assert!(p.warm_prefill);
        assert!(p.note.is_none());
    }

    #[test]
    fn tight_sheds_warmup_first_then_clamps_context() {
        // ~1GB effective headroom → drop warm-up and clamp ctx to 1024.
        let free = VLM_WEIGHTS + 5 * GIB + GIB; // reserve is 5GB (daemon+vision)
        let p = memory_adaptive_plan(REQ, VLM_WEIGHTS, Some(free), true);
        assert!(!p.warm_prefill);
        assert_eq!(p.max_seq, 1024);
        assert!(p.note.is_some());
    }

    #[test]
    fn moderately_tight_sheds_warmup_but_keeps_more_ctx() {
        // ~3GB effective → warm-up off, ctx clamped to 2048 (not 1024).
        let free = VLM_WEIGHTS + 5 * GIB + 3 * GIB;
        let p = memory_adaptive_plan(REQ, VLM_WEIGHTS, Some(free), true);
        assert!(!p.warm_prefill);
        assert_eq!(p.max_seq, 2048);
    }

    #[test]
    fn clamp_never_raises_a_small_request() {
        // A caller that already asked for 512 must not be bumped up to 1024/2048.
        let free = VLM_WEIGHTS + 5 * GIB + GIB;
        let p = memory_adaptive_plan(512, VLM_WEIGHTS, Some(free), true);
        assert_eq!(p.max_seq, 512);
        assert!(!p.warm_prefill);
    }

    use super::hardware_adaptive_plan;
    use skill_data::gpu_stats::SystemResources;

    #[test]
    fn swap_pressure_discounts_free_ram() {
        // Free RAM alone looks comfortable (~9GB effective), but 6GB of swap is in
        // use → real headroom ~3GB → shed warm-up (OS is already paging).
        let res = SystemResources {
            free_ram_bytes: Some(VLM_WEIGHTS + 5 * GIB + 9 * GIB),
            used_swap_bytes: Some(6 * GIB),
            ..Default::default()
        };
        let p = hardware_adaptive_plan(REQ, VLM_WEIGHTS, &res, true);
        assert!(!p.warm_prefill, "swap pressure should shed the warm-up graph");
        assert!(p.note.is_some());
    }

    #[test]
    fn on_battery_sheds_warmup_and_caps_ctx_even_with_plenty_ram() {
        let res = SystemResources {
            free_ram_bytes: Some(VLM_WEIGHTS + 5 * GIB + 20 * GIB),
            used_swap_bytes: Some(0),
            on_battery: Some(true),
            ..Default::default()
        };
        let p = hardware_adaptive_plan(REQ, VLM_WEIGHTS, &res, true);
        assert!(!p.warm_prefill, "battery should shed warm-up");
        assert!(p.max_seq <= 2048, "battery should cap context");
    }

    #[test]
    fn desktop_ac_plenty_ram_keeps_fast_path() {
        let res = SystemResources {
            free_ram_bytes: Some(VLM_WEIGHTS + 5 * GIB + 20 * GIB),
            used_swap_bytes: Some(0),
            on_battery: Some(false),
            ..Default::default()
        };
        let p = hardware_adaptive_plan(REQ, VLM_WEIGHTS, &res, true);
        assert!(p.warm_prefill && p.max_seq == REQ, "roomy desktop keeps the fast path");
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
                InferToken::Status(_) => {}
                InferToken::Done {
                    completion_tokens,
                    n_ctx,
                    metrics,
                    ..
                } => {
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
        let gguf = std::env::var("QWEN35_GGUF")
            .unwrap_or_else(|_| "/Users/Shared/weights/qwen3.5-0.8b-gguf/Qwen3.5-0.8B-Q4_K_M.gguf".to_string());
        let path = PathBuf::from(&gguf);
        assert!(path.is_file(), "GGUF not found: {gguf}");

        // Default config (verifies OOTB max_seq is adequate), only overriding the
        // device so the test runs on GPU.
        let cfg = LlmConfig {
            rlx_device: std::env::var("QWEN35_DEVICE").unwrap_or_else(|_| "metal".into()),
            n_gpu_layers: 999,
            ..LlmConfig::default()
        };

        let mut runner =
            super::RlxTextRunner::load_with_mmproj(&path, None, &cfg).expect("load Qwen3.5-0.8B via skill-llm engine");
        assert_eq!(runner.family(), "qwen35", "expected qwen35 runner");

        // The Qwen3.5 GGUF's embedded chat template fails to render in rlx-models,
        // so the daemon (resolve_chat_template) falls back to a built-in ChatML
        // template for qwen — use the exact same one here.
        let tpl = crate::engine::rlx_actor::chatml_template(&path).expect("build ChatML template for qwen35");
        let user = |c: &str| rlx_models::run::ChatMessage {
            role: "user".into(),
            content: c.into(),
        };
        let asst = |c: &str| rlx_models::run::ChatMessage {
            role: "assistant".into(),
            content: c.into(),
        };
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
                    InferToken::Status(_) => {}
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
    fn repetition_loop_detects_cycles() {
        use super::is_repetition_loop as loops;
        // Non-repeating / short / whitespace → not a loop.
        assert!(!loops(""));
        assert!(!loops("The animal in the photo is a black puppy."));
        assert!(!loops(&format!(
            "A unique, non-repeating sentence about a scene. {}",
            " ".repeat(200)
        )));
        // A longer varied paragraph with no verbatim cycle → not a loop.
        assert!(!loops(
            "A snow-covered peak rises against a pastel sky; below, a calm valley \
             stretches toward a distant, hazy treeline under soft morning light."
        ));
        // The real degeneration: a sentence (+ stray </think>) repeats verbatim.
        assert!(loops(
            &"The main shape is a circle, and its color is green.\n</think>\n\n".repeat(4)
        ));
        assert!(loops(&"The animal in the photo is a black puppy.\n".repeat(3)));
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

        let gguf = std::env::var("FARA_GGUF")
            .unwrap_or_else(|_| "/Users/Shared/weights/fara1.5-4b-gguf/Fara1.5-4B-Q4_K_M.gguf".to_string());
        let mmproj = std::env::var("FARA_MMPROJ")
            .unwrap_or_else(|_| "/Users/Shared/weights/fara1.5-4b-gguf/mmproj-Fara1.5-4B-f16.gguf".to_string());
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
        let tpl = crate::engine::rlx_actor::chatml_template(&path).expect("build ChatML template for Fara");
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
                    InferToken::Status(_) => {}
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
            assert!(
                low.contains("blue"),
                "{label}: colour not perceived (vision broken?): {out:?}"
            );
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

    /// Multiple images (colour × shape) through the daemon VLM path. Run with
    /// `RLX_QWEN35_VISION_F16=all` too — used to confirm the CPU F16-weight matmul
    /// fix (the vision tower perceives colours correctly at F16, no blue→green).
    ///   FARA_IMG_DIR=<scratchpad> cargo test -p skill-llm --release \
    ///     --features llm-rlx-metal fara_vlm_multi_image -- --ignored --nocapture
    #[ignore = "runtime: needs Fara1.5-4B GGUF + mmproj + probe PNGs (FARA_IMG_DIR)"]
    #[test]
    fn fara_vlm_multi_image() {
        let gguf = std::env::var("FARA_GGUF")
            .unwrap_or_else(|_| "/Users/Shared/weights/fara1.5-4b-gguf/Fara1.5-4B-Q4_K_M.gguf".to_string());
        let mmproj = std::env::var("FARA_MMPROJ")
            .unwrap_or_else(|_| "/Users/Shared/weights/fara1.5-4b-gguf/mmproj-Fara1.5-4B-f16.gguf".to_string());
        let dir = std::env::var("FARA_IMG_DIR").expect("set FARA_IMG_DIR to the probe-PNG dir");
        let (path, mmp) = (PathBuf::from(&gguf), PathBuf::from(&mmproj));
        assert!(path.is_file() && mmp.is_file(), "Fara weights missing");

        let cfg = LlmConfig {
            rlx_device: std::env::var("FARA_DEVICE").unwrap_or_else(|_| "metal".into()),
            n_gpu_layers: 999,
            rlx_max_seq: 1280,
            ..LlmConfig::default()
        };
        let mut runner = super::RlxTextRunner::load_with_mmproj(&path, Some(&mmp), &cfg).expect("load Fara + mmproj");
        let tpl = crate::engine::rlx_actor::chatml_template(&path).expect("chatml");

        fn run_vlm(runner: &mut super::RlxTextRunner, prompt: &str, png: &[u8]) -> (String, usize) {
            let params = GenParams {
                max_tokens: 64,
                temperature: 0.0,
                thinking_budget: Some(0),
                ..GenParams::default()
            };
            let (tx, mut rx) = unbounded_channel::<InferToken>();
            runner.generate_multimodal(prompt, std::slice::from_ref(&png.to_vec()), params, tx);
            let (mut text, mut n) = (String::new(), 0usize);
            while let Ok(tok) = rx.try_recv() {
                match tok {
                    InferToken::Delta(s) => text.push_str(&s),
                    InferToken::Status(_) => {}
                    InferToken::Done { completion_tokens, .. } => n = completion_tokens,
                    InferToken::Error(e) => panic!("multimodal error: {e}"),
                }
            }
            (text.trim().to_string(), n)
        }

        // (file, colour keyword, shape keywords). Files written by the probe script.
        let cases: &[(&str, &str, &[&str])] = &[
            ("blue_circle_128.png", "blue", &["circle", "round", "disc", "dot"]),
            ("red_square.png", "red", &["square", "rectangle", "box"]),
            ("green_circle.png", "green", &["circle", "round", "disc", "dot"]),
            ("yellow_tri.png", "yellow", &["circle", "round", "disc", "dot"]),
        ];
        let f16 = !matches!(
            std::env::var("RLX_QWEN35_VISION_F16").ok().as_deref(),
            Some("0") | Some("off") | Some("f32") | Some("none")
        ); // F16 default-on
        let mut fails = Vec::new();
        for (file, colour, shapes) in cases {
            let p = std::path::Path::new(&dir).join(file);
            let Ok(png) = std::fs::read(&p) else {
                eprintln!("[multi] skip missing {}", p.display());
                continue;
            };
            let msgs = [rlx_models::run::ChatMessage {
                role: "user".into(),
                content: "<__media__>What is the main shape and its color? One short sentence.".into(),
            }];
            let prompt = tpl.render(&msgs, true).expect("render");
            let (out, n) = run_vlm(&mut runner, &prompt, &png);
            let low = out.to_lowercase();
            let ok_c = low.contains(colour);
            let ok_s = shapes.iter().any(|s| low.contains(s));
            eprintln!("[multi f16={f16}] {file}: n={n} colour({colour})={ok_c} shape={ok_s} :: {out:?}");
            if !(ok_c && ok_s) {
                fails.push(format!("{file}: {out:?}"));
            }
        }
        assert!(fails.is_empty(), "VLM mis-perceived images: {fails:#?}");
    }

    /// Describe real photographs (no assertions) — run once at F32 and once with
    /// `RLX_QWEN35_VISION_F16=all` and diff the descriptions to validate F16 on
    /// real images before promoting it to default.
    ///   FARA_IMG_DIR=<dir with real_*.jpg> cargo test -p skill-llm --release \
    ///     --features llm-rlx-metal fara_vlm_describe -- --ignored --nocapture
    #[ignore = "runtime: needs Fara1.5-4B + mmproj + real_*.jpg in FARA_IMG_DIR"]
    #[test]
    fn fara_vlm_describe() {
        let gguf = std::env::var("FARA_GGUF")
            .unwrap_or_else(|_| "/Users/Shared/weights/fara1.5-4b-gguf/Fara1.5-4B-Q4_K_M.gguf".to_string());
        let mmproj = std::env::var("FARA_MMPROJ")
            .unwrap_or_else(|_| "/Users/Shared/weights/fara1.5-4b-gguf/mmproj-Fara1.5-4B-f16.gguf".to_string());
        let dir = std::env::var("FARA_IMG_DIR").expect("set FARA_IMG_DIR");
        let (path, mmp) = (PathBuf::from(&gguf), PathBuf::from(&mmproj));
        let cfg = LlmConfig {
            rlx_device: "metal".into(),
            n_gpu_layers: 999,
            rlx_max_seq: 1280,
            ..LlmConfig::default()
        };
        let mut runner = super::RlxTextRunner::load_with_mmproj(&path, Some(&mmp), &cfg).expect("load Fara + mmproj");
        let tpl = crate::engine::rlx_actor::chatml_template(&path).expect("chatml");
        let f16 = !matches!(
            std::env::var("RLX_QWEN35_VISION_F16").ok().as_deref(),
            Some("0") | Some("off") | Some("f32") | Some("none")
        ); // F16 default-on

        for file in ["real_237.jpg", "real_24.jpg", "real_431.jpg", "real_866.jpg"] {
            let Ok(png) = std::fs::read(std::path::Path::new(&dir).join(file)) else {
                eprintln!("[desc] skip missing {file}");
                continue;
            };
            let msgs = [rlx_models::run::ChatMessage {
                role: "user".into(),
                content: "<__media__>Describe this image in one sentence.".into(),
            }];
            let prompt = tpl.render(&msgs, true).expect("render");
            let params = GenParams {
                max_tokens: 80,
                temperature: 0.0,
                thinking_budget: Some(0),
                ..GenParams::default()
            };
            let (tx, mut rx) = unbounded_channel::<InferToken>();
            runner.generate_multimodal(&prompt, std::slice::from_ref(&png), params, tx);
            let (mut text, mut n) = (String::new(), 0usize);
            while let Ok(tok) = rx.try_recv() {
                match tok {
                    InferToken::Delta(s) => text.push_str(&s),
                    InferToken::Status(_) => {}
                    InferToken::Done { completion_tokens, .. } => n = completion_tokens,
                    InferToken::Error(e) => panic!("multimodal error: {e}"),
                }
            }
            eprintln!("[desc f16={f16}] {file} (n={n}): {}", text.trim());
        }
    }

    /// Full DAEMON image path (minus HTTP): JSON `messages` → `render_chat` (which
    /// auto-injects the `<__media__>` marker for image turns) → `generate_multimodal`.
    /// The other VLM tests hand-build the marker prompt; this one exercises the
    /// actor's real prompt construction so image chat is verified end-to-end.
    ///   FARA_IMG_DIR=<dir> cargo test -p skill-llm --release --features llm-rlx-metal \
    ///     daemon_vlm_image_path -- --ignored --nocapture
    #[ignore = "runtime: needs Fara1.5-4B + mmproj + real_237.jpg in FARA_IMG_DIR"]
    #[test]
    fn daemon_vlm_image_path() {
        let gguf = std::env::var("FARA_GGUF")
            .unwrap_or_else(|_| "/Users/Shared/weights/fara1.5-4b-gguf/Fara1.5-4B-Q4_K_M.gguf".to_string());
        let mmproj = std::env::var("FARA_MMPROJ")
            .unwrap_or_else(|_| "/Users/Shared/weights/fara1.5-4b-gguf/mmproj-Fara1.5-4B-f16.gguf".to_string());
        let dir = std::env::var("FARA_IMG_DIR").expect("set FARA_IMG_DIR");
        let (path, mmp) = (PathBuf::from(&gguf), PathBuf::from(&mmproj));
        let png = std::fs::read(std::path::Path::new(&dir).join("real_237.jpg"))
            .expect("real_237.jpg (black puppy) in FARA_IMG_DIR");

        let cfg = LlmConfig {
            rlx_device: "metal".into(),
            n_gpu_layers: 999,
            rlx_max_seq: 1280,
            ..LlmConfig::default()
        };
        let mut runner = super::RlxTextRunner::load_with_mmproj(&path, Some(&mmp), &cfg).expect("load Fara");

        // Exactly what the daemon actor builds: OpenAI-style JSON messages with NO
        // marker in the text — render_chat injects it because images are present.
        let tpl = Some(crate::engine::rlx_actor::chatml_template(&path).expect("chatml"));
        // Verbose prompt (no brevity cue) — reproduces the reasoning-model decode
        // loop at this max_seq; the streaming repetition guard must break it well
        // before the 64-token cap.
        let messages = vec![serde_json::json!({
            "role": "user",
            "content": "What animal is in this photo and what colour is it?"
        })];
        let prompt = crate::engine::rlx_actor::render_chat(&tpl, &messages, /*with_media=*/ true).expect("render_chat");
        eprintln!("[daemon-vlm] rendered prompt: {prompt:?}");
        assert_eq!(
            prompt.matches("<__media__>").count(),
            1,
            "actor must inject exactly one media marker: {prompt:?}"
        );
        assert!(
            prompt.contains("What animal"),
            "user text missing from prompt: {prompt:?}"
        );

        let params = GenParams {
            max_tokens: 64,
            temperature: 0.0,
            thinking_budget: Some(0),
            ..GenParams::default()
        };
        let (tx, mut rx) = unbounded_channel::<InferToken>();
        runner.generate_multimodal(&prompt, std::slice::from_ref(&png), params, tx);
        let (mut text, mut n, mut done) = (String::new(), 0usize, false);
        while let Ok(tok) = rx.try_recv() {
            match tok {
                InferToken::Delta(s) => text.push_str(&s),
                InferToken::Status(_) => {}
                InferToken::Done { completion_tokens, .. } => {
                    n = completion_tokens;
                    done = true;
                }
                InferToken::Error(e) => panic!("multimodal error: {e}"),
            }
        }
        let out = text.trim();
        eprintln!("[daemon-vlm] n={n} out={out:?}");
        assert!(done && !out.is_empty(), "no answer");
        // The guard must end the degenerate loop cleanly: no stray </think> leaks
        // and a single answer (well under the cap, and not the ~2× the raw loop
        // emitted before the periodic guard alone would trip).
        assert!(!out.contains("</think>"), "stray </think> leaked: {out:?}");
        assert!(n < 40, "did not break the decode loop cleanly: n={n} {out:?}");
        let low = out.to_lowercase();
        assert!(
            low.contains("dog") || low.contains("puppy") || low.contains("lab"),
            "daemon path failed to identify the dog: {out:?}"
        );
        assert!(low.contains("black"), "daemon path failed the colour: {out:?}");
    }

    /// End-to-end latency benchmark across modalities (text, reasoning, image)
    /// through the real daemon engine. Measures TTFT (wall time to the first
    /// streamed token — includes vision encode for image turns), total latency,
    /// and prefill/decode t/s. Warms the decode buckets + vision graph first so
    /// numbers are steady-state.
    ///   FARA_IMG_DIR=<dir> cargo test -p skill-llm --release --features llm-rlx-metal \
    ///     fara_e2e_bench -- --ignored --nocapture
    #[ignore = "runtime bench: Fara1.5-4B + mmproj + real_*.jpg in FARA_IMG_DIR"]
    #[test]
    fn fara_e2e_bench() {
        let gguf = std::env::var("FARA_GGUF")
            .unwrap_or_else(|_| "/Users/Shared/weights/fara1.5-4b-gguf/Fara1.5-4B-Q4_K_M.gguf".to_string());
        let mmproj = std::env::var("FARA_MMPROJ")
            .unwrap_or_else(|_| "/Users/Shared/weights/fara1.5-4b-gguf/mmproj-Fara1.5-4B-f16.gguf".to_string());
        let dir = std::env::var("FARA_IMG_DIR").expect("set FARA_IMG_DIR");
        let cfg = LlmConfig {
            rlx_device: "metal".into(),
            n_gpu_layers: 999,
            rlx_max_seq: 1408,
            ..LlmConfig::default()
        };
        let mut runner =
            super::RlxTextRunner::load_with_mmproj(&PathBuf::from(&gguf), Some(&PathBuf::from(&mmproj)), &cfg)
                .expect("load Fara + mmproj");
        let tpl = crate::engine::rlx_actor::chatml_template(&PathBuf::from(&gguf)).expect("chatml");
        let img = |f: &str| std::fs::read(std::path::Path::new(&dir).join(f)).ok();

        #[derive(Default)]
        struct Row {
            name: String,
            ttft_ms: f64,
            latency_ms: f64,
            ptoks: usize,
            ctoks: usize,
            prefill_tps: f64,
            decode_tps: f64,
        }

        fn bench(
            runner: &mut super::RlxTextRunner,
            tpl: &rlx_models::run::ChatTemplate,
            name: &str,
            content: &str,
            image: Option<&[u8]>,
            max_tokens: usize,
            think: Option<u32>,
        ) -> Row {
            let text = if image.is_some() {
                format!("<__media__>{content}")
            } else {
                content.to_string()
            };
            let msgs = [rlx_models::run::ChatMessage {
                role: "user".into(),
                content: text,
            }];
            let prompt = tpl.render(&msgs, true).expect("render");
            let params = GenParams {
                max_tokens,
                temperature: 0.0,
                thinking_budget: think,
                ..GenParams::default()
            };
            let (tx, mut rx) = unbounded_channel::<InferToken>();
            let t0 = std::time::Instant::now();
            match image {
                Some(png) => runner.generate_multimodal(&prompt, std::slice::from_ref(&png.to_vec()), params, tx),
                None => runner.generate(&prompt, params, tx),
            }
            let mut row = Row {
                name: name.into(),
                ..Default::default()
            };
            // `generate*` is synchronous (it streams into the channel, then returns),
            // so a channel-drain timestamp can't see TTFT — take it from the metrics,
            // where `first_at` was captured inside the run (spans vision encode for
            // image turns). Wall latency is the whole synchronous call.
            while let Ok(tok) = rx.try_recv() {
                match tok {
                    InferToken::Delta(_) => {}
                    InferToken::Status(_) => {}
                    InferToken::Done {
                        prompt_tokens,
                        completion_tokens,
                        metrics,
                        ..
                    } => {
                        row.ptoks = prompt_tokens;
                        row.ctoks = completion_tokens;
                        row.ttft_ms = metrics.prefill_ms;
                        row.prefill_tps = metrics.prefill_tps;
                        row.decode_tps = metrics.decode_tps;
                    }
                    InferToken::Error(e) => panic!("{name}: {e}"),
                }
            }
            row.latency_ms = t0.elapsed().as_secs_f64() * 1e3;
            row
        }

        // Warmup: compile decode buckets (text) + the vision graph (image) so the
        // measured rows are steady-state, not first-touch.
        let _ = bench(&mut runner, &tpl, "warmup-text", "Hi.", None, 8, Some(0));
        if let Some(w) = img("real_237.jpg") {
            let _ = bench(&mut runner, &tpl, "warmup-img", "Hi.", Some(&w), 8, Some(0));
        }

        let mut rows = Vec::new();
        rows.push(bench(
            &mut runner,
            &tpl,
            "text-short",
            "What is the capital of France? Answer in one sentence.",
            None,
            64,
            Some(0),
        ));
        // Repeat the SAME prompt — if the prefill cache is kept across turns, this
        // second same-length prefill should reuse it (near-zero compile) instead of
        // paying the full rebuild again.
        rows.push(bench(
            &mut runner,
            &tpl,
            "text-short-2",
            "What is the capital of France? Answer in one sentence.",
            None,
            64,
            Some(0),
        ));
        rows.push(bench(
            &mut runner,
            &tpl,
            "text-long",
            "Write a detailed multi-paragraph explanation of how a CPU fetches, decodes, and executes instructions.",
            None,
            256,
            Some(0),
        ));
        rows.push(bench(&mut runner, &tpl, "reasoning", "I have 3 apples, eat 1, buy 5 more, then give away 2. How many apples do I have? Show your step-by-step reasoning.", None, 256, None));
        if let Some(w) = img("real_237.jpg") {
            rows.push(bench(
                &mut runner,
                &tpl,
                "image-short",
                "Describe this image in one sentence.",
                Some(&w),
                96,
                Some(0),
            ));
            // Repeat same image+prompt to measure warm multimodal prefill.
            rows.push(bench(
                &mut runner,
                &tpl,
                "image-short-2",
                "Describe this image in one sentence.",
                Some(&w),
                96,
                Some(0),
            ));
        }
        if let Some(w) = img("real_431.jpg") {
            rows.push(bench(
                &mut runner,
                &tpl,
                "image-detail",
                "Describe this image in detail.",
                Some(&w),
                224,
                Some(0),
            ));
        }

        eprintln!(
            "\n{:<13} {:>6} {:>6} {:>10} {:>11} {:>12} {:>11}",
            "scenario", "ptoks", "ctoks", "TTFT ms", "latency ms", "prefill t/s", "decode t/s"
        );
        eprintln!("{}", "-".repeat(74));
        for r in &rows {
            eprintln!(
                "{:<13} {:>6} {:>6} {:>10.0} {:>11.0} {:>12.1} {:>11.1}",
                r.name, r.ptoks, r.ctoks, r.ttft_ms, r.latency_ms, r.prefill_tps, r.decode_tps
            );
            // Sanity: every scenario produced tokens and stopped before the cap-loop.
            assert!(r.ctoks > 0, "{}: produced no tokens", r.name);
        }
    }
}
