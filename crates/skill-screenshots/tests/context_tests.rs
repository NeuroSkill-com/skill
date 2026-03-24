// SPDX-License-Identifier: GPL-3.0-only
//! Unit tests for the screenshot context and config types.

use skill_screenshots::context::{ActiveWindowInfo, ScreenshotContext};
use skill_screenshots::config::{ScreenshotConfig, fastembed_model_enum};
use serde_json::Value;

/// Minimal mock context for testing.
struct MockCtx {
    config: ScreenshotConfig,
    session_active: bool,
}

impl ScreenshotContext for MockCtx {
    fn config(&self) -> ScreenshotConfig { self.config.clone() }
    fn is_session_active(&self) -> bool { self.session_active }
    fn active_window(&self) -> ActiveWindowInfo { ActiveWindowInfo::default() }
    fn emit_event(&self, _event: &str, _payload: Value) {}
    fn embed_image_via_llm(&self, _png: &[u8]) -> Option<Vec<f32>> { None }
}

#[test]
fn mock_context_default_config() {
    let ctx = MockCtx {
        config: ScreenshotConfig::default(),
        session_active: false,
    };
    assert!(!ctx.is_session_active());
    let win = ctx.active_window();
    assert!(win.app_name.is_empty());
    assert!(win.window_title.is_empty());
}

#[test]
fn fastembed_model_clip() {
    let mut cfg = ScreenshotConfig::default();
    cfg.fastembed_model = "clip-vit-b-32".into();
    assert!(fastembed_model_enum(&cfg).is_some());
}

#[test]
fn fastembed_model_nomic() {
    let mut cfg = ScreenshotConfig::default();
    cfg.fastembed_model = "nomic-embed-vision-v1.5".into();
    assert!(fastembed_model_enum(&cfg).is_some());
}

#[test]
fn fastembed_model_unknown_returns_none() {
    let mut cfg = ScreenshotConfig::default();
    cfg.fastembed_model = "unknown-model".into();
    assert!(fastembed_model_enum(&cfg).is_none());
}

#[test]
fn active_window_info_default() {
    let info = ActiveWindowInfo::default();
    assert!(info.app_name.is_empty());
    assert!(info.window_title.is_empty());
}
