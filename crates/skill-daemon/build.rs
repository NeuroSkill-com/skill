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

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // skill-daemon can exceed ld64 compact unwind table limits in debug
        // builds; force DWARF unwind info to avoid noisy linker warnings.
        println!("cargo:rustc-link-arg-bins=-Wl,-no_compact_unwind");
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=vulkan");
        linux_openblas::link_system_openblas(true);
    }
}
