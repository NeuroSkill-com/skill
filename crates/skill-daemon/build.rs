// SPDX-License-Identifier: GPL-3.0-only
//! Build script for skill-daemon.
//!
//! - Embeds repo-root `VERSION` as `SKILL_PRODUCT_VERSION` (source of truth for
//!   `/v1/version` and support diagnostics).
//! - On Linux, emits the extra link directives the daemon binary needs but that
//!   the crates providing those symbols don't emit themselves:
//!   - `cargo:rustc-link-lib=vulkan` (Vulkan loader symbols for wgpu)
//!   - OpenBLAS search path + rpath (see `build-support/linux_openblas.rs`)

use std::path::PathBuf;

mod linux_openblas {
    include!("../../build-support/linux_openblas.rs");
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let version_path = manifest_dir.join("../../VERSION");
    println!("cargo:rerun-if-changed={}", version_path.display());

    let product_version = std::fs::read_to_string(&version_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", version_path.display()))
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if product_version.is_empty() {
        panic!("{} is empty", version_path.display());
    }
    println!("cargo:rustc-env=SKILL_PRODUCT_VERSION={product_version}");

    // NOTE: do NOT add `-Wl,-no_compact_unwind` here.
    //
    // It was previously passed on macOS to silence ld64's "compact unwind table
    // limit exceeded" warnings in debug builds. The cost was not cosmetic: it
    // drops the `__unwind_info` section, and without it Apple's libunwind
    // cannot start an unwind — every panic in skill-daemon died as
    // `fatal runtime error: failed to initiate panic, error 5` (error 5 is
    // `_URC_END_OF_STACK`) and aborted the process with SIGABRT.
    //
    // Consequences, all of which were live: a panic anywhere killed the whole
    // daemon instead of just its task; `catch_unwind` in tokio and libtest
    // never ran, so a single panicking unit test took down the entire test
    // binary; and no `Drop` ever ran, which disabled every RAII cleanup in the
    // session path (CSV finalisation, embed-queue drain, device-state reset).
    // See github.com/NeuroSkill-com/skill#89.
    //
    // Linker warnings are the cheaper problem. Leave unwinding alone.

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=vulkan");
        linux_openblas::link_system_openblas(true);
    }
}
