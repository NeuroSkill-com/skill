// SPDX-License-Identifier: GPL-3.0-only
//! Emits `asr_active` cfg when the `asr` feature is enabled AND the host target
//! ships the optional rlx model deps (non-Windows). Source code gates on
//! `cfg(asr_active)` instead of the bare feature so Windows builds compile the
//! no-op stubs even when `asr` is enabled via a `default` features chain.

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_ASR");
    println!("cargo::rustc-check-cfg=cfg(asr_active)");
    let feat_on = std::env::var_os("CARGO_FEATURE_ASR").is_some();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if feat_on && target_os != "windows" {
        println!("cargo:rustc-cfg=asr_active");
    }
}
