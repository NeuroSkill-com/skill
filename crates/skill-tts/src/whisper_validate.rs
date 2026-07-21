// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Optional Whisper ASR gate for synthesized PCM (feature `whisper-validate`).

use std::path::{Path, PathBuf};

use anyhow::Context;
use rlx_runtime::Device;
use rlx_whisper::SAMPLE_RATE as WHISPER_RATE;

/// Resample mono PCM with linear interpolation.
pub fn resample_linear(samples: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
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

/// Minimum expected speech duration from reference text (~0.42 s/word, floor 0.85 s).
pub fn min_expected_speech_secs(text: &str) -> f32 {
    let words = text.split_whitespace().count().max(1) as f32;
    (words * 0.42).max(0.85)
}

/// Fraction of reference words that must appear in the Whisper transcript.
pub fn transcript_covers_reference(reference: &str, transcript: &str, min_ratio: f32) -> bool {
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

/// Transcribe mono PCM and assert it covers `reference_text` (word overlap).
pub fn validate_pcm_with_whisper(
    pcm: &[f32],
    sample_rate: u32,
    reference_text: &str,
    min_word_ratio: f32,
) -> anyhow::Result<String> {
    use rlx_whisper::WhisperRunner;

    let whisper_dir = whisper_weights_dir().with_context(|| {
        "Whisper weights not found for ASR validation.\n\
         Set RLX_WHISPER_DIR or run `just fetch-whisper` in rlx-models."
    })?;

    let pcm_16k = resample_linear(pcm, sample_rate, WHISPER_RATE as u32);

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
        transcript_covers_reference(reference_text, &transcript, min_word_ratio),
        "Whisper transcript missed reference text.\n\
         reference: {reference_text:?}\n\
         heard:     {transcript:?}"
    );
    Ok(transcript)
}

/// Read a mono WAV file and run [`validate_pcm_with_whisper`].
pub fn validate_wav_with_whisper(wav: &Path, reference_text: &str, min_word_ratio: f32) -> anyhow::Result<String> {
    let reader = hound::WavReader::open(wav).with_context(|| format!("open WAV {}", wav.display()))?;
    let spec = reader.spec();
    let pcm: Vec<f32> = reader
        .into_samples::<i16>()
        .map(|s| s.unwrap_or(0) as f32 / i16::MAX as f32)
        .collect();
    validate_pcm_with_whisper(&pcm, spec.sample_rate, reference_text, min_word_ratio)
}
