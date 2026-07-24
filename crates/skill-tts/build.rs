// SPDX-License-Identifier: GPL-3.0-only
//! Emits `tts_kitten_active` / `tts_engines_active` cfgs when the matching
//! feature is enabled. KittenTTS and Orpheus / Qwen3-TTS engines run on
//! Linux, Windows, and macOS (CPU / CUDA / wgpu / Metal per target deps).

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_TTS_KITTEN");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_TTS_ENGINES");
    println!("cargo::rustc-check-cfg=cfg(tts_kitten_active)");
    println!("cargo::rustc-check-cfg=cfg(tts_engines_active)");
    if std::env::var_os("CARGO_FEATURE_TTS_KITTEN").is_some() {
        println!("cargo:rustc-cfg=tts_kitten_active");
    }
    if std::env::var_os("CARGO_FEATURE_TTS_ENGINES").is_some() {
        println!("cargo:rustc-cfg=tts_engines_active");
    }
}
