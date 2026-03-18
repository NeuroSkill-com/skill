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

    /// Enable flash attention (KV cache in f16 instead of f32, faster on
    /// GPU backends that support it — Metal, CUDA, Vulkan).  Default: `true`.
    #[serde(default = "default_flash_attention")]
    pub flash_attention: bool,

    /// Offload the KQV tensor operations to the GPU even when not all layers
    /// are offloaded.  Default: `true`.
    #[serde(default = "default_offload_kqv")]
    pub offload_kqv: bool,
}

fn default_llm_parallel()      -> usize { 1 }
fn default_mmproj_n_threads()  -> i32   { 4 }
fn default_autoload_mmproj()   -> bool  { true }
fn default_flash_attention()   -> bool  { true }
fn default_offload_kqv()       -> bool  { true }

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enabled:          false,
            model_path:       None,
            n_gpu_layers:     u32::MAX,
            ctx_size:         None,
            parallel:         default_llm_parallel(),
            api_key:          None,
            tools:            LlmToolConfig::default(),
            mmproj:           None,
            mmproj_n_threads: default_mmproj_n_threads(),
            no_mmproj_gpu:    false,
            autoload_mmproj:  default_autoload_mmproj(),
            verbose:          false,
            autostart:        false,
            flash_attention:  default_flash_attention(),
            offload_kqv:      default_offload_kqv(),
        }
    }
}
