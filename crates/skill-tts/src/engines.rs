// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Pluggable RLX TTS engines behind a single dedicated worker thread.
//!
//! Mirrors `kitten.rs` (worker + command channel + rodio playback) but dispatches
//! across engines via the [`Synthesizer`] trait, selected at runtime by
//! [`crate::active_engine`]. Qwen3-TTS, Orpheus, and Kyutai-TTS route through
//! patched `rlx-models` crates (see workspace `[patch.crates-io]`).
//!   * Orpheus needs a pre-exported SNAC decoder (`scripts/export_snac_decoder.py`).
//!   * Kyutai downloads `kyutai/tts-1.6b-en_fr` (~4 GB) on first use.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use rlx_runtime::Device;
use tokio::sync::oneshot;

use crate::{play_f32_audio, skill_dir};

/// `true` once a synthesizer has been built and is ready to speak.
pub static READY: AtomicBool = AtomicBool::new(false);
/// `true` while a synthesizer is being constructed.
pub static LOADING: AtomicBool = AtomicBool::new(false);

// ─── Engine abstraction ─────────────────────────────────────────────────────────

/// A loaded TTS backend: turns text into mono PCM plus its sample rate.
trait Synthesizer: Send {
    fn synthesize(&mut self, text: &str, voice: &str) -> Result<(Vec<f32>, u32)>;
}

/// Preset voices to surface in the UI for `engine` (empty for engines with no
/// fixed preset list).
pub fn voices_for(engine: &str) -> Vec<String> {
    match engine.trim().to_ascii_lowercase().as_str() {
        "qwen3-tts" | "qwen3_tts" => rlx_qwen3_tts::PRESET_SPEAKERS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        "orpheus" => rlx_orpheus::VOICES.iter().map(|s| (*s).to_string()).collect(),
        _ => Vec::new(),
    }
}

/// Preset voices for the currently active engine.
pub fn voices() -> Vec<String> {
    voices_for(&crate::active_engine().0)
}

// ─── Qwen3-TTS ──────────────────────────────────────────────────────────────────

struct Qwen3Synth {
    runner: rlx_qwen3_tts::Qwen3TtsRunner,
}

impl Synthesizer for Qwen3Synth {
    fn synthesize(&mut self, text: &str, voice: &str) -> Result<(Vec<f32>, u32)> {
        let speaker = if voice.trim().is_empty() {
            skill_constants::QWEN3_TTS_VOICE_DEFAULT
        } else {
            voice.trim()
        };
        // The runner only writes a WAV file, so round-trip through a temp file to
        // recover the PCM the worker plays.
        let tmp = std::env::temp_dir().join(format!("skill-tts-qwen3-{}.wav", std::process::id()));
        let synth = self.runner.synthesize_custom_voice(text, speaker, "English", &tmp);
        let pcm = synth.and_then(|()| read_wav_mono_f32(&tmp));
        let _ = std::fs::remove_file(&tmp);
        Ok((
            pcm.context("qwen3-tts synthesize")?,
            skill_constants::QWEN3_TTS_SAMPLE_RATE,
        ))
    }
}

// ─── Orpheus ────────────────────────────────────────────────────────────────────

struct OrpheusSynth {
    tts: rlx_orpheus::OrpheusTts,
}

impl Synthesizer for OrpheusSynth {
    fn synthesize(&mut self, text: &str, voice: &str) -> Result<(Vec<f32>, u32)> {
        let v = if voice.trim().is_empty() {
            skill_constants::ORPHEUS_VOICE_DEFAULT
        } else {
            voice.trim()
        };
        let saved = self.tts.config.clone();
        self.tts.config.max_new_tokens = orpheus_max_tokens_for_text(text, saved.max_new_tokens);
        self.tts.config.greedy = orpheus_use_greedy();
        if self.tts.config.greedy {
            self.tts.config.temperature = 0.0;
        }
        let mut res = self.tts.synthesize(text, Some(v)).context("orpheus synthesize")?;
        self.tts.config = saved;
        rlx_orpheus::normalize_pcm_peak(&mut res.samples);
        Ok((res.samples, res.sample_rate))
    }
}

// ─── Kyutai-TTS ─────────────────────────────────────────────────────────────────

struct KyutaiSynth {
    session: rlx_kyutai_tts::KyutaiTtsSession,
    max_steps_ceiling: usize,
}

impl Synthesizer for KyutaiSynth {
    fn synthesize(&mut self, text: &str, _voice: &str) -> Result<(Vec<f32>, u32)> {
        let cfg = kyutai_generation_config(text, self.max_steps_ceiling);
        let result = self.session.generate(text, &cfg).context("kyutai-tts synthesize")?;
        Ok((result.samples, result.sample_rate))
    }
}

// ─── Inflect-Nano ─────────────────────────────────────────────────────────────
//
// Inflect-Nano-v1: FastSpeech-style acoustic + Snake HiFi-GAN vocoder, single
// English speaker. Loaded from a one-time exported bundle (no Hub download).

struct InflectNanoSynth {
    model: rlx_inflect_nano::InflectNano,
    device: Device,
}

impl Synthesizer for InflectNanoSynth {
    fn synthesize(&mut self, text: &str, _voice: &str) -> Result<(Vec<f32>, u32)> {
        let opts = rlx_inflect_nano::InferOpts::default();
        let wav = self
            .model
            .synthesize_on(text, &opts, self.device)
            .context("inflect-nano synthesize")?;
        Ok((wav.samples, wav.sample_rate))
    }
}

// ─── TinyTTS ──────────────────────────────────────────────────────────────────
//
// TinyTTS (MeloTTS / VITS2, 44.1 kHz), single English speaker. Loaded from a
// one-time exported bundle (`config.json` + `onnx/` + `frontend/`).

struct TinyTtsSynth {
    model: rlx_tiny_tts::TinyTts,
    device: Device,
}

impl Synthesizer for TinyTtsSynth {
    fn synthesize(&mut self, text: &str, _voice: &str) -> Result<(Vec<f32>, u32)> {
        let opts = rlx_tiny_tts::InferOpts::from_config(self.model.config());
        let wav = self
            .model
            .synthesize_on(text, self.device, &opts)
            .context("tiny-tts synthesize")?;
        Ok((wav.samples, wav.sample_rate))
    }
}

// ─── Construction ───────────────────────────────────────────────────────────────

/// Pick the inference device for Qwen3-TTS and other RLX engines.
///
/// `SKILL_TTS_DEVICE` overrides (`cpu`, `cuda`, `gpu`, `metal`, `auto`, …).
/// Default: first available accelerator (CUDA → wgpu → Metal → CPU).
fn resolve_device() -> Device {
    skill_tts_device_override().unwrap_or_else(rlx_orpheus::preferred_synth_device)
}

/// `SKILL_TTS_DEVICE` override shared by Qwen3-TTS and Orpheus.
fn skill_tts_device_override() -> Option<Device> {
    let v = std::env::var("SKILL_TTS_DEVICE").ok()?;
    let v = v.trim();
    if v.is_empty() || v.eq_ignore_ascii_case("auto") {
        return None;
    }
    if v.eq_ignore_ascii_case("cpu") {
        return Some(Device::Cpu);
    }
    rlx_orpheus::resolve_orpheus_device(v).ok().map(|rt| rt.lm)
}

/// Build the backend for `engine`, honouring an optional `model` repo override.
fn build_synthesizer(engine: &str, model: &str) -> Result<Box<dyn Synthesizer>> {
    match engine.trim().to_ascii_lowercase().as_str() {
        "qwen3-tts" | "qwen3_tts" => {
            let repo = if model.trim().is_empty() {
                skill_constants::QWEN3_TTS_HF_REPO
            } else {
                model.trim()
            };
            let dir = ensure_hf_model_dir(repo, "models/qwen3-tts/hf-cache")?;
            // The text tokenizer ships in a separate repo; the loader expects
            // `tokenizer.json` alongside the weights, so fetch it if absent.
            ensure_qwen3_tokenizer(&dir)?;
            let runner = rlx_qwen3_tts::Qwen3TtsRunner::builder()
                .model_dir(dir)
                .device(resolve_device())
                .build()
                .context("build Qwen3-TTS runner")?;
            Ok(Box::new(Qwen3Synth { runner }))
        }
        "orpheus" => {
            apply_orpheus_tts_env_defaults();
            let (gguf, snac) = ensure_orpheus_models()?;
            let runtime = orpheus_runtime_device();
            let mut tts =
                rlx_orpheus::OrpheusTts::load_on_with(&gguf, &snac, runtime, orpheus_backbone_opts(runtime.lm))
                    .with_context(|| format!("load Orpheus on {:?}", runtime.lm))?;
            tts.config = orpheus_generation_config();
            orpheus_warmup(&mut tts);
            tts_log!("tts", "orpheus ready on {:?} quant={}", runtime.lm, orpheus_quant());
            Ok(Box::new(OrpheusSynth { tts }))
        }
        "kyutai-tts" | "kyutai" => {
            let device = kyutai_device();
            let (tts_dir, mimi_dir) = ensure_kyutai_models()?;
            let session = rlx_kyutai_tts::KyutaiTtsSession::open_on(&tts_dir, &mimi_dir, device)
                .context("open Kyutai TTS session")?;
            let max_steps_ceiling = kyutai_max_steps_ceiling();
            tts_log!("tts", "kyutai-tts ready on {:?}", device);
            Ok(Box::new(KyutaiSynth {
                session,
                max_steps_ceiling,
            }))
        }
        "inflect-nano" | "inflect_nano" => {
            let dir = resolve_bundle_dir(
                model,
                "INFLECT_NANO_DIR",
                "inflect-nano",
                "Export it once with rlx-inflect-nano's scripts/export_inflect_nano.py.",
            )?;
            let synth = rlx_inflect_nano::InflectNano::load_from_dir(&dir).context("load Inflect-Nano")?;
            let device = resolve_device();
            tts_log!("tts", "inflect-nano ready on {:?} ({})", device, dir.display());
            Ok(Box::new(InflectNanoSynth { model: synth, device }))
        }
        "tiny-tts" | "tiny_tts" => {
            let dir = resolve_bundle_dir(
                model,
                "TINY_TTS_DIR",
                "tiny-tts",
                "Export it once with rlx-tiny-tts's scripts/export_tiny_tts.py.",
            )?;
            let synth = rlx_tiny_tts::TinyTts::load_from_dir(&dir).context("load TinyTTS")?;
            let device = resolve_device();
            tts_log!("tts", "tiny-tts ready on {:?} ({})", device, dir.display());
            Ok(Box::new(TinyTtsSynth { model: synth, device }))
        }
        other => anyhow::bail!("unknown TTS engine: {other}"),
    }
}

/// Resolve a pre-exported model bundle directory for engines with no Hub download
/// (Inflect-Nano, TinyTTS). Order:
///   1. the engine's `model` override (an explicit bundle path),
///   2. the app-bundled resource dir (`<resource_dir>/tts/<subdir>`),
///   3. `$env_var` (`INFLECT_NANO_DIR` / `TINY_TTS_DIR`),
///   4. `<skill_dir>/models/<subdir>`.
///
/// A valid bundle contains a `config.json`. Bails with export guidance when none
/// is found.
fn resolve_bundle_dir(model: &str, env_var: &str, subdir: &str, export_hint: &str) -> Result<PathBuf> {
    let is_bundle = |p: &Path| p.join("config.json").is_file();

    let model = model.trim();
    if !model.is_empty() {
        let p = PathBuf::from(model);
        if is_bundle(&p) {
            return Ok(p);
        }
    }
    // Shipped inside the app bundle (Tauri `resources/tts/<subdir>`).
    if let Some(res) = crate::tts_resource_dir() {
        let p = res.join("tts").join(subdir);
        if is_bundle(&p) {
            return Ok(p);
        }
    }
    if let Ok(v) = std::env::var(env_var) {
        let p = PathBuf::from(v.trim());
        if is_bundle(&p) {
            return Ok(p);
        }
    }
    let dir = skill_dir().join("models").join(subdir);
    if is_bundle(&dir) {
        return Ok(dir);
    }
    anyhow::bail!(
        "{subdir} bundle not found. It ships in the app bundle; for a dev run place the exported \
         bundle at {}, or set {env_var} to its directory. {export_hint}",
        dir.display()
    )
}

// ─── HF download ────────────────────────────────────────────────────────────────

/// Download every sibling of `repo_id` into `<skill_dir>/<cache_subdir>` and return
/// the resulting snapshot directory. Idempotent — hf-hub caches by blob hash.
fn ensure_hf_model_dir(repo_id: &str, cache_subdir: &str) -> Result<PathBuf> {
    use hf_hub::api::sync::ApiBuilder;

    let cache = skill_dir().join(cache_subdir);
    std::fs::create_dir_all(&cache).ok();

    let api = ApiBuilder::new()
        .with_cache_dir(cache)
        .build()
        .context("init hf-hub api")?;
    let repo = api.model(repo_id.to_string());
    let info = repo.info().with_context(|| format!("hf info for {repo_id}"))?;

    let mut dir: Option<PathBuf> = None;
    for sib in &info.siblings {
        let path = repo
            .get(&sib.rfilename)
            .with_context(|| format!("download {repo_id}/{}", sib.rfilename))?;
        if dir.is_none() {
            dir = path.parent().map(Path::to_path_buf);
        }
    }
    dir.ok_or_else(|| anyhow::anyhow!("no files in {repo_id}"))
}

/// The Qwen3-TTS weights repo ships split BPE files (`vocab.json` + `merges.txt`)
/// but no consolidated `tokenizer.json`, which `rlx-qwen3-tts` requires. The talker
/// is a Qwen3-0.6B backbone whose `tokenizer.json` has byte-identical vocab + merge
/// rules, so we source it from there and graft on the model's own special tokens.
const QWEN3_TTS_TOKENIZER_SRC_REPO: &str = "Qwen/Qwen3-0.6B";

/// Ensure `tokenizer.json` is present in the Qwen3-TTS model dir.
///
/// The weights repo lacks it; we build it from the matching Qwen3 base
/// `tokenizer.json` (proven-identical vocab + merges) plus the model's own
/// `added_tokens_decoder` (so TTS control tokens like `<tts_text_bos>` encode as
/// single ids).
fn ensure_qwen3_tokenizer(model_dir: &Path) -> Result<()> {
    let dest = model_dir.join("tokenizer.json");
    if dest.exists() {
        return Ok(());
    }

    let base_path = fetch_hf_file(
        QWEN3_TTS_TOKENIZER_SRC_REPO,
        "tokenizer.json",
        "models/qwen3-tts/tok-cache",
    )?;
    let mut tj: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&base_path)?).context("parse base tokenizer.json")?;

    let cfg_path = model_dir.join("tokenizer_config.json");
    if cfg_path.exists() {
        graft_added_tokens(&mut tj, &cfg_path)?;
    }

    let bytes = serde_json::to_vec_pretty(&tj).context("serialize tokenizer.json")?;
    std::fs::write(&dest, bytes).with_context(|| format!("write {}", dest.display()))?;
    Ok(())
}

/// Replace a tokenizer.json `added_tokens` array with the authoritative set from a
/// `tokenizer_config.json`'s `added_tokens_decoder` (keeps the model's exact ids +
/// flags, including TTS-specific control tokens).
fn graft_added_tokens(tj: &mut serde_json::Value, cfg_path: &Path) -> Result<()> {
    let cfg: serde_json::Value =
        serde_json::from_slice(&std::fs::read(cfg_path)?).context("parse tokenizer_config.json")?;
    let Some(decoder) = cfg.get("added_tokens_decoder").and_then(|v| v.as_object()) else {
        return Ok(());
    };
    let mut tokens: Vec<serde_json::Value> = Vec::with_capacity(decoder.len());
    for (id_str, obj) in decoder {
        let Ok(id) = id_str.parse::<u64>() else { continue };
        let content = obj.get("content").and_then(|v| v.as_str()).unwrap_or_default();
        let flag = |k: &str, d: bool| obj.get(k).and_then(serde_json::Value::as_bool).unwrap_or(d);
        tokens.push(serde_json::json!({
            "id": id,
            "content": content,
            "single_word": flag("single_word", false),
            "lstrip": flag("lstrip", false),
            "rstrip": flag("rstrip", false),
            "normalized": flag("normalized", false),
            "special": flag("special", true),
        }));
    }
    tokens.sort_by_key(|t| t.get("id").and_then(serde_json::Value::as_u64).unwrap_or(0));
    if let Some(obj) = tj.as_object_mut() {
        obj.insert("added_tokens".into(), serde_json::Value::Array(tokens));
    }
    Ok(())
}

/// Download a single file from an HF repo into `<skill_dir>/<cache_subdir>` and
/// return the local path.
fn fetch_hf_file(repo_id: &str, filename: &str, cache_subdir: &str) -> Result<PathBuf> {
    use hf_hub::api::sync::ApiBuilder;
    let cache = skill_dir().join(cache_subdir);
    std::fs::create_dir_all(&cache).ok();
    let api = ApiBuilder::new()
        .with_cache_dir(cache)
        .build()
        .context("init hf-hub api")?;
    api.model(repo_id.to_string())
        .get(filename)
        .with_context(|| format!("download {repo_id}/{filename}"))
}

/// Default Orpheus GGUF quant — Q8_0 is portable; on Metal with RAM, Q4_K_M is the
/// fast native packed path (override with `ORPHEUS_QUANT=F16` or `Q8_0`).
const ORPHEUS_QUANT_DEFAULT: &str = "Q8_0";

/// Chat-length generation cap (speech tokens). Override with `ORPHEUS_MAX_NEW_TOKENS`.
const ORPHEUS_CHAT_MAX_TOKENS: u32 = 384;

/// LM steps budget: ~5 SNAC frames per spoken word (7 interleaved tokens per frame).
const ORPHEUS_TOKENS_PER_WORD: u32 = 35;
/// Prompt tail + first frames before steady speech.
const ORPHEUS_TOKEN_OVERHEAD: u32 = 49;
/// Minimum LM steps (~12 SNAC frames ≈ 1.0 s at 24 kHz).
const ORPHEUS_TOKEN_FLOOR: u32 = 84;

fn set_env_if_unset(key: &str, val: &str) {
    if std::env::var(key).is_err() {
        // SAFETY: engine worker is single-threaded during init.
        unsafe { std::env::set_var(key, val) };
    }
}

/// Quant tag for Hub download (`ORPHEUS_QUANT` overrides).
fn orpheus_quant() -> String {
    std::env::var("ORPHEUS_QUANT")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let dev = skill_tts_device_override().unwrap_or_else(rlx_orpheus::preferred_synth_device);
            if matches!(dev, Device::Metal) && !orpheus_low_mem_mode() {
                return "Q4_K_M".to_string();
            }
            ORPHEUS_QUANT_DEFAULT.to_string()
        })
}

fn orpheus_low_mem_mode() -> bool {
    std::env::var("ORPHEUS_LOW_MEM")
        .ok()
        .is_some_and(|v| matches!(v.as_str(), "1" | "true" | "TRUE"))
}

/// TTS-tuned rlx-orpheus env defaults. Skipped when the user already set the variable.
///
/// Content parity (rlx-orpheus `backends_codes_parity`, `tokens::use_snac_logit_mask_for`):
/// - [`orpheus_backbone_opts`] defaults to [`BackboneLoadOptions::synthesis`] (dynamic decode,
///   no SNAC logit mask on Metal).
/// - `ORPHEUS_FOR_TTS=1` opts into bucket decode via [`BackboneLoadOptions::for_tts`].
fn apply_orpheus_tts_env_defaults() {
    set_env_if_unset("ORPHEUS_BUCKET_DECODE", "0");
    set_env_if_unset("ORPHEUS_MASK_LOGITS", "0");
    set_env_if_unset("ORPHEUS_SNAC_DEVICE", "cpu");
    // Resident KV on by default for Vulkan/Metal/CUDA bucket decode (opt out: ORPHEUS_RESIDENT_KV=0).
    // Drop packed Metal prefill graphs after seeding KV (decode compile needs headroom).
    set_env_if_unset("ORPHEUS_PREFILL_PERSIST", "0");
    let for_tts = std::env::var("ORPHEUS_FOR_TTS")
        .ok()
        .is_some_and(|v| matches!(v.as_str(), "1" | "true" | "TRUE"));
    if for_tts {
        // Packed GPU prefill + dynamic decode peaks past RAM; bucket decode reuses one graph.
        set_env_if_unset("ORPHEUS_BUCKET_DECODE", "1");
        set_env_if_unset("ORPHEUS_COMPILE_SEQ_CAP", "128");
        set_env_if_unset("ORPHEUS_DECODE_CACHE_CAP", "128");
        set_env_if_unset("ORPHEUS_COMPILE_CACHE", "1");
    }
    let dev = skill_tts_device_override().unwrap_or_else(rlx_orpheus::preferred_synth_device);
    match dev {
        Device::Metal => {
            if orpheus_quant().eq_ignore_ascii_case("Q8_0") {
                set_env_if_unset("ORPHEUS_METAL_PREFILL", "cpu");
                set_env_if_unset("RLX_METAL_F32_PREFILL_CPU", "1");
            } else if for_tts {
                set_env_if_unset("ORPHEUS_METAL_PREFILL", "packed");
            }
        }
        // CUDA/ROCm TTS: parity-safe native prefill (CPU F32 reference hidden+KV,
        // mmap weights) + fused Q4 bucketed decode. See rlx-llama32/docs/cuda-gguf-decode.md.
        // Escape hatches: ORPHEUS_CUDA_F32_PREFILL=1 (full CPU prefill).
        Device::Cuda => {
            if for_tts {
                set_env_if_unset("ORPHEUS_CUDA_NATIVE_PREFILL", "1");
            } else {
                set_env_if_unset("ORPHEUS_CUDA_PREFILL", "cpu");
            }
            set_env_if_unset("ORPHEUS_PREFILL_PERSIST", "0");
            set_env_if_unset("RLX_CUDA_ARENA_POOL", "0");
            set_env_if_unset("ORPHEUS_BUCKET_DECODE", "1");
            set_env_if_unset("ORPHEUS_COMPILE_SEQ_CAP", "128");
            set_env_if_unset("ORPHEUS_DECODE_CACHE_CAP", "128");
        }
        Device::Rocm => {
            set_env_if_unset("ORPHEUS_ROCM_PREFILL", "cpu");
            set_env_if_unset("ORPHEUS_PREFILL_PERSIST", "0");
            set_env_if_unset("ORPHEUS_BUCKET_DECODE", "1");
            set_env_if_unset("ORPHEUS_COMPILE_SEQ_CAP", "128");
            set_env_if_unset("ORPHEUS_DECODE_CACHE_CAP", "128");
        }
        _ => {}
    }
}

/// LM load path: synthesis reference (content-correct) unless `ORPHEUS_FOR_TTS=1`.
fn orpheus_backbone_opts(device: Device) -> rlx_orpheus::BackboneLoadOptions {
    if std::env::var("ORPHEUS_FOR_TTS")
        .ok()
        .is_some_and(|v| matches!(v.as_str(), "1" | "true" | "TRUE"))
        || matches!(device, Device::Cuda | Device::Rocm)
    {
        return rlx_orpheus::BackboneLoadOptions::for_tts(device);
    }
    rlx_orpheus::BackboneLoadOptions::synthesis()
}

fn orpheus_generation_config() -> rlx_orpheus::GenerationConfig {
    let ceiling = std::env::var("ORPHEUS_MAX_NEW_TOKENS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(ORPHEUS_CHAT_MAX_TOKENS);
    let mut cfg = rlx_orpheus::GenerationConfig::chat();
    cfg.max_new_tokens = ceiling;
    cfg.greedy = orpheus_use_greedy();
    if cfg.greedy {
        cfg.temperature = 0.0;
    }
    cfg
}

/// Greedy sampling when `ORPHEUS_GREEDY=1`. Default is stochastic (better Metal parity).
fn orpheus_use_greedy() -> bool {
    std::env::var("ORPHEUS_GREEDY")
        .ok()
        .is_some_and(|v| matches!(v.as_str(), "1" | "true" | "TRUE"))
}

/// Minimum expected speech duration from reference text (Orpheus @ 24 kHz).
pub fn orpheus_min_expected_speech_secs(text: &str) -> f32 {
    let words = text.split_whitespace().count().max(1) as f32;
    (words * 0.42).max(0.85)
}

/// Scale `max_new_tokens` to utterance length (upstream allows EOS before the cap).
fn orpheus_max_tokens_for_text(text: &str, ceiling: u32) -> u32 {
    let words = text.split_whitespace().count().max(1) as u32;
    let by_words = words
        .saturating_mul(ORPHEUS_TOKENS_PER_WORD)
        .saturating_add(ORPHEUS_TOKEN_OVERHEAD);
    let by_chars = (text.len() as u32)
        .saturating_mul(6)
        .saturating_add(ORPHEUS_TOKEN_OVERHEAD);
    by_words.max(by_chars).clamp(ORPHEUS_TOKEN_FLOOR, ceiling)
}

const ORPHEUS_WARMUP_MAX_TOKENS: u32 = 36;

fn orpheus_warmup(tts: &mut rlx_orpheus::OrpheusTts) {
    if !orpheus_warmup_enabled() {
        return;
    }
    let saved = tts.config.clone();
    tts.config.max_new_tokens = ORPHEUS_WARMUP_MAX_TOKENS.min(saved.max_new_tokens);
    tts.config.greedy = true;
    if let Err(e) = tts.synthesize(
        orpheus_warmup_text().as_str(),
        Some(skill_constants::ORPHEUS_VOICE_DEFAULT),
    ) {
        tts_log!("tts", "orpheus warmup skipped: {e:#}");
    }
    tts.config = saved;
}

fn orpheus_warmup_enabled() -> bool {
    std::env::var("ORPHEUS_WARMUP")
        .ok()
        .is_some_and(|v| matches!(v.as_str(), "1" | "true" | "TRUE"))
}

fn orpheus_warmup_text() -> String {
    std::env::var("ORPHEUS_WARMUP_TEXT").unwrap_or_else(|_| "Hello.".into())
}

/// Kyutai defaults to CPU eager (stable); override with `SKILL_TTS_DEVICE` / `KYUTAI_DEVICE`.
fn kyutai_device() -> Device {
    if let Some(dev) = skill_tts_device_override() {
        return dev;
    }
    if let Ok(spec) = std::env::var("KYUTAI_DEVICE") {
        if spec.eq_ignore_ascii_case("cpu") {
            return Device::Cpu;
        }
        if let Ok(rt) = rlx_orpheus::resolve_orpheus_device(&spec) {
            return rt.lm;
        }
    }
    // Kyutai eager path is CPU-first; GPU LM is opt-in via SKILL_TTS_DEVICE.
    Device::Cpu
}

const KYUTAI_CHAT_MAX_STEPS: usize = 100;

fn kyutai_max_steps_ceiling() -> usize {
    std::env::var("KYUTAI_MAX_STEPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(KYUTAI_CHAT_MAX_STEPS)
}

fn kyutai_max_steps_for_text(text: &str, ceiling: usize) -> usize {
    let words = text.split_whitespace().count().max(1);
    let by_words = words.saturating_mul(10).saturating_add(25);
    let by_chars = text.len().saturating_mul(3) / 2 + 25;
    by_words.max(by_chars).clamp(25, ceiling)
}

fn kyutai_generation_config(text: &str, ceiling: usize) -> rlx_kyutai_tts::GenerationConfig {
    rlx_kyutai_tts::GenerationConfig {
        max_steps: kyutai_max_steps_for_text(text, ceiling),
        ..rlx_kyutai_tts::GenerationConfig::default()
    }
}

fn ensure_kyutai_models() -> Result<(PathBuf, PathBuf)> {
    let tts_dir = skill_dir().join("models/kyutai-tts");
    std::fs::create_dir_all(&tts_dir)?;
    rlx_kyutai_tts::ensure_weights(&tts_dir).context("fetch Kyutai TTS weights")?;
    Ok((tts_dir, rlx_kyutai_tts::default_mimi_dir()))
}

/// Resolved LM + SNAC targets for Orpheus.
fn orpheus_runtime_device() -> rlx_orpheus::OrpheusRuntimeDevice {
    if let Some(lm) = skill_tts_device_override() {
        return rlx_orpheus::OrpheusRuntimeDevice {
            lm,
            snac: orpheus_snac_exec(lm),
        };
    }
    if let Ok(spec) = std::env::var("ORPHEUS_DEVICE") {
        if let Ok(rt) = rlx_orpheus::resolve_orpheus_device(&spec) {
            return rt;
        }
    }
    let lm = rlx_orpheus::preferred_synth_device();
    rlx_orpheus::OrpheusRuntimeDevice {
        lm,
        snac: orpheus_snac_exec(lm),
    }
}

/// SNAC backend: `ORPHEUS_SNAC_DEVICE` / `default_snac_exec`, with legacy
/// `ORPHEUS_SNAC_COREML=0` forcing CPU eager decode.
fn orpheus_snac_exec(lm: Device) -> rlx_orpheus::SnacExec {
    use rlx_orpheus::SnacExec;
    if std::env::var("ORPHEUS_SNAC_COREML")
        .ok()
        .is_some_and(|v| v == "0" || v.eq_ignore_ascii_case("false"))
    {
        return SnacExec::CpuEager;
    }
    rlx_orpheus::default_snac_exec(lm)
}

/// Resolve the Orpheus backbone GGUF (auto-downloaded) + SNAC decoder safetensors.
///
/// The GGUF backbone fetches from the Hub. The SNAC decoder safetensors are NOT on
/// the Hub (only a PyTorch checkpoint is) and must be exported once — we resolve it
/// from `ORPHEUS_SNAC_PATH` or `<skill_dir>/models/orpheus/snac_24khz_decoder.safetensors`.
fn ensure_orpheus_models() -> Result<(PathBuf, PathBuf)> {
    let quant = orpheus_quant();
    let gguf = resolve_orpheus_gguf(&quant)?;
    let snac = orpheus_snac_path(&skill_dir().join("models/orpheus"))?;
    Ok((gguf, snac))
}

/// Prefer local weights (`ORPHEUS_GGUF_PATH`, `ORPHEUS_WEIGHTS_DIR`, rlx default dir)
/// before hitting the Hub.
fn resolve_orpheus_gguf(quant: &str) -> Result<PathBuf> {
    if let Ok(p) = std::env::var("ORPHEUS_GGUF_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    for dir in [
        std::env::var("ORPHEUS_WEIGHTS_DIR").ok().map(PathBuf::from),
        Some(rlx_orpheus::default_orpheus_dir()),
    ]
    .into_iter()
    .flatten()
    {
        let p = dir.join(rlx_orpheus::orpheus_gguf_filename(quant));
        if p.is_file() {
            tts_log!("tts", "orpheus gguf: {}", p.display());
            return Ok(p);
        }
    }
    let cache = skill_dir().join("models/orpheus/hf-cache");
    let dest = skill_dir().join("models/orpheus");
    std::fs::create_dir_all(&cache).ok();
    std::fs::create_dir_all(&dest).ok();
    rlx_orpheus::fetch_orpheus_gguf(&cache, &dest, quant)
        .with_context(|| format!("download Orpheus GGUF backbone ({quant})"))
}

/// Locate the pre-exported SNAC decoder safetensors.
fn orpheus_snac_path(dest: &Path) -> Result<PathBuf> {
    if let Ok(p) = std::env::var("ORPHEUS_SNAC_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    let cached = dest.join(rlx_orpheus::SNAC_DECODER_SAFETENSORS);
    if cached.is_file() {
        return Ok(cached);
    }
    anyhow::bail!(
        "Orpheus SNAC decoder not found (the snac_24khz safetensors are not on the Hub). \
         Export it once and set ORPHEUS_SNAC_PATH, or place it at {}",
        cached.display()
    )
}

/// Synthesize `text` with the active engine and return mono PCM + sample rate
/// **without** playing it. Builds a fresh synthesizer each call — intended for
/// file export / verification, not the hot speak path.
pub fn synthesize_pcm(text: &str) -> Result<(Vec<f32>, u32)> {
    let (engine, model, voice) = crate::active_engine();
    let mut synth = build_synthesizer(&engine, &model)?;
    synth.synthesize(text, &voice)
}

/// Read a mono/​multi-channel WAV into mono f32 samples (downmixing if needed).
fn read_wav_mono_f32(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path).context("open temp wav")?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|s| s as f32 / scale)
                .collect()
        }
    };

    if channels <= 1 {
        return Ok(interleaved);
    }
    Ok(interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect())
}

// ─── Worker channel ─────────────────────────────────────────────────────────────

pub enum Cmd {
    Init {
        done: oneshot::Sender<Result<()>>,
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
            .name("skill-tts-engines".into())
            .spawn(|| worker(rx))
            .expect("failed to spawn skill-tts engines worker thread");
        tx
    })
}

// ─── Worker ─────────────────────────────────────────────────────────────────────

/// A built synthesizer tagged with the `(engine, model)` it was built for, so the
/// worker rebuilds when the active selection changes.
struct Current {
    engine: String,
    model: String,
    synth: Box<dyn Synthesizer>,
}

/// Ensure `cur` matches the active `(engine, model)`, (re)building if needed.
fn ensure_current(cur: &mut Option<Current>) -> Result<()> {
    let (engine, model, _voice) = crate::active_engine();
    let engine = engine.trim().to_ascii_lowercase();
    let model = model.trim().to_string();

    if let Some(c) = cur {
        if c.engine == engine && c.model == model {
            return Ok(());
        }
    }

    LOADING.store(true, Ordering::Release);
    READY.store(false, Ordering::Release);
    let built = build_synthesizer(&engine, &model);
    LOADING.store(false, Ordering::Release);

    match built {
        Ok(synth) => {
            *cur = Some(Current { engine, model, synth });
            READY.store(true, Ordering::Release);
            Ok(())
        }
        Err(e) => {
            *cur = None;
            Err(e)
        }
    }
}

fn worker(rx: std::sync::mpsc::Receiver<Cmd>) {
    let mut stream: Option<rodio::MixerDeviceSink> = None;
    let mut current: Option<Current> = None;

    for cmd in rx {
        match cmd {
            Cmd::Init { done } => {
                let r = ensure_current(&mut current);
                match &r {
                    Ok(_) => tts_log!("tts", "engine ready ({})", crate::active_engine().0),
                    Err(e) => tts_log!("tts", "engine init failed: {e}"),
                }
                done.send(r).ok();
            }

            Cmd::Speak { text, voice, done } => {
                if let Err(e) = ensure_current(&mut current) {
                    tts_log!("tts", "engine load failed: {e}");
                    done.send(()).ok();
                    continue;
                }
                let voice = if voice.trim().is_empty() {
                    crate::active_engine().2
                } else {
                    voice
                };
                stream = rodio::DeviceSinkBuilder::open_default_sink()
                    .map_err(|e| tts_log!("tts", "could not open audio: {e}"))
                    .ok();
                if let (Some(c), Some(s)) = (current.as_mut(), stream.as_ref()) {
                    match c.synth.synthesize(&text, &voice) {
                        Ok((pcm, sr)) => play_f32_audio(s, pcm, sr),
                        Err(e) => tts_log!("tts", "synthesis error: {e}"),
                    }
                } else {
                    tts_log!("tts", "speak skipped: no audio device");
                }
                done.send(()).ok();
            }

            Cmd::Unload { done } => {
                current = None;
                READY.store(false, Ordering::Release);
                tts_log!("tts", "engine unloaded");
                done.send(()).ok();
            }

            Cmd::Shutdown { done } => {
                drop(stream.take());
                drop(current.take());
                READY.store(false, Ordering::Release);
                tts_log!("tts", "engines shutdown complete");
                done.send(()).ok();
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwen3_exposes_preset_voices() {
        let v = voices_for("qwen3-tts");
        assert!(!v.is_empty(), "qwen3-tts should expose preset speakers");
        assert!(v.iter().any(|s| s == skill_constants::QWEN3_TTS_VOICE_DEFAULT));
        // Engine name matching is case/format-insensitive.
        assert_eq!(voices_for("Qwen3-TTS"), voices_for("qwen3_tts"));
    }

    #[test]
    fn orpheus_exposes_preset_voices() {
        let v = voices_for("orpheus");
        assert_eq!(v.len(), rlx_orpheus::VOICES.len());
        assert!(v.iter().any(|s| s == skill_constants::ORPHEUS_VOICE_DEFAULT));
    }

    #[test]
    fn unknown_engine_has_no_preset_voices() {
        assert!(voices_for("kitten").is_empty());
        assert!(voices_for("nope").is_empty());
    }

    #[test]
    fn kyutai_max_steps_scales_with_text() {
        let short = kyutai_max_steps_for_text("Hi.", 100);
        let long = kyutai_max_steps_for_text("Artificial intelligence will transform the way we live and work.", 100);
        assert!(short < long);
        assert!(long <= 100);
    }

    #[test]
    fn skill_tts_device_override_cpu() {
        let _guard = EnvVarGuard::set("SKILL_TTS_DEVICE", "cpu");
        assert_eq!(skill_tts_device_override(), Some(Device::Cpu));
    }

    #[test]
    fn skill_tts_device_override_auto_is_none() {
        let _guard = EnvVarGuard::set("SKILL_TTS_DEVICE", "auto");
        assert_eq!(skill_tts_device_override(), None);
    }

    #[test]
    fn orpheus_max_tokens_scales_with_text() {
        let short = orpheus_max_tokens_for_text("Hi.", 384);
        let hello = orpheus_max_tokens_for_text("Hello world.", 384);
        let long = orpheus_max_tokens_for_text("Artificial intelligence will transform the way we live and work.", 384);
        assert!(short >= ORPHEUS_TOKEN_FLOOR);
        assert!(hello > short, "hello={hello} short={short}");
        assert!(hello >= 119, "expected >=119 tokens for two words, got {hello}");
        assert!(long > hello);
        assert!(long <= 384);
    }

    #[test]
    fn orpheus_greedy_is_opt_in() {
        let _guard = EnvVarGuard::unset("ORPHEUS_GREEDY");
        assert!(!orpheus_use_greedy());
        let _on = EnvVarGuard::set("ORPHEUS_GREEDY", "1");
        assert!(orpheus_use_greedy());
    }

    #[test]
    fn orpheus_pcm_normalize_boosts_quiet_signal() {
        let mut pcm = vec![0.01f32; 100];
        rlx_orpheus::normalize_pcm_peak(&mut pcm);
        let peak = pcm.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.9, "expected peak near 0.95, got {peak}");
    }

    struct EnvVarGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, val: &str) -> Self {
            let prev = std::env::var(key).ok();
            unsafe { std::env::set_var(key, val) };
            Self { key, prev }
        }

        fn unset(key: &'static str) -> Self {
            let prev = std::env::var(key).ok();
            unsafe { std::env::remove_var(key) };
            Self { key, prev }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }
}
