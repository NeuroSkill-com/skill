// SPDX-License-Identifier: GPL-3.0-only
//! Build script for skill-daemon.
//!
//! Fixes missing linker directives when using prebuilt llama-cpp-sys static
//! archives on Linux.  The upstream build.rs prebuilt code path omits:
//!   - `cargo:rustc-link-lib=vulkan` (Vulkan symbols from ggml-vulkan.cpp)
//!   - OpenBLAS search path + rpath (see `build-support/linux_openblas.rs`)

mod linux_openblas {
    include!("../../build-support/linux_openblas.rs");
}

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=vulkan");
        linux_openblas::link_system_openblas(true);
    }
}
