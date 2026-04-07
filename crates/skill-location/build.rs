// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
fn main() {
    // Only compile the Objective-C bridge when targeting macOS.
    // `cfg!()` checks the *target* triple (not the host), so cross-compilation
    // from Linux/Windows targeting macOS will still compile the .m file, while
    // native Linux/Windows builds skip it entirely.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-changed=src/skill_location_macos.m");

        // macOS's BSD ar doesn't support the -D (deterministic) flag that the
        // cc crate passes by default.  Use llvm-ar when available.
        let mut build = cc::Build::new();
        build
            .file("src/skill_location_macos.m")
            .flag("-fobjc-arc")
            .flag("-mmacosx-version-min=11.0");
        if std::path::Path::new("/opt/homebrew/opt/llvm/bin/llvm-ar").exists() {
            build.archiver("/opt/homebrew/opt/llvm/bin/llvm-ar");
        }
        build.compile("skill_location_macos");

        println!("cargo:rustc-link-lib=framework=CoreLocation");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }
}
