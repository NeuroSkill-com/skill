// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Ignored smoke tests for FunASR (SenseVoice) and Nemotron-ASR load paths.
//!
//! These download large Hub checkpoints on first run:
//!   cargo test -p skill-asr --features asr --test asr_funasr_nemotron_smoke -- --ignored --nocapture
//!
//! Optional: `SKILL_ASR_SMOKE_NEMOTRON=1` to also exercise the ~2.4 GB Nemotron pack.
#![cfg(feature = "asr")]

#[test]
#[ignore = "downloads FunASR SenseVoice (~1 GB) on first run"]
fn funasr_sensevoice_loads() {
    skill_asr::smoke_ensure_engine("funasr", "FunAudioLLM/SenseVoiceSmall")
        .expect("FunASR / SenseVoice should load (or download) cleanly");
}

#[test]
#[ignore = "downloads Nemotron ASR (~2.4 GB); set SKILL_ASR_SMOKE_NEMOTRON=1"]
fn nemotron_asr_loads() {
    if std::env::var_os("SKILL_ASR_SMOKE_NEMOTRON").is_none() {
        eprintln!("skip: set SKILL_ASR_SMOKE_NEMOTRON=1 to download/run Nemotron smoke");
        return;
    }
    skill_asr::smoke_ensure_engine("nemotron-asr", "nvidia/nemotron-3.5-asr-streaming-0.6b")
        .expect("Nemotron-ASR should load (or download) cleanly");
}

#[test]
#[ignore = "downloads RLX-ASR pack on first run"]
fn rlx_asr_loads() {
    skill_asr::smoke_ensure_engine("rlx-asr", "eugenehp/rlx-asr").expect("RLX-ASR should load (or download) cleanly");
}
