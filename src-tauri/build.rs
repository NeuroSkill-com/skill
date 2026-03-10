// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // ── espeak-ng: static on all platforms ────────────────────────────────────
    //
    // libespeak-ng must be linked statically so the binary has no runtime
    // dependency on a system espeak-ng installation.
    //
    // Archive filename and link behaviour by target:
    //
    //  Target ABI      | File            | C++ lib
    //  ─────────────────┼─────────────────┼─────────────────────────────────────
    //  macOS            | libespeak-ng.a  | -lc++
    //  Linux            | libespeak-ng.a  | -lstdc++
    //  Windows MSVC     | espeak-ng.lib   | (MSVC links runtime automatically)
    //  Windows MinGW/GNU| libespeak-ng.a  | -lstdc++
    //
    // Build script dispatch:
    //  target_env=msvc  → scripts/build-espeak-static.ps1   (PowerShell, MSVC)
    //  target_env=gnu   → scripts/build-espeak-static-mingw.sh  (bash, MinGW)
    //  macOS/Linux      → scripts/build-espeak-static.sh    (bash, Unix)
    //
    // ESPEAK_LIB_DIR (set by .cargo/config.toml) resolves to:
    //  MSVC/Unix  → espeak-static/lib/
    //  MinGW      → espeak-static-mingw/lib/     ← separate dir, no conflict
    enforce_espeak_static();

    // ── Bake the dev data path into the binary ────────────────────────────────
    //
    // Emits ESPEAK_DATA_PATH_DEV so that `cargo run` / debug builds on the
    // build machine find the espeak data without ESPEAK_DATA_PATH set.
    // Resolved relative to ESPEAK_LIB_DIR so it automatically points at
    // espeak-static-mingw/share/... when cross-compiling via MinGW.
    emit_espeak_data_path_dev();

    // ── Copy espeak-ng-data/ into resources/ for Tauri bundling ──────────────
    //
    // The bundle function is selected by BUILD HOST (cfg!), not target.
    // For cross-compilation (Linux host → Windows MinGW target) the Linux
    // function runs and finds data via ESPEAK_LIB_DIR → espeak-static-mingw/.
    #[cfg(target_os = "macos")]
    bundle_espeak_data_macos();
    #[cfg(target_os = "linux")]
    bundle_espeak_data_linux();
    #[cfg(target_os = "windows")]
    bundle_espeak_data_windows();

    tauri_build::build()
}

// ── Target environment helpers ────────────────────────────────────────────────

fn target_os()  -> String { std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() }
fn target_env() -> String { std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default() }
fn target_arch() -> String { std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default() }

// ── Platform library filename ─────────────────────────────────────────────────
//
// Only MSVC uses the no-prefix `.lib` convention.  MinGW (target_env = "gnu")
// and all Unix targets use the `lib*.a` convention — `rustc-link-lib=static=X`
// maps to `libX.a` on these platforms automatically.

fn static_lib_name() -> &'static str {
    if target_os() == "windows" && target_env() == "msvc" {
        "espeak-ng.lib"
    } else {
        "libespeak-ng.a"
    }
}

// ── Static linking enforcement ────────────────────────────────────────────────
//
// Resolution order (mirrors kittentts/neutts build.rs so all three agree):
//
//  1. ESPEAK_LIB_DIR env var   — set via .cargo/config.toml
//  2. Local build artefact     — espeak-static[{-mingw}]/lib/
//  3. Platform path walk       — Homebrew / apt / vcpkg / MSYS2 system paths

fn enforce_espeak_static() {
    println!("cargo:rerun-if-env-changed=ESPEAK_LIB_DIR");

    let t_os   = target_os();
    let t_env  = target_env();
    let t_arch = target_arch();
    let lib    = static_lib_name();

    // Track whichever lib dir ESPEAK_LIB_DIR resolves to.
    if let Ok(dir) = std::env::var("ESPEAK_LIB_DIR") {
        println!("cargo:rerun-if-changed={dir}/{lib}");
    } else {
        println!("cargo:rerun-if-changed=espeak-static/lib/{lib}");
        println!("cargo:rerun-if-changed=espeak-static-mingw/lib/{lib}");
    }

    // 1. ESPEAK_LIB_DIR
    if let Ok(dir) = std::env::var("ESPEAK_LIB_DIR") {
        if !Path::new(&dir).join(lib).exists() {
            build_espeak_static(&t_os, &t_env);
        }
        require_static_archive(&dir, &t_os, &t_env);
        return;
    }

    // 2. Local artefact fallback (ESPEAK_LIB_DIR not set).
    //    MinGW uses a separate output directory to avoid clobbering the
    //    native Unix or MSVC archive.
    let local_dir = if t_os == "windows" && t_env == "gnu" {
        "espeak-static-mingw/lib"
    } else {
        "espeak-static/lib"
    };
    let local = format!("{local_dir}/{lib}");
    if !Path::new(&local).exists() {
        build_espeak_static(&t_os, &t_env);
    }
    if Path::new(&local).exists() {
        let abs = std::fs::canonicalize(local_dir)
            .unwrap_or_else(|_| PathBuf::from(local_dir));
        emit_static_link(abs.to_string_lossy().as_ref(), &t_os, &t_env);
        return;
    }

    // 3. System-wide path walk.
    if let Some(dir) = find_static_in_system(&t_os, &t_arch, &t_env) {
        emit_static_link(dir.to_string_lossy().as_ref(), &t_os, &t_env);
        return;
    }

    panic!(
        "\n\nbuild.rs: {lib} not found even after running the espeak build script.\n\
         Check the script output above for errors.\n\n"
    );
}

// ── Build script dispatch ─────────────────────────────────────────────────────
//
// Dispatch rules (target, not host):
//
//  target_os="windows", target_env="gnu"  → MinGW bash script
//      Works for cross-compilation from Linux/macOS and for native MinGW
//      (MSYS2 provides bash).
//
//  target_os="windows", target_env="msvc" → PowerShell script
//      Must run on a native Windows host; cross-compiling to MSVC is not
//      supported by the Rust toolchain or espeak-ng's build system.
//
//  macOS / Linux                          → Unix bash script

fn build_espeak_static(t_os: &str, t_env: &str) {
    match (t_os, t_env) {
        ("windows", "gnu")  => build_espeak_static_mingw(),
        ("windows", _)      => build_espeak_static_windows(),
        _                   => build_espeak_static_unix(),
    }
}

fn build_espeak_static_unix() {
    let script = std::fs::canonicalize("../scripts/build-espeak-static.sh")
        .unwrap_or_else(|_| PathBuf::from("../scripts/build-espeak-static.sh"));

    if !script.exists() {
        panic!(
            "\n\nbuild.rs: scripts/build-espeak-static.sh not found at {}.\n\n",
            script.display()
        );
    }
    eprintln!("build.rs: running {} …", script.display());

    let cur_path = std::env::var("PATH").unwrap_or_default();
    let full_path = format!(
        "/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:{cur_path}"
    );
    let shell_cmd = format!("exec 1>&2; bash '{}'", script.display());

    let status = Command::new("bash")
        .args(["-c", &shell_cmd])
        .env("PATH", &full_path)
        .status()
        .unwrap_or_else(|e| panic!("build.rs: failed to launch build-espeak-static.sh: {e}"));

    if !status.success() {
        panic!("\n\nbuild.rs: build-espeak-static.sh failed ({status}).\n\n");
    }
    println!("cargo:warning=build.rs: espeak-ng static library built successfully.");
}

fn build_espeak_static_mingw() {
    let script = std::fs::canonicalize("../scripts/build-espeak-static-mingw.sh")
        .unwrap_or_else(|_| PathBuf::from("../scripts/build-espeak-static-mingw.sh"));

    if !script.exists() {
        panic!(
            "\n\nbuild.rs: scripts/build-espeak-static-mingw.sh not found at {}.\n\
             Run it manually from the repository root:\n\
               bash scripts/build-espeak-static-mingw.sh\n\n",
            script.display()
        );
    }
    eprintln!("build.rs: running {} …", script.display());

    let cur_path = std::env::var("PATH").unwrap_or_default();
    let full_path = format!(
        "/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:{cur_path}"
    );
    let shell_cmd = format!("exec 1>&2; bash '{}'", script.display());

    // On Windows (MSYS2) bash should already be in PATH; the PATH augmentation
    // above is harmless (non-existent directories are skipped).
    let shell = if cfg!(target_os = "windows") { "bash" } else { "bash" };

    let status = Command::new(shell)
        .args(["-c", &shell_cmd])
        .env("PATH", &full_path)
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "build.rs: failed to launch build-espeak-static-mingw.sh: {e}\n\
                 On Linux:  sudo apt install gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64\n\
                 On macOS:  brew install mingw-w64\n\
                 On MSYS2:  pacman -S mingw-w64-x86_64-gcc cmake"
            )
        });

    if !status.success() {
        panic!(
            "\n\nbuild.rs: build-espeak-static-mingw.sh failed ({status}).\n\
             See the output above for the exact error.\n\n"
        );
    }
    println!("cargo:warning=build.rs: espeak-ng MinGW static library built successfully.");
}

fn build_espeak_static_windows() {
    let script_rel = Path::new("..\\scripts\\build-espeak-static.ps1");
    let script = std::fs::canonicalize(script_rel).unwrap_or_else(|_| script_rel.to_path_buf());

    if !script.exists() {
        panic!(
            "\n\nbuild.rs: scripts\\build-espeak-static.ps1 not found at {}.\n\
             Run it manually from the repository root:\n\
               .\\scripts\\build-espeak-static.ps1\n\n",
            script.display()
        );
    }
    eprintln!("build.rs: running {} …", script.display());

    let status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script.to_string_lossy(),
        ])
        .status()
        .unwrap_or_else(|e| panic!("build.rs: failed to launch build-espeak-static.ps1: {e}"));

    if !status.success() {
        panic!(
            "\n\nbuild.rs: build-espeak-static.ps1 failed ({status}).\n\
             Run from a Developer PowerShell for VS:\n\
               .\\scripts\\build-espeak-static.ps1\n\n"
        );
    }
    println!("cargo:warning=build.rs: espeak-ng static library built successfully.");
}

// ── Link directives ───────────────────────────────────────────────────────────

fn emit_static_link(dir: &str, t_os: &str, t_env: &str) {
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=static=espeak-ng");
    // espeak-ng is a C++ project; link the appropriate C++ runtime.
    match (t_os, t_env) {
        ("windows", "msvc") => { /* MSVC links the C++ runtime automatically */ }
        ("windows", _)      => println!("cargo:rustc-link-lib=dylib=stdc++"),  // MinGW
        ("macos", _)        => println!("cargo:rustc-link-lib=dylib=c++"),
        _                   => println!("cargo:rustc-link-lib=dylib=stdc++"),
    }
}

fn require_static_archive(dir: &str, t_os: &str, t_env: &str) {
    let lib = static_lib_name();
    if Path::new(dir).join(lib).exists() {
        emit_static_link(dir, t_os, t_env);
    } else {
        panic!(
            "\n\nbuild.rs: ESPEAK_LIB_DIR is set to {dir:?} but {lib} \
             was not found there even after running the build script.\n\
             \n\
             Manual fix (macOS/Linux):  bash scripts/build-espeak-static.sh\n\
             Manual fix (MinGW):        bash scripts/build-espeak-static-mingw.sh\n\
             Manual fix (Windows MSVC): .\\scripts\\build-espeak-static.ps1\n\n"
        );
    }
}

// ── System path walk ──────────────────────────────────────────────────────────

fn find_static_in_system(t_os: &str, t_arch: &str, t_env: &str) -> Option<PathBuf> {
    let lib = static_lib_name();
    let mut dirs: Vec<PathBuf> = Vec::new();

    match t_os {
        "macos" => {
            if let Some(keg) = brew_prefix("espeak-ng") {
                dirs.push(PathBuf::from(format!("{keg}/lib")));
            }
            for prefix in ["/opt/homebrew", "/usr/local"] {
                dirs.push(PathBuf::from(format!("{prefix}/opt/espeak-ng/lib")));
                dirs.push(PathBuf::from(format!("{prefix}/lib")));
            }
        }
        "linux" => {
            let multiarch = match t_arch {
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
        "windows" if t_env == "gnu" => {
            // MinGW / MSYS2 system paths.
            // Native MSYS2 installs to /mingw64; cross-compiler sysroot on Linux.
            dirs.extend([
                "/mingw64/lib",
                "/mingw32/lib",
                "/usr/x86_64-w64-mingw32/lib",
                "/usr/i686-w64-mingw32/lib",
            ].map(PathBuf::from));
            if let Ok(prefix) = std::env::var("MINGW_PREFIX") {
                dirs.push(PathBuf::from(format!("{prefix}/lib")));
            }
            // vcpkg MinGW triplet
            if let Ok(vcpkg_root) = std::env::var("VCPKG_ROOT") {
                dirs.push(PathBuf::from(format!(
                    "{vcpkg_root}/installed/x64-mingw-static/lib"
                )));
            }
        }
        "windows" => {
            // MSVC — vcpkg and Chocolatey
            if let Ok(r) = std::env::var("VCPKG_ROOT") {
                dirs.push(PathBuf::from(format!("{r}\\installed\\x64-windows-static\\lib")));
                dirs.push(PathBuf::from(format!("{r}\\installed\\x64-windows-static-md\\lib")));
            }
            if let Ok(r) = std::env::var("VCPKG_INSTALLATION_ROOT") {
                dirs.push(PathBuf::from(format!("{r}\\installed\\x64-windows-static\\lib")));
            }
            if let Ok(c) = std::env::var("ChocolateyInstall") {
                dirs.push(PathBuf::from(format!("{c}\\lib\\espeak-ng\\tools\\lib")));
            }
        }
        _ => {}
    }

    dirs.into_iter()
        .filter(|p| p.is_dir())
        .find(|p| p.join(lib).exists())
}

fn brew_prefix(formula: &str) -> Option<String> {
    let out = Command::new("brew").args(["--prefix", formula]).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8(out.stdout).ok()?.trim().to_owned())
    } else {
        None
    }
}

// ── Dev data path ─────────────────────────────────────────────────────────────
//
// Bakes the absolute path to the espeak-ng-data directory as compile-time env
// `ESPEAK_DATA_PATH_DEV`.  Resolved relative to ESPEAK_LIB_DIR so it
// automatically picks up espeak-static-mingw/share/... for MinGW builds.

fn emit_espeak_data_path_dev() {
    if let Ok(lib_dir) = std::env::var("ESPEAK_LIB_DIR") {
        let data_dir = Path::new(&lib_dir)
            .parent()
            .unwrap_or(Path::new("."))
            .join("share")
            .join("espeak-ng-data");

        println!("cargo:rerun-if-changed=espeak-static/share/espeak-ng-data");
        println!("cargo:rerun-if-changed=espeak-static-mingw/share/espeak-ng-data");

        if data_dir.is_dir() {
            let abs = std::fs::canonicalize(&data_dir).unwrap_or(data_dir);
            println!("cargo:rustc-env=ESPEAK_DATA_PATH_DEV={}", abs.display());
            println!("cargo:warning=espeak-ng dev data path baked in: {}", abs.display());
        } else {
            println!(
                "cargo:warning=espeak-ng data dir not found yet ({}); \
                 will be baked in after the first full build.",
                data_dir.display()
            );
        }
    }
}

// ── Bundle helpers ────────────────────────────────────────────────────────────
//
// Copies espeak-ng-data/ into src-tauri/resources/espeak-ng-data/ so Tauri
// includes it in the bundle.
//
// Data lookup order (all variants):
//   1. Path relative to ESPEAK_LIB_DIR — handles cross-compilation where
//      ESPEAK_LIB_DIR points at espeak-static-mingw/lib/ (MinGW target) while
//      the build host is Linux/macOS (so the Linux/macOS #[cfg] function runs).
//   2. Well-known per-platform directories.
//
// The espeak-ng data files are target-independent (text phoneme tables) so
// the same directory can be bundled regardless of the compilation target.

fn espeak_data_from_lib_dir() -> Option<String> {
    let lib_dir = std::env::var("ESPEAK_LIB_DIR").ok()?;
    let data = Path::new(&lib_dir)
        .parent()?
        .join("share")
        .join("espeak-ng-data");
    if data.is_dir() {
        Some(data.to_string_lossy().into_owned())
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn bundle_espeak_data_macos() {
    let brew_prefix_val = brew_prefix("espeak-ng");
    let data_src: String = espeak_data_from_lib_dir()
        .into_iter()
        .chain([
            "espeak-static/share/espeak-ng-data".to_string(),
            "espeak-static-mingw/share/espeak-ng-data".to_string(),
        ])
        .chain(brew_prefix_val.as_deref().map(|p| format!("{p}/share/espeak-ng-data")))
        .chain(brew_prefix_val.as_deref().map(|p| format!("{p}/lib/espeak-ng-data")))
        .chain([
            "/opt/homebrew/share/espeak-ng-data".to_string(),
            "/usr/local/share/espeak-ng-data".to_string(),
        ])
        .find(|p| Path::new(p.as_str()).is_dir())
        .unwrap_or_else(|| {
            panic!(
                "build.rs: espeak-ng-data/ not found.\n\
                 Run:  bash scripts/build-espeak-static.sh\n\
                 Or:   brew install espeak-ng"
            )
        });

    let data_dst = "resources/espeak-ng-data";
    if Path::new(data_dst).exists() {
        std::fs::remove_dir_all(data_dst)
            .expect("build.rs: failed to remove old resources/espeak-ng-data/");
    }
    copy_dir_all(&data_src, data_dst);
    println!("cargo:rerun-if-changed={data_src}");
}

#[cfg(target_os = "linux")]
fn bundle_espeak_data_linux() {
    let multiarch = std::env::var("CARGO_CFG_TARGET_ARCH")
        .map(|a| match a.as_str() {
            "x86_64"  => "x86_64-linux-gnu",
            "aarch64" => "aarch64-linux-gnu",
            "arm"     => "arm-linux-gnueabihf",
            _         => "",
        })
        .unwrap_or_default();

    let data_src: String = espeak_data_from_lib_dir()
        .into_iter()
        .chain([
            "espeak-static/share/espeak-ng-data".to_string(),
            "espeak-static-mingw/share/espeak-ng-data".to_string(),
            format!("/usr/lib/{multiarch}/espeak-ng-data"),
            "/usr/lib/espeak-ng-data".to_string(),
            "/usr/share/espeak-ng-data".to_string(),
            "/usr/local/share/espeak-ng-data".to_string(),
        ])
        .filter(|p| !p.contains("//") && Path::new(p.as_str()).is_dir())
        .next()
        .unwrap_or_else(|| {
            panic!(
                "build.rs: espeak-ng-data/ not found on Linux.\n\
                 Run:  bash scripts/build-espeak-static.sh\n\
                 For MinGW cross-compilation:  bash scripts/build-espeak-static-mingw.sh\n\
                 Or install:  sudo apt-get install espeak-ng-data"
            )
        });

    let data_dst = "resources/espeak-ng-data";
    if Path::new(data_dst).exists() {
        std::fs::remove_dir_all(data_dst)
            .expect("build.rs: failed to remove old resources/espeak-ng-data/");
    }
    copy_dir_all(&data_src, data_dst);
    println!("cargo:rerun-if-changed={data_src}");
}

// ── Windows bundle helper ─────────────────────────────────────────────────────
//
// On a native Windows host ESPEAK_LIB_DIR points at either:
//   espeak-static/lib        (MSVC target)
//   espeak-static-mingw/lib  (MinGW/MSYS2 target)
// so espeak_data_from_lib_dir() finds the right data directory automatically.

#[cfg(target_os = "windows")]
fn bundle_espeak_data_windows() {
    let vcpkg_share: String = std::env::var("VCPKG_ROOT")
        .map(|r| format!("{r}\\installed\\x64-windows-static\\share\\espeak-ng-data"))
        .unwrap_or_default();
    let vcpkg_mingw_share: String = std::env::var("VCPKG_ROOT")
        .map(|r| format!("{r}/installed/x64-mingw-static/share/espeak-ng-data"))
        .unwrap_or_default();
    let choco_share: String = std::env::var("ChocolateyInstall")
        .map(|c| format!("{c}\\lib\\espeak-ng\\tools\\espeak-ng-data"))
        .unwrap_or_default();
    let msys_share = r"/mingw64/share/espeak-ng-data".to_string();

    let data_src: String = espeak_data_from_lib_dir()
        .into_iter()
        .chain([
            "espeak-static/share/espeak-ng-data".to_string(),
            "espeak-static-mingw/share/espeak-ng-data".to_string(),
            vcpkg_share,
            vcpkg_mingw_share,
            choco_share,
            msys_share,
        ])
        .filter(|p| !p.is_empty() && Path::new(p.as_str()).is_dir())
        .next()
        .unwrap_or_else(|| {
            panic!(
                "build.rs: espeak-ng-data/ not found.\n\
                 Run (MSVC):   .\\scripts\\build-espeak-static.ps1\n\
                 Run (MinGW):  bash scripts/build-espeak-static-mingw.sh"
            )
        });

    let data_dst = "resources/espeak-ng-data";
    if Path::new(data_dst).exists() {
        std::fs::remove_dir_all(data_dst)
            .expect("build.rs: failed to remove old resources/espeak-ng-data/");
    }
    copy_dir_all(&data_src, data_dst);
    println!("cargo:rerun-if-changed={data_src}");
    println!("cargo:warning=build.rs: espeak-ng-data bundled from {data_src}");
}

// ── Cross-platform directory copy ─────────────────────────────────────────────

fn copy_dir_all(src: &str, dst: &str) {
    let src_path = Path::new(src);
    let dst_path = Path::new(dst);
    std::fs::create_dir_all(dst_path)
        .unwrap_or_else(|e| panic!("build.rs: create_dir_all({dst}) failed: {e}"));

    for entry in std::fs::read_dir(src_path)
        .unwrap_or_else(|e| panic!("build.rs: read_dir({src}) failed: {e}"))
    {
        let entry     = entry.unwrap_or_else(|e| panic!("build.rs: dir entry error: {e}"));
        let target    = dst_path.join(entry.file_name());
        let file_type = entry.file_type().unwrap();

        if file_type.is_dir() {
            copy_dir_all(
                &entry.path().to_string_lossy(),
                &target.to_string_lossy(),
            );
        } else {
            // Resolve symlinks so bundles contain plain files (required for
            // macOS notarisation).  Fall back to the original path on Windows
            // where symlinks may require elevated privileges.
            let src_file = if file_type.is_symlink() {
                std::fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path())
            } else {
                entry.path()
            };
            if src_file.is_file() {
                remove_if_exists(&target);
                std::fs::copy(&src_file, &target)
                    .unwrap_or_else(|e| panic!("build.rs: copy {src_file:?}: {e}"));
            }
        }
    }
}

fn remove_if_exists(path: &Path) {
    if path.exists() {
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_readonly(false);
            std::fs::set_permissions(path, perms).ok();
        }
        std::fs::remove_file(path).ok();
    }
}
