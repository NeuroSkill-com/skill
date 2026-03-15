// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Event emitter abstraction — decouples LLM logic from tauri::AppHandle.

use serde_json::Value;

/// Trait for emitting events to the frontend or any listener.
///
/// In production this is implemented by `tauri::AppHandle`; in tests or
/// standalone usage it can be a no-op or a collector.
pub trait LlmEventEmitter: Send + Sync + 'static {
    /// Emit a named event with a JSON payload.
    fn emit_event(&self, event: &str, payload: Value);
}

/// Blanket impl: `Arc<dyn LlmEventEmitter>` also implements the trait,
/// so functions taking `&dyn LlmEventEmitter` work with `&Arc<dyn LlmEventEmitter>`.
impl<T: LlmEventEmitter + ?Sized> LlmEventEmitter for std::sync::Arc<T> {
    fn emit_event(&self, event: &str, payload: Value) {
        (**self).emit_event(event, payload);
    }
}

/// No-op emitter for testing or headless usage.
#[derive(Clone)]
pub struct NoopEmitter;

impl LlmEventEmitter for NoopEmitter {
    fn emit_event(&self, _event: &str, _payload: Value) {}
}
