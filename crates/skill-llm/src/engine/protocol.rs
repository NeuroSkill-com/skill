// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Wire protocol types between axum handlers and the inference actor.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use crate::tools;

// ── Wire protocol ─────────────────────────────────────────────────────────────

pub enum InferRequest {
    /// Generate a chat completion from a list of `{"role","content"}` messages.
    /// The actor applies `model.apply_chat_template()` so the correct EOS/stop
    /// tokens are always used regardless of the model family.
    /// `images` holds raw image bytes (decoded from base64 data-URLs or fetched
    /// from URLs) in the same order as the `image_url` parts across all messages.
    Generate {
        messages: Vec<Value>,
        images:   Vec<Vec<u8>>,
        params:   GenParams,
        token_tx: UnboundedSender<InferToken>,
    },
    /// Raw text completion (prompt already formatted by the caller).
    Complete {
        prompt:   String,
        params:   GenParams,
        token_tx: UnboundedSender<InferToken>,
    },
    /// Compute mean-pooled embeddings for a list of strings.
    Embed {
        inputs:    Vec<String>,
        result_tx: tokio::sync::oneshot::Sender<Result<Vec<Vec<f32>>, String>>,
    },
    /// Embed a single image via the loaded mmproj vision projector.
    /// Used by the screenshot worker for visual-similarity embeddings.
    /// Returns `None` if no mmproj is loaded or encoding fails.
    EmbedImage {
        bytes:     Vec<u8>,
        result_tx: tokio::sync::oneshot::Sender<Option<Vec<f32>>>,
    },
    /// Simple liveness probe (kept for future use; status now via `AtomicBool`).
    #[allow(dead_code)]
    Health {
        result_tx: tokio::sync::oneshot::Sender<bool>,
    },
}

pub enum InferToken {
    /// A piece of decoded text to stream to the client.
    Delta(String),
    /// Generation finished normally.
    Done {
        finish_reason:     String,
        prompt_tokens:     usize,
        completion_tokens: usize,
        n_ctx:             usize,
    },
    /// Generation aborted with an error.
    Error(String),
}

// ── Request / response types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GenParams {
    pub temperature:      f32,
    pub top_k:            i32,
    pub top_p:            f32,
    pub repeat_penalty:   f32,
    pub seed:             u32,
    pub max_tokens:       usize,
    pub stop:             Vec<String>,
    /// Maximum tokens the model may spend inside a `<think>…</think>` block.
    ///
    /// `None`  = unlimited thinking (default off — model decides).
    /// `Some(0)` = skip thinking entirely (pre-fill empty `<think>\n\n</think>`).
    /// `Some(n)` = force-close the think block after `n` tokens.
    #[serde(default)]
    pub thinking_budget:  Option<u32>,
}

impl Default for GenParams {
    fn default() -> Self {
        Self {
            temperature:    0.8,
            top_k:          40,
            top_p:          0.9,
            repeat_penalty: 1.1,
            seed:           0xDEAD_BEEF,
            max_tokens:     2048,
            stop:           Vec::new(),
            // Default: minimal (512 tokens) so simple queries don't over-think.
            thinking_budget: Some(512),
        }
    }
}

// Chat completions request
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<Value>,
    #[serde(default)]
    pub tools:    Vec<tools::Tool>,
    #[serde(default)]
    pub tool_choice: Option<Value>,
    #[serde(default)]
    pub stream:   bool,
    #[serde(flatten)]
    pub gen:      GenParams,
}

// Text completions request
#[derive(Debug, Deserialize)]
pub struct CompletionRequest {
    pub prompt: Value, // String or Vec<String>
    #[serde(default)]
    pub stream: bool,
    #[serde(flatten)]
    pub gen:    GenParams,
}

// Embeddings request
#[derive(Debug, Deserialize)]
pub struct EmbeddingsRequest {
    pub input: Value, // String or Vec<String>
}
