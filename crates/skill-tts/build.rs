// SPDX-License-Identifier: GPL-3.0-only
//! Emits `tts_kitten_active` cfg when the `tts-kitten` feature is enabled
//! AND the host target ships the optional `kittentts` dep (non-Windows).
//! Source code uses `cfg(tts_kitten_active)` instead of the bare feature
//! gate so Windows builds skip the kitten code path even when the feature
//! is on (e.g. via the `default` features chain in src-tauri).

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_TTS_KITTEN");
    println!("cargo::rustc-check-cfg=cfg(tts_kitten_active)");
    let feat_on = std::env::var_os("CARGO_FEATURE_TTS_KITTEN").is_some();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if feat_on && target_os != "windows" {
        println!("cargo:rustc-cfg=tts_kitten_active");
    }
}
