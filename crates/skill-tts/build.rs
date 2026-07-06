// SPDX-License-Identifier: GPL-3.0-only
//! Emits `tts_kitten_active` / `tts_engines_active` cfgs when the matching
//! feature is enabled AND the host target ships the optional rlx deps.
//! Kitten stays non-Windows; Orpheus / Qwen3-TTS engines run on Linux,
//! Windows, and macOS (CPU / CUDA / wgpu / Metal per target deps).

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_TTS_KITTEN");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_TTS_ENGINES");
    println!("cargo::rustc-check-cfg=cfg(tts_kitten_active)");
    println!("cargo::rustc-check-cfg=cfg(tts_engines_active)");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let non_windows = target_os != "windows";
    if std::env::var_os("CARGO_FEATURE_TTS_KITTEN").is_some() && non_windows {
        println!("cargo:rustc-cfg=tts_kitten_active");
    }
    // Orpheus / Qwen3-TTS: all desktop OSes (CPU + CUDA + wgpu where compiled in).
    if std::env::var_os("CARGO_FEATURE_TTS_ENGINES").is_some() {
        println!("cargo:rustc-cfg=tts_engines_active");
    }
}
