// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! LLM configuration types — shared between the skill-llm crate and the
//! main application.

use serde::{Deserialize, Serialize};

// ── Tool configuration ────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmToolConfig {
    pub date:       bool,
    pub location:   bool,
    pub web_search: bool,
    pub web_fetch:  bool,

    /// Allow the LLM to execute bash/shell commands.
    #[serde(default)]
    pub bash: bool,

    /// Allow the LLM to read file contents.
    #[serde(default)]
    pub read_file: bool,

    /// Allow the LLM to write/create files.
    #[serde(default)]
    pub write_file: bool,

    /// Allow the LLM to make surgical find-and-replace edits to files.
    #[serde(default)]
    pub edit_file: bool,

    /// Tool execution mode: "parallel" or "sequential".
    /// Parallel: prepare sequentially, execute concurrently.
    /// Sequential: prepare and execute one at a time.
    #[serde(default = "default_tool_execution_mode")]
    pub execution_mode: ToolExecutionMode,

    /// Maximum number of tool-calling rounds per chat turn.
    #[serde(default = "default_max_tool_rounds")]
    pub max_rounds: usize,

    /// Maximum number of tool calls executed per round.
    #[serde(default = "default_max_tool_calls_per_round")]
    pub max_calls_per_round: usize,
}

/// How tool calls from a single assistant message are executed.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolExecutionMode {
    /// Execute tool calls one by one in order.
    Sequential,
    /// Prepare sequentially, then execute allowed tools concurrently.
    Parallel,
}

fn default_tool_execution_mode()       -> ToolExecutionMode { ToolExecutionMode::Parallel }
fn default_max_tool_rounds()           -> usize { 3 }
fn default_max_tool_calls_per_round()  -> usize { 4 }

impl Default for LlmToolConfig {
    fn default() -> Self {
        Self {
            date:               true,
            location:           true,
            web_search:         true,
            web_fetch:          true,
            bash:               false,
            read_file:          false,
            write_file:         false,
            edit_file:          false,
            execution_mode:     default_tool_execution_mode(),
            max_rounds:         default_max_tool_rounds(),
            max_calls_per_round: default_max_tool_calls_per_round(),
        }
    }
}

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

    /// KV-cache / context size in tokens.  `None` → use the model's trained
    /// context length (capped at 4096 tokens to avoid OOM).
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
}

fn default_llm_parallel()      -> usize { 1 }
fn default_mmproj_n_threads()  -> i32   { 4 }
fn default_autoload_mmproj()   -> bool  { true }

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
        }
    }
}
