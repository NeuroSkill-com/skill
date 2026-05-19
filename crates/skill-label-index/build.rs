// SPDX-License-Identifier: GPL-3.0-only
//! Link system OpenBLAS on Linux when building with `turboquant-index`.
//!
//! `turbovec` uses ndarray BLAS (CBLAS) on Linux/macOS. macOS uses `accelerate-src`
//! from `Cargo.toml`; Linux CI installs `libopenblas-dev` and needs explicit link
//! paths below (Debian/Ubuntu alternatives layout). Windows turbovec builds use
//! pure-Rust/faer paths in the upstream crate (no OpenBLAS).

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    if std::env::var_os("CARGO_FEATURE_TURBOQUANT_INDEX").is_none() {
        return;
    }

    for dir in &[
        "/usr/lib/x86_64-linux-gnu/openblas-pthread",
        "/usr/lib/x86_64-linux-gnu/openblas-openmp",
        "/usr/lib/x86_64-linux-gnu",
    ] {
        if std::path::Path::new(dir).exists() {
            println!("cargo:rustc-link-search={dir}");
        }
    }
    println!("cargo:rustc-link-lib=openblas");
}
