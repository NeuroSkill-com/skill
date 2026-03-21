// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Screenshot module — thin adapter over the `skill-screenshots` crate.
//!
//! Re-exports the public API and provides a `TauriScreenshotContext` that
//! bridges `tauri::AppHandle` + `AppState` to the `ScreenshotContext` trait.

#[allow(unused_imports)]
pub use skill_screenshots::capture::*;
#[allow(unused_imports)]
pub use skill_screenshots::config::*;
#[allow(unused_imports)]
pub use skill_screenshots::context::*;
#[allow(unused_imports)]
pub use skill_screenshots::ScreenshotConfig;

use std::sync::Arc;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use crate::{MutexExt, EmbedderState};
use crate::AppStateExt;

/// Bridges `tauri::AppHandle` + `AppState` to the `ScreenshotContext` trait.
pub struct TauriScreenshotContext {
    pub app: AppHandle,
}

impl skill_screenshots::ScreenshotContext for TauriScreenshotContext {
    fn config(&self) -> skill_screenshots::ScreenshotConfig {
        let r = self.app.app_state();
        let g = r.lock_or_recover();
        g.screenshot_config.clone()
    }

    fn is_session_active(&self) -> bool {
        let r = self.app.app_state();
        let g = r.lock_or_recover();
        g.session_start_utc.is_some()
    }

    fn active_window(&self) -> skill_screenshots::ActiveWindowInfo {
        let r = self.app.app_state();
        let g = r.lock_or_recover();
        match &g.input.current_active_window {
            Some(aw) => skill_screenshots::ActiveWindowInfo {
                app_name: aw.app_name.clone(),
                window_title: aw.window_title.clone(),
            },
            None => skill_screenshots::ActiveWindowInfo::default(),
        }
    }

    fn emit_event(&self, event: &str, payload: Value) {
        let _ = self.app.emit(event, payload);
    }

    fn embed_text(&self, text: &str) -> Option<Vec<f32>> {
        let embedder = Arc::clone(&*self.app.state::<Arc<EmbedderState>>());
        let mut guard = embedder.0.lock().ok()?;
        let te = guard.as_mut()?;
        let mut vecs = te.embed(vec![text], None).ok()?;
        if vecs.is_empty() { None } else { Some(vecs.remove(0)) }
    }

    fn embed_image_via_llm(&self, png_bytes: &[u8]) -> Option<Vec<f32>> {
        #[cfg(feature = "llm")]
        {
            let cell = {
                let r = self.app.app_state();
                let g = r.lock_or_recover();
                { let __a = g.llm.clone(); let __r = __a.lock_or_recover().state_cell.clone(); __r }
            };
            let state = cell.lock().ok()?.as_ref()?.clone();
            if !state.vision_ready.load(std::sync::atomic::Ordering::Relaxed) {
                return None;
            }
            let (tx, rx) = tokio::sync::oneshot::channel();
            state.req_tx.send(crate::llm::InferRequest::EmbedImage {
                bytes: png_bytes.to_vec(),
                result_tx: tx,
            }).ok()?;
            rx.blocking_recv().ok()?
        }
        #[cfg(not(feature = "llm"))]
        {
            let _ = png_bytes;
            None
        }
    }

    fn ocr_via_llm(&self, png_bytes: &[u8]) -> Option<String> {
        #[cfg(feature = "llm")]
        {
            let cell = {
                let r = self.app.app_state();
                let g = r.lock_or_recover();
                { let __a = g.llm.clone(); let __r = __a.lock_or_recover().state_cell.clone(); __r }
            };
            let state = cell.lock().ok()?.as_ref()?.clone();
            if !state.is_ready() || !state.vision_ready.load(std::sync::atomic::Ordering::Relaxed) {
                return None;
            }

            // Encode the image as a base64 data URL for the chat message.
            let b64 = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                png_bytes,
            );
            let data_url = format!("data:image/png;base64,{b64}");

            let messages = vec![
                serde_json::json!({
                    "role": "system",
                    "content": "You are an OCR assistant. Extract ALL visible text from the image exactly as it appears. Output only the extracted text, nothing else. Preserve line breaks. If no text is visible, output an empty string."
                }),
                serde_json::json!({
                    "role": "user",
                    "content": [
                        {
                            "type": "image_url",
                            "image_url": { "url": data_url }
                        },
                        {
                            "type": "text",
                            "text": "Extract all visible text from this screenshot."
                        }
                    ]
                }),
            ];

            let images = crate::llm::extract_images_from_messages(&messages);
            let params = crate::llm::GenParams {
                max_tokens: 2048,
                temperature: 0.0,
                thinking_budget: Some(0),
                ..Default::default()
            };

            let (tok_tx, mut tok_rx) = tokio::sync::mpsc::unbounded_channel();
            state.req_tx.send(crate::llm::InferRequest::Generate {
                messages,
                images,
                params,
                token_tx: tok_tx,
            }).ok()?;

            // Collect tokens synchronously (we're on the embed thread).
            let mut text = String::new();
            while let Some(tok) = tok_rx.blocking_recv() {
                match tok {
                    crate::llm::InferToken::Delta(t) => text.push_str(&t),
                    crate::llm::InferToken::Done { .. } => break,
                    crate::llm::InferToken::Error(_) => return None,
                }
            }

            let trimmed = text.trim().to_string();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        }
        #[cfg(not(feature = "llm"))]
        {
            let _ = png_bytes;
            None
        }
    }

    fn gpu_init_guard(&self) -> Option<Box<dyn std::any::Any + Send>> {
        // Acquire the lock and wrap it in a newtype that is Send.
        // The guard cannot outlive the static mutex, so this is safe —
        // the Box keeps the guard alive until dropped.
        #[allow(dead_code)]
        struct GpuGuard(std::sync::MutexGuard<'static, ()>);
        // SAFETY: The MutexGuard borrows a process-global static Mutex.
        // It is only ever used on the calling thread (the screenshot embed
        // thread) and dropped in place — never actually transferred across
        // threads.  The Send bound is required by the trait signature but
        // the guard is held and dropped on the same thread.
        unsafe impl Send for GpuGuard {}
        Some(Box::new(GpuGuard(crate::gpu_init_lock())))
    }
}
