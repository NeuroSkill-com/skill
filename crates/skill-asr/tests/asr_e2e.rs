// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! End-to-end ASR test: real Silero VAD + real Whisper over a recorded clip.
//!
//! Downloads the Whisper `base.en` checkpoint on first run, so it's `#[ignore]`d.
//! Provide any mono WAV via `SKILL_ASR_TEST_WAV` and (optionally) a word/phrase
//! you expect to hear via `SKILL_ASR_TEST_EXPECT`.
//!
//! Generate a self-contained fixture on macOS and run it:
//!   say -o /tmp/clip.aiff "testing one two three four"
//!   afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/clip.aiff /tmp/clip.wav
//!   SKILL_ASR_TEST_WAV=/tmp/clip.wav SKILL_ASR_TEST_EXPECT=testing \
//!     cargo test -p skill-asr --features asr --test asr_e2e -- --ignored --nocapture
#![cfg(feature = "asr")]

use std::path::PathBuf;

#[test]
#[ignore = "downloads Whisper weights; needs a WAV via SKILL_ASR_TEST_WAV"]
fn vad_whisper_transcribes_clip() {
    let Some(wav) = std::env::var_os("SKILL_ASR_TEST_WAV") else {
        eprintln!("skip: set SKILL_ASR_TEST_WAV to a mono WAV clip");
        return;
    };
    let wav = PathBuf::from(wav);
    assert!(wav.is_file(), "SKILL_ASR_TEST_WAV not found: {}", wav.display());

    let segments = skill_asr::transcribe_wav(&wav, "en").expect("transcription failed");
    let heard = segments.join(" ").to_lowercase();
    eprintln!("[asr_e2e] segments={} heard={heard:?}", segments.len());

    assert!(!heard.trim().is_empty(), "expected a non-empty transcript");

    if let Ok(expect) = std::env::var("SKILL_ASR_TEST_EXPECT") {
        let expect = expect.to_lowercase();
        // Fuzzy word-coverage rather than exact substring: small ASR/TTS slips
        // (e.g. "fox" → "fix") shouldn't fail an intelligibility round-trip.
        let words = |s: &str| -> Vec<String> {
            s.to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .map(String::from)
                .collect()
        };
        let heard_words = words(&heard);
        let want_words = words(&expect);
        let hits = want_words.iter().filter(|w| heard_words.contains(w)).count();
        let ratio = if want_words.is_empty() {
            1.0
        } else {
            hits as f32 / want_words.len() as f32
        };
        assert!(
            ratio >= 0.6,
            "expected ≥60% of {want_words:?} in transcript {heard:?} (got {hits}/{} = {ratio:.2})",
            want_words.len()
        );
    }
}
