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

use std::{
    collections::VecDeque,
    num::NonZeroU32,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc::{self, UnboundedSender};

use llama_cpp_4::{
    context::params::{LlamaContextParams, LlamaPoolingType},
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{AddBos, LlamaModel, Special, params::LlamaModelParams},
    sampling::LlamaSampler,
};

use crate::config::{LlmConfig, LlmToolConfig};
use crate::event::LlmEventEmitter;
use crate::catalog::LlmCatalog;
use crate::tools;

/// Current time as milliseconds since the Unix epoch.
pub fn unix_ts_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

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
const LOG_CAP: usize = skill_constants::LLM_LOG_CAP;

/// Create a new, empty log buffer.
pub fn new_log_buffer() -> LlmLogBuffer {
    Arc::new(Mutex::new(VecDeque::with_capacity(LOG_CAP)))
}

/// Optional file sink for LLM log lines.
///
/// `Arc<Mutex<…>>` so both `push_log` (called from any thread via macros)
/// and `run_actor` (which creates it) can hold a reference.
pub type LlmLogFile = Arc<Mutex<std::io::BufWriter<std::fs::File>>>;

const LLM_LOG_DIR: &str = skill_constants::LLM_LOG_DIR;

/// Append a log entry to the in-memory buffer, emit a `llm:log` Tauri event,
/// and optionally write to the per-session log file.
fn push_log_inner(
    app: &dyn LlmEventEmitter,
    buf:      &LlmLogBuffer,
    file:     Option<&LlmLogFile>,
    level:    &str,
    msg:      &str,
) {
    llm_log!("llm", "[{level}] {msg}");
    let ts    = unix_ts_ms();
    let entry = LlmLogEntry { ts, level: level.to_string(), message: msg.to_string() };

    { let mut q = buf.lock().unwrap(); if q.len() >= LOG_CAP { q.pop_front(); } q.push_back(entry.clone()); }
    app.emit_event("llm:log", serde_json::to_value(&entry).unwrap_or_default());

    if let Some(f) = file {
        use std::io::Write;
        let dt = chrono_iso(ts);
        let _ = writeln!(f.lock().unwrap(), "[{dt}] [{level:5}] {msg}");
    }
}

/// Convenience wrapper — no file sink (used from axum handlers / cmds).
pub fn push_log(app: &dyn LlmEventEmitter, buf: &LlmLogBuffer, level: &str, msg: &str) {
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

// ── Shared state (held in axum Router via `.with_state()`) ────────────────────

pub struct LlmServerState {
    /// Channel to the inference actor.
    pub req_tx: tokio::sync::mpsc::UnboundedSender<InferRequest>,
    /// Display name shown in `/v1/models`.
    pub model_name:       String,
    /// Optional Bearer token required on every request.
    pub api_key:      Option<String>,
    /// Built-in tools currently allowed for chat requests.
    #[cfg(feature = "llm")]
    pub allowed_tools: Arc<Mutex<LlmToolConfig>>,

    /// Set to `true` by the actor once the model + context are fully loaded.
    pub ready:        Arc<AtomicBool>,
    /// Context window size in tokens; set by the actor after context creation.
    pub n_ctx:        Arc<std::sync::atomic::AtomicUsize>,
    /// Whether a vision projector (mmproj) was loaded — enables image input.
    pub vision_ready: Arc<AtomicBool>,
    /// Set of tool_call_ids that the user has cancelled from the UI.
    /// Checked before each tool execution; cancelled calls return an error
    /// result instead of running.
    pub cancelled_tool_calls: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    /// Base directory for storing tool-generated script and output files.
    /// Located at `skill_dir/chats/scripts/`. Subdirectories are created
    /// lazily per tool invocation timestamp.
    pub scripts_dir: std::path::PathBuf,
    /// Discovered Agent Skills — injected into the system prompt so the LLM
    /// can load specialised instructions via `read_file` on demand.
    pub skills: Arc<Vec<skill_skills::Skill>>,

    /// Abort signal for IPC-streamed chat (`chat_completions_ipc`).
    ///
    /// Increment the value to cancel a running IPC generation:
    /// `abort_tx.send_modify(|v| *v = v.wrapping_add(1))`.
    /// The streaming command subscribes via `abort_tx.subscribe()` and
    /// breaks out of its token loop as soon as the value changes.
    pub abort_tx:     tokio::sync::watch::Sender<u64>,
    /// OS thread handle for the actor.  Taken (set to `None`) by `shutdown()`.
    join_handle:      Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl LlmServerState {
    /// Whether the actor has finished loading the model.
    pub fn is_ready(&self) -> bool { self.ready.load(Ordering::Relaxed) }

    pub fn set_allowed_tools(&self, tools: LlmToolConfig) {
        *self.allowed_tools.lock().unwrap() = tools;
    }

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

    /// Send a chat completion request and stream the generated tokens back via
    /// the returned `UnboundedReceiver`.
    ///
    /// Returns `Err` when the model is still loading or the actor has exited.
    /// Images should be raw JPEG/PNG bytes decoded from base64 data-URLs; pass
    /// an empty `Vec` for text-only prompts.
    pub fn chat(
        &self,
        messages: Vec<Value>,
        images:   Vec<Vec<u8>>,
        params:   GenParams,
    ) -> Result<tokio::sync::mpsc::UnboundedReceiver<InferToken>, String> {
        if !self.is_ready() {
            return Err("LLM model still loading — retry in a few seconds".to_string());
        }
        let (tok_tx, tok_rx) = tokio::sync::mpsc::unbounded_channel();
        self.req_tx
            .send(InferRequest::Generate { messages, images, params, token_tx: tok_tx })
            .map_err(|_| "LLM actor has exited".to_string())?;
        Ok(tok_rx)
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
            // Snap to a char boundary — raw byte arithmetic can land inside a
            // multi-byte codepoint (e.g. CJK) and cause a panic.
            let drain = (0..=drain).rev()
                .find(|&i| self.tag_buf.is_char_boundary(i))
                .unwrap_or(0);
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

// Execute one generation pass: tokenise `prompt`, decode the prompt batch,
// run the sampling loop with a hold-back stop-string buffer, and stream
// `InferToken` messages back through `token_tx`.
//
// The hold-back buffer works like this:
//   – Every decoded piece is appended to `pending`.
//   – We emit only the prefix of `pending` that is guaranteed to NOT be the
//     start of any stop string (i.e. everything except the last
//     `max_stop_len - 1` characters).
//   – On loop exit we flush whatever is left, trimming any trailing stop string.
//
// This means stop strings that span multiple token pieces are handled
// correctly without blocking the stream for more than a few bytes.

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

/// Decode all base64-embedded images across an entire messages array.
///
/// Iterates every message's `content` field (which may be a string or an
/// OpenAI-style parts array) and collects raw JPEG/PNG bytes in document
/// order.  Plain HTTP/S image URLs are silently skipped — only
/// `data:<mime>;base64,<…>` data-URLs are supported.
///
/// Call this before passing `messages` to [`LlmServerState::chat`] so the
/// actor receives pre-decoded bytes alongside the text context.
pub fn extract_images_from_messages(messages: &[Value]) -> Vec<Vec<u8>> {
    messages.iter()
        .flat_map(|m| {
            m.get("content")
                .map(extract_images_from_content)
                .unwrap_or_default()
        })
        .collect()
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

// ── Tool definitions, execution, and context management ───────────────────────
// All tool logic lives in the `skill-tools` crate. These imports bring the
// functions needed by the orchestration code in this module.
use skill_tools::defs::{enabled_builtin_llm_tools, filter_allowed_tool_defs, is_builtin_tool_enabled};
use skill_tools::exec::execute_builtin_tool_call;
use skill_tools::context::trim_messages_to_fit;

struct ToolCallStreamSanitizer {
    raw:                 String,
    emitted_visible_len: usize,
}

impl ToolCallStreamSanitizer {
    fn new() -> Self {
        Self { raw: String::new(), emitted_visible_len: 0 }
    }

    fn push(&mut self, piece: &str) -> String {
        self.raw.push_str(piece);
        let visible = tools::strip_tool_call_blocks_preserve(&self.raw);
        if visible.len() <= self.emitted_visible_len {
            return String::new();
        }
        if !visible.is_char_boundary(self.emitted_visible_len) {
            return String::new();
        }

        let delta = visible[self.emitted_visible_len..].to_string();
        self.emitted_visible_len = visible.len();
        delta
    }
}

async fn collect_infer_output<F>(
    mut tok_rx: mpsc::UnboundedReceiver<InferToken>,
    mut on_visible_delta: F,
) -> Result<(String, String, usize, usize, usize), String>
where
    F: FnMut(&str),
{
    let mut text              = String::new();
    let mut finish_reason     = "stop".to_string();
    let mut prompt_tokens     = 0usize;
    let mut completion_tokens = 0usize;
    let mut n_ctx             = 0usize;
    let mut sanitizer         = ToolCallStreamSanitizer::new();

    while let Some(tok) = tok_rx.recv().await {
        match tok {
            InferToken::Delta(t) => {
                text.push_str(&t);
                let visible = sanitizer.push(&t);
                if !visible.is_empty() {
                    on_visible_delta(&visible);
                }
            }
            InferToken::Done { finish_reason: fr, prompt_tokens: pt, completion_tokens: ct, n_ctx: nc } => {
                finish_reason = fr;
                prompt_tokens = pt;
                completion_tokens = ct;
                n_ctx = nc;
                break;
            }
            InferToken::Error(e) => return Err(e),
        }
    }

    Ok((text, finish_reason, prompt_tokens, completion_tokens, n_ctx))
}

/// Callback signatures for tool-call lifecycle hooks (pi-mono style).
///
/// `BeforeToolCallFn`: called after argument validation but before execution.
/// Return `Some(reason)` to block execution (the reason becomes the error
/// message in the tool result).  Return `None` to allow execution.
///
/// `AfterToolCallFn`: called after execution with the raw result.
/// Return `Some(replacement)` to override the result; `None` to keep it.
#[allow(dead_code)]
pub type BeforeToolCallFn = Box<dyn Fn(&tools::ToolCall, &Value) -> Option<String> + Send + Sync>;
#[allow(dead_code)]
pub type AfterToolCallFn  = Box<dyn Fn(&tools::ToolCall, &Value, bool) -> Option<(Value, bool)> + Send + Sync>;

/// Extended tool-call event sink (pi-mono style lifecycle events).
pub enum ToolEvent {
    /// Legacy: simple status string (kept for backwards compat).
    Status { tool_name: String, status: String, detail: Option<String> },
    /// Tool execution is about to begin (after validation).
    ExecutionStart { tool_call_id: String, tool_name: String, args: Value },
    /// Tool execution finished.
    ExecutionEnd { tool_call_id: String, tool_name: String, result: Value, is_error: bool },
}

pub async fn run_chat_with_builtin_tools<F, G>(
    state: &LlmServerState,
    base_messages: Vec<Value>,
    params: GenParams,
    mut tools_from_req: Vec<tools::Tool>,
    mut on_visible_delta: F,
    mut on_tool_event: G,
) -> Result<(String, String, usize, usize, usize), String>
where
    F: FnMut(&str),
    G: FnMut(ToolEvent),
{
    let cancelled_set = state.cancelled_tool_calls.clone();
    // Clear cancelled set at the start of a new chat request.
    { cancelled_set.lock().unwrap().clear(); }
    let allowed_tools = state.allowed_tools.lock().unwrap().clone();

    let max_rounds         = allowed_tools.max_rounds;
    let max_calls_per_round = allowed_tools.max_calls_per_round;
    let execution_mode     = allowed_tools.execution_mode.clone();

    let mut messages = base_messages;
    if tools_from_req.is_empty() {
        tools_from_req = enabled_builtin_llm_tools(&allowed_tools);
    } else {
        tools_from_req = filter_allowed_tool_defs(tools_from_req, &allowed_tools);
    }
    let n_ctx = state.n_ctx.load(Ordering::Relaxed);
    tools::inject_tools_into_system_prompt(&mut messages, &tools_from_req, n_ctx);

    // Inject discovered Agent Skills into the system prompt so the LLM knows
    // which specialised instruction files it can load via read_file.
    let skills_block = skill_skills::format_skills_for_prompt(&state.skills);
    if !skills_block.is_empty() {
        // Append to the first system message (which inject_tools just created/extended).
        let has_system = messages.first()
            .and_then(|m| m.get("role"))
            .and_then(|r| r.as_str()) == Some("system");
        if has_system {
            if let Some(content) = messages[0].get("content").and_then(|c| c.as_str()).map(|s| s.to_string()) {
                messages[0]["content"] = serde_json::Value::String(format!("{content}{skills_block}"));
            }
        } else {
            messages.insert(0, json!({ "role": "system", "content": skills_block }));
        }
    }

    // Build a lookup map for argument validation.
    let tool_defs: std::collections::HashMap<String, tools::Tool> = tools_from_req
        .iter()
        .map(|t| (t.function.name.clone(), t.clone()))
        .collect();

    // Cross-round dedup: track (tool_name, arguments) pairs already executed
    // so the model can't re-call the exact same tool with the same args.
    let mut executed_calls = std::collections::HashSet::<(String, String)>::new();

    for _ in 0..=max_rounds {
        // ── Context-aware history trimming ──────────────────────────────
        // Estimate token count (~4 chars/token) and drop the oldest
        // non-system messages until we fit within ~75% of n_ctx, leaving
        // room for the model's response.
        trim_messages_to_fit(&mut messages, n_ctx);

        let images = extract_images_from_messages(&messages);
        let (tok_tx, tok_rx) = mpsc::unbounded_channel();
        state.req_tx
            .send(InferRequest::Generate {
                messages: messages.clone(),
                images,
                params: params.clone(),
                token_tx: tok_tx,
            })
            .map_err(|_| "LLM actor has exited".to_string())?;

        let (assistant_text, finish_reason, prompt_tokens, completion_tokens, n_ctx) = collect_infer_output(tok_rx, |delta| {
            on_visible_delta(delta);
        }).await?;
        let tool_calls = tools::extract_tool_calls(&assistant_text);
        if tool_calls.is_empty() {
            let cleaned = tools::strip_tool_call_blocks(&assistant_text);
            return Ok((cleaned, finish_reason, prompt_tokens, completion_tokens, n_ctx));
        }

        let cleaned = tools::strip_tool_call_blocks(&assistant_text);

        // Filter out:
        // 1. Bash calls with no command (will just error with "missing command")
        // 2. Calls already executed in a prior round with identical args (cross-round dedup)
        let selected_calls: Vec<tools::ToolCall> = tool_calls
            .into_iter()
            .filter(|tc| {
                // Skip bash calls with empty/missing command
                if tc.function.name == "bash" {
                    let args: Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(Value::Object(Default::default()));
                    if args.get("command").and_then(|c| c.as_str()).unwrap_or("").is_empty() {
                        return false;
                    }
                }
                // Skip exact duplicates from prior rounds
                let key = (tc.function.name.clone(), tc.function.arguments.clone());
                if executed_calls.contains(&key) {
                    return false;
                }
                true
            })
            .take(max_calls_per_round)
            .collect();

        // If all calls were filtered out, treat as no tool calls — return the text.
        if selected_calls.is_empty() {
            return Ok((cleaned, finish_reason, prompt_tokens, completion_tokens, n_ctx));
        }

        // Always push an assistant message to maintain user/assistant alternation.
        // If the model only emitted tool calls (no prose), use a short placeholder.
        // This prevents consecutive user messages (original query + tool result)
        // which break most local model chat templates.
        let assistant_content = if cleaned.trim().is_empty() {
            "[Calling tools…]".to_string()
        } else {
            cleaned
        };
        messages.push(json!({
            "role": "assistant",
            "content": assistant_content,
        }));

        // Record these calls for cross-round dedup
        for tc in &selected_calls {
            executed_calls.insert((tc.function.name.clone(), tc.function.arguments.clone()));
        }

        match execution_mode {
            crate::config::ToolExecutionMode::Sequential => {
                execute_tool_calls_sequential(
                    &selected_calls, &tool_defs, &allowed_tools,
                    &mut messages, &mut on_tool_event,
                    &cancelled_set, &state.scripts_dir,
                ).await;
            }
            crate::config::ToolExecutionMode::Parallel => {
                execute_tool_calls_parallel(
                    &selected_calls, &tool_defs, &allowed_tools,
                    &mut messages, &mut on_tool_event,
                    &cancelled_set, &state.scripts_dir,
                ).await;
            }
        }
    }

    Err(format!("tool-calling round limit reached ({max_rounds} rounds). You can increase this in Settings → LLM → Tools → Max rounds."))
}

/// Validate arguments for a tool call.  Returns the parsed args `Value` or an
/// error result to inject directly.
fn validate_and_prepare(
    tc: &tools::ToolCall,
    tool_defs: &std::collections::HashMap<String, tools::Tool>,
    allowed_tools: &crate::config::LlmToolConfig,
) -> Result<Value, Value> {
    // Check if tool is enabled.
    if !is_builtin_tool_enabled(allowed_tools, &tc.function.name) {
        return Err(json!({ "ok": false, "tool": tc.function.name, "error": "tool disabled in settings" }));
    }

    // Parse raw arguments string.
    let args: Value = serde_json::from_str(&tc.function.arguments)
        .unwrap_or_else(|_| json!({}));

    // Validate against JSON Schema if a definition exists.
    if let Some(tool_def) = tool_defs.get(&tc.function.name) {
        match tools::validate_tool_arguments(tool_def, &args) {
            Ok(validated) => Ok(validated),
            Err(err_msg) => Err(json!({ "ok": false, "tool": tc.function.name, "error": err_msg })),
        }
    } else {
        // Unknown tool — let execution handle the error.
        Ok(args)
    }
}

/// Execute tool calls one-by-one in order (pi-mono "sequential" mode).
async fn execute_tool_calls_sequential<G>(
    calls: &[tools::ToolCall],
    tool_defs: &std::collections::HashMap<String, tools::Tool>,
    allowed_tools: &crate::config::LlmToolConfig,
    messages: &mut Vec<Value>,
    on_tool_event: &mut G,
    cancelled_set: &Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    scripts_dir: &std::path::Path,
)
where
    G: FnMut(ToolEvent),
{
    for tc in calls {
        // Check if this tool call was cancelled by the user before execution.
        if cancelled_set.lock().unwrap().contains(&tc.id) {
            let cancel_result = json!({ "ok": false, "tool": tc.function.name, "error": "cancelled by user" });
            on_tool_event(ToolEvent::Status {
                tool_name: tc.function.name.clone(),
                status: "cancelled".into(),
                detail: Some("cancelled by user".into()),
            });
            on_tool_event(ToolEvent::ExecutionEnd {
                tool_call_id: tc.id.clone(),
                tool_name: tc.function.name.clone(),
                result: cancel_result.clone(),
                is_error: true,
            });
            messages.push(json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": cancel_result.to_string(),
            }));
            continue;
        }

        let args_result = validate_and_prepare(tc, tool_defs, allowed_tools);

        let args_for_event = match &args_result {
            Ok(v) => v.clone(),
            Err(_) => serde_json::from_str(&tc.function.arguments).unwrap_or(json!({})),
        };

        // Emit start events (legacy + rich).
        let detail_str = if tc.function.arguments.len() > 2 {
            Some(tc.function.arguments.clone())
        } else { None };
        on_tool_event(ToolEvent::Status {
            tool_name: tc.function.name.clone(),
            status: "calling".into(),
            detail: detail_str,
        });
        on_tool_event(ToolEvent::ExecutionStart {
            tool_call_id: tc.id.clone(),
            tool_name: tc.function.name.clone(),
            args: args_for_event,
        });

        // Re-check cancellation after emitting start (user may cancel while waiting).
        let (tool_result, is_error) = if cancelled_set.lock().unwrap().contains(&tc.id) {
            (json!({ "ok": false, "tool": tc.function.name, "error": "cancelled by user" }), true)
        } else {
            match args_result {
                Err(err_val) => (err_val, true),
                Ok(_) => {
                    let result = execute_builtin_tool_call(tc, allowed_tools, scripts_dir).await;
                    let ok = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                    (result, !ok)
                }
            }
        };

        // Emit end events (legacy + rich).
        on_tool_event(ToolEvent::Status {
            tool_name: tc.function.name.clone(),
            status: if is_error { "error" } else { "done" }.into(),
            detail: if is_error { tool_result.get("error").and_then(|v| v.as_str()).map(|s| s.to_string()) } else { None },
        });
        on_tool_event(ToolEvent::ExecutionEnd {
            tool_call_id: tc.id.clone(),
            tool_name: tc.function.name.clone(),
            result: tool_result.clone(),
            is_error,
        });

        messages.push(json!({
            "role": "tool",
            "tool_call_id": tc.id,
            "content": tool_result.to_string(),
        }));
    }
}

/// Execute tool calls concurrently (pi-mono "parallel" mode).
///
/// Preparation (validation) is done sequentially, then all valid calls are
/// spawned concurrently and results collected in source order.
async fn execute_tool_calls_parallel<G>(
    calls: &[tools::ToolCall],
    tool_defs: &std::collections::HashMap<String, tools::Tool>,
    allowed_tools: &crate::config::LlmToolConfig,
    messages: &mut Vec<Value>,
    on_tool_event: &mut G,
    cancelled_set: &Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    scripts_dir: &std::path::Path,
)
where
    G: FnMut(ToolEvent),
{
    // Phase 1: Prepare all calls (validate arguments, emit start events).
    struct PreparedCall {
        tc: tools::ToolCall,
        validation: Result<Value, Value>,
    }

    let mut prepared = Vec::with_capacity(calls.len());
    for tc in calls {
        // Check if already cancelled before validation.
        if cancelled_set.lock().unwrap().contains(&tc.id) {
            let cancel_result = json!({ "ok": false, "tool": tc.function.name, "error": "cancelled by user" });
            on_tool_event(ToolEvent::Status {
                tool_name: tc.function.name.clone(),
                status: "cancelled".into(),
                detail: Some("cancelled by user".into()),
            });
            on_tool_event(ToolEvent::ExecutionEnd {
                tool_call_id: tc.id.clone(),
                tool_name: tc.function.name.clone(),
                result: cancel_result.clone(),
                is_error: true,
            });
            messages.push(json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": cancel_result.to_string(),
            }));
            continue;
        }

        let args_result = validate_and_prepare(tc, tool_defs, allowed_tools);

        let args_for_event = match &args_result {
            Ok(v) => v.clone(),
            Err(_) => serde_json::from_str(&tc.function.arguments).unwrap_or(json!({})),
        };

        let detail_str = if tc.function.arguments.len() > 2 {
            Some(tc.function.arguments.clone())
        } else { None };
        on_tool_event(ToolEvent::Status {
            tool_name: tc.function.name.clone(),
            status: "calling".into(),
            detail: detail_str,
        });
        on_tool_event(ToolEvent::ExecutionStart {
            tool_call_id: tc.id.clone(),
            tool_name: tc.function.name.clone(),
            args: args_for_event,
        });

        prepared.push(PreparedCall { tc: tc.clone(), validation: args_result });
    }

    // Phase 2: Execute valid calls concurrently, immediately resolve errors.
    // Cancelled calls are short-circuited.
    let mut futures = Vec::with_capacity(prepared.len());
    for p in &prepared {
        let tc = p.tc.clone();
        let allowed = allowed_tools.clone();
        let is_valid = p.validation.is_ok();
        let cancel_check = cancelled_set.clone();
        let sdir = scripts_dir.to_path_buf();

        if is_valid {
            futures.push(tokio::spawn(async move {
                // Check cancellation before executing.
                if cancel_check.lock().unwrap().contains(&tc.id) {
                    return (tc.clone(), json!({ "ok": false, "tool": tc.function.name, "error": "cancelled by user" }), true);
                }
                let result = execute_builtin_tool_call(&tc, &allowed, &sdir).await;
                let ok = result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                (tc, result, !ok)
            }));
        } else {
            let err_val = p.validation.as_ref().err().unwrap().clone();
            futures.push(tokio::spawn(async move {
                (tc, err_val, true)
            }));
        }
    }

    // Phase 3: Collect results in source order and emit end events.
    for future in futures {
        let (tc, tool_result, is_error) = future.await.unwrap_or_else(|e| {
            let tc = calls[0].clone(); // fallback
            (tc, json!({"ok": false, "error": e.to_string()}), true)
        });

        on_tool_event(ToolEvent::Status {
            tool_name: tc.function.name.clone(),
            status: if is_error { "error" } else { "done" }.into(),
            detail: if is_error { tool_result.get("error").and_then(|v| v.as_str()).map(|s| s.to_string()) } else { None },
        });
        on_tool_event(ToolEvent::ExecutionEnd {
            tool_call_id: tc.id.clone(),
            tool_name: tc.function.name.clone(),
            result: tool_result.clone(),
            is_error,
        });

        messages.push(json!({
            "role": "tool",
            "tool_call_id": tc.id,
            "content": tool_result.to_string(),
        }));
    }
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
    app: &dyn LlmEventEmitter,
    log_buf:  &LlmLogBuffer,
    log_file: Option<&LlmLogFile>,
    params:   &GenParams,
    token_tx: UnboundedSender<InferToken>,
    n_prompt: usize,
) {
    let n_ctx = ctx.n_ctx() as usize;
    let n_batch = ctx.n_batch() as usize; let _ = n_batch; // available for future use

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
    // After a forced </think> injection, discard tokens (still decoded into KV
    // cache for coherence) until the model reaches a clean line break.
    // This prevents the orphaned tail of the interrupted thinking sentence
    // ("s?), hacking.") from leaking into the visible response.
    let mut discard_until_nl = false;

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

        // After forced </think> injection: decode token into KV cache for
        // coherence, but suppress it from the output stream until the model
        // reaches a clean line boundary (end of the orphaned thinking tail).
        if discard_until_nl {
            if piece.contains('\n') {
                discard_until_nl = false;
                // The newline itself is not emitted — next token starts fresh.
            }
            let mut b = LlamaBatch::new(1, 1);
            b.add(token, n_cur as i32, &[0], true).ok();
            if ctx.decode(&mut b).is_err() {
                token_tx.send(InferToken::Error("decode error".into())).ok();
                break;
            }
            n_cur += 1;
            continue;
        }

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
            // Decode the triggering token too, but discard its text — the
            // model was mid-sentence when we cut it off.
            discard_until_nl = true;
            let mut b = LlamaBatch::new(1, 1);
            b.add(token, n_cur as i32, &[0], true).ok();
            if ctx.decode(&mut b).is_err() {
                token_tx.send(InferToken::Error("decode error".into())).ok();
                break;
            }
            n_cur += 1;
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
    llm_info!(app, log_buf, log_file,
        "generation done — prompt={n_prompt} completion={n_gen} ctx={n_ctx} finish={finish_reason}");
    token_tx.send(InferToken::Done {
        finish_reason,
        prompt_tokens:     n_prompt,
        completion_tokens: n_gen,
        n_ctx,
    }).ok();
}

// ── Text-only generation ───────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn run_generation(
    model:    &llama_cpp_4::model::LlamaModel,
    ctx:      &mut llama_cpp_4::context::LlamaContext<'_>,
    app: &dyn LlmEventEmitter,
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
    app: &dyn LlmEventEmitter,
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
    mmproj_path:   Option<std::path::PathBuf>,
    app: Arc<dyn LlmEventEmitter>,
    log_buf:       LlmLogBuffer,
    log_path:      Option<std::path::PathBuf>,
    ready_flag:    Arc<AtomicBool>,
    n_ctx_flag:    Arc<std::sync::atomic::AtomicUsize>,
    vision_flag:   Arc<AtomicBool>,
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
    
    // ── Windows-specific Vulkan SDK setup ─────────────────────────────────────
    #[cfg(target_os = "windows")]
    {
        // On Windows, the Vulkan loader DLL (vulkan-1.dll) must be found in PATH.
        // The Vulkan SDK installer sets `VULKAN_SDK` and adds its bin directory to PATH.
        // Some user configurations may need explicit path injection for robustness.
        if let Ok(vulkan_sdk_path) = std::env::var("VULKAN_SDK") {
            // VULKAN_SDK is typically e.g. "C:\VulkanSDK\1.3.290.0".
            // The Vulkan loader DLL (vulkan-1.dll) lives in the Bin
            // subdirectory, so we need to add "{VULKAN_SDK}\Bin" to PATH.
            let vulkan_bin = std::path::Path::new(&vulkan_sdk_path).join("Bin");
            let vulkan_bin_str = vulkan_bin.to_string_lossy().to_string();

            if let Ok(current_path) = std::env::var("PATH") {
                std::env::set_var(
                    "PATH",
                    format!("{};{}", vulkan_bin_str, current_path),
                );
                llm_info!(&app, &log_buf, log_file,
                    "Vulkan SDK Bin directory injected into PATH: {}", vulkan_bin_str);
            } else {
                std::env::set_var("PATH", &vulkan_bin_str);
            }
        }
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        // Non-Windows: no special Vulkan path handling needed
    }

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
    if !config.verbose {
        backend_md.void_logs(); // silence llama.cpp / ggml verbose stderr
    }
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
    llm_info!(&app, &log_buf, log_file,
        "creating context (n_ctx={}, n_gpu_layers={}, flash_attn={}, offload_kqv={})",
        ctx_size.map_or(0, |n| n.get()), config.n_gpu_layers,
        config.flash_attention, config.offload_kqv);
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(ctx_size)
        .with_n_threads(-1)
        .with_n_threads_batch(-1)
        .with_flash_attention(config.flash_attention)
        .with_offload_kqv(config.offload_kqv);

    let mut ctx = match model.new_context(backend, ctx_params) {
        Ok(c)  => c,
        Err(e) => { llm_error!(&app, &log_buf, log_file, "failed to create context: {e}"); return; }
    };

    n_ctx_flag.store(ctx.n_ctx() as usize, Ordering::Relaxed);
    llm_info!(&app, &log_buf, log_file, "context ready — n_ctx={} — running warmup pass…", ctx.n_ctx());
    
    // ── Windows Vulkan diagnostic check ────────────────────────────────────────
    #[cfg(target_os = "windows")]
    {
        // Detect if GPU layers were actually loaded or if we fell back to CPU
        // by checking for common signs of Vulkan initialization failure.
        // The backend init succeeded but device selection may have failed silently.
        let n_layers = config.n_gpu_layers;
        if n_layers > 0 {
            llm_info!(&app, &log_buf, log_file,
                "GPU offload requested: {} layer(s)", n_layers);
            llm_warn!(&app, &log_buf, log_file,
                "on Windows, ensure Vulkan SDK is installed and VULKAN_SDK env var is set");
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Non-Windows systems — Metal (macOS) and CUDA handle device detection differently
    }
    
    app.emit_event("llm:status", json!({"status":"loading","detail":"warming_up"}));

    // ── Multimodal projector (llm-mtmd feature) ───────────────────────────────
    // mtmd_log_set is part of libmtmd (linked via llama-cpp-4) but not yet
    // exposed in the llama-cpp-4 Rust wrapper — declare it directly.
    #[cfg(feature = "llm-mtmd")]
    extern "C" {
        fn mtmd_log_set(
            log_callback: Option<unsafe extern "C" fn(
                level:     u32,
                text:      *const std::os::raw::c_char,
                user_data: *mut   std::os::raw::c_void,
            )>,
            user_data: *mut std::os::raw::c_void,
        );
    }

    #[cfg(feature = "llm-mtmd")]
    let mtmd_ctx: Option<llama_cpp_4::mtmd::MtmdContext> = {
        if mmproj_path.is_none() {
            llm_info!(&app, &log_buf, log_file,
                "vision disabled — no mmproj file configured; \
                 download a vision projector in Settings → LLM to enable image input");
        }
        mmproj_path.as_ref().and_then(|p| {
            use llama_cpp_4::mtmd::{MtmdContext, MtmdContextParams};

            // Guard: verify the file still exists on disk before handing it to
            // the C library.  mtmd_init_from_file can abort/segfault on some
            // platforms when the file is missing rather than returning null.
            if !p.exists() {
                llm_error!(&app, &log_buf, log_file,
                    "mmproj file missing: {} — vision disabled", p.display());
                return None;
            }

            // Silence clip_model_loader tensor spam before loading the projector.
            // clip.cpp maintains its own logger (separate from llama_log_set),
            // so we must call mtmd_log_set explicitly.
            if !config.verbose {
                unsafe extern "C" fn noop(
                    _level: u32,
                    _text:  *const std::os::raw::c_char,
                    _ud:    *mut   std::os::raw::c_void,
                ) {}
                unsafe { mtmd_log_set(Some(noop), std::ptr::null_mut()) };
            }
            // Validate file size — reject empty / obviously truncated files
            // before handing them to the C library, which may abort internally.
            let file_size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            if file_size < 1024 {
                llm_error!(&app, &log_buf, log_file,
                    "mmproj file too small ({file_size} bytes): {} — \
                     likely a failed download; re-download in Settings → LLM",
                    p.display());
                return None;
            }

            // Linux Vulkan + mtmd init can hard-abort for some projector
            // / driver combinations.  We attempt GPU first (for speed) and
            // silently retry on CPU if the GPU path panics or errors.
            //
            // Users can force CPU-only with `no_mmproj_gpu: true` in settings,
            // or force GPU-only with `SKILL_FORCE_MMPROJ_GPU=1`.
            let force_mmproj_gpu = std::env::var("SKILL_FORCE_MMPROJ_GPU")
                .ok()
                .as_deref()
                .map(|v| matches!(v, "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(false);

            let mmproj_use_gpu = !config.no_mmproj_gpu || force_mmproj_gpu;

            llm_info!(&app, &log_buf, log_file,
                "loading mmproj: {} ({:.1} MB, gpu={}, threads={})",
                p.display(), file_size as f64 / 1_048_576.0,
                mmproj_use_gpu, config.mmproj_n_threads);

            // Helper: attempt to load mmproj with a given GPU flag.
            let try_load_mmproj = |use_gpu: bool| -> Result<MtmdContext, String> {
                let params = MtmdContextParams::default()
                    .use_gpu(use_gpu)
                    .n_threads(config.mmproj_n_threads)
                    .print_timings(config.verbose)
                    .warmup(false);
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    MtmdContext::init_from_file(p, &model, params)
                })) {
                    Ok(Ok(mc))     => Ok(mc),
                    Ok(Err(e))     => Err(format!("{e}")),
                    Err(_panic)    => Err("panic in native code".into()),
                }
            };

            // First attempt with the requested GPU mode.
            let result = try_load_mmproj(mmproj_use_gpu);

            // On Linux: if GPU failed, automatically retry on CPU (unless
            // the user explicitly forced GPU-only).
            let result = match result {
                Ok(mc) => Ok(mc),
                Err(ref gpu_err) if mmproj_use_gpu && cfg!(target_os = "linux") && !force_mmproj_gpu => {
                    llm_warn!(&app, &log_buf, log_file,
                        "mmproj GPU load failed ({gpu_err}); retrying on CPU…");
                    try_load_mmproj(false)
                }
                Err(e) => Err(e),
            };

            match result {
                Ok(mc) => {
                    llm_info!(&app, &log_buf, log_file,
                        "mmproj loaded ✓ — vision={} audio={}",
                        mc.supports_vision(), mc.supports_audio());
                    vision_flag.store(true, Ordering::Relaxed);
                    Some(mc)
                }
                Err(e) => {
                    llm_error!(&app, &log_buf, log_file,
                        "failed to load mmproj: {e} — file: {}", p.display());
                    llm_info!(&app, &log_buf, log_file,
                        "vision disabled — to enable image input, \
                         ensure the mmproj file matches your model or re-download it in Settings → LLM");
                    None
                }
            }
        })
    };
    #[cfg(not(feature = "llm-mtmd"))]
    let _ = &mmproj_path;

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
    let model_file    = model_path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let vision_loaded = vision_flag.load(Ordering::Relaxed);
    llm_info!(&app, &log_buf, log_file, "server ready — model={} supports_vision={}", model_file, vision_loaded);
    app.emit_event("llm:status", json!({"status":"running","model":model_file,"supports_vision":vision_loaded,"supports_tools":true}));

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
                        let mut role = m.get("role")?.as_str()?.to_string();
                        let raw_content = extract_fn(m.get("content")?, marker);

                        // Map "tool" role to "user" with a wrapper — most local
                        // model chat templates only support system/user/assistant.
                        let content = if role == "tool" {
                            role = "user".to_string();
                            format!("[Tool Result]\n{}", raw_content)
                        } else {
                            raw_content
                        };

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

            InferRequest::EmbedImage { bytes, result_tx } => {
                // Embed a single image via the mmproj vision projector.
                // Used by the screenshot worker for visual-similarity embeddings.
                #[cfg(feature = "llm-mtmd")]
                {
                    if let Some(ref mtmd) = mtmd_ctx {
                        use llama_cpp_4::mtmd::{MtmdBitmap, MtmdContext, MtmdInputChunks, MtmdInputText, MtmdInputChunkType};

                        let embedding = (|| -> Option<Vec<f32>> {
                            let bitmap = MtmdBitmap::from_buf(mtmd, &bytes).ok()?;
                            let bitmap_refs = [&bitmap];

                            let text = MtmdInputText::new(
                                MtmdContext::default_marker(),
                                false, false,
                            );
                            let mut chunks = MtmdInputChunks::new();
                            mtmd.tokenize(&text, &bitmap_refs, &mut chunks).ok()?;

                            // Find the image chunk and encode it
                            for chunk in chunks.iter() {
                                if chunk.chunk_type() == MtmdInputChunkType::Image {
                                    mtmd.encode_chunk(&chunk).ok()?;
                                    let n_tokens = chunk.n_tokens();
                                    let n_embd = model.n_embd() as usize;
                                    let n_elements = n_tokens * n_embd;
                                    let embd = mtmd.output_embd(n_elements);
                                    // Mean-pool across tokens to get a single vector
                                    let mut pooled = vec![0.0f32; n_embd];
                                    for t in 0..n_tokens {
                                        for d in 0..n_embd {
                                            pooled[d] += embd[t * n_embd + d];
                                        }
                                    }
                                    if n_tokens > 0 {
                                        for d in 0..n_embd {
                                            pooled[d] /= n_tokens as f32;
                                        }
                                    }
                                    // L2-normalize
                                    let norm: f32 = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
                                    if norm > 0.0 {
                                        for v in &mut pooled {
                                            *v /= norm;
                                        }
                                    }
                                    return Some(pooled);
                                }
                            }
                            None
                        })();

                        result_tx.send(embedding).ok();
                    } else {
                        llm_warn!(&app, &log_buf, log_file,
                            "EmbedImage: no mmproj loaded — returning None");
                        result_tx.send(None).ok();
                    }
                }
                #[cfg(not(feature = "llm-mtmd"))]
                {
                    result_tx.send(None).ok();
                }
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
    app.emit_event("llm:status", json!({"status":"stopped"}));
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
    app: Arc<dyn LlmEventEmitter>,
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

    // Resolve the mmproj path: explicit selection → auto-detect from catalog →
    // legacy config.mmproj field.
    //
    // Safety guards:
    // 1) Skip stale paths that no longer exist on disk.
    // 2) If the active model is from the bundled catalog, reject mmproj files
    //    that belong to a different repo. This prevents stale config.mmproj
    //    values (for a previously-used model family) from being passed to mtmd
    //    for an incompatible active model.
    let active_model_repo = catalog.active_model_entry().map(|e| e.repo.as_str());
    let mmproj_path = catalog
        .resolve_mmproj_path(config.autoload_mmproj)
        .or_else(|| config.mmproj.clone())
        .filter(|p| {
            let Some(model_repo) = active_model_repo else { return true; };

            let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or_default();
            let mmproj_repo = catalog.entries.iter()
                .find(|e| {
                    e.is_mmproj
                        && (e.local_path.as_ref().is_some_and(|lp| lp == p)
                            || e.filename == file_name)
                })
                .map(|e| e.repo.as_str());

            if let Some(mm_repo) = mmproj_repo {
                if mm_repo != model_repo {
                    push_log(&app, &log_buf, "warn",
                        &format!(
                            "mmproj/model repo mismatch — skipping vision projector: {} \
                             (mmproj repo: {}, model repo: {})",
                            p.display(), mm_repo, model_repo,
                        ));
                    return false;
                }
            }
            true
        })
        .filter(|p| {
            if p.exists() { return true; }
            push_log(&app, &log_buf, "warn",
                &format!("mmproj file not found (deleted?): {} — skipping vision", p.display()));
            false
        });
    let model_name  = model_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("llama.cpp-model")
        .to_owned();

    push_log(&app, &log_buf, "info", &format!("starting LLM server — model: {model_name}"));

    // ── Per-session log file ──────────────────────────────────────────────────
    // Written to skill_dir/llm_logs/llm_<unix-seconds>.txt so each server run
    // has its own timestamped transcript in a dedicated LLM-only folder.
    let ts_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let log_dir = skill_dir.join(LLM_LOG_DIR);
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join(format!("llm_{ts_secs}.txt"));
    push_log(&app, &log_buf, "info", &format!("session log → {}", log_path.display()));

    let (req_tx, req_rx) = mpsc::unbounded_channel::<InferRequest>();
    let ready_flag  = Arc::new(AtomicBool::new(false));
    let n_ctx_flag  = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let vision_flag = Arc::new(AtomicBool::new(false));
    let allowed_tools = Arc::new(Mutex::new(config.tools.clone()));
    let (abort_tx, _) = tokio::sync::watch::channel(0u64);

    let config2     = config.clone();
    let path2       = model_path.clone();
    let mmproj2     = mmproj_path.clone();
    let app2        = app.clone();
    let buf2        = log_buf.clone();
    let ready2      = ready_flag.clone();
    let n_ctx2      = n_ctx_flag.clone();
    let vision2     = vision_flag.clone();

    let join_handle = std::thread::Builder::new()
        .name("llm-actor".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || run_actor(req_rx, config2, path2, mmproj2, app2, buf2,
                                 Some(log_path), ready2, n_ctx2, vision2))
        .expect("failed to spawn llm-actor thread");

    app.emit_event("llm:status", json!({"status":"loading","model":model_name}));

    // Base scripts directory — subdirectories created lazily per tool invocation.
    let scripts_dir = skill_dir.join("chats").join("scripts");
    let _ = std::fs::create_dir_all(&scripts_dir);

    // Discover Agent Skills from all configured locations.
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    // In development, `skills/` lives at the project root (git submodule).
    // In production, it's next to the executable.
    let bundled_skills_dir = exe_dir.as_ref()
        .map(|d| d.join(skill_constants::SKILLS_SUBDIR))
        .filter(|d| d.is_dir())
        .or_else(|| {
            // Fallback: check current working directory for a skills/ subdir
            // (common in development when running from the project root).
            let cwd = std::env::current_dir().ok()?;
            let p = cwd.join(skill_constants::SKILLS_SUBDIR);
            if p.is_dir() { Some(p) } else { None }
        });
    let skills_result = skill_skills::load_skills(skill_skills::LoadSkillsOptions {
        cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        skill_dir: skill_dir.to_path_buf(),
        bundled_dir: bundled_skills_dir,
        skill_paths: Vec::new(),
        include_defaults: true,
    });
    let n_skills = skills_result.skills.len();
    for diag in &skills_result.diagnostics {
        push_log(&app, &log_buf, &diag.level, &format!("[skills] {}: {}", diag.path, diag.message));
    }
    if n_skills > 0 {
        let names: Vec<&str> = skills_result.skills.iter().map(|s| s.name.as_str()).collect();
        push_log(&app, &log_buf, "info", &format!("discovered {n_skills} skill(s): {}", names.join(", ")));
    }

    Some(Arc::new(LlmServerState {
        req_tx,
        model_name,
        api_key:      config.api_key.clone(),
        allowed_tools,
        cancelled_tool_calls: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        scripts_dir,
        skills:       Arc::new(skills_result.skills),
        ready:        ready_flag,
        n_ctx:        n_ctx_flag,
        vision_ready: vision_flag,
        abort_tx,
        join_handle:  Mutex::new(Some(join_handle)),
    }))
}

