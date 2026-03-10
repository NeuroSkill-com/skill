// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! OpenAI-compatible LLM inference server — native llama-cpp-4 backend.
//!
//! # Architecture
//!
//! A dedicated OS thread ("actor") owns the `LlamaBackend`, `LlamaModel`, and
//! `LlamaContext`.  Axum HTTP handlers communicate with the actor through a
//! pair of channels:
//!
//! ```text
//!  axum handler  ──InferRequest──▶  actor thread
//!  axum handler  ◀──InferToken ──  actor thread   (unbounded mpsc per request)
//! ```
//!
//! This design sidesteps all `LlamaContext<'model>` lifetime issues: the actor
//! owns both the model and the context in a single scope, so lifetimes are
//! trivially satisfied.
//!
//! # Concurrency
//!
//! The actor processes requests one at a time (the llama.cpp decode loop is
//! not thread-safe).  The `InferRequest` channel's sender-side is held behind
//! an `Arc<Mutex<>>`, so multiple concurrent HTTP requests will queue up behind
//! the actor without deadlocking.
//!
//! # Endpoints
//!
//! | Method | Path                     | Description                     |
//! |--------|--------------------------|---------------------------------|
//! | GET    | `/health`                | Own liveness + model ready state|
//! | GET    | `/v1/models`             | List loaded model               |
//! | POST   | `/v1/chat/completions`   | Chat (streaming SSE + JSON)     |
//! | POST   | `/v1/completions`        | Raw text completion             |
//! | POST   | `/v1/embeddings`         | Dense embeddings (mean pool)    |
//!
//! # Feature flags
//!
//! | Flag         | Effect                                          |
//! |--------------|-------------------------------------------------|
//! | `llm`        | Core: model loading + inference actor           |
//! | `llm-metal`  | Metal GPU offload (macOS)                       |
//! | `llm-cuda`   | CUDA GPU offload (NVIDIA)                       |
//! | `llm-vulkan` | Vulkan GPU offload (cross-platform)             |
//! | `llm-mtmd`   | Multimodal: image/audio via libmtmd             |

pub mod tools;
pub mod catalog;
pub mod cmds;

use std::{
    collections::VecDeque,
    num::NonZeroU32,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, sse},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc::{self, UnboundedSender};
use tauri::Emitter as _;

use llama_cpp_4::{
    context::params::{LlamaContextParams, LlamaPoolingType},
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{AddBos, LlamaModel, Special, params::LlamaModelParams},
    sampling::LlamaSampler,
};

use crate::settings::LlmConfig;
use catalog::LlmCatalog;

// ── Log buffer ────────────────────────────────────────────────────────────────

/// One line in the LLM server log.
#[derive(Debug, Clone, Serialize)]
pub struct LlmLogEntry {
    /// Unix timestamp in milliseconds.
    pub ts:      u64,
    /// `"info"` | `"warn"` | `"error"`
    pub level:   String,
    /// Human-readable message.
    pub message: String,
}

/// Shared log ring-buffer (max [`LOG_CAP`] entries, oldest dropped first).
pub type LlmLogBuffer = Arc<Mutex<VecDeque<LlmLogEntry>>>;

/// Maximum number of log lines kept in memory.
const LOG_CAP: usize = 500;

/// Create a new, empty log buffer.
pub fn new_log_buffer() -> LlmLogBuffer {
    Arc::new(Mutex::new(VecDeque::with_capacity(LOG_CAP)))
}

/// Optional file sink for LLM log lines.
///
/// `Arc<Mutex<…>>` so both `push_log` (called from any thread via macros)
/// and `run_actor` (which creates it) can hold a reference.
pub type LlmLogFile = Arc<Mutex<std::io::BufWriter<std::fs::File>>>;

/// Append a log entry to the in-memory buffer, emit a `llm:log` Tauri event,
/// and optionally write to the per-session log file.
fn push_log_inner(
    app:      &tauri::AppHandle,
    buf:      &LlmLogBuffer,
    file:     Option<&LlmLogFile>,
    level:    &str,
    msg:      &str,
) {
    eprintln!("[llm][{level}] {msg}");
    let ts    = unix_ts_ms();
    let entry = LlmLogEntry { ts, level: level.to_string(), message: msg.to_string() };

    { let mut q = buf.lock().unwrap(); if q.len() >= LOG_CAP { q.pop_front(); } q.push_back(entry.clone()); }
    let _ = app.emit("llm:log", entry);

    if let Some(f) = file {
        use std::io::Write;
        let dt = chrono_iso(ts);
        let _ = writeln!(f.lock().unwrap(), "[{dt}] [{level:5}] {msg}");
    }
}

/// Convenience wrapper — no file sink (used from axum handlers / cmds).
pub fn push_log(app: &tauri::AppHandle, buf: &LlmLogBuffer, level: &str, msg: &str) {
    push_log_inner(app, buf, None, level, msg);
}

/// Format a Unix-ms timestamp as `HH:MM:SS.mmm` (no libc/chrono dependency).
fn chrono_iso(ts_ms: u64) -> String {
    let total_s  = ts_ms / 1000;
    let ms       = ts_ms % 1000;
    let secs     = total_s % 60;
    let mins     = (total_s / 60) % 60;
    let hours    = (total_s / 3600) % 24;
    format!("{hours:02}:{mins:02}:{secs:02}.{ms:03}")
}

// Actor-side macros include the optional file sink.
macro_rules! llm_info  { ($app:expr, $buf:expr, $file:expr, $($t:tt)*) => { push_log_inner($app, $buf, $file, "info",  &format!($($t)*)) } }
macro_rules! llm_warn  { ($app:expr, $buf:expr, $file:expr, $($t:tt)*) => { push_log_inner($app, $buf, $file, "warn",  &format!($($t)*)) } }
macro_rules! llm_error { ($app:expr, $buf:expr, $file:expr, $($t:tt)*) => { push_log_inner($app, $buf, $file, "error", &format!($($t)*)) } }

// ── Wire protocol between axum handlers and the actor ─────────────────────────

enum InferRequest {
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
    Done { finish_reason: String },
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
struct ChatRequest {
    messages: Vec<Value>,
    #[serde(default)]
    stream:   bool,
    #[serde(flatten)]
    gen:      GenParams,
}

// Text completions request
#[derive(Debug, Deserialize)]
struct CompletionRequest {
    prompt: Value, // String or Vec<String>
    #[serde(default)]
    stream: bool,
    #[serde(flatten)]
    gen:    GenParams,
}

// Embeddings request
#[derive(Debug, Deserialize)]
struct EmbeddingsRequest {
    input: Value, // String or Vec<String>
}

// ── Shared state (held in axum Router via `.with_state()`) ────────────────────

pub struct LlmServerState {
    /// Channel to the inference actor.
    req_tx:      tokio::sync::mpsc::UnboundedSender<InferRequest>,
    /// Display name shown in `/v1/models`.
    model_name:  String,
    /// Optional Bearer token required on every request.
    pub api_key: Option<String>,
    /// Set to `true` by the actor once the model + context are fully loaded.
    pub ready:   Arc<AtomicBool>,
    /// OS thread handle for the actor.  Taken (set to `None`) by `shutdown()`.
    join_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl LlmServerState {
    /// Whether the actor has finished loading the model.
    pub fn is_ready(&self) -> bool { self.ready.load(Ordering::Relaxed) }

    /// Stop the actor and **block until the thread has fully exited**.
    ///
    /// Dropping `req_tx` closes the channel → the actor's `blocking_recv()`
    /// loop returns `None` → the actor drops `ctx`, `model`, and backend in
    /// the correct order → Metal/CUDA resources are released.
    ///
    /// Must be called while the caller holds the **only** remaining
    /// `Arc<LlmServerState>` (i.e. the cell has already been taken).
    pub fn shutdown(self) {
        // Taking the join handle *before* `req_tx` is dropped prevents a race
        // where the thread exits and the handle becomes invalid.
        let handle = self.join_handle.lock().unwrap().take();
        // Dropping `self` here also drops `req_tx`, closing the channel.
        drop(self);
        if let Some(h) = handle {
            let _ = h.join();
        }
    }
}

/// A dynamic cell that holds the (optional) running server state.
///
/// The axum router always has `/v1/*` routes registered; they check this cell
/// at request time and return 503 when `None` (server stopped/not started).
/// `start_llm_server` / `stop_llm_server` Tauri commands swap the contents.
pub type LlmStateCell = Arc<Mutex<Option<Arc<LlmServerState>>>>;

/// Create a new, empty server state cell.
pub fn new_state_cell() -> LlmStateCell {
    Arc::new(Mutex::new(None))
}

/// Gracefully stop the server referenced by `cell`, blocking until the actor
/// thread has fully exited.  Safe to call from any thread, including the Tauri
/// `RunEvent::Exit` handler.  No-op if the server is not running.
pub fn shutdown_cell(cell: &LlmStateCell) {
    if let Some(server_state) = cell.lock().unwrap().take() {
        match Arc::try_unwrap(server_state) {
            Ok(owned) => owned.shutdown(),
            Err(arc)  => drop(arc),   // in-flight axum handler; actor exits when arc drops
        }
    }
}

// ── Server status ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmStatus { Stopped, Loading, Running }

/// Query the current server status from the cell.
pub fn cell_status(cell: &LlmStateCell) -> (LlmStatus, String) {
    match &*cell.lock().unwrap() {
        None    => (LlmStatus::Stopped, String::new()),
        Some(s) => (
            if s.is_ready() { LlmStatus::Running } else { LlmStatus::Loading },
            s.model_name.clone(),
        ),
    }
}

// ── Actor thread ───────────────────────────────────────────────────────────────

// ── Think-budget tracker ──────────────────────────────────────────────────────

/// Tracks the model's `<think>…</think>` block and enforces a token budget.
///
/// Feed every decoded piece via `feed()`.  When the budget is exhausted the
/// method returns `Some("\n</think>\n")` — that string should be:
///   1. Appended to the outgoing `pending` buffer (so the UI sees it), and
///   2. Tokenised and decoded into the KV cache (so the model continues from
///      a logically consistent state after the closing tag).
struct ThinkTracker {
    budget:    Option<u32>,
    inside:    bool,
    closed:    bool,
    tag_buf:   String,   // accumulate chars to detect multi-token tags
    tok_count: u32,
}

impl ThinkTracker {
    fn new(budget: Option<u32>) -> Self {
        Self { budget, inside: false, closed: false, tag_buf: String::new(), tok_count: 0 }
    }

    /// Returns `Some(inject)` if the think block must be force-closed now.
    fn feed(&mut self, piece: &str) -> Option<String> {
        if self.closed { return None; }

        self.tag_buf.push_str(piece);
        // Keep tag_buf bounded — only need enough to detect the longest tag
        let cap = "</think>".len() + 4;
        if self.tag_buf.len() > cap * 2 {
            let drain = self.tag_buf.len() - cap;
            self.tag_buf.drain(..drain);
        }

        if !self.inside {
            // Detect <think> opening
            if self.tag_buf.contains("<think>") {
                self.inside = true;
                // Trim everything up to and including the opening tag
                if let Some(p) = self.tag_buf.find("<think>") {
                    self.tag_buf = self.tag_buf[p + 7..].to_string();
                }
            }
            return None;
        }

        // Inside the think block
        self.tok_count += 1;

        // Check for natural close
        if self.tag_buf.contains("</think>") {
            self.inside = false;
            self.closed = true;
            self.tag_buf.clear();
            return None;
        }

        // Enforce budget
        if let Some(budget) = self.budget {
            if self.tok_count >= budget {
                self.inside = false;
                self.closed = true;
                self.tag_buf.clear();
                return Some("\n</think>\n".to_string());
            }
        }
        None
    }
}

// ── Generation helper ─────────────────────────────────────────────────────────

/// Execute one generation pass: tokenise `prompt`, decode the prompt batch,
/// run the sampling loop with a hold-back stop-string buffer, and stream
/// `InferToken` messages back through `token_tx`.
///
/// The hold-back buffer works like this:
///   – Every decoded piece is appended to `pending`.
///   – We emit only the prefix of `pending` that is guaranteed to NOT be the
///     start of any stop string (i.e. everything except the last
///     `max_stop_len - 1` characters).
///   – On loop exit we flush whatever is left, trimming any trailing stop string.
///
/// This means stop strings that span multiple token pieces are handled
/// correctly without blocking the stream for more than a few bytes.
// ── Image decoding helpers (available to any code, used by the actor) ─────────

/// Decode a base64 data-URL (`data:<mime>;base64,<data>`) or return `None`
/// for plain HTTP/S URLs (which we cannot fetch synchronously from the actor).
fn decode_image_url(url: &str) -> Option<Vec<u8>> {
    let data = url.strip_prefix("data:")?;
    // data:<mime>;base64,<payload>
    let payload = data.split(';').nth(1)?.strip_prefix("base64,")?;
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.decode(payload).ok()
}

/// Extract all raw image bytes from a single `content` value (string or parts array).
/// Returns images in document order.
fn extract_images_from_content(content: &Value) -> Vec<Vec<u8>> {
    let Value::Array(parts) = content else { return Vec::new() };
    parts.iter()
        .filter_map(|p| {
            if p.get("type")?.as_str() != Some("image_url") { return None; }
            let url = p.get("image_url")?.get("url")?.as_str()?;
            decode_image_url(url)
        })
        .collect()
}

// ── Shared sampling loop ───────────────────────────────────────────────────────

/// Run the token-by-token generation loop starting at `n_prompt` KV positions.
///
/// Precondition: the KV cache already contains the fully-decoded prompt (text
/// or text+images) and the logits for the last prompt position are valid.
/// `sampler.sample(ctx, -1)` samples from those logits.
#[allow(clippy::too_many_arguments)]
fn run_sampling_loop(
    model:    &llama_cpp_4::model::LlamaModel,
    ctx:      &mut llama_cpp_4::context::LlamaContext<'_>,
    app:      &tauri::AppHandle,
    log_buf:  &LlmLogBuffer,
    log_file: Option<&LlmLogFile>,
    params:   &GenParams,
    token_tx: UnboundedSender<InferToken>,
    n_prompt: usize,
) {
    let n_ctx = ctx.n_ctx() as usize;

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::top_k(params.top_k),
        LlamaSampler::top_p(params.top_p, 1),
        LlamaSampler::temp(params.temperature),
        LlamaSampler::dist(params.seed),
    ]);

    // Stop strings: user-supplied + model-family defaults.
    let mut stop_strings = params.stop.clone();
    for s in &["<|im_end|>", "<|endoftext|>", "<|user|>",
                "<|eot_id|>", "<|EOT|>", "[/INST]"] {
        if !stop_strings.iter().any(|x| x == s) {
            stop_strings.push(s.to_string());
        }
    }
    let max_stop_len = stop_strings.iter().map(|s| s.len()).max().unwrap_or(0);
    let hold_back    = max_stop_len.saturating_sub(1);

    // Think-budget tracker (budget=0 is handled before this call; None = unlimited)
    let tracker_budget = match params.thinking_budget {
        Some(0) | None => None,
        Some(n)        => Some(n),
    };
    let mut think_tracker = ThinkTracker::new(tracker_budget);

    let max_new = params.max_tokens.min(n_ctx.saturating_sub(n_prompt));
    let mut n_cur = n_prompt;
    let mut finish_reason = "length".to_string();
    let mut pending = String::new();

    'gen: loop {
        if n_cur >= n_prompt + max_new { break; }

        // -1 = "last token that had logits computed" — works after both
        // `ctx.decode()` (text-only path) and `eval_chunks()` (mtmd path).
        let token = sampler.sample(ctx, -1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            finish_reason = "stop".to_string();
            break;
        }

        let piece = model.token_to_str(token, Special::Plaintext).unwrap_or_default();

        // Think-budget enforcement: inject </think> when budget exhausted.
        if let Some(inject) = think_tracker.feed(&piece) {
            token_tx.send(InferToken::Delta(inject.clone())).ok();

            if let Ok(inj_toks) = model.str_to_token(&inject, AddBos::Never) {
                if !inj_toks.is_empty() {
                    let mut inj_batch = LlamaBatch::new(inj_toks.len(), 1);
                    for (i, &t) in inj_toks.iter().enumerate() {
                        inj_batch.add(t, n_cur as i32 + i as i32, &[0],
                                      i == inj_toks.len() - 1).ok();
                    }
                    if ctx.decode(&mut inj_batch).is_err() {
                        llm_warn!(app, log_buf, log_file, "decode error injecting </think>");
                    }
                    n_cur += inj_toks.len();
                }
            }
            continue;
        }

        pending.push_str(&piece);

        // Check for stop strings.
        for stop in &stop_strings {
            if pending.ends_with(stop.as_str()) {
                let safe_end = pending.len().saturating_sub(stop.len());
                if safe_end > 0 {
                    token_tx.send(InferToken::Delta(pending[..safe_end].to_string())).ok();
                }
                finish_reason = "stop".to_string();
                break 'gen;
            }
        }

        // Emit safe prefix (hold back potential partial stop string).
        if pending.len() > hold_back {
            let emit_end = pending.len() - hold_back;
            let emit_end = (0..=emit_end).rev()
                .find(|&i| pending.is_char_boundary(i))
                .unwrap_or(0);
            if emit_end > 0 {
                let chunk: String = pending.drain(..emit_end).collect();
                if token_tx.send(InferToken::Delta(chunk)).is_err() { break; }
            }
        }

        // Decode the new token so `sampler.sample(ctx, -1)` works next iteration.
        let mut gen_batch = LlamaBatch::new(1, 1);
        if gen_batch.add(token, n_cur as i32, &[0], true).is_err() { break; }
        if ctx.decode(&mut gen_batch).is_err() {
            token_tx.send(InferToken::Error("decode error".into())).ok();
            break;
        }
        n_cur += 1;
    }

    // Flush hold-back buffer, trimming any trailing stop string.
    let flush_end = stop_strings.iter()
        .find_map(|s| pending.ends_with(s.as_str()).then_some(pending.len().saturating_sub(s.len())))
        .unwrap_or(pending.len());
    if flush_end > 0 {
        token_tx.send(InferToken::Delta(pending[..flush_end].to_string())).ok();
    }

    let n_gen = n_cur.saturating_sub(n_prompt);
    llm_info!(app, log_buf, log_file, "generation done — {n_gen} tokens, finish_reason={finish_reason}");
    token_tx.send(InferToken::Done { finish_reason }).ok();
}

// ── Text-only generation ───────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn run_generation(
    model:    &llama_cpp_4::model::LlamaModel,
    ctx:      &mut llama_cpp_4::context::LlamaContext<'_>,
    app:      &tauri::AppHandle,
    log_buf:  &LlmLogBuffer,
    log_file: Option<&LlmLogFile>,
    prompt:   String,
    params:   GenParams,
    token_tx: UnboundedSender<InferToken>,
) {
    ctx.clear_kv_cache();

    // When thinking is disabled, pre-fill an empty <think>\n\n</think>\n block.
    let prompt = if params.thinking_budget == Some(0) {
        format!("{prompt}<think>\n\n</think>\n")
    } else {
        prompt
    };

    let Ok(tokens) = model.str_to_token(&prompt, AddBos::Always) else {
        token_tx.send(InferToken::Error("tokenization failed".into())).ok();
        return;
    };
    let n_prompt = tokens.len();
    let n_ctx    = ctx.n_ctx() as usize;

    llm_info!(app, log_buf, log_file, "prompt: {n_prompt} tokens, thinking_budget={:?}", params.thinking_budget);
    if n_prompt >= n_ctx {
        let msg = format!("prompt too long ({n_prompt} ≥ n_ctx {n_ctx})");
        llm_warn!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    let mut batch = LlamaBatch::new(n_ctx, 1);
    for (i, &tok) in tokens.iter().enumerate() {
        if batch.add(tok, i as i32, &[0], i == n_prompt - 1).is_err() { break; }
    }
    if ctx.decode(&mut batch).is_err() {
        llm_error!(app, log_buf, log_file, "decode error on prompt");
        token_tx.send(InferToken::Error("decode error on prompt".into())).ok();
        return;
    }

    run_sampling_loop(model, ctx, app, log_buf, log_file, &params, token_tx, n_prompt);
}

// ── Multimodal generation (llm-mtmd feature) ──────────────────────────────────

#[cfg(feature = "llm-mtmd")]
#[allow(clippy::too_many_arguments)]
fn run_generation_multimodal(
    model:     &llama_cpp_4::model::LlamaModel,
    ctx:       &mut llama_cpp_4::context::LlamaContext<'_>,
    mtmd_ctx:  &llama_cpp_4::mtmd::MtmdContext,
    app:       &tauri::AppHandle,
    log_buf:   &LlmLogBuffer,
    log_file:  Option<&LlmLogFile>,
    prompt:    String,   // contains media markers in place of image_url parts
    images:    Vec<Vec<u8>>,
    params:    GenParams,
    token_tx:  UnboundedSender<InferToken>,
) {
    use llama_cpp_4::mtmd::{MtmdBitmap, MtmdInputChunks, MtmdInputText};

    ctx.clear_kv_cache();

    let n_ctx = ctx.n_ctx() as usize;

    // When thinking is disabled, pre-fill an empty <think>\n\n</think>\n block.
    let prompt = if params.thinking_budget == Some(0) {
        format!("{prompt}<think>\n\n</think>\n")
    } else {
        prompt
    };

    // Decode raw bytes → MtmdBitmap (auto-detects JPEG/PNG/etc.)
    let bitmaps: Vec<MtmdBitmap> = images.iter()
        .enumerate()
        .filter_map(|(i, bytes)| {
            match MtmdBitmap::from_buf(mtmd_ctx, bytes) {
                Ok(b)  => Some(b),
                Err(e) => {
                    llm_warn!(app, log_buf, log_file, "image {i} decode failed: {e}");
                    None
                }
            }
        })
        .collect();

    if bitmaps.is_empty() && !images.is_empty() {
        token_tx.send(InferToken::Error("all images failed to decode".into())).ok();
        return;
    }

    llm_info!(app, log_buf, log_file,
        "multimodal prompt — {} image(s), thinking_budget={:?}",
        bitmaps.len(), params.thinking_budget);

    let bitmap_refs: Vec<&MtmdBitmap> = bitmaps.iter().collect();
    let text = MtmdInputText::new(&prompt, true, true);
    let mut chunks = MtmdInputChunks::new();

    if let Err(e) = mtmd_ctx.tokenize(&text, &bitmap_refs, &mut chunks) {
        let msg = format!("mtmd tokenize error: {e}");
        llm_error!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    let n_tokens = chunks.n_tokens();
    llm_info!(app, log_buf, log_file, "prompt+images: ~{n_tokens} tokens");
    if n_tokens >= n_ctx {
        let msg = format!("prompt+images too long ({n_tokens} ≥ n_ctx {n_ctx})");
        llm_warn!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    let n_batch = ctx.n_batch() as i32;
    let mut n_past = 0i32;
    if let Err(e) = mtmd_ctx.eval_chunks(ctx.as_ptr(), &chunks, 0, 0, n_batch, true, &mut n_past) {
        let msg = format!("mtmd eval error: {e}");
        llm_error!(app, log_buf, log_file, "{msg}");
        token_tx.send(InferToken::Error(msg)).ok();
        return;
    }

    let n_prompt = n_past as usize;
    run_sampling_loop(model, ctx, app, log_buf, log_file, &params, token_tx, n_prompt);
}

#[allow(clippy::too_many_arguments)]
fn run_actor(
    mut rx:       tokio::sync::mpsc::UnboundedReceiver<InferRequest>,
    config:       LlmConfig,
    model_path:   std::path::PathBuf,
    mmproj_path:  Option<std::path::PathBuf>,
    app:          tauri::AppHandle,
    log_buf:      LlmLogBuffer,
    log_path:     Option<std::path::PathBuf>,
    ready_flag:   Arc<AtomicBool>,
) {
    // ── per-session log file ──────────────────────────────────────────────────
    let log_file_handle: Option<LlmLogFile> = log_path.as_ref().and_then(|p| {
        std::fs::OpenOptions::new()
            .create(true).write(true).truncate(true)
            .open(p).ok()
            .map(|f| Arc::new(Mutex::new(std::io::BufWriter::new(f))))
    });
    let log_file = log_file_handle.as_ref();

    // ── init backend ──────────────────────────────────────────────────────────
    // llama-cpp-4's backend is a process-wide singleton gated by an AtomicBool.
    // neutts (if compiled in) may have already called init(); that returns
    // BackendAlreadyInitialized.  Either way the native library is ready.
    //
    // We wrap the handle in ManuallyDrop to prevent our Drop from calling
    // llama_backend_free() — neutts may still need the singleton.
    // LlamaBackend is a zero-field unit struct (a compile-time proof token),
    // so mem::zeroed() is valid and Deref/DerefMut work transparently.
    // `LlamaBackend` is a process-wide singleton.  Track whether *we* called
    // `init()` so we know whether to free it when the actor exits.
    // If neutts already holds the singleton, we get a ZST proxy but must NOT
    // call `llama_backend_free` — neutts will do it.
    let (mut backend_md, we_own_backend) = match LlamaBackend::init() {
        Ok(b) => {
            llm_info!(&app, &log_buf, log_file, "llama backend initialised");
            (std::mem::ManuallyDrop::new(b), true)
        }
        Err(_) => {
            llm_info!(&app, &log_buf, log_file, "llama backend already initialised (shared with neutts)");
            // SAFETY: ZST — no data, no pointers.
            (std::mem::ManuallyDrop::new(unsafe { std::mem::zeroed::<LlamaBackend>() }), false)
        }
    };
    backend_md.void_logs(); // silence llama.cpp's verbose stderr
    let backend: &LlamaBackend = &backend_md;

    // ── load model ──
    llm_info!(&app, &log_buf, log_file, "loading model: {}", model_path.display());
    let model_params = LlamaModelParams::default()
        .with_n_gpu_layers(config.n_gpu_layers);

    let model = match LlamaModel::load_from_file(backend, &model_path, &model_params) {
        Ok(m)  => { llm_info!(&app, &log_buf, log_file, "model loaded ✓"); m }
        Err(e) => { llm_error!(&app, &log_buf, log_file, "failed to load model: {e}"); return; }
    };

    // ── create generation context ──
    let ctx_size = NonZeroU32::new(config.ctx_size.unwrap_or(4096));
    llm_info!(&app, &log_buf, log_file, "creating context (n_ctx={}, n_gpu_layers={})",
              ctx_size.map_or(0, |n| n.get()), config.n_gpu_layers);
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(ctx_size)
        .with_n_threads(-1)
        .with_n_threads_batch(-1);

    let mut ctx = match model.new_context(backend, ctx_params) {
        Ok(c)  => c,
        Err(e) => { llm_error!(&app, &log_buf, log_file, "failed to create context: {e}"); return; }
    };

    llm_info!(&app, &log_buf, log_file, "context ready — n_ctx={} — running warmup pass…", ctx.n_ctx());
    let _ = app.emit("llm:status", json!({"status":"loading","detail":"warming_up"}));

    // ── Multimodal projector (llm-mtmd feature) ───────────────────────────────
    #[cfg(feature = "llm-mtmd")]
    let mtmd_ctx: Option<llama_cpp_4::mtmd::MtmdContext> = mmproj_path.as_ref().and_then(|p| {
        use llama_cpp_4::mtmd::{MtmdContext, MtmdContextParams};
        match MtmdContext::init_from_file(p, &model, MtmdContextParams::default()) {
            Ok(mc) => {
                llm_info!(&app, &log_buf, log_file,
                    "mmproj loaded ✓ — vision={} audio={}",
                    mc.supports_vision(), mc.supports_audio());
                Some(mc)
            }
            Err(e) => {
                llm_error!(&app, &log_buf, log_file, "failed to load mmproj: {e}");
                None
            }
        }
    });
    #[cfg(not(feature = "llm-mtmd"))]
    let _ = &mmproj_path; // suppress unused warning

    // ── Warmup / prewarm ──────────────────────────────────────────────────────
    // Running one tiny decode pass compiles Metal/CUDA/Vulkan shader graphs,
    // transfers weights to VRAM, and allocates the KV-cache backing store so
    // the very first real user request is not penalised.
    //
    // We feed a single BOS token, decode it, then clear the KV cache so the
    // context is pristine for the first real request.
    let warmup_ok = (|| -> bool {
        // Use the model's BOS token; fall back to token 1 (almost universal).
        let bos = model.token_bos();
        let warmup_tokens = if let Ok(toks) = model.str_to_token(" ", AddBos::Always) {
            toks
        } else {
            vec![bos]
        };
        let n = warmup_tokens.len().min(4); // at most 4 tokens
        let mut batch = LlamaBatch::new(n, 1);
        for (i, &tok) in warmup_tokens[..n].iter().enumerate() {
            let last = i == n - 1;
            if batch.add(tok, i as i32, &[0], last).is_err() { return false; }
        }
        let ok = ctx.decode(&mut batch).is_ok();
        ctx.clear_kv_cache();
        ok
    })();

    if warmup_ok {
        llm_info!(&app, &log_buf, log_file, "warmup complete — GPU kernels compiled, weights in VRAM");
    } else {
        llm_warn!(&app, &log_buf, log_file, "warmup decode failed — first request may be slow");
    }

    // Signal that the model is fully loaded and warmed up.
    ready_flag.store(true, Ordering::Relaxed);
    let model_file = model_path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    llm_info!(&app, &log_buf, log_file, "server ready — model={}", model_file);
    let _ = app.emit("llm:status", json!({"status":"running","model":model_file}));

    // ── event loop ──
    while let Some(req) = rx.blocking_recv() {
        match req {
            InferRequest::Health { result_tx } => {
                result_tx.send(true).ok();
            }

            InferRequest::Generate { messages, images, params, token_tx } => {
                llm_info!(&app, &log_buf, log_file, "chat request — {} messages, {} image(s), max_tokens={}",
                          messages.len(), images.len(), params.max_tokens);

                // ── Build the prompt text ─────────────────────────────────────
                // Content may be a plain string OR a parts array
                // [{type:"text",text:"…"},{type:"image_url",url:"…"}].
                // For the multimodal path we replace each image_url part with
                // the mtmd media marker; for the text-only path we skip images.

                #[cfg(feature = "llm-mtmd")]
                let use_mtmd = !images.is_empty() && mtmd_ctx.is_some();
                #[cfg(not(feature = "llm-mtmd"))]
                let use_mtmd = false;

                fn extract_text_plain(content: &Value) -> String {
                    match content {
                        Value::String(s) => s.clone(),
                        Value::Array(parts) => parts.iter()
                            .filter_map(|p| {
                                if p.get("type")?.as_str() != Some("text") { return None; }
                                Some(p.get("text")?.as_str()?.to_string())
                            })
                            .collect::<Vec<_>>()
                            .join(" "),
                        _ => String::new(),
                    }
                }

                fn extract_text_with_markers(content: &Value, marker: &str) -> String {
                    match content {
                        Value::String(s) => s.clone(),
                        Value::Array(parts) => parts.iter()
                            .filter_map(|p| {
                                match p.get("type")?.as_str()? {
                                    "text"      => Some(p.get("text")?.as_str()?.to_string()),
                                    "image_url" => Some(marker.to_string()),
                                    _           => None,
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" "),
                        _ => String::new(),
                    }
                }

                let extract_fn: fn(&Value, &str) -> String = if use_mtmd {
                    extract_text_with_markers
                } else {
                    |c, _| extract_text_plain(c)
                };

                #[cfg(feature = "llm-mtmd")]
                let marker = llama_cpp_4::mtmd::MtmdContext::default_marker();
                #[cfg(not(feature = "llm-mtmd"))]
                let marker = "";

                let chat_msgs: Vec<llama_cpp_4::model::LlamaChatMessage> = messages
                    .iter()
                    .filter_map(|m| {
                        let role    = m.get("role")?.as_str()?.to_string();
                        let content = extract_fn(m.get("content")?, marker);
                        llama_cpp_4::model::LlamaChatMessage::new(role, content).ok()
                    })
                    .collect();

                let prompt = match model.apply_chat_template(None, chat_msgs, true) {
                    Ok(p)  => p,
                    Err(e) => {
                        llm_error!(&app, &log_buf, log_file, "apply_chat_template failed: {e}");
                        token_tx.send(InferToken::Error(format!("template error: {e}"))).ok();
                        continue;
                    }
                };

                // ── Dispatch to text-only or multimodal path ──────────────────
                #[cfg(feature = "llm-mtmd")]
                if use_mtmd {
                    if let Some(ref mc) = mtmd_ctx {
                        run_generation_multimodal(&model, &mut ctx, mc,
                            &app, &log_buf, log_file, prompt, images, params, token_tx);
                        continue;
                    }
                }

                run_generation(&model, &mut ctx, &app, &log_buf, log_file,
                               prompt, params, token_tx);
            }

            InferRequest::Complete { prompt, params, token_tx } => {
                llm_info!(&app, &log_buf, log_file, "completion request — max_tokens={}", params.max_tokens);
                run_generation(&model, &mut ctx, &app, &log_buf, log_file,
                               prompt, params, token_tx);
            }

            InferRequest::Embed { inputs, result_tx } => {
                llm_info!(&app, &log_buf, log_file, "embeddings request — {} input(s)", inputs.len());
                // Create a temporary embeddings context (cheap: no KV cache).
                let emb_params = LlamaContextParams::default()
                    .with_n_ctx(NonZeroU32::new(512))
                    .with_embeddings(true)
                    .with_pooling_type(LlamaPoolingType::Mean);

                let mut emb_ctx = match model.new_context(backend, emb_params) {
                    Ok(c)  => c,
                    Err(e) => {
                        result_tx.send(Err(e.to_string())).ok();
                        continue;
                    }
                };

                let embed_result: Result<Vec<Vec<f32>>, String> = (|| {
                    let mut all = Vec::new();
                    for text in &inputs {
                        emb_ctx.clear_kv_cache();

                        let tokens = model.str_to_token(text, AddBos::Always)
                            .map_err(|e| e.to_string())?;
                        let n = tokens.len().min(emb_ctx.n_ctx() as usize - 1);

                        let mut batch = LlamaBatch::new(n + 1, 1);
                        for (i, &tok) in tokens[..n].iter().enumerate() {
                            let last = i == n - 1;
                            batch.add(tok, i as i32, &[0], last).ok();
                        }

                        emb_ctx.decode(&mut batch)
                            .map_err(|_| "embed decode error".to_string())?;

                        let vec = emb_ctx.embeddings_seq_ith(0)
                            .map_err(|e| e.to_string())?;
                        all.push(vec.to_vec());
                    }
                    Ok(all)
                })();

                if let Ok(ref vecs) = embed_result {
                    llm_info!(&app, &log_buf, log_file, "embeddings done — {} vector(s)", vecs.len());
                }
                result_tx.send(embed_result).ok();
            }
        }
    }

    // ── Ordered teardown ──────────────────────────────────────────────────────
    // GPU resources must be released in strict order:
    //   LlamaContext  (holds Metal/CUDA compute state)  → drop first
    //   LlamaModel    (holds weight tensors in VRAM)    → drop second
    //   LlamaBackend  (calls llama_backend_free)        → drop last
    //
    // Rust drops locals in reverse-declaration order, which already gives us
    // ctx → model → backend_md.  We make it explicit with `drop()` calls so
    // the ordering is visible and enforced even if locals are re-arranged.
    drop(ctx);
    drop(model);
    if we_own_backend {
        // SAFETY: we called init() so we own the singleton; ctx and model are
        // already dropped above, so no dangling references to backend remain.
        unsafe { std::mem::ManuallyDrop::drop(&mut backend_md); }
    }
    // else: leave backend free to neutts

    llm_info!(&app, &log_buf, log_file, "actor exiting — GPU resources released");
    let _ = app.emit("llm:status", json!({"status":"stopped"}));
}

// ── Public init ────────────────────────────────────────────────────────────────

/// Initialise the LLM server state.
///
/// Spawns the inference actor thread and returns the shared state used by the
/// axum router.  Returns `None` when:
/// - `config.enabled == false`
/// - No model is selected or the model file does not exist
pub fn init(
    config:    &LlmConfig,
    catalog:   &LlmCatalog,
    app:       tauri::AppHandle,
    log_buf:   LlmLogBuffer,
    skill_dir: &std::path::Path,
) -> Option<Arc<LlmServerState>> {
    if !config.enabled {
        push_log(&app, &log_buf, "info", "LLM server disabled — skipping init");
        return None;
    }

    let model_path = catalog.active_model_path()
        .or_else(|| config.model_path.clone())
        .or_else(|| {
            push_log(&app, &log_buf, "warn", "no model selected — LLM server disabled");
            None
        })?;

    if !model_path.exists() {
        push_log(&app, &log_buf, "error",
            &format!("model file not found: {} — LLM server disabled", model_path.display()));
        return None;
    }

    let mmproj_path = catalog.active_mmproj_path();
    let model_name  = model_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("llama.cpp-model")
        .to_owned();

    push_log(&app, &log_buf, "info", &format!("starting LLM server — model: {model_name}"));

    // ── Per-session log file ──────────────────────────────────────────────────
    // Written to skill_dir/llm_<unix-seconds>.txt so each server run has its
    // own timestamped transcript alongside the other skill log files.
    let ts_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let log_path = skill_dir.join(format!("llm_{ts_secs}.txt"));
    push_log(&app, &log_buf, "info", &format!("session log → {}", log_path.display()));

    let (req_tx, req_rx) = mpsc::unbounded_channel::<InferRequest>();
    let ready_flag = Arc::new(AtomicBool::new(false));

    let config2     = config.clone();
    let path2       = model_path.clone();
    let mmproj2     = mmproj_path.clone();
    let app2        = app.clone();
    let buf2        = log_buf.clone();
    let ready2      = ready_flag.clone();

    let join_handle = std::thread::Builder::new()
        .name("llm-actor".into())
        .stack_size(8 * 1024 * 1024)   // llama.cpp is stack-hungry
        .spawn(move || run_actor(req_rx, config2, path2, mmproj2, app2, buf2, Some(log_path), ready2))
        .expect("failed to spawn llm-actor thread");

    // Emit "loading" status so the chat window can show a spinner.
    let _ = app.emit("llm:status", json!({"status":"loading","model":model_name}));

    Some(Arc::new(LlmServerState {
        req_tx,
        model_name,
        api_key:     config.api_key.clone(),
        ready:       ready_flag,
        join_handle: Mutex::new(Some(join_handle)),
    }))
}

// ── Auth + cell-extraction helpers ────────────────────────────────────────────

fn check_auth(state: &LlmServerState, headers: &axum::http::HeaderMap) -> bool {
    let Some(ref key) = state.api_key else { return true; };
    headers.get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|token| token == key.as_str())
        .unwrap_or(false)
}

/// Lock the cell, clone the inner Arc (cheap), and return an error response if
/// the server is not running.  Usage: `let state = get_state!(cell);`
macro_rules! get_state {
    ($cell:expr) => {{
        match $cell.lock().unwrap().clone() {
            Some(s) => s,
            None => return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error":{
                    "message": "LLM server not running — POST /llm/start or use the Settings → LLM tab",
                    "code":    "server_not_running"
                }})),
            ).into_response(),
        }
    }};
}

macro_rules! require_auth {
    ($state:expr, $headers:expr) => {
        if !check_auth(&$state, &$headers) {
            return (StatusCode::UNAUTHORIZED, Json(json!({
                "error":{"message":"Invalid API key","type":"invalid_request_error","code":"invalid_api_key"}
            }))).into_response();
        }
    };
}

// ── Handlers ───────────────────────────────────────────────────────────────────

async fn health(State(cell): State<LlmStateCell>) -> Response {
    match &*cell.lock().unwrap() {
        None    => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"status":"stopped"}))).into_response(),
        Some(s) => {
            let status = if s.is_ready() { "ok" } else { "loading" };
            Json(json!({"status": status, "model": s.model_name})).into_response()
        }
    }
}

/// `GET /llm/status` — machine-readable server status for external callers.
async fn server_status(State(cell): State<LlmStateCell>) -> Response {
    let (status, model) = cell_status(&cell);
    Json(json!({"status": status, "model": model})).into_response()
}

async fn list_models(
    State(cell): State<LlmStateCell>,
    headers:     axum::http::HeaderMap,
) -> Response {
    let state = get_state!(cell);
    require_auth!(state, headers);
    let ts = unix_ts();
    Json(json!({
        "object": "list",
        "data": [{"id": state.model_name, "object": "model", "created": ts, "owned_by": "skill"}]
    })).into_response()
}

// ── /v1/chat/completions ──────────────────────────────────────────────────────

async fn chat_completions(
    State(cell): State<LlmStateCell>,
    headers:     axum::http::HeaderMap,
    Json(req):   Json<ChatRequest>,
) -> Response {
    let state = get_state!(cell);
    require_auth!(state, headers);

    if !state.is_ready() {
        return (StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error":{"message":"Model is still loading","code":"loading"}}))).into_response();
    }

    // Extract images from all messages in document order.
    // Only base64 data-URLs are supported (plain HTTP URLs are skipped).
    let images: Vec<Vec<u8>> = req.messages.iter()
        .flat_map(|m| {
            m.get("content")
                .map(|c| extract_images_from_content(c))
                .unwrap_or_default()
        })
        .collect();

    let (tok_tx, tok_rx) = mpsc::unbounded_channel();
    let _ = state.req_tx.send(InferRequest::Generate {
        messages: req.messages.clone(),
        images,
        params:   req.gen.clone(),
        token_tx: tok_tx,
    });

    if req.stream {
        stream_chat_response(tok_rx, &state.model_name).await
    } else {
        collect_chat_response(tok_rx, &state.model_name).await
    }
}

// ── /v1/completions ───────────────────────────────────────────────────────────

async fn completions(
    State(cell): State<LlmStateCell>,
    headers:     axum::http::HeaderMap,
    Json(req):   Json<CompletionRequest>,
) -> Response {
    let state = get_state!(cell);
    require_auth!(state, headers);

    if !state.is_ready() {
        return (StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error":{"message":"Model is still loading","code":"loading"}}))).into_response();
    }

    let prompt = match &req.prompt {
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    };
    let (tok_tx, tok_rx) = mpsc::unbounded_channel();
    let _ = state.req_tx.send(InferRequest::Complete {
        prompt, params: req.gen.clone(), token_tx: tok_tx,
    });

    if req.stream {
        stream_completion_response(tok_rx, &state.model_name).await
    } else {
        collect_completion_response(tok_rx, &state.model_name).await
    }
}

// ── /v1/embeddings ────────────────────────────────────────────────────────────

async fn embeddings(
    State(cell): State<LlmStateCell>,
    headers:     axum::http::HeaderMap,
    Json(req):   Json<EmbeddingsRequest>,
) -> Response {
    let state = get_state!(cell);
    require_auth!(state, headers);

    let inputs: Vec<String> = match &req.input {
        Value::String(s) => vec![s.clone()],
        Value::Array(arr) => arr.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        _ => return (StatusCode::BAD_REQUEST, Json(json!({"error":"invalid input"}))).into_response(),
    };

    if !state.is_ready() {
        return (StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error":{"message":"Model is still loading","code":"loading"}}))).into_response();
    }

    let (result_tx, result_rx) = tokio::sync::oneshot::channel();
    let _ = state.req_tx.send(InferRequest::Embed { inputs, result_tx });

    match result_rx.await {
        Ok(Ok(vecs)) => {
            let data: Vec<Value> = vecs.into_iter().enumerate().map(|(i, vec)| json!({
                "object": "embedding", "index": i, "embedding": vec,
            })).collect();
            Json(json!({"object":"list","data":data,"model":state.model_name})).into_response()
        }
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e}))).into_response(),
        Err(_)     => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":"actor died"}))).into_response(),
    }
}

// ── Streaming helpers ─────────────────────────────────────────────────────────

async fn stream_chat_response(
    mut tok_rx: mpsc::UnboundedReceiver<InferToken>,
    model_name: &str,
) -> Response {
    let model_name = model_name.to_owned();
    let id = format!("chatcmpl-{}", short_id());
    let ts = unix_ts();

    let stream = async_stream::stream! {
        while let Some(tok) = tok_rx.recv().await {
            match tok {
                InferToken::Delta(text) => {
                    let data = serde_json::to_string(&json!({
                        "id": id, "object": "chat.completion.chunk",
                        "created": ts, "model": model_name,
                        "choices": [{"index":0,"delta":{"content":text},"finish_reason":null}],
                    })).unwrap_or_default();
                    yield Ok::<sse::Event, String>(sse::Event::default().data(data));
                }
                InferToken::Done { finish_reason } => {
                    let data = serde_json::to_string(&json!({
                        "id": id, "object": "chat.completion.chunk",
                        "created": ts, "model": model_name,
                        "choices": [{"index":0,"delta":{},"finish_reason":finish_reason}],
                    })).unwrap_or_default();
                    yield Ok(sse::Event::default().data(data));
                    yield Ok(sse::Event::default().data("[DONE]"));
                    return;
                }
                InferToken::Error(e) => {
                    let data = serde_json::to_string(&json!({"error":e})).unwrap_or_default();
                    yield Ok(sse::Event::default().data(data));
                    return;
                }
            }
        }
    };

    sse::Sse::new(stream)
        .keep_alive(sse::KeepAlive::default())
        .into_response()
}

async fn collect_chat_response(
    mut tok_rx: mpsc::UnboundedReceiver<InferToken>,
    model_name: &str,
) -> Response {
    let id = format!("chatcmpl-{}", short_id());
    let ts = unix_ts();
    let mut text          = String::new();
    let mut finish_reason = "stop".to_string();

    while let Some(tok) = tok_rx.recv().await {
        match tok {
            InferToken::Delta(t)          => text.push_str(&t),
            InferToken::Done { finish_reason: fr } => { finish_reason = fr; break; }
            InferToken::Error(e)          => {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e}))).into_response();
            }
        }
    }

    Json(json!({
        "id": id, "object": "chat.completion", "created": ts, "model": model_name,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": text},
            "finish_reason": finish_reason,
        }],
        "usage": {"prompt_tokens":0,"completion_tokens":0,"total_tokens":0},
    })).into_response()
}

async fn stream_completion_response(
    mut tok_rx: mpsc::UnboundedReceiver<InferToken>,
    model_name: &str,
) -> Response {
    let model_name = model_name.to_owned();
    let id = format!("cmpl-{}", short_id());
    let ts = unix_ts();

    let stream = async_stream::stream! {
        while let Some(tok) = tok_rx.recv().await {
            match tok {
                InferToken::Delta(text) => {
                    let data = serde_json::to_string(&json!({
                        "id": id, "object": "text_completion.chunk",
                        "created": ts, "model": model_name,
                        "choices": [{"text": text, "index": 0, "finish_reason": null}],
                    })).unwrap_or_default();
                    yield Ok::<sse::Event, String>(sse::Event::default().data(data));
                }
                InferToken::Done { finish_reason } => {
                    let data = serde_json::to_string(&json!({
                        "id": id, "object": "text_completion.chunk",
                        "created": ts, "model": model_name,
                        "choices": [{"text": "", "index": 0, "finish_reason": finish_reason}],
                    })).unwrap_or_default();
                    yield Ok(sse::Event::default().data(data));
                    yield Ok(sse::Event::default().data("[DONE]"));
                    return;
                }
                InferToken::Error(e) => {
                    yield Ok(sse::Event::default().data(
                        serde_json::to_string(&json!({"error":e})).unwrap_or_default()
                    ));
                    return;
                }
            }
        }
    };

    sse::Sse::new(stream)
        .keep_alive(sse::KeepAlive::default())
        .into_response()
}

async fn collect_completion_response(
    mut tok_rx: mpsc::UnboundedReceiver<InferToken>,
    model_name: &str,
) -> Response {
    let id = format!("cmpl-{}", short_id());
    let ts = unix_ts();
    let mut text          = String::new();
    let mut finish_reason = "stop".to_string();

    while let Some(tok) = tok_rx.recv().await {
        match tok {
            InferToken::Delta(t)                   => text.push_str(&t),
            InferToken::Done { finish_reason: fr } => { finish_reason = fr; break; }
            InferToken::Error(e)                   => {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e}))).into_response();
            }
        }
    }

    Json(json!({
        "id": id, "object": "text_completion", "created": ts, "model": model_name,
        "choices": [{"text": text, "index": 0, "finish_reason": finish_reason}],
        "usage": {"prompt_tokens":0,"completion_tokens":0,"total_tokens":0},
    })).into_response()
}

// ── Chat-template helper ──────────────────────────────────────────────────────

// Format a list of OpenAI chat messages into a plain-text prompt.
//
// Ideally we would call `model.apply_chat_template()` here, but that requires
// a reference to the model which lives only in the actor thread.  We use the
// simple `<|role|>\ncontent\n` format that most modern chat models support
// (Qwen3, Llama-3, Mistral, etc.).  The actor applies the template in the
// `Generate` handler when the model is available.
//
// TODO: send raw messages to the actor and let it apply the model's built-in
// chat template via `model.apply_chat_template()`.

// ── Utilities ─────────────────────────────────────────────────────────────────

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn unix_ts_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn short_id() -> String {
    format!("{:x}", unix_ts() ^ 0xCAFE_BABE)
}

// ── Router ────────────────────────────────────────────────────────────────────

/// Build and return the LLM sub-router.
///
/// The router uses a `LlmStateCell` rather than a direct `Arc<LlmServerState>`.
/// Routes are always mounted; handlers return HTTP 503 when the cell is `None`
/// (server stopped).  Merge into the main axum router with `.merge(llm::router(cell))`.
pub fn router(cell: LlmStateCell) -> Router {
    Router::new()
        .route("/health",                       get(health))
        .route("/llm/status",                   get(server_status))
        .route("/v1/models",                    get(list_models))
        .route("/v1/chat/completions",          post(chat_completions))
        .route("/v1/completions",               post(completions))
        .route("/v1/embeddings",                post(embeddings))
        .with_state(cell)
}
