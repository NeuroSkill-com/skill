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
    let mut transcriber = build_transcriber(&mode.engine, &mode.model, &mode.language, on_event).map_err(|e| {
        on_event(AsrEvent::Error {
            message: format!("ASR engine load failed: {e:#}"),
        });
        e
    })?;

    // ── Silero streaming VAD (weights embedded — no download) ─────────────────
    let mut session = SileroSession::new(SileroWeights::embedded(), SileroConfig::default());
    let frame_len = session.frame_samples();
    let mut seg = Segmenter::new(SegmenterConfig::defaults_16k(frame_len));

    // ── Microphone (cpal owns a non-Send stream; keep it on this thread) ──────
    let host = cpal::default_host();
    let device = host.default_input_device().context(
        "no default audio input device (microphone) — plug in a mic or grant Microphone \
         permission to NeuroSkill / skill-daemon in System Settings → Privacy & Security",
    )?;
    let default_cfg = device.default_input_config().with_context(|| {
        let name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "<unknown>".into());
        format!(
            "cannot open microphone '{name}' (no usable input format). On machines with \
             speakers only this usually means no mic is connected; otherwise check \
             System Settings → Privacy & Security → Microphone"
        )
    })?;
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

struct FunAsrTranscriber(rlx_funasr::pipeline::AsrModel);
impl Transcriber for FunAsrTranscriber {
    fn transcribe(&mut self, pcm16k: &[f32]) -> Result<String> {
        self.0.transcribe(pcm16k)
    }
}

struct NemotronAsrTranscriber(rlx_nemotron_asr::NemotronAsr);
impl Transcriber for NemotronAsrTranscriber {
    fn transcribe(&mut self, pcm16k: &[f32]) -> Result<String> {
        self.0.transcribe(pcm16k)
    }
}

struct RlxAsrTranscriber(rlx_asr::AsrSession);
impl Transcriber for RlxAsrTranscriber {
    fn transcribe(&mut self, pcm16k: &[f32]) -> Result<String> {
        let t = self.0.transcribe(pcm16k, TARGET_HZ as u32)?;
        Ok(t.text)
    }
}

/// Construct the ASR backend for `engine` (`"whisper"` | `"qwen3-asr"` |
/// `"voxtral"` | `"funasr"` | `"nemotron-asr"` | `"rlx-asr"`; unknown falls back
/// to Whisper). `model` is the engine-specific model id (HF repo).
fn build_transcriber(engine: &str, model: &str, language: &str, on_event: &EventSink) -> Result<Box<dyn Transcriber>> {
    let engine = crate::normalize_asr_engine_id(engine);
    let progress = |label: String, downloaded: u64, total: u64| {
        on_event(AsrEvent::Download {
            label,
            downloaded,
            total,
        });
    };
    match engine.as_str() {
        "qwen3-asr" => {
            progress("Qwen3-ASR weights…".into(), 0, 0);
            let dir = ensure_qwen3_asr_model(model)?;
            let runner = rlx_qwen3_asr::AsrRunner::builder()
                .weights(&dir)
                .device(asr_device())
                .build()
                .context("build Qwen3-ASR runner")?;
            Ok(Box::new(Qwen3AsrTranscriber(runner)))
        }
        "voxtral" => {
            progress("Voxtral weights…".into(), 0, 0);
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
        "funasr" => {
            progress("FunASR / SenseVoice weights…".into(), 0, 0);
            let dir = ensure_funasr_model(model)?;
            let asr = rlx_funasr::runner::open_asr(&dir, asr_device()).context("open FunASR model")?;
            Ok(Box::new(FunAsrTranscriber(asr)))
        }
        "nemotron-asr" => {
            progress("Nemotron-ASR .nemo (large download)…".into(), 0, 0);
            let nemo = ensure_nemotron_asr_model(model)?;
            let mut asr = rlx_nemotron_asr::NemotronAsr::open(&nemo, asr_device()).context("open Nemotron ASR")?;
            let lang = match language.trim() {
                "" | "auto" => "en-US",
                l => l,
            };
            let _ = asr.set_language(lang);
            Ok(Box::new(NemotronAsrTranscriber(asr)))
        }
        "rlx-asr" => {
            progress("RLX-ASR pack…".into(), 0, 0);
            let dir = ensure_rlx_asr_model(model)?;
            let session = rlx_asr::AsrSession::load(&dir).context("load RLX-ASR session")?;
            Ok(Box::new(RlxAsrTranscriber(session)))
        }
        _ => {
            progress("Whisper weights…".into(), 0, 0);
            Ok(Box::new(WhisperTranscriber(build_runner(model, language)?)))
        }
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

/// Load (and optionally download) an ASR engine then drop it — for smoke tests.
/// Does not open the microphone.
pub fn smoke_ensure_engine(engine: &str, model: &str) -> Result<()> {
    let sink: EventSink = Arc::new(|_| {});
    let _ = build_transcriber(engine, model, "en", &sink)?;
    Ok(())
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

/// Ensure a Qwen3-ASR checkpoint dir is present (downloads safetensors, config,
/// and tokenizer on first use). When `model` isn't a Qwen3-ASR repo (e.g. it's
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

/// FunASR / SenseVoice / Paraformer: download Hub snapshot into `~/.skill/models/funasr/`.
fn ensure_funasr_model(model: &str) -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("SKILL_ASR_FUNASR_DIR") {
        let p = PathBuf::from(dir);
        if p.join("config.yaml").is_file() {
            return Ok(p);
        }
    }
    let repo = {
        let m = model.trim();
        if m.is_empty() || m.to_ascii_lowercase().contains("whisper") {
            "FunAudioLLM/SenseVoiceSmall"
        } else {
            m
        }
    };
    let dest = crate::skill_dir().join("models/funasr").join(
        repo.rsplit('/')
            .next()
            .unwrap_or("SenseVoiceSmall")
            .replace([' ', ':'], "_"),
    );
    if dest.join("config.yaml").is_file()
        && (dest.join("model.pt").is_file() || dest.join("model.safetensors").is_file())
    {
        return Ok(dest);
    }
    // SenseVoiceSmall needs these; Paraformer-zh uses a similar set.
    let files: &[&str] = if repo.to_ascii_lowercase().contains("paraformer") {
        &["config.yaml", "model.pt", "am.mvn", "tokens.json"]
    } else {
        &[
            "config.yaml",
            "model.pt",
            "chn_jpn_yue_eng_ko_spectok.bpe.model",
            "am.mvn",
        ]
    };
    materialize_named_files(repo, files, &dest, "models/funasr/hf-cache")?;
    Ok(dest)
}

/// Nemotron streaming ASR: download the `.nemo` into `~/.skill/models/nemotron-asr/`.
fn ensure_nemotron_asr_model(model: &str) -> Result<PathBuf> {
    if let Ok(p) = std::env::var("SKILL_ASR_NEMOTRON_NEMO") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Ok(path);
        }
    }
    let repo = {
        let m = model.trim();
        if m.is_empty() || !m.to_ascii_lowercase().contains("nemotron") {
            "nvidia/nemotron-3.5-asr-streaming-0.6b"
        } else {
            m
        }
    };
    let dest_dir = crate::skill_dir().join("models/nemotron-asr");
    let nemo_name = "nemotron-3.5-asr-streaming-0.6b.nemo";
    let nemo_path = dest_dir.join(nemo_name);
    if nemo_path.is_file() {
        return Ok(nemo_path);
    }
    std::fs::create_dir_all(&dest_dir).with_context(|| format!("create {}", dest_dir.display()))?;
    let cache = crate::skill_dir().join("models/nemotron-asr/hf-cache");
    std::fs::create_dir_all(&cache).ok();
    use hf_hub::api::sync::ApiBuilder;
    let api = ApiBuilder::new()
        .with_cache_dir(cache)
        .build()
        .context("init hf-hub api")?;
    let src = api
        .model(repo.to_string())
        .get(nemo_name)
        .with_context(|| format!("download {repo}/{nemo_name}"))?;
    if src != nemo_path {
        std::fs::copy(&src, &nemo_path).with_context(|| format!("copy {} → {}", src.display(), nemo_path.display()))?;
    }
    Ok(nemo_path)
}

/// RLX packed ASR (`model.rlxp` / legacy `model.gguf`) from eugenehp/rlx-asr.
fn ensure_rlx_asr_model(model: &str) -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("RLX_ASR_DIR").or_else(|_| std::env::var("SKILL_ASR_RLX_DIR")) {
        let p = PathBuf::from(dir);
        if p.join(rlx_asr::DEFAULT_RLXP_NAME).is_file()
            || p.join(rlx_asr::DEFAULT_GGUF_NAME).is_file()
            || rlx_asr::resolve_pack_path(&p).is_some()
        {
            return Ok(p);
        }
    }
    let repo = {
        let m = model.trim();
        if m.is_empty() || m.to_ascii_lowercase().contains("whisper") {
            rlx_asr::HF_REPO
        } else {
            m
        }
    };
    let dest = crate::skill_dir().join("models/rlx-asr");
    for name in [rlx_asr::DEFAULT_RLXP_NAME, rlx_asr::DEFAULT_GGUF_NAME] {
        if dest.join(name).is_file() {
            return Ok(dest);
        }
    }
    std::fs::create_dir_all(&dest).with_context(|| format!("create {}", dest.display()))?;
    use hf_hub::api::sync::ApiBuilder;
    let api = ApiBuilder::new()
        .with_cache_dir(crate::skill_dir().join("models/rlx-asr/hf-cache"))
        .build()
        .context("init hf-hub api")?;
    let hf = api.model(repo.to_string());
    let mut last_err: Option<anyhow::Error> = None;
    for name in [rlx_asr::DEFAULT_RLXP_NAME, rlx_asr::DEFAULT_GGUF_NAME] {
        match hf.get(name) {
            Ok(src) => {
                let out = dest.join(name);
                if src != out {
                    std::fs::copy(&src, &out).with_context(|| format!("copy {} → {}", src.display(), out.display()))?;
                }
                return Ok(dest);
            }
            Err(e) => last_err = Some(e.into()),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        anyhow::anyhow!(
            "no pack in {repo} ({}/{})",
            rlx_asr::DEFAULT_RLXP_NAME,
            rlx_asr::DEFAULT_GGUF_NAME
        )
    }))
}

/// Download named files from an HF repo into `dest`, using `cache_subdir` under skill dir.
fn materialize_named_files(repo: &str, files: &[&str], dest: &Path, cache_subdir: &str) -> Result<()> {
    use hf_hub::api::sync::ApiBuilder;
    std::fs::create_dir_all(dest).with_context(|| format!("create {}", dest.display()))?;
    let cache = crate::skill_dir().join(cache_subdir);
    std::fs::create_dir_all(&cache).ok();
    let api = ApiBuilder::new()
        .with_cache_dir(cache)
        .build()
        .context("init hf-hub api")?;
    let hf = api.model(repo.to_string());
    for name in files {
        match hf.get(name) {
            Ok(src) => {
                let out = dest.join(name);
                if let Some(parent) = out.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if src != out {
                    std::fs::copy(&src, &out).with_context(|| format!("copy {} → {}", src.display(), out.display()))?;
                }
            }
            Err(e) => {
                // Optional sidecars (am.mvn / tokens) shouldn't hard-fail SenseVoice.
                if *name == "am.mvn" || *name == "tokens.json" {
                    continue;
                }
                return Err(e).with_context(|| format!("download {repo}/{name}"));
            }
        }
    }
    Ok(())
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
