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
