// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! TTS module — thin adapter layer over the `skill-tts` crate.
//!
//! The core TTS engine (workers, synthesis, audio, config) lives in the
//! `skill-tts` workspace crate.  This module re-exports the public API and
//! provides `#[tauri::command]` wrappers that depend on `tauri::AppHandle`.

// ── Re-exports from skill-tts ─────────────────────────────────────────────────

#[allow(unused_imports)]
pub use skill_tts::{
    NeuttsConfig, NeuttsVoiceInfo, TtsProgressEvent, TTS_PROGRESS_EVENT,
    init_tts_dirs, init_neutts_samples_dir, init_espeak_bundled_data_path,
    set_logging, tts_shutdown, neutts_apply_config, use_neutts,
};

// ── Tauri commands ────────────────────────────────────────────────────────────

use tauri::{AppHandle, Emitter};

/// Initialise (or reinitialise) the active TTS backend.
#[tauri::command]
pub async fn tts_init(app_handle: AppHandle) -> Result<(), String> {
    let app = app_handle.clone();
    let emit = move |ev: TtsProgressEvent| {
        app.emit(TTS_PROGRESS_EVENT, ev).ok();
    };
    skill_tts::tts_init_with_callback(emit).await
}

/// Unload the active TTS backend, freeing memory.
#[tauri::command]
pub async fn tts_unload(app_handle: AppHandle) -> Result<(), String> {
    let result = skill_tts::tts_unload().await;
    if result.is_ok() {
        app_handle.emit(TTS_PROGRESS_EVENT, TtsProgressEvent::unloaded()).ok();
    }
    result
}

/// Speak `text` aloud.
#[tauri::command]
pub async fn tts_speak(text: String, voice: Option<String>) {
    skill_tts::tts_speak(text, voice).await;
}

/// Return all available voice names for the active backend.
#[tauri::command]
pub async fn tts_list_voices() -> Vec<String> {
    skill_tts::tts_list_voices()
}

/// Return structured metadata for every NeuTTS preset voice.
#[tauri::command]
pub async fn tts_list_neutts_voices() -> Vec<NeuttsVoiceInfo> {
    skill_tts::tts_list_neutts_voices()
}

/// Return the currently active voice name.
#[tauri::command]
pub async fn tts_get_voice() -> String {
    skill_tts::tts_get_voice()
}

/// Set the active voice name.
#[tauri::command]
pub async fn tts_set_voice(voice: String) {
    skill_tts::tts_set_voice(voice);
}
