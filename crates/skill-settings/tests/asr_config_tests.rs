#![allow(clippy::unwrap_used)]
// SPDX-License-Identifier: GPL-3.0-only
//! Tests for `AsrConfig` defaults + serde, and `UserSettings` back-compat when
//! the `asr` block is absent or partial.

use skill_asr::{RoutingMode, TriggerMode};
use skill_settings::{AsrConfig, UserSettings};

#[test]
fn asr_config_defaults() {
    let c = AsrConfig::default();
    assert!(c.enabled);
    assert_eq!(c.default_trigger, TriggerMode::Continuous);
    assert_eq!(c.default_routing, RoutingMode::VoiceLoop);
    assert_eq!(c.language, "en");
    assert_eq!(c.engine, "whisper");
    assert_eq!(c.model, "openai/whisper-base.en");
}

#[test]
fn asr_config_serde_roundtrip() {
    let c = AsrConfig {
        enabled: false,
        default_trigger: TriggerMode::PushToTalk,
        default_routing: RoutingMode::TranscribeOnly,
        language: "es".into(),
        engine: "whisper".into(),
        model: "openai/whisper-small".into(),
    };
    let json = serde_json::to_string(&c).unwrap();
    let back: AsrConfig = serde_json::from_str(&json).unwrap();
    assert!(!back.enabled);
    assert_eq!(back.default_trigger, TriggerMode::PushToTalk);
    assert_eq!(back.default_routing, RoutingMode::TranscribeOnly);
    assert_eq!(back.language, "es");
    assert_eq!(back.engine, "whisper");
    assert_eq!(back.model, "openai/whisper-small");
}

#[test]
fn asr_config_partial_json_defaults_missing_fields() {
    // `#[serde(default)]` on the struct → missing fields fall back to defaults,
    // so the chat window can persist just the field it changed.
    let c: AsrConfig = serde_json::from_str(r#"{"default_trigger":"push_to_talk"}"#).unwrap();
    assert_eq!(c.default_trigger, TriggerMode::PushToTalk);
    assert_eq!(c.default_routing, RoutingMode::VoiceLoop);
    assert_eq!(c.language, "en");
    assert!(c.enabled);
}

#[test]
fn user_settings_without_asr_block_loads_default() {
    // A settings.json written before the ASR feature existed must still load.
    let s: UserSettings = serde_json::from_str("{}").unwrap();
    assert_eq!(s.asr.default_trigger, TriggerMode::Continuous);
    assert_eq!(s.asr.default_routing, RoutingMode::VoiceLoop);
    assert!(s.asr.enabled);
}
