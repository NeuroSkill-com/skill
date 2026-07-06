// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Real ASR+VAD engine — only compiled when `cfg(asr_active)` (feature `asr` on a
//! non-Windows target). Microphone capture via `cpal`, streaming voice-activity
//! detection via `rlx-vad` (Silero), transcription via `rlx-whisper`.
//!
//! Lifecycle: [`spawn`] starts a dedicated OS thread that owns the (non-`Send`)
//! cpal input stream, the Whisper runner and the Silero session. All status is
//! reported asynchronously through the [`EventSink`] callback; the heavy model
//! download/build happens on that thread so `spawn` returns immediately.
//!
//! The segmentation logic lives in [`crate::segmenter`] (pure, unit-tested);
//! this module only wires audio + models to it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rlx_runtime::Device;
use rlx_vad::{resample_linear, SileroConfig, SileroSession, SileroWeights};
use rlx_whisper::WhisperRunner;

use crate::segmenter::{downmix, SegEvent, Segmenter, SegmenterConfig};
use crate::{AsrEvent, AsrMode, EventSink, TriggerMode};

const TARGET_HZ: usize = 16_000;

/// Spawn the engine on its own thread. Returns once the thread is launched;
/// readiness and failures are delivered via `on_event`.
pub fn spawn(mode: AsrMode, on_event: EventSink, ptt: Arc<AtomicBool>, stop_rx: Receiver<()>, gen: u64) -> Result<()> {
    std::thread::Builder::new()
        .name("skill-asr".into())
        .spawn(move || {
            if let Err(e) = run(&mode, &on_event, &ptt, &stop_rx) {
                on_event(AsrEvent::Error {
                    message: format!("{e:#}"),
                });
            }
            // Clear the global handle (if this thread still owns the session) so
            // is_running()/status() report stopped after an error or natural exit.
            crate::mark_stopped(gen);
            on_event(AsrEvent::Stopped);
        })
        .context("spawn skill-asr thread")?;
    Ok(())
}

fn run(mode: &AsrMode, on_event: &EventSink, ptt: &Arc<AtomicBool>, stop_rx: &Receiver<()>) -> Result<()> {
    on_event(AsrEvent::Loading);

    // ── ASR engine (download on first use) ───────────────────────────────────
    // Dispatch on `mode.engine`: "whisper" (default) or "qwen3-asr". Unknown
    // engines fall through to Whisper.
    let mut transcriber = build_transcriber(&mode.engine, &mode.model, &mode.language)?;

    // ── Silero streaming VAD (weights embedded — no download) ─────────────────
    let mut session = SileroSession::new(SileroWeights::embedded(), SileroConfig::default());
    let frame_len = session.frame_samples();
    let mut seg = Segmenter::new(SegmenterConfig::defaults_16k(frame_len));

    // ── Microphone (cpal owns a non-Send stream; keep it on this thread) ──────
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .context("no default audio input device (microphone)")?;
    let default_cfg = device.default_input_config().context("query default input config")?;
    let sample_format = default_cfg.sample_format();
    let stream_cfg: cpal::StreamConfig = default_cfg.into();
    let channels = stream_cfg.channels.max(1) as usize;
    // cpal 0.17: `SampleRate` / `ChannelCount` are plain `u32` / `u16` aliases.
    let dev_hz = stream_cfg.sample_rate as usize;

    let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>();
    let err_sink = on_event.clone();
    let err_fn = move |e: cpal::StreamError| {
        err_sink(AsrEvent::Error {
            message: format!("audio stream error: {e}"),
        });
    };

    let stream = build_input_stream(&device, &stream_cfg, sample_format, channels, audio_tx, err_fn)
        .context("open microphone stream")?;
    stream.play().context("start microphone stream")?;

    on_event(AsrEvent::Listening);

    let trigger = mode.trigger;
    let mut pending: Vec<f32> = Vec::new();
    let mut was_speaking = false;

    // ── Capture / process loop ────────────────────────────────────────────────
    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }

        // Barge-in guard: while the assistant is speaking (TTS playback), discard
        // captured audio so the mic doesn't transcribe the assistant's own voice.
        // On release, reset VAD + buffers so leftover/echo tail isn't picked up.
        let speaking = crate::is_speaking();
        if was_speaking && !speaking {
            session.reset();
            seg = Segmenter::new(SegmenterConfig::defaults_16k(frame_len));
            pending.clear();
        }
        was_speaking = speaking;

        match audio_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => {
                if speaking {
                    continue; // drop audio captured during playback
                }
                let pcm16k = if dev_hz == TARGET_HZ {
                    chunk
                } else {
                    resample_linear(&chunk, dev_hz, TARGET_HZ)
                };
                match trigger {
                    TriggerMode::Continuous => {
                        pending.extend_from_slice(&pcm16k);
                        while pending.len() >= frame_len {
                            let frame: Vec<f32> = pending.drain(..frame_len).collect();
                            let prob = session.predict_frame(&frame).unwrap_or(0.0);
                            for ev in seg.push_vad_frame(prob, &frame) {
                                handle(ev, &mut session, transcriber.as_mut(), on_event);
                            }
                        }
                    }
                    TriggerMode::PushToTalk => {
                        for ev in seg.push_ptt(ptt.load(Ordering::Relaxed), &pcm16k) {
                            handle(ev, &mut session, transcriber.as_mut(), on_event);
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                // No audio this tick: still service push-to-talk release edges
                // (unless we're gating capture while the assistant speaks).
                if !speaking {
                    if let TriggerMode::PushToTalk = trigger {
                        for ev in seg.push_ptt(ptt.load(Ordering::Relaxed), &[]) {
                            handle(ev, &mut session, transcriber.as_mut(), on_event);
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    // Flush any in-flight utterance before tearing down.
    if let Some(buf) = seg.flush() {
        transcribe_utterance(&buf, transcriber.as_mut(), on_event);
    }
    drop(stream);
    Ok(())
}

/// Map a segmenter event to an `AsrEvent` (+ ASR transcription / VAD reset).
fn handle(ev: SegEvent, session: &mut SileroSession, transcriber: &mut dyn Transcriber, on_event: &EventSink) {
    match ev {
        SegEvent::SpeechStart => on_event(AsrEvent::SpeechStart),
        SegEvent::SpeechEnd => {
            session.reset(); // clear Silero LSTM state between utterances
            on_event(AsrEvent::SpeechEnd);
        }
        SegEvent::Utterance(buf) => transcribe_utterance(&buf, transcriber, on_event),
    }
}

fn transcribe_utterance(pcm: &[f32], transcriber: &mut dyn Transcriber, on_event: &EventSink) {
    match transcriber.transcribe(pcm) {
        Ok(text) => {
            let text = text.trim().to_string();
            if !text.is_empty() {
                on_event(AsrEvent::Transcript { text, is_final: true });
            }
        }
        Err(e) => on_event(AsrEvent::Error {
            message: format!("transcribe: {e:#}"),
        }),
    }
}

// ── ASR backends (engine dispatch) ──────────────────────────────────────────

/// A loaded ASR backend: turns 16 kHz mono PCM into text. One per session.
trait Transcriber {
    fn transcribe(&mut self, pcm16k: &[f32]) -> Result<String>;
}

struct WhisperTranscriber(WhisperRunner);
impl Transcriber for WhisperTranscriber {
    fn transcribe(&mut self, pcm16k: &[f32]) -> Result<String> {
        self.0.transcribe_greedy(pcm16k)
    }
}

struct Qwen3AsrTranscriber(rlx_qwen3_asr::AsrRunner);
impl Transcriber for Qwen3AsrTranscriber {
    fn transcribe(&mut self, pcm16k: &[f32]) -> Result<String> {
        // Empty system prompt → the model auto-detects language and emits a
        // `language <lang>` tag that `transcribe_pcm` strips from the text.
        self.0.transcribe_pcm(pcm16k, "")
    }
}

struct VoxtralTranscriber {
    runner: rlx_voxtral::VoxtralRunner,
    model_dir: PathBuf,
    language: Option<String>,
}
impl Transcriber for VoxtralTranscriber {
    fn transcribe(&mut self, pcm16k: &[f32]) -> Result<String> {
        // Voxtral exposes only `transcribe_wav(Path) -> token ids`, so spill the
        // PCM to a temp 16 kHz WAV, transcribe, then decode the ids to text.
        let tmp = std::env::temp_dir().join(format!("skill-asr-voxtral-{}.wav", std::process::id()));
        write_wav_16k(&tmp, pcm16k)?;
        let ids = self.runner.transcribe_wav(&tmp, self.language.as_deref());
        let _ = std::fs::remove_file(&tmp);
        let ids = ids.context("voxtral transcribe")?;
        rlx_voxtral::decode_token_ids(Some(self.model_dir.as_path()), &ids)
    }
}

/// Construct the ASR backend for `engine` (`"whisper"` | `"qwen3-asr"` |
/// `"voxtral"`; unknown falls back to Whisper). `model` is the engine-specific
/// model id (HF repo).
fn build_transcriber(engine: &str, model: &str, language: &str) -> Result<Box<dyn Transcriber>> {
    match engine.trim().to_ascii_lowercase().as_str() {
        "qwen3-asr" | "qwen3_asr" => {
            let dir = ensure_qwen3_asr_model(model)?;
            let runner = rlx_qwen3_asr::AsrRunner::builder()
                .weights(&dir)
                .device(asr_device())
                .build()
                .context("build Qwen3-ASR runner")?;
            Ok(Box::new(Qwen3AsrTranscriber(runner)))
        }
        "voxtral" => {
            let dir = ensure_voxtral_model(model)?;
            let runner = rlx_voxtral::VoxtralRunner::builder()
                .weights(&dir)
                .device(asr_device())
                .build()
                .context("build Voxtral runner")?;
            let lang = match language.trim() {
                "" | "auto" => None,
                l => Some(l.to_string()),
            };
            Ok(Box::new(VoxtralTranscriber {
                runner,
                model_dir: dir,
                language: lang,
            }))
        }
        _ => Ok(Box::new(WhisperTranscriber(build_runner(model, language)?))),
    }
}

/// Build a Whisper runner against the cached/downloaded checkpoint for `model`
/// (a HuggingFace repo, e.g. `openai/whisper-base.en` or `openai/whisper-small`).
fn build_runner(model: &str, language: &str) -> Result<WhisperRunner> {
    let dir = ensure_whisper_model(model).context("prepare Whisper model")?;
    WhisperRunner::builder()
        .weights(dir.join("model.safetensors"))
        .config_path(dir.join("config.json"))
        .tokenizer_path(dir.join("tokenizer.json"))
        .device(asr_device())
        .language(language.to_string())
        .build()
        .context("build WhisperRunner")
}

/// Offline batch transcription: run the same Silero-VAD segmentation + Whisper
/// path as the live engine over a 16 kHz mono PCM buffer (no microphone).
/// Returns one transcript per detected utterance. Useful for transcribing
/// recorded clips and for integration tests.
pub fn transcribe_pcm_16k(pcm16k: &[f32], language: &str) -> Result<Vec<String>> {
    let mut runner = build_runner(skill_constants::WHISPER_ASR_HF_REPO, language)?;
    let mut session = SileroSession::new(SileroWeights::embedded(), SileroConfig::default());
    let frame_len = session.frame_samples();
    let mut seg = Segmenter::new(SegmenterConfig::defaults_16k(frame_len));

    let mut out = Vec::new();
    let mut i = 0;
    while i + frame_len <= pcm16k.len() {
        let frame = &pcm16k[i..i + frame_len];
        let prob = session.predict_frame(frame).unwrap_or(0.0);
        for ev in seg.push_vad_frame(prob, frame) {
            match ev {
                SegEvent::SpeechEnd => session.reset(),
                SegEvent::Utterance(buf) => push_transcript(&mut runner, &buf, &mut out),
                SegEvent::SpeechStart => {}
            }
        }
        i += frame_len;
    }
    if let Some(buf) = seg.flush() {
        push_transcript(&mut runner, &buf, &mut out);
    }
    Ok(out)
}

/// Load a mono WAV (any sample rate; resampled to 16 kHz) and [`transcribe_pcm_16k`].
pub fn transcribe_wav(path: &Path, language: &str) -> Result<Vec<String>> {
    let (sr, pcm) = rlx_vad::load_wav_mono_f32(path).with_context(|| format!("load wav {}", path.display()))?;
    let pcm16k = if sr == TARGET_HZ {
        pcm
    } else {
        resample_linear(&pcm, sr, TARGET_HZ)
    };
    transcribe_pcm_16k(&pcm16k, language)
}

fn push_transcript(runner: &mut WhisperRunner, pcm: &[f32], out: &mut Vec<String>) {
    if let Ok(text) = runner.transcribe_greedy(pcm) {
        let t = text.trim().to_string();
        if !t.is_empty() {
            out.push(t);
        }
    }
}

/// Whisper inference device. Prefers Metal when the backend is compiled in and
/// available (Apple Silicon) — rlx-whisper runs the mel encoder on the GPU and
/// keeps the decoder on CPU. Falls back to CPU otherwise. `SKILL_ASR_DEVICE=cpu`
/// forces CPU. Safe regardless of features: `is_available` returns false when
/// the Metal backend isn't linked.
fn asr_device() -> Device {
    if std::env::var("SKILL_ASR_DEVICE")
        .map(|v| v.eq_ignore_ascii_case("cpu"))
        .unwrap_or(false)
    {
        return Device::Cpu;
    }
    if rlx_runtime::device_ext::is_available(Device::Metal) {
        Device::Metal
    } else {
        Device::Cpu
    }
}

fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: cpal::SampleFormat,
    channels: usize,
    tx: mpsc::Sender<Vec<f32>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream> {
    use cpal::SampleFormat as SF;
    let stream = match format {
        SF::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let _ = tx.send(downmix(data, channels));
            },
            err_fn,
            None,
        )?,
        SF::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let f: Vec<f32> = data.iter().map(|s| *s as f32 / 32768.0).collect();
                let _ = tx.send(downmix(&f, channels));
            },
            err_fn,
            None,
        )?,
        SF::U16 => device.build_input_stream(
            config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                let f: Vec<f32> = data.iter().map(|s| (*s as f32 - 32768.0) / 32768.0).collect();
                let _ = tx.send(downmix(&f, channels));
            },
            err_fn,
            None,
        )?,
        other => anyhow::bail!("unsupported input sample format: {other:?}"),
    };
    Ok(stream)
}

/// Ensure the Whisper checkpoint is present (download via hf-hub on first use)
/// and return the directory holding `model.safetensors` + `config.json` +
/// `tokenizer.json`.
fn ensure_whisper_model(model_repo: &str) -> Result<PathBuf> {
    use hf_hub::api::sync::ApiBuilder;

    // Explicit override for offline/dev installs.
    if let Ok(dir) = std::env::var("SKILL_ASR_WHISPER_DIR") {
        let p = PathBuf::from(dir);
        if p.join("model.safetensors").is_file() && p.join("tokenizer.json").is_file() {
            return Ok(p);
        }
    }

    let repo_id = if model_repo.trim().is_empty() {
        skill_constants::WHISPER_ASR_HF_REPO
    } else {
        model_repo.trim()
    };
    let cache = crate::skill_dir().join("models/whisper/hf-cache");
    std::fs::create_dir_all(&cache).ok();
    let api = ApiBuilder::new()
        .with_cache_dir(cache)
        .build()
        .context("init hf-hub api")?;
    let repo = api.model(repo_id.to_string());

    let model = repo.get("model.safetensors").context("download model.safetensors")?;
    let _ = repo.get("config.json").context("download config.json")?;
    let _ = repo.get("tokenizer.json").context("download tokenizer.json")?;

    model
        .parent()
        .map(PathBuf::from)
        .context("resolve Whisper model directory")
}

/// Ensure a Qwen3-ASR checkpoint dir is present (downloads safetensors + config
/// + tokenizer on first use). When `model` isn't a Qwen3-ASR repo (e.g. it's
/// still the Whisper default), use `Qwen/Qwen3-ASR-0.6B`.
fn ensure_qwen3_asr_model(model: &str) -> Result<PathBuf> {
    let repo = if model.to_ascii_lowercase().contains("qwen3-asr") {
        model.trim()
    } else {
        rlx_qwen3_asr::HF_MODEL_ID_0_6B
    };
    ensure_hf_model_dir(repo, "models/qwen3-asr/hf-cache")
}

/// Ensure a Voxtral checkpoint dir is present. When `model` isn't a Voxtral repo,
/// use `mistralai/Voxtral-Mini-3B-2507`.
fn ensure_voxtral_model(model: &str) -> Result<PathBuf> {
    let repo = if model.to_ascii_lowercase().contains("voxtral") {
        model.trim()
    } else {
        rlx_voxtral::HF_MODEL_ID_MINI_3B
    };
    ensure_hf_model_dir(repo, "models/voxtral/hf-cache")
}

/// Write 16 kHz mono f32 PCM as a 16-bit WAV (for engines that only take a file).
fn write_wav_16k(path: &Path, pcm: &[f32]) -> Result<()> {
    let rate = TARGET_HZ as u32;
    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + pcm.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in pcm {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(path, out).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Download (or reuse) the config/tokenizer/safetensors files for a HuggingFace
/// model repo and return the snapshot directory. Used by multi-file engines
/// (Whisper uses its own 3-file fetch).
fn ensure_hf_model_dir(repo_id: &str, cache_subdir: &str) -> Result<PathBuf> {
    use hf_hub::api::sync::ApiBuilder;

    if let Ok(dir) = std::env::var("SKILL_ASR_MODEL_DIR") {
        let p = PathBuf::from(dir);
        if p.join("config.json").is_file() {
            return Ok(p);
        }
    }

    let cache = crate::skill_dir().join(cache_subdir);
    std::fs::create_dir_all(&cache).ok();
    let api = ApiBuilder::new()
        .with_cache_dir(cache)
        .build()
        .context("init hf-hub api")?;
    let repo = api.model(repo_id.to_string());
    let info = repo.info().with_context(|| format!("hf repo info {repo_id}"))?;

    let mut dir: Option<PathBuf> = None;
    for sib in info.siblings {
        let f = sib.rfilename;
        let keep = f.ends_with(".json") || f.ends_with(".txt") || f.ends_with(".model") || f.ends_with(".safetensors");
        if keep {
            let path = repo.get(&f).with_context(|| format!("download {f}"))?;
            dir.get_or_insert(path);
        }
    }
    dir.and_then(|p| p.parent().map(PathBuf::from))
        .with_context(|| format!("no model files in {repo_id}"))
}
