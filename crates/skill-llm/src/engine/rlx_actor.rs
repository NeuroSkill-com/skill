// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! RLX inference actor — processes [`InferRequest`]s on a dedicated OS thread.
//!
//! Owns a [`RlxTextRunner`] (which wraps `Box<dyn LmRunner>` from rlx-models)
//! and processes [`InferRequest`]s on a dedicated OS thread. Mirrors the
//! [`actor`](super::actor) module's event-loop shape so the rest of the
//! engine machinery (init, channels, status events) stays unchanged.
//!
//! Capabilities:
//!   * Generate / Complete — supported (text-only).
//!   * Embed                — not supported (returns an error).
//!   * EmbedImage           — not supported (returns `None`).
//!   * Health               — supported.
//!
//! Chat templating uses `rlx_models::run::auto_chat_template`, which loads
//! the Jinja chat template directly from the GGUF metadata. MiniCPM5 ships
//! an agent/tooling template that relies on Python-only Jinja helpers, so
//! that family uses a simplified ChatML template instead.

use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use serde_json::json;

use super::logging::{LlmLogBuffer, LlmLogFile};
use super::protocol::{InferRequest, InferToken};
use super::rlx_backend::RlxTextRunner;
use crate::config::LlmConfig;
use crate::event::LlmEventEmitter;

#[allow(clippy::too_many_arguments)]
pub(super) fn run_actor(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<InferRequest>,
    config: LlmConfig,
    model_path: std::path::PathBuf,
    mmproj_path: Option<std::path::PathBuf>,
    app: Arc<dyn LlmEventEmitter>,
    log_buf: LlmLogBuffer,
    log_path: Option<std::path::PathBuf>,
    ready_flag: Arc<AtomicBool>,
    n_ctx_flag: Arc<std::sync::atomic::AtomicUsize>,
    vision_flag: Arc<AtomicBool>,
) {
    let log_file_handle: Option<LlmLogFile> = log_path.as_ref().and_then(|p| {
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(p)
            .ok()
            .map(|f| Arc::new(Mutex::new(std::io::BufWriter::new(f))))
    });
    let log_file = log_file_handle.as_ref();

    llm_info!(
        &app,
        &log_buf,
        log_file,
        "[rlx-only actor] loading model: {}",
        model_path.display()
    );

    let mut runner = match RlxTextRunner::load_with_mmproj(&model_path, mmproj_path.as_deref(), &config) {
        Ok(r) => r,
        Err(e) => {
            llm_error!(&app, &log_buf, log_file, "RLX runner load failed: {e}");
            app.emit_event(
                "llm:status",
                json!({"status":"stopped","error":format!("RLX load failed: {e}")}),
            );
            return;
        }
    };

    let supports_vision = runner.supports_multimodal();
    vision_flag.store(supports_vision, Ordering::Relaxed);
    // No fixed n_ctx in RLX — report config-requested value (or 0).
    n_ctx_flag.store(config.ctx_size.unwrap_or(0) as usize, Ordering::Relaxed);

    let family = runner.family();
    let chat_template = resolve_chat_template(&model_path, family, &app, &log_buf, log_file);

    ready_flag.store(true, Ordering::Relaxed);
    let model_file = model_path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    llm_info!(
        &app,
        &log_buf,
        log_file,
        "[rlx-only actor] ready — model={} family={}",
        model_file,
        family
    );
    app.emit_event(
        "llm:status",
        json!({
            "status": "running",
            "model": model_file,
            "runtime": "Rlx",
            "supports_vision": supports_vision,
            "supports_tools": true,
        }),
    );

    while let Some(req) = rx.blocking_recv() {
        match req {
            InferRequest::Health { result_tx } => {
                result_tx.send(true).ok();
            }

            InferRequest::Generate {
                messages,
                images,
                params,
                token_tx,
            } => {
                let prompt = render_chat(&chat_template, &messages);
                let prompt = match prompt {
                    Ok(p) => p,
                    Err(e) => {
                        token_tx
                            .send(InferToken::Error(format!("chat template render failed: {e}")))
                            .ok();
                        continue;
                    }
                };
                llm_info!(
                    &app,
                    &log_buf,
                    log_file,
                    "chat request — {} messages, {} image(s), max_tokens={}",
                    messages.len(),
                    images.len(),
                    params.max_tokens
                );
                if !images.is_empty() {
                    if runner.supports_multimodal() {
                        runner.generate_multimodal(&prompt, &images, params, token_tx);
                    } else {
                        token_tx
                            .send(InferToken::Error(
                                "RLX runtime: this model has no mmproj vision encoder \
                                 attached — load an mmproj GGUF or use a text-only model"
                                    .into(),
                            ))
                            .ok();
                    }
                    continue;
                }
                runner.generate(&prompt, params, token_tx);
            }

            InferRequest::Complete {
                prompt,
                params,
                token_tx,
            } => {
                llm_info!(
                    &app,
                    &log_buf,
                    log_file,
                    "completion request — max_tokens={}",
                    params.max_tokens
                );
                runner.generate(&prompt, params, token_tx);
            }

            InferRequest::Embed { result_tx, .. } => {
                result_tx
                    .send(Err(anyhow::anyhow!(
                        "embeddings are not supported by the RLX-only actor"
                    )))
                    .ok();
            }

            InferRequest::EmbedImage { result_tx, .. } => {
                result_tx.send(None).ok();
            }
        }
    }

    drop(runner);
    llm_info!(&app, &log_buf, log_file, "[rlx-only actor] exiting — runner dropped");
    app.emit_event("llm:status", json!({"status":"stopped"}));
}

/// MiniCPM5 GGUF metadata embeds a tool-agent Jinja template that calls
/// Python string methods (`startswith`, …) unsupported by minijinja. Use a
/// ChatML subset that matches plain chat + generation without tools.
fn minicpm5_chat_template(model_path: &Path) -> anyhow::Result<rlx_models::run::ChatTemplate> {
    use rlx_models::run::ChatTemplate;
    let im_end = format!("<|{}|>", "im_end");
    let source = format!(
        "{{{{ bos_token }}}}{{%- for m in messages -%}}\
        {{%- if m.role == 'system' -%}}\
        <|im_start|>system\n{{{{ m.content }}}}{im_end}\n\
        {{%- elif m.role == 'user' or (m.role == 'system' and not loop.first) -%}}\
        <|im_start|>{{{{ m.role }}}}\n{{{{ m.content }}}}{im_end}\n\
        {{%- elif m.role == 'assistant' -%}}\
        <|im_start|>assistant\n{{{{ m.content }}}}{im_end}\n\
        {{%- endif -%}}\
        {{%- endfor -%}}\
        {{%- if add_generation_prompt -%}}\
        <|im_start|>assistant\n\
        {{%- endif -%}}",
        im_end = im_end,
    );
    let mut tpl = ChatTemplate::from_source(source)?;
    if let Ok(gguf) = ChatTemplate::from_gguf(model_path) {
        tpl = tpl.with_tokens(
            gguf.bos_token().map(str::to_string),
            gguf.eos_token().map(str::to_string),
        );
    }
    Ok(tpl)
}

fn resolve_chat_template(
    model_path: &Path,
    family: &str,
    app: &Arc<dyn LlmEventEmitter>,
    log_buf: &LlmLogBuffer,
    log_file: Option<&LlmLogFile>,
) -> Option<rlx_models::run::ChatTemplate> {
    if family == "minicpm5" {
        match minicpm5_chat_template(model_path) {
            Ok(t) => {
                llm_info!(
                    app,
                    log_buf,
                    log_file,
                    "MiniCPM5: using simplified ChatML template (GGUF agent template skipped)"
                );
                return Some(t);
            }
            Err(e) => {
                llm_warn!(
                    app,
                    log_buf,
                    log_file,
                    "MiniCPM5 chat template setup failed ({e}) — trying GGUF metadata"
                );
            }
        }
    }

    match rlx_models::run::auto_chat_template(model_path) {
        Ok(t) => Some(t),
        Err(e) => {
            llm_warn!(
                app,
                log_buf,
                log_file,
                "no chat template available ({e}) — falling back to simple role-tagged concat"
            );
            None
        }
    }
}

/// Render a list of `{"role","content"}` chat messages via the resolved
/// chat template. Falls back to a simple `"<role>: <content>\n"` concat
/// when no template was loaded.
fn render_chat(
    template: &Option<rlx_models::run::ChatTemplate>,
    messages: &[serde_json::Value],
) -> anyhow::Result<String> {
    use rlx_models::run::ChatMessage;
    let msgs: Vec<ChatMessage> = messages
        .iter()
        .map(|m| {
            let role = m.get("role").and_then(|v| v.as_str()).unwrap_or("user").to_string();
            // `content` may be a string or an array of parts; we
            // only join the textual parts (rlx is text-only here).
            let content = match m.get("content") {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Array(parts)) => parts
                    .iter()
                    .filter_map(|p| {
                        if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                            Some(t.to_string())
                        } else {
                            p.as_str().map(|s| s.to_string())
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            };
            ChatMessage { role, content }
        })
        .collect();
    if let Some(tpl) = template {
        tpl.render(&msgs, true).or_else(|_| simple_render_chat(&msgs))
    } else {
        simple_render_chat(&msgs)
    }
}

fn simple_render_chat(msgs: &[rlx_models::run::ChatMessage]) -> anyhow::Result<String> {
    let mut out = String::new();
    for m in msgs {
        out.push_str(&format!("{}: {}\n", m.role, m.content));
    }
    out.push_str("assistant: ");
    Ok(out)
}
