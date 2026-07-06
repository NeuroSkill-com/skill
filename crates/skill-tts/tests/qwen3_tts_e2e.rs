// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! End-to-end Qwen3-TTS smoke test (ignored by default — downloads ~the model).
//!
//! Run with:
//!   cargo test -p skill-tts --features tts-engines --test qwen3_tts_e2e -- --ignored --nocapture
//!
//! Verifies the real engine path: hf-hub download → `Qwen3TtsRunner` →
//! `synthesize_custom_voice` → PCM. Asserts the output is the expected sample
//! rate, long enough, and not silent. Writes a WAV to the temp dir for listening.

#![cfg(feature = "tts-engines")]

#[test]
#[ignore = "downloads the Qwen3-TTS model + runs inference"]
fn qwen3_tts_synthesizes_audible_pcm() {
    // Cache models under a stable dir so reruns don't re-download.
    let dir = std::env::temp_dir().join("skill-tts-e2e");
    std::fs::create_dir_all(&dir).unwrap();
    skill_tts::set_skill_dir(dir.clone());
    skill_tts::set_active_engine("qwen3-tts".into(), String::new(), String::new());

    let text = "Hello, this is a test of Qwen3 text to speech running natively in RLX.";
    let (pcm, sr) = skill_tts::engines::synthesize_pcm(text).expect("qwen3-tts synthesis should succeed");

    assert_eq!(sr, 24_000, "expected 24 kHz output");
    let secs = pcm.len() as f32 / sr as f32;
    assert!(
        secs > 0.5,
        "expected >0.5s of audio, got {secs:.2}s ({} samples)",
        pcm.len()
    );

    // Not silent: RMS well above the noise floor.
    let rms = (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len() as f32).sqrt();
    assert!(rms > 1e-3, "output looks silent (rms={rms:.2e})");

    // Peak within sane bounds (no clipping garbage / NaNs).
    let peak = pcm.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
    assert!(peak.is_finite() && peak <= 1.5, "peak out of range: {peak}");

    let out = dir.join("qwen3_tts_e2e.wav");
    write_wav_mono(&out, &pcm, sr);
    eprintln!(
        "qwen3-tts OK: {secs:.2}s, rms={rms:.3}, peak={peak:.3} → {}",
        out.display()
    );
}

/// Minimal 16-bit mono WAV writer (test-only, avoids extra deps here).
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
