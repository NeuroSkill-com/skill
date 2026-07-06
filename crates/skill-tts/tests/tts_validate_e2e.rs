// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Synthesize a fixed sentence with each pluggable engine and write WAVs for an
//! **external** Whisper intelligibility check (ignored — downloads models).
//!
//! Orpheus needs a pre-exported SNAC decoder; set `ORPHEUS_SNAC_PATH`:
//!   python3 scripts/export_snac_decoder.py ~/.cache/rlx-orpheus-snac
//!   ORPHEUS_SNAC_PATH=~/.cache/rlx-orpheus-snac/snac_24khz_decoder.safetensors \
//!     cargo test -p skill-tts --features "tts-engines,whisper-validate" \
//!     --test tts_validate_e2e -- --ignored --nocapture
//!
//! With `whisper-validate`, rlx-whisper runs in-process (set `RLX_WHISPER_DIR` or
//! fetch weights under `../rlx-models/.cache/whisper-base.en`).

#![cfg(feature = "tts-engines")]

const DEFAULT_SENTENCE: &str = "Artificial intelligence will transform the way we live and work.";

#[test]
#[ignore = "downloads Qwen3-TTS + Orpheus models and runs inference"]
fn synthesize_engines_for_whisper_validation() {
    let cache = std::env::temp_dir().join("skill-tts-e2e");
    std::fs::create_dir_all(&cache).unwrap();
    skill_tts::set_skill_dir(cache);

    let out_dir = std::env::var("SKILL_TTS_VALIDATE_OUT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("skill-tts-validate"));
    std::fs::create_dir_all(&out_dir).unwrap();

    let wav_name = std::env::var("SKILL_TTS_VALIDATE_WAV_NAME").ok();
    let whisper_soft = std::env::var("SKILL_TTS_WHISPER_SOFT")
        .ok()
        .is_some_and(|v| matches!(v.as_str(), "1" | "true" | "TRUE"));

    let sentence = std::env::var("SKILL_TTS_VALIDATE_TEXT").unwrap_or_else(|_| DEFAULT_SENTENCE.to_string());
    eprintln!("REFERENCE: {sentence}");

    // Default to both; override with SKILL_TTS_VALIDATE_ENGINES=qwen3-tts to scope.
    let engines = std::env::var("SKILL_TTS_VALIDATE_ENGINES").unwrap_or_else(|_| "qwen3-tts,orpheus".into());

    let whisper_min = std::env::var("SKILL_TTS_WHISPER_MIN_RATIO")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.65_f32);

    let min_secs = skill_tts::engines::orpheus_min_expected_speech_secs(&sentence);

    for engine in engines.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        skill_tts::set_active_engine(engine.to_string(), String::new(), String::new());
        let (pcm, sr) = skill_tts::engines::synthesize_pcm(&sentence)
            .unwrap_or_else(|e| panic!("{engine} synthesis failed: {e:#}"));

        let secs = pcm.len() as f32 / sr as f32;
        let rms = (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len().max(1) as f32).sqrt();
        let peak = pcm.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!(
            secs >= min_secs,
            "{engine}: too short ({secs:.2}s < expected {min_secs:.2}s for {sentence:?})"
        );
        assert!(rms > 1e-3, "{engine}: looks silent (rms={rms:.2e})");
        assert!(peak.is_finite() && peak <= 1.5, "{engine}: peak out of range ({peak})");

        let out = out_dir.join(wav_name.as_deref().unwrap_or(engine));
        let out = if out.extension().is_some() {
            out
        } else {
            out.with_extension("wav")
        };
        write_wav_mono(&out, &pcm, sr);
        eprintln!(
            "WAV[{engine}] {secs:.2}s rms={rms:.3} peak={peak:.3} sr={sr} -> {}",
            out.display()
        );

        #[cfg(feature = "whisper-validate")]
        {
            match skill_tts::whisper_validate::validate_pcm_with_whisper(&pcm, sr, &sentence, whisper_min) {
                Ok(transcript) => eprintln!("WHISPER[{engine}] PASS: {transcript}"),
                Err(e) if whisper_soft => eprintln!("WHISPER[{engine}] FAIL (soft): {e:#}"),
                Err(e) => panic!("{engine} Whisper validation failed: {e:#}"),
            }
        }
    }
}

/// Minimal 16-bit mono WAV writer (test-only).
fn write_wav_mono(path: &std::path::Path, pcm: &[f32], sr: u32) {
    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + pcm.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&sr.to_le_bytes());
    out.extend_from_slice(&(sr * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in pcm {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}
