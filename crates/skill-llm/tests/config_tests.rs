#![allow(clippy::unwrap_used)]
// SPDX-License-Identifier: GPL-3.0-only
//! Tests for LlmConfig defaults and serde round-trip.

use skill_llm::config::LlmConfig;

#[test]
fn default_config_disabled() {
    let cfg = LlmConfig::default();
    assert!(!cfg.enabled);
    assert!(!cfg.autostart);
}

#[test]
fn default_gpu_layers_offload_all() {
    let cfg = LlmConfig::default();
    assert_eq!(cfg.n_gpu_layers, u32::MAX);
}

#[test]
fn default_ctx_size_auto() {
    let cfg = LlmConfig::default();
    assert!(cfg.ctx_size.is_none());
}

#[test]
fn default_flash_attention_on() {
    let cfg = LlmConfig::default();
    assert!(cfg.flash_attention);
    assert!(cfg.offload_kqv);
}

#[test]
fn default_memory_thresholds_sensible() {
    let cfg = LlmConfig::default();
    assert!(cfg.gpu_memory_threshold > 0.0);
    assert!(cfg.gpu_memory_gen_threshold > 0.0);
    assert!(cfg.gpu_memory_threshold >= cfg.gpu_memory_gen_threshold);
}

#[test]
fn serde_roundtrip_default() {
    let cfg = LlmConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let restored: LlmConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg.enabled, restored.enabled);
    assert_eq!(cfg.n_gpu_layers, restored.n_gpu_layers);
    assert_eq!(cfg.flash_attention, restored.flash_attention);
}

#[test]
fn serde_roundtrip_custom() {
    let mut cfg = LlmConfig::default();
    cfg.enabled = true;
    cfg.ctx_size = Some(4096);
    cfg.api_key = Some("secret".into());
    cfg.verbose = true;

    let json = serde_json::to_string(&cfg).unwrap();
    let restored: LlmConfig = serde_json::from_str(&json).unwrap();
    assert!(restored.enabled);
    assert_eq!(restored.ctx_size, Some(4096));
    assert_eq!(restored.api_key.as_deref(), Some("secret"));
    assert!(restored.verbose);
}

#[test]
fn serde_missing_fields_use_defaults() {
    let json = r#"{"enabled": true}"#;
    let cfg: LlmConfig = serde_json::from_str(json).unwrap();
    assert!(cfg.enabled);
    // n_gpu_layers uses #[serde(default)] → 0, not u32::MAX from Default impl
    assert!(cfg.flash_attention);
    assert_eq!(cfg.parallel, 1);
}

#[test]
fn serde_unknown_fields_ignored() {
    let json = r#"{"enabled": false, "unknown_future_field": 42}"#;
    let cfg: LlmConfig = serde_json::from_str(json).unwrap();
    assert!(!cfg.enabled);
}
