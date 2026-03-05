// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! NeuTTS backend — GGUF backbone + NeuCodec decoder, voice-cloning, multilingual.
//!
//! Compiled only when the `tts-neutts` Cargo feature is enabled.
#![cfg(feature = "tts-neutts")]

use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock, atomic::{AtomicBool, Ordering}};

use hf_hub::{Cache, Repo, api::sync::ApiBuilder as HfApiBuilder};
use hound;
use rodio::{DeviceSinkBuilder, MixerDeviceSink};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

use super::{play_f32_audio, skill_dir, tts_log, init_espeak_data_path};

// ─── Constants ────────────────────────────────────────────────────────────────

pub(super) const SAMPLE_RATE: u32 = neutts::codec::SAMPLE_RATE;

/// Runtime path to the bundled preset voice sample files.
/// Set by `mod.rs::init_neutts_samples_dir()` during Tauri setup.
/// Falls back to `resources/neutts-samples` relative to CWD if unset.
static SAMPLES_DIR: OnceLock<PathBuf> = OnceLock::new();

pub(super) fn set_samples_dir(path: PathBuf) { let _ = SAMPLES_DIR.set(path); }

fn samples_dir() -> PathBuf {
    SAMPLES_DIR.get().cloned().unwrap_or_else(|| PathBuf::from("resources/neutts-samples"))
}

/// Valid preset voice ids — must match filenames under `samples_dir()`.
pub(super) const PRESET_NAMES: &[&str] = &["jo", "dave", "greta", "juliette", "mateo"];

pub(super) fn is_preset(name: &str) -> bool { PRESET_NAMES.contains(&name) }

// ─── Paths within skill_dir ───────────────────────────────────────────────────

/// `skill_dir/models/neutts/` — stores `neucodec_decoder.safetensors` (converted once).
pub(super) fn model_dir() -> PathBuf { skill_dir().join("models/neutts") }

/// `skill_dir/cache/neutts-ref-codes/` — encoded voice reference `.npy` files.
fn ref_code_cache_dir() -> PathBuf { skill_dir().join("cache/neutts-ref-codes") }

/// `skill_dir/cache/neutts-wav/` — generated speech WAV files (content-addressed).
fn wav_cache_dir() -> PathBuf { skill_dir().join("cache/neutts-wav") }

// ─── Statics ──────────────────────────────────────────────────────────────────

pub(super) static LOADING: AtomicBool = AtomicBool::new(false);
pub(super) static READY:   AtomicBool = AtomicBool::new(false);

struct RuntimeConfig {
    backbone_repo: String,
    gguf_file:     Option<String>,
    voice_preset:  String,
    ref_wav_path:  String,
    ref_text:      String,
}

static CFG: OnceLock<RwLock<RuntimeConfig>> = OnceLock::new();

fn cfg_lock() -> &'static RwLock<RuntimeConfig> {
    CFG.get_or_init(|| RwLock::new(RuntimeConfig {
        backbone_repo: "neuphonic/neutts-nano-q4-gguf".into(),
        gguf_file:     None,
        voice_preset:  "jo".into(),
        ref_wav_path:  String::new(),
        ref_text:      String::new(),
    }))
}

pub(super) fn read_cfg() -> (String, Option<String>, String, String, String) {
    let g = cfg_lock().read().unwrap();
    (g.backbone_repo.clone(), g.gguf_file.clone(),
     g.voice_preset.clone(), g.ref_wav_path.clone(), g.ref_text.clone())
}

pub(super) fn set_voice_preset(preset: String) {
    if let Ok(mut g) = cfg_lock().write() {
        g.voice_preset = preset;
    }
}

// ─── Config application ───────────────────────────────────────────────────────

/// Sync runtime config from settings.  Called from `mod.rs::neutts_apply_config`.
pub(super) fn apply_config(cfg: &crate::settings::NeuttsConfig) {
    let was_ready = READY.load(Ordering::Relaxed);

    if let Ok(mut g) = cfg_lock().write() {
        g.backbone_repo = cfg.backbone_repo.clone();
        g.gguf_file     = if cfg.gguf_file.is_empty() { None } else { Some(cfg.gguf_file.clone()) };
        g.voice_preset  = cfg.voice_preset.clone();
        g.ref_wav_path  = cfg.ref_wav_path.clone();
        g.ref_text      = cfg.ref_text.clone();
    }

    // When KittenTTS is also compiled, the `enabled` flag is the runtime switch
    // stored in `super::NEUTTS_ENABLED`.  Update it from here.
    #[cfg(feature = "tts-kitten")]
    super::NEUTTS_ENABLED.store(cfg.enabled, Ordering::Relaxed);

    if cfg.enabled && was_ready {
        READY.store(false, Ordering::Relaxed);
        tts_log("NeuTTS config updated — will reinitialise on next tts_init");
    }
}

// ─── Worker channel ───────────────────────────────────────────────────────────

pub(super) enum Cmd {
    Init {
        backbone_repo: String,
        gguf_file:     Option<String>,
        voice_preset:  String,
        ref_wav_path:  String,
        ref_text:      String,
        cb:   Box<dyn FnMut(neutts::download::LoadProgress) + Send + 'static>,
        done: oneshot::Sender<Result<(), String>>,
    },
    /// `voice_override`: an optional preset name that overrides the reference
    /// for this single utterance only (without mutating stored state).
    Speak {
        text:           String,
        voice_override: Option<String>,
        done:           oneshot::Sender<()>,
    },
    Unload { done: oneshot::Sender<()> },
    /// Blocking shutdown: drops the model synchronously so the Metal/llama.cpp
    /// context is released **before** `exit()` fires C++ static destructors.
    /// Uses a plain `std::sync::mpsc` channel so it can be called from a
    /// non-async context (e.g. Tauri's `RunEvent::Exit` callback).
    Shutdown { done: std::sync::mpsc::SyncSender<()> },
}

static TX: OnceLock<std::sync::mpsc::SyncSender<Cmd>> = OnceLock::new();

/// Send a `Shutdown` command to the worker if it has been started.
/// Returns `true` if the channel send succeeded (worker is running).
pub(super) fn try_shutdown(done: std::sync::mpsc::SyncSender<()>) -> bool {
    TX.get().map(|ch| ch.send(Cmd::Shutdown { done }).is_ok()).unwrap_or(false)
}

pub(super) fn get_tx() -> &'static std::sync::mpsc::SyncSender<Cmd> {
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Cmd>(16);
        std::thread::Builder::new()
            .name("skill-neutts".into())
            .spawn(|| worker(rx))
            .expect("failed to spawn NeuTTS worker thread");
        tx
    })
}

// ─── Worker ───────────────────────────────────────────────────────────────────

fn worker(rx: std::sync::mpsc::Receiver<Cmd>) {
    init_espeak_data_path();

    let mut stream:           Option<MixerDeviceSink> = DeviceSinkBuilder::open_default_sink()
        .map_err(|e| eprintln!("[neutts] warning: could not open audio: {e}")).ok();
    let mut model:            Option<neutts::NeuTTS>  = None;
    let mut loaded_backbone:  String                  = String::new();
    let mut ref_codes:        Vec<i32>                = Vec::new();
    let mut ref_text_cached:  String                  = String::new();
    // Stable per-voice identifier for WAV cache key.
    let mut loaded_voice_key: String                  = "default".to_string();

    for cmd in rx {
        match cmd {
            // ── Init ─────────────────────────────────────────────────────────
            Cmd::Init { backbone_repo, gguf_file, voice_preset, ref_wav_path, ref_text, mut cb, done } => {
                LOADING.store(true, Ordering::Relaxed);

                if model.is_none() || loaded_backbone != backbone_repo {
                    READY.store(false, Ordering::Relaxed);
                    match load(&backbone_repo, gguf_file.as_deref(), |p| cb(p)) {
                        Ok(m) => {
                            eprintln!("[neutts] backbone ready (repo={backbone_repo})");
                            loaded_backbone = backbone_repo;
                            model = Some(m);
                        }
                        Err(e) => {
                            LOADING.store(false, Ordering::Relaxed);
                            done.send(Err(format!("neutts backbone load failed: {e}"))).ok();
                            continue;
                        }
                    }
                }

                let (codes, txt, vkey) = load_ref_codes(
                    model.as_ref().unwrap(), &voice_preset, &ref_wav_path, &ref_text,
                );
                ref_codes        = codes;
                ref_text_cached  = txt;
                loaded_voice_key = vkey;

                READY.store(true, Ordering::Relaxed);
                LOADING.store(false, Ordering::Relaxed);
                done.send(Ok(())).ok();
            }

            // ── Speak ─────────────────────────────────────────────────────────
            Cmd::Speak { text, voice_override, done } => {
                // Lazy-init if unloaded.
                if model.is_none() {
                    let (repo, gguf, preset, wav, txt) = read_cfg();
                    match load(&repo, gguf.as_deref(), |_| {}) {
                        Ok(m) => {
                            loaded_backbone = repo;
                            let (codes, rtext, vkey) = load_ref_codes(&m, &preset, &wav, &txt);
                            ref_codes        = codes;
                            ref_text_cached  = rtext;
                            loaded_voice_key = vkey;
                            model = Some(m);
                            READY.store(true, Ordering::Relaxed);
                        }
                        Err(e) => {
                            eprintln!("[neutts] lazy init failed: {e}");
                            done.send(()).ok();
                            continue;
                        }
                    }
                }
                if stream.is_none() {
                    stream = DeviceSinkBuilder::open_default_sink()
                        .map_err(|e| eprintln!("[neutts] could not open audio: {e}")).ok();
                }

                // Per-utterance voice override (loads inline without touching stored state).
                let (eff_codes, eff_text, eff_vkey): (
                    std::borrow::Cow<Vec<i32>>,
                    std::borrow::Cow<str>,
                    std::borrow::Cow<str>,
                ) = match voice_override.as_deref().filter(|v| !v.is_empty()) {
                    Some(ovr) if is_preset(ovr) => {
                        tts_log(&format!("per-utterance preset override: {ovr:?}"));
                        let (c, t, k) = load_ref_codes(model.as_ref().unwrap(), ovr, "", "");
                        (std::borrow::Cow::Owned(c),
                         std::borrow::Cow::Owned(t),
                         std::borrow::Cow::Owned(k))
                    }
                    _ => (
                        std::borrow::Cow::Borrowed(&ref_codes),
                        std::borrow::Cow::Borrowed(ref_text_cached.as_str()),
                        std::borrow::Cow::Borrowed(loaded_voice_key.as_str()),
                    ),
                };

                if let (Some(m), Some(s)) = (&model, &stream) {
                    speak_cached(m, s, &text, &eff_codes, &eff_text, &loaded_backbone, &eff_vkey);
                } else {
                    eprintln!("[neutts] speak skipped: no audio device");
                }
                done.send(()).ok();
            }

            // ── Unload ────────────────────────────────────────────────────────
            Cmd::Unload { done } => {
                model = None;
                ref_codes.clear();
                ref_text_cached.clear();
                loaded_backbone.clear();
                loaded_voice_key = "default".to_string();
                READY.store(false, Ordering::Relaxed);
                LOADING.store(false, Ordering::Relaxed);
                eprintln!("[neutts] model unloaded");
                done.send(()).ok();
            }

            // ── Shutdown (blocking, called from RunEvent::Exit) ───────────────
            Cmd::Shutdown { done } => {
                // Explicitly drop all resources in the correct order so the
                // llama.cpp Metal context is fully released before `exit()`
                // fires C++ static destructors (`ggml_metal_device_free`).
                drop(stream.take());
                ref_codes.clear();
                ref_text_cached.clear();
                loaded_backbone.clear();
                READY.store(false, Ordering::Relaxed);
                LOADING.store(false, Ordering::Relaxed);
                eprintln!("[neutts] shutdown complete — Metal context released");
                done.send(()).ok();
                // Exit the worker loop so the thread ends cleanly.
                return;
            }
        }
    }
}

// ─── Model loading ────────────────────────────────────────────────────────────
//
// Downloaded blobs (GGUF backbone, pytorch_model.bin) go to the standard
// HuggingFace cache (~/.cache/huggingface/hub).
// The *converted* neucodec_decoder.safetensors is written to skill_dir once.

fn load<F>(
    backbone_repo: &str,
    gguf_file:     Option<&str>,
    mut on_progress: F,
) -> Result<neutts::NeuTTS, String>
where
    F: FnMut(neutts::download::LoadProgress),
{
    use neutts::download::{
        LoadProgress, CODEC_DECODER_REPO, CODEC_DECODER_FILE,
        CODEC_SOURCE_FILE, CODEC_DECODER_SIZE_MB, find_model,
        convert_neucodec_checkpoint,
    };

    let hf_cache = Cache::from_env();
    let api      = HfApiBuilder::new()
        .build()
        .map_err(|e| format!("Failed to init HF client: {e}"))?;

    // ── Step 1/3: backbone GGUF → standard HF cache ───────────────────────────
    on_progress(LoadProgress::Fetching {
        step: 1, total: 3,
        file: gguf_file.unwrap_or("*.gguf").to_string(),
        repo: backbone_repo.into(),
        size_mb: find_model(backbone_repo).map(|m| m.size_mb),
    });

    let resolved_gguf: String = match gguf_file {
        Some(f) => f.to_string(),
        None => {
            let info = api.model(backbone_repo.to_string()).info()
                .map_err(|e| format!("repo info for '{backbone_repo}': {e}"))?;
            info.siblings.into_iter()
                .map(|s| s.rfilename)
                .find(|f| f.ends_with(".gguf"))
                .ok_or_else(|| format!("no .gguf file in '{backbone_repo}'"))?
        }
    };

    let backbone_path = hf_dl(
        &api, &hf_cache, backbone_repo, &resolved_gguf,
        |dl, tot| on_progress(LoadProgress::Downloading {
            step: 1, total: 3, downloaded: dl, total_bytes: tot,
        }),
    )?;

    // ── Step 2/3: NeuCodec decoder → skill_dir (converted once) ──────────────
    let decoder_dest = model_dir().join(CODEC_DECODER_FILE);

    let decoder_path = if decoder_dest.exists() {
        on_progress(LoadProgress::Fetching {
            step: 2, total: 3,
            file: CODEC_DECODER_FILE.into(),
            repo: "(skill_dir)".into(),
            size_mb: None,
        });
        decoder_dest
    } else {
        on_progress(LoadProgress::Fetching {
            step: 2, total: 3,
            file: CODEC_SOURCE_FILE.into(),
            repo: CODEC_DECODER_REPO.into(),
            size_mb: Some(CODEC_DECODER_SIZE_MB),
        });
        let bin_path = hf_dl(
            &api, &hf_cache, CODEC_DECODER_REPO, CODEC_SOURCE_FILE,
            |dl, tot| on_progress(LoadProgress::Downloading {
                step: 2, total: 3, downloaded: dl, total_bytes: tot,
            }),
        )?;
        on_progress(LoadProgress::Loading {
            step: 2, total: 3,
            component: format!("converting {CODEC_SOURCE_FILE} → {CODEC_DECODER_FILE}"),
        });
        convert_neucodec_checkpoint(&bin_path, &decoder_dest, 16, CODEC_DECODER_REPO)
            .map_err(|e| format!("checkpoint conversion failed: {e}"))?;
        decoder_dest
    };

    // ── Step 3/3: load from explicit paths ────────────────────────────────────
    on_progress(LoadProgress::Loading {
        step: 3, total: 3,
        component: "backbone + NeuCodec decoder".into(),
    });
    let language = neutts::download::find_model(backbone_repo)
        .map(|m| m.language)
        .unwrap_or("en-us")
        .to_string();
    neutts::NeuTTS::load_with_decoder(&backbone_path, &decoder_path, &language)
        .map_err(|e| format!("failed to load NeuTTS: {e}"))
}

/// HuggingFace download with byte-level progress, checking `cache` first.
fn hf_dl<F: FnMut(u64, u64)>(
    api:      &hf_hub::api::sync::Api,
    cache:    &Cache,
    repo_id:  &str,
    filename: &str,
    mut on_bytes: F,
) -> Result<PathBuf, String> {
    use hf_hub::api::Progress;

    let cache_repo = cache.repo(Repo::model(repo_id.to_string()));
    if let Some(path) = cache_repo.get(filename) {
        on_bytes(1, 1);
        return Ok(path);
    }
    struct Prog<F: FnMut(u64, u64)> { cb: F, done: u64, total: u64 }
    impl<F: FnMut(u64, u64)> Progress for Prog<F> {
        fn init(&mut self, size: usize, _: &str) { self.total = size as u64; (self.cb)(0, self.total); }
        fn update(&mut self, n: usize) { self.done += n as u64; (self.cb)(self.done, self.total); }
        fn finish(&mut self) { (self.cb)(self.total, self.total); }
    }
    api.model(repo_id.to_string())
        .download_with_progress(filename, Prog { cb: on_bytes, done: 0, total: 0 })
        .map_err(|e| format!("download '{filename}' from '{repo_id}': {e}"))
}

// ─── Reference code loading ───────────────────────────────────────────────────
//
// Returns `(ref_codes, ref_text, voice_key)`.
//
// `voice_key` is stable across restarts and used as part of the WAV cache key:
//   preset  → preset name (`"jo"`, `"dave"`, …)
//   custom  → `"custom-{sha256_of_wav_file}"`
//   default → `"default"`

fn load_ref_codes(
    model:    &neutts::NeuTTS,
    preset:   &str,
    wav_path: &str,
    ref_text: &str,
) -> (Vec<i32>, String, String) {

    // ── Preset voice ──────────────────────────────────────────────────────────
    if !preset.is_empty() {
        let base = samples_dir();
        let npy = base.join(format!("{preset}.npy"));
        let txt = base.join(format!("{preset}.txt"));
        match model.load_ref_codes(&npy) {
            Ok(codes) => {
                let text = std::fs::read_to_string(&txt)
                    .map(|s| s.trim().to_string()).unwrap_or_default();
                tts_log(&format!("preset voice '{preset}' loaded ({} tokens)", codes.len()));
                return (codes, text, preset.to_string());
            }
            Err(e) => eprintln!("[neutts] preset '{preset}' not found at {}: {e}", npy.display()),
        }
    }

    // ── Custom WAV ────────────────────────────────────────────────────────────
    if !wav_path.is_empty() {
        let path      = Path::new(wav_path);
        let voice_key = neutts::cache::sha256_file(path)
            .map(|h| format!("custom-{h}"))
            .unwrap_or_else(|_| format!("custom-{wav_path}"));

        let cache = neutts::RefCodeCache::with_dir(ref_code_cache_dir())
            .map_err(|e| eprintln!("[neutts] ref-code cache open failed: {e}")).ok();

        if let Some((codes, outcome)) = cache.as_ref()
            .and_then(|c| c.try_load(path).ok().flatten())
        {
            tts_log(&format!("custom voice ref-code cache hit: {outcome}"));
            return (codes, ref_text.to_string(), voice_key);
        }

        match neutts::codec::NeuCodecEncoder::new() {
            Ok(enc) => match enc.encode_wav(path) {
                Ok(codes) => {
                    if let Some(c) = &cache {
                        if let Ok(outcome) = c.store(path, &codes) {
                            tts_log(&format!("custom voice encoded+cached: {outcome}"));
                        }
                    }
                    return (codes, ref_text.to_string(), voice_key);
                }
                Err(e) => eprintln!("[neutts] WAV encoding failed: {e}"),
            },
            Err(e) => eprintln!("[neutts] NeuCodecEncoder not available ({e})"),
        }
    }

    // ── Backbone built-in voice ───────────────────────────────────────────────
    tts_log("using backbone built-in voice (no reference)");
    (Vec::new(), String::new(), "default".to_string())
}

// ─── Synthesis ────────────────────────────────────────────────────────────────

fn synthesize(
    model: &neutts::NeuTTS, text: &str, ref_codes: &[i32], ref_text: &str,
) -> Result<Vec<f32>, String> {
    let t0    = std::time::Instant::now();
    let audio = model.infer(text, ref_codes, ref_text)
        .map_err(|e| format!("neutts synthesis failed for {text:?}: {e}"))?;
    if audio.is_empty() {
        return Err(format!("synthesis returned no samples for {text:?}"));
    }
    tts_log(&format!(
        "synthesised {} samples ({:.2} s) in {} ms — text={text:?}",
        audio.len(), audio.len() as f32 / SAMPLE_RATE as f32,
        t0.elapsed().as_millis(),
    ));
    Ok(audio)
}

// ─── WAV cache ────────────────────────────────────────────────────────────────
//
// `skill_dir/cache/neutts-wav/{sha256(backbone ‖ voice_key ‖ text)}.wav`
//
// The hash covers the full (model, voice, text) triplet so any change to any
// dimension produces a different filename — no explicit invalidation needed.

fn wav_cache_path(backbone: &str, voice_key: &str, text: &str) -> PathBuf {
    let mut h = Sha256::new();
    h.update(backbone.as_bytes());
    h.update(b"\0");
    h.update(voice_key.as_bytes());
    h.update(b"\0");
    h.update(text.as_bytes());
    wav_cache_dir().join(format!("{:x}.wav", h.finalize()))
}

// ─── Speak: cache check → synthesise → write cache → play ────────────────────

fn speak_cached(
    model:     &neutts::NeuTTS,
    stream:    &MixerDeviceSink,
    text:      &str,
    ref_codes: &[i32],
    ref_text:  &str,
    backbone:  &str,
    voice_key: &str,
) {
    let cache_path = wav_cache_path(backbone, voice_key, text);

    if cache_path.exists() {
        tts_log(&format!("WAV cache hit: {}", cache_path.display()));
        play_wav(stream, &cache_path);
        return;
    }

    match synthesize(model, text, ref_codes, ref_text) {
        Ok(audio) => {
            if let Err(e) = model.write_wav(&audio, &cache_path) {
                eprintln!("[neutts] WAV cache write failed: {e}");
            } else {
                tts_log(&format!("WAV cached: {}", cache_path.display()));
            }
            play_f32_audio(stream, audio, SAMPLE_RATE);
        }
        Err(e) => eprintln!("[neutts] synthesis error: {e}"),
    }
}

/// Play a cached WAV file.
///
/// Uses `hound::WavReader` to read 16-bit PCM Int samples (as written by
/// `NeuTTS::write_wav`) and converts i16 → f32 for `SamplesBuffer`.
/// This avoids rodio/symphonia format probing which can fail on some PCM WAVs.
fn play_wav(stream: &MixerDeviceSink, path: &Path) {
    let reader = match hound::WavReader::open(path) {
        Ok(r)  => r,
        Err(e) => { eprintln!("[neutts] WAV cache open failed ({}): {e}", path.display()); return; }
    };
    let sample_rate = reader.spec().sample_rate;
    let samples: Vec<f32> = reader
        .into_samples::<i16>()
        .filter_map(|s| s.ok())
        .map(|s| s as f32 / i16::MAX as f32)
        .collect();

    if samples.is_empty() {
        eprintln!("[neutts] WAV cache file is empty: {}", path.display());
        return;
    }
    tts_log(&format!(
        "WAV cache playback: {} samples @ {} Hz ({})",
        samples.len(), sample_rate, path.display()
    ));
    play_f32_audio(stream, samples, sample_rate);
}

// ─── Progress mapper ──────────────────────────────────────────────────────────

pub(super) fn progress_to_event(p: neutts::download::LoadProgress) -> super::TtsProgressEvent {
    use neutts::download::LoadProgress as NP;
    match p {
        NP::Fetching { step, total, file, repo, size_mb } => {
            let label = match size_mb {
                Some(mb) => format!("{file} from {repo} (~{mb} MB)"),
                None     => format!("{file} from {repo}"),
            };
            super::TtsProgressEvent::step(step, total, label)
        }
        NP::Downloading { step, total, downloaded, total_bytes } => {
            super::TtsProgressEvent::step(step, total,
                format!("Downloading… {}/{} MB",
                    downloaded / 1_048_576, total_bytes / 1_048_576))
        }
        NP::Loading { step, total, component } => {
            super::TtsProgressEvent::step(step, total, component)
        }
    }
}
