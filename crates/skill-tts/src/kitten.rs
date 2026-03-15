// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! KittenTTS backend — ONNX-based English TTS, ~30 MB, no GPU required.
//!
//! Compiled only when the `tts-kitten` Cargo feature is enabled.


use std::sync::{OnceLock, atomic::{AtomicBool, Ordering}};

use kittentts::{KittenTTS, download::{self, LoadProgress}};
use rodio::{DeviceSinkBuilder, MixerDeviceSink};
use tokio::sync::oneshot;

use crate::{play_f32_audio, tts_log, init_espeak_data_path};

// ─── Constants ────────────────────────────────────────────────────────────────

pub const HF_REPO:       &str = "KittenML/kitten-tts-mini-0.8";
pub const VOICE_DEFAULT: &str = "Jasper";
const SPEED:                    f32  = 1.0;
pub const SAMPLE_RATE:   u32  = kittentts::SAMPLE_RATE;

// ─── Statics ──────────────────────────────────────────────────────────────────

pub static AVAILABLE_VOICES: OnceLock<Vec<String>>               = OnceLock::new();
pub static LOADED:           AtomicBool                          = AtomicBool::new(false);
           static ACTIVE_VOICE:     OnceLock<std::sync::RwLock<String>> = OnceLock::new();

// ─── Voice accessors ──────────────────────────────────────────────────────────

fn voice_lock() -> &'static std::sync::RwLock<String> {
    ACTIVE_VOICE.get_or_init(|| std::sync::RwLock::new(VOICE_DEFAULT.to_string()))
}

pub fn get_voice() -> String {
    voice_lock().read().map(|g| g.clone()).unwrap_or_else(|_| VOICE_DEFAULT.to_string())
}

pub fn set_voice(voice: String) {
    if let Ok(mut g) = voice_lock().write() { *g = voice; }
}

// ─── Worker channel ───────────────────────────────────────────────────────────

pub enum Cmd {
    Init  { cb: Box<dyn FnMut(LoadProgress) + Send + 'static>, done: oneshot::Sender<Result<(), String>> },
    Speak { text: String, voice: String, done: oneshot::Sender<()> },
    Unload { done: oneshot::Sender<()> },
    Shutdown { done: std::sync::mpsc::SyncSender<()> },
}

static TX: OnceLock<std::sync::mpsc::SyncSender<Cmd>> = OnceLock::new();

/// Send a blocking `Shutdown` command to the worker if it has been started.
/// Returns `true` if the channel send succeeded (worker is running).
pub fn try_shutdown(done: std::sync::mpsc::SyncSender<()>) -> bool {
    TX.get().map(|ch| ch.send(Cmd::Shutdown { done }).is_ok()).unwrap_or(false)
}

pub fn get_tx() -> &'static std::sync::mpsc::SyncSender<Cmd> {
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Cmd>(16);
        std::thread::Builder::new()
            .name("skill-tts".into())
            .spawn(|| worker(rx))
            .expect("failed to spawn KittenTTS worker thread");
        tx
    })
}

// ─── Worker ───────────────────────────────────────────────────────────────────

fn worker(rx: std::sync::mpsc::Receiver<Cmd>) {
    init_espeak_data_path();

    let mut stream: Option<MixerDeviceSink> = DeviceSinkBuilder::open_default_sink()
        .map_err(|e| eprintln!("[tts] warning: could not open audio: {e}")).ok();
    let mut model: Option<KittenTTS> = None;

    for cmd in rx {
        match cmd {
            Cmd::Init { cb, done } => {
                if LOADED.load(Ordering::Relaxed) {
                    done.send(Ok(())).ok();
                    continue;
                }
                match download::load_from_hub_cb(HF_REPO, cb) {
                    Ok(m) => {
                        let voices = m.available_voices.clone();
                        let _ = AVAILABLE_VOICES.set(voices.clone());
                        eprintln!("[tts] KittenTTS ready (voices={voices:?})");
                        model = Some(m);
                        LOADED.store(true, Ordering::Relaxed);
                        done.send(Ok(())).ok();
                    }
                    Err(e) => {
                        done.send(Err(format!("kittentts load failed: {e}"))).ok();
                    }
                }
            }

            Cmd::Speak { text, voice, done } => {
                if model.is_none() {
                    match download::load_from_hub_cb(HF_REPO, |_| {}) {
                        Ok(m) => {
                            let _ = AVAILABLE_VOICES.set(m.available_voices.clone());
                            LOADED.store(true, Ordering::Relaxed);
                            model = Some(m);
                        }
                        Err(e) => {
                            eprintln!("[tts] lazy init failed: {e}");
                            done.send(()).ok();
                            continue;
                        }
                    }
                }
                if stream.is_none() {
                    stream = DeviceSinkBuilder::open_default_sink()
                        .map_err(|e| eprintln!("[tts] could not open audio: {e}")).ok();
                }
                if let (Some(m), Some(s)) = (&model, &stream) {
                    if let Err(e) = speak_inner(m, s, &text, &voice) {
                        eprintln!("[tts] synthesis error: {e}");
                    }
                } else {
                    eprintln!("[tts] speak skipped: no audio device");
                }
                done.send(()).ok();
            }

            Cmd::Unload { done } => {
                model = None;
                LOADED.store(false, Ordering::Relaxed);
                eprintln!("[tts] KittenTTS model unloaded");
                done.send(()).ok();
            }

            Cmd::Shutdown { done } => {
                // Explicitly drop resources before process static teardown.
                drop(stream.take());
                drop(model.take());
                LOADED.store(false, Ordering::Relaxed);
                eprintln!("[tts] KittenTTS shutdown complete");
                done.send(()).ok();
                return;
            }
        }
    }
}

// ─── Synthesis ────────────────────────────────────────────────────────────────

fn speak_inner(
    model: &KittenTTS, stream: &MixerDeviceSink, text: &str, voice: &str,
) -> Result<(), String> {
    let t0      = std::time::Instant::now();
    let samples = model
        .generate(text, voice, SPEED, true)
        .map_err(|e| format!("synthesis failed for {text:?}: {e}"))?;
    if samples.is_empty() {
        eprintln!("[tts] no samples for {text:?} voice={voice:?}");
        return Ok(());
    }
    tts_log(&format!(
        "synthesised {} samples ({:.2} s) in {} ms — text={text:?} voice={voice:?}",
        samples.len(), samples.len() as f32 / SAMPLE_RATE as f32,
        t0.elapsed().as_millis(),
    ));
    play_f32_audio(stream, samples, SAMPLE_RATE);
    Ok(())
}
