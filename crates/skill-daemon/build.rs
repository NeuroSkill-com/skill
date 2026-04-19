// SPDX-License-Identifier: GPL-3.0-only
//! Build script for skill-daemon.
//!
//! Fixes missing linker directives when using prebuilt llama-cpp-sys static
//! archives on Linux.  The upstream build.rs prebuilt code path omits:
//!   - `cargo:rustc-link-lib=vulkan` (Vulkan symbols from ggml-vulkan.cpp)
//!   - `cargo:rustc-link-search` for openblas in its alternatives directory

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=vulkan");

        // openblas installs to a subdirectory managed by update-alternatives;
        // the linker won't find it with just -L /usr/lib/x86_64-linux-gnu.
        for dir in &[
            "/usr/lib/x86_64-linux-gnu/openblas-pthread",
            "/usr/lib/x86_64-linux-gnu/openblas-openmp",
            "/usr/lib/x86_64-linux-gnu",
        ] {
            if std::path::Path::new(dir).exists() {
                println!("cargo:rustc-link-search={dir}");
            }
        }
    }
}
