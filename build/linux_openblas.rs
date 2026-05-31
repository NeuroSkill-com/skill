// SPDX-License-Identifier: GPL-3.0-only
// Shared build.rs helper: link Debian/Ubuntu system OpenBLAS on Linux.
//
// `libopenblas-dev` installs `libopenblas.so.0` under update-alternatives
// subdirs (e.g. `openblas-pthread/`) that are not on the default loader path.
// We emit link-search + rpath so `cargo test` and release binaries resolve it
// without `LD_LIBRARY_PATH`.

// Emit `cargo:rustc-link-*` directives when `enabled` and target OS is Linux.
pub fn link_system_openblas(enabled: bool) {
    if !enabled || std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }

    for dir in &[
        "/usr/lib/x86_64-linux-gnu/openblas-pthread",
        "/usr/lib/x86_64-linux-gnu/openblas-openmp",
        "/usr/lib/x86_64-linux-gnu",
    ] {
        if std::path::Path::new(dir).exists() {
            println!("cargo:rustc-link-search={dir}");
            println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
        }
    }
    println!("cargo:rustc-link-lib=openblas");
}
