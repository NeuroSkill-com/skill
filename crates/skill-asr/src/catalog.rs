// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! ASR engine catalog for the UI / daemon.

use serde::Serialize;

/// One selectable ASR backend.
#[derive(Debug, Clone, Serialize)]
pub struct AsrEngineInfo {
    pub id: String,
    pub label: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub experimental: bool,
    pub downloadable: bool,
    /// False when this build cannot run ASR (Windows / feature off).
    pub available: bool,
}

fn asr_runtime_available() -> bool {
    cfg!(asr_active)
}

/// Normalize legacy / alias engine ids.
pub fn normalize_engine_id(engine: &str) -> String {
    match engine.trim().to_ascii_lowercase().as_str() {
        "qwen3_asr" => "qwen3-asr".into(),
        "sensevoice" | "paraformer" => "funasr".into(),
        "nemotron" => "nemotron-asr".into(),
        "rlxasr" | "rlx_asr" => "rlx-asr".into(),
        other => other.to_string(),
    }
}

/// Whether `engine` is a known catalog id.
pub fn is_known_engine(engine: &str) -> bool {
    let id = normalize_engine_id(engine);
    list_engines().iter().any(|e| e.id == id)
}

/// Full ASR catalog. SenseVoice is the FunASR default (Paraformer optional).
pub fn list_engines() -> Vec<AsrEngineInfo> {
    let avail = asr_runtime_available();
    vec![
        AsrEngineInfo {
            id: "whisper".into(),
            label: "Whisper".into(),
            models: vec![
                "openai/whisper-tiny.en".into(),
                "openai/whisper-base.en".into(),
                "openai/whisper-small.en".into(),
                "openai/whisper-small".into(),
                "openai/whisper-medium".into(),
                "openai/whisper-large-v3".into(),
            ],
            default_model: skill_constants::WHISPER_ASR_HF_REPO.to_string(),
            experimental: false,
            downloadable: true,
            available: avail,
        },
        AsrEngineInfo {
            id: "qwen3-asr".into(),
            label: "Qwen3-ASR".into(),
            models: vec!["Qwen/Qwen3-ASR-0.6B".into(), "Qwen/Qwen3-ASR-1.7B".into()],
            default_model: "Qwen/Qwen3-ASR-0.6B".into(),
            experimental: false,
            downloadable: true,
            available: avail,
        },
        AsrEngineInfo {
            id: "voxtral".into(),
            label: "Voxtral".into(),
            models: vec!["mistralai/Voxtral-Mini-3B-2507".into()],
            default_model: "mistralai/Voxtral-Mini-3B-2507".into(),
            experimental: true,
            downloadable: true,
            available: avail,
        },
        AsrEngineInfo {
            id: "funasr".into(),
            label: "FunASR".into(),
            // SenseVoice is the supported default; Paraformer-zh remains selectable.
            models: vec!["FunAudioLLM/SenseVoiceSmall".into(), "funasr/paraformer-zh".into()],
            default_model: "FunAudioLLM/SenseVoiceSmall".into(),
            experimental: false,
            downloadable: true,
            available: avail,
        },
        AsrEngineInfo {
            id: "nemotron-asr".into(),
            label: "Nemotron-ASR".into(),
            models: vec!["nvidia/nemotron-3.5-asr-streaming-0.6b".into()],
            default_model: "nvidia/nemotron-3.5-asr-streaming-0.6b".into(),
            experimental: true,
            downloadable: true,
            available: avail,
        },
        AsrEngineInfo {
            id: "rlx-asr".into(),
            label: "RLX-ASR".into(),
            models: vec!["eugenehp/rlx-asr".into()],
            default_model: "eugenehp/rlx-asr".into(),
            experimental: true,
            downloadable: true,
            available: avail,
        },
    ]
}

/// Whether any ASR engine can run in this binary.
pub fn voice_asr_available() -> bool {
    asr_runtime_available()
}
