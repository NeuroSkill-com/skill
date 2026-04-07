// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
fn main() {
    #[cfg(target_os = "macos")]
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
