// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Rust target triple constants and Apple Mac hardware profiles.
//!
//! Build scripts resolve aliases like `mac-neo` to [`AARCH64_APPLE_DARWIN`] and
//! export `SKILL_MAC_PROFILE=neo`. At runtime, [`effective_apple_mac_profile`]
//! prefers that env var, then falls back to `hw.model` detection.

/// macOS Apple Silicon (arm64) — M-series, MacBook Neo (A-series), etc.
pub const AARCH64_APPLE_DARWIN: &str = "aarch64-apple-darwin";

/// macOS Intel (x86_64).
pub const X86_64_APPLE_DARWIN: &str = "x86_64-apple-darwin";

/// macOS universal fat binary.
pub const UNIVERSAL_APPLE_DARWIN: &str = "universal-apple-darwin";

/// Linux arm64.
pub const AARCH64_UNKNOWN_LINUX_GNU: &str = "aarch64-unknown-linux-gnu";

/// Linux x86_64.
pub const X86_64_UNKNOWN_LINUX_GNU: &str = "x86_64-unknown-linux-gnu";

/// Windows x86_64 (MSVC).
pub const X86_64_PC_WINDOWS_MSVC: &str = "x86_64-pc-windows-msvc";

/// Windows arm64 (MSVC).
pub const AARCH64_PC_WINDOWS_MSVC: &str = "aarch64-pc-windows-msvc";

/// Target triples the project builds and ships.
pub const SUPPORTED_TARGET_TRIPLES: &[&str] = &[
    AARCH64_APPLE_DARWIN,
    X86_64_APPLE_DARWIN,
    UNIVERSAL_APPLE_DARWIN,
    AARCH64_UNKNOWN_LINUX_GNU,
    X86_64_UNKNOWN_LINUX_GNU,
    X86_64_PC_WINDOWS_MSVC,
    AARCH64_PC_WINDOWS_MSVC,
];

/// macOS hardware tier for tuning defaults (memory, GPU offload, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppleMacProfile {
    /// M-series and other full-featured Apple Silicon Macs.
    Default,
    /// MacBook Neo and similar A-series Macs (8 GB unified memory, entry GPU).
    Neo,
}

/// Classify an `hw.model` sysctl value (e.g. `"Mac17,5"` for MacBook Neo).
#[must_use]
pub fn apple_mac_profile_from_hw_model(model: &str) -> AppleMacProfile {
    // MacBook Neo (A18 Pro, 2026): Mac17,5 — first A-series Mac.
    if model.starts_with("Mac17,") {
        return AppleMacProfile::Neo;
    }
    AppleMacProfile::Default
}

/// Read `hw.model` on macOS; returns `None` on other platforms or on failure.
#[cfg(target_os = "macos")]
pub fn read_hw_model() -> Option<String> {
    use std::ffi::CString;

    let c_name = CString::new("hw.model").ok()?;
    let mut size: usize = 0;
    // SAFETY: size-query sysctlbyname with a valid C string name.
    let ret = unsafe {
        libc::sysctlbyname(
            c_name.as_ptr(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret != 0 || size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size];
    // SAFETY: `buf` is sized from the prior sysctl call; name is valid.
    let ret = unsafe {
        libc::sysctlbyname(
            c_name.as_ptr(),
            buf.as_mut_ptr() as *mut _,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret != 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let s = String::from_utf8_lossy(&buf[..end]).into_owned();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn read_hw_model() -> Option<String> {
    None
}

/// Detect profile from live hardware (macOS only).
#[must_use]
pub fn detect_apple_mac_profile() -> AppleMacProfile {
    read_hw_model()
        .map(|m| apple_mac_profile_from_hw_model(&m))
        .unwrap_or(AppleMacProfile::Default)
}

/// Profile for tuning: `SKILL_MAC_PROFILE` env (set at build time) wins over hardware.
#[must_use]
pub fn effective_apple_mac_profile() -> AppleMacProfile {
    if let Ok(v) = std::env::var("SKILL_MAC_PROFILE") {
        if v.eq_ignore_ascii_case("neo") {
            return AppleMacProfile::Neo;
        }
    }
    detect_apple_mac_profile()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macbook_neo_hw_model() {
        assert_eq!(apple_mac_profile_from_hw_model("Mac17,5"), AppleMacProfile::Neo);
    }

    #[test]
    fn m_series_hw_model() {
        assert_eq!(apple_mac_profile_from_hw_model("Mac14,9"), AppleMacProfile::Default);
    }

    #[test]
    fn supported_triples_include_apple_silicon() {
        assert!(SUPPORTED_TARGET_TRIPLES.contains(&AARCH64_APPLE_DARWIN));
    }
}
