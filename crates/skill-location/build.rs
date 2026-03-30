// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-changed=src/skill_location_macos.m");

        cc::Build::new()
            .file("src/skill_location_macos.m")
            .flag("-fobjc-arc")
            .flag("-mmacosx-version-min=11.0")
            .compile("skill_location_macos");

        println!("cargo:rustc-link-lib=framework=CoreLocation");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }
}
