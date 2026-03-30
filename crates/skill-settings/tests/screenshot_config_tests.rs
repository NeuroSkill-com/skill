#![allow(clippy::unwrap_used)]
// SPDX-License-Identifier: GPL-3.0-only
//! Tests for ScreenshotConfig defaults, serde round-trip, and interval logic.

use skill_settings::screenshot_config::ScreenshotConfig;

// ── Default values ───────────────────────────────────────────────────────────

#[test]
fn default_config_is_disabled() {
    let cfg = ScreenshotConfig::default();
    assert!(!cfg.enabled);
}

#[test]
fn default_interval_is_5() {
    let cfg = ScreenshotConfig::default();
    assert_eq!(cfg.interval_secs, 5);
}

#[test]
fn default_quality_in_range() {
    let cfg = ScreenshotConfig::default();
    assert!(cfg.quality > 0 && cfg.quality <= 100);
}

#[test]
fn default_ocr_enabled() {
    let cfg = ScreenshotConfig::default();
    assert!(cfg.ocr_enabled);
}

#[test]
fn default_gif_disabled() {
    let cfg = ScreenshotConfig::default();
    assert!(!cfg.gif_enabled);
}

// ── effective_interval_secs ──────────────────────────────────────────────────

#[test]
fn effective_interval_default() {
    let cfg = ScreenshotConfig::default();
    // interval_secs=5, epoch=5 → multiplier=1 → effective=5
    assert_eq!(cfg.effective_interval_secs(), 5);
}

#[test]
fn effective_interval_multiple_of_epoch() {
    let mut cfg = ScreenshotConfig::default();
    cfg.interval_secs = 15;
    assert_eq!(cfg.effective_interval_secs(), 15);
}

#[test]
fn effective_interval_rounds_non_multiple() {
    let mut cfg = ScreenshotConfig::default();
    cfg.interval_secs = 7; // Rounds to nearest epoch (5s) → 5 or 10
    let eff = cfg.effective_interval_secs();
    assert!(eff % 5 == 0, "effective interval should be a multiple of 5, got {eff}");
}

#[test]
fn effective_interval_clamps_zero() {
    let mut cfg = ScreenshotConfig::default();
    cfg.interval_secs = 0;
    // Should clamp to minimum (1× epoch = 5s)
    assert!(cfg.effective_interval_secs() >= 5);
}

#[test]
fn effective_interval_clamps_huge() {
    let mut cfg = ScreenshotConfig::default();
    cfg.interval_secs = 999;
    // Should clamp to maximum (12× epoch = 60s)
    assert!(cfg.effective_interval_secs() <= 60);
}

// ── interval_multiplier ──────────────────────────────────────────────────────

#[test]
fn multiplier_at_min() {
    let mut cfg = ScreenshotConfig::default();
    cfg.interval_secs = 1;
    assert!(cfg.interval_multiplier() >= 1);
}

#[test]
fn multiplier_at_max() {
    let mut cfg = ScreenshotConfig::default();
    cfg.interval_secs = 60;
    assert!(cfg.interval_multiplier() <= 12);
}

// ── model_id ─────────────────────────────────────────────────────────────────

#[test]
fn model_id_fastembed_default() {
    let cfg = ScreenshotConfig::default();
    assert!(cfg.model_id().contains("clip"));
}

#[test]
fn model_id_mmproj() {
    let mut cfg = ScreenshotConfig::default();
    cfg.embed_backend = "mmproj".into();
    assert_eq!(cfg.model_id(), "mmproj");
}

#[test]
fn model_id_llm_vlm() {
    let mut cfg = ScreenshotConfig::default();
    cfg.embed_backend = "llm-vlm".into();
    assert_eq!(cfg.model_id(), "llm-vlm");
}

// ── Serde round-trip ─────────────────────────────────────────────────────────

#[test]
fn serde_roundtrip_default() {
    let cfg = ScreenshotConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let restored: ScreenshotConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg.interval_secs, restored.interval_secs);
    assert_eq!(cfg.quality, restored.quality);
    assert_eq!(cfg.enabled, restored.enabled);
}

#[test]
fn serde_roundtrip_custom() {
    let mut cfg = ScreenshotConfig::default();
    cfg.enabled = true;
    cfg.interval_secs = 30;
    cfg.quality = 90;
    cfg.gif_enabled = true;
    cfg.embed_backend = "mmproj".into();

    let json = serde_json::to_string(&cfg).unwrap();
    let restored: ScreenshotConfig = serde_json::from_str(&json).unwrap();
    assert!(restored.enabled);
    assert_eq!(restored.interval_secs, 30);
    assert_eq!(restored.quality, 90);
    assert!(restored.gif_enabled);
    assert_eq!(restored.embed_backend, "mmproj");
}

#[test]
fn serde_missing_fields_use_defaults() {
    let json = r#"{"enabled": true}"#;
    let cfg: ScreenshotConfig = serde_json::from_str(json).unwrap();
    assert!(cfg.enabled);
    assert_eq!(cfg.interval_secs, 5);
    assert_eq!(cfg.quality, 60);
    assert!(cfg.ocr_enabled);
}
