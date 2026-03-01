// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Calibration TTS — lightweight ONNX-based speech synthesis via **kittentts**.
//!
//! ## Architecture
//!
//! All blocking work (model download, ONNX inference, audio playback) runs on a
//! single dedicated OS thread named `"skill-tts"`.  Tokio async code sends work
//! items through an `mpsc::sync_channel` and awaits a `tokio::sync::oneshot`
//! for the result.  This keeps the tokio thread-pool free and guarantees that
//! audio operations are never interrupted by the scheduler.
//!
//! ```text
//!  Tauri command (async)
//!       │  TtsCmd via SyncSender
//!       ▼
//!  ┌─────────────────────────────────────┐
//!  │  skill-tts thread                   │
//!  │  • owns KittenTTS model             │
//!  │  • owns rodio OutputStream          │
//!  │  • processes one cmd at a time      │
//!  └──────────────┬──────────────────────┘
//!                 │  oneshot reply
//!                 ▼
//!  Tauri command returns / emits event
//! ```
//!
//! ## Progress events
//!
//! `tts_init` emits `"tts-progress"` to every open window.
//! Payload: `{ phase:"step"|"ready", step, total, label }`.
//!
//! ## Silence padding
//!
//! 1 second of silence is appended to every utterance so audio drivers (CoreAudio,
//! PipeWire) drain the hardware buffer before the sink closes.
//!
//! ## Requirements
//!
//! * **espeak-ng** on `$PATH` — `brew install espeak-ng` / `apt install espeak-ng`
//! * Internet on first launch to fetch ~30 MB from HuggingFace Hub (cached).

use std::sync::{OnceLock, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};

use kittentts::{download::{self, LoadProgress}, KittenTTS};
use rodio::{buffer::SamplesBuffer, DeviceSinkBuilder, MixerDeviceSink, Player};
use tauri::Emitter;
use tokio::sync::oneshot;

// ─── Logging ─────────────────────────────────────────────────────────────────

/// Runtime-configurable flag.  Set via [`set_logging`] from `settings_cmds.rs`.
static TTS_LOGGING: AtomicBool = AtomicBool::new(false);

pub fn set_logging(enabled: bool) {
    TTS_LOGGING.store(enabled, Ordering::Relaxed);
}

#[inline]
fn tts_log(msg: &str) {
    if TTS_LOGGING.load(Ordering::Relaxed) {
        eprintln!("[tts] {msg}");
    }
}

// ─── Constants ────────────────────────────────────────────────────────────────

const HF_REPO:          &str = "KittenML/kitten-tts-mini-0.8";
const VOICE_DEFAULT:    &str = "Jasper";
const SPEED:            f32  = 1.0;
const SAMPLE_RATE:      u32  = kittentts::SAMPLE_RATE;
const TAIL_SILENCE_SECS: f32 = 1.0;

// ─── Shared state (populated once, read often) ────────────────────────────────

/// Voice names bundled in the loaded model.  Empty until first `tts_init`.
static AVAILABLE_VOICES: OnceLock<Vec<String>> = OnceLock::new();

/// Currently selected voice.  Updated by `tts_set_voice`; read by `tts_speak`.
static ACTIVE_VOICE: OnceLock<RwLock<String>> = OnceLock::new();

fn voice_lock() -> &'static RwLock<String> {
    ACTIVE_VOICE.get_or_init(|| RwLock::new(VOICE_DEFAULT.to_string()))
}

fn get_voice() -> String {
    voice_lock()
        .read()
        .map(|g| g.clone())
        .unwrap_or_else(|_| VOICE_DEFAULT.to_string())
}

fn set_voice_inner(voice: String) {
    if let Ok(mut g) = voice_lock().write() {
        *g = voice;
    }
}

// ─── Progress event payload ───────────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
struct TtsProgressEvent {
    phase: &'static str,
    step:  u32,
    total: u32,
    label: String,
}

impl TtsProgressEvent {
    fn step(step: u32, total: u32, label: impl Into<String>) -> Self {
        Self { phase: "step", step, total, label: label.into() }
    }
    fn ready() -> Self {
        Self { phase: "ready", step: 0, total: 0, label: String::new() }
    }
}

// ─── Worker thread ────────────────────────────────────────────────────────────

enum TtsCmd {
    Init {
        cb:   Box<dyn FnMut(LoadProgress) + Send + 'static>,
        done: oneshot::Sender<Result<(), String>>,
    },
    Speak {
        text:  String,
        voice: String,
        done:  oneshot::Sender<()>,
    },
}

/// Channel to the dedicated TTS thread.  The thread is started the first time
/// `get_tx()` is called (typically from `tts_init` or `tts_speak`).
static TTS_TX: OnceLock<std::sync::mpsc::SyncSender<TtsCmd>> = OnceLock::new();

fn get_tx() -> &'static std::sync::mpsc::SyncSender<TtsCmd> {
    TTS_TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::sync_channel::<TtsCmd>(16);
        std::thread::Builder::new()
            .name("skill-tts".into())
            .spawn(|| tts_worker(rx))
            .expect("failed to spawn TTS worker thread");
        tx
    })
}

// ─── espeak-ng data path (macOS app bundle) ───────────────────────────────────

/// On macOS, espeak-ng needs its data directory pointed at the right location
/// before the first phonemisation call.
///
/// Resolution order:
/// 1. `ESPEAK_DATA_PATH` env var                     — runtime override
/// 2. `Contents/Resources/espeak-ng-data/`           — .app bundle (release)
/// 3. Compile-time `ESPEAK_DATA_PATH_DEV`            — dev build (baked by build.rs
///    from `espeak-static/share/espeak-ng-data` after the static lib is built)
#[cfg(target_os = "macos")]
fn init_espeak_data_path() {
    // ── 1. Runtime env-var override ──────────────────────────────────────────
    if let Ok(p) = std::env::var("ESPEAK_DATA_PATH") {
        let path = std::path::Path::new(&p);
        if path.is_dir() {
            kittentts::phonemize::set_data_path(path);
            eprintln!("[tts] espeak-ng data path (env): {p}");
            return;
        }
        eprintln!("[tts] ESPEAK_DATA_PATH={p} is not a directory — ignoring");
    }

    // ── 2. .app bundle path ───────────────────────────────────────────────────
    // Layout: Contents/MacOS/<exe>  →  Contents/Resources/espeak-ng-data/
    if let Ok(exe) = std::env::current_exe() {
        let bundled = exe
            .parent()                     // Contents/MacOS/
            .and_then(|p| p.parent())     // Contents/
            .map(|p| p.join("Resources").join("espeak-ng-data"));

        if let Some(ref path) = bundled {
            if path.is_dir() {
                kittentts::phonemize::set_data_path(path);
                eprintln!("[tts] espeak-ng data path (bundle): {}", path.display());
                return;
            }
        }
    }

    // ── 3. Compile-time dev path (baked in by build.rs) ──────────────────────
    // build.rs emits cargo:rustc-env=ESPEAK_DATA_PATH_DEV=<abs-path> after the
    // static library build places espeak-ng-data under espeak-static/share/.
    // This makes plain `cargo run` / debug builds work without Homebrew.
    if let Some(p) = option_env!("ESPEAK_DATA_PATH_DEV") {
        let path = std::path::Path::new(p);
        if path.is_dir() {
            kittentts::phonemize::set_data_path(path);
            eprintln!("[tts] espeak-ng data path (dev static): {p}");
            return;
        }
        eprintln!("[tts] ESPEAK_DATA_PATH_DEV={p} (compile-time) is not a directory — ignoring");
    }

    // ── nothing found ─────────────────────────────────────────────────────────
    eprintln!(
        "[tts] WARNING: espeak-ng data path not resolved — \
         phonemisation will likely fail.\n\
         Run `bash scripts/build-espeak-static.sh` so \
         espeak-static/share/espeak-ng-data is populated, \
         then rebuild."
    );
}

#[cfg(not(target_os = "macos"))]
fn init_espeak_data_path() {
    // Linux / Windows: espeak-ng data is on standard system paths; no action needed.
}

/// Entry-point for the dedicated TTS OS thread.
///
/// The thread owns the KittenTTS model and the `rodio::OutputStream` for its
/// entire lifetime.  It processes one [`TtsCmd`] at a time, serialising both
/// inference and audio playback so they never overlap.
fn tts_worker(rx: std::sync::mpsc::Receiver<TtsCmd>) {
    // Point espeak-ng at the bundled data directory before any phonemisation.
    // This must happen before the first KittenTTS model load (which triggers
    // espeak_ng_Initialize internally on the first generate() call).
    init_espeak_data_path();

    // Open the default audio output once at startup.  If the device is not
    // available yet, we leave `stream = None` and retry on the first Speak.
    let mut stream: Option<MixerDeviceSink> = DeviceSinkBuilder::open_default_sink()
        .map_err(|e| eprintln!("[tts] warning: could not open audio at startup: {e}"))
        .ok();

    // KittenTTS model — None until the first Init cmd is processed.
    let mut model: Option<KittenTTS> = None;

    for cmd in rx {
        match cmd {
            // ── Init ─────────────────────────────────────────────────────────
            TtsCmd::Init { cb, done } => {
                if model.is_some() {
                    // Already loaded — reply immediately.
                    done.send(Ok(())).ok();
                    continue;
                }
                match download::load_from_hub_cb(HF_REPO, cb) {
                    Ok(m) => {
                        let voices = m.available_voices.clone();
                        let _ = AVAILABLE_VOICES.set(voices.clone());
                        eprintln!(
                            "[tts] model ready (repo={HF_REPO} sample_rate={SAMPLE_RATE} Hz \
                             voices={voices:?})"
                        );
                        model = Some(m);
                        done.send(Ok(())).ok();
                    }
                    Err(e) => {
                        done.send(Err(format!("kittentts: model load failed: {e}"))).ok();
                    }
                }
            }

            // ── Speak ─────────────────────────────────────────────────────────
            TtsCmd::Speak { text, voice, done } => {
                // Lazy-init if tts_init was never called.
                if model.is_none() {
                    match download::load_from_hub_cb(HF_REPO, |_| {}) {
                        Ok(m) => {
                            let _ = AVAILABLE_VOICES.set(m.available_voices.clone());
                            model = Some(m);
                        }
                        Err(e) => {
                            eprintln!("[tts] lazy init failed: {e}");
                            done.send(()).ok();
                            continue;
                        }
                    }
                }

                // Re-open audio stream if it failed previously.
                if stream.is_none() {
                    stream = DeviceSinkBuilder::open_default_sink()
                        .map_err(|e| eprintln!("[tts] could not open audio: {e}"))
                        .ok();
                }

                match (&model, &stream) {
                    (Some(m), Some(s)) => {
                        match speak_model_inner(m, s, &text, &voice) {
                            Ok(()) => {}
                            Err(SpeakError::Synthesis(e)) => {
                                eprintln!("[tts] synthesis error: {e}");
                            }
                        }
                    }
                    (_, None) => {
                        eprintln!("[tts] speak skipped: no audio output device");
                    }
                    _ => {}
                }

                done.send(()).ok();
            }
        }
    }

    eprintln!("[tts] worker thread exiting");
}

// ─── Synthesis + playback ─────────────────────────────────────────────────────

/// Distinguishes synthesis failures (espeak/ONNX) from audio-device failures.
/// Only audio-device failures should cause the stream to be reopened.
enum SpeakError {
    Synthesis(String),
}

fn speak_model_inner(
    model:  &KittenTTS,
    stream: &MixerDeviceSink,
    text:   &str,
    voice:  &str,
) -> Result<(), SpeakError> {
    let t0 = std::time::Instant::now();

    let mut samples = model
        .generate(text, voice, SPEED, /* preprocess */ true)
        .map_err(|e| SpeakError::Synthesis(format!("synthesis failed for {text:?}: {e}")))?;

    if samples.is_empty() {
        eprintln!("[tts] synthesis returned no samples for {text:?} voice={voice:?}");
        return Ok(());
    }

    tts_log(&format!(
        "synthesised {len} samples ({dur:.2} s) in {ms} ms — \
         text={text:?} voice={voice:?}",
        len = samples.len(),
        dur = samples.len() as f32 / SAMPLE_RATE as f32,
        ms  = t0.elapsed().as_millis(),
    ));

    // Append tail silence so the hardware buffer fully drains.
    let silence_len = (SAMPLE_RATE as f32 * TAIL_SILENCE_SECS) as usize;
    samples.extend(std::iter::repeat_n(0.0_f32, silence_len));

    let player = Player::connect_new(stream.mixer());
    player.append(SamplesBuffer::new(
        std::num::NonZero::new(1u16).unwrap(),
        std::num::NonZero::new(SAMPLE_RATE).unwrap(),
        samples,
    ));
    player.sleep_until_end();

    Ok(())
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

/// Pre-download and warm-up the TTS model, broadcasting `"tts-progress"` events
/// to every open window.  Safe to call multiple times — emits `{ phase:"ready" }`
/// immediately if already initialised.
#[tauri::command]
pub async fn tts_init(app: tauri::AppHandle) {
    // Fast path: already initialised.
    if AVAILABLE_VOICES.get().is_some() {
        app.emit("tts-progress", TtsProgressEvent::ready()).ok();
        return;
    }

    let tx = get_tx();
    let (done_tx, done_rx) = oneshot::channel::<Result<(), String>>();

    let app2 = app.clone();
    let cb = move |progress: LoadProgress| {
        let event = match progress {
            LoadProgress::Fetching { step, total, file } => {
                TtsProgressEvent::step(step, total, file)
            }
            LoadProgress::Loading => TtsProgressEvent::step(4, 4, "Loading ONNX session"),
        };
        app2.emit("tts-progress", event).ok();
    };

    if tx.send(TtsCmd::Init { cb: Box::new(cb), done: done_tx }).is_err() {
        eprintln!("[tts] tts_init: channel send failed (thread not running?)");
        return;
    }

    match done_rx.await {
        Ok(Ok(())) => {
            app.emit("tts-progress", TtsProgressEvent::ready()).ok();
        }
        Ok(Err(e)) => {
            eprintln!("[tts] init error: {e}");
        }
        Err(_) => {
            eprintln!("[tts] tts_init: worker thread died");
        }
    }
}

/// Synthesise `text` and play it on the default audio output.
///
/// Awaits completion — returns only when playback is fully done.  This lets
/// callers sequence TTS before a countdown begins.
///
/// `voice` is optional; if omitted or `null`, the last voice set via
/// [`tts_set_voice`] (or `"Jasper"` by default) is used.
#[tauri::command]
pub async fn tts_speak(text: String, voice: Option<String>) {
    let voice = voice
        .filter(|v| !v.is_empty())
        .unwrap_or_else(get_voice);

    let tx = get_tx();
    let (done_tx, done_rx) = oneshot::channel::<()>();

    if tx.send(TtsCmd::Speak { text, voice, done: done_tx }).is_err() {
        eprintln!("[tts] tts_speak: channel send failed");
        return;
    }

    done_rx.await.ok();
}

/// Return the voice names bundled in the KittenTTS model.
///
/// If the model is already initialised, the cached list is returned instantly.
/// Otherwise a lightweight fetch of `config.json` + `voices.npz` is performed
/// on a blocking thread (no ONNX session build, so it completes in milliseconds
/// when the files are already cached locally).  The result is stored in
/// `AVAILABLE_VOICES` so subsequent calls — including the real `tts_init` —
/// see the same list without redundant work.
///
/// Falls back to `["Jasper"]` only if both the model and the Hub files are
/// completely unavailable (no network and no local cache).
#[tauri::command]
pub async fn tts_list_voices() -> Vec<String> {
    // Fast path: already cached (model loaded or previous call populated it).
    if let Some(voices) = AVAILABLE_VOICES.get() {
        return voices.clone();
    }

    // Slower path: read config.json + voices.npz without building ONNX session.
    let voices = tokio::task::spawn_blocking(|| {
        download::list_voices_from_hub(HF_REPO)
            .unwrap_or_else(|e| {
                eprintln!("[tts] list_voices_from_hub failed: {e}");
                vec![VOICE_DEFAULT.to_string()]
            })
    })
    .await
    .unwrap_or_else(|_| vec![VOICE_DEFAULT.to_string()]);

    // Cache for future calls (OnceLock::set is a no-op if already set).
    let _ = AVAILABLE_VOICES.set(voices.clone());
    voices
}

/// Persist `voice` as the active voice used by all subsequent [`tts_speak`]
/// calls (including those from the calibration page) that do not supply an
/// explicit voice.
#[tauri::command]
pub async fn tts_set_voice(voice: String) {
    tts_log(&format!("active voice → {voice:?}"));
    set_voice_inner(voice);
}
