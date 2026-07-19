#![allow(clippy::unwrap_used, clippy::panic)]
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//!
//! NeuTTS (rlx-neutts) headless synthesis test.
//!
//! Voice-clones a bundled preset (`jo.npy` precomputed ref-codes — the
//! `NeuCodecEncoder` is unimplemented in rlx-neutts 0.2.9, so runtime reference
//! encoding isn't available) and synthesizes a phrase to a WAV. Downloads the
//! backbone GGUF + neucodec decoder on first run, so it's `#[ignore]`d.
//!
//! Run (release + metal for a usable speed):
//!   cargo test -p skill-neutts --features espeak,metal --test neutts_e2e -- --ignored --nocapture
//!
//! Validate the output transcribes back (cross-check with the ASR side):
//!   SKILL_ASR_TEST_WAV=$TMPDIR/neutts_e2e.wav SKILL_ASR_TEST_EXPECT="quick brown" \
//!     cargo test -p skill-asr --release --features asr --test asr_e2e -- --ignored
#![cfg(feature = "espeak")]

use std::path::PathBuf;

fn ref_files() -> (PathBuf, PathBuf) {
    let base = std::env::var("NEUTTS_SAMPLES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/neutts-samples"));
    (base.join("jo.npy"), base.join("jo.txt"))
}

#[test]
#[ignore = "downloads NeuTTS backbone + neucodec; uses bundled jo.npy preset"]
fn neutts_synthesizes_to_wav() {
    let (npy, txt) = ref_files();
    if !npy.is_file() {
        eprintln!("skip: preset ref-codes not found at {}", npy.display());
        return;
    }
    let backbone =
        std::env::var("NEUTTS_BACKBONE_REPO").unwrap_or_else(|_| "neuphonic/neutts-nano-q4-gguf".to_string());

    eprintln!("[neutts] loading backbone {backbone} + neucodec …");
    let model = match neutts::download::load_from_hub_cb(&backbone, None, |_| {}) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[neutts] backbone load failed: {e:#}");
            panic!("NeuTTS backbone load failed");
        }
    };

    let ref_codes = model.load_ref_codes(&npy).expect("load preset ref-codes");
    let ref_text = std::fs::read_to_string(&txt)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    eprintln!(
        "[neutts] preset 'jo': {} ref tokens, ref_text={ref_text:?}",
        ref_codes.len()
    );

    let text = "The quick brown fox jumps over the lazy dog.";
    let audio = model.infer(text, &ref_codes, &ref_text).expect("neutts infer");
    assert!(!audio.is_empty(), "synthesized audio is empty");

    let out = std::env::temp_dir().join("neutts_e2e.wav");
    model.write_wav(&audio, &out).expect("write wav");
    let bytes = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    eprintln!(
        "[neutts] wrote {bytes} bytes → {} ({:.2}s @ {} Hz)",
        out.display(),
        audio.len() as f32 / neutts::SAMPLE_RATE as f32,
        neutts::SAMPLE_RATE
    );
    assert!(bytes > 1000, "wav suspiciously small ({bytes} bytes)");
}
