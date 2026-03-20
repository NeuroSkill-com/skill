// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! `skill-tts` — TTS engine extracted from the NeuroSkill monolith.
//!
//! This crate contains:
//!
//! - **config** — `NeuttsConfig`
//! - **kitten** — KittenTTS backend (feature `tts-kitten`)
//! - **neutts** — NeuTTS backend (feature `tts-neutts`)
//! - Core TTS logic: audio output, espeak init, backend routing, progress events

pub mod config;
pub mod log;

/// Log a message from the TTS subsystem.
///
/// ```ignore
/// tts_log!("tts", "KittenTTS ready (voices={voices:?})");
/// tts_log!("neutts", "backbone ready (repo={repo})");
/// ```
///
/// Short-circuits (no `format!` allocation) when logging is disabled.
#[allow(unused_macros)]
macro_rules! tts_log {
    ($tag:expr, $($arg:tt)*) => {
        if $crate::log::log_enabled() {
            $crate::log::write_log($tag, &format!($($arg)*));
        }
    };
}

#[cfg(feature = "tts-kitten")]
pub mod kitten;

#[cfg(feature = "tts-neutts")]
pub mod neutts;

#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
use std::num::NonZero;
use std::path::PathBuf;
use std::sync::OnceLock;
#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
use std::sync::atomic::Ordering;
#[cfg(all(feature = "tts-kitten", feature = "tts-neutts"))]
use std::sync::atomic::AtomicBool;

pub use config::NeuttsConfig;

// ─── SKILL_DIR ────────────────────────────────────────────────────────────────

static SKILL_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Initialise the `SKILL_DIR` static and pre-create every sub-directory
/// that the TTS subsystem writes files into.
pub fn init_tts_dirs(dir: &std::path::Path) {
    let _ = SKILL_DIR.set(dir.to_path_buf());
    for sub in &[
        "models/neutts",
        "cache/neutts-wav",
        "cache/neutts-ref-codes",
    ] {
        let _ = std::fs::create_dir_all(skill_dir().join(sub));
    }
}

/// Store the runtime path to bundled NeuTTS voice-preset sample files.
/// Called from `lib.rs` setup once the Tauri resource dir is known.
pub fn init_neutts_samples_dir(path: PathBuf) {
    #[cfg(feature = "tts-neutts")]
    neutts::set_samples_dir(path);
    #[cfg(not(feature = "tts-neutts"))]
    let _ = path;
}

/// Return the resolved skill directory.
pub fn skill_dir() -> PathBuf {
    SKILL_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| {
            // Fallback: use platform-appropriate default
            dirs::data_local_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
                .join("NeuroSkill")
        })
}

/// Set the skill directory explicitly (alternative to `init_tts_dirs`).
pub fn set_skill_dir(dir: PathBuf) {
    let _ = SKILL_DIR.set(dir);
}

// ─── Logging ──────────────────────────────────────────────────────────────────
//
// See `log` module for the full API.  The `tts_log!` macro is re-exported
// at crate root by `#[macro_export]`.  Legacy `set_logging` delegates to
// `log::set_log_enabled` for backwards compatibility.

/// Enable or disable TTS log output (backwards-compatible wrapper).
pub fn set_logging(enable: bool) { log::set_log_enabled(enable); }

// ─── Shared constants ─────────────────────────────────────────────────────────

#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
pub use skill_constants::TTS_TAIL_SILENCE_SECS as TAIL_SILENCE_SECS;

// ─── Progress event ───────────────────────────────────────────────────────────

/// Payload for the `"tts-progress"` event.
#[derive(Clone, serde::Serialize)]
pub struct TtsProgressEvent {
    pub phase: String,
    pub step:  u32,
    pub total: u32,
    pub label: String,
}

pub use skill_constants::TTS_PROGRESS_EVENT;

#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
impl TtsProgressEvent {
    pub fn step(step: u32, total: u32, label: String) -> Self {
        Self { phase: "step".into(), step, total, label }
    }
    pub fn ready(total: u32) -> Self {
        Self { phase: "ready".into(), step: total, total, label: String::new() }
    }
    pub fn unloaded() -> Self {
        Self { phase: "unloaded".into(), step: 0, total: 0, label: String::new() }
    }
    pub fn error(label: String) -> Self {
        Self { phase: "error".into(), step: 0, total: 0, label }
    }
}

// ─── espeak-ng data path ──────────────────────────────────────────────────────

/// Set espeak-ng data path from the bundled resource directory.
pub fn init_espeak_bundled_data_path(resource_dir: &std::path::Path) {
    let data_path = resource_dir.join("espeak-ng-data");
    if data_path.is_dir() {
        #[cfg(feature = "tts-kitten")]
        kittentts::phonemize::set_data_path(&data_path);
        #[cfg(feature = "tts-neutts")]
        ::neutts::phonemize::set_data_path(&data_path);
    }
}

/// Resolve espeak-ng data path from environment or build-time baked path.
#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
pub fn init_espeak_data_path() {
    let explicit   = std::env::var("ESPEAK_DATA_PATH").ok();
    let dev_baked  = option_env!("ESPEAK_DATA_PATH_DEV");

    let resolved = explicit
        .as_deref()
        .into_iter()
        .chain(dev_baked)
        .find(|p| std::path::Path::new(p).is_dir());

    if let Some(dir) = resolved {
        let data_path = std::path::Path::new(dir);
        #[cfg(feature = "tts-kitten")]
        kittentts::phonemize::set_data_path(data_path);
        #[cfg(feature = "tts-neutts")]
        ::neutts::phonemize::set_data_path(data_path);
    }
}

// ─── Shared audio output ──────────────────────────────────────────────────────

#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
pub fn play_f32_audio(
    stream:      &rodio::MixerDeviceSink,
    mut samples: Vec<f32>,
    sample_rate: u32,
) {
    use rodio::buffer::SamplesBuffer;

    let silence_samples = (TAIL_SILENCE_SECS * sample_rate as f32) as usize;
    samples.resize(samples.len() + silence_samples, 0.0f32);

    let channels = NonZero::<u16>::new(1).expect("1 is non-zero");
    let rate     = NonZero::<u32>::new(sample_rate.max(1)).expect("max(1) is non-zero");
    let buf      = SamplesBuffer::new(channels, rate, samples);

    let player = rodio::Player::connect_new(stream.mixer());
    player.append(buf);
    player.sleep_until_end();
}

// ─── Back-end routing ─────────────────────────────────────────────────────────

#[cfg(all(feature = "tts-kitten", feature = "tts-neutts"))]
pub static NEUTTS_ENABLED: AtomicBool = AtomicBool::new(false);

#[cfg(all(feature = "tts-kitten", feature = "tts-neutts"))]
pub fn use_neutts() -> bool { NEUTTS_ENABLED.load(Ordering::Relaxed) }

#[cfg(all(feature = "tts-neutts", not(feature = "tts-kitten")))]
pub fn use_neutts() -> bool { true }

#[cfg(all(feature = "tts-kitten", not(feature = "tts-neutts")))]
pub fn use_neutts() -> bool { false }

#[cfg(not(any(feature = "tts-kitten", feature = "tts-neutts")))]
pub fn use_neutts() -> bool { false }

// ─── Shutdown ─────────────────────────────────────────────────────────────────

/// Synchronously drop all TTS backends before process exit.
pub fn tts_shutdown() {
    #[cfg(feature = "tts-kitten")]
    {
        let timeout = std::time::Duration::from_secs(8);
        let (tx, rx) = std::sync::mpsc::sync_channel::<()>(0);
        if kitten::try_shutdown(tx) && rx.recv_timeout(timeout).is_err() {
            tts_log!("tts", "KittenTTS shutdown timed out \u{2014} forcing drop");
        }
    }

    #[cfg(feature = "tts-neutts")]
    {
        let timeout = std::time::Duration::from_secs(8);
        let (tx, rx) = std::sync::mpsc::sync_channel::<()>(0);
        if neutts::try_shutdown(tx) && rx.recv_timeout(timeout).is_err() {
            tts_log!("neutts", "shutdown timed out \u{2014} forcing drop");
        }
    }
}

/// Apply new NeuTTS configuration.
pub fn neutts_apply_config(cfg: &NeuttsConfig) {
    #[cfg(feature = "tts-neutts")]
    neutts::apply_config(cfg);
    #[cfg(not(feature = "tts-neutts"))]
    let _ = cfg;
}

// ─── Metadata ─────────────────────────────────────────────────────────────────

/// Metadata for a single NeuTTS preset voice.
#[derive(Clone, serde::Serialize)]
pub struct NeuttsVoiceInfo {
    pub id:     String,
    pub lang:   String,
    pub flag:   String,
    pub gender: String,
}

// ─── Public API: speak, list voices, get/set voice ────────────────────────────

/// Speak `text` aloud using the active TTS backend.
pub async fn tts_speak(text: String, voice: Option<String>) {
    let voice_str = voice.unwrap_or_default();
    #[cfg(not(any(feature = "tts-kitten", feature = "tts-neutts")))]
    { let _ = (&text, &voice_str); }

    if use_neutts() {
        #[cfg(feature = "tts-neutts")]
        {
            let override_voice = if voice_str.is_empty() || voice_str == "default" {
                None
            } else {
                Some(voice_str)
            };
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = neutts::get_tx().send(neutts::Cmd::Speak {
                text,
                voice_override: override_voice,
                done: tx,
            });
            let _ = rx.await;
        }
    } else {
        #[cfg(feature = "tts-kitten")]
        {
            let resolved_voice = if voice_str.is_empty() || voice_str == "default" {
                kitten::get_voice()
            } else {
                voice_str
            };
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = kitten::get_tx().send(kitten::Cmd::Speak {
                text, voice: resolved_voice, done: tx,
            });
            let _ = rx.await;
        }
    }
}

/// Return all available voice names for the active backend.
pub fn tts_list_voices() -> Vec<String> {
    if use_neutts() {
        #[cfg(feature = "tts-neutts")]
        return neutts::PRESET_NAMES.iter().map(|s| s.to_string()).collect();
    } else {
        #[cfg(feature = "tts-kitten")]
        return kitten::AVAILABLE_VOICES
            .get()
            .cloned()
            .unwrap_or_else(|| vec![kitten::VOICE_DEFAULT.to_string()]);
    }
    #[allow(unreachable_code)]
    Vec::new()
}

/// Return structured metadata for every NeuTTS preset voice.
pub fn tts_list_neutts_voices() -> Vec<NeuttsVoiceInfo> {
    #[cfg(feature = "tts-neutts")]
    return vec![
        NeuttsVoiceInfo { id: "jo".into(),       lang: "en-US".into(), flag: "🇺🇸".into(), gender: "F".into() },
        NeuttsVoiceInfo { id: "dave".into(),     lang: "en-US".into(), flag: "🇺🇸".into(), gender: "M".into() },
        NeuttsVoiceInfo { id: "greta".into(),    lang: "de-DE".into(), flag: "🇩🇪".into(), gender: "F".into() },
        NeuttsVoiceInfo { id: "juliette".into(), lang: "fr-FR".into(), flag: "🇫🇷".into(), gender: "F".into() },
        NeuttsVoiceInfo { id: "mateo".into(),    lang: "es-ES".into(), flag: "🇪🇸".into(), gender: "M".into() },
    ];
    #[allow(unreachable_code)]
    Vec::new()
}

/// Return the currently active voice name.
pub fn tts_get_voice() -> String {
    if use_neutts() {
        #[cfg(feature = "tts-neutts")]
        {
            let (_, _, preset, _, _) = neutts::read_cfg();
            return preset;
        }
    } else {
        #[cfg(feature = "tts-kitten")]
        return kitten::get_voice();
    }
    #[allow(unreachable_code)]
    String::new()
}

/// Set the active voice name.
pub fn tts_set_voice(voice: String) {
    #[cfg(not(any(feature = "tts-kitten", feature = "tts-neutts")))]
    let _ = &voice;
    if use_neutts() {
        #[cfg(feature = "tts-neutts")]
        {
            if neutts::is_preset(&voice) {
                neutts::set_voice_preset(voice);
            }
        }
    } else {
        #[cfg(feature = "tts-kitten")]
        {
            let voices = kitten::AVAILABLE_VOICES.get().cloned()
                .unwrap_or_else(|| vec![kitten::VOICE_DEFAULT.to_string()]);
            if voices.iter().any(|v| v == &voice) || voice == kitten::VOICE_DEFAULT {
                kitten::set_voice(voice);
            }
        }
    }
}

/// Initialise the active TTS backend. Returns progress events via callback.
pub async fn tts_init_with_callback<F: Fn(TtsProgressEvent) + Clone + Send + 'static>(
    emit: F,
) -> Result<(), String> {
    if use_neutts() {
        #[cfg(feature = "tts-neutts")]
        {
            if neutts::READY.load(Ordering::Relaxed) {
                emit(TtsProgressEvent::ready(3));
                return Ok(());
            }
            if neutts::LOADING.load(Ordering::Relaxed) {
                return Err("NeuTTS is already loading".into());
            }
            let (backbone, gguf, preset, wav, text) = neutts::read_cfg();
            let (tx, rx) = tokio::sync::oneshot::channel();
            let emit_c = emit.clone();
            neutts::get_tx().send(neutts::Cmd::Init {
                backbone_repo: backbone,
                gguf_file:     gguf,
                voice_preset:  preset,
                ref_wav_path:  wav,
                ref_text:      text,
                cb:   Box::new(move |p| emit_c(neutts::progress_to_event(p))),
                done: tx,
            }).map_err(|e| format!("neutts init channel send: {e}"))?;
            let result = rx.await.map_err(|e| format!("neutts init channel recv: {e}"))
                .and_then(|r| r);
            match &result {
                Ok(_)    => emit(TtsProgressEvent::ready(3)),
                Err(msg) => emit(TtsProgressEvent::error(msg.clone())),
            }
            return result;
        }
    } else {
        #[cfg(feature = "tts-kitten")]
        {
            if kitten::LOADED.load(Ordering::Relaxed) {
                emit(TtsProgressEvent::ready(4));
                return Ok(());
            }
            let (tx, rx) = tokio::sync::oneshot::channel();
            let emit_c = emit.clone();
            kitten::get_tx().send(kitten::Cmd::Init {
                cb: Box::new(move |p| {
                    use kittentts::download::LoadProgress as KP;
                    let ev = match p {
                        KP::Fetching { step, total, file } => TtsProgressEvent::step(
                            step, total, file),
                        KP::Loading => TtsProgressEvent::step(4, 4, "Loading model…".into()),
                    };
                    emit_c(ev);
                }),
                done: tx,
            }).map_err(|e| format!("kitten init channel send: {e}"))?;
            let result = rx.await.map_err(|e| format!("kitten init channel recv: {e}"))
                .and_then(|r| r);
            match &result {
                Ok(_)    => emit(TtsProgressEvent::ready(4)),
                Err(msg) => emit(TtsProgressEvent::error(msg.clone())),
            }
            return result;
        }
    }
    #[allow(unreachable_code)]
    Err("no TTS backend compiled".into())
}

/// Unload the active TTS backend.
pub async fn tts_unload() -> Result<(), String> {
    if use_neutts() {
        #[cfg(feature = "tts-neutts")]
        {
            let (tx, rx) = tokio::sync::oneshot::channel();
            neutts::get_tx().send(neutts::Cmd::Unload { done: tx })
                .map_err(|e| format!("neutts unload channel send: {e}"))?;
            rx.await.map_err(|e| format!("neutts unload channel recv: {e}"))?;
            return Ok(());
        }
    } else {
        #[cfg(feature = "tts-kitten")]
        {
            let (tx, rx) = tokio::sync::oneshot::channel();
            kitten::get_tx().send(kitten::Cmd::Unload { done: tx })
                .map_err(|e| format!("kitten unload channel send: {e}"))?;
            rx.await.map_err(|e| format!("kitten unload channel recv: {e}"))?;
            return Ok(());
        }
    }
    #[allow(unreachable_code)]
    Err("no TTS backend compiled".into())
}
