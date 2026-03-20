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
}
