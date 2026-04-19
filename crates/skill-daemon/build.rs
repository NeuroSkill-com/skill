// SPDX-License-Identifier: GPL-3.0-only
//! Build script for skill-daemon.
//!
//! When using prebuilt llama-cpp-sys static archives on Linux, the upstream
//! build.rs omits `cargo:rustc-link-lib=vulkan` in the prebuilt code path
//! even though the archives contain Vulkan symbols (ggml-vulkan.cpp).
//! We emit the link directive here to fix the final link.

fn main() {
    // Only needed on Linux — macOS uses Metal, Windows uses vulkan-1.lib
    // which the upstream build.rs handles correctly.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=vulkan");
    }
}
