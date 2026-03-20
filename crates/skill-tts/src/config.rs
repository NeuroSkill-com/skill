// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! TTS configuration types.

use serde::{Deserialize, Serialize};

/// NeuTTS configuration — persisted in `~/.skill/settings.json` under `neutts`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct NeuttsConfig {
    /// Use NeuTTS instead of KittenTTS for all speech synthesis.
    pub enabled: bool,

    /// HuggingFace backbone repo, e.g. `"neuphonic/neutts-nano-q4-gguf"`.
    #[serde(default = "default_neutts_backbone_repo")]
    pub backbone_repo: String,

    /// Specific GGUF filename within the repo.
    /// Empty string means "auto-select the first `.gguf` file found".
    pub gguf_file: String,

    /// Absolute path to a reference WAV file used for voice cloning.
    pub ref_wav_path: String,

    /// Verbatim transcript of the speech in `ref_wav_path`.
    pub ref_text: String,

    /// Name of a bundled preset voice from `neutts-rs/samples/`.
    pub voice_preset: String,
}

pub fn default_neutts_backbone_repo() -> String {
    "neuphonic/neutts-nano-q4-gguf".into()
}

impl Default for NeuttsConfig {
    fn default() -> Self {
        Self {
            enabled:       false,
            backbone_repo: default_neutts_backbone_repo(),
            gguf_file:     String::new(),
            voice_preset:  "jo".into(),
            ref_wav_path:  String::new(),
            ref_text:      String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_disabled() {
        let cfg = NeuttsConfig::default();
        assert!(!cfg.enabled);
    }

    #[test]
    fn default_voice_preset_is_jo() {
        assert_eq!(NeuttsConfig::default().voice_preset, "jo");
    }

    #[test]
    fn default_backbone_repo() {
        assert!(NeuttsConfig::default().backbone_repo.contains("neutts"));
    }

    #[test]
    fn json_round_trip() {
        let cfg = NeuttsConfig {
            enabled: true,
            voice_preset: "dave".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: NeuttsConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.voice_preset, "dave");
    }

    #[test]
    fn deserialize_empty_json_gives_defaults() {
        let cfg: NeuttsConfig = serde_json::from_str("{}").unwrap();
        assert!(!cfg.enabled);
        assert_eq!(cfg.voice_preset, "jo");
    }
}
