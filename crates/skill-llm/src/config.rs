// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! LLM configuration types — shared between the skill-llm crate and the
//! main application.

use serde::{Deserialize, Serialize};

// Re-export tool config types from skill-tools so existing `crate::config::*`
// paths continue to work.
pub use skill_tools::types::{LlmToolConfig, ToolExecutionMode};

// ── LLM server configuration ─────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    /// Enable the LLM server.  When `false` (the default) no model is loaded
    /// and all `/v1/*` endpoints return HTTP 503.
    #[serde(default)]
    pub enabled: bool,

    /// Absolute path to a GGUF model file.  Required when `enabled = true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_path: Option<std::path::PathBuf>,

    /// Number of transformer layers to offload to the GPU.
    /// `0` = CPU-only inference.  `-1` (stored as `u32::MAX`) = offload all.
    #[serde(default)]
    pub n_gpu_layers: u32,

    /// KV-cache / context size in tokens.
    ///
    /// `None` → auto-recommend based on the model's parameter count, quant,
    /// and the system's available GPU/unified memory (via `recommend_ctx_size`).
    /// When set explicitly, the value is capped at the model's trained maximum
    /// context length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctx_size: Option<u32>,

    /// Maximum number of inference requests processed concurrently.
    /// Default: 1.
    #[serde(default = "default_llm_parallel")]
    pub parallel: usize,

    /// Optional Bearer token required on every `/v1/*` request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Allow-list for built-in chat tools exposed to the local LLM chat.
    #[serde(default)]
    pub tools: LlmToolConfig,

    // ── Multimodal (requires `llm-mtmd` feature) ──────────────────────────────
    /// Path to the multimodal projector (mmproj) GGUF file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mmproj: Option<std::path::PathBuf>,

    /// Number of threads used by the vision/audio encoder.  Default: 4.
    #[serde(default = "default_mmproj_n_threads")]
    pub mmproj_n_threads: i32,

    /// Disable GPU offloading for the mmproj model (use CPU instead).
    #[serde(default)]
    pub no_mmproj_gpu: bool,

    /// Automatically load the vision projector (mmproj) when the LLM server
    /// starts.  Default: `true`.
    #[serde(default = "default_autoload_mmproj")]
    pub autoload_mmproj: bool,

    /// Enable verbose llama.cpp / clip_model_loader logging to stderr.
    #[serde(default)]
    pub verbose: bool,

    /// Auto-start the LLM server when the app launches (if a model is
    /// downloaded and selected).  Default: `false`.
    #[serde(default)]
    pub autostart: bool,

    /// Maximum tokens per decode call during prompt prefill.
    /// Larger = faster prefill at the cost of more peak memory.
    /// `None` = auto (min(n_ctx, 2048)).  Default: `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n_batch: Option<u32>,

    /// Micro-batch size for GPU kernel dispatch during prefill.
    /// `None` = auto (min(n_batch, 512)).  Default: `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n_ubatch: Option<u32>,

    /// Enable flash attention (KV cache in f16 instead of f32, faster on
    /// GPU backends that support it — Metal, CUDA, Vulkan).  Default: `true`.
    #[serde(default = "default_flash_attention")]
    pub flash_attention: bool,

    /// Offload the KQV tensor operations to the GPU even when not all layers
    /// are offloaded.  Default: `true`.
    #[serde(default = "default_offload_kqv")]
    pub offload_kqv: bool,

    /// Minimum free GPU/unified memory (in GB) required before starting a
    /// decode pass.  If available memory drops below this threshold, the
    /// request is rejected with a recoverable error instead of risking a
    /// Metal/CUDA abort that crashes the process.  Default: `0.5` GB.
    #[serde(default = "default_gpu_memory_threshold")]
    pub gpu_memory_threshold: f64,

    /// Minimum free GPU/unified memory (in GB) during token generation.
    /// If memory drops below this during sampling, generation stops early
    /// with a `gpu_memory` finish reason.  Default: `0.3` GB.
    /// Checked every 64 tokens to minimize overhead.
    #[serde(default = "default_gpu_memory_gen_threshold")]
    pub gpu_memory_gen_threshold: f64,

    // ── Model metadata (populated by init.rs from the catalog entry) ──────────
    /// Model parameter count in billions (e.g. 7.0 for a 7B model).
    /// Used at runtime to estimate memory for dynamic context resizing.
    #[serde(default, skip_serializing)]
    pub params_b: f64,

    /// Quantization tag (e.g. `"Q4_K_M"`).
    /// Used at runtime to estimate memory for dynamic context resizing.
    #[serde(default, skip_serializing)]
    pub quant: String,

    /// Maximum context length the model was trained on (in tokens).
    /// The runtime context size is never grown beyond this value.
    #[serde(default, skip_serializing)]
    pub max_context_length: u32,

    // ── TurboQuant KV-cache settings (llama-cpp-4 ≥ 0.2.20) ──────────────────
    /// Storage type for the **K** (key) KV-cache tensors.
    ///
    /// Options: `"f16"` (default, highest quality), `"q8_0"` (saves ~47% VRAM,
    /// near-lossless with TurboQuant), `"q5_0"` (saves ~69%), `"q4_0"` (saves
    /// ~75%).  Combining quantized types with `attn_rot_disabled = false`
    /// (default) keeps output quality high at reduced memory.
    #[serde(default = "default_cache_type_k")]
    pub cache_type_k: String,

    /// Storage type for the **V** (value) KV-cache tensors.
    ///
    /// Same options as `cache_type_k`.  V-cache quantization is generally
    /// lossier than K-cache quantization; `"f16"` is the safest choice.
    #[serde(default = "default_cache_type_v")]
    pub cache_type_v: String,

    /// Disable the TurboQuant attention rotation (llama.cpp PR #21038).
    ///
    /// When `false` (the default), llama.cpp applies a Hadamard rotation to
    /// Q/K/V tensors before writing them to the KV cache.  This significantly
    /// improves the quality of quantized KV caches at near-zero overhead.
    /// Set to `true` only if you experience compatibility issues with a
    /// particular model.
    #[serde(default)]
    pub attn_rot_disabled: bool,
}

fn default_llm_parallel() -> usize {
    1
}
fn default_mmproj_n_threads() -> i32 {
    4
}
fn default_autoload_mmproj() -> bool {
    true
}
fn default_flash_attention() -> bool {
    true
}
fn default_offload_kqv() -> bool {
    true
}
fn default_gpu_memory_threshold() -> f64 {
    0.5
}
fn default_gpu_memory_gen_threshold() -> f64 {
    0.3
}
fn default_cache_type_k() -> String {
    "f16".into()
}
fn default_cache_type_v() -> String {
    "f16".into()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model_path: None,
            n_gpu_layers: u32::MAX,
            ctx_size: None,
            parallel: default_llm_parallel(),
            api_key: None,
            tools: LlmToolConfig::default(),
            mmproj: None,
            mmproj_n_threads: default_mmproj_n_threads(),
            no_mmproj_gpu: false,
            autoload_mmproj: default_autoload_mmproj(),
            verbose: false,
            autostart: false,
            n_batch: None,
            n_ubatch: None,
            flash_attention: default_flash_attention(),
            offload_kqv: default_offload_kqv(),
            gpu_memory_threshold: default_gpu_memory_threshold(),
            gpu_memory_gen_threshold: default_gpu_memory_gen_threshold(),
            params_b: 0.0,
            quant: String::new(),
            max_context_length: 0,
            cache_type_k: default_cache_type_k(),
            cache_type_v: default_cache_type_v(),
            attn_rot_disabled: false,
        }
    }
}
