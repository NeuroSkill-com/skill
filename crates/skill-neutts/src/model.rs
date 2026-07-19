// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! NeuTTS model wrapper — GGUF backbone + NeuCodec decoder, voice-cloning.
//!
//! Wraps [`rlx_neutts::NeuTTS`] and adds the utility layer that
//! `rlx_neutts 0.2.0` does not yet include: NPY reference-code I/O,
//! WAV writing, and plain-text inference via espeak phonemisation.

use std::path::Path;

use anyhow::{Context, Result};
use rlx_neutts::{GenerationConfig, NeuCodecDecoder, NeuCodecEncoder, NeuTTS as RlxNeuTTS, SAMPLE_RATE};

use crate::npy;

pub struct NeuTTS {
    pub(crate) inner: RlxNeuTTS,
}

impl NeuTTS {
    pub fn load_with_decoder(backbone_path: &Path, decoder_path: &Path, language: &str) -> Result<Self> {
        Ok(Self {
            inner: RlxNeuTTS::load_with_decoder(backbone_path, decoder_path, language)?,
        })
    }

    pub fn load(backbone_path: &Path, language: &str) -> Result<Self> {
        Ok(Self {
            inner: RlxNeuTTS::load(backbone_path, language)?,
        })
    }

    pub fn codec(&self) -> &NeuCodecDecoder {
        &self.inner.codec
    }

    pub fn language(&self) -> &str {
        &self.inner.language
    }

    pub fn config(&self) -> &GenerationConfig {
        &self.inner.config
    }

    pub fn set_config(&mut self, config: GenerationConfig) {
        self.inner.config = config;
    }

    pub fn load_ref_codes(&self, path: &Path) -> Result<Vec<i32>> {
        npy::load_npy_i32(path).with_context(|| format!("Failed to load reference codes: {}", path.display()))
    }

    pub fn load_ref_codes_from_bytes(&self, bytes: &[u8]) -> Result<Vec<i32>> {
        npy::parse_npy(bytes)
            .context("Failed to parse embedded NPY reference codes")?
            .into_i32()
            .context("Failed to convert embedded NPY to i32")
    }

    pub fn encode_reference(&self, wav_path: &Path, encoder: &mut NeuCodecEncoder) -> Result<Vec<i32>> {
        encoder
            .encode_wav(wav_path)
            .with_context(|| format!("Failed to encode reference audio: {}", wav_path.display()))
    }

    pub fn save_ref_codes(&self, codes: &[i32], path: &Path) -> Result<()> {
        npy::write_npy_i32(path, codes).with_context(|| format!("Failed to save reference codes: {}", path.display()))
    }

    /// Synthesise speech from plain text using espeak-ng phonemisation.
    ///
    /// Requires the `espeak` Cargo feature.  Returns an error when the feature
    /// is disabled — use [`infer_from_ipa`](Self::infer_from_ipa) to bypass
    /// phonemisation.
    pub fn infer(&self, text: &str, ref_codes: &[i32], ref_text: &str) -> Result<Vec<f32>> {
        #[cfg(feature = "espeak")]
        {
            let ref_phones =
                crate::phonemize::phonemize(ref_text, self.language()).context("Phonemisation of ref_text failed")?;
            let input_phones =
                crate::phonemize::phonemize(text, self.language()).context("Phonemisation of input text failed")?;
            return self.infer_from_ipa(&input_phones, ref_codes, &ref_phones);
        }
        #[cfg(not(feature = "espeak"))]
        {
            let _ = (text, ref_codes, ref_text);
            anyhow::bail!(
                "NeuTTS::infer requires the `espeak` Cargo feature.\n\
                 Enable it or use NeuTTS::infer_from_ipa() to bypass phonemisation."
            )
        }
    }

    pub fn infer_from_ipa(&self, input_ipa: &str, ref_codes: &[i32], ref_ipa: &str) -> Result<Vec<f32>> {
        self.inner.infer_from_ipa(input_ipa, ref_codes, ref_ipa)
    }

    pub fn decode_tokens(&self, speech_ids: &[i32]) -> Result<Vec<f32>> {
        self.inner.decode_tokens(speech_ids)
    }

    /// Write `audio` (f32 PCM, 24 kHz mono) to a 16-bit WAV file.
    pub fn write_wav(&self, audio: &[f32], output_path: &Path) -> Result<()> {
        let peak = audio.iter().map(|&s| s.abs()).fold(0.0f32, f32::max);
        let scale = if peak > 1.0 { 1.0 / peak } else { 1.0 };

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(output_path, spec)
            .with_context(|| format!("Cannot create WAV: {}", output_path.display()))?;
        for &s in audio {
            let s16 = (s * scale * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            writer.write_sample(s16).context("WAV write error")?;
        }
        writer.finalize().context("WAV finalise error")
    }

    /// Encode `audio` to in-memory 16-bit PCM WAV bytes.
    pub fn to_wav_bytes(&self, audio: &[f32]) -> Vec<u8> {
        let peak = audio.iter().map(|&s| s.abs()).fold(0.0f32, f32::max);
        let scale = if peak > 1.0 { 1.0 / peak } else { 1.0 };

        let num_channels: u16 = 1;
        let bits_per_sample: u16 = 16;
        let sample_rate: u32 = SAMPLE_RATE;
        let byte_rate: u32 = sample_rate * num_channels as u32 * bits_per_sample as u32 / 8;
        let block_align: u16 = num_channels * bits_per_sample / 8;
        let data_size: u32 = (audio.len() * 2) as u32;

        let mut buf = Vec::with_capacity(44 + audio.len() * 2);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(36 + data_size).to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&num_channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&bits_per_sample.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        for &s in audio {
            let s16 = (s * scale * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            buf.extend_from_slice(&s16.to_le_bytes());
        }
        buf
    }
}
