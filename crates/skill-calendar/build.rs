// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
fn main() {
    // A build script's own `cfg!`/`#[cfg]` describes the HOST that runs it, not
    // the target being built for.  With `#[cfg(target_os = "macos")]` this
    // block also ran when cross-compiling from a Mac to Windows, handing an
    // Objective-C file to clang-cl: `fatal error: 'Foundation/Foundation.h'
    // file not found`, which is a confusing way to discover that EventKit is
    // not available on Windows.  A native Windows runner never saw it, since
    // there the host cfg is false for the right reason by accident.
    // CARGO_CFG_TARGET_OS is the target; `cc` is an unconditional
    // build-dependency, so nothing else has to be gated.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    {
        println!("cargo:rerun-if-changed=src/skill_calendar_macos.m");

        // macOS's BSD ar doesn't support the -D (deterministic) flag that the
        // cc crate passes by default.  Use llvm-ar when available.
        let mut build = cc::Build::new();
        build.file("src/skill_calendar_macos.m").flag("-fobjc-arc");
        if std::path::Path::new("/opt/homebrew/opt/llvm/bin/llvm-ar").exists() {
            build.archiver("/opt/homebrew/opt/llvm/bin/llvm-ar");
        }
        build.compile("skill_calendar_macos");

        println!("cargo:rustc-link-lib=framework=EventKit");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }
}
