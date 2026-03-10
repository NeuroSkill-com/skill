// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // ── espeak-ng: static on macOS / Linux; skipped on Windows ───────────────
    //
    // libespeak-ng must be linked statically so the binary has no runtime
    // dependency on a system espeak-ng installation.  We enforce this here
    // (in addition to kittentts/neutts build.rs) so that a plain `cargo build`
    // without ESPEAK_LIB_DIR set still produces a fully static binary.
    //
    // On Windows, kittentts and neutts are built without the `espeak` feature
    // (see Cargo.toml [target.'cfg(not(target_os = "windows"))'.dependencies]),
    // so no static linking is needed and build-espeak-static.sh is never run.
    //
    // If the pre-built archive (espeak-static/lib/libespeak-ng.a) is absent,
    // build-espeak-static.sh is invoked automatically on macOS and Linux.
    enforce_espeak_static();

    // ── Bake the dev data path into the binary (macOS / Linux only) ──────────
    //
    // Emits ESPEAK_DATA_PATH_DEV so that `cargo run` / plain debug builds on
    // the build machine find the data without needing ESPEAK_DATA_PATH set or
    // a system espeak-ng install.  This is a last-resort fallback; the Tauri
    // bundle path (Contents/Resources/espeak-ng-data) takes priority at runtime
    // via init_espeak_bundled_data_path() called from lib.rs setup().
    // Skipped on Windows (espeak not used).
    emit_espeak_data_path_dev();

    // ── Copy espeak-ng-data/ into resources/ for Tauri bundling ──────────────
    //
    // tauri.conf.json declares resources/espeak-ng-data as a bundle resource.
    // This step must run before tauri_build::build() validates the resource
    // paths.
    //
    // On macOS / Linux the directory is populated from the static build or from
    // a system espeak-ng install.  On Windows it is created as an empty
    // placeholder so the Tauri bundler does not error on the missing path;
    // kittentts/neutts do not use espeak on Windows so no data is needed.
    #[cfg(target_os = "macos")]
    bundle_espeak_data_macos();
    #[cfg(target_os = "linux")]
    bundle_espeak_data_linux();
    #[cfg(target_os = "windows")]
    bundle_espeak_data_windows();

    tauri_build::build()
}

// ── Static linking enforcement ────────────────────────────────────────────────
//
// Resolution order (mirrors kittentts/neutts build.rs so all three agree on
// the same archive):
//
//  1. ESPEAK_LIB_DIR env var   — set via .cargo/config.toml
//  2. espeak-static/lib/       — local build from build-espeak-static.sh
//  3. Platform path walk       — system / Homebrew locations (static only)
//
// If none of the above yields libespeak-ng.a the build script is invoked
// automatically on macOS and Linux.  The build panics if it still cannot
// locate the archive after running the script.  On Windows this function
// returns immediately — no static archive is required (espeak not used).

fn enforce_espeak_static() {
    println!("cargo:rerun-if-env-changed=ESPEAK_LIB_DIR");
    println!("cargo:rerun-if-changed=espeak-static/lib/libespeak-ng.a");

    let target_os   = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    // On Windows, kittentts and neutts are built without the `espeak` feature
    // (see [target.'cfg(not(target_os = "windows"))'.dependencies] in Cargo.toml).
    // No static espeak-ng library is needed, and build-espeak-static.sh cannot
    // run without Bash.  Skip entirely.
    if target_os == "windows" {
        println!(
            "cargo:warning=build.rs: skipping espeak-ng static enforcement on Windows \
             (kittentts/neutts use built-in romaniser instead)."
        );
        return;
    }

    // 1. ESPEAK_LIB_DIR (always set by .cargo/config.toml to espeak-static/lib).
    //    If the archive is absent, build it from source first.
    if let Ok(dir) = std::env::var("ESPEAK_LIB_DIR") {
        if !Path::new(&dir).join("libespeak-ng.a").exists() {
            build_espeak_static();
        }
        require_static_archive(&dir, &target_os);
        return;
    }

    // 2. Fallback: local build artifact (ESPEAK_LIB_DIR not set).
    if !Path::new("espeak-static/lib/libespeak-ng.a").exists() {
        build_espeak_static();
    }

    if Path::new("espeak-static/lib/libespeak-ng.a").exists() {
        let abs = std::fs::canonicalize("espeak-static/lib")
            .unwrap_or_else(|_| PathBuf::from("espeak-static/lib"));
        emit_static_link(abs.to_string_lossy().as_ref(), &target_os);
        return;
    }

    // 3. Platform path walk (static archives only).
    if let Some(dir) = find_static_in_system(&target_os, &target_arch) {
        emit_static_link(dir.to_string_lossy().as_ref(), &target_os);
        return;
    }

    // Still nothing — hard error on all platforms.
    panic!(
        "\n\nbuild.rs: libespeak-ng.a not found even after running \
         scripts/build-espeak-static.sh.\n\
         Check the script output above for errors.\n\n"
    );
}

/// Run `scripts/build-espeak-static.sh` from the workspace root.
///
/// Invoked automatically on **all** platforms when `espeak-static/lib/libespeak-ng.a`
/// is absent.  The script uses cmake + git (both ship with Xcode CLT on macOS
/// and are installable via apt/dnf on Linux), and produces a self-contained
/// merged static archive at `src-tauri/espeak-static/lib/libespeak-ng.a`.
fn build_espeak_static() {
    // build.rs runs from src-tauri/; the script lives one level up.
    let script = std::fs::canonicalize("../scripts/build-espeak-static.sh")
        .unwrap_or_else(|_| Path::new("../scripts/build-espeak-static.sh").to_path_buf());

    if !script.exists() {
        panic!(
            "\n\nbuild.rs: scripts/build-espeak-static.sh not found at {}.\n\
             Cannot auto-build libespeak-ng.a.\n\n",
            script.display()
        );
    }

    eprintln!("build.rs: libespeak-ng.a not found — running {} …", script.display());

    // Augment PATH so cmake, libtool, nm, git are found even when Cargo
    // launched us with a minimal environment.
    let current_path = std::env::var("PATH").unwrap_or_default();
    let full_path = format!(
        "/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:{current_path}"
    );

    // IMPORTANT: Cargo *silently drops* every non-`cargo:` line from a build
    // script's stdout.  cmake writes its build errors to stdout, so without the
    // `exec 1>&2` prefix they would vanish and the failure appears silent.
    // The `exec 1>&2` redirect makes the script's stdout share the build
    // script's stderr fd, which Cargo *does* show when the build fails.
    let shell_cmd = format!("exec 1>&2; bash '{}'", script.display());

    let status = Command::new("bash")
        .args(["-c", &shell_cmd])
        .env("PATH", &full_path)
        .status()
        .unwrap_or_else(|e| panic!("build.rs: failed to launch build-espeak-static.sh: {e}"));

    if !status.success() {
        panic!(
            "\n\nbuild.rs: scripts/build-espeak-static.sh failed ({status}).\n\
             See the script output above for the exact error.\n\n"
        );
    }

    println!("cargo:warning=build.rs: espeak-ng static library built successfully.");
}

/// Emit `rustc-link-search` + `rustc-link-lib=static=espeak-ng` for `dir`.
/// Also links the C++ standard library required by espeak-ng's object files.
fn emit_static_link(dir: &str, target_os: &str) {
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=static=espeak-ng");
    // espeak-ng is a C++ project; its static archive pulls in C++ symbols that
    // the linker must resolve against the platform's C++ stdlib.
    if target_os == "macos" {
        println!("cargo:rustc-link-lib=dylib=c++");
    } else {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }
}

/// Verify that `libespeak-ng.a` is present in `dir`, then emit link directives.
/// Panics on all platforms if the archive is missing — the caller must have
/// already invoked `build_espeak_static()` to ensure it exists.
fn require_static_archive(dir: &str, target_os: &str) {
    if Path::new(dir).join("libespeak-ng.a").exists() {
        emit_static_link(dir, target_os);
    } else {
        panic!(
            "\n\nbuild.rs: ESPEAK_LIB_DIR is set to {dir:?} but libespeak-ng.a \
             was not found there even after running build-espeak-static.sh.\n\
             Check the script output above for errors.\n\
             \n\
             Manual fix:  bash scripts/build-espeak-static.sh\n\n"
        );
    }
}

/// Walk well-known system directories looking for `libespeak-ng.a`.
fn find_static_in_system(target_os: &str, target_arch: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if target_os == "macos" {
        if let Some(keg) = brew_prefix("espeak-ng") {
            dirs.push(PathBuf::from(format!("{keg}/lib")));
        }
        for prefix in ["/opt/homebrew", "/usr/local"] {
            dirs.push(PathBuf::from(format!("{prefix}/opt/espeak-ng/lib")));
            dirs.push(PathBuf::from(format!("{prefix}/lib")));
        }
    } else {
        let multiarch = match target_arch {
            "x86_64"  => "x86_64-linux-gnu",
            "aarch64" => "aarch64-linux-gnu",
            "arm"     => "arm-linux-gnueabihf",
            _         => "",
        };
        if !multiarch.is_empty() {
            dirs.push(PathBuf::from(format!("/usr/lib/{multiarch}")));
        }
        dirs.extend(["/usr/lib64", "/usr/lib", "/usr/local/lib"].map(PathBuf::from));
    }

    dirs.into_iter()
        .filter(|p| p.is_dir())
        .find(|p| p.join("libespeak-ng.a").exists())
}

/// Run `brew --prefix <formula>` and return the keg path on success.
fn brew_prefix(formula: &str) -> Option<String> {
    let out = Command::new("brew").args(["--prefix", formula]).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8(out.stdout).ok()?.trim().to_owned())
    } else {
        None
    }
}

// ── macOS bundle helpers ──────────────────────────────────────────────────────
//
// espeak-ng-data/
// ───────────────
// Copied into src-tauri/resources/espeak-ng-data/ so Tauri includes it at
// Contents/Resources/espeak-ng-data/ in the .app bundle.
// tts.rs resolves that path at runtime via init_espeak_data_path().

/// Bake the absolute path to `espeak-static/share/espeak-ng-data` into the
/// binary as the compile-time env `ESPEAK_DATA_PATH_DEV`.
///
/// This lets plain `cargo run` / debug builds on the **build machine** find the
/// data directory without needing `ESPEAK_DATA_PATH` set or a system espeak-ng.
/// The value is an absolute path resolved at build time; it is only used as a
/// last-resort fallback in `init_espeak_data_path()` — the bundle path
/// (`Contents/Resources/espeak-ng-data`) takes priority via the
/// `init_espeak_bundled_data_path()` call in lib.rs setup().
///
/// Works on both macOS and Linux.  Skipped on Windows (espeak not used).
fn emit_espeak_data_path_dev() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        return;
    }
    // ESPEAK_LIB_DIR is always set via .cargo/config.toml to
    // <workspace>/espeak-static/lib.  The data lives one level up, in
    // <workspace>/espeak-static/share/espeak-ng-data.
    if let Ok(lib_dir) = std::env::var("ESPEAK_LIB_DIR") {
        let data_dir = Path::new(&lib_dir)
            .parent()
            .unwrap_or(Path::new("."))
            .join("share")
            .join("espeak-ng-data");

        // Tell Cargo to re-run build.rs if the data dir appears or changes.
        println!("cargo:rerun-if-changed=espeak-static/share/espeak-ng-data");

        if data_dir.is_dir() {
            // Canonicalise so the baked path is always absolute even if the
            // ESPEAK_LIB_DIR value is relative (which it is from config.toml).
            let abs = std::fs::canonicalize(&data_dir).unwrap_or(data_dir);
            println!("cargo:rustc-env=ESPEAK_DATA_PATH_DEV={}", abs.display());
            println!("cargo:warning=espeak-ng dev data path baked in: {}", abs.display());
        } else {
            // Directory not yet present (first build before script ran).
            // Cargo will re-run build.rs on the next `cargo build` via the
            // rerun-if-changed above, at which point the data will be present.
            println!(
                "cargo:warning=espeak-ng data dir not found yet ({}); \
                 will be baked in after the first full build.",
                data_dir.display()
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn bundle_espeak_data_macos() {
    // ── Copy espeak-ng-data/ ──────────────────────────────────────────────────
    //
    // Prefer data from the same espeak-ng source that was built statically
    // (espeak-static/share/espeak-ng-data/), then fall back to the Homebrew
    // installation for developer convenience.
    let static_data = "espeak-static/share/espeak-ng-data";
    let brew_prefix_val = brew_prefix("espeak-ng");
    let data_src = [
        Some(static_data.to_string()),
        brew_prefix_val.as_deref().map(|p| format!("{p}/share/espeak-ng-data")),
        brew_prefix_val.as_deref().map(|p| format!("{p}/lib/espeak-ng-data")),
        Some("/opt/homebrew/share/espeak-ng-data".to_string()),
        Some("/usr/local/share/espeak-ng-data".to_string()),
    ]
    .into_iter()
    .flatten()
    .find(|p| Path::new(p.as_str()).is_dir())
    .unwrap_or_else(|| {
        panic!(
            "build.rs: espeak-ng-data/ not found.\n\
             Ensure espeak-ng is installed: brew install espeak-ng\n\
             Or run: bash scripts/build-espeak-static.sh"
        )
    });

    let data_dst = "resources/espeak-ng-data";
    // Always refresh so stale data from a previous espeak-ng version is removed.
    if Path::new(data_dst).exists() {
        std::fs::remove_dir_all(data_dst)
            .expect("build.rs: failed to remove old resources/espeak-ng-data/");
    }
    copy_dir_all(&data_src, data_dst);

    // Re-run the build script whenever the source artefacts change.
    println!("cargo:rerun-if-changed={data_src}");
}

// ── Linux bundle helper ───────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn bundle_espeak_data_linux() {
    // Priority order mirrors the macOS version:
    //   1. Our own static build  (espeak-static/share/espeak-ng-data)
    //   2. System package paths  (/usr/lib/<multiarch>/espeak-ng-data, /usr/share/…)
    let multiarch = std::env::var("CARGO_CFG_TARGET_ARCH")
        .map(|a| match a.as_str() {
            "x86_64"  => "x86_64-linux-gnu",
            "aarch64" => "aarch64-linux-gnu",
            "arm"     => "arm-linux-gnueabihf",
            _         => "",
        })
        .unwrap_or_default();

    let candidates: Vec<String> = [
        "espeak-static/share/espeak-ng-data".to_string(),
        format!("/usr/lib/{multiarch}/espeak-ng-data"),
        "/usr/lib/espeak-ng-data".to_string(),
        "/usr/share/espeak-ng-data".to_string(),
        "/usr/local/share/espeak-ng-data".to_string(),
    ]
    .into_iter()
    .filter(|p| !p.contains("//") && Path::new(p.as_str()).is_dir())
    .collect();

    let data_src = candidates.first().unwrap_or_else(|| {
        panic!(
            "build.rs: espeak-ng-data/ not found on Linux.\n\
             Run:  bash scripts/build-espeak-static.sh\n\
             Or install:  sudo apt-get install espeak-ng-data"
        )
    });

    let data_dst = "resources/espeak-ng-data";
    if Path::new(data_dst).exists() {
        std::fs::remove_dir_all(data_dst)
            .expect("build.rs: failed to remove old resources/espeak-ng-data/");
    }
    copy_dir_all(data_src, data_dst);

    println!("cargo:rerun-if-changed={data_src}");
}

// ── Windows bundle helper ─────────────────────────────────────────────────────
//
// On Windows, kittentts and neutts are compiled without the `espeak` feature,
// so no espeak-ng-data is needed at runtime.  However tauri.conf.json declares
// `resources/espeak-ng-data` as a bundle resource for all platforms; if the
// directory is absent the Tauri bundler errors out.
//
// We create an empty placeholder directory here so the bundler is satisfied.
// The directory will be included in the Windows installer but contains nothing,
// which is correct — espeak is simply not used on Windows.

#[cfg(target_os = "windows")]
fn bundle_espeak_data_windows() {
    let data_dst = "resources/espeak-ng-data";
    if !Path::new(data_dst).exists() {
        std::fs::create_dir_all(data_dst)
            .unwrap_or_else(|e| {
                panic!("build.rs: create_dir_all({data_dst}) failed: {e}");
            });
        println!(
            "cargo:warning=build.rs: created empty resources/espeak-ng-data placeholder \
             for Windows bundle (espeak not used on Windows)."
        );
    }
}

/// Recursively copy a directory tree from `src` to `dst`.
#[cfg(unix)]
fn copy_dir_all(src: &str, dst: &str) {
    let src_path = Path::new(src);
    let dst_path = Path::new(dst);
    std::fs::create_dir_all(dst_path)
        .unwrap_or_else(|e| panic!("build.rs: create_dir_all({dst}) failed: {e}"));

    for entry in std::fs::read_dir(src_path)
        .unwrap_or_else(|e| panic!("build.rs: read_dir({src}) failed: {e}"))
    {
        let entry = entry.unwrap_or_else(|e| panic!("build.rs: dir entry error: {e}"));
        let target = dst_path.join(entry.file_name());
        let file_type = entry.file_type().unwrap();

        if file_type.is_dir() {
            copy_dir_all(
                &entry.path().to_string_lossy(),
                &target.to_string_lossy(),
            );
        } else if file_type.is_symlink() {
            // Resolve and copy the symlink target rather than the link itself,
            // so the bundle contains plain files (required for notarisation).
            let resolved = std::fs::canonicalize(entry.path())
                .unwrap_or_else(|_| entry.path().to_path_buf());
            if resolved.is_file() {
                remove_if_readonly(&target);
                std::fs::copy(&resolved, &target)
                    .unwrap_or_else(|e| panic!("build.rs: copy symlink {resolved:?}: {e}"));
            }
        } else {
            remove_if_readonly(&target);
            std::fs::copy(entry.path(), &target)
                .unwrap_or_else(|e| panic!("build.rs: copy {:?}: {e}", entry.path()));
        }
    }
}

/// Remove `path` if it exists and is read-only so a subsequent `fs::copy`
/// does not fail with "Permission denied".
#[cfg(unix)]
fn remove_if_readonly(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if path.exists() {
        let mut perms = std::fs::metadata(path)
            .map(|m| m.permissions())
            .unwrap_or_else(|_| std::fs::Permissions::from_mode(0o644));
        perms.set_mode(perms.mode() | 0o200); // add owner-write bit
        std::fs::set_permissions(path, perms).ok();
        std::fs::remove_file(path).ok();
    }
}
