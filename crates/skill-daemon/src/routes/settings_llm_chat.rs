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

/// Largest byte offset `<= buf.len() - back` that lands on a UTF-8 char
/// boundary. The streaming `<think>` splitter holds back the last `back` bytes
/// of `buf` (a possible partial `<think>`/`</think>` tag split across deltas)
/// before flushing the rest; slicing at a raw `len - back` offset panics when
/// that offset falls inside a multi-byte character (CJK, emoji), so we floor it.
/// Tags are ASCII, so flooring never hides a real partial tag.
fn flush_boundary(buf: &str, back: usize) -> usize {
    let mut safe = buf.len().saturating_sub(back);
    while safe > 0 && !buf.is_char_boundary(safe) {
        safe -= 1;
    }
    safe
}

/// A piece of streamed model output, routed by `<think>` tags.
#[derive(Debug, PartialEq, Eq)]
enum ThinkPart {
    Reasoning(String),
    Content(String),
}

/// Append `delta` to `buf`, then split off any now-emittable reasoning/content,
/// tracking `<think>...</think>` state in `in_think`. A possible partial tag (and
/// any trailing multi-byte char) is held back in `buf` for the next delta; every
/// slice goes through [`flush_boundary`], so a CJK/emoji char split across deltas
/// is never sliced mid-byte — the daemon aborts on ANY panic in this streaming
/// task, so this path must be panic-free. The caller flushes the remaining `buf`
/// once the stream ends.
fn split_think_delta(delta: &str, in_think: &mut bool, buf: &mut String, out: &mut Vec<ThinkPart>) {
    buf.push_str(delta);
    loop {
        if *in_think {
            if let Some(end) = buf.find("</think>") {
                if end > 0 {
                    out.push(ThinkPart::Reasoning(buf[..end].to_string()));
                }
                *buf = buf[end + "</think>".len()..].to_string();
                *in_think = false;
                continue;
            }
            let safe = flush_boundary(buf, 8);
            if safe > 0 {
                out.push(ThinkPart::Reasoning(buf[..safe].to_string()));
                *buf = buf[safe..].to_string();
            }
            break;
        } else if let Some(start) = buf.find("<think>") {
            if start > 0 {
                out.push(ThinkPart::Content(buf[..start].to_string()));
            }
            *buf = buf[start + "<think>".len()..].to_string();
            *in_think = true;
            continue;
        } else {
            let safe = flush_boundary(buf, 7);
            if safe > 0 {
                out.push(ThinkPart::Content(buf[..safe].to_string()));
                *buf = buf[safe..].to_string();
            }
            break;
        }
    }
}

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
                        // Split the delta into reasoning/content, holding back a
                        // partial tag or multi-byte char (panic-free — see
                        // `split_think_delta`). Then emit each piece as an SSE chunk.
                        let mut parts: Vec<ThinkPart> = Vec::new();
                        split_think_delta(delta, &mut in_think, &mut buf, &mut parts);
                        for part in parts {
                            let (field, text) = match part {
                                ThinkPart::Reasoning(t) => ("reasoning_content", t),
                                ThinkPart::Content(t) => ("content", t),
                            };
                            let mut delta_obj = serde_json::Map::new();
                            delta_obj.insert(field.to_string(), serde_json::Value::String(text));
                            let chunk = serde_json::json!({
                                "id": &chat_id2,
                                "object": "chat.completion.chunk",
                                "choices": [{"index": 0, "delta": delta_obj, "finish_reason": serde_json::Value::Null}],
                            });
                            let _ = tx.try_send(format!("data: {}\n\n", chunk));
                        }
                    },
                    |evt| {
                        // Forward real generation-phase markers ("vision" /
                        // "prefill") as an SSE chunk with a `phase` delta field so
                        // the client can show accurate progress during the
                        // pre-token lead-in. Backward-compatible: content-only
                        // consumers ignore the extra field.
                        if let skill_llm::ToolEvent::Phase { phase } = evt {
                            let chunk = serde_json::json!({
                                "id": &chat_id2,
                                "object": "chat.completion.chunk",
                                "choices": [{"index": 0, "delta": {"phase": phase}, "finish_reason": serde_json::Value::Null}],
                            });
                            let _ = tx.try_send(format!("data: {}\n\n", chunk));
                        }
                    },
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

#[cfg(test)]
mod flush_boundary_tests {
    use super::flush_boundary;

    /// Reproduces the daemon crash: the model streams multi-byte output (Fara,
    /// multilingual, emitted "三" = "three"). A raw `len - 7` slice landed inside
    /// the 3-byte char and panicked ("not a char boundary"); flush_boundary must
    /// floor to the char start so `buf[..safe]`/`buf[safe..]` never panic.
    #[test]
    fn floors_into_multibyte_char() {
        let buf = "三"; // 3 bytes, one char
        for back in 0..=4 {
            let safe = flush_boundary(buf, back);
            assert!(buf.is_char_boundary(safe), "back={back} gave mid-char offset {safe}");
            // Must not panic — exercise both halves the splitter slices.
            let _ = (&buf[..safe], &buf[safe..]);
        }
    }

    /// ASCII behaves exactly like the old `len - back` (no flooring needed).
    #[test]
    fn ascii_is_exact() {
        let buf = "hello world"; // 11 bytes
        assert_eq!(flush_boundary(buf, 7), 4);
        assert_eq!(flush_boundary(buf, 0), 11);
        assert_eq!(flush_boundary(buf, 100), 0); // saturates
    }

    /// Mixed ASCII + CJK tail: holding back 1 byte lands raw offset 6 inside the
    /// 3-byte "三"; flooring must back it down to 4 so "red " flushes and the
    /// multi-byte char is held whole (this is the exact daemon-crash shape).
    #[test]
    fn mixed_ascii_and_cjk() {
        let buf = "red 三"; // "red " = 4 bytes, "三" = 3 bytes → 7 total
        let safe = flush_boundary(buf, 1);
        assert_eq!(safe, 4, "raw offset 6 is mid-char; must floor to 4");
        assert!(buf.is_char_boundary(safe));
        assert_eq!(&buf[..safe], "red ");
        let _ = &buf[safe..]; // "三" — must not panic
    }
}

#[cfg(test)]
mod split_think_tests {
    use super::{split_think_delta, ThinkPart};

    /// Drive `split_think_delta` over a list of deltas exactly as the SSE handler
    /// does (including the end-of-stream flush of the held-back tail), returning
    /// the reconstructed (reasoning, content) strings.
    fn run(deltas: &[&str]) -> (String, String) {
        let mut in_think = false;
        let mut buf = String::new();
        let mut parts: Vec<ThinkPart> = Vec::new();
        for d in deltas {
            split_think_delta(d, &mut in_think, &mut buf, &mut parts);
        }
        // End-of-stream flush (mirrors the handler's post-loop flush of `buf`).
        if !buf.is_empty() {
            parts.push(if in_think {
                ThinkPart::Reasoning(buf.clone())
            } else {
                ThinkPart::Content(buf.clone())
            });
        }
        let mut reasoning = String::new();
        let mut content = String::new();
        for p in parts {
            match p {
                ThinkPart::Reasoning(t) => reasoning.push_str(&t),
                ThinkPart::Content(t) => content.push_str(&t),
            }
        }
        (reasoning, content)
    }

    #[test]
    fn whole_delta_with_think_and_cjk() {
        let (r, c) = run(&["<think>reasoning here</think>红、绿、蓝是三原色。"]);
        assert_eq!(r, "reasoning here");
        assert_eq!(c, "红、绿、蓝是三原色。");
    }

    /// The exact daemon-crash scenario: multilingual (CJK) content streamed so the
    /// 7-byte content hold-back repeatedly lands inside a 3-byte char. Must not
    /// panic and must reconstruct the full string.
    #[test]
    fn cjk_content_split_at_char_boundaries() {
        let text = "三原色：红绿蓝。🎨 mixed 日本語 and emoji 😀 end.";
        // Feed as many small deltas split ONLY at char boundaries, but 1 char each
        // so the tail is always a fresh multi-byte char under the 7-byte hold-back.
        let deltas: Vec<String> = text.chars().map(|ch| ch.to_string()).collect();
        let refs: Vec<&str> = deltas.iter().map(String::as_str).collect();
        let (r, c) = run(&refs);
        assert_eq!(r, "");
        assert_eq!(c, text, "CJK/emoji content must round-trip without loss or panic");
    }

    /// Empty `<think>` block (reasoning models emit `<think>\n\n</think>`) followed
    /// by CJK, fed char-by-char across the tag + char boundaries.
    #[test]
    fn empty_think_prefix_then_cjk_charwise() {
        let text = "<think>\n\n</think>答案是：蓝色🌊";
        let deltas: Vec<String> = text.chars().map(|ch| ch.to_string()).collect();
        let refs: Vec<&str> = deltas.iter().map(String::as_str).collect();
        let (r, c) = run(&refs);
        assert_eq!(r, "\n\n");
        assert_eq!(c, "答案是：蓝色🌊");
    }

    /// A `<think>` tag itself split across deltas must still be detected (not
    /// leaked into content) and CJK reasoning must round-trip.
    #[test]
    fn think_tag_split_across_deltas() {
        let (r, c) = run(&["<thi", "nk>推", "理内容", "</thin", "k>结果"]);
        assert_eq!(r, "推理内容");
        assert_eq!(c, "结果");
    }
}
