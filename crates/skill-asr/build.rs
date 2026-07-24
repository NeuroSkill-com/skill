// SPDX-License-Identifier: GPL-3.0-only
//! Emits `asr_active` when the `asr` feature is enabled on any desktop OS
//! (macOS / Linux / Windows). Source gates on `cfg(asr_active)` so builds
//! without the feature keep the no-op stubs.

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_ASR");
    println!("cargo::rustc-check-cfg=cfg(asr_active)");
    let feat_on = std::env::var_os("CARGO_FEATURE_ASR").is_some();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let desktop = matches!(target_os.as_str(), "macos" | "linux" | "windows");
    if feat_on && desktop {
        println!("cargo:rustc-cfg=asr_active");
    }
}
