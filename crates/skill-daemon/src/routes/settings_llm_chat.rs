// SPDX-License-Identifier: GPL-3.0-only
//! LLM chat/completions/image/OCR handlers.

use axum::{extract::State, Json};
use base64::Engine as _;
use tokio_stream::StreamExt as _;

use crate::{
    routes::settings::{
        ChatCompletionsRequest, ChatIdRequest, ChatRenameRequest, ChatSaveMessageRequest, ChatSaveToolCallsRequest,
        ChatSessionParamsRequest, ChatSessionResponse, LlmImageRequest, ToolCancelRequest,
    },
    state::AppState,
};

pub(crate) async fn chat_last_session_impl(State(state): State<AppState>) -> Json<ChatSessionResponse> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) else {
            return ChatSessionResponse {
                session_id: 0,
                messages: vec![],
            };
        };
        let session_id = store.get_or_create_last_session();
        let messages = store.load_session(session_id);
        ChatSessionResponse { session_id, messages }
    })
    .await
    .unwrap_or(ChatSessionResponse {
        session_id: 0,
        messages: vec![],
    });
    Json(out)
}

pub(crate) async fn chat_load_session_impl(
    State(state): State<AppState>,
    Json(req): Json<ChatIdRequest>,
) -> Json<ChatSessionResponse> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) else {
            return ChatSessionResponse {
                session_id: req.id,
                messages: vec![],
            };
        };
        let messages = store.load_session(req.id);
        ChatSessionResponse {
            session_id: req.id,
            messages,
        }
    })
    .await
    .unwrap_or(ChatSessionResponse {
        session_id: req.id,
        messages: vec![],
    });
    Json(out)
}

pub(crate) async fn chat_list_sessions_impl(
    State(state): State<AppState>,
) -> Json<Vec<skill_llm::chat_store::SessionSummary>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        skill_llm::chat_store::ChatStore::open(&skill_dir)
            .map(|mut store| store.list_sessions())
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(out)
}

pub(crate) async fn chat_rename_session_impl(
    State(state): State<AppState>,
    Json(req): Json<ChatRenameRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) {
            store.rename_session(req.id, &req.title);
        }
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn chat_delete_session_impl(
    State(state): State<AppState>,
    Json(req): Json<ChatIdRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) {
            store.delete_session(req.id);
        }
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn chat_archive_session_impl(
    State(state): State<AppState>,
    Json(req): Json<ChatIdRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) {
            store.archive_session(req.id);
        }
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn chat_unarchive_session_impl(
    State(state): State<AppState>,
    Json(req): Json<ChatIdRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) {
            store.unarchive_session(req.id);
        }
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn chat_list_archived_sessions_impl(
    State(state): State<AppState>,
) -> Json<Vec<skill_llm::chat_store::SessionSummary>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let out = tokio::task::spawn_blocking(move || {
        skill_llm::chat_store::ChatStore::open(&skill_dir)
            .map(|mut store| store.list_archived_sessions())
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(out)
}

pub(crate) async fn chat_save_message_impl(
    State(state): State<AppState>,
    Json(req): Json<ChatSaveMessageRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let id = tokio::task::spawn_blocking(move || {
        skill_llm::chat_store::ChatStore::open(&skill_dir)
            .map(|mut store| store.save_message(req.session_id, &req.role, &req.content, req.thinking.as_deref()))
            .unwrap_or(0)
    })
    .await
    .unwrap_or(0);
    Json(serde_json::json!({"id": id}))
}

pub(crate) async fn chat_get_session_params_impl(
    State(state): State<AppState>,
    Json(req): Json<ChatIdRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let value = tokio::task::spawn_blocking(move || {
        skill_llm::chat_store::ChatStore::open(&skill_dir)
            .map(|store| store.get_session_params(req.id))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    Json(serde_json::json!({"value": value}))
}

pub(crate) async fn chat_set_session_params_impl(
    State(state): State<AppState>,
    Json(req): Json<ChatSessionParamsRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) {
            store.set_session_params(req.id, &req.params_json);
        }
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn chat_new_session_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let id = tokio::task::spawn_blocking(move || {
        skill_llm::chat_store::ChatStore::open(&skill_dir)
            .map(|mut store| store.new_session())
            .unwrap_or(0)
    })
    .await
    .unwrap_or(0);
    Json(serde_json::json!({"id": id}))
}

pub(crate) async fn chat_save_tool_calls_impl(
    State(state): State<AppState>,
    Json(req): Json<ChatSaveToolCallsRequest>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(mut store) = skill_llm::chat_store::ChatStore::open(&skill_dir) {
            store.save_tool_calls(req.message_id, &req.tool_calls);
        }
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn llm_chat_completions_impl(
    State(state): State<AppState>,
    Json(req): Json<ChatCompletionsRequest>,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let want_stream = req.stream.unwrap_or(false);

    #[cfg(feature = "llm")]
    {
        let srv_opt = state.llm_state_cell.lock().ok().and_then(|g| g.clone());
        let Some(srv) = srv_opt else {
            return Json(serde_json::json!({"error":"LLM server not running"})).into_response();
        };

        // Build params: prefer explicit `params` object, fall back to OpenAI top-level fields.
        let params_val = if req.params.is_null() || req.params.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            let mut p = serde_json::Map::new();
            if let Some(t) = req.temperature {
                p.insert("temperature".into(), t.into());
            }
            if let Some(m) = req.max_tokens {
                p.insert("n_predict".into(), m.into());
            }
            if let Some(s) = req.stop {
                p.insert("stop".into(), s);
            }
            serde_json::Value::Object(p)
        } else {
            req.params
        };
        let params: skill_llm::GenParams = serde_json::from_value(params_val).unwrap_or_default();

        let chat_id = format!(
            "chatcmpl-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );

        if want_stream {
            // SSE streaming response
            let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);
            let chat_id2 = chat_id.clone();

            tokio::spawn(async move {
                // Track <think>...</think> tags to route to reasoning_content vs content
                let mut in_think = false;
                let mut buf = String::new();

                let result = skill_llm::run_chat_with_builtin_tools(
                    &srv, req.messages, params, Vec::new(),
                    |delta| {
                        buf.push_str(delta);

                        // Process buffered text for think tag boundaries
                        loop {
                            if in_think {
                                if let Some(end) = buf.find("</think>") {
                                    let thinking = &buf[..end];
                                    if !thinking.is_empty() {
                                        let chunk = serde_json::json!({
                                            "id": &chat_id2,
                                            "object": "chat.completion.chunk",
                                            "choices": [{"index": 0, "delta": {"reasoning_content": thinking}, "finish_reason": serde_json::Value::Null}],
                                        });
                                        let _ = tx.try_send(format!("data: {}\n\n", chunk));
                                    }
                                    buf = buf[end + "</think>".len()..].to_string();
                                    in_think = false;
                                    continue;
                                }
                                // Still inside think — might have partial </think> at end
                                // Flush everything except last 8 chars (len of "</think>")
                                let safe = buf.len().saturating_sub(8);
                                if safe > 0 {
                                    let chunk = serde_json::json!({
                                        "id": &chat_id2,
                                        "object": "chat.completion.chunk",
                                        "choices": [{"index": 0, "delta": {"reasoning_content": &buf[..safe]}, "finish_reason": serde_json::Value::Null}],
                                    });
                                    let _ = tx.try_send(format!("data: {}\n\n", chunk));
                                    buf = buf[safe..].to_string();
                                }
                                break;
                            } else {
                                if let Some(start) = buf.find("<think>") {
                                    let before = &buf[..start];
                                    if !before.is_empty() {
                                        let chunk = serde_json::json!({
                                            "id": &chat_id2,
                                            "object": "chat.completion.chunk",
                                            "choices": [{"index": 0, "delta": {"content": before}, "finish_reason": serde_json::Value::Null}],
                                        });
                                        let _ = tx.try_send(format!("data: {}\n\n", chunk));
                                    }
                                    buf = buf[start + "<think>".len()..].to_string();
                                    in_think = true;
                                    continue;
                                }
                                // No <think> tag — might have partial at end
                                let safe = buf.len().saturating_sub(7);
                                if safe > 0 {
                                    let chunk = serde_json::json!({
                                        "id": &chat_id2,
                                        "object": "chat.completion.chunk",
                                        "choices": [{"index": 0, "delta": {"content": &buf[..safe]}, "finish_reason": serde_json::Value::Null}],
                                    });
                                    let _ = tx.try_send(format!("data: {}\n\n", chunk));
                                    buf = buf[safe..].to_string();
                                }
                                break;
                            }
                        }
                    },
                    |_evt| {},
                ).await;

                // Flush remaining buffer
                if !buf.is_empty() {
                    let field = if in_think { "reasoning_content" } else { "content" };
                    let chunk = serde_json::json!({
                        "id": &chat_id2,
                        "object": "chat.completion.chunk",
                        "choices": [{"index": 0, "delta": {field: &buf}, "finish_reason": serde_json::Value::Null}],
                    });
                    let _ = tx.try_send(format!("data: {}\n\n", chunk));
                }

                // Send final chunk with finish_reason and usage
                match result {
                    Ok((_text, finish_reason, prompt_tokens, completion_tokens, _n_ctx)) => {
                        let final_chunk = serde_json::json!({
                            "id": &chat_id2,
                            "object": "chat.completion.chunk",
                            "choices": [{
                                "index": 0,
                                "delta": {},
                                "finish_reason": finish_reason,
                            }],
                            "usage": {
                                "prompt_tokens": prompt_tokens,
                                "completion_tokens": completion_tokens,
                                "total_tokens": prompt_tokens + completion_tokens,
                            },
                        });
                        let _ = tx.send(format!("data: {}\n\n", final_chunk)).await;
                    }
                    Err(e) => {
                        let err_chunk = serde_json::json!({
                            "error": { "message": e.to_string() },
                        });
                        let _ = tx.send(format!("data: {}\n\n", err_chunk)).await;
                    }
                }
                let _ = tx.send("data: [DONE]\n\n".to_string()).await;
            });

            let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
            let body = axum::body::Body::from_stream(stream.map(|s| Ok::<_, std::convert::Infallible>(s)));
            return axum::response::Response::builder()
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .body(body)
                .unwrap_or_else(|_| Json(serde_json::json!({"error":"stream setup failed"})).into_response());
        }

        // Non-streaming response
        let result =
            skill_llm::run_chat_with_builtin_tools(&srv, req.messages, params, Vec::new(), |_delta| {}, |_evt| {})
                .await;

        return match result {
            Ok((text, finish_reason, prompt_tokens, completion_tokens, _n_ctx)) => Json(serde_json::json!({
                "id": chat_id,
                "object": "chat.completion",
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": text },
                    "finish_reason": finish_reason,
                }],
                "usage": {
                    "prompt_tokens": prompt_tokens,
                    "completion_tokens": completion_tokens,
                    "total_tokens": prompt_tokens + completion_tokens,
                }
            }))
            .into_response(),
            Err(e) => Json(serde_json::json!({"error": e.to_string()})).into_response(),
        };
    }

    #[cfg(not(feature = "llm"))]
    {
        let _ = req;
        let _ = state;
        let _ = want_stream;
        Json(serde_json::json!({
            "content": "Daemon LLM unavailable (compiled without llm feature)",
            "finish_reason": "stop",
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "n_ctx": 0
        }))
        .into_response()
    }
}

pub(crate) async fn llm_embed_image_impl(
    State(state): State<AppState>,
    Json(req): Json<LlmImageRequest>,
) -> Json<serde_json::Value> {
    let bytes = match base64::engine::general_purpose::STANDARD.decode(req.png_base64.as_bytes()) {
        Ok(b) => b,
        Err(e) => return Json(serde_json::json!({"error": format!("invalid base64: {e}")})),
    };

    #[cfg(feature = "llm")]
    {
        let srv_opt = state.llm_state_cell.lock().ok().and_then(|g| g.clone());
        let Some(srv) = srv_opt else {
            return Json(serde_json::json!({"error":"LLM server not running"}));
        };
        if !srv.vision_ready.load(std::sync::atomic::Ordering::Relaxed) {
            return Json(serde_json::json!({"error":"vision not ready"}));
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        if srv
            .req_tx
            .send(skill_llm::InferRequest::EmbedImage { bytes, result_tx: tx })
            .is_err()
        {
            return Json(serde_json::json!({"error":"failed to queue embed request"}));
        }
        return match rx.await {
            Ok(Some(v)) => Json(serde_json::json!({"embedding": v})),
            Ok(None) => Json(serde_json::json!({"embedding": serde_json::Value::Null})),
            Err(e) => Json(serde_json::json!({"error": e.to_string()})),
        };
    }

    #[cfg(not(feature = "llm"))]
    {
        let _ = bytes;
        let _ = state;
        Json(serde_json::json!({"error":"LLM unavailable"}))
    }
}

pub(crate) async fn llm_ocr_impl(
    State(state): State<AppState>,
    Json(req): Json<LlmImageRequest>,
) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        let srv_opt = state.llm_state_cell.lock().ok().and_then(|g| g.clone());
        let Some(srv) = srv_opt else {
            return Json(serde_json::json!({"error":"LLM server not running"}));
        };

        let data_url = format!("data:image/png;base64,{}", req.png_base64);
        let messages = vec![
            serde_json::json!({
                "role": "system",
                "content": "You are an OCR assistant. Extract ALL visible text from the image exactly as it appears. Output only the extracted text, nothing else. Preserve line breaks. If no text is visible, output an empty string."
            }),
            serde_json::json!({
                "role": "user",
                "content": [
                    {"type":"image_url","image_url":{"url": data_url}},
                    {"type":"text","text":"Extract all visible text from this screenshot."}
                ]
            }),
        ];

        let params = skill_llm::GenParams {
            max_tokens: 2048,
            temperature: 0.0,
            thinking_budget: Some(0),
            ..Default::default()
        };

        let result =
            skill_llm::run_chat_with_builtin_tools(&srv, messages, params, Vec::new(), |_delta| {}, |_evt| {}).await;

        return match result {
            Ok((text, ..)) => Json(serde_json::json!({"text": text.trim()})),
            Err(e) => Json(serde_json::json!({"error": e.to_string()})),
        };
    }

    #[cfg(not(feature = "llm"))]
    {
        let _ = req;
        let _ = state;
        Json(serde_json::json!({"error":"LLM unavailable"}))
    }
}

pub(crate) async fn llm_abort_stream_impl(State(state): State<AppState>) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        if let Ok(guard) = state.llm_state_cell.lock() {
            if let Some(srv) = guard.as_ref() {
                srv.abort_tx.send_modify(|v| *v = v.wrapping_add(1));
            }
        }
    }
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn llm_cancel_tool_call_impl(
    State(state): State<AppState>,
    Json(req): Json<ToolCancelRequest>,
) -> Json<serde_json::Value> {
    #[cfg(feature = "llm")]
    {
        if let Ok(guard) = state.llm_state_cell.lock() {
            if let Some(srv) = guard.as_ref() {
                if let Ok(mut c) = srv.cancelled_tool_calls.lock() {
                    c.insert(req.tool_call_id);
                }
            }
        }
    }
    #[cfg(not(feature = "llm"))]
    {
        let _ = req;
    }
    Json(serde_json::json!({"ok": true}))
}
