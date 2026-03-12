// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! TTS subsystem — KittenTTS and/or NeuTTS backends.
//!
//! Back-ends are gated by feature flags:
//!   `tts-kitten`  → KittenTTS (small ONNX, English only)
//!   `tts-neutts`  → NeuTTS (GGUF, multilingual, voice-cloning)
//!
//! Both can be active simultaneously; `use_neutts()` decides which back-end
//! handles a given call at runtime.

#[cfg(feature = "tts-kitten")]
mod kitten;

#[cfg(feature = "tts-neutts")]
mod neutts;

#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
use std::num::NonZero;
use std::path::PathBuf;
use std::sync::{OnceLock, atomic::{AtomicBool, Ordering}};

use tauri::{AppHandle, Emitter};

// ─── SKILL_DIR ────────────────────────────────────────────────────────────────

static SKILL_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Initialise the `SKILL_DIR` static and pre-create every sub-directory
/// that the TTS subsystem writes files into.
pub(crate) fn init_tts_dirs(dir: &std::path::Path) {
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
pub(crate) fn init_neutts_samples_dir(path: PathBuf) {
    #[cfg(feature = "tts-neutts")]
    neutts::set_samples_dir(path);
    #[cfg(not(feature = "tts-neutts"))]
    let _ = path;
}

/// Return the resolved skill directory.
///
/// Uses the value set by [`init_tts_dirs`] (which is always called from
/// `lib.rs::setup()` before TTS is used).  The fallback delegates to
/// [`crate::settings::default_skill_dir`] so that Windows gets
/// `%LOCALAPPDATA%\NeuroSkill` rather than a relative `.skill` path or a
/// misplaced `~/.skill` (which `$HOME` / `dirs::home_dir()` can produce
/// incorrectly on Windows when `USERPROFILE` is set but `HOME` is not).
pub(super) fn skill_dir() -> PathBuf {
    SKILL_DIR
        .get()
        .cloned()
        .unwrap_or_else(crate::settings::default_skill_dir)
}

// ─── Logging ──────────────────────────────────────────────────────────────────

static TTS_LOGGING: AtomicBool = AtomicBool::new(false);

#[allow(dead_code)]
pub fn set_logging(enable: bool) { TTS_LOGGING.store(enable, Ordering::Relaxed); }

#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
pub(super) fn tts_log(msg: &str) {
    if TTS_LOGGING.load(Ordering::Relaxed) {
        eprintln!("[tts] {msg}");
    }
}

// ─── Shared constants ─────────────────────────────────────────────────────────

#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
pub(super) const TAIL_SILENCE_SECS: f32 = 0.25;

// ─── Progress event ───────────────────────────────────────────────────────────

/// Payload for the `"tts-progress"` Tauri event.
///
/// Frontend shape (TypeScript):
/// ```ts
/// type TtsProgress = { phase: "step" | "ready" | "unloaded"; step: number; total: number; label: string };
/// ```
#[derive(Clone, serde::Serialize)]
pub struct TtsProgressEvent {
    pub phase: String,
    pub step:  u32,
    pub total: u32,
    pub label: String,
}

pub(crate) const TTS_PROGRESS_EVENT: &str = "tts-progress";

#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
impl TtsProgressEvent {
    /// A mid-load progress step.
    pub(super) fn step(step: u32, total: u32, label: String) -> Self {
        Self { phase: "step".into(), step, total, label }
    }
    /// Loading finished successfully.
    pub(super) fn ready(total: u32) -> Self {
        Self { phase: "ready".into(), step: total, total, label: String::new() }
    }
    /// Backend was unloaded.
    pub(super) fn unloaded() -> Self {
        Self { phase: "unloaded".into(), step: 0, total: 0, label: String::new() }
    }
    /// Loading failed — `label` contains the human-readable error message.
    pub(super) fn error(label: String) -> Self {
        Self { phase: "error".into(), step: 0, total: 0, label }
    }
}

// ─── espeak-ng data path ──────────────────────────────────────────────────────

/// Called from **`lib.rs` setup()** — before any worker thread starts — with the
/// Tauri resource directory.  Sets the espeak-ng data path to the directory
/// that was bundled at build time (`{resource_dir}/espeak-ng-data`).
///
/// Because both kittentts and neutts use a `OnceCell` for the data path, the
/// **first** call wins.  Calling this from `setup()` ensures the correct bundle
/// path is locked in before `init_espeak_data_path()` runs on a worker thread.
pub(crate) fn init_espeak_bundled_data_path(resource_dir: &std::path::Path) {
    let data_path = resource_dir.join("espeak-ng-data");
    if data_path.is_dir() {
        #[cfg(feature = "tts-kitten")]
        kittentts::phonemize::set_data_path(&data_path);
        // Use `::neutts::` (crate root) to avoid ambiguity with the `neutts` submodule.
        #[cfg(feature = "tts-neutts")]
        ::neutts::phonemize::set_data_path(&data_path);
    }
    // If the bundle path doesn't exist (e.g. first-run dev build before the
    // first tauri bundle), init_espeak_data_path() on the worker will try the
    // ESPEAK_DATA_PATH_DEV fallback below.
}

/// Called from **worker-thread startup** (neutts and kittentts workers).
///
/// Resolves the espeak-ng data directory in priority order and calls
/// `set_data_path()`.  Because `set_data_path()` uses a `OnceCell`, it is a
/// no-op if `init_espeak_bundled_data_path()` already ran from `setup()`.
///
/// Priority:
///   1. `ESPEAK_DATA_PATH` env var           — explicit runtime override
///   2. `ESPEAK_DATA_PATH_DEV`               — absolute path baked in by
///      `build.rs` pointing at the
///      `espeak-static/share/espeak-ng-data`
///      directory on the *build* machine.
///      Used for plain `cargo run` builds.
///   3. espeak-ng compiled-in path           — NULL → espeak-ng uses its own
///      system path as a last resort.
///
/// We only call `set_data_path()` when the path actually exists on disk so we
/// never hand a non-existent path to espeak, which would permanently poison the
/// `OnceCell` init result and break all phonemisation.
#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
pub(super) fn init_espeak_data_path() {
    let explicit   = std::env::var("ESPEAK_DATA_PATH").ok();
    let dev_baked  = option_env!("ESPEAK_DATA_PATH_DEV"); // absolute path from build.rs

    let resolved = explicit
        .as_deref()
        .into_iter()
        .chain(dev_baked)
        .find(|p| std::path::Path::new(p).is_dir());

    if let Some(dir) = resolved {
        let data_path = std::path::Path::new(dir);
        #[cfg(feature = "tts-kitten")]
        kittentts::phonemize::set_data_path(data_path);
        // Use `::neutts::` (crate root) to avoid ambiguity with the `neutts` submodule.
        #[cfg(feature = "tts-neutts")]
        ::neutts::phonemize::set_data_path(data_path);
    }
    // else: leave DATA_PATH unset; espeak-ng falls through to its compiled-in
    // system path (works when a system espeak-ng is installed, or when the
    // static lib was built with a matching CMAKE_INSTALL_PREFIX).
}

// ─── Shared audio output ──────────────────────────────────────────────────────

#[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
pub(super) fn play_f32_audio(
    stream:      &rodio::MixerDeviceSink,
    mut samples: Vec<f32>,
    sample_rate: u32,
) {
    use rodio::buffer::SamplesBuffer;

    // Append tail silence so the last syllable is not cut off.
    let silence_samples = (TAIL_SILENCE_SECS * sample_rate as f32) as usize;
    samples.resize(samples.len() + silence_samples, 0.0f32);

    let channels = NonZero::<u16>::new(1).unwrap();
    let rate     = NonZero::<u32>::new(sample_rate.max(1)).unwrap();
    let buf      = SamplesBuffer::new(channels, rate, samples);

    // `rodio::play` expects Read+Seek (file decoder); for raw samples use
    // `Player::connect_new` which accepts any `Source`.
    let player = rodio::Player::connect_new(stream.mixer());
    player.append(buf);
    player.sleep_until_end();
}

// ─── Back-end routing ─────────────────────────────────────────────────────────

// When BOTH features are compiled we use a runtime flag.
// When only one feature is compiled we return a compile-time constant so the
// compiler can dead-strip the unused branch.

#[cfg(all(feature = "tts-kitten", feature = "tts-neutts"))]
pub(super) static NEUTTS_ENABLED: AtomicBool = AtomicBool::new(false);

#[cfg(all(feature = "tts-kitten", feature = "tts-neutts"))]
fn use_neutts() -> bool { NEUTTS_ENABLED.load(Ordering::Relaxed) }

#[cfg(all(feature = "tts-neutts", not(feature = "tts-kitten")))]
fn use_neutts() -> bool { true }

#[cfg(all(feature = "tts-kitten", not(feature = "tts-neutts")))]
fn use_neutts() -> bool { false }

#[cfg(not(any(feature = "tts-kitten", feature = "tts-neutts")))]
fn use_neutts() -> bool { false }

// ─── Public config entry-point ────────────────────────────────────────────────

/// Synchronously drop all TTS backends before process exit.
///
/// Must be called from Tauri's `RunEvent::Exit` handler (non-async context).
/// Waits up to 8 s for each backend to release its resources; this prevents
/// the llama.cpp/Metal `ggml_metal_device_free` assertion from firing during
/// C++ static destructors after `exit()`.
pub(crate) fn tts_shutdown() {
    #[cfg(feature = "tts-neutts")]
    {
        let timeout = std::time::Duration::from_secs(8);
        let (tx, rx) = std::sync::mpsc::sync_channel::<()>(0);
        if neutts::try_shutdown(tx) && rx.recv_timeout(timeout).is_err() {
            eprintln!("[neutts] shutdown timed out — forcing drop");
        }
    }
}

/// Apply new NeuTTS configuration (called from `settings_cmds`).
pub fn neutts_apply_config(cfg: &crate::settings::NeuttsConfig) {
    #[cfg(feature = "tts-neutts")]
    neutts::apply_config(cfg);
    #[cfg(not(feature = "tts-neutts"))]
    let _ = cfg;
}

// ─── List NeuTTS voices ───────────────────────────────────────────────────────

/// Metadata for a single NeuTTS preset voice.
#[derive(Clone, serde::Serialize)]
pub struct NeuttsVoiceInfo {
    pub id:     String,
    pub lang:   String,
    pub flag:   String,
    pub gender: String,
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

/// Initialise (or reinitialise) the active TTS backend.
///
/// Progress events are emitted to the `"tts_progress"` channel.  The frontend
/// can listen via `appWindow.listen("tts_progress", …)`.
#[tauri::command]
pub async fn tts_init(app_handle: AppHandle) -> Result<(), String> {
    #[cfg(any(feature = "tts-kitten", feature = "tts-neutts"))]
    let emit = {
        let app = app_handle.clone();
        move |ev: TtsProgressEvent| {
            app.emit(TTS_PROGRESS_EVENT, ev).ok();
        }
    };
    #[cfg(not(any(feature = "tts-kitten", feature = "tts-neutts")))]
    let _ = &app_handle;

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

/// Unload the active TTS backend, freeing memory.
#[tauri::command]
pub async fn tts_unload(app_handle: AppHandle) -> Result<(), String> {
    let _ = &app_handle;
    if use_neutts() {
        #[cfg(feature = "tts-neutts")]
        {
            let (tx, rx) = tokio::sync::oneshot::channel();
            neutts::get_tx().send(neutts::Cmd::Unload { done: tx })
                .map_err(|e| format!("neutts unload channel send: {e}"))?;
            rx.await.map_err(|e| format!("neutts unload channel recv: {e}"))?;
            app_handle.emit(TTS_PROGRESS_EVENT, TtsProgressEvent::unloaded()).ok();
            return Ok(());
        }
    } else {
        #[cfg(feature = "tts-kitten")]
        {
            let (tx, rx) = tokio::sync::oneshot::channel();
            kitten::get_tx().send(kitten::Cmd::Unload { done: tx })
                .map_err(|e| format!("kitten unload channel send: {e}"))?;
            rx.await.map_err(|e| format!("kitten unload channel recv: {e}"))?;
            app_handle.emit(TTS_PROGRESS_EVENT, TtsProgressEvent::unloaded()).ok();
            return Ok(());
        }
    }

    #[allow(unreachable_code)]
    Err("no TTS backend compiled".into())
}

/// Speak `text` aloud.
///
/// `voice` is optional:
/// - KittenTTS: used as voice name (falls back to the stored active voice)
/// - NeuTTS:    used as per-utterance preset-voice override
///
/// Signature is exactly two arguments to stay compatible with `ws_commands.rs`:
/// ```rust
/// tokio::spawn(async move { crate::tts::tts_speak(text, voice).await });
/// ```
#[tauri::command]
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
/// For KittenTTS these are discovered from the model; for NeuTTS the preset list.
#[tauri::command]
pub async fn tts_list_voices() -> Vec<String> {
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
#[tauri::command]
pub async fn tts_list_neutts_voices() -> Vec<NeuttsVoiceInfo> {
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

/// Return the currently active voice name (KittenTTS) or preset (NeuTTS).
#[tauri::command]
pub async fn tts_get_voice() -> String {
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

/// Set the active voice name (KittenTTS) or preset (NeuTTS).
#[tauri::command]
pub async fn tts_set_voice(voice: String) {
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
