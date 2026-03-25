// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-changed=src/skill_calendar_macos.m");

        cc::Build::new()
            .file("src/skill_calendar_macos.m")
            .flag("-fobjc-arc")
            .compile("skill_calendar_macos");

        println!("cargo:rustc-link-lib=framework=EventKit");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }
}
