// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! HTTP/REST handlers for the LLM inference server.
//!
//! Mounted by [`super::engine::router`] under `/v1/*` paths.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::State,
    http::StatusCode,
    response::{sse, IntoResponse, Response},
    Json, Router,
    routing::{get, post},
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::engine::{
    ChatRequest, CompletionRequest, EmbeddingsRequest,
    InferRequest, InferToken, ToolEvent,
    LlmServerState, LlmStateCell,
    cell_status, run_chat_with_builtin_tools,
};

// ── Auth + cell-extraction helpers ────────────────────────────────────────────

fn check_auth(state: &LlmServerState, headers: &axum::http::HeaderMap) -> bool {
    let Some(ref key) = state.api_key else { return true; };
    headers.get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|token| token == key.as_str())
        .unwrap_or(false)
}

macro_rules! get_state {
    ($cell:expr) => {{
        match $cell.lock().expect("lock poisoned").clone() {
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
    match &*cell.lock().expect("lock poisoned") {
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
    let _ = &req.tool_choice;

    if !state.is_ready() {
        return (StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error":{"message":"Model is still loading","code":"loading"}}))).into_response();
    }

    match run_chat_with_builtin_tools(&state, req.messages.clone(), req.gen.clone(), req.tools.clone(), |_| {}, |_: ToolEvent| {}).await {
        Ok((text, finish_reason, prompt_tokens, completion_tokens, n_ctx)) => {
            if req.stream {
                let model_name = state.model_name.clone();
                let id = format!("chatcmpl-{}", short_id());
                let ts = unix_ts();
                let stream = async_stream::stream! {
                    if !text.is_empty() {
                        let data = serde_json::to_string(&json!({
                            "id": id,
                            "object": "chat.completion.chunk",
                            "created": ts,
                            "model": model_name,
                            "choices": [{"index":0,"delta":{"content":text},"finish_reason":null}],
                        })).unwrap_or_default();
                        yield Ok::<sse::Event, String>(sse::Event::default().data(data));
                    }

                    let done = serde_json::to_string(&json!({
                        "id": id,
                        "object": "chat.completion.chunk",
                        "created": ts,
                        "model": model_name,
                        "choices": [{"index":0,"delta":{},"finish_reason":finish_reason}],
                        "usage": {
                            "prompt_tokens": prompt_tokens,
                            "completion_tokens": completion_tokens,
                            "total_tokens": prompt_tokens + completion_tokens,
                            "n_ctx": n_ctx,
                        }
                    })).unwrap_or_default();
                    yield Ok(sse::Event::default().data(done));
                    yield Ok(sse::Event::default().data("[DONE]"));
                };

                sse::Sse::new(stream)
                    .keep_alive(sse::KeepAlive::default())
                    .into_response()
            } else {
                let id = format!("chatcmpl-{}", short_id());
                let ts = unix_ts();
                Json(json!({
                    "id": id,
                    "object": "chat.completion",
                    "created": ts,
                    "model": state.model_name,
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": text},
                        "finish_reason": finish_reason,
                    }],
                    "usage": {
                        "prompt_tokens": prompt_tokens,
                        "completion_tokens": completion_tokens,
                        "total_tokens": prompt_tokens + completion_tokens,
                        "n_ctx": n_ctx,
                    },
                })).into_response()
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error":e}))).into_response(),
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

#[allow(dead_code)]
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
                InferToken::Done { finish_reason, prompt_tokens, completion_tokens, n_ctx } => {
                    let data = serde_json::to_string(&json!({
                        "id": id, "object": "chat.completion.chunk",
                        "created": ts, "model": model_name,
                        "choices": [{"index":0,"delta":{},"finish_reason":finish_reason}],
                        "usage": {
                            "prompt_tokens":     prompt_tokens,
                            "completion_tokens": completion_tokens,
                            "total_tokens":      prompt_tokens + completion_tokens,
                            "n_ctx":             n_ctx,
                        },
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

#[allow(dead_code)]
async fn collect_chat_response(
    mut tok_rx: mpsc::UnboundedReceiver<InferToken>,
    model_name: &str,
) -> Response {
    let id = format!("chatcmpl-{}", short_id());
    let ts = unix_ts();
    let mut text              = String::new();
    let mut finish_reason     = "stop".to_string();
    let mut prompt_tokens     = 0usize;
    let mut completion_tokens = 0usize;
    let mut n_ctx             = 0usize;

    while let Some(tok) = tok_rx.recv().await {
        match tok {
            InferToken::Delta(t) => text.push_str(&t),
            InferToken::Done { finish_reason: fr, prompt_tokens: pt, completion_tokens: ct, n_ctx: nc } => {
                finish_reason = fr; prompt_tokens = pt; completion_tokens = ct; n_ctx = nc;
                break;
            }
            InferToken::Error(e) => {
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
        "usage": {
            "prompt_tokens":     prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens":      prompt_tokens + completion_tokens,
            "n_ctx":             n_ctx,
        },
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
                InferToken::Done { finish_reason, .. } => {
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
            InferToken::Done { finish_reason: fr, .. } => { finish_reason = fr; break; }
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

// unix_ts_ms imported from super::engine

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
