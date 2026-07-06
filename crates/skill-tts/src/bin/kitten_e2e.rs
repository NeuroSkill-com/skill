// SPDX-License-Identifier: GPL-3.0-only
//! Headless E2E for KittenTTS (HF download → ONNX → WAV → optional Whisper ASR).
//!
//! ```bash
//! cargo run -p skill-tts --features tts-kitten,whisper-validate --release --bin kitten-e2e
//! # or isolated:
//! cargo run --manifest-path crates/skill-tts/e2e/Cargo.toml --release
//! ```

use std::path::PathBuf;

use skill_tts::kitten::{self, LoadProgress};

fn main() -> anyhow::Result<()> {
    let skill_dir = std::env::var("SKILL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("skill-kitten-e2e"));
    skill_tts::init_tts_dirs(&skill_dir);
    skill_tts::set_logging(true);

    let out = std::env::var("OUT_WAV")
        .map(PathBuf::from)
        .unwrap_or_else(|_| skill_dir.join("kitten_e2e.wav"));
    let text = std::env::var("TEXT").unwrap_or_else(|_| "Hello from Skill.".into());
    let voice = std::env::var("VOICE").unwrap_or_else(|_| kitten::VOICE_DEFAULT.to_string());

    let skip_whisper = std::env::var("WHISPER_VALIDATE")
        .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        .unwrap_or(false);

    eprintln!("skill_dir={}", skill_dir.display());
    eprintln!("out={}", out.display());
    eprintln!("repo={}", kitten::HF_REPO);

    kitten::e2e_synthesize_to_wav(&text, &voice, &out, |p| match p {
        LoadProgress::Fetching { step, total, file } => {
            eprintln!("[{step}/{total}] fetch {file}");
        }
        LoadProgress::Loading => eprintln!("[loading] KittenTTS ONNX session…"),
    })?;

    let meta = std::fs::metadata(&out)?;
    eprintln!("wrote {} bytes → {}", meta.len(), out.display());

    #[cfg(feature = "whisper-validate")]
    if !skip_whisper {
        let transcript = kitten::e2e_validate_wav_with_whisper(&out, &text)?;
        eprintln!("Whisper OK: {transcript:?}");
    }

    eprintln!("OK");
    Ok(())
}
