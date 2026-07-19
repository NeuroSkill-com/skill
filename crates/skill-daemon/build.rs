// SPDX-License-Identifier: GPL-3.0-only
//! Build script for skill-daemon.
//!
//! On Linux, emits the extra link directives the daemon binary needs but that
//! the crates providing those symbols don't emit themselves:
//!   - `cargo:rustc-link-lib=vulkan` (Vulkan loader symbols)
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
