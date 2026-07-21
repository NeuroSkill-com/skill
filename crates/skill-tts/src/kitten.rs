// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! KittenTTS backend — native RLX inference via `rlx-kittentts`.
//!
//! Voices + config come from [`HF_REPO`]; the decomposed RLX graph is fetched
//! from [`skill_constants::KITTEN_TTS_RLX_HF_REPO`] into `<snapshot>/rlx_bundle/`.

use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    OnceLock,
};

use anyhow::Context;
use rlx_kittentts::KittenTTS;
use rlx_runtime::Device;
use rodio::MixerDeviceSink;
use tokio::sync::oneshot;

use crate::{init_espeak_data_path, play_f32_audio, skill_dir};

// ─── Constants ────────────────────────────────────────────────────────────────

pub use skill_constants::KITTEN_TTS_HF_REPO as HF_REPO;
pub use skill_constants::KITTEN_TTS_VOICE_DEFAULT as VOICE_DEFAULT;
const SPEED: f32 = skill_constants::KITTEN_TTS_SPEED;
const LANG: &str = "en-us";
pub const SAMPLE_RATE: u32 = rlx_kittentts::SAMPLE_RATE;

/// Progress event emitted during model loading (mirrors legacy `kittentts` API).
#[derive(Debug, Clone)]
pub enum LoadProgress {
    Fetching { step: u32, total: u32, file: String },
    Loading,
}

// ─── Statics ──────────────────────────────────────────────────────────────────

pub static AVAILABLE_VOICES: OnceLock<Vec<String>> = OnceLock::new();
pub static LOADED: AtomicBool = AtomicBool::new(false);
static ACTIVE_VOICE: OnceLock<std::sync::RwLock<String>> = OnceLock::new();

// ─── Voice accessors ──────────────────────────────────────────────────────────

fn voice_lock() -> &'static std::sync::RwLock<String> {
    ACTIVE_VOICE.get_or_init(|| std::sync::RwLock::new(VOICE_DEFAULT.to_string()))
}

pub fn get_voice() -> String {
    voice_lock()
        .read()
        .map(|g| g.clone())
        .unwrap_or_else(|_| VOICE_DEFAULT.to_string())
}

pub fn set_voice(voice: String) {
    if let Ok(mut g) = voice_lock().write() {
        *g = voice;
    }
}

// ─── Worker channel ───────────────────────────────────────────────────────────

pub enum Cmd {
    Init {
        cb: Box<dyn FnMut(LoadProgress) + Send + 'static>,
        done: oneshot::Sender<anyhow::Result<()>>,
    },
    Speak {
        text: String,
        voice: String,
        done: oneshot::Sender<()>,
    },
    Unload {
        done: oneshot::Sender<()>,
    },
    Shutdown {
        done: std::sync::mpsc::SyncSender<()>,
    },
}

static TX: OnceLock<std::sync::mpsc::SyncSender<Cmd>> = OnceLock::new();

pub fn try_shutdown(done: std::sync::mpsc::SyncSender<()>) -> bool {
    TX.get()
        .map(|ch| ch.send(Cmd::Shutdown { done }).is_ok())
        .unwrap_or(false)
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

// ─── Model load ─────────────────────────────────────────────────────────────

fn hf_cache_dir() -> PathBuf {
    skill_dir().join("models/kittentts/hf-cache")
}

fn resolve_device() -> Device {
    if std::env::var("KITTEN_TTS_DEVICE").as_deref() == Ok("metal") {
        #[cfg(all(target_os = "macos", feature = "tts-kitten"))]
        if rlx_runtime::device_ext::is_available(Device::Metal) {
            return Device::Metal;
        }
    }
    Device::Cpu
}

/// Match rlx-kittentts smoke defaults so HF `rlx_bundle` synthesis is audible.
fn apply_native_runtime_defaults() {
    if std::env::var_os("KITTEN_RLX_INFER").is_none() {
        // SAFETY: process-local defaults before any KittenTTS load; no concurrent readers yet.
        unsafe {
            std::env::set_var("KITTEN_RLX_INFER", "production");
        }
    }
}

/// Fetch the RLX-native graph into `<snapshot>/rlx_bundle/` when missing.
fn ensure_rlx_bundle(snapshot: &Path, mut on_progress: impl FnMut(LoadProgress)) -> anyhow::Result<()> {
    let dest = snapshot.join("rlx_bundle");
    let files = skill_constants::KITTEN_TTS_RLX_BUNDLE_FILES;
    if files.iter().all(|name| dest.join(name).is_file()) {
        return Ok(());
    }

    std::fs::create_dir_all(&dest).with_context(|| format!("create {}", dest.display()))?;

    let api = hf_hub::api::sync::ApiBuilder::new()
        .with_cache_dir(hf_cache_dir())
        .build()
        .context("hf_hub ApiBuilder (KittenTTS RLX bundle)")?;
    let repo = api.model(skill_constants::KITTEN_TTS_RLX_HF_REPO.to_string());

    // Progress steps 2..=4 follow the KittenML fetch at step 1.
    for (i, name) in files.iter().enumerate() {
        on_progress(LoadProgress::Fetching {
            step: (i as u32) + 2,
            total: 5,
            file: (*name).into(),
        });
        let cached = repo
            .get(name)
            .with_context(|| format!("download {name} from {}", skill_constants::KITTEN_TTS_RLX_HF_REPO))?;
        let target = dest.join(name);
        if cached != target {
            std::fs::copy(&cached, &target)
                .with_context(|| format!("copy {} → {}", cached.display(), target.display()))?;
        }
    }
    Ok(())
}

fn load_from_hub_cb<F>(repo_id: &str, mut on_progress: F) -> anyhow::Result<KittenTTS>
where
    F: FnMut(LoadProgress),
{
    apply_native_runtime_defaults();

    on_progress(LoadProgress::Fetching {
        step: 1,
        total: 5,
        file: "config.json".into(),
    });
    let snapshot = rlx_kittentts::download::fetch_repo(repo_id, &hf_cache_dir())
        .with_context(|| format!("fetch KittenTTS checkpoint {repo_id}"))?;

    ensure_rlx_bundle(&snapshot, &mut on_progress)?;

    on_progress(LoadProgress::Loading);
    KittenTTS::load_from_dir(&snapshot, resolve_device()).context("load KittenTTS (native RLX)")
}

// ─── Worker ───────────────────────────────────────────────────────────────────

fn worker(rx: std::sync::mpsc::Receiver<Cmd>) {
    init_espeak_data_path();

    let mut stream: Option<rodio::MixerDeviceSink> = None;
    let mut model: Option<KittenTTS> = None;

    for cmd in rx {
        match cmd {
            Cmd::Init { cb, done } => {
                if LOADED.load(Ordering::Relaxed) {
                    done.send(Ok(())).ok();
                    continue;
                }
                match load_from_hub_cb(HF_REPO, cb) {
                    Ok(m) => {
                        let voices = m.available_voices.clone();
                        let _ = AVAILABLE_VOICES.set(voices.clone());
                        tts_log!("tts", "KittenTTS ready (native RLX, voices={voices:?})");
                        model = Some(m);
                        LOADED.store(true, Ordering::Relaxed);
                        done.send(Ok(())).ok();
                    }
                    Err(e) => {
                        done.send(Err(anyhow::anyhow!("rlx-kittentts load failed: {e}"))).ok();
                    }
                }
            }

            Cmd::Speak { text, voice, done } => {
                if model.is_none() {
                    match load_from_hub_cb(HF_REPO, |_| {}) {
                        Ok(m) => {
                            let _ = AVAILABLE_VOICES.set(m.available_voices.clone());
                            LOADED.store(true, Ordering::Relaxed);
                            model = Some(m);
                        }
                        Err(e) => {
                            tts_log!("tts", "lazy init failed: {e}");
                            done.send(()).ok();
                            continue;
                        }
                    }
                }
                stream = rodio::DeviceSinkBuilder::open_default_sink()
                    .map_err(|e| tts_log!("tts", "could not open audio: {e}"))
                    .ok();
                if let (Some(m), Some(s)) = (&model, &stream) {
                    if let Err(e) = speak_inner(m, s, &text, &voice) {
                        tts_log!("tts", "synthesis error: {e}");
                    }
                } else {
                    tts_log!("tts", "speak skipped: no audio device");
                }
                done.send(()).ok();
            }

            Cmd::Unload { done } => {
                model = None;
                LOADED.store(false, Ordering::Relaxed);
                tts_log!("tts", "KittenTTS model unloaded");
                done.send(()).ok();
            }

            Cmd::Shutdown { done } => {
                drop(stream.take());
                drop(model.take());
                LOADED.store(false, Ordering::Relaxed);
                tts_log!("tts", "KittenTTS shutdown complete");
                done.send(()).ok();
                return;
            }
        }
    }
}

// ─── Synthesis ────────────────────────────────────────────────────────────────

fn speak_inner(model: &KittenTTS, stream: &MixerDeviceSink, text: &str, voice: &str) -> anyhow::Result<()> {
    let t0 = std::time::Instant::now();
    let samples = model
        .generate_from_text(text, voice, SPEED, LANG)
        .with_context(|| format!("synthesis failed for {text:?}"))?;
    if samples.is_empty() {
        tts_log!("tts", "no samples for {text:?} voice={voice:?}");
        return Ok(());
    }
    tts_log!(
        "tts",
        "synthesised {} samples ({:.2} s) in {} ms — text={text:?} voice={voice:?}",
        samples.len(),
        samples.len() as f32 / SAMPLE_RATE as f32,
        t0.elapsed().as_millis(),
    );
    play_f32_audio(stream, samples, SAMPLE_RATE);
    Ok(())
}

/// Headless E2E: download checkpoints, synthesize, write WAV.
pub fn e2e_synthesize_to_wav(
    text: &str,
    voice: &str,
    out: &Path,
    mut on_progress: impl FnMut(LoadProgress),
) -> anyhow::Result<()> {
    let model = load_from_hub_cb(HF_REPO, &mut on_progress)?;
    let audio = model
        .generate_from_text(text, voice, SPEED, LANG)
        .with_context(|| format!("synthesis failed for {text:?}"))?;
    anyhow::ensure!(!audio.is_empty(), "synthesised audio is empty");
    model
        .write_wav(&audio, out)
        .with_context(|| format!("write WAV {}", out.display()))?;
    Ok(())
}

#[cfg(feature = "whisper-validate")]
fn resample_linear(samples: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == to_hz || samples.is_empty() {
        return samples.to_vec();
    }
    let out_len = (samples.len() as u64 * to_hz as u64 / from_hz as u64).max(1) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 * from_hz as f64 / to_hz as f64;
        let idx = src.floor() as usize;
        let frac = (src - idx as f64) as f32;
        let a = samples[idx.min(samples.len() - 1)];
        let b = samples[(idx + 1).min(samples.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

#[cfg(feature = "whisper-validate")]
fn whisper_weights_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("RLX_WHISPER_DIR") {
        let p = PathBuf::from(dir);
        if p.join("model.safetensors").is_file() && p.join("tokenizer.json").is_file() {
            return Some(p);
        }
    }
    for name in ["whisper-base.en", "whisper-tiny.en", "whisper-tiny"] {
        for root in [
            PathBuf::from(".cache").join(name),
            dirs::home_dir()
                .map(|h| h.join(".cache/rlx-models").join(name))
                .unwrap_or_default(),
            PathBuf::from("../rlx-models/.cache").join(name),
        ] {
            if root.join("model.safetensors").is_file() && root.join("tokenizer.json").is_file() {
                return Some(root);
            }
        }
    }
    None
}

#[cfg(feature = "whisper-validate")]
fn transcript_covers_reference(reference: &str, transcript: &str, min_ratio: f32) -> bool {
    fn words(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2)
            .map(str::to_string)
            .collect()
    }
    let reference_words = words(reference);
    if reference_words.is_empty() {
        return false;
    }
    let heard = words(transcript);
    let hits = reference_words
        .iter()
        .filter(|w| heard.iter().any(|h| h == *w || h.contains(w.as_str())))
        .count();
    hits as f32 / reference_words.len() as f32 >= min_ratio
}

/// Resample synthesized WAV to 16 kHz and transcribe with `rlx-whisper` (intelligibility gate).
#[cfg(feature = "whisper-validate")]
pub fn e2e_validate_wav_with_whisper(wav: &Path, reference_text: &str) -> anyhow::Result<String> {
    use rlx_whisper::{WhisperRunner, SAMPLE_RATE as WHISPER_RATE};

    let whisper_dir = whisper_weights_dir().with_context(|| {
        "Whisper weights not found for ASR validation.\n\
         Set RLX_WHISPER_DIR or run `just fetch-whisper` in rlx-models."
    })?;

    let pcm_24k = hound::WavReader::open(wav)
        .with_context(|| format!("open WAV {}", wav.display()))?
        .samples::<i16>()
        .map(|s| s.unwrap_or(0) as f32 / i16::MAX as f32)
        .collect::<Vec<_>>();
    let pcm_16k = resample_linear(&pcm_24k, SAMPLE_RATE, WHISPER_RATE as u32);

    let mut runner = WhisperRunner::builder()
        .weights(whisper_dir.join("model.safetensors"))
        .config_path(whisper_dir.join("config.json"))
        .tokenizer_path(whisper_dir.join("tokenizer.json"))
        .device(Device::Cpu)
        .language("en")
        .build()
        .context("build WhisperRunner")?;

    let transcript = runner.transcribe_greedy(&pcm_16k).context("Whisper transcribe")?;

    anyhow::ensure!(
        transcript_covers_reference(reference_text, &transcript, 0.45),
        "Whisper transcript missed reference text.\n\
         reference: {reference_text:?}\n\
         heard:     {transcript:?}"
    );
    Ok(transcript)
}
